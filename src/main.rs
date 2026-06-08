use std::{
    env,
    ffi::OsStr,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args as ClapArgs, Parser, Subcommand, ValueEnum};
use opus_decoder::OpusDecoder;
use symphonia::{
    core::{
        codecs::audio::{AudioCodecParameters, AudioDecoderOptions, well_known::CODEC_ID_OPUS},
        errors::Error as SymphoniaError,
        formats::{FormatOptions, FormatReader, TrackType, probe::Hint},
        io::MediaSourceStream,
        meta::MetadataOptions,
        packet::Packet,
    },
    default::{get_codecs, get_probe},
};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, install_logging_hooks,
};

const TARGET_SAMPLE_RATE: u32 = 16_000;
const DEFAULT_MODEL_NAME: &str = "base";
const DEFAULT_LANGUAGE: &str = "ko";
const MODEL_ENV: &str = "TRANSCRIPTOR_MODEL";
const HOME_ENV: &str = "TRANSCRIPTOR_HOME";
const LANGUAGE_ENV: &str = "TRANSCRIPTOR_LANGUAGE";
const CONFIG_FILE_NAME: &str = "config.json";
const MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";
const DOWNLOAD_PROGRESS_BYTES: u64 = 1024 * 1024;
const DOWNLOAD_PROGRESS_INTERVAL: Duration = Duration::from_millis(250);
const PROGRESS_SCHEMA_VERSION: u32 = 1;

static EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
    Json,
    Text,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum ProgressFormat {
    None,
    Json,
}

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Transcribe an audio file to stdout with Whisper",
    args_conflicts_with_subcommands = true,
    subcommand_precedence_over_arg = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[command(flatten)]
    transcribe: TranscribeArgs,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Read or update persistent transcriptor settings.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Show the configured default model.
    Get,

    /// Set the default model name used when no model path or model name is passed.
    SetModel {
        /// Model name such as base, small, medium, large-v3, or large-v3-turbo.
        model_name: String,
    },

    /// Reset the configured default model back to base.
    UnsetModel,
}

#[derive(Debug, ClapArgs)]
struct TranscribeArgs {
    /// Audio file to transcribe.
    audio: Option<PathBuf>,

    /// Path to a whisper.cpp ggml model file. Overrides TRANSCRIPTOR_MODEL.
    #[arg(short, long)]
    model: Option<PathBuf>,

    /// Model name to auto-download when --model and TRANSCRIPTOR_MODEL are not set.
    #[arg(long)]
    model_name: Option<String>,

    /// Language code such as ko, en, ja, or auto. Defaults to ko.
    #[arg(short, long)]
    language: Option<String>,

    /// CPU worker threads to use. Defaults to available parallelism.
    #[arg(short, long)]
    threads: Option<usize>,

    /// Output format.
    #[arg(long, value_enum, default_value = "json")]
    format: OutputFormat,

    /// Emit machine-readable progress events to stderr.
    #[arg(long, value_enum, default_value = "none")]
    progress: ProgressFormat,

    /// Print progress and whisper.cpp logs to stderr.
    #[arg(short, long)]
    verbose: bool,

    /// Do not auto-download the model if it is missing from ~/.transcriptor.
    #[arg(long)]
    no_download: bool,
}

fn main() {
    let args: Vec<_> = env::args_os().collect();
    let parse_errors_as_json = args_request_json_progress(&args);
    let cli = match Cli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(err) => {
            if parse_errors_as_json
                && !matches!(
                    err.kind(),
                    clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
                )
            {
                let message = err.to_string();
                let causes = vec![message.clone()];
                let _ = emit_error_event(
                    ProgressFormat::Json,
                    "cli_parse",
                    err.exit_code(),
                    &message,
                    &causes,
                );
                let _ = emit_process_finished(ProgressFormat::Json, false, err.exit_code());
                std::process::exit(err.exit_code());
            }

            err.exit();
        }
    };
    let progress = cli.transcribe.progress;

    let _ = emit_process_started(progress);
    if let Err(err) = run(cli) {
        if progress == ProgressFormat::Json {
            let causes = error_causes(&err);
            let message = causes
                .first()
                .map(String::as_str)
                .unwrap_or("runtime error");
            let _ = emit_error_event(progress, "runtime", 1, message, &causes);
            let _ = emit_process_finished(progress, false, 1);
        } else {
            eprintln!("Error: {err:?}");
        }
        std::process::exit(1);
    }

    let _ = emit_process_finished(progress, true, 0);
}

