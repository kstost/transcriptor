# Spawn Integration Specification

This document defines the process and stream contract for applications that
spawn `transcriptor` as a child process.

Use this mode when the parent application needs to monitor model readiness,
model downloads, transcription phases, and failures while keeping the final
transcript output separate.

---

## Command

Run `transcriptor` with JSON progress events enabled:

```bash
transcriptor --progress json audio.ogg
```

The parent process should capture both stdout and stderr independently.

Do not combine `--progress json` with `--verbose` if the parent expects every
stderr line to be JSON. `--verbose` may add human-readable logs to stderr.

---

## Stream Contract

| Stream | Content |
|---|---|
| stdout | Final transcription result only. JSON by default, or plain text with `--format text`. |
| stderr | Newline-delimited JSON progress events when `--progress json` is used. |

Each stderr event is emitted as exactly one JSON object followed by `\n`.
Each event is flushed immediately after the newline.

The parent should parse stderr line by line. Do not parse stderr as one large
JSON document.

On success:

1. stderr receives progress events.
2. stdout receives the final transcript.
3. stderr receives `process_finished` with `success: true`.
4. The process exits with code `0`.

`transcriptor` writes and flushes stdout before emitting the successful
`process_finished` event. Because stdout and stderr are separate OS streams, a
parent process should still drain stdout until EOF or wait for child process
exit before parsing the final transcript. Do not rely on cross-stream read
arrival order.

On runtime failure:

1. stderr receives progress events up to the failed phase.
2. stderr receives `error`.
3. stderr receives `process_finished` with `success: false`.
4. The process exits with a non-zero code.

On CLI parse failure, if `--progress json` or `--progress=json` is present:

1. stderr receives `error` with `error_type: "cli_parse"`.
2. stderr receives `process_finished` with `success: false`.
3. The process exits with Clap's exit code, usually `2`.

Help and version output keep Clap's normal behavior and are not emitted as
progress JSON.

---

## Common Event Fields

Every progress event includes these fields:

| Field | Type | Meaning |
|---|---|---|
| `schema_version` | number | Progress event schema version. Current value: `1`. |
| `sequence` | number | Monotonic event sequence number within this process. Starts at `1`. |
| `timestamp_unix_ms` | number | Unix timestamp in milliseconds. |
| `event` | string | Event name. |

The parent should ignore unknown fields and unknown event names unless it has a
specific reason to fail closed.

The parent should treat `schema_version` changes as compatibility boundaries.
For schema version `1`, fields documented here are stable unless a future
release explicitly documents otherwise.

---

## Event Flow

### Model Already Available

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

### Model Download Required

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

There can be zero or more `model_download_progress` events. Very small or very
fast downloads may emit only the final progress event.

### Invalid Explicit Model Path

This applies to `--model <PATH>` and `TRANSCRIPTOR_MODEL`.

```text
process_started
transcription_started
model_unavailable
error
process_finished
```

### Invalid Config or Model Name

This applies to invalid saved config and invalid `--model-name` values.

```text
process_started
transcription_started
error
process_finished
```

### Missing Auto-Download Model With `--no-download`

```text
process_started
transcription_started
audio_decode_started
audio_decode_finished
model_download_required
error
process_finished
```

### Broken or Unsupported Audio

```text
process_started
transcription_started
audio_decode_started
audio_decode_failed
error
process_finished
```

The model is not downloaded or loaded when audio decoding fails.

### CLI Parse Error

When `--progress json` is present:

```text
error
process_finished
```

No `process_started` event is emitted for parse errors because parsing did not
produce a valid run configuration.

---

## Event Reference

### `process_started`

Emitted after CLI parsing succeeds and before transcription work begins.

Additional fields: none.

Example:

```json
{"schema_version":1,"sequence":1,"timestamp_unix_ms":1760000000000,"event":"process_started"}
```

### `transcription_started`

Emitted when the requested transcription operation starts.

| Field | Type | Meaning |
|---|---|---|
| `audio_path` | string | Audio path passed to the process. |

