# Architecture Overview

## Workspace Structure

```
kardhub/
├── crates/
│   ├── kardhub-core/        # Platform-agnostic shared library
│   ├── kardhub-app/         # Dioxus desktop application
│   ├── kardhub-extension/   # Browser extension (Chromium + Firefox, MV3)
│   └── kardhub-wasm-bridge/ # WASM bridge for extension ↔ core
├── docs/architecture/       # This directory
├── Cargo.toml               # Workspace root
└── rustfmt.toml
```

## Crate Responsibilities

### kardhub-core

The shared, platform-agnostic library containing:

- **Domain models** — `Issue`, `PullRequest`, `Card`, `Column`, `Source`, etc.
- **Card-to-column mapping engine** — rules that sort cards into Kanban columns based on labels, reviews, CI status, and branch state.
- **GitHub REST API client** — uses `reqwest` (native) or `gloo-net` (wasm) depending on target.
- **Authentication helpers** — PAT validation and OAuth utilities.
- **Markdown rendering** — converts GitHub-flavored markdown to HTML via `pulldown-cmark`.
- **Issue linking** — parses `Issue: owner/repo#N` syntax for linking PRs to issues.

### kardhub-app

Dioxus 0.7 desktop application providing:

- Sidebar with source selection (personal / organization) and repository picker.
- Kanban board rendering with drag-free column layout.
- Card detail panel (slide-open) with full metadata.
- Settings panel for default repos, cache status, and manual refresh.
- Local JSON cache under `$XDG_CONFIG_HOME/kardhub/cache/`.
- Dark theme.

### kardhub-extension

Browser extension (Chromium + Firefox, Manifest V3):

- Content script injecting a KardHub tab into GitHub repo pages.
- Background worker for API calls and cache management.
- Popup for PAT configuration.
- Floating "Link Issues" button on PR pages.

### kardhub-wasm-bridge

Thin WASM layer exposing `kardhub-core` functions to the browser extension via `wasm-bindgen`.

## Data Flow

1. User authenticates with a GitHub Personal Access Token.
2. `kardhub-core` fetches organizations, repositories, issues, and PRs via the GitHub REST API.
3. Data is cached locally as JSON files with per-type TTLs.
4. The mapping engine assigns each issue/PR to a Kanban column.
5. The UI (desktop or extension) renders the board from the mapped cards.
6. Incremental sync fetches only items updated since the last `synced_at` timestamp.
