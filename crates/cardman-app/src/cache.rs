//! Local JSON file cache for persistable app data.
//!
//! Stores sources (organizations), repositories per source, closed issues
//! per repo, PR data, full card sets, and user settings. Files live under
//! `$XDG_CONFIG_HOME/cardman/cache/` (or `~/.config/cardman/cache/`).
//!
//! Each data type has its own TTL. Closed issues and merged PRs use
//! cumulative caching: new entries are appended and deduplicated.
//! Sync timestamps are embedded inside each JSON file via [`Timestamped`].

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use cardman_core::models::{Card, Issue, PullRequest};
use serde::{Deserialize, Serialize};

// ── Per-type TTL constants (seconds) ─────────────────────────────────

/// Organization list: 6 months.
const TTL_SOURCES: u64 = 180 * 24 * 60 * 60;
/// Repository list: 1 month.
const TTL_REPOS: u64 = 30 * 24 * 60 * 60;
/// Open issues / open PRs / cards: 3 hours.
const TTL_OPEN: u64 = 3 * 60 * 60;

/// User-configurable defaults persisted across sessions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppSettings {
    /// Default repository name per source key (e.g. `"personal"` → `"my-repo"`).
    #[serde(default)]
    pub default_repos: HashMap<String, String>,
    /// Persisted GitHub PAT for auto-login.
    #[serde(default)]
    pub token: Option<String>,
    /// Last active source (`"personal"` or org name).
    #[serde(default)]
    pub last_source: Option<String>,
    /// Last selected repository names.
    #[serde(default)]
    pub last_repos: Vec<String>,
}

/// Wrapper that embeds a sync timestamp alongside cached data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timestamped<T> {
    /// ISO-8601 UTC timestamp of last sync.
    pub synced_at: String,
    /// The cached data.
    pub data: T,
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

/// Load a timestamped JSON file, returning data and timestamp.
fn load_timestamped<T: for<'de> Deserialize<'de>>(name: &str) -> Option<Timestamped<T>> {
    load_json::<Timestamped<T>>(name)
}

/// Save data with current UTC timestamp.
fn save_timestamped<T: Serialize>(name: &str, data: &T) {
    let ts = Timestamped {
        synced_at: now_iso(),
        data,
    };
    save_json(name, &ts);
}

/// Check whether a timestamped cache file is fresh (synced within `ttl_secs`).
fn is_ts_fresh(name: &str, ttl_secs: u64) -> bool {
    let Some(ts) = load_json::<Timestamped<serde_json::Value>>(name) else {
        return false;
    };
    is_iso_within(&ts.synced_at, ttl_secs)
}

/// Check whether an ISO-8601 timestamp is within `ttl_secs` of now.
fn is_iso_within(iso: &str, ttl_secs: u64) -> bool {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let Some(ts) = parse_iso_epoch(iso) else {
        return false;
    };
    now.saturating_sub(ts) < ttl_secs
}

