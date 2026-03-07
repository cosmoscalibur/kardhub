# Contributing to KardHub

Thanks for your interest in contributing!

## Setup

1. Install Rust (nightly or stable with edition 2024 support)
2. Install system dependencies for the desktop app (see [README](README.md#requirements))
3. Clone the repository and build:

```zsh
git clone https://github.com/cosmoscalibur/kardhub.git
cd kardhub
cargo build --workspace
```

## Development Workflow

1. Create a branch from `main`
2. Make your changes
3. Verify locally:

```zsh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

4. Open a pull request against `main`

## Code Style

- Format with `cargo fmt` (config in `rustfmt.toml`)
- All clippy warnings are errors (`-D warnings`)
- Place unit tests in the same file using `#[cfg(test)]` modules
- Keep `kardhub-core` platform-agnostic — no desktop or browser dependencies

## Pre-commit Hooks (Optional)

```zsh
pip install pre-commit
pre-commit install
```

This runs format, clippy, and test checks before each commit.