fn run(cli: Cli) -> Result<()> {
    if let Some(command) = cli.command {
        return run_command(command);
    }

    let args = cli.transcribe;
    if args.audio.is_none() {
        bail!("missing audio file; run `transcriptor --help` for usage");
    }

    if !args.verbose {
        install_logging_hooks();
    }

    let transcript = transcribe(&args)?;
    print_transcript(&args, transcript.trim())?;
    Ok(())
}

fn run_command(command: Command) -> Result<()> {
    match command {
        Command::Config { command } => run_config_command(command),
    }
}

fn run_config_command(command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Get => {
            let config = load_config()?;
            let model_name = config
                .model_name
                .clone()
                .unwrap_or_else(|| DEFAULT_MODEL_NAME.to_string());
            let source = if config.model_name.is_some() {
                "config"
            } else {
                "default"
            };
            println!("model_name={model_name}");
            println!("source={source}");
            println!("config={}", config_path()?.display());
        }
        ConfigCommand::SetModel { model_name } => {
            let model_name = normalize_model_name(&model_name)?;
            let mut config = load_config()?;
            config.model_name = Some(model_name.clone());
            save_config(&config)?;
            println!("Default model set to {model_name}");
        }
        ConfigCommand::UnsetModel => {
            let mut config = load_config()?;
            config.model_name = None;
            save_config(&config)?;
            println!("Default model reset to {DEFAULT_MODEL_NAME}");
        }
    }

    Ok(())
}

fn print_transcript(args: &TranscribeArgs, text: &str) -> Result<()> {
    match args.format {
        OutputFormat::Json => {
            let output = serde_json::json!({ "text": text });
            println!("{}", serde_json::to_string(&output)?);
        }
        OutputFormat::Text => {
            println!("{text}");
        }
    }

    io::stdout().flush().context("failed to flush stdout")
}

fn transcribe(args: &TranscribeArgs) -> Result<String> {
    let audio_path = args
        .audio
        .as_deref()
        .ok_or_else(|| anyhow!("missing audio file; run `transcriptor --help` for usage"))?;
    emit_transcription_started(args.progress, audio_path)?;
    validate_model_selection(args)?;

    if args.verbose {
        eprintln!("Decoding audio: {}", audio_path.display());
    }
    emit_path_event(
        args.progress,
        "audio_decode_started",
        "audio_path",
        audio_path,
    )?;
    let audio = match decode_audio(audio_path).context("failed to decode audio") {
        Ok(audio) => audio,
        Err(err) => {
            emit_audio_decode_failed(args.progress, audio_path, &err)?;
            return Err(err);
        }
    };
    if audio.is_empty() {
        let err = anyhow!("decoded audio is empty");
        emit_audio_decode_failed(args.progress, audio_path, &err)?;
        return Err(err);
    }
    emit_audio_decode_finished(args.progress, audio_path, audio.len())?;

    let model_path = resolve_model_path(args)?;

    if args.verbose {
        eprintln!("Loading model: {}", model_path.display());
    }
    emit_path_event(
        args.progress,
        "model_load_started",
        "model_path",
        &model_path,
    )?;
    let ctx = WhisperContext::new_with_params(
        model_path
            .to_str()
            .ok_or_else(|| anyhow!("model path is not valid UTF-8"))?,
        WhisperContextParameters::default(),
    )
    .context("failed to load Whisper model")?;
    emit_path_event(
        args.progress,
        "model_load_finished",
        "model_path",
        &model_path,
    )?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 0 });
    params.set_n_threads(thread_count(args.threads));
    params.set_translate(false);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    let language = resolve_language(args);
    if let Some(language) = language.as_deref().filter(|language| *language != "auto") {
        params.set_language(Some(language));
    }

    if args.verbose {
        eprintln!("Transcribing...");
    }
    emit_path_event(
        args.progress,
        "whisper_inference_started",
        "model_path",
        &model_path,
    )?;
    let mut state = ctx
        .create_state()
        .context("failed to create Whisper state")?;
    state
        .full(params, &audio)
        .context("failed to run Whisper transcription")?;
    emit_path_event(
        args.progress,
        "whisper_inference_finished",
        "model_path",
        &model_path,
    )?;

    let mut transcript = String::new();
    let mut segment_count = 0usize;
    for segment in state.as_iter() {
        segment_count += 1;
        let text = segment.to_string();
        if !transcript.is_empty()
            && !text.starts_with(char::is_whitespace)
            && !transcript.ends_with(char::is_whitespace)
        {
            transcript.push(' ');
        }
        transcript.push_str(&text);
    }
    emit_transcription_finished(args.progress, transcript.as_str(), segment_count)?;

    Ok(transcript)
}

