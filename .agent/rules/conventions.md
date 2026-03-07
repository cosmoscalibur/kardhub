---
trigger: always_on
---

# Coding Conventions

## Module Organization

- Each crate lives under `crates/` as a workspace member.
- `kardhub-core` is the shared library — it must remain platform-agnostic (no Dioxus, no desktop-only APIs).
- `kardhub-app` depends on `kardhub-core` for the Dioxus desktop UI.
- `kardhub-extension` and `kardhub-wasm-bridge` target `wasm32-unknown-unknown`.

## Naming

- Crate names use `kardhub-` prefix with kebab-case.
- Rust modules, functions, and variables use `snake_case`.
- Types and traits use `PascalCase`.
- Constants use `SCREAMING_SNAKE_CASE`.

## Error Handling

- Use `Result<T, E>` for fallible operations.
- Define per-crate error types; avoid `Box<dyn Error>` in public APIs.

## Testing

- Place unit tests in a `#[cfg(test)] mod tests` block at the bottom of the source file.
- Test function names should describe the scenario: `fn parses_valid_token()`.
- Run all tests with `cargo test --workspace`.

## Formatting & Linting

- Format: `cargo fmt` (config in `rustfmt.toml`).
- Lint: `cargo clippy --all-targets -- -D warnings`.
- Import sorting: handled by `cargo fmt` (default behavior).

## Dependencies

- Shared dependencies go in `[workspace.dependencies]` in the root `Cargo.toml`.
- Crate-level `Cargo.toml` files reference workspace deps with `workspace = true`.
