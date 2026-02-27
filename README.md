# Cardman

A desktop Kanban board synced with GitHub issues and pull requests, built entirely in Rust.

## Features

- **GitHub sync** — authenticates via Personal Access Token and pulls issues + PRs from any repo you own or belong to
- **Smart column mapping** — cards auto-sort into Kanban columns based on labels, reviews, CI status, and branch state
- **Local cache** — sources, repos, and closed issues are cached locally for instant startup; open cards are shown from cache while a background refresh runs
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
| 🚪 Closed | Closed issues |

## Architecture

```
cardman/
├── crates/
│   ├── cardman-core/     # Platform-agnostic: models, mapping engine, GitHub REST client
│   ├── cardman-app/      # Dioxus desktop application
│   └── cardman-extension/# Browser extension (scaffold)
├── Cargo.toml            # Workspace root
└── rustfmt.toml
```

| Crate | Purpose |
|---|---|
| `cardman-core` | Domain models, card-to-column mapping, GitHub API client (`reqwest`), OAuth helpers |
| `cardman-app` | Dioxus 0.7 desktop UI — sidebar, board, card detail, settings, local JSON cache |
| `cardman-extension` | Browser extension scaffold (Chromium + Firefox, future) |

## Requirements

- **Rust** nightly or stable with edition 2024 support
- **System dependencies** for Dioxus desktop (WebView):
  - **Debian/Ubuntu**: `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev libxdo-dev`
  - **Arch/Manjaro**: `sudo pacman -S webkit2gtk-4.1 gtk3 libayatana-appindicator xdotool`
  - **macOS / Windows**: no extra dependencies needed

## Quick Start

```zsh
# Clone
git clone https://github.com/cosmoscalibur/cardman.git
cd cardman

# Run the desktop app
cargo run -p cardman-app

# Run tests
cargo test -p cardman-core

# Lint
cargo clippy --all-targets -- -D warnings
```

On first launch, enter a [GitHub Personal Access Token](https://github.com/settings/tokens) with `repo` and `read:org` scopes.

## Local Cache

Data is cached as JSON files under `$XDG_CONFIG_HOME/cardman/cache/` (defaults to `~/.config/cardman/cache/`):

| File | Contents |
|---|---|
| `sources.json` | Organization logins |
| `repos_{source}.json` | Repository names per source |
| `closed_{owner}_{repo}.json` | Closed issues per repo |
| `cards_{owner}_{repo}.json` | Full board cards per repo |
| `settings.json` | Default repos and preferences |

Cached data is used instantly on startup; a background fetch updates it with live data.

## License

[MIT](LICENSE)
