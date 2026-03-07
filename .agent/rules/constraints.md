---
trigger: always_on
---

# Architectural Constraints

## Platform Boundaries

- `kardhub-core` MUST NOT depend on platform-specific crates (Dioxus, wasm-bindgen runtime, desktop file I/O).
  - Use feature flags or conditional compilation (`#[cfg(target_arch = "wasm32")]`) for platform-specific API clients (reqwest vs gloo-net).
- `kardhub-app` is desktop-only (Dioxus desktop). Do not add web or mobile targets here.
- `kardhub-extension` and `kardhub-wasm-bridge` target wasm only.

## Things to Avoid

- Do not add `unsafe` code without explicit justification.
- Do not introduce new direct dependencies without adding them to `[workspace.dependencies]`.
- Do not store secrets (GitHub PAT) in source code, logs, or error messages.
- Do not bypass the cache layer — all GitHub data access should go through the cache module.

## Migration Notes

- There are no deprecated modules at this time.
