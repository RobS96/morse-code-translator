# Morse Code Translator

[![CI](https://github.com/RobS96/morse-code-translator/actions/workflows/ci.yml/badge.svg)](https://github.com/RobS96/morse-code-translator/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org)

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
git clone https://github.com/RobS96/morse-code-translator.git && cd morse-code-translator
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

Tagged releases publish binaries for Windows, macOS (universal), and
Linux under
[Releases](https://github.com/RobS96/morse-code-translator/releases) —
no Rust toolchain needed.

Alternatively, install just the CLI straight from a clone:

```bash
cargo install --path morse-cli
```

## Usage — CLI

```bash
morse encode "SOS"                 # -> ... --- ...
morse decode "... --- ..."         # -> SOS
morse transmit "HELLO WORLD"       # flashes + beeps it live in your terminal
morse transmit "SOS" --wpm 25      # faster: 25 words-per-minute
morse transmit "SOS" -u 60         # or set the raw unit length directly (ms)
```

Multi-word Morse uses `/` as the word separator:

```bash
morse decode ".... .. / - .... . .-. ."   # -> HI THERE
```

**Procedural signs ("prosigns").** Common ham-radio prosigns — `<AR>`
(end of message), `<SK>` (end of contact), `<BT>` (new paragraph/break),
`<KN>` (over to a specific station), `<AS>` (wait), `<CT>` (start
copying), `<BK>`, `<SN>` — encode as a single fused character with no
inter-letter gap, matching how they're actually sent on the air:

```bash
morse encode "CQ CQ DE W1AW <KN>"
```

> Some prosigns (`AR`, `AS`, `BT`, `KN`) happen to share their fused code
> with an existing punctuation mark (`+`, `&`, `=`, `(`) — that's real
> Morse, not a bug here: historically, prosigns were built by fusing
> letter pairs that already had a code. Out of context the two readings
> are genuinely indistinguishable on the air, so `decode` resolves the
> ambiguity toward the punctuation reading.

**Farnsworth timing.** Recommended by the ARRL and CW Academy for
*learning* Morse: characters play at a brisk, natural speed while the
pauses between letters/words stretch out, giving you recognition time
without encouraging you to count dits and dahs:

```bash
morse transmit "PARIS" --wpm 20 --farnsworth-wpm 5   # 20 WPM characters, 5 WPM spacing
```

## Alphabets

Besides International (Latin) Morse, the translator speaks the national
Morse alphabets for **Cyrillic** (Russian standard, plus Ukrainian І/Є/Ї
and Bulgarian Ъ), **Greek**, **Hebrew**, **Arabic**, **Persian**,
**Japanese** (Wabun kana) and **Korean** (Hangul jamo). Digits and
punctuation are shared; Arabic-Indic, Persian and full-width digits are
accepted too.

| | Encode (text → Morse) | Decode (Morse → text) |
|---|---|---|
| Alphabet choice | Detected from the text; override with `--alphabet` | `--alphabet`, default `latin`: the same dots and dashes mean different letters in each alphabet |
| Normalisation | Lowercase, Greek tonos, Hebrew final letters, Ё, katakana, small kana, voiced kana (が → か + ゛) and Hangul syllables (한 → ㅎㅏㄴ) are all accepted | Hebrew final forms are restored at word ends and voiced kana are recomposed; Korean comes back as jamo, because regrouping jamo into syllables is ambiguous |
| Accented Latin (Ä, Ñ, Ś, …) | Encoded with their extension codes | Decoded as plain ASCII: most extension codes are shared (Ä/Æ/Ą) or collide with prosigns |

Arabic and Persian share letters but not codes (خ is `---` in Arabic and
`-..-` in Persian). Text containing a Persian-only letter (پ چ ژ گ ک ی)
is detected as Persian; anything else in Arabic script is detected as
Arabic. Pass `-a persian` or `-a arabic` to be explicit.

```bash
morse alphabets                                        # list them
morse encode "привет"                                  # .--. .-. .. .-- . -
morse decode ".--. .-. .. .-- . -" --alphabet cyrillic # ПРИВЕТ
morse encode "こんにちは"                                # ---- .-.-. -.-. ..-. -...
morse decode "---- .-.-. -.-. ..-. -..." -a japanese    # こんにちは
```

Every table is parsed from the ITU-R M.1677-1-derived tables on Wikipedia
(Korean from the Republic of Korea's radio-station operating regulation),
and a unit test asserts no two letters in an alphabet share a code.

## Usage — GUI

```bash
cargo run --release -p morse-gui
# or, once built:
./target/release/morse-gui
```

- Type text (or Morse) — the translation updates live.
- In Encode mode, click a prosign chip (`<AR>`, `<SK>`, ...) to insert it.
- Hit **📋 Copy** to copy the translated result to your clipboard.
- Drag the **Character speed** slider (in WPM); tick **Farnsworth
  timing** to reveal a second, slower **Effective speed** slider for the
  letter/word gaps.
- Hit **▶ Transmit** — the lamp flashes and a tone plays in sync.
- Pick the **Alphabet** (auto-detected by default) and the interface
  **Language**: English, Español, Français, Deutsch, Italiano, Português,
  Русский, Українська, Ελληνικά, 日本語, 한국어 or 简体中文. The language
  defaults from `LANGUAGE`/`LC_ALL`/`LC_MESSAGES`/`LANG`.
- Hebrew, Arabic, Japanese, Korean and Chinese glyphs use a font from
  your OS (Arial Unicode on macOS; Noto/DejaVu on Linux; Arial, Yu
  Gothic, Malgun Gothic or Microsoft YaHei on Windows). Nothing is bundled
  or downloaded. If none is found, a warning appears.
- Limitation: egui has no right-to-left layout or Arabic letter shaping,
  so Hebrew/Arabic/Persian *input* shows in logical order with unjoined
  letters. The Morse output is unaffected. For the same reason the
  interface itself isn't offered in RTL languages.

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