Example:

```json
{"schema_version":1,"sequence":2,"timestamp_unix_ms":1760000000001,"event":"transcription_started","audio_path":"audio.ogg"}
```

### `model_ready`

Emitted when a model file is available and will be loaded.

| Field | Type | Meaning |
|---|---|---|
| `source` | string | `cli`, `env`, `cache`, or `download`. |
| `model_name` | string or null | Model name when known. Null for explicit file paths. |
| `path` | string | Model file path. |
| `download_required` | boolean | Whether this run had to download the model. |

Example:

```json
{"schema_version":1,"sequence":3,"timestamp_unix_ms":1760000000002,"event":"model_ready","source":"cache","model_name":"base","path":"/home/me/.transcriptor/models/ggml-base.bin","download_required":false}
```

### `model_unavailable`

Emitted when an explicit model path exists in the selection chain but is not a
usable file.

| Field | Type | Meaning |
|---|---|---|
| `source` | string | `cli`, `env`, or `cache`. |
| `model_name` | string or null | Model name when known. Null for explicit file paths. |
| `path` | string | Model file path that failed validation. |
| `download_required` | boolean | Always false for this event in schema version `1`. |

Example:

```json
{"schema_version":1,"sequence":3,"timestamp_unix_ms":1760000000002,"event":"model_unavailable","source":"cli","model_name":null,"path":"/tmp/missing.bin","download_required":false}
```

### `model_download_required`

Emitted when the selected auto-download model is not present locally.

| Field | Type | Meaning |
|---|---|---|
| `source` | string | `auto`. |
| `model_name` | string | Selected model name. |
| `path` | string | Destination model path. |
| `download_required` | boolean | Always true. |

Example:

```json
{"schema_version":1,"sequence":3,"timestamp_unix_ms":1760000000002,"event":"model_download_required","source":"auto","model_name":"small","path":"/home/me/.transcriptor/models/ggml-small.bin","download_required":true}
```

### `model_download_started`

Emitted after the HTTP response is received and before bytes are written.

| Field | Type | Meaning |
|---|---|---|
| `model_name` | string | Selected model name. |
| `path` | string | Destination model path. |
| `downloaded_bytes` | number | Bytes downloaded so far. Starts at `0`. |
| `total_bytes` | number or null | HTTP `Content-Length` when available. |
| `percent` | number or null | Download percentage when `total_bytes` is known. |
| `url` | string | Download URL. |

Example:

```json
{"schema_version":1,"sequence":4,"timestamp_unix_ms":1760000000200,"event":"model_download_started","model_name":"small","path":"/home/me/.transcriptor/models/ggml-small.bin","downloaded_bytes":0,"total_bytes":488377186,"percent":0.0,"url":"https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin"}
```

### `model_download_progress`

Emitted while bytes are being written. Events are throttled by byte count and
time interval.

| Field | Type | Meaning |
|---|---|---|
| `model_name` | string | Selected model name. |
| `path` | string | Destination model path. |
| `downloaded_bytes` | number | Bytes downloaded so far. |
| `total_bytes` | number or null | HTTP `Content-Length` when available. |
| `percent` | number or null | Download percentage when `total_bytes` is known. |

Example:

```json
{"schema_version":1,"sequence":5,"timestamp_unix_ms":1760000000450,"event":"model_download_progress","model_name":"small","path":"/home/me/.transcriptor/models/ggml-small.bin","downloaded_bytes":1048576,"total_bytes":488377186,"percent":0.214704}
```

### `model_download_finished`

Emitted after the temporary download file is fully written and moved into place.

| Field | Type | Meaning |
|---|---|---|
| `model_name` | string | Selected model name. |
| `path` | string | Final model path. |
| `downloaded_bytes` | number | Total bytes written. |
| `total_bytes` | number or null | HTTP `Content-Length` when available. |
| `percent` | number or null | Download percentage when `total_bytes` is known. |

Example:

```json
{"schema_version":1,"sequence":6,"timestamp_unix_ms":1760000012000,"event":"model_download_finished","model_name":"small","path":"/home/me/.transcriptor/models/ggml-small.bin","downloaded_bytes":488377186,"total_bytes":488377186,"percent":100.0}
```

### `model_load_started`

Emitted before the Whisper model is loaded.

| Field | Type | Meaning |
|---|---|---|
| `model_path` | string | Model file path. |

### `model_load_finished`

Emitted after the Whisper model is loaded.

| Field | Type | Meaning |
|---|---|---|
| `model_path` | string | Model file path. |

### `audio_decode_started`

Emitted before audio decoding begins.

| Field | Type | Meaning |
|---|---|---|
| `audio_path` | string | Audio path passed to the process. |

### `audio_decode_finished`

Emitted after audio decoding, downmixing, and resampling finish.

| Field | Type | Meaning |
|---|---|---|
| `audio_path` | string | Audio path passed to the process. |
| `sample_rate` | number | Output sample rate sent to Whisper. Current value: `16000`. |
| `sample_count` | number | Number of decoded mono samples. |
| `duration_seconds` | number | Approximate decoded audio duration. |

Example:

```json
{"schema_version":1,"sequence":7,"timestamp_unix_ms":1760000001000,"event":"audio_decode_finished","audio_path":"audio.ogg","sample_rate":16000,"sample_count":42456,"duration_seconds":2.6535}
```

### `audio_decode_failed`

Emitted when audio decoding, downmixing, or resampling fails. This usually
means the input is not an audio file, is corrupted, has no supported audio
track, or uses an unsupported codec/container.

| Field | Type | Meaning |
|---|---|---|
| `audio_path` | string | Audio path passed to the process. |
| `message` | string | Top-level decode error message. |
| `causes` | array of strings | Error chain from top-level context to root cause. |

Example:

```json
{"schema_version":1,"sequence":4,"timestamp_unix_ms":1760000001001,"event":"audio_decode_failed","audio_path":"broken.ogg","message":"failed to decode audio","causes":["failed to decode audio","failed to probe audio format","unsupported feature: core (probe): no suitable format reader found"]}
```

### `whisper_inference_started`

Emitted immediately before Whisper inference begins.

| Field | Type | Meaning |
|---|---|---|
| `model_path` | string | Model file path. |

### `whisper_inference_finished`

Emitted after Whisper inference returns successfully.

| Field | Type | Meaning |
|---|---|---|
| `model_path` | string | Model file path. |

### `transcription_finished`

Emitted after Whisper segments are collected into the final transcript string.
The transcript text itself is not duplicated in stderr.

| Field | Type | Meaning |
|---|---|---|
| `segment_count` | number | Number of Whisper segments collected. |
| `text_bytes` | number | UTF-8 byte length of the final transcript. |
| `text_chars` | number | Unicode scalar count of the final transcript. |

Example:

```json
{"schema_version":1,"sequence":10,"timestamp_unix_ms":1760000003000,"event":"transcription_finished","segment_count":1,"text_bytes":50,"text_chars":20}
```

### `error`

Emitted when the process fails and `--progress json` is active.

| Field | Type | Meaning |
|---|---|---|
| `error_type` | string | `runtime` or `cli_parse`. |
| `exit_code` | number | Exit code that will be used. |
| `message` | string | Human-readable error message. |
| `causes` | array of strings | Error chain. For CLI parse errors this contains the parse message. |

Example:

```json
{"schema_version":1,"sequence":5,"timestamp_unix_ms":1760000000001,"event":"error","error_type":"runtime","exit_code":1,"message":"failed to decode audio","causes":["failed to decode audio","failed to probe audio format"]}
```

### `process_finished`

Emitted at the end of the progress stream.

| Field | Type | Meaning |
|---|---|---|
| `success` | boolean | Whether the process completed successfully. |
| `exit_code` | number | Process exit code. |

On successful runs, stdout is written and flushed before this event.

