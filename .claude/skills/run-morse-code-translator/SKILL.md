---
name: run-morse-code-translator
description: Build, run, and drive the morse-code-translator Rust workspace (morse-core library, morse-cli, morse-gui) — use when asked to run/build/test/screenshot the morse translator, verify an encode/decode change, or exercise the CLI or GUI directly.
---

Paths below are relative to the repo root. The driver is
`.claude/skills/run-morse-code-translator/driver.sh`; it finds the repo root
from its own location.

This is a 3-crate Cargo workspace: `morse-core` (pure library, no binary),
`morse-cli` (binary name `morse`), `morse-gui` (binary name `morse-gui`, a real
native `eframe`/`egui` window — no Electron, no webview). All three share one
`cargo build --workspace`.

## Prerequisites

- Rust toolchain (stable, via rustup). Minimum versions are declared as
  `rust-version` in the manifests.
- The GUI steps (`smoke-gui`, `gui-interact`) are macOS-only: they use
  `osascript`, `screencapture` and `cliclick` (`brew install cliclick`).
  Build, test and the library/CLI steps work on any platform with zsh.
- Build output goes to `target/` in the repo, or to `CARGO_TARGET_DIR` if
  that is set; the driver looks for binaries in the same place. If the
  default location is not writable where you are running, set
  `CARGO_TARGET_DIR` to a directory that is.

## Build

```bash
zsh .claude/skills/run-morse-code-translator/driver.sh build
```

Builds all three crates. ~2 min cold, near-instant when the target directory
is warm.

## Run (agent path) — driver.sh

```bash
zsh .claude/skills/run-morse-code-translator/driver.sh smoke-core   # library
zsh .claude/skills/run-morse-code-translator/driver.sh smoke-cli    # CLI
zsh .claude/skills/run-morse-code-translator/driver.sh smoke-gui    # GUI, screenshots default state
zsh .claude/skills/run-morse-code-translator/driver.sh gui-interact # optional: types into the GUI, screenshots each step
zsh .claude/skills/run-morse-code-translator/driver.sh test         # cargo test --workspace
zsh .claude/skills/run-morse-code-translator/driver.sh all          # build + smoke-core + smoke-cli + smoke-gui + test
```

Screenshots and scratch files land in `$TMPDIR/morse-shots/` (override with
`SHOT_DIR=...`), written below as `$SHOT_DIR`.

### `morse-core` (library)

No binary — `smoke-core` compiles a throwaway program against the built rlib
and runs it directly:

```
morse-core smoke OK: SOS -> ... --- ... -> SOS, plan len 2
```

Public API used: `morse_core::encode`, `decode`, `build_signal_plan`. Also
public: `encode_in` / `decode_in` / `build_signal_plan_in` (explicit
alphabet), `encode_lossy_report` (output plus the characters that were
dropped), `normalize_input`, `build_schedule` (a plan laid out as timed
tones and silences), `render_samples` (the schedule as PCM samples) and
`write_wav`.

### `morse-cli` (binary `morse`)

`--help` prints the full option list. Wrong or missing arguments print the
usage to stderr and exit 1. Options may come before or after the text.

```bash
target/debug/morse encode "SOS"              # ... --- ...
target/debug/morse decode "... --- ..."      # SOS
target/debug/morse decode ".-" -a cyrillic   # А
target/debug/morse transmit "HELLO" -u 80    # flashes/beeps in the terminal, blocks until done
target/debug/morse wav "HELLO" -o "$SHOT_DIR/hello.wav" --force   # 16-bit mono WAV; without --force an existing file is exit 3
target/debug/morse encode "A ~ B"            # .- / -... on stdout, a warning naming ~ on stderr
```

### `morse-gui` (binary `morse-gui`)

Real native window (`eframe`, 520×720 default size), driven with
`osascript`/System Events + `cliclick` + `screencapture`. `smoke-gui` launches
it, repositions the window to logical `(60, 60)` (see Gotchas — this step is
not optional), and screenshots the default state (input `SOS`, result
`... --- ...`). Expected output: `$SHOT_DIR/gui_default.png` shows the
window with the "Text -> Morse" tab active, `SOS` → `... --- ...`.

