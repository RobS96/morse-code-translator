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
| `morse-cli`  | Terminal tool: `encode`, `decode`, `transmit` (bell + ANSI flash), `wav` (audio file). |
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

Requires the [Rust toolchain](https://rustup.rs) (stable): Rust 1.88 or
newer for `morse-core` and `morse-cli`, 1.95 or newer for `morse-gui`.

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
no Rust toolchain needed. Each archive carries a CycloneDX SBOM per crate
under `sbom/`, and each platform has a `SHA256SUMS-<platform>.txt` beside
it. To check a download, run these in the folder holding the archive and
its checksum file:

```bash
# Linux
sha256sum -c SHA256SUMS-linux-x86_64.txt
# macOS
shasum -a 256 -c SHA256SUMS-macos-universal.txt
```

```powershell
# Windows: compare with the hash in SHA256SUMS-windows-x86_64.txt
Get-FileHash .\morse-*-windows-x86_64.zip -Algorithm SHA256
```

On any platform, [GitHub CLI](https://cli.github.com) can confirm the
archive was built by this repository's release workflow, from a release
tag:

```bash
gh attestation verify <archive> --repo RobS96/morse-code-translator \
  --signer-workflow RobS96/morse-code-translator/.github/workflows/ci.yml
```

Without `--signer-workflow`, an attestation from any workflow in the
repository would be accepted. A release is only published from a signed
tag on a commit that is on `main`.

Things to know before running a downloaded binary:

- **macOS:** the binaries are not signed with an Apple Developer ID or
  notarised, and there is no `.app` bundle. Gatekeeper blocks them when
  they carry the quarantine flag a browser download adds; after verifying
  the archive as above, clear it with
  `xattr -d com.apple.quarantine morse morse-gui`.
- **Linux:** the binaries are built on Ubuntu 24.04 and need glibc 2.39 or
  newer. On an older distribution, build from source. `morse-gui` also needs the X11/xkbcommon, OpenGL and
  ALSA runtime libraries, which desktop installs already have.
- **Windows:** SmartScreen may warn about an unsigned download.

Alternatively, install just the CLI straight from a clone:

```bash
cargo install --locked --path morse-cli
```

## Usage — CLI

```bash
morse encode "SOS"                 # -> ... --- ...
morse decode "... --- ..."         # -> SOS
echo "SOS" | morse encode          # no text argument: read it from stdin
morse transmit "HELLO WORLD"       # flashes + beeps it live in your terminal
morse transmit "SOS" --wpm 25      # faster: 25 words-per-minute
morse transmit "SOS" -u 60         # or set the raw unit length directly (ms)
morse wav "SOS" -o sos.wav         # the same Morse as a WAV audio file
morse --help                       # every option
morse --version
```

Options may come before or after the text (`morse -a cyrillic decode ".-"`
works too), and `--name=value` is accepted for the long ones. Quote the text
so it arrives as one argument. Morse that starts with a dash (`morse decode
"-... ---"`) is read as text, not as an option. A misspelt option, an option
with its value missing, or a second piece of text is a usage error (exit
code 1).

With no text argument, the text is read from standard input, unless that is
a terminal. A terminal, or standard input with nothing on it, is the usual
`missing text` usage error. `-` is not a stand-in for standard input: it is
the Morse for T.

Characters with no Morse code are left out, and a word made up only of such
characters is left out whole, so `morse encode "A ~ B"` prints `.- / -...`.
`decode` does the same with codes it does not recognise. Each command then
names what was left out on stderr:

```
morse: warning: left out 1 character with no Morse code: '~' (U+007E)
morse: warning: left out 1 code not recognised in the latin alphabet: "..--..--"
```

A warning names the first ten distinct characters or codes and counts the
rest (`and 16 more`).

An accented Latin letter with no Morse code of its own (Ê, Ú, Č, …) is not
left out whole: its base letter is sent, and the accent is what the warning
names, as the combining mark it would be if typed separately. `morse encode
"ÊTRE"` prints `. - .-. .` and warns that `'\u{302}' (U+0302)`, the
circumflex, was left out.

None of this changes the exit code unless you pass `--strict` to `encode`,
`decode` or `wav`; the translation is printed (or the file written) either
way.

| Exit code | Meaning |
|---|---|
| 0 | Success |
| 1 | Usage error; the usage text follows the message on stderr |
| 2 | `--strict` was given and something was left out |
| 3 | Standard input could not be read (it must be UTF-8), or the output or the WAV file could not be written |

Output that is closed early, as in `morse transmit "CQ" | head -1`, ends the
program quietly with exit code 0.

Multi-word Morse uses `/` as the word separator:

```bash
morse decode ".... .. / - .... . .-. ."   # -> HI THERE
```

`decode` also reads the look-alike characters that typeset Morse and
autocorrect put in place of dots and dashes: `·` and `•` as a dot; `−`
(minus), `–`, `—` and `_` as a dash; `…` as three dots; and `|` as the word
separator. Each long dash counts as **one** dash. If your editor's "smart
dashes" turned a typed `--` into a single `—`, that information is gone and
the result will be wrong: turn smart punctuation off when typing Morse.

Invisible format characters are ignored by `decode` and `encode` alike: a
byte order mark at the start of a file or of piped input, soft hyphens,
and zero-width and text-direction marks.

**Procedural signs ("prosigns").** Common ham-radio prosigns — `<AR>`
(end of message), `<SK>` (end of contact), `<BT>` (new paragraph/break),
`<KN>` (over to a specific station), `<AS>` (wait), `<CT>` (start
copying), `<BK>`, `<SN>`, `<HH>` (error, eight dots) and `<SOS>` — encode
as a single fused character with no inter-letter gap, matching how they're
actually sent on the air:

```bash
morse encode "CQ CQ DE W1AW <KN>"
```

`<VE>`, `<KA>` and `<VA>` are accepted as other spellings of `<SN>`, `<CT>`
and `<SK>` (the same codes); `decode` writes the latter. A prosign can sit
inside a word, as in `SOS<SK>`, which is also how `decode` writes one that
follows a letter with no word gap between them.

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
morse transmit "PARIS" --wpm 20 --farnsworth-wpm 5   # 20 WPM characters, 5 WPM overall
```

`--wpm` and `--farnsworth-wpm` take a speed from 1 to 100 WPM.
`--farnsworth-wpm` may not be higher than the character speed (`--wpm`, or
the speed `-u` works out to, or the default 12 WPM); that is a usage error.
So is a raw gap unit (`-g`) shorter than the character unit, and so is
giving both options of a pair that set the same thing: `--wpm` with `-u`,
or `--farnsworth-wpm` with `-g`.

**WAV files.** `morse wav` writes the transmission as audio instead of
flashing it, with the same timing options and the same checks on them:

```bash
morse wav "PARIS PARIS" -o paris.wav --wpm 20 --farnsworth-wpm 10 --tone 700
```

The file is 16-bit mono PCM at 44100 Hz. It starts with the first tone and
ends with the last, and every tone fades in and out over 5 ms so the keying
does not click. `--tone` sets the pitch (20 to 20000 Hz, default 600) and
`--volume` the peak level (0 to 1, default 0.2).

`-o` is required and takes a file path; `-o -` is not standard output. A
file that already exists is left alone (exit code 3) unless you pass
`--force`. The audio goes to a temporary file next to the target, which is
moved into place once complete, so a run that fails leaves no partial file.
Characters with no Morse code are left out and named on stderr as for
`encode`, and `--strict` turns that into exit code 2; the file is written
either way. One file holds at most 172.8 million samples, about 65 minutes;
a longer transmission is refused (exit code 3).

## Alphabets

Besides International (Latin) Morse, the translator speaks the national
Morse alphabets for **Cyrillic** (Russian standard, plus Bulgarian Ъ),
**Ukrainian**, **Greek**, **Hebrew**, **Arabic**, **Persian**,
**Japanese** (Wabun kana) and **Korean** (Hangul jamo). Digits and
punctuation are shared; Arabic-Indic, Persian and full-width digits are
accepted too.

| | Encode (text → Morse) | Decode (Morse → text) |
|---|---|---|
| Alphabet choice | Detected from the text; override with `--alphabet` | `--alphabet`, default `latin`: the same dots and dashes mean different letters in each alphabet |
| Normalisation | Lowercase, Greek tonos and dialytika, Hebrew final letters, Ё, Ukrainian Ґ (sent as Г), Arabic ة (sent as ه), Persian ئ ؤ ة ۀ (sent as ی و ه ه), katakana, small kana, voiced kana (が → か + ゛) and Hangul syllables (한 → ㅎㅏㄴ) are all accepted | Hebrew final forms are restored at word ends and voiced kana are recomposed; Korean comes back as jamo, because regrouping jamo into syllables is ambiguous |
| Accented Latin (Ä, Ñ, Ś, …) | Encoded with their extension codes. A letter that has none (Ê, Ú, Č, …) is sent as its base letter, and its accent is reported as left out | Each extension code decodes to one letter: `.-.-` Ä, `.--.-` Å, `-.-..` Ç, `----` CH, `..--.` Ð, `..-..` É, `.-..-` È, `--.-.` Ĝ, `.---.` Ĵ, `--.--` Ñ, `---.` Ö, `...-...` Ś, `.--..` Þ, `..--` Ü, `--..-.` Ź, `--..-` Ż. Letters that share a code come back as the one listed (Æ and Ą as Ä, Ł as È). `...-.` is the prosign `<SN>`, so Ŝ does not round-trip |
| Latin letters in another alphabet | Always accepted (`QTH Москва`) | Decoded only where the alphabet has no letter of its own for the code (J, U and V in Greek; never in Cyrillic or Ukrainian, which have a letter for all 26 codes); otherwise the alphabet's letter wins |
| `×` and `%` | Per ITU-R M.1677-1, `×` is sent as X and `%` as `0/0`, joined to a number before it by a hyphen (`2%` → `2-0/0`) | Read back as sent: `X`, `2-0/0` |

Arabic and Persian share letters but not codes (خ is `---` in Arabic and
`-..-` in Persian). Text containing a Persian-only letter (پ چ ژ گ ک ی)
is detected as Persian; anything else in Arabic script is detected as
Arabic. Pass `-a persian` or `-a arabic` to be explicit.

Russian and Ukrainian share letters but not codes either: И is `..` in
Russian Morse and `-.--` in Ukrainian Morse, where `..` is І. Cyrillic text
containing a Ukrainian-only letter (і ї є ґ) and no Russian-only one
(ы э ъ ё) is detected as Ukrainian. Any other Cyrillic text is detected as
`cyrillic`, and that includes Ukrainian words spelt without those four
letters (`мир`) and text with letters of both groups. Pass `-a uk` or
`-a ru` to be explicit.

| Letter | `-a cyrillic` (`ru`, `russian`, `bg`) | `-a ukrainian` (`uk`) |
|---|---|---|
| И | `..` | `-.--` |
| І | Sent as `..`, read back as И | `..` |
| Є | Sent as `..-..`, read back as Э | `..-..` |
| Ї | `.---.` | `.---.` |
| Ґ | No code: left out and reported | Sent as Г (`--.`), read back as Г |
| Ы, Э | `-.--`, `..-..` | No code: left out and reported |
| Ъ, Ё | Sent as Ь (`-..-`) and Е (`.`). `--.--` is read back as Ъ | No code: left out and reported |

Every other letter the two alphabets share has the same code in both. Ї is
sent as `.---.`; the Ukrainian regulation table gives it the code of І
(`..`), which could not be read back as Ї.

`-a cyrillic` reads `--.--` as Ъ, the code Wikipedia's "Russian Morse code"
gives the letter, but goes on sending Ъ with the code of Ь (the Bulgarian
convention), so Ъ does not round-trip: sent, it reads back as Ь.

Kana make text Japanese. CJK punctuation on its own (an ideographic space,
`、`, `。`) does not, since other scripts use it too; pass `-a japanese` to
send such text with the Wabun codes.

```bash
morse alphabets                                        # list them
morse encode "привет"                                  # .--. .-. .. .-- . -
morse decode ".--. .-. .. .-- . -" --alphabet cyrillic # ПРИВЕТ
morse encode "привіт"                                  # .--. .-. -.-- .-- .. -
morse decode ".--. .-. -.-- .-- .. -" -a uk            # ПРИВІТ
morse encode "こんにちは"                                # ---- .-.-. -.-. ..-. -...
morse decode "---- .-.-. -.-. ..-. -..." -a japanese    # こんにちは
```

Every table is parsed from the ITU-R M.1677-1-derived tables on Wikipedia
(Korean from the Republic of Korea's radio-station operating regulation,
Ukrainian from the regulation column of the table in Ukrainian Wikipedia's
«Абетка Морзе»), and unit tests assert that no two letters in an alphabet,
and no two characters of the international table, share a code.

### Decomposed and half-width input

Text copied from file names, terminals or older systems often arrives with
letters split into a base and a combining mark, or in half-width forms. The
encoder rewrites the common cases before looking anything up
(`morse_core::normalize_input`):

| Input | Becomes |
|---|---|
| Cyrillic И + U+0306, Е + U+0308, І + U+0308 (either case) | Й, Ё, Ї |
| Kana + combining dakuten/handakuten (U+3099, U+309A) | The precomposed kana (か + U+3099 → が); where none exists, the kana followed by a spacing ゛ or ゜ |
| Half-width katakana and punctuation (U+FF61–U+FF9F) | Full-width, voiced marks combined (ｶﾞ → ガ) |
| Hangul conjoining jamo (U+1100–U+1112, U+1161–U+1175, U+11A8–U+11C2) | Compatibility jamo, double and compound jamo as their component letters |
| Invisible format characters: the zero-width non-joiner written inside Persian words (U+200C) and the rest of U+200B–U+200F, the byte order mark (U+FEFF), the soft hyphen (U+00AD), U+202A–U+202E and U+2060–U+2069 | Removed: neither letters nor word breaks |
| Typographic punctuation: `‘` `’` `ʼ`; `“` `”` `„`; `‐` `‑` `–` `—` `−`; `…`; Arabic `؟` `،` `؛` | `'`; `"`; `-`; `...`; `?` `,` `;` |
| Full-width letters, digits and punctuation (U+FF01–U+FF5E) | ASCII, for every character the encoder reads (`ＳＯＳ！` → `SOS!`). Full-width brackets `（` `）` keep their Wabun codes; a character with no code either way (`＃`) is reported as typed |

This is a hand-written subset, not full Unicode normalisation. Not covered:
other combining marks (decomposed accented Latin such as N + U+0303, Greek
tonos, Hebrew points, Arabic vowel marks), archaic jamo, ligatures and
presentation forms. With those the base letter is sent and the rest is left
out and reported. A decomposed letter is not recomposed, so N + U+0303 is
sent as N where Ñ would be `--.--`: precompose such text before encoding if
the accent matters.

Precomposed accented Latin letters with no code of their own are treated
the same way as their decomposed forms: Ê is sent as E and U+0302 is
reported, exactly as for E + U+0302. That covers the accented letters of
the Latin-1 Supplement and Latin Extended-A blocks (those of French,
Spanish, Portuguese, Czech, Polish, Turkish and so on). Anything else with
no code is left out and reported whole: a letter that is not a base letter
plus an accent, such as Œ, Ħ or Ŋ, and accented letters from further
blocks, such as Romanian Ș and Ț or Vietnamese ế.

### Codes with two readings

Within an alphabet no two letters share a code, but a letter can share one
with a punctuation mark or a prosign from the international table. On
decode **the alphabet's own letter wins**. A unit test lists every such
case, and fails if a table change adds or removes one. Today only Wabun has
any:

| Code | Decoded with `-a japanese` | Other reading (every other alphabet) |
|---|---|---|
| `-.--.` | る | `(`, `<KN>` |
| `.-...` | お | `&`, `<AS>` |
| `-.-.-` | さ | `<CT>` |
| `-...-` | め | `=`, `<BT>` |
| `-..-.` | も | `/` |
| `.-.-.` | ん | `+`, `<AR>` |
| `.-.-.-` | 、 | `.` |
| `-.--.-` | （ | `)` |
| `.-..-.` | ） | `"` |

So Japanese text containing `( & = / + . ) "` or those prosigns does not
round-trip: the code is sent correctly and read back as the kana. No letter
in any alphabet shares a code with a digit. Between punctuation and
prosigns, punctuation wins (see the prosign note above).

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
- Hit **▶ Transmit** — the lamp flashes and a tone plays in sync. **⏹ Stop**
  ends it early. The **Tone** (300 to 1200 Hz) and **Volume** sliders apply
  from the next transmission. With no sound output available, the lamp
  still flashes and the status line says so.
- Characters with no Morse code, and codes that are not recognised, are
  left out of the result and listed in the status line under the lamp.
- Pick the **Alphabet** (auto-detected by default) and the interface
  **Language**: English, Español, Français, Deutsch, Italiano, Português,
  Русский, Українська, Ελληνικά, 日本語, 한국어 or 简体中文. The language
  defaults from `LANGUAGE`/`LC_ALL`/`LC_MESSAGES`/`LANG`; Windows does not
  set these, so it starts in English there.
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
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

CI (`.github/workflows/ci.yml`) runs all three on **Ubuntu, macOS, and
Windows** for every push/PR, along with `cargo deny check` and `cargo vet
check`. It also builds the per-OS release archives on every run, and
publishes them when a release tag is pushed.

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the full workflow, and
[`CHANGELOG.md`](CHANGELOG.md) for release history.

## License

MIT — see [LICENSE](LICENSE).
