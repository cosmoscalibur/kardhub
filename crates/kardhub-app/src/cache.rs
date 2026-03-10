//! Local JSON file cache for persistable app data.
//!
//! Stores sources (organizations), repositories per source, closed issues
//! per repo, PR data, full card sets, and user settings. Files live under
//! `$XDG_CONFIG_HOME/kardhub/cache/` (or `~/.config/kardhub/cache/`).
//!
//! Each data type has its own TTL. Closed issues and merged PRs use
//! cumulative caching: new entries are appended and deduplicated.
//! Sync timestamps are embedded inside each JSON file via [`Timestamped`].

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use kardhub_core::models::{AuthenticatedUser, Card, Issue, Label, PullRequest, User};
use serde::{Deserialize, Serialize};

// ── Per-type TTL constants (seconds) ─────────────────────────────────

/// Organization list: 6 months.
const TTL_SOURCES: i64 = 180 * 24 * 60 * 60;
/// Repository list: 1 month.
const TTL_REPOS: i64 = 30 * 24 * 60 * 60;
/// Open issues / open PRs / cards: 3 hours.
const TTL_OPEN: i64 = 3 * 60 * 60;

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
    /// UTC timestamp of last sync.
    pub synced_at: DateTime<Utc>,
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
    let dir = base.join("kardhub").join("cache");
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
///
/// If the file exists but fails to deserialize (corrupt/outdated schema),
/// the file is deleted so the next sync will re-fetch fresh data.
fn load_json<T: for<'de> Deserialize<'de>>(name: &str) -> Option<T> {
    let path = cache_dir().join(name);
    let data = fs::read_to_string(&path).ok()?;
    match serde_json::from_str(&data) {
        Ok(v) => Some(v),
        Err(_) => {
            let _ = fs::remove_file(&path);
            None
        }
    }
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
        synced_at: Utc::now(),
        data,
    };
    save_json(name, &ts);
}

/// Check whether a timestamped cache file is fresh (synced within `ttl_secs`).
fn is_ts_fresh(name: &str, ttl_secs: i64) -> bool {
    let Some(ts) = load_json::<Timestamped<serde_json::Value>>(name) else {
        return false;
    };
    let Some(ttl) = Duration::try_seconds(ttl_secs) else {
        return false;
    };
    Utc::now().signed_duration_since(ts.synced_at) < ttl
}

// ── Unified source map ───────────────────────────────────────────────

/// Personal repos categorized by the user's relationship.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceRepos {
    /// Repos owned by the authenticated user.
    #[serde(default)]
    pub owner: Vec<String>,
    /// Repos where the user is an outside collaborator.
    #[serde(default)]
    pub collaborator: Vec<String>,
}

impl SourceRepos {
    /// Flat list combining owner and collaborator repos.
    pub fn all_repos(&self) -> Vec<String> {
        let mut repos = self.owner.clone();
        repos.extend(self.collaborator.iter().cloned());
        repos
    }
}

/// Organization repos split by membership relationship.
///
/// Each org login appears under exactly one category.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OrgSources {
    /// Orgs where the user is a member, keyed by org login.
    #[serde(default)]
    pub member: BTreeMap<String, Vec<String>>,
    /// Orgs where the user is an outside collaborator, keyed by org login.
    #[serde(default)]
    pub collaborator: BTreeMap<String, Vec<String>>,
}

/// Unified source map stored in a single cache file.
///
/// Groups all accessible repos by source (personal vs organization)
/// and by user relationship.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceMap {
    /// Personal repositories grouped by relationship.
    pub personal: SourceRepos,
    /// Organization repositories split by member/collaborator.
    pub organizations: OrgSources,
}

impl SourceMap {
    /// Sorted list of all organization logins (member + collaborator).
    pub fn org_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .organizations
            .member
            .keys()
            .chain(self.organizations.collaborator.keys())
            .cloned()
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Flat repo-name list for a given sidebar source.
    pub fn repos_for_source(&self, source: &str) -> Vec<String> {
        if source == "personal" {
            self.personal.all_repos()
        } else if let Some(repos) = self.organizations.member.get(source) {
            repos.clone()
        } else if let Some(repos) = self.organizations.collaborator.get(source) {
            repos.clone()
        } else {
            Vec::new()
        }
    }
}