fn thread_count(requested: Option<usize>) -> i32 {
    let count = requested.filter(|threads| *threads > 0).unwrap_or_else(|| {
        env::var("TRANSCRIPTOR_THREADS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    });

    let count = if count == 0 {
        std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1)
    } else {
        count
    };

    count.min(i32::MAX as usize) as i32
}

fn resolve_language(args: &TranscribeArgs) -> Option<String> {
    let language = args
        .language
        .clone()
        .or_else(|| env::var(LANGUAGE_ENV).ok())
        .unwrap_or_else(|| DEFAULT_LANGUAGE.to_string());

    let language = language.trim().to_ascii_lowercase();
    if language.is_empty() {
        None
    } else {
        Some(language)
    }
}

fn resolve_model_path(args: &TranscribeArgs) -> Result<PathBuf> {
    if let Some(path) = &args.model {
        let path = match ensure_existing_model(path) {
            Ok(path) => path,
            Err(err) => {
                emit_model_event(args.progress, "model_unavailable", "cli", None, path, false)?;
                return Err(err);
            }
        };
        emit_model_event(args.progress, "model_ready", "cli", None, &path, false)?;
        return Ok(path);
    }

    if let Some(path) = env::var_os(MODEL_ENV).map(PathBuf::from) {
        let path = match ensure_existing_model(&path) {
            Ok(path) => path,
            Err(err) => {
                emit_model_event(
                    args.progress,
                    "model_unavailable",
                    "env",
                    None,
                    &path,
                    false,
                )?;
                return Err(err);
            }
        };
        emit_model_event(args.progress, "model_ready", "env", None, &path, false)?;
        return Ok(path);
    }

    let model_name = resolve_model_name(args)?;
    let file_name = model_file_name(&model_name)?;
    let path = transcriptor_home()?.join("models").join(&file_name);
    if path.exists() {
        let path = match ensure_existing_model(&path) {
            Ok(path) => path,
            Err(err) => {
                emit_model_event(
                    args.progress,
                    "model_unavailable",
                    "cache",
                    Some(&model_name),
                    &path,
                    false,
                )?;
                return Err(err);
            }
        };
        emit_model_event(
            args.progress,
            "model_ready",
            "cache",
            Some(&model_name),
            &path,
            false,
        )?;
        return Ok(path);
    }

    emit_model_event(
        args.progress,
        "model_download_required",
        "auto",
        Some(&model_name),
        &path,
        true,
    )?;

    if args.no_download {
        bail!(
            "model is missing at {}; run without --no-download or pass --model",
            path.display()
        );
    }

    download_model(&model_name, &path, args.verbose, args.progress)?;
    emit_model_event(
        args.progress,
        "model_ready",
        "download",
        Some(&model_name),
        &path,
        true,
    )?;
    Ok(path)
}

fn validate_model_selection(args: &TranscribeArgs) -> Result<()> {
    if let Some(path) = &args.model {
        if let Err(err) = ensure_existing_model(path) {
            emit_model_event(args.progress, "model_unavailable", "cli", None, path, false)?;
            return Err(err);
        }
        return Ok(());
    }

    if let Some(path) = env::var_os(MODEL_ENV).map(PathBuf::from) {
        if let Err(err) = ensure_existing_model(&path) {
            emit_model_event(
                args.progress,
                "model_unavailable",
                "env",
                None,
                &path,
                false,
            )?;
            return Err(err);
        }
        return Ok(());
    }

    let model_name = resolve_model_name(args)?;
    let file_name = model_file_name(&model_name)?;
    let path = transcriptor_home()?.join("models").join(&file_name);
    if path.exists()
        && let Err(err) = ensure_existing_model(&path)
    {
        emit_model_event(
            args.progress,
            "model_unavailable",
            "cache",
            Some(&model_name),
            &path,
            false,
        )?;
        return Err(err);
    }

    Ok(())
}

fn resolve_model_name(args: &TranscribeArgs) -> Result<String> {
    if let Some(model_name) = &args.model_name {
        return normalize_model_name(model_name);
    }

    Ok(load_config()?
        .model_name
        .unwrap_or_else(|| DEFAULT_MODEL_NAME.to_string()))
}

fn ensure_existing_model(path: &Path) -> Result<PathBuf> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("model file does not exist: {}", path.display()))?;
    if !metadata.is_file() {
        bail!("model path is not a file: {}", path.display());
    }
    Ok(path.to_path_buf())
}

