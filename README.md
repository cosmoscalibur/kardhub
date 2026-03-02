# KardHub

A Kanban board synced with GitHub issues and pull requests, built entirely in Rust. Available as a desktop app and a browser extension.

## Features

- **GitHub sync** — authenticates via Personal Access Token and pulls issues + PRs from any repo you own or belong to
- **Smart column mapping** — cards auto-sort into Kanban columns based on labels, reviews, CI status, and branch state
- **Local cache** — per-type TTLs (orgs 6mo, repos 1mo, open items 3h); incremental sync fetches only updated items after first load; closed issues and merged/closed PRs cached cumulatively
- **Session persistence** — token, last source, and selected repos saved; auto-login on relaunch
- **Settings** — configure a default repository per source (personal / organization), view cache status, and force per-type refresh
- **Dark & light themes** — toggle between a dark theme and a high-contrast light theme
- **Card detail panel** — click any card to slide open a detail panel with full metadata (status, labels, priority, assignees, description)

### Kanban Columns

| Column | Trigger |
|---|---|
| 🧊 Icebox | Priority `#6` |
| ⏳ Prebacklog | Priority `#4`, `#5`, or no priority label |
| 📥 Backlog | Priority `#1`–`#3` |
| ❌ Failed | `QA-Failed` label |
| 🚧 In Progress | Open PR (default) |
| 👀 Code review | PR with reviewers, passing CI |
| ⏳ QA Backlog | PR with N approvals, passing CI |
| 🔍 QA Review | `QA` label on PR |
| ☑️ Ready for STG | `QA-OK` label, or QA user approved |
| ✅ Ready for deploy | PR merged to default branch |
| 📦 In Release | PR branch is `release/*` or `release-*` |
| 🚪 Closed | Closed issues, closed-not-merged PRs |

## Architecture

```
kardhub/
├── crates/
│   ├── kardhub-core/     # Platform-agnostic: models, mapping engine, GitHub REST client
│   ├── kardhub-app/      # Dioxus desktop application
│   └── kardhub-extension/# Browser extension (scaffold)
├── Cargo.toml            # Workspace root
└── rustfmt.toml
```

| Crate | Purpose |
|---|---|
| `kardhub-core` | Domain models, card-to-column mapping, GitHub API client (`reqwest` native / `gloo-net` wasm), OAuth helpers |
| `kardhub-app` | Dioxus 0.7 desktop UI — sidebar, board, card detail, settings, local JSON cache |
| `kardhub-extension` | Browser extension (Chromium + Firefox, MV3) — content script, background worker, popup |

## Requirements

- **Rust** nightly or stable with edition 2024 support
- **Desktop app** system dependencies (WebView):
  - **Debian/Ubuntu**: `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev libxdo-dev`
  - **Arch/Manjaro**: `sudo pacman -S webkit2gtk-4.1 gtk3 libayatana-appindicator xdotool`
  - **macOS / Windows**: no extra dependencies needed
- **Browser extension** build dependencies:
  - `rustup target add wasm32-unknown-unknown`
  - `cargo install wasm-pack`

## Quick Start

```zsh
# Clone
git clone https://github.com/cosmoscalibur/kardhub.git
cd kardhub

# Run the desktop app
cargo run -p kardhub-app

# Run tests
cargo test -p kardhub-core

# Lint
cargo clippy --all-targets -- -D warnings
```

On first launch, enter a [GitHub Personal Access Token](https://github.com/settings/tokens) with `repo` and `read:org` scopes.

## Browser Extension

```zsh
# Build the extension
zsh crates/kardhub-extension/build.sh
```

The packaged extension is output to `crates/kardhub-extension/dist/`.

**Chrome / Chromium:**

1. Open `chrome://extensions`
2. Enable **Developer mode**
3. Click **Load unpacked** → select the `dist/` directory

**Firefox:**

1. Open `about:debugging#/runtime/this-firefox`
2. Click **Load Temporary Add-on** → select `dist/manifest.json`

After loading, click the KardHub icon in the toolbar to configure your GitHub PAT. On any GitHub repository page, a **KardHub** tab appears in the repo navigation to show the Kanban board. On pull request pages, a floating **🃏 Link Issues** button lets you search and link issues using `Issue: owner/repo#N` syntax.

## Local Cache

Data is cached as JSON files under `$XDG_CONFIG_HOME/kardhub/cache/` (defaults to `~/.config/kardhub/cache/`).

### Cache Files

| File | Contents |
|---|---|
| `sources.json` | Organization logins |
| `repos_{source}.json` | Repository names per source |
| `open_{owner}_{repo}.json` | Open issues per repo (replaced each sync) |
| `closed_{owner}_{repo}.json` | Closed issues per repo (cumulative) |
| `prs_{owner}_{repo}.json` | Open pull requests per repo |
| `merged_{owner}_{repo}.json` | Closed PRs per repo — merged + closed (cumulative) |
| `cards_{owner}_{repo}.json` | Full board cards per repo |
| `settings.json` | Token, last source/repos, default repos |

All data files (except `settings.json`) embed a `synced_at` ISO-8601 UTC timestamp via a `Timestamped<T>` wrapper. TTL freshness and incremental sync cutoffs are derived from this embedded timestamp — no separate sync marker files needed.

### TTL per Data Type

| Data | TTL | Behavior |
|---|---|---|
| Organizations | 6 months | Refetched after TTL expires |
| Repositories | 1 month | Refetched after TTL expires |
| Cards | 3 hours | Skipped if cache is fresh |
| Open issues | per-sync | Full first, `since`-based after (replaced) |
| Closed issues | cumulative | 1 page of 100 first, `since`-based after |
| Open PRs | per-sync | Always full pagination (with reviews/CI) |
| Closed PRs | cumulative | Full first, `paginate_until` cutoff after |

### Incremental Sync

Each data type syncs independently using its own embedded `synced_at` timestamp:

- **Open issues** — first: full pagination (`state=open`); after: `since` filter
- **Closed issues** — first: single page of 100; after: `since` filter (cumulative)
- **Open PRs** — first: full pagination (`state=open`) with reviews/CI; after: `paginate_until` cutoff, deduped against closed
- **Closed PRs** — first: full pagination (`state=closed`); after: paginate desc by updated, stop at cutoff (cumulative)

This dramatically reduces API calls after the initial load.

### Logout

Signing out clears all cached data (all JSON and sync files in the cache directory).

## License

[MIT](LICENSE)
