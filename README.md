# Morse Code Translator

[![CI](https://github.com/RobS96/Morse-Code-Translator-/actions/workflows/ci.yml/badge.svg)](https://github.com/RobS96/Morse-Code-Translator-/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](https://www.rust-lang.org)

Encode, decode, and **transmit** Morse code — as a terminal tool or a
native desktop app — on **Windows, macOS, and Linux**. "Transmit" flashes
a lamp and plays a tone in real time, timed to standard Morse ratios, so
you can see and hear the rhythm, not just read it.

```
Dot  = short flash/beep  (1 unit)
Dash = long flash/beep   (3 units)
· gap between symbols      (1 unit)
· gap between letters      (3 units)
· gap between words        (7 units)
```

## What's inside

| Crate        | What it is                                                        |
| ------------ | ------------------------------------------------------------------ |
| `morse-core` | Pure encode/decode/timing logic. No I/O — fully unit tested.      |
| `morse-cli`  | Terminal tool: `encode`, `decode`, `transmit` (bell + ANSI flash). |
| `morse-gui`  | Native desktop app (eframe/egui): live lamp + audible tone.        |

```
┌─────────────┐     ┌────────────┐
│  morse-cli  │────▶│            │
└─────────────┘     │ morse-core │  (encode / decode / signal timing)
┌─────────────┐     │            │
│  morse-gui  │────▶│            │
└─────────────┘     └────────────┘
```

## Install

### Everyone (build from source)

Requires the [Rust toolchain](https://rustup.rs) (stable).

```bash
git clone https://github.com/RobS96/Morse-Code-Translator-.git && cd Morse-Code-Translator-
```

**Linux only** — the GUI needs windowing/audio headers to *build*:

```bash
sudo apt-get update && sudo apt-get install -y libx11-dev libxkbcommon-dev libxkbcommon-x11-dev libgl1-mesa-dev libasound2-dev pkg-config
```

macOS and Windows need nothing extra — the system frameworks are used
automatically.

Then build everything:

```bash
cargo build --release --workspace
```

Binaries land in `target/release/`:
- `morse` (`morse.exe` on Windows) — CLI
- `morse-gui` (`morse-gui.exe` on Windows) — desktop app

### Pre-built binaries

Tagged releases publish binaries for Windows, macOS, and Linux under
[Releases](https://github.com/RobS96/Morse-Code-Translator-/releases) —
no Rust toolchain needed.

## Usage — CLI

```bash
morse encode "SOS"                 # -> ... --- ...
morse decode "... --- ..."         # -> SOS
morse transmit "HELLO WORLD"       # flashes + beeps it live in your terminal
morse transmit "SOS" -u 60         # faster: 60ms per unit (default 100)
```

Multi-word Morse uses `/` as the word separator:

```bash
morse decode ".... .. / - .... . .-. ."   # -> HI THERE
```

## Usage — GUI

```bash
cargo run --release -p morse-gui
# or, once built:
./target/release/morse-gui
```

- Type text (or Morse) — the translation updates live.
- Drag the speed slider to change ms/unit.
- Hit **▶ Transmit** — the lamp flashes and a tone plays in sync.

## Try it hands-on

Transmit `"SOS"` and watch/listen: three short flashes, three long, three
short. That 1/3/1/3/7-unit rhythm is the entire timing system in
miniature — everything else is just more letters.

## Development

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI (`.github/workflows/ci.yml`) runs all three on **Ubuntu, macOS, and
Windows** for every push/PR, and builds per-OS release binaries on tags.

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the full workflow, and
[`CHANGELOG.md`](CHANGELOG.md) for release history.

## License

MIT — see [LICENSE](LICENSE).
