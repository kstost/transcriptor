# transcriptor

> 오디오 파일을 Whisper로 전사해서 결과 텍스트만 stdout으로 출력하는 단일 실행 파일 CLI

`transcriptor`는 터미널에서 다음처럼 쓰기 위한 도구입니다.

```bash
transcriptor audio.ogg
```

성공하면 전사된 텍스트만 stdout으로 출력합니다. 모델 로딩, 다운로드, 오디오 디코딩 같은 진행 메시지는 stderr로 나가므로 다른 프로그램에서 결과만 파이프로 받기 쉽습니다.

별도 `ffmpeg`나 서버 프로세스를 요구하지 않습니다. 실행 파일은 오디오 디코딩과 Whisper 실행에 필요한 코드를 포함하고, 필요한 모델 파일은 `~/.transcriptor/` 아래에 준비합니다.

---

## 목차

1. [먼저 알아둘 개념](#1-먼저-알아둘-개념)
2. [이 앱이 해주는 일](#2-이-앱이-해주는-일)
3. [빠른 시작](#3-빠른-시작)
4. [설치하기](#4-설치하기)
5. [처음 실행해 보기](#5-처음-실행해-보기)
6. [CLI 명령어](#6-cli-명령어)
7. [모델과 저장 위치](#7-모델과-저장-위치)
8. [환경 변수](#8-환경-변수)
9. [지원 플랫폼](#9-지원-플랫폼)
10. [문제가 생겼을 때](#10-문제가-생겼을-때)
11. [업데이트와 제거](#11-업데이트와-제거)
12. [소스에서 빌드하기](#12-소스에서-빌드하기)
13. [작동 원리](#13-작동-원리)

---

## 1. 먼저 알아둘 개념

| 용어 | 뜻 |
|---|---|
| 터미널 | 글자로 명령을 입력하는 창입니다. macOS의 Terminal, Windows Terminal, Linux terminal 같은 앱입니다. |
| CLI | Command Line Interface의 줄임말입니다. 터미널에서 명령으로 쓰는 프로그램이라는 뜻입니다. |
| stdout | 프로그램의 표준 출력입니다. `transcriptor`는 전사 결과 텍스트만 stdout으로 출력합니다. |
| stderr | 오류나 진행 상태를 출력하는 표준 에러입니다. 모델 다운로드, 로딩 상태, 디코딩 상태는 stderr로 출력합니다. |
| Whisper | OpenAI가 공개한 음성 인식 모델 계열입니다. 이 프로젝트는 whisper.cpp 계열 ggml 모델 파일을 사용합니다. |
| 모델 파일 | Whisper가 음성을 텍스트로 바꾸기 위해 필요한 `.bin` 파일입니다. 기본값은 `ggml-base.bin`입니다. |
| `~` | 사용자 홈 폴더를 짧게 쓰는 표시입니다. 예를 들어 `~/.transcriptor/`는 내 홈 폴더 안의 `.transcriptor` 폴더입니다. |
| 단일 실행 파일 | 설치 후 `transcriptor` 하나만 실행하면 되는 형태입니다. 단, 모델 파일은 `~/.transcriptor/`에 있어야 합니다. |

---

## 2. 이 앱이 해주는 일

### 오디오 파일 전사

오디오 파일을 읽고 Whisper로 전사한 뒤 텍스트를 출력합니다.

```bash
transcriptor sample.ogg
```

### stdout에 텍스트만 출력

다른 명령과 조합하기 쉽도록 전사 결과만 stdout으로 나갑니다.

```bash
transcriptor meeting.ogg > meeting.txt
transcriptor memo.ogg | pbcopy
```

### 모델 자동 준비

`--model`이나 `TRANSCRIPTOR_MODEL`을 지정하지 않으면 기본 모델 `base`를 사용합니다. 모델 파일이 없으면 `~/.transcriptor/models/ggml-base.bin`으로 내려받습니다.

### 외부 오디오 도구 없이 디코딩

`ffmpeg` 같은 외부 실행 파일을 호출하지 않습니다. 현재 구현은 Rust 오디오 디코더를 사용해 입력을 읽고, Whisper가 요구하는 16 kHz mono float PCM으로 변환합니다.

### 언어 지정

기본 언어는 한국어(`ko`)입니다. 자동 감지를 쓰려면 `--language auto`를 지정합니다.

```bash
transcriptor -l ko sample.ogg
transcriptor -l auto sample.ogg
```

---

## 3. 빠른 시작

### 1단계: 설치

macOS/Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/kstost/transcriptor/refs/heads/main/manage.sh | bash
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/kstost/transcriptor/refs/heads/main/manage.ps1 | iex
```

### 2단계: 설치 확인

```bash
transcriptor --version
```

### 3단계: 실행

```bash
transcriptor audio.ogg
```

처음 실행할 때 기본 모델이 없으면 자동으로 다운로드합니다. 다운로드된 모델은 `~/.transcriptor/models/`에 저장됩니다.

### 4단계: 결과를 파일로 저장

```bash
transcriptor audio.ogg > transcript.txt
```

---

## 4. 설치하기

### macOS / Linux

터미널에서 다음 명령을 실행합니다.

```bash
curl -fsSL https://raw.githubusercontent.com/kstost/transcriptor/refs/heads/main/manage.sh | bash
```

설치 스크립트는 운영체제와 CPU 종류에 맞는 바이너리를 내려받아 `/usr/local/bin` 또는 `~/.local/bin`에 설치합니다.

설치 위치를 직접 지정하려면:

```bash
TRANSCRIPTOR_INSTALL_DIR="$HOME/bin" \
curl -fsSL https://raw.githubusercontent.com/kstost/transcriptor/refs/heads/main/manage.sh | bash
```

### Windows

PowerShell에서 실행합니다. 관리자 권한은 필요 없습니다.

```powershell
irm https://raw.githubusercontent.com/kstost/transcriptor/refs/heads/main/manage.ps1 | iex
```

기본 설치 위치는 `%LOCALAPPDATA%\transcriptor\`입니다. 스크립트는 이 폴더를 사용자 PATH에 추가합니다.

설치 후 현재 PowerShell에서 명령을 찾지 못하면 PowerShell 창을 새로 여세요.

### 배포 파일

설치 스크립트는 `dist_beta` 아래의 다음 파일을 사용합니다.

```text
transcriptor-linux-aarch64
transcriptor-linux-x86_64
transcriptor-macos-aarch64
transcriptor-macos-x86_64
transcriptor-windows-aarch64.exe
transcriptor-windows-x86_64.exe
```

---

## 5. 처음 실행해 보기

```bash
transcriptor sample.ogg
```

처음 실행하면 다음 순서로 동작합니다.

1. 모델 위치를 결정합니다.
2. 기본 모델이 없으면 `~/.transcriptor/models/`에 다운로드합니다.
3. 오디오 파일을 디코딩합니다.
4. 16 kHz mono로 변환합니다.
5. Whisper 전사를 실행합니다.
6. 전사 텍스트를 stdout으로 출력합니다.

진행 메시지는 stderr로 출력됩니다. 그래서 아래처럼 파일로 저장해도 진행 메시지는 터미널에 보이고, 결과 텍스트만 파일에 들어갑니다.

```bash
transcriptor sample.ogg > sample.txt
```

---

## 6. CLI 명령어

기본 형태:

```bash
transcriptor [OPTIONS] <AUDIO>
```

예:

```bash
transcriptor audio.ogg
transcriptor -l ko audio.ogg
transcriptor -l auto audio.ogg
transcriptor --model ~/.transcriptor/models/ggml-small.bin audio.ogg
transcriptor --model-name small audio.ogg
transcriptor --threads 4 audio.ogg
```

옵션:

| 옵션 | 뜻 |
|---|---|
| `<AUDIO>` | 전사할 오디오 파일입니다. |
| `-m, --model <MODEL>` | 사용할 whisper.cpp ggml 모델 파일 경로입니다. `TRANSCRIPTOR_MODEL`보다 우선합니다. |
| `--model-name <MODEL_NAME>` | 자동 다운로드할 모델 이름입니다. 기본값은 `base`입니다. |
| `-l, --language <LANGUAGE>` | 언어 코드입니다. 예: `ko`, `en`, `ja`, `auto`. 기본값은 `ko`입니다. |
| `-t, --threads <THREADS>` | 사용할 CPU worker thread 수입니다. 기본값은 사용 가능한 병렬 처리 수입니다. |
| `--no-download` | 모델 파일이 없을 때 자동 다운로드하지 않습니다. |
| `-h, --help` | 도움말을 출력합니다. |
| `-V, --version` | 버전을 출력합니다. |

---

## 7. 모델과 저장 위치

기본 모델 저장 위치:

```text
~/.transcriptor/models/ggml-base.bin
```

모델 선택 우선순위:

1. `--model <PATH>`
2. `TRANSCRIPTOR_MODEL`
3. `~/.transcriptor/models/ggml-<model-name>.bin`

기본 모델 이름은 `base`입니다. `--model-name small`을 지정하면 `ggml-small.bin`을 찾거나 다운로드합니다.

```bash
transcriptor --model-name small audio.ogg
```

모델을 직접 준비해 두고 네트워크 다운로드를 막으려면:

```bash
transcriptor --model ~/.transcriptor/models/ggml-base.bin --no-download audio.ogg
```

완전히 오프라인으로 쓰려면 실행 전에 모델 파일이 이미 준비되어 있어야 합니다.

---

## 8. 환경 변수

| 환경 변수 | 뜻 |
|---|---|
| `TRANSCRIPTOR_HOME` | 기본 런타임 디렉터리입니다. 기본값은 `~/.transcriptor`입니다. |
| `TRANSCRIPTOR_MODEL` | 기본 모델 파일 경로입니다. `--model`이 있으면 `--model`이 우선합니다. |
| `TRANSCRIPTOR_LANGUAGE` | 기본 언어입니다. `--language`가 있으면 `--language`가 우선합니다. |
| `TRANSCRIPTOR_THREADS` | 기본 CPU worker thread 수입니다. `--threads`가 있으면 `--threads`가 우선합니다. |
| `TRANSCRIPTOR_BASE_URL` | 설치 스크립트가 바이너리를 받을 base URL입니다. |
| `TRANSCRIPTOR_INSTALL_DIR` | 설치 스크립트의 설치 위치입니다. |

예:

```bash
TRANSCRIPTOR_LANGUAGE=ko transcriptor audio.ogg
TRANSCRIPTOR_THREADS=4 transcriptor audio.ogg
TRANSCRIPTOR_HOME="$HOME/.cache/transcriptor" transcriptor audio.ogg
```

---

## 9. 지원 플랫폼

빌드 산출물 기준 지원 대상:

| 운영체제 | CPU |
|---|---|
| Linux | x86_64, aarch64 |
| macOS | x86_64, aarch64 |
| Windows | x86_64, aarch64 |

Linux 호스트에서 다음 전체 빌드를 확인했습니다.

```bash
python3 build.py --all --windows --no-color
```

---

## 10. 문제가 생겼을 때

### `model is missing`

`--no-download`를 지정했는데 모델 파일이 없을 때 발생합니다. `--no-download`를 빼거나 `--model`로 실제 모델 파일을 지정하세요.

```bash
transcriptor --model ~/.transcriptor/models/ggml-base.bin audio.ogg
```

### 모델 다운로드가 실패함

네트워크가 막혀 있거나 Hugging Face 접근이 실패한 상태입니다. 다른 환경에서 모델 파일을 받아 `~/.transcriptor/models/`에 넣거나, `TRANSCRIPTOR_MODEL`로 직접 경로를 지정하세요.

### 결과 언어가 이상함

기본값은 `ko`입니다. 다른 언어 파일이면 언어를 지정하거나 자동 감지를 사용하세요.

```bash
transcriptor -l en audio.ogg
transcriptor -l auto audio.ogg
```

짧거나 잡음이 많은 파일에서는 자동 감지가 틀릴 수 있습니다. 언어를 알고 있다면 직접 지정하는 편이 안정적입니다.

### 전사 결과가 비어 있음

오디오가 너무 짧거나, 음성이 거의 없거나, 디코딩된 오디오가 비어 있을 수 있습니다. 입력 파일을 확인하세요.

### 명령을 찾을 수 없음

설치 후 새 터미널을 열어 보세요. macOS/Linux에서 `~/.local/bin`에 설치된 경우 PATH 반영이 새 셸에서 적용될 수 있습니다.

---

## 11. 업데이트와 제거

### macOS / Linux 업데이트

```bash
curl -fsSL https://raw.githubusercontent.com/kstost/transcriptor/refs/heads/main/manage.sh | bash -s update
```

### Windows 업데이트

```powershell
irm https://raw.githubusercontent.com/kstost/transcriptor/refs/heads/main/manage.ps1 | iex
```

### 제거

설치된 실행 파일을 삭제하면 됩니다.

macOS/Linux:

```bash
rm -f ~/.local/bin/transcriptor
sudo rm -f /usr/local/bin/transcriptor
```

Windows PowerShell:

```powershell
Remove-Item "$env:LOCALAPPDATA\transcriptor\transcriptor.exe" -Force
```

모델과 런타임 데이터를 지우려면 `~/.transcriptor/`도 삭제하세요.

---

## 12. 소스에서 빌드하기

현재 플랫폼만 빌드:

```bash
cargo build --release
```

빌드 시스템 상태 확인:

```bash
python3 build.py --status
```

빌드 도구 준비:

```bash
python3 build.py --setup
```

Linux/macOS 전체 빌드:

```bash
python3 build.py --all
```

Windows까지 포함한 전체 빌드:

```bash
python3 build.py --all --windows
```

산출물은 `dist_beta/`에 생성됩니다.

Windows cross build는 Linux에서 `clang-cl` 19 이상이 필요합니다. 필요한 패키지를 설치하려면:

```bash
sudo ./install_windows_build_deps.sh
```

---

## 13. 작동 원리

`transcriptor`는 입력 오디오를 Rust 코드에서 직접 디코딩한 뒤 mono 16 kHz float PCM으로 변환합니다. 그 다음 `whisper-rs`를 통해 whisper.cpp 모델을 실행하고, 세그먼트 텍스트를 이어 붙여 stdout으로 출력합니다.

기본 런타임 파일은 `~/.transcriptor/` 아래에 둡니다.

```text
~/.transcriptor/
└── models/
    └── ggml-base.bin
```

외부 프로그램을 실행해 오디오를 변환하지 않기 때문에 설치 후 필요한 것은 `transcriptor` 실행 파일과 모델 파일입니다. 모델이 이미 준비되어 있으면 네트워크 없이도 전사할 수 있습니다.
