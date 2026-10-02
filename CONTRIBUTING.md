# Contributing

Thanks for considering a contribution! This project is a Cargo workspace,
so the usual Rust workflow applies.

## Project layout

```
morse-core/   pure encode/decode/timing logic — no I/O, fully unit tested
morse-cli/    terminal UI (encode / decode / transmit / wav)
morse-gui/    cross-platform desktop GUI (eframe + rodio)
```

If you're changing core Morse logic (tables, timing ratios), it belongs in
`morse-core`, so both the CLI and GUI pick it up for free.

## Setup

```bash
git clone https://github.com/RobS96/morse-code-translator.git && cd morse-code-translator
```

**Linux only** — install GUI/audio headers before building `morse-gui`:

```bash
sudo apt-get update && sudo apt-get install -y libx11-dev libxkbcommon-dev libxkbcommon-x11-dev libgl1-mesa-dev libasound2-dev pkg-config
```

macOS and Windows need no extra system packages.

## Before opening a PR

Run the checks CI runs on your code, in one line:

```bash
cargo fmt --all --check && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo test --locked --workspace
```

CI also runs these, which matter mostly when dependencies change:

- **`cargo deny check`** and **`cargo vet check --locked`** (install with
  `cargo install --locked cargo-deny cargo-vet`). A new or updated
  dependency fails `cargo vet` until it is covered by an audit or an
  exemption in `supply-chain/`; say in the PR why the dependency is needed
  and a maintainer will handle that part.
- **Minimum supported Rust**: `morse-core` and `morse-cli` must build on
  Rust 1.88, `morse-gui` on 1.95. Avoid newer language or standard-library
  features, or raise `rust-version` in the same PR and say why.
- **Packaging**: the release archives are built on every PR, so a change
  that breaks them shows up there.

- **New behavior** → add a unit test in `morse-core/src/lib.rs` (see the
  `#[cfg(test)] mod tests` block for the pattern).
- **Bug fix** → add a regression test that fails before your fix and
  passes after.
- **Public API change** → update doc comments (`///`) and `README.md`.
- **New GUI text** → a new message in `morse-gui/src/i18n.rs` needs a
  string in each of the 12 languages; the crate does not compile without
  them.

## Commit / PR conventions

- Keep commits focused; one logical change per commit.
- PR description should say *what* changed and *why*, and link any related
  issue.
- Update `CHANGELOG.md` under `[Unreleased]` for any user-facing change.

## Releasing (maintainer)

1. In one PR: set the new version in `Cargo.toml` (`[workspace.package]`)
   and in the `morse-core` dependency lines of `morse-cli/Cargo.toml` and
   `morse-gui/Cargo.toml`, run `cargo build` so `Cargo.lock` follows, and
   rename the `[Unreleased]` heading in `CHANGELOG.md` to
   `[x.y.z] - YYYY-MM-DD`.
2. After it is merged and `main` is green, push a signed annotated tag
   `vx.y.z` on that merge commit.
3. The tag run builds the archives, checks that the tag, the crate version,
   the version the binary prints and the changelog heading agree, checks
   that the tag is signed and on `main`, and then publishes.

Releases are immutable and release tags cannot be moved or deleted, so a
mistake is fixed by the next version. The checks in step 3 run before
anything is public; a tag that fails them publishes nothing.

## Reporting bugs / requesting features

Use the issue templates under **Issues → New Issue** — they prompt for the
info needed to reproduce or evaluate the request.

## Code style

- Standard `rustfmt` defaults (no custom `rustfmt.toml`).
- `clippy` clean with `-D warnings`.
- Prefer small, pure functions in `morse-core` over logic embedded in the
  CLI/GUI, so it stays testable without a terminal or display.
