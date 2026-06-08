# transcriptor

`transcriptor` is a single-binary command-line tool that transcribes an audio
file with Whisper and prints one result object to stdout.

```bash
transcriptor audio.ogg
```

Default output is one JSON line:

```json
{"text":"transcribed text"}
```

Successful default runs are quiet on stderr. Progress messages and
`whisper.cpp` logs are printed only when `--verbose` is used.

Parent applications that spawn `transcriptor` can use `--progress json` to read
machine-readable progress events from stderr. See
[SPAWN_INTEGRATION.md](SPAWN_INTEGRATION.md) for the full stream and event
contract.

`transcriptor` does not call `ffmpeg` or any other external audio conversion
program. The binary contains the Rust audio decoding path and the Whisper
runtime bindings. Whisper model files are prepared under `~/.transcriptor/`.

---

## Table of Contents

1. [Quick Start](#quick-start)
2. [Install](#install)
3. [Usage](#usage)
4. [Output Contract](#output-contract)
5. [Spawn Integration](#spawn-integration)
6. [Models and Runtime Files](#models-and-runtime-files)
7. [Environment Variables](#environment-variables)
8. [Supported Platforms](#supported-platforms)
9. [Dependencies](#dependencies)
10. [License and Notices](#license-and-notices)
11. [Troubleshooting](#troubleshooting)
12. [Update and Remove](#update-and-remove)
13. [Build From Source](#build-from-source)
14. [How It Works](#how-it-works)
15. [Disclaimer](#disclaimer)

---

## Quick Start

Install on macOS or Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/kstost/transcriptor/refs/heads/main/manage.sh | bash
```

Install on Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/kstost/transcriptor/refs/heads/main/manage.ps1 | iex
```

Check the installed binary:

```bash
transcriptor --version
```

Transcribe an audio file:

```bash
transcriptor audio.ogg
```

Save the JSON result:

```bash
transcriptor audio.ogg > transcript.json
```

Print plain text instead of JSON:

```bash
transcriptor --format text audio.ogg
```

Set the default Whisper model:

```bash
transcriptor config set-model small
```

Show progress and Whisper logs:

```bash
transcriptor --verbose audio.ogg
```

---

## Install

### macOS and Linux

```bash
curl -fsSL https://raw.githubusercontent.com/kstost/transcriptor/refs/heads/main/manage.sh | bash
```

The installer downloads the prebuilt binary for the current OS and CPU
architecture. It installs to `/usr/local/bin` when that path is writable or
`sudo` is available; otherwise it falls back to `$HOME/.local/bin`.

Override the install directory:

```bash
TRANSCRIPTOR_INSTALL_DIR="$HOME/bin" \
curl -fsSL https://raw.githubusercontent.com/kstost/transcriptor/refs/heads/main/manage.sh | bash
```

Override the binary source:

```bash
TRANSCRIPTOR_BASE_URL="https://example.com/dist_beta" bash manage.sh install
```

### Windows

Run this in PowerShell:

```powershell
irm https://raw.githubusercontent.com/kstost/transcriptor/refs/heads/main/manage.ps1 | iex
```

The default install directory is `%LOCALAPPDATA%\transcriptor`. The installer
adds that directory to the user PATH. Open a new PowerShell window if the
`transcriptor` command is not found immediately after installation.

Override the install directory:

```powershell
$env:TRANSCRIPTOR_INSTALL_DIR = "$env:USERPROFILE\bin"
.\manage.ps1 install
```

### Release Files

The installers expect these files under `dist_beta`:

```text
transcriptor-linux-aarch64
transcriptor-linux-x86_64
transcriptor-macos-aarch64
transcriptor-macos-x86_64
transcriptor-windows-aarch64.exe
transcriptor-windows-x86_64.exe
```

---

## Usage

Basic form:

```bash
transcriptor [OPTIONS] <AUDIO>
transcriptor config <COMMAND>
```

Examples:

```bash
transcriptor audio.ogg
transcriptor -l ko audio.ogg
transcriptor -l auto audio.ogg
transcriptor --model ~/.transcriptor/models/ggml-small.bin audio.ogg
transcriptor --model-name small audio.ogg
transcriptor config set-model small
transcriptor config get
transcriptor --progress json audio.ogg
transcriptor --threads 4 audio.ogg
transcriptor --format text audio.ogg
transcriptor --verbose audio.ogg
```

Options:

| Option | Meaning |
|---|---|
| `<AUDIO>` | Audio file to transcribe. |
| `-m, --model <MODEL>` | Path to a whisper.cpp ggml model file. Overrides `TRANSCRIPTOR_MODEL`. |
| `--model-name <MODEL_NAME>` | Model name to auto-download for this run when no model path is set. Overrides the saved model setting. |
| `-l, --language <LANGUAGE>` | Language code such as `ko`, `en`, `ja`, or `auto`. Default: `ko`. |
| `-t, --threads <THREADS>` | CPU worker thread count. Default: available parallelism. |
| `--format <FORMAT>` | Output format: `json` or `text`. Default: `json`. |
| `--progress <PROGRESS>` | Progress event output: `none` or `json`. Default: `none`. |
| `-v, --verbose` | Print progress messages and `whisper.cpp` logs to stderr. |
| `--no-download` | Do not auto-download a missing model. |
| `-h, --help` | Print help. |
| `-V, --version` | Print version. |

Config commands:

| Command | Meaning |
|---|---|
| `transcriptor config get` | Show the saved default model. |
| `transcriptor config set-model <MODEL_NAME>` | Save a default model name such as `small` or `large-v3-turbo`. |
| `transcriptor config unset-model` | Remove the saved model setting and return to `base`. |

---

## Output Contract

Default output is JSON only on stdout:

```bash
transcriptor sample2.ogg
```

```json
{"text":"..."}
```

On a successful default run:

| Stream | Content |
|---|---|
| stdout | Exactly one JSON object followed by a newline. |
| stderr | Empty. |

When `--format text` is used, stdout contains only the transcript text followed
by a newline.

When `--verbose` is used, stdout keeps the selected output format and stderr
receives progress messages plus `whisper.cpp` logs.

When `--progress json` is used, stdout still keeps the selected output format.
stderr receives newline-delimited JSON events for model readiness and, when
needed, model download progress. Do not combine `--progress json` with
`--verbose` if the parent process expects every stderr line to be JSON. Runtime
errors and CLI parse errors are also emitted as JSON events when
`--progress json` is present. Each progress event is flushed after its newline.

Every progress event includes these common fields:

| Field | Meaning |
|---|---|
| `schema_version` | Progress event schema version. Current value: `1`. |
| `sequence` | Monotonic event sequence number from this process. |
| `timestamp_unix_ms` | Event timestamp in Unix milliseconds. |
| `event` | Event name. |

Common event flow when the model must be downloaded:

```text
process_started
transcription_started
audio_decode_started
audio_decode_finished
model_download_required
model_download_started
model_download_progress
model_download_finished
model_ready
model_load_started
model_load_finished
whisper_inference_started
whisper_inference_finished
transcription_finished
process_finished
```

Common event flow when the model is already available:

```text
process_started
transcription_started
audio_decode_started
audio_decode_finished
model_ready
model_load_started
model_load_finished
whisper_inference_started
whisper_inference_finished
transcription_finished
process_finished
```

If an explicit `--model` path or `TRANSCRIPTOR_MODEL` path is invalid, a
`model_unavailable` event is emitted before the error event.

Invalid config and invalid `--model-name` values fail before audio decoding.

If the audio file is broken or unsupported, `audio_decode_failed` is emitted
before the error event. The model is not downloaded or loaded in that case.

Example stderr events:

```json
{"schema_version":1,"sequence":5,"timestamp_unix_ms":1760000000000,"event":"model_download_required","source":"auto","model_name":"small","path":"/home/me/.transcriptor/models/ggml-small.bin","download_required":true}
{"schema_version":1,"sequence":6,"timestamp_unix_ms":1760000000200,"event":"model_download_started","model_name":"small","path":"/home/me/.transcriptor/models/ggml-small.bin","downloaded_bytes":0,"total_bytes":488377186,"percent":0.0,"url":"https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin"}
{"schema_version":1,"sequence":7,"timestamp_unix_ms":1760000000450,"event":"model_download_progress","model_name":"small","path":"/home/me/.transcriptor/models/ggml-small.bin","downloaded_bytes":1048576,"total_bytes":488377186,"percent":0.214704}
{"schema_version":1,"sequence":8,"timestamp_unix_ms":1760000012000,"event":"model_download_finished","model_name":"small","path":"/home/me/.transcriptor/models/ggml-small.bin","downloaded_bytes":488377186,"total_bytes":488377186,"percent":100.0}
{"schema_version":1,"sequence":9,"timestamp_unix_ms":1760000012001,"event":"model_ready","source":"download","model_name":"small","path":"/home/me/.transcriptor/models/ggml-small.bin","download_required":true}
```

If the model is already available, no download events are emitted. The parent
process receives a readiness event instead:

```json
{"schema_version":1,"sequence":5,"timestamp_unix_ms":1760000000000,"event":"model_ready","source":"cache","model_name":"small","path":"/home/me/.transcriptor/models/ggml-small.bin","download_required":false}
```

Broken audio example:

```json
{"schema_version":1,"sequence":4,"timestamp_unix_ms":1760000000001,"event":"audio_decode_failed","audio_path":"broken.ogg","message":"failed to decode audio","causes":["failed to decode audio","failed to probe audio format","unsupported feature: core (probe): no suitable format reader found"]}
```

Runtime error example:

```json
{"schema_version":1,"sequence":5,"timestamp_unix_ms":1760000000002,"event":"error","error_type":"runtime","exit_code":1,"message":"failed to decode audio","causes":["failed to decode audio","failed to probe audio format","unsupported feature: core (probe): no suitable format reader found"]}
```

---

## Spawn Integration

Applications that spawn `transcriptor` should use:

```bash
transcriptor --progress json audio.ogg
```

In this mode, stdout remains reserved for the final transcription result, and
stderr becomes a newline-delimited JSON event stream. The parent process can use
events such as `model_ready`, `model_download_required`,
`model_download_progress`, `audio_decode_started`,
`whisper_inference_started`, `transcription_finished`, `error`, and
`process_finished` to drive its UI or job state.

Because stdout and stderr are separate streams, parent applications should drain
stdout until EOF before parsing the final transcript.

The detailed spawn contract, event schema, event ordering, error behavior, and
Node.js/Python parsing examples are documented in
[SPAWN_INTEGRATION.md](SPAWN_INTEGRATION.md).

---

## Models and Runtime Files

Default runtime directory:

```text
~/.transcriptor/
```

Default config file:

```text
~/.transcriptor/config.json
```

Default model path:

```text
~/.transcriptor/models/ggml-base.bin
```

Model selection priority:

1. `--model <PATH>`
2. `TRANSCRIPTOR_MODEL`
3. `--model-name <MODEL_NAME>`
4. saved `model_name` from `~/.transcriptor/config.json`
5. built-in default model name: `base`

The default model name is `base`. This maps to:

```text
ggml-base.bin
```

Model names come from the whisper.cpp ggml model files. `transcriptor` maps a
name such as `small` to `ggml-small.bin` and downloads it from the default model
source when it is missing.

Common model names:

| Model name | Disk size | Notes |
|---|---:|---|
| `tiny` | 75 MiB | Fastest, lowest accuracy. |
| `base` | 142 MiB | Built-in default. |
| `small` | 466 MiB | Good default upgrade for Korean transcription. |
| `medium` | 1.5 GiB | Higher accuracy, slower CPU use. |
| `large-v3` | 2.9 GiB | High accuracy, largest common model. |
| `large-v3-turbo` | 1.5 GiB | Strong accuracy with better speed than full large. |

Useful variants:

| Suffix | Meaning |
|---|---|
| `.en` | English-only model. Do not use for Korean audio. |
| `q5_0`, `q5_1`, `q8_0` | Quantized models. Smaller downloads and lower memory use, with possible accuracy loss. |

Set a persistent default model:

```bash
transcriptor config set-model small
```

Show the current persistent setting:

```bash
transcriptor config get
```

Use a different auto-download model:

```bash
transcriptor --model-name small audio.ogg
```

Stream model status and download progress for a parent process:

```bash
transcriptor --progress json audio.ogg
```

Use a model file that is already present:

```bash
transcriptor --model ~/.transcriptor/models/ggml-base.bin audio.ogg
```

Run without network access:

```bash
transcriptor --no-download --model ~/.transcriptor/models/ggml-base.bin audio.ogg
```

The default model download source is:

```text
https://huggingface.co/ggerganov/whisper.cpp/resolve/main
```

The upstream model list is published at:

```text
https://huggingface.co/ggerganov/whisper.cpp
```

---

## Environment Variables

| Variable | Meaning |
|---|---|
| `TRANSCRIPTOR_HOME` | Runtime directory. Default: `~/.transcriptor`. |
| `TRANSCRIPTOR_MODEL` | Default model file path. Overridden by `--model`. |
| `TRANSCRIPTOR_LANGUAGE` | Default language. Overridden by `--language`. |
| `TRANSCRIPTOR_THREADS` | Default CPU worker thread count. Overridden by `--threads`. |
| `TRANSCRIPTOR_BASE_URL` | Installer base URL containing release binaries. |
| `TRANSCRIPTOR_INSTALL_DIR` | Installer destination directory. |

Examples:

```bash
TRANSCRIPTOR_LANGUAGE=ko transcriptor audio.ogg
TRANSCRIPTOR_THREADS=4 transcriptor audio.ogg
TRANSCRIPTOR_HOME="$HOME/.cache/transcriptor" transcriptor audio.ogg
```

---

## Supported Platforms

The build system produces these release targets:

| OS | CPU architectures |
|---|---|
| Linux | `x86_64`, `aarch64` |
| macOS | `x86_64`, `aarch64` |
| Windows | `x86_64`, `aarch64` |

The full release build command is:

```bash
python3 build.py --all --windows --no-color
```

---

## Dependencies

This project aims to avoid external runtime programs. In normal installed use,
the required runtime pieces are:

- the `transcriptor` binary
- a compatible whisper.cpp ggml model file under `~/.transcriptor/` or passed
  with `--model`

No `ffmpeg` executable is required.

### Direct Rust Dependencies

These are the direct crates declared in `Cargo.toml`:

| Project | Used for |
|---|---|
| `anyhow` | Error handling. |
| `clap` with `derive` | Command-line argument parsing. |
| `opus-decoder` | Pure Rust Opus packet decoding. |
| `serde_json` | JSON stdout formatting. |
| `symphonia` with `aac`, `isomp4`, `mp3` | Audio probing, container reading, and non-Opus audio decoding. |
| `ureq` | Model download over HTTP(S). |
| `whisper-rs` | Rust bindings around whisper.cpp. |

### Whisper and Model Dependencies

`transcriptor` depends on these upstream Whisper-related projects and artifacts:

- `whisper-rs`
- `whisper-rs-sys`
- `whisper.cpp`
- `ggml`, which is part of the whisper.cpp source tree
- whisper.cpp ggml model files such as `ggml-base.bin`
- Hugging Face hosted model artifacts from `ggerganov/whisper.cpp`

### Audio Dependencies

The audio path depends on:

- `symphonia`
- `symphonia-core`
- `symphonia-common`
- `symphonia-format-ogg`
- `symphonia-format-isomp4`
- `symphonia-format-mkv`
- `symphonia-format-riff`
- `symphonia-codec-vorbis`
- `symphonia-codec-aac`
- `symphonia-codec-adpcm`
- `symphonia-codec-pcm`
- `symphonia-bundle-flac`
- `symphonia-bundle-mp3`
- `symphonia-metadata`
- `opus-decoder`

### Build and Release Tooling Dependencies

The repository build system uses these tools or projects when building release
artifacts:

- Rust
- Cargo
- rustup
- Python 3
- Zig 0.13.0
- `cargo-zigbuild`
- `cargo-xwin`
- CMake
- Ninja
- LLVM
- Clang
- `clang-cl` 19 or newer for Windows MSVC cross builds
- LLD / `lld-link`
- `llvm-lib`
- macOS SDK 14.0 archive from `joseluisq/macosx-sdks`
- MSVC CRT/SDK files downloaded by `cargo-xwin`
- `curl` or `wget` for the Unix installer
- PowerShell `Invoke-WebRequest` for the Windows installer

Tools downloaded by the build system are placed under:

```text
builder/tools/
```

### Complete Resolved Cargo Package Set

The exact versions are recorded in `Cargo.lock`. The current resolved Cargo
package set, including direct, transitive, normal, build, and proc-macro
packages, contains:

```text
adler2
anstream
anstyle
anstyle-parse
anstyle-query
anyhow
autocfg
base64
bindgen
bitflags
bytemuck
cc
cexpr
cfg-if
clang-sys
clap
clap_builder
clap_derive
clap_lex
cmake
colorchoice
crc32fast
displaydoc
either
extended
find-msvc-tools
flate2
form_urlencoded
fs_extra
getrandom
glob
heck
icu_collections
icu_locale_core
icu_normalizer
icu_normalizer_data
icu_properties
icu_properties_data
icu_provider
idna
idna_adapter
is_terminal_polyfill
itertools
itoa
lazy_static
libc
libloading
litemap
log
memchr
minimal-lexical
miniz_oxide
nom
num-complex
num-integer
num-traits
once_cell
opus-decoder
percent-encoding
potential_utf
prettyplease
primal-check
proc-macro2
quote
regex
regex-automata
regex-lite
regex-syntax
ring
rustc-hash
rustfft
rustls
rustls-pki-types
rustls-webpki
semver
serde_core
serde_json
shlex
simd-adler32
smallvec
stable_deref_trait
strength_reduce
strsim
subtle
symphonia
symphonia-bundle-flac
symphonia-bundle-mp3
symphonia-codec-aac
symphonia-codec-adpcm
symphonia-codec-pcm
symphonia-codec-vorbis
symphonia-common
symphonia-core
symphonia-format-isomp4
symphonia-format-mkv
symphonia-format-ogg
symphonia-format-riff
symphonia-metadata
syn
synstructure
thiserror
thiserror-impl
tinystr
transpose
unicode-ident
untrusted
ureq
url
utf8_iter
utf8parse
webpki-roots
whisper-rs
whisper-rs-sys
writeable
yoke
yoke-derive
zerofrom
zerofrom-derive
zeroize
zerotrie
zerovec
zerovec-derive
zmij
```

---

## License and Notices

`transcriptor` is distributed under the MIT License. See [LICENSE](LICENSE).

The default STT model artifacts are OpenAI Whisper model weights converted to
whisper.cpp ggml format and hosted by `ggerganov/whisper.cpp` on Hugging Face.
OpenAI Whisper code and model weights are released under the MIT License, and
whisper.cpp is also released under the MIT License.

Runtime model files, upstream projects, and usage caveats are summarized in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

User-provided audio and generated transcripts may be subject to separate rights
or consent requirements that are not covered by the software/model licenses.

---

## Troubleshooting

### The model is missing

If `--no-download` is set and the model file is not present, either remove
`--no-download` or pass a valid model file:

```bash
transcriptor --model ~/.transcriptor/models/ggml-base.bin audio.ogg
```

### Model download fails

Check network access to Hugging Face. For offline use, prepare a model file in
advance and pass it with `--model` or place it in `~/.transcriptor/models/`.

### The language is wrong

The default language is `ko`. If the file is not Korean, pass a language code or
use automatic detection:

```bash
transcriptor -l en audio.ogg
transcriptor -l auto audio.ogg
```

Automatic language detection can be unreliable for short or noisy clips. If the
language is known, passing it explicitly is usually more stable.

### The transcript is empty

The decoded audio may be empty, too short, too quiet, or mostly non-speech.
Verify the input file.

### The command is not found after installation

Open a new terminal. On macOS and Linux, PATH changes for `$HOME/.local/bin` may
only apply to new shells.

---

## Update and Remove

Update on macOS or Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/kstost/transcriptor/refs/heads/main/manage.sh | bash -s update
```

Update on Windows:

```powershell
irm https://raw.githubusercontent.com/kstost/transcriptor/refs/heads/main/manage.ps1 | iex
```

Remove the installed binary on macOS or Linux:

```bash
rm -f ~/.local/bin/transcriptor
sudo rm -f /usr/local/bin/transcriptor
```

Remove the installed binary on Windows PowerShell:

```powershell
Remove-Item "$env:LOCALAPPDATA\transcriptor\transcriptor.exe" -Force
```

Remove runtime files and downloaded models:

```bash
rm -rf ~/.transcriptor
```

---

## Build From Source

Build for the current platform:

```bash
cargo build --release
```

Check build tool status:

```bash
python3 build.py --status
```

Install local build tools under `builder/tools`:

```bash
python3 build.py --setup
```

Build Linux and macOS release targets:

```bash
python3 build.py --all
```

Build Linux, macOS, and Windows release targets:

```bash
python3 build.py --all --windows
```

Release binaries are written to `dist_beta/`.

On Linux, Windows MSVC cross builds require `clang-cl` 19 or newer:

```bash
sudo ./install_windows_build_deps.sh
```

---

## How It Works

1. Resolve the model path from CLI options, environment variables, saved config,
   or the built-in default model name.
2. Download the model if it is missing and downloads are allowed.
3. Decode the input audio in Rust.
4. Downmix to mono.
5. Resample to 16 kHz float PCM.
6. Run Whisper through `whisper-rs`.
7. Collect all Whisper segments into one transcript string.
8. Print either JSON or plain text to stdout.

Runtime directory layout:

```text
~/.transcriptor/
|-- config.json
`-- models/
    `-- ggml-base.bin
```

Because audio decoding is handled in-process, installed use does not require an
external `ffmpeg` binary. Once a model file is present, transcription can run
without network access.

---

## Disclaimer

This software is provided as is, without warranty of any kind, whether express
or implied. This includes, but is not limited to, warranties of merchantability,
fitness for a particular purpose, and non-infringement.

In no event shall the author, copyright holders, or contributors be liable for
any claim, damages, or other liability arising from the use of, or inability to
use, this software. This includes, but is not limited to, data loss or
corruption, system malfunction, security issues, financial loss, and direct,
indirect, incidental, special, punitive, or consequential damages.

All risks and responsibilities arising from use of this software rest with the
user.
