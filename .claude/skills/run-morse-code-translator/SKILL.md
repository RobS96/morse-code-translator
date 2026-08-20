---
name: run-morse-code-translator
description: Build, run, and drive the morse-code-translator Rust workspace (morse-core library, morse-cli, morse-gui) — use when asked to run/build/test/screenshot the morse translator, verify an encode/decode change, or exercise the CLI or GUI directly.
---

Paths below are relative to `~/dev/morse-code-translator/` (the repo root). The
driver is `.claude/skills/run-morse-code-translator/driver.sh`.

This is a 3-crate Cargo workspace: `morse-core` (pure library, no binary),
`morse-cli` (binary name `morse`), `morse-gui` (binary name `morse-gui`, a real
native `eframe`/`egui` window — no Electron, no webview). All three share one
`cargo build --workspace`.

## Prerequisites

- Rust toolchain (already present via rustup on this host).
- `cliclick` for GUI interaction only (`brew install cliclick`) — not needed
  for build/CLI/library verification.
- This host's `~/.cargo/config.toml` redirects `target-dir` to
  `/private/tmp/cargo-target` — Claude Code's sandbox blocks writes there, so
  every `cargo`/`rustc` command needs the sandbox override
  (`dangerouslyDisableSandbox: true`, or run unsandboxed).

## Build

```bash
zsh .claude/skills/run-morse-code-translator/driver.sh build
```

Builds all three crates. ~2 min cold, near-instant if `/private/tmp` wasn't wiped.

## Run (agent path) — driver.sh

```bash
zsh .claude/skills/run-morse-code-translator/driver.sh smoke-core   # library
zsh .claude/skills/run-morse-code-translator/driver.sh smoke-cli    # CLI
zsh .claude/skills/run-morse-code-translator/driver.sh smoke-gui    # GUI, screenshots default state
zsh .claude/skills/run-morse-code-translator/driver.sh gui-interact # optional: types into the GUI, screenshots each step
zsh .claude/skills/run-morse-code-translator/driver.sh test         # cargo test --workspace
zsh .claude/skills/run-morse-code-translator/driver.sh all          # build + smoke-core + smoke-cli + smoke-gui + test
```

Screenshots land in `/tmp/morse-shots/` (override with `SHOT_DIR=...`).

### `morse-core` (library)

No binary — `smoke-core` compiles a throwaway program against the built rlib
and runs it directly:

```
morse-core smoke OK: SOS -> ... --- ... -> SOS, plan len 2
```

Public API used: `morse_core::encode`, `decode`, `build_signal_plan`.

### `morse-cli` (binary `morse`)

Real flags, confirmed by reading `morse-cli/src/main.rs` (there is no
`--help`; wrong/missing args print usage to stderr and exit 1):

```bash
/private/tmp/cargo-target/debug/morse encode "SOS"              # ... --- ...
/private/tmp/cargo-target/debug/morse decode "... --- ..."      # SOS
/private/tmp/cargo-target/debug/morse transmit "HELLO" -u 80    # flashes/beeps in the terminal, blocks until done
```

### `morse-gui` (binary `morse-gui`)

Real native window (`eframe`, 480×420 default size), driven with
`osascript`/System Events + `cliclick` + `screencapture`. `smoke-gui` launches
it, repositions the window to logical `(60, 60)` (see Gotchas — this step is
not optional), and screenshots the default state (input `SOS`, result
`... --- ...`). Verified output: `/tmp/morse-shots/gui_default.png` shows the
window with the "Text -> Morse" tab active, `SOS` → `... --- ...`.

`gui-interact` then drives it further — confirmed this session:
- Typing `HELLO` into the Text field → Result updates to
  `.... . .-.. .-.. ---` (`/tmp/morse-shots/gui_encode_hello.png`).
- Clicking the "Morse -> Text" tab, typing `.... . .-.. .-.. ---` → Result
  shows `HELLO` (`/tmp/morse-shots/gui_decode_hello.png`).

The "Transmit (flash + beep)" button and lamp indicator were **not**
screenshot-verified this session (a concurrent process on the shared display
stole window focus mid-attempt — see Gotchas). The button is at approximately
window-origin + `(87, 260)`; confirm visually before relying on it.

## Test

```bash
zsh .claude/skills/run-morse-code-translator/driver.sh test
```

24 tests, all in `morse-core` (`morse-cli`/`morse-gui` have none of their own):
encode/decode round trips, multi-word handling, unknown-char dropping, signal
timing ratios. All pass.

## Gotchas

- **`cargo build` fails with `Operation not permitted` on
  `/private/tmp/cargo-target/debug`** under the default sandbox — this repo's
  global cargo config redirects the target dir there and it's outside the
  sandbox's allowed write paths. Every build/test/rustc command needs the
  sandbox override.
- **A freshly launched `morse-gui` window can render at an off-screen
  position** (observed once at logical position `(720, -900)` — likely a
  restored/garbage window position from `eframe`'s persistence). Always
  reposition it explicitly before screenshotting:
  `osascript -e 'tell application "System Events" to tell process "morse-gui" to set position of window "Morse Code Translator" to {60, 60}'`.
  `smoke-gui` does this automatically.
- **`cliclick`/System Events coordinates are logical points, not screenshot
  pixels.** This host captures screenshots at 2x (retina) and the image you
  view may be further downscaled for display — do not click at coordinates
  read directly off a displayed screenshot. Instead: position the window at a
  known logical origin (`smoke-gui` uses `(60, 60)`), then click at
  `window_origin + offset`. The offsets baked into `gui-interact` were
  cross-validated from two independent runs (agreed within ~2pt):
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
- **Shared-display collision with other concurrent agents/sessions.** This
  host may have other automated sessions driving GUI apps at the same time.
  During this skill's authoring, a concurrent process (unrelated to this
  repo) repeatedly stole window focus — once fully replacing the frontmost
  app with System Settings mid-interaction. Symptoms: a click/keystroke you
  sent lands somewhere unexpected, or a screenshot shows a window you didn't
  launch. Mitigation: re-run
  `osascript -e 'tell application "System Events" to set frontmost of process "morse-gui" to true'`
  immediately before each interaction, verify the frontmost app in the next
  screenshot before trusting it, and if something else has taken over, back
  off rather than keep clicking blind (especially near System Settings —
  don't risk toggling a privacy/security setting by accident).

## Troubleshooting

- **Window not found / `Can't get process "morse-gui"` from `osascript`** —
  the process died or was killed by something else (e.g. a stray keystroke
  sent to the wrong app during a focus-steal, see Gotchas above). Check
  `ps -eo pid,command | awk '/morse-gui/ && !/awk/'`; if empty, just relaunch
  via `smoke-gui`.
- **GUI field doesn't update after `cliclick t:"..."`** — almost always a
  coordinate miss (clicked outside the field, or the window moved/was never
  repositioned). Re-run `smoke-gui` to reposition, then retry with the
  offsets above.