/// Load cached source map.
pub fn load_source_map() -> Option<SourceMap> {
    load_timestamped::<SourceMap>("source_map.json").map(|t| t.data)
}

/// Save source map to cache.
pub fn save_source_map(map: &SourceMap) {
    save_timestamped("source_map.json", map);
}

/// Check whether source map cache is fresh (< 6 months).
pub fn is_source_map_fresh() -> bool {
    is_ts_fresh("source_map.json", TTL_SOURCES)
}

// ── Profile (authenticated user) ─────────────────────────────────────

/// Load cached authenticated user profile.
pub fn load_profile() -> Option<AuthenticatedUser> {
    load_timestamped::<AuthenticatedUser>("profile.json").map(|t| t.data)
}

/// Save authenticated user profile to cache.
pub fn save_profile(user: &AuthenticatedUser) {
    save_timestamped("profile.json", user);
}

/// Check whether profile cache is fresh (< 1 month).
pub fn is_profile_fresh() -> bool {
    is_ts_fresh("profile.json", TTL_REPOS)
}

// ── Members ──────────────────────────────────────────────────────────

/// Save organization members and collaborators.
pub fn save_members(org: &str, members: &[User]) {
    let key = source_key(org);
    save_timestamped(&format!("members_{key}.json"), &members.to_vec());
}

/// Load cached organization members and collaborators.
#[allow(dead_code)] // Used by UI to resolve author/assignee logins
pub fn load_members(org: &str) -> Option<Vec<User>> {
    let key = source_key(org);
    load_timestamped::<Vec<User>>(&format!("members_{key}.json")).map(|t| t.data)
}

/// Check whether members cache is fresh (< 1 month).
pub fn is_members_fresh(org: &str) -> bool {
    let key = source_key(org);
    is_ts_fresh(&format!("members_{key}.json"), TTL_REPOS)
}

// ── Labels ───────────────────────────────────────────────────────────

/// Labels TTL: 1 month.
const TTL_LABELS: i64 = 30 * 24 * 60 * 60;

/// Save repository labels.
pub fn save_labels(owner: &str, repo: &str, labels: &[Label]) {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    save_timestamped(&format!("labels_{key_o}_{key_r}.json"), &labels.to_vec());
}

/// Load cached repository labels.
pub fn load_labels(owner: &str, repo: &str) -> Option<Vec<Label>> {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    load_timestamped::<Vec<Label>>(&format!("labels_{key_o}_{key_r}.json")).map(|t| t.data)
}

/// Check whether labels cache is fresh (< 1 month).
pub fn is_labels_fresh(owner: &str, repo: &str) -> bool {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    is_ts_fresh(&format!("labels_{key_o}_{key_r}.json"), TTL_LABELS)
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
pub fn open_sync_time(owner: &str, repo: &str) -> Option<DateTime<Utc>> {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    load_timestamped::<serde_json::Value>(&format!("open_{key_o}_{key_r}.json"))
        .map(|t| t.synced_at)
}

/// Get the last sync time for closed issues from the closed issues cache.
pub fn closed_sync_time(owner: &str, repo: &str) -> Option<DateTime<Utc>> {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    load_timestamped::<serde_json::Value>(&format!("closed_{key_o}_{key_r}.json"))
        .map(|t| t.synced_at)
}

/// Get the last sync time for open PRs from the open PRs cache.
pub fn prs_sync_time(owner: &str, repo: &str) -> Option<DateTime<Utc>> {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    load_timestamped::<serde_json::Value>(&format!("prs_{key_o}_{key_r}.json")).map(|t| t.synced_at)
}

/// Get the last sync time for merged/closed PRs from the merged PRs cache.
pub fn merged_sync_time(owner: &str, repo: &str) -> Option<DateTime<Utc>> {
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

/// Remove per-repo caches that feed incremental sync (open issues,
/// open PRs, and derived cards). Deleting these files forces a full
/// re-fetch on the next `fetch_cards` call, preventing issues from
/// being permanently missed by stale `since` timestamps.
pub fn clear_repo_open_cache(owner: &str, repo: &str) {
    let key_o = source_key(owner);
    let key_r = source_key(repo);
    let dir = cache_dir();
    for prefix in ["open_", "cards_", "prs_"] {
        let path = dir.join(format!("{prefix}{key_o}_{key_r}.json"));
        let _ = fs::remove_file(path);
    }
}
