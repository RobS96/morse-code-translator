# Security Policy

## Supported Versions

This project is pre-1.0; only the latest release on `main` is supported
with security fixes.

| Version | Supported |
| ------- | --------- |
| latest  | ✅        |
| older   | ❌        |

## Reporting a Vulnerability

This tool has a small attack surface (it reads text/Morse strings and
plays local audio/visual output — it makes no network calls and reads no
files by default). It's hardened as follows:

- `#![forbid(unsafe_code)]` on all three crates.
- All CLI/GUI-facing numeric inputs (WPM, raw unit-ms) are validated and
  clamped, so a zero/negative/absurd value is a usage error rather than an
  effectively-infinite hang or an integer-overflow panic.
- `cargo deny check` (advisory DB, license allowlist, banned/duplicate
  deps, source registries — see `deny.toml`) runs in CI on every push/PR
  and gates releases.

If you still find a security issue:

1. **Do not** open a public issue.
2. Use GitHub's **"Report a vulnerability"** button under the Security tab
   (private security advisory), or contact the maintainer directly via
   their GitHub profile.
3. Include steps to reproduce and the potential impact.

You should get an acknowledgement within a few days. Once a fix is ready,
it will be released and credited in `CHANGELOG.md` (unless you'd prefer to
remain anonymous).