fn transcriptor_home() -> Result<PathBuf> {
    if let Some(path) = env::var_os(HOME_ENV).filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(path));
    }

    let home = default_home_dir().ok_or_else(|| {
        anyhow!(
            "could not determine home directory; set TRANSCRIPTOR_HOME, pass --model, or set TRANSCRIPTOR_MODEL"
        )
    })?;
    Ok(home.join(".transcriptor"))
}

fn config_path() -> Result<PathBuf> {
    Ok(transcriptor_home()?.join(CONFIG_FILE_NAME))
}

#[derive(Debug, Default)]
struct Config {
    model_name: Option<String>,
}

fn load_config() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }

    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config file: {}", path.display()))?;
    parse_config(&text).with_context(|| format!("failed to parse config file: {}", path.display()))
}

fn save_config(config: &Config) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory: {}", parent.display()))?;
    }

    fs::write(&path, config_json(config)?)
        .with_context(|| format!("failed to write config file: {}", path.display()))
}

fn parse_config(text: &str) -> Result<Config> {
    if text.trim().is_empty() {
        return Ok(Config::default());
    }

    let value: serde_json::Value = serde_json::from_str(text)?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("config root must be a JSON object"))?;

    let model_name = match object.get("model_name") {
        Some(serde_json::Value::String(model_name)) => Some(normalize_model_name(model_name)?),
        Some(serde_json::Value::Null) | None => None,
        Some(_) => bail!("config field model_name must be a string"),
    };

    Ok(Config { model_name })
}

fn config_json(config: &Config) -> Result<String> {
    let mut object = serde_json::Map::new();
    if let Some(model_name) = &config.model_name {
        object.insert(
            "model_name".to_string(),
            serde_json::Value::String(model_name.clone()),
        );
    }

    Ok(format!(
        "{}\n",
        serde_json::to_string_pretty(&serde_json::Value::Object(object))?
    ))
}

fn default_home_dir() -> Option<PathBuf> {
    if let Some(home) = env::var_os("HOME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(home));
    }

    if let Some(profile) = env::var_os("USERPROFILE").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(profile));
    }

    let drive = env::var_os("HOMEDRIVE").filter(|v| !v.is_empty())?;
    let path = env::var_os("HOMEPATH").filter(|v| !v.is_empty())?;
    Some(PathBuf::from(drive).join(path))
}

fn model_file_name(model_name: &str) -> Result<String> {
    let model_name = model_name.trim();
    if model_name.is_empty()
        || model_name.contains('/')
        || model_name.contains('\\')
        || model_name.contains("..")
    {
        bail!("invalid model name: {model_name:?}");
    }

    if model_name.starts_with("ggml-") && model_name.ends_with(".bin") {
        Ok(model_name.to_string())
    } else {
        Ok(format!("ggml-{model_name}.bin"))
    }
}

fn normalize_model_name(model_name: &str) -> Result<String> {
    let model_name = model_name.trim();
    model_file_name(model_name)?;

    if let Some(name) = model_name
        .strip_prefix("ggml-")
        .and_then(|name| name.strip_suffix(".bin"))
    {
        Ok(name.to_string())
    } else {
        Ok(model_name.to_string())
    }
}

