# Security Policy

## Supported Versions

This project is pre-1.0; only the latest release on `main` is supported
with security fixes.

| Version | Supported |
| ------- | --------- |
| latest  | ✅        |
| older   | ❌        |

## Reporting a Vulnerability

This tool has a small attack surface: it reads text/Morse strings and
plays local audio/visual output. It makes no network calls. The CLI reads
no files, and writes one only for `morse wav`: the path given to `-o`, by
way of a temporary file next to it (`.<name>.<pid>.tmp`, created only if
no file of that name exists) that is moved into place once complete. An
existing file at that path is replaced only with `--force`. The GUI writes
no files. It reads font files from a fixed list of operating-system font
paths at startup (see `morse-gui/src/fonts.rs`) and hands them to the font
renderer, and it reads the `LANGUAGE`, `LC_ALL`, `LC_MESSAGES` and `LANG`
environment variables to pick the interface language. The GUI never opens
a path taken from user input.

It's hardened as follows:

- `#![forbid(unsafe_code)]` on all three crates. This covers this
  project's own code, not its dependencies.
- The CLI validates its numeric options (`--wpm`, `--farnsworth-wpm`,
  `-u`, `-g`, and for `wav` `--tone` and `--volume`): a non-numeric, zero,
  negative, non-finite or out-of-range value, an option with its value
  missing, an unknown alphabet, and a `--farnsworth-wpm` above the
  character speed are usage errors with a non-zero exit. Options the
  chosen command does not use (for example `--wpm` with `encode`) are
  accepted and ignored without being checked, except `-o` and `--force`,
  which are usage errors outside `wav`.
- The GUI takes speeds, tone and volume from bounded sliders, and
  `morse-core` clamps every unit length it derives from a WPM value, so a
  zero/negative/absurd value cannot become an effectively-infinite sleep
  or an integer overflow.
- Audio is rendered into memory before it is played or written, and one
  rendering is capped at 172.8 million samples (an hour at 48 kHz, about
  690 MB). A longer transmission is an error from `morse wav`, and is
  shown by the GUI with the lamp alone; neither tries to allocate more.
- Input characters with no Morse code are left out of the output. The CLI
  names them in a warning on stderr; the GUI lists them in its status
  line.
- Release builds enable integer overflow checks.
- `cargo deny check` (advisory DB, license allowlist, banned/duplicate
  deps, source registries — see `deny.toml`) and `cargo vet check` run in
  CI on every push/PR and gate releases.
- Release archives are published with SHA-256 checksum files and GitHub
  build provenance attestations; verify a download with
  `gh attestation verify <file> --repo RobS96/morse-code-translator
  --signer-workflow RobS96/morse-code-translator/.github/workflows/ci.yml`.
- A release is published only from a signed, annotated tag that GitHub
  verifies, on a commit that is already on `main`; the workflow checks
  both before it publishes, and that the tag matches the version the
  binaries report.

If you still find a security issue:

1. **Do not** open a public issue.
2. Use GitHub's **"Report a vulnerability"** button under the Security tab
   (private security advisory), or contact the maintainer directly via
   their GitHub profile.
3. Include steps to reproduce and the potential impact.

You should get an acknowledgement within a few days. Once a fix is ready,
it will be released and credited in `CHANGELOG.md` (unless you'd prefer to
remain anonymous).
