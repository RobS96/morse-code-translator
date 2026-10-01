#!/bin/zsh
# Driver for run-morse-code-translator.
#
# Usage: driver.sh <build|smoke-core|smoke-cli|smoke-gui|gui-interact|test|all>
#
# Environment:
#   REPO              repo root (default: found from this script's location)
#   CARGO_TARGET_DIR  where cargo builds (default: $REPO/target)
#   SHOT_DIR          screenshots and scratch files (default: $TMPDIR/morse-shots)
set -uo pipefail

REPO="${REPO:-${0:A:h:h:h:h}}"
TARGET_DIR="${TARGET_DIR:-${CARGO_TARGET_DIR:-$REPO/target}}"
SHOT_DIR="${SHOT_DIR:-${TMPDIR:-/tmp}/morse-shots}"
mkdir -p "$SHOT_DIR"

cmd_build() {
    cd "$REPO" && cargo build --workspace
}

cmd_test() {
    cd "$REPO" && cargo test --workspace
}

# Library / SDK path: morse-core has no binary of its own. Compile a tiny
# throwaway program against the already-built rlib and run it -- this is
# the "import and call" smoke test for a pure library crate.
cmd_smoke_core() {
    cd "$REPO"
    local src="$SHOT_DIR/lib_smoke.rs"
    cat > "$src" <<'RUST'
fn main() {
    let msg = "SOS";
    let morse = morse_core::encode(msg);
    assert_eq!(morse, "... --- ...");
    let back = morse_core::decode(&morse);
    assert_eq!(back, msg);
    let plan = morse_core::build_signal_plan("E");
    assert_eq!(plan.len(), 1);
    println!("morse-core smoke OK: {msg} -> {morse} -> {back}, plan len {}", plan.len());
}
RUST
    rustc --edition 2024 \
        -L "$TARGET_DIR/debug/deps" \
        --extern morse_core="$TARGET_DIR/debug/libmorse_core.rlib" \
        "$src" -o "$SHOT_DIR/lib_smoke" \
        && "$SHOT_DIR/lib_smoke"
}

cmd_smoke_cli() {
    local bin="$TARGET_DIR/debug/morse"
    echo "=== encode ===";       "$bin" encode "SOS"
    echo "=== decode ===";       "$bin" decode "... --- ..."
    echo "=== multi-word round trip ==="
    local enc; enc=$("$bin" encode "RUST LANG 2026")
    echo "$enc"
    "$bin" decode "$enc"
    echo "=== no-args usage (expect exit 1) ==="
    "$bin"; echo "exit=$?"
}

# GUI path: launch the real eframe/egui window, park it at a known logical
# position, and screenshot the default state. See SKILL.md Gotchas for why
# the window must be repositioned before anything else, and for the
# offset-from-window-origin coordinates used by gui-interact.
cmd_smoke_gui() {
    local bin="$TARGET_DIR/debug/morse-gui"
    "$bin" > "$SHOT_DIR/gui_stdout.log" 2>"$SHOT_DIR/gui_stderr.log" &
    local pid=$!
    sleep 2
    osascript -e 'tell application "System Events" to set frontmost of process "morse-gui" to true'
    sleep 0.3
    osascript -e 'tell application "System Events" to tell process "morse-gui" to set position of window "Morse Code Translator" to {60, 60}'
    sleep 0.3
    screencapture -x "$SHOT_DIR/gui_default.png"
    echo "pid=$pid screenshot=$SHOT_DIR/gui_default.png"
}

# Optional interactive pass -- requires cliclick (brew install cliclick)
# and the window already positioned at logical (60,60) by smoke-gui.
# Offsets below are relative to the window's top-left (60,60). They were
# measured against an earlier window layout; check the screenshots and
# re-measure if a click misses.
cmd_gui_interact() {
    command -v cliclick >/dev/null || { echo "cliclick not found -- brew install cliclick" >&2; return 1; }
    osascript -e 'tell application "System Events" to set frontmost of process "morse-gui" to true'
    sleep 0.3
    # Field (shared by both tabs): window_origin + (151,127)
    cliclick tc:211,187
    cliclick t:"HELLO"
    sleep 0.3
    screencapture -x "$SHOT_DIR/gui_encode_hello.png"

    # Decode tab: window_origin + (147,82)
    cliclick c:207,142
    sleep 0.2
    cliclick tc:211,187
    cliclick t:".... . .-.. .-.. ---"
    sleep 0.3
    screencapture -x "$SHOT_DIR/gui_decode_hello.png"
    echo "screenshots: $SHOT_DIR/gui_encode_hello.png $SHOT_DIR/gui_decode_hello.png"
}

case "${1:-}" in
    build)        cmd_build ;;
    test)         cmd_test ;;
    smoke-core)   cmd_smoke_core ;;
    smoke-cli)    cmd_smoke_cli ;;
    smoke-gui)    cmd_smoke_gui ;;
    gui-interact) cmd_gui_interact ;;
    all)          cmd_build && cmd_smoke_core && cmd_smoke_cli && cmd_smoke_gui && cmd_test ;;
    *)
        echo "Usage: $0 <build|smoke-core|smoke-cli|smoke-gui|gui-interact|test|all>" >&2
        exit 1
        ;;
esac