fn download_model(
    model_name: &str,
    destination: &Path,
    verbose: bool,
    progress: ProgressFormat,
) -> Result<()> {
    let file_name = model_file_name(model_name)?;
    let url = format!("{MODEL_BASE_URL}/{file_name}");
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow!("model destination has no parent directory"))?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create model component directory: {}",
            parent.display()
        )
    })?;

    let tmp = destination.with_extension("download");
    if tmp.exists() {
        fs::remove_file(&tmp).with_context(|| {
            format!(
                "failed to remove previous temporary download: {}",
                tmp.display()
            )
        })?;
    }

    if verbose {
        eprintln!("Downloading model: {url}");
    }
    let response = ureq::get(&url)
        .call()
        .map_err(|err| anyhow!("failed to download model from {url}: {err}"))?;

    if !(200..300).contains(&response.status()) {
        bail!(
            "model download failed with HTTP status {}",
            response.status()
        );
    }

    let total_bytes = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok());
    emit_download_event(
        progress,
        "model_download_started",
        model_name,
        destination,
        Some(&url),
        0,
        total_bytes,
    )?;

    let mut reader = response.into_reader();
    let mut file = File::create(&tmp)
        .with_context(|| format!("failed to create temporary model file: {}", tmp.display()))?;
    let bytes = copy_with_download_progress(
        &mut reader,
        &mut file,
        progress,
        model_name,
        destination,
        total_bytes,
    )
    .context("failed to write downloaded model")?;
    file.flush().context("failed to flush downloaded model")?;

    if bytes == 0 {
        bail!("downloaded model is empty");
    }

    fs::rename(&tmp, destination).with_context(|| {
        format!(
            "failed to move model from {} to {}",
            tmp.display(),
            destination.display()
        )
    })?;
    if verbose {
        eprintln!("Model saved: {}", destination.display());
    }
    emit_download_event(
        progress,
        "model_download_finished",
        model_name,
        destination,
        None,
        bytes,
        total_bytes,
    )?;

    Ok(())
}

fn copy_with_download_progress(
    reader: &mut impl Read,
    writer: &mut impl Write,
    progress: ProgressFormat,
    model_name: &str,
    destination: &Path,
    total_bytes: Option<u64>,
) -> Result<u64> {
    let mut buffer = [0u8; 64 * 1024];
    let mut downloaded = 0u64;
    let mut last_emitted_bytes = 0u64;
    let mut last_emitted_at = Instant::now();

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }

        writer.write_all(&buffer[..read])?;
        downloaded = downloaded.saturating_add(read as u64);

        let now = Instant::now();
        if downloaded.saturating_sub(last_emitted_bytes) >= DOWNLOAD_PROGRESS_BYTES
            || now.duration_since(last_emitted_at) >= DOWNLOAD_PROGRESS_INTERVAL
        {
            emit_download_event(
                progress,
                "model_download_progress",
                model_name,
                destination,
                None,
                downloaded,
                total_bytes,
            )?;
            last_emitted_bytes = downloaded;
            last_emitted_at = now;
        }
    }

    if downloaded != last_emitted_bytes {
        emit_download_event(
            progress,
            "model_download_progress",
            model_name,
            destination,
            None,
            downloaded,
            total_bytes,
        )?;
    }

    Ok(downloaded)
}

fn emit_download_event(
    progress: ProgressFormat,
    event: &str,
    model_name: &str,
    destination: &Path,
    url: Option<&str>,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
) -> Result<()> {
    if progress != ProgressFormat::Json {
        return Ok(());
    }

    let percent = total_bytes
        .filter(|total| *total > 0)
        .map(|total| downloaded_bytes as f64 * 100.0 / total as f64);
    let mut object = progress_event(event);
    object.insert("model_name".to_string(), serde_json::json!(model_name));
    object.insert(
        "path".to_string(),
        serde_json::json!(destination.display().to_string()),
    );
    object.insert(
        "downloaded_bytes".to_string(),
        serde_json::json!(downloaded_bytes),
    );
    object.insert("total_bytes".to_string(), serde_json::json!(total_bytes));
    object.insert("percent".to_string(), serde_json::json!(percent));
    if let Some(url) = url {
        object.insert("url".to_string(), serde_json::json!(url));
    }

    emit_progress_object(progress, object)
}