/// Parse an ISO-8601 UTC timestamp to epoch seconds (basic parser).
fn parse_iso_epoch(iso: &str) -> Option<u64> {
    // Expected: "YYYY-MM-DDTHH:MM:SSZ"
    if iso.len() < 20 {
        return None;
    }
    let y: i64 = iso[0..4].parse().ok()?;
    let mo: u64 = iso[5..7].parse().ok()?;
    let d: u64 = iso[8..10].parse().ok()?;
    let h: u64 = iso[11..13].parse().ok()?;
    let mi: u64 = iso[14..16].parse().ok()?;
    let s: u64 = iso[17..19].parse().ok()?;
    // Days from epoch using inverse of Hinnant's algorithm
    let (yr, mo2) = if mo <= 2 {
        (y - 1, mo + 9)
    } else {
        (y, mo - 3)
    };
    let era = (if yr >= 0 { yr } else { yr - 399 }) / 400;
    let yoe = (yr - era * 400) as u64;
    let doy = (153 * mo2 + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = (era * 146_097 + doe as i64 - 719_468) as u64;
    Some(days * 86400 + h * 3600 + mi * 60 + s)
}

/// Get current UTC time as ISO-8601 string.
fn now_iso() -> String {
    let epoch = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format_epoch_as_iso(epoch)
}

/// Format epoch seconds as ISO-8601 UTC string.
fn format_epoch_as_iso(epoch: u64) -> String {
    let days = (epoch / 86400) as i64;
    let rem = epoch % 86400;
    let h = rem / 3600;
    let m = (rem % 3600) / 60;
    let s = rem % 60;

    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr = if mo <= 2 { y + 1 } else { y };

    format!("{yr:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

// ── Sources (organizations) ──────────────────────────────────────────

/// Load cached organization logins.
pub fn load_sources() -> Option<Vec<String>> {
    load_timestamped::<Vec<String>>("sources.json").map(|t| t.data)
}

/// Save organization logins to cache.
pub fn save_sources(orgs: &[String]) {
    save_timestamped("sources.json", &orgs.to_vec());
}

/// Check whether sources cache is fresh (< 6 months).
pub fn is_sources_fresh() -> bool {
    is_ts_fresh("sources.json", TTL_SOURCES)
}

// ── Repositories ─────────────────────────────────────────────────────

/// Load cached repository names for a given source.
pub fn load_repos(src_key: &str) -> Option<Vec<String>> {
    load_timestamped::<Vec<String>>(&format!("repos_{src_key}.json")).map(|t| t.data)
}

/// Save repository names for a given source.
pub fn save_repos(src_key: &str, repos: &[String]) {
    save_timestamped(&format!("repos_{src_key}.json"), &repos.to_vec());
}

/// Check whether repos cache is fresh (< 1 month).
pub fn is_repos_fresh(src_key: &str) -> bool {
    is_ts_fresh(&format!("repos_{src_key}.json"), TTL_REPOS)
}

// ── Open issues ──────────────────────────────────────────────────────

/// Load cached open issues for a repo.
pub fn load_open_issues(owner: &str, repo: &str) -> Option<Vec<Issue>> {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    load_timestamped::<Vec<Issue>>(&format!("open_{key_o}_{key_r}.json")).map(|t| t.data)
}

/// Save open issues for a repo (replaced each time).
pub fn save_open_issues(owner: &str, repo: &str, issues: &[Issue]) {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    save_timestamped(&format!("open_{key_o}_{key_r}.json"), &issues.to_vec());
}

// ── Closed issues (cumulative) ───────────────────────────────────────

/// Load cached closed issues for a repo.
pub fn load_closed_issues(owner: &str, repo: &str) -> Option<Vec<Issue>> {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    load_timestamped::<Vec<Issue>>(&format!("closed_{key_o}_{key_r}.json")).map(|t| t.data)
}

/// Cumulatively save closed issues: merge with existing, dedup by number.
pub fn save_closed_issues(owner: &str, repo: &str, new_issues: &[Issue]) {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    let fname = format!("closed_{key_o}_{key_r}.json");

    let mut existing = load_timestamped::<Vec<Issue>>(&fname)
        .map(|t| t.data)
        .unwrap_or_default();
    for issue in new_issues {
        if !existing.iter().any(|e| e.number == issue.number) {
            existing.push(issue.clone());
        }
    }
    save_timestamped(&fname, &existing);
}

// ── Pull requests ────────────────────────────────────────────────────

/// Load cached open pull requests for a repo.
pub fn load_prs(owner: &str, repo: &str) -> Option<Vec<PullRequest>> {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    load_timestamped::<Vec<PullRequest>>(&format!("prs_{key_o}_{key_r}.json")).map(|t| t.data)
}

/// Save open pull requests for a repo (replaced each time).
pub fn save_prs(owner: &str, repo: &str, prs: &[PullRequest]) {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    save_timestamped(&format!("prs_{key_o}_{key_r}.json"), &prs.to_vec());
}

/// Cumulatively save merged/closed PRs: merge with existing, dedup by number.
pub fn save_merged_prs(owner: &str, repo: &str, new_prs: &[PullRequest]) {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    let fname = format!("merged_{key_o}_{key_r}.json");

    let mut existing = load_timestamped::<Vec<PullRequest>>(&fname)
        .map(|t| t.data)
        .unwrap_or_default();
    for pr in new_prs {
        if !existing.iter().any(|e| e.number == pr.number) {
            existing.push(pr.clone());
        }
    }
    save_timestamped(&fname, &existing);
}

/// Load cached merged/closed PRs for a repo.
pub fn load_merged_prs(owner: &str, repo: &str) -> Option<Vec<PullRequest>> {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    load_timestamped::<Vec<PullRequest>>(&format!("merged_{key_o}_{key_r}.json")).map(|t| t.data)
}

// ── Cards (full board data) ──────────────────────────────────────────

/// Load cached cards for a repo.
pub fn load_cards(owner: &str, repo: &str) -> Option<Vec<Card>> {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    load_timestamped::<Vec<Card>>(&format!("cards_{key_o}_{key_r}.json")).map(|t| t.data)
}

/// Save cards for a repo.
pub fn save_cards(owner: &str, repo: &str, cards: &[Card]) {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    save_timestamped(&format!("cards_{key_o}_{key_r}.json"), &cards.to_vec());
}

/// Count cached cards for a repo (0 if not cached).
pub fn cached_card_count(owner: &str, repo: &str) -> usize {
    load_cards(owner, repo).map(|c| c.len()).unwrap_or(0)
}

/// Check whether the cards cache for a repo is fresh (< 3h).
pub fn is_cards_fresh(owner: &str, repo: &str) -> bool {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    is_ts_fresh(&format!("cards_{key_o}_{key_r}.json"), TTL_OPEN)
}

// ── Sync timestamps (per data type) ──────────────────────────────────

/// Get the last sync time for open issues from the open issues cache.
pub fn open_sync_time(owner: &str, repo: &str) -> Option<String> {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    load_timestamped::<serde_json::Value>(&format!("open_{key_o}_{key_r}.json"))
        .map(|t| t.synced_at)
}

/// Get the last sync time for closed issues from the closed issues cache.
pub fn closed_sync_time(owner: &str, repo: &str) -> Option<String> {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    load_timestamped::<serde_json::Value>(&format!("closed_{key_o}_{key_r}.json"))
        .map(|t| t.synced_at)
}

/// Get the last sync time for open PRs from the open PRs cache.
pub fn prs_sync_time(owner: &str, repo: &str) -> Option<String> {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    load_timestamped::<serde_json::Value>(&format!("prs_{key_o}_{key_r}.json")).map(|t| t.synced_at)
}

/// Get the last sync time for merged/closed PRs from the merged PRs cache.
pub fn merged_sync_time(owner: &str, repo: &str) -> Option<String> {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    load_timestamped::<serde_json::Value>(&format!("merged_{key_o}_{key_r}.json"))
        .map(|t| t.synced_at)
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

// ── Cache management ─────────────────────────────────────────────────

/// Remove all cached data (called on sign out).
pub fn clear_all_cache() {
    let dir = cache_dir();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let _ = fs::remove_file(path);
            }
        }
    }
}
