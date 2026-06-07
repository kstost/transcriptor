use std::{
    env,
    ffi::OsStr,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
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
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

const TARGET_SAMPLE_RATE: u32 = 16_000;
const DEFAULT_MODEL_NAME: &str = "base";
const DEFAULT_LANGUAGE: &str = "ko";
const MODEL_ENV: &str = "TRANSCRIPTOR_MODEL";
const HOME_ENV: &str = "TRANSCRIPTOR_HOME";
const LANGUAGE_ENV: &str = "TRANSCRIPTOR_LANGUAGE";
const MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

#[derive(Debug, Parser)]
#[command(version, about = "Transcribe an audio file to stdout with Whisper")]
struct Args {
    /// Audio file to transcribe.
    audio: PathBuf,

    /// Path to a whisper.cpp ggml model file. Overrides TRANSCRIPTOR_MODEL.
    #[arg(short, long)]
    model: Option<PathBuf>,

    /// Model name to auto-download when --model and TRANSCRIPTOR_MODEL are not set.
    #[arg(long, default_value = DEFAULT_MODEL_NAME)]
    model_name: String,

    /// Language code such as ko, en, ja, or auto. Defaults to ko.
    #[arg(short, long)]
    language: Option<String>,

    /// CPU worker threads to use. Defaults to available parallelism.
    #[arg(short, long)]
    threads: Option<usize>,

    /// Do not auto-download the model if it is missing from ~/.transcriptor.
    #[arg(long)]
    no_download: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let transcript = transcribe(&args)?;
    println!("{}", transcript.trim());
    Ok(())
}

fn transcribe(args: &Args) -> Result<String> {
    let model_path = resolve_model_path(args)?;

    eprintln!("Loading model: {}", model_path.display());
    let ctx = WhisperContext::new_with_params(
        model_path
            .to_str()
            .ok_or_else(|| anyhow!("model path is not valid UTF-8"))?,
        WhisperContextParameters::default(),
    )
    .context("failed to load Whisper model")?;

    eprintln!("Decoding audio: {}", args.audio.display());
    let audio = decode_audio(&args.audio).context("failed to decode audio")?;
    if audio.is_empty() {
        bail!("decoded audio is empty");
    }

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

    eprintln!("Transcribing...");
    let mut state = ctx
        .create_state()
        .context("failed to create Whisper state")?;
    state
        .full(params, &audio)
        .context("failed to run Whisper transcription")?;

    let mut transcript = String::new();
    for segment in state.as_iter() {
        let text = segment.to_string();
        if !transcript.is_empty()
            && !text.starts_with(char::is_whitespace)
            && !transcript.ends_with(char::is_whitespace)
        {
            transcript.push(' ');
        }
        transcript.push_str(&text);
    }

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

fn resolve_language(args: &Args) -> Option<String> {
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

fn resolve_model_path(args: &Args) -> Result<PathBuf> {
    if let Some(path) = &args.model {
        return ensure_existing_model(path);
    }

    if let Some(path) = env::var_os(MODEL_ENV).map(PathBuf::from) {
        return ensure_existing_model(&path);
    }

    let file_name = model_file_name(&args.model_name)?;
    let path = transcriptor_home()?.join("models").join(&file_name);
    if path.exists() {
        return Ok(path);
    }

    if args.no_download {
        bail!(
            "model is missing at {}; run without --no-download or pass --model",
            path.display()
        );
    }

    download_model(&args.model_name, &path)?;
    Ok(path)
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

fn download_model(model_name: &str, destination: &Path) -> Result<()> {
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

    eprintln!("Downloading model: {url}");
    let response = ureq::get(&url)
        .call()
        .map_err(|err| anyhow!("failed to download model from {url}: {err}"))?;

    if !(200..300).contains(&response.status()) {
        bail!(
            "model download failed with HTTP status {}",
            response.status()
        );
    }

    let mut reader = response.into_reader();
    let mut file = File::create(&tmp)
        .with_context(|| format!("failed to create temporary model file: {}", tmp.display()))?;
    let bytes = io::copy(&mut reader, &mut file).context("failed to write downloaded model")?;
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
    eprintln!("Model saved: {}", destination.display());

    Ok(())
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
    fn rejects_path_like_model_names() {
        assert!(model_file_name("../base").is_err());
        assert!(model_file_name("nested/base").is_err());
    }

    #[test]
    fn resamples_to_expected_length() {
        let input = vec![0.0; 48_000];
        let output = resample_linear(&input, 48_000, 16_000);
        assert_eq!(output.len(), 16_000);
    }
}