fn emit_model_event(
    progress: ProgressFormat,
    event: &str,
    source: &str,
    model_name: Option<&str>,
    path: &Path,
    download_required: bool,
) -> Result<()> {
    if progress != ProgressFormat::Json {
        return Ok(());
    }

    let mut object = progress_event(event);
    object.insert("source".to_string(), serde_json::json!(source));
    object.insert("model_name".to_string(), serde_json::json!(model_name));
    object.insert(
        "path".to_string(),
        serde_json::json!(path.display().to_string()),
    );
    object.insert(
        "download_required".to_string(),
        serde_json::json!(download_required),
    );
    emit_progress_object(progress, object)
}

fn emit_error_event(
    progress: ProgressFormat,
    error_type: &str,
    exit_code: i32,
    message: &str,
    causes: &[String],
) -> Result<()> {
    if progress != ProgressFormat::Json {
        return Ok(());
    }

    let mut object = progress_event("error");
    object.insert("error_type".to_string(), serde_json::json!(error_type));
    object.insert("exit_code".to_string(), serde_json::json!(exit_code));
    object.insert("message".to_string(), serde_json::json!(message));
    object.insert("causes".to_string(), serde_json::json!(causes));
    emit_progress_object(progress, object)
}

fn emit_process_started(progress: ProgressFormat) -> Result<()> {
    let object = progress_event("process_started");
    emit_progress_object(progress, object)
}

fn emit_process_finished(progress: ProgressFormat, success: bool, exit_code: i32) -> Result<()> {
    let mut object = progress_event("process_finished");
    object.insert("success".to_string(), serde_json::json!(success));
    object.insert("exit_code".to_string(), serde_json::json!(exit_code));
    emit_progress_object(progress, object)
}

fn emit_transcription_started(progress: ProgressFormat, audio_path: &Path) -> Result<()> {
    let mut object = progress_event("transcription_started");
    object.insert(
        "audio_path".to_string(),
        serde_json::json!(audio_path.display().to_string()),
    );
    emit_progress_object(progress, object)
}

fn emit_transcription_finished(
    progress: ProgressFormat,
    transcript: &str,
    segment_count: usize,
) -> Result<()> {
    let mut object = progress_event("transcription_finished");
    object.insert(
        "segment_count".to_string(),
        serde_json::json!(segment_count),
    );
    object.insert(
        "text_bytes".to_string(),
        serde_json::json!(transcript.len()),
    );
    object.insert(
        "text_chars".to_string(),
        serde_json::json!(transcript.chars().count()),
    );
    emit_progress_object(progress, object)
}

fn emit_audio_decode_finished(
    progress: ProgressFormat,
    audio_path: &Path,
    sample_count: usize,
) -> Result<()> {
    let mut object = progress_event("audio_decode_finished");
    object.insert(
        "audio_path".to_string(),
        serde_json::json!(audio_path.display().to_string()),
    );
    object.insert(
        "sample_rate".to_string(),
        serde_json::json!(TARGET_SAMPLE_RATE),
    );
    object.insert("sample_count".to_string(), serde_json::json!(sample_count));
    object.insert(
        "duration_seconds".to_string(),
        serde_json::json!(sample_count as f64 / TARGET_SAMPLE_RATE as f64),
    );
    emit_progress_object(progress, object)
}

fn emit_audio_decode_failed(
    progress: ProgressFormat,
    audio_path: &Path,
    err: &anyhow::Error,
) -> Result<()> {
    let mut object = progress_event("audio_decode_failed");
    object.insert(
        "audio_path".to_string(),
        serde_json::json!(audio_path.display().to_string()),
    );
    let causes = error_causes(err);
    object.insert(
        "message".to_string(),
        serde_json::json!(
            causes
                .first()
                .map(String::as_str)
                .unwrap_or("failed to decode audio")
        ),
    );
    object.insert("causes".to_string(), serde_json::json!(causes));
    emit_progress_object(progress, object)
}

fn emit_path_event(
    progress: ProgressFormat,
    event: &str,
    path_key: &str,
    path: &Path,
) -> Result<()> {
    let mut object = progress_event(event);
    object.insert(
        path_key.to_string(),
        serde_json::json!(path.display().to_string()),
    );
    emit_progress_object(progress, object)
}

