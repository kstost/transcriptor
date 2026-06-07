# Build

`transcriptor` uses the Python build system adapted from `cokacmux`.

## Status

```sh
python3 build.py --status
```

## Local Tool Setup

Build tools are installed under `builder/tools` and are ignored by git.

```sh
python3 build.py --setup
```

For Linux/macOS cross builds only:

```sh
python3 build.py --setup-cross
```

For Windows cross builds:

```sh
python3 build.py --setup-windows
```

On Linux, Windows MSVC cross builds require `clang-cl` 19 or newer. If the
system packages are missing, install them with:

```sh
sudo ./install_windows_build_deps.sh
```

## Build

Current platform:

```sh
python3 build.py --native
```

All Linux and macOS targets:

```sh
python3 build.py --all
```

Include Windows targets:

```sh
python3 build.py --all --windows
```

Output binaries are written to `dist_beta`, for example:

```text
dist_beta/transcriptor-linux-aarch64
dist_beta/transcriptor-linux-x86_64
dist_beta/transcriptor-macos-aarch64
dist_beta/transcriptor-macos-x86_64
dist_beta/transcriptor-windows-x86_64.exe
dist_beta/transcriptor-windows-aarch64.exe
```
