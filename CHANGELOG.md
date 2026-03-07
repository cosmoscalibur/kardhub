# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial workspace with `kardhub-core`, `kardhub-app`, `kardhub-extension`, and `kardhub-wasm-bridge` crates
- GitHub sync via Personal Access Token (issues + PRs)
- Kanban column mapping based on labels, reviews, CI status, and branch state
- Local JSON cache with per-type TTLs and incremental sync
- Session persistence (token, last source, selected repos)
- Settings panel with default repository config and cache management
- Dark and light theme toggle
- Card detail panel with full metadata