fn progress_event(event: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut object = serde_json::Map::new();
    object.insert(
        "schema_version".to_string(),
        serde_json::json!(PROGRESS_SCHEMA_VERSION),
    );
    object.insert(
        "sequence".to_string(),
        serde_json::json!(EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed)),
    );
    object.insert(
        "timestamp_unix_ms".to_string(),
        serde_json::json!(timestamp_unix_ms()),
    );
    object.insert("event".to_string(), serde_json::json!(event));
    object
}

fn timestamp_unix_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn emit_progress_object(
    progress: ProgressFormat,
    object: serde_json::Map<String, serde_json::Value>,
) -> Result<()> {
    if progress != ProgressFormat::Json {
        return Ok(());
    }

    let mut stderr = io::stderr().lock();
    writeln!(
        stderr,
        "{}",
        serde_json::to_string(&serde_json::Value::Object(object))?
    )?;
    stderr.flush().context("failed to flush progress event")?;
    Ok(())
}

fn args_request_json_progress(args: &[std::ffi::OsString]) -> bool {
    let mut index = 1;
    while index < args.len() {
        if args[index] == "--progress" {
            return args
                .get(index + 1)
                .and_then(|arg| arg.to_str())
                .is_some_and(|value| value == "json");
        }

        if let Some(value) = args[index]
            .to_str()
            .and_then(|arg| arg.strip_prefix("--progress="))
        {
            return value == "json";
        }

        index += 1;
    }

    false
}

fn error_causes(err: &anyhow::Error) -> Vec<String> {
    err.chain().map(|cause| cause.to_string()).collect()
}

fn decode_audio(path: &Path) -> Result<Vec<f32>> {
    decode_with_symphonia(path)
}

fn decode_with_symphonia(path: &Path) -> Result<Vec<f32>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let media_source = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(OsStr::to_str) {
        hint.with_extension(extension);
    }

    let mut format = get_probe()
        .probe(
            &hint,
            media_source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .context("failed to probe audio format")?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| anyhow!("no audio track found"))?;

    let track_id = track.id;
    let track_delay = track.delay.unwrap_or(0) as usize;
    let audio_params = track
        .codec_params
        .as_ref()
        .ok_or_else(|| anyhow!("audio track has no codec parameters"))?
        .audio()
        .ok_or_else(|| anyhow!("selected track is not audio"))?
        .clone();

    if audio_params.codec == CODEC_ID_OPUS {
        return decode_opus_packets(&mut *format, track_id, &audio_params, track_delay);
    }

    decode_symphonia_packets(&mut *format, track_id, &audio_params)
}

fn decode_symphonia_packets(
    format: &mut dyn FormatReader,
    track_id: u32,
    audio_params: &AudioCodecParameters,
) -> Result<Vec<f32>> {
    let sample_rate = audio_params
        .sample_rate
        .ok_or_else(|| anyhow!("audio track has no sample rate"))?;
    let mut decoder = get_codecs()
        .make_audio_decoder(audio_params, &AudioDecoderOptions::default())
        .context("failed to create audio decoder")?;

    let mut mono = Vec::new();

    while let Some(packet) = read_next_packet(format)? {
        if packet.track_id != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(err) => return Err(err).context("failed to decode audio packet"),
        };

        let spec = decoded.spec();
        if spec.rate() != sample_rate {
            bail!("audio sample rate changed mid-stream");
        }

        let channels = spec.channels().count();
        if channels == 0 {
            bail!("decoded audio has no channels");
        }

        let mut samples = vec![0.0f32; decoded.samples_interleaved()];
        decoded.copy_to_slice_interleaved(&mut samples);
        append_downmixed_packet(&mut mono, &samples, channels, &packet, 0);
    }

    Ok(resample_linear(&mono, sample_rate, TARGET_SAMPLE_RATE))
}