`gui-interact` then drives it further:
- Typing `HELLO` into the Text field → Result updates to
  `.... . .-.. .-.. ---` (`$SHOT_DIR/gui_encode_hello.png`).
- Clicking the "Morse -> Text" tab, typing `.... . .-.. .-.. ---` → Result
  shows `HELLO` (`$SHOT_DIR/gui_decode_hello.png`).

The click offsets were measured against an earlier, smaller window layout
and have not been re-measured since the GUI gained its language and
alphabet pickers. Check each screenshot before trusting a click, and
re-measure the offsets if a click misses. The Transmit button and lamp have
no verified offset.

## Test

```bash
zsh .claude/skills/run-morse-code-translator/driver.sh test
```

Unit tests live in all three crates: `morse-core` (encode/decode round trips
per alphabet, input normalisation, dropped-character reporting, code
collisions, signal timing, the keying schedule, audio rendering and the WAV
writer), `morse-cli` (argument parsing and validation, plus end-to-end runs
of the binary in `morse-cli/tests/`) and `morse-gui` (interface
translations, and the transmission logic that needs no window or sound
card). `cargo test --workspace` prints one `test result:` line per test
binary; all should read `0 failed`.

## Gotchas

- **`cargo build` fails with a permission error on the target directory** —
  the location cargo is configured to build into is not writable from where
  you are running. Point `CARGO_TARGET_DIR` at a writable directory; the
  driver picks binaries up from there.
- **A freshly launched `morse-gui` window can render at an off-screen
  position** (observed once at logical position `(720, -900)` — likely a
  restored/garbage window position from `eframe`'s persistence). Always
  reposition it explicitly before screenshotting:
  `osascript -e 'tell application "System Events" to tell process "morse-gui" to set position of window "Morse Code Translator" to {60, 60}'`.
  `smoke-gui` does this automatically.
- **`cliclick`/System Events coordinates are logical points, not screenshot
  pixels.** Retina displays capture screenshots at 2x and the image you
  view may be further downscaled for display — do not click at coordinates
  read directly off a displayed screenshot. Instead: position the window at a
  known logical origin (`smoke-gui` uses `(60, 60)`), then click at
  `window_origin + offset`. The offsets baked into `gui-interact`:
  input field ≈ `origin + (151, 127)`, "Morse -> Text" tab ≈
  `origin + (147, 82)`, "Text -> Morse" tab ≈ `origin + (52, 82)`.
- **`cliclick`'s `kd:cmd t:a ku:cmd` does NOT perform Cmd+A select-all** — `t:`
  types literal characters, it doesn't combine with a held modifier the way
  you'd expect. Use `tc:x,y` (triple-click) to select-all in an egui text
  field instead, then `t:"new text"` to replace it.
- **egui's AccessKit integration does not expose the app's real widgets to
  System Events** — `get entire contents` of the window only shows the 3
  traffic-light buttons and the title static text, not the tabs/fields. Pixel
  (offset-based) clicking is required; there's no accessible-element path.
- **Another application can take window focus mid-run.** Symptoms: a
  click/keystroke you sent lands somewhere unexpected, or a screenshot shows
  a window you didn't launch. Mitigation: re-run
  `osascript -e 'tell application "System Events" to set frontmost of process "morse-gui" to true'`
  immediately before each interaction, verify the frontmost app in the next
  screenshot before trusting it, and if something else has taken over, stop
  rather than keep clicking blind.

## Troubleshooting

- **Window not found / `Can't get process "morse-gui"` from `osascript`** —
  the process has exited (e.g. a stray keystroke sent to the wrong app
  after a focus change, see Gotchas above). Check
  `ps -eo pid,command | awk '/morse-gui/ && !/awk/'`; if empty, just relaunch
  via `smoke-gui`.
- **GUI field doesn't update after `cliclick t:"..."`** — almost always a
  coordinate miss (clicked outside the field, or the window moved/was never
  repositioned). Re-run `smoke-gui` to reposition, then retry with the
  offsets above.