Example:

```json
{"schema_version":1,"sequence":11,"timestamp_unix_ms":1760000003010,"event":"process_finished","success":true,"exit_code":0}
```

---

## Parent Process State Guidance

A parent application can treat these event groups as phases:

| Phase | Events |
|---|---|
| Starting | `process_started`, `transcription_started` |
| Model selection | `model_ready`, `model_unavailable`, `model_download_required` |
| Download | `model_download_started`, `model_download_progress`, `model_download_finished` |
| Model loading | `model_load_started`, `model_load_finished` |
| Audio decoding | `audio_decode_started`, `audio_decode_finished`, `audio_decode_failed` |
| Inference | `whisper_inference_started`, `whisper_inference_finished` |
| Completion | `transcription_finished`, `process_finished` |
| Failure | `error`, `process_finished` with `success: false` |

Recommended handling:

- Use `sequence` to preserve ordering.
- Use `process_finished` as the terminal progress event.
- Use the child process exit code as the final authority for success/failure.
- Read stdout only for the final transcript.
- Drain stdout until EOF before parsing the final transcript.
- Read stderr line by line and parse each line as JSON.
- Ignore unknown event fields.
- Avoid assuming every future event flow will contain the same number of events.
- Do not use `--verbose` in machine-integration mode.

---

## Node.js Example

```js
import { spawn } from "node:child_process";
import readline from "node:readline";

const child = spawn("transcriptor", ["--progress", "json", "audio.ogg"], {
  stdio: ["ignore", "pipe", "pipe"],
});

let stdout = "";
child.stdout.setEncoding("utf8");
child.stdout.on("data", (chunk) => {
  stdout += chunk;
});

const stderrLines = readline.createInterface({ input: child.stderr });
stderrLines.on("line", (line) => {
  if (!line.trim()) return;

  let event;
  try {
    event = JSON.parse(line);
  } catch {
    // This should not happen when --progress json is used without --verbose.
    return;
  }

  switch (event.event) {
    case "model_ready":
      console.log("Model ready:", event.path);
      break;
    case "model_download_required":
      console.log("Downloading model:", event.model_name);
      break;
    case "model_download_progress":
      console.log("Download:", event.percent ?? event.downloaded_bytes);
      break;
    case "error":
      console.error("transcriptor error:", event.message);
      break;
  }
});

child.on("close", (code) => {
  if (code === 0) {
    const result = JSON.parse(stdout);
    console.log(result.text);
  } else {
    console.error(`transcriptor exited with ${code}`);
  }
});
```

---

## Python Example

```python
import json
import subprocess
import sys
import threading

proc = subprocess.Popen(
    ["transcriptor", "--progress", "json", "audio.ogg"],
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
)

events = []

def read_stderr():
    for line in proc.stderr:
        line = line.strip()
        if not line:
            continue
        event = json.loads(line)
        events.append(event)
        if event["event"] == "model_download_progress":
            print("download", event.get("percent"), file=sys.stderr)

thread = threading.Thread(target=read_stderr)
thread.start()

stdout = proc.stdout.read()
code = proc.wait()
thread.join()

if code == 0:
    result = json.loads(stdout)
    print(result["text"])
else:
    raise SystemExit(code)
```

---

## Compatibility Notes

- `--progress json` is opt-in. Default runs keep stderr quiet on success.
- `--progress json` does not change stdout format.
- `--format text` changes stdout only; progress events remain JSON.
- `model_name` can be null when the model is selected by explicit file path.
- `total_bytes` and `percent` can be null if the server does not provide a
  content length.
- Config, model-name, and explicit model path validation runs before audio
  decoding. Model readiness and download events are emitted only after audio
  decoding succeeds. Broken or unsupported audio fails before any model download
  starts.
- Cross-stream arrival order between stdout and stderr is not guaranteed by the
  operating system.
- Parse errors are JSON only when `--progress json` or `--progress=json` is
  present in the command line.
- Help and version output are not progress events.