fn decode_opus_packets(
    format: &mut dyn FormatReader,
    track_id: u32,
    audio_params: &AudioCodecParameters,
    track_delay: usize,
) -> Result<Vec<f32>> {
    let sample_rate = audio_params
        .sample_rate
        .ok_or_else(|| anyhow!("Opus track has no sample rate"))?;
    let channels = audio_params
        .channels
        .as_ref()
        .ok_or_else(|| anyhow!("Opus track has no channel layout"))?
        .count();

    if !(1..=2).contains(&channels) {
        bail!("only mono/stereo Opus is supported; input has {channels} channels");
    }

    let mut decoder = OpusDecoder::new(sample_rate, channels)
        .with_context(|| format!("failed to create Opus decoder at {sample_rate} Hz"))?;
    let mut packet_samples = vec![0.0f32; decoder.max_frame_size_per_channel() * channels];
    let mut mono = Vec::new();
    let mut remaining_delay = track_delay;

    while let Some(packet) = read_next_packet(format)? {
        if packet.track_id != track_id {
            continue;
        }

        let samples_per_channel = decoder
            .decode_float(&packet.data, &mut packet_samples, false)
            .context("failed to decode Opus packet")?;
        let written = samples_per_channel * channels;

        append_downmixed_packet(
            &mut mono,
            &packet_samples[..written],
            channels,
            &packet,
            remaining_delay,
        );
        remaining_delay = remaining_delay.saturating_sub(samples_per_channel);
    }

    Ok(resample_linear(&mono, sample_rate, TARGET_SAMPLE_RATE))
}

fn read_next_packet(format: &mut dyn FormatReader) -> Result<Option<Packet>> {
    match format.next_packet() {
        Ok(packet) => Ok(packet),
        Err(SymphoniaError::IoError(err)) if err.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
        Err(SymphoniaError::ResetRequired) => {
            bail!("audio stream reset is required but not supported")
        }
        Err(err) => Err(err).context("failed to read audio packet"),
    }
}

fn append_downmixed_packet(
    mono: &mut Vec<f32>,
    interleaved: &[f32],
    channels: usize,
    packet: &Packet,
    extra_start_trim: usize,
) {
    let frame_count = interleaved.len() / channels;
    let trim_start = (packet.trim_start.get() as usize)
        .saturating_add(extra_start_trim)
        .min(frame_count);
    let trim_end = (packet.trim_end.get() as usize).min(frame_count.saturating_sub(trim_start));
    let end_frame = frame_count.saturating_sub(trim_end);

    for frame in interleaved[trim_start * channels..end_frame * channels].chunks_exact(channels) {
        let sum: f32 = frame.iter().copied().sum();
        mono.push(sum / channels as f32);
    }
}

fn resample_linear(input: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
    if input.is_empty() || source_rate == target_rate {
        return input.to_vec();
    }

    let output_len =
        ((input.len() as f64) * (target_rate as f64) / (source_rate as f64)).round() as usize;
    if output_len == 0 {
        return Vec::new();
    }

    let step = source_rate as f64 / target_rate as f64;
    (0..output_len)
        .map(|index| {
            let position = index as f64 * step;
            let base = position.floor() as usize;
            let next = (base + 1).min(input.len() - 1);
            let fraction = (position - base as f64) as f32;
            input[base] * (1.0 - fraction) + input[next] * fraction
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_names_map_to_whisper_cpp_files() {
        assert_eq!(model_file_name("base").unwrap(), "ggml-base.bin");
        assert_eq!(model_file_name("small.en").unwrap(), "ggml-small.en.bin");
        assert_eq!(model_file_name("ggml-base.bin").unwrap(), "ggml-base.bin");
    }

    #[test]
    fn normalizes_model_names_for_config_storage() {
        assert_eq!(normalize_model_name(" small ").unwrap(), "small");
        assert_eq!(
            normalize_model_name("ggml-large-v3-turbo.bin").unwrap(),
            "large-v3-turbo"
        );
    }

    #[test]
    fn rejects_path_like_model_names() {
        assert!(model_file_name("../base").is_err());
        assert!(model_file_name("nested/base").is_err());
    }

    #[test]
    fn parses_config_model_name() {
        let config = parse_config(r#"{ "model_name": "ggml-small.bin" }"#).unwrap();
        assert_eq!(config.model_name.as_deref(), Some("small"));
    }

    #[test]
    fn serializes_config_model_name() {
        let config = Config {
            model_name: Some("large-v3-turbo".to_string()),
        };
        assert_eq!(
            config_json(&config).unwrap(),
            "{\n  \"model_name\": \"large-v3-turbo\"\n}\n"
        );
    }

    #[test]
    fn resamples_to_expected_length() {
        let input = vec![0.0; 48_000];
        let output = resample_linear(&input, 48_000, 16_000);
        assert_eq!(output.len(), 16_000);
    }
}
