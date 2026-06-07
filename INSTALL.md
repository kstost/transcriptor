# Install

The install scripts follow the same pattern as `cokacmux`: they download the
prebuilt binary for the current OS/architecture from `dist_beta`, install it,
and verify it by running `transcriptor --version`.

## Linux / macOS

```sh
curl -fsSL https://raw.githubusercontent.com/kstost/transcriptor/refs/heads/main/manage.sh | bash
```

Update uses the same script:

```sh
curl -fsSL https://raw.githubusercontent.com/kstost/transcriptor/refs/heads/main/manage.sh | bash -s update
```

The default install location is `/usr/local/bin` when writable or sudo is
available; otherwise it falls back to `$HOME/.local/bin`.

Override the binary source or install directory:

```sh
TRANSCRIPTOR_BASE_URL=https://example.com/dist_beta \
TRANSCRIPTOR_INSTALL_DIR="$HOME/bin" \
bash manage.sh install
```

## Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/kstost/transcriptor/refs/heads/main/manage.ps1 | iex
```

The default install location is `%LOCALAPPDATA%\transcriptor`, and the script
adds that directory to the user PATH.

Override the binary source or install directory:

```powershell
$env:TRANSCRIPTOR_BASE_URL = "https://example.com/dist_beta"
$env:TRANSCRIPTOR_INSTALL_DIR = "$env:USERPROFILE\bin"
.\manage.ps1 install
```

## Expected Release Files

The scripts expect these files under `dist_beta`:

```text
transcriptor-linux-aarch64
transcriptor-linux-x86_64
transcriptor-macos-aarch64
transcriptor-macos-x86_64
transcriptor-windows-aarch64.exe
transcriptor-windows-x86_64.exe
```

