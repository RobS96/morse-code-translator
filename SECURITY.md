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
plays local audio/visual output. It makes no network calls and writes no
files. The CLI reads no files. The GUI reads font files from a fixed list
of operating-system font paths at startup (see `morse-gui/src/fonts.rs`)
and hands them to the font renderer, and it reads the `LANGUAGE`,
`LC_ALL`, `LC_MESSAGES` and `LANG` environment variables to pick the
interface language. It never opens a path taken from user input.

It's hardened as follows:

- `#![forbid(unsafe_code)]` on all three crates. This covers this
  project's own code, not its dependencies.
- The CLI validates its numeric options (`--wpm`, `--farnsworth-wpm`,
  `-u`, `-g`): a non-numeric, zero, negative or out-of-range value, an
  option with its value missing, an unknown alphabet, and a
  `--farnsworth-wpm` above the character speed are usage errors with a
  non-zero exit. Options the chosen command does not use (for example
  `--wpm` with `encode`) are accepted and ignored without being checked.
- The GUI takes speeds from bounded sliders, and `morse-core` clamps every
  unit length it derives from a WPM value, so a zero/negative/absurd value
  cannot become an effectively-infinite sleep or an integer overflow.
- Input characters with no Morse code are left out of the output. The CLI
  names them in a warning on stderr; the GUI does not yet show which were
  left out.
- Release builds enable integer overflow checks.
- `cargo deny check` (advisory DB, license allowlist, banned/duplicate
  deps, source registries — see `deny.toml`) and `cargo vet check` run in
  CI on every push/PR and gate releases.
- Release archives are published with SHA-256 checksum files and GitHub
  build provenance attestations; verify a download with
  `gh attestation verify <file> --repo RobS96/morse-code-translator`.

If you still find a security issue:

1. **Do not** open a public issue.
2. Use GitHub's **"Report a vulnerability"** button under the Security tab
   (private security advisory), or contact the maintainer directly via
   their GitHub profile.
3. Include steps to reproduce and the potential impact.

You should get an acknowledgement within a few days. Once a fix is ready,
it will be released and credited in `CHANGELOG.md` (unless you'd prefer to
remain anonymous).
