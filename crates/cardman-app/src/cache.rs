//! Local JSON file cache for persistable app data.
//!
//! Stores sources (organizations), repositories per source, closed issues
//! per repo, and default settings. Files live under
//! `$XDG_CONFIG_HOME/cardman/cache/` (or `~/.config/cardman/cache/`).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use cardman_core::models::{Card, Issue};
use serde::{Deserialize, Serialize};

/// User-configurable defaults persisted across sessions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppSettings {
    /// Default repository name per source key (e.g. `"personal"` → `"my-repo"`).
    pub default_repos: HashMap<String, String>,
}

// ── Path helpers ─────────────────────────────────────────────────────

/// Resolve the cache directory, creating it if needed.
fn cache_dir() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".config")
        });
    let dir = base.join("cardman").join("cache");
    let _ = fs::create_dir_all(&dir);
    dir
}

/// Build the key string for a source (used in filenames).
pub fn source_key(source_name: &str) -> String {
    source_name
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric(), "_")
}

// ── Generic helpers ──────────────────────────────────────────────────

/// Load a JSON file, returning `None` on any error.
fn load_json<T: for<'de> Deserialize<'de>>(name: &str) -> Option<T> {
    let path = cache_dir().join(name);
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Save a value as JSON to a file.
fn save_json<T: Serialize>(name: &str, value: &T) {
    let path = cache_dir().join(name);
    if let Ok(json) = serde_json::to_string_pretty(value) {
        let _ = fs::write(path, json);
    }
}

// ── Sources (organizations) ──────────────────────────────────────────

/// Load cached organization logins.
pub fn load_sources() -> Option<Vec<String>> {
    load_json("sources.json")
}

/// Save organization logins to cache.
pub fn save_sources(orgs: &[String]) {
    save_json("sources.json", &orgs);
}

// ── Repositories ─────────────────────────────────────────────────────

/// Load cached repository names for a given source.
pub fn load_repos(src_key: &str) -> Option<Vec<String>> {
    load_json(&format!("repos_{src_key}.json"))
}

/// Save repository names for a given source.
pub fn save_repos(src_key: &str, repos: &[String]) {
    save_json(&format!("repos_{src_key}.json"), &repos);
}

// ── Closed issues ────────────────────────────────────────────────────

/// Check whether closed issues are cached for a repo.
pub fn has_closed_issues(owner: &str, repo: &str) -> bool {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    cache_dir()
        .join(format!("closed_{key_o}_{key_r}.json"))
        .exists()
}

/// Load cached closed issues for a repo.
pub fn load_closed_issues(owner: &str, repo: &str) -> Option<Vec<Issue>> {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    load_json(&format!("closed_{key_o}_{key_r}.json"))
}

/// Save closed issues for a repo.
pub fn save_closed_issues(owner: &str, repo: &str, issues: &[Issue]) {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    save_json(&format!("closed_{key_o}_{key_r}.json"), &issues);
}

// ── Cards (full board data) ──────────────────────────────────────────

/// Load cached cards for a repo.
pub fn load_cards(owner: &str, repo: &str) -> Option<Vec<Card>> {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    load_json(&format!("cards_{key_o}_{key_r}.json"))
}

/// Save cards for a repo.
pub fn save_cards(owner: &str, repo: &str, cards: &[Card]) {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    save_json(&format!("cards_{key_o}_{key_r}.json"), &cards);
}

// ── Settings ─────────────────────────────────────────────────────────

/// Load persisted app settings.
pub fn load_settings() -> AppSettings {
    load_json("settings.json").unwrap_or_default()
}

/// Save app settings.
pub fn save_settings(settings: &AppSettings) {
    save_json("settings.json", settings);
}
