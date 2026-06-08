# Third-Party Notices

`transcriptor` uses third-party open-source software and model artifacts. This
document summarizes the main notices relevant to runtime behavior and model
downloads.

This notice file is informational and is not a substitute for the license files
distributed by each upstream project.

---

## Project License

`transcriptor` is distributed under the MIT License. See [LICENSE](LICENSE).

---

## Whisper Models

`transcriptor` uses OpenAI Whisper speech recognition models through
whisper.cpp-compatible ggml model files.

The OpenAI Whisper repository states that the Whisper code and model weights are
released under the MIT License.

- Project: OpenAI Whisper
- Copyright: Copyright (c) 2022 OpenAI
- License: MIT License
- Repository: https://github.com/openai/whisper
- License text: https://github.com/openai/whisper/blob/main/LICENSE
- Model card: https://github.com/openai/whisper/blob/main/model-card.md

The default model download source used by `transcriptor` is the Hugging Face
repository `ggerganov/whisper.cpp`, which hosts OpenAI Whisper models converted
to ggml format for whisper.cpp.

- Model repository: https://huggingface.co/ggerganov/whisper.cpp
- Repository license label: MIT
- Default download base URL:
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main

Model files are downloaded at runtime into:

```text
~/.transcriptor/models/
```

The built-in default model name is `base`, which maps to:

```text
ggml-base.bin
```

---

## whisper.cpp and ggml

`transcriptor` uses `whisper-rs`, which binds to whisper.cpp. whisper.cpp is a
C/C++ implementation for running Whisper models and includes ggml components.

- Project: whisper.cpp
- Copyright: Copyright (c) 2023-2026 The ggml authors
- License: MIT License
- Repository: https://github.com/ggml-org/whisper.cpp
- License text: https://github.com/ggml-org/whisper.cpp/blob/master/LICENSE

---

## Rust Runtime Dependencies

`transcriptor` directly depends on these Rust crates:

| Crate | Purpose |
|---|---|
| `anyhow` | Error handling. |
| `clap` | Command-line argument parsing. |
| `opus-decoder` | Opus packet decoding. |
| `serde_json` | JSON stdout and progress event encoding. |
| `symphonia` | Audio container and codec decoding. |
| `ureq` | Model downloads over HTTP. |
| `whisper-rs` | Rust bindings around whisper.cpp. |

The complete resolved Cargo dependency set is recorded in `Cargo.lock`.

---

## Audio and Transcript Rights

The MIT licenses above cover the relevant software and model artifacts. They do
not grant rights to third-party audio content supplied by users.

Users are responsible for ensuring they have the necessary rights or consent to
transcribe input audio. Transcription output can inherit legal or contractual
restrictions from the source audio or surrounding usage context.

---

## Model Limitations and Use

The OpenAI Whisper model card describes limitations and risks including
hallucinated text, uneven language performance, and concerns around
transcribing recordings of people without consent.

Before deploying `transcriptor` in production, evaluate transcription quality
and consent/privacy requirements for the intended domain.
