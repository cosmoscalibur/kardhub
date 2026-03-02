//! Lightweight WASM bridge for the Cardman browser extension.
//!
//! Exposes `cardman-core` mapping and classification logic as
//! `#[wasm_bindgen]` functions with JSON string I/O. No Dioxus dependency,
//! so the output module is safe to import in a service worker.

use std::collections::HashMap;

use wasm_bindgen::prelude::*;

use cardman_core::mapping::{MappingConfig, map_card};
use cardman_core::models::{Card, CardSource, CiStatus, Issue, IssueState, Label, PullRequest};

// ── Shared deserialisation types (mirrors GitHub API) ────────────────

#[derive(serde::Deserialize)]
struct RawIssue {
    number: u64,
    title: String,
    body: Option<String>,
    #[serde(default)]
    labels: Vec<RawLabel>,
    #[serde(default)]
    assignees: Vec<RawUser>,
    state: String,
    #[serde(default)]
    user: Option<RawUser>,
}

#[derive(serde::Deserialize)]
struct RawPr {
    number: u64,
    title: String,
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    labels: Vec<RawLabel>,
    #[serde(default)]
    assignees: Vec<RawUser>,
    #[serde(default)]
    requested_reviewers: Vec<RawUser>,
    head: Option<RawHead>,
    merged_at: Option<String>,
    state: String,
    #[serde(default)]
    user: Option<RawUser>,
}

#[derive(serde::Deserialize)]
struct RawLabel {
    name: String,
    #[serde(default)]
    color: String,
}

#[derive(serde::Deserialize)]
struct RawUser {
    login: String,
    #[serde(default)]
    avatar_url: Option<String>,
}

#[derive(serde::Deserialize)]
struct RawHead {
    #[serde(rename = "ref")]
    branch_ref: String,
}

// ── Serialisation types (returned to JS) ─────────────────────────────

#[derive(serde::Serialize)]
struct MapResult {
    columns: Vec<ColDef>,
    cards: Vec<MappedCard>,
}

#[derive(serde::Serialize)]
struct ColDef {
    emoji: String,
    name: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MappedCard {
    column: String,
    emoji: String,
    number: u64,
    title: String,
    is_pr: bool,
    owner: String,
    repo: String,
    labels: Vec<CardLabel>,
    assignees: Vec<CardAssignee>,
    priority: Option<u8>,
}

#[derive(serde::Serialize)]
struct CardLabel {
    name: String,
    color: String,
}

#[derive(serde::Serialize)]
struct CardAssignee {
    login: String,
    avatar_url: String,
}

// ── map_cards_json ───────────────────────────────────────────────────

/// Map raw GitHub API card data to Kanban columns using `cardman-core`.
///
/// Takes JSON `{ "issues": [...], "prs": [...] }` and returns a JSON
/// object with `columns` (ordered definitions) and `cards` (mapped items
/// with column, labels, assignees, etc.).
#[wasm_bindgen]
pub fn map_cards_json(raw_json: &str, owner: &str, repo: &str) -> String {
    #[derive(serde::Deserialize)]
    struct RawData {
        #[serde(default)]
        issues: Vec<RawIssue>,
        #[serde(default)]
        prs: Vec<RawPr>,
    }

    let raw: RawData = match serde_json::from_str(raw_json) {
        Ok(d) => d,
        Err(_) => {
            return serde_json::to_string(&MapResult {
                columns: Vec::new(),
                cards: Vec::new(),
            })
            .unwrap_or_default();
        }
    };

    let config = MappingConfig::default();
    let mut cards: Vec<Card> = Vec::new();
    // Preserve avatar URLs from raw API (cardman-core only stores login strings)
    let mut avatar_map: HashMap<String, String> = HashMap::new();

    for ri in raw.issues {
        let issue = Issue {
            number: ri.number,
            title: ri.title,
            body: ri.body,
            labels: ri
                .labels
                .into_iter()
                .map(|l| Label {
                    name: l.name,
                    color: l.color,
                })
                .collect(),
            assignees: ri
                .assignees
                .into_iter()
                .map(|u| {
                    if let Some(ref url) = u.avatar_url {
                        avatar_map.insert(u.login.clone(), url.clone());
                    }
                    u.login
                })
                .collect(),
            state: if ri.state == "open" {
                IssueState::Open
            } else {
                IssueState::Closed
            },
            sub_issues: Vec::new(),
            author: ri.user.map(|u| u.login).unwrap_or_default(),
            updated_at: chrono::Utc::now(),
        };
        cards.push(map_card(owner, repo, CardSource::Issue(issue), &config));
    }

    for rp in raw.prs {
        let merged = rp.merged_at.is_some();
        let closed = rp.state == "closed" && !merged;
        let author = rp.user.map(|u| u.login).unwrap_or_default();
        let assignee_logins: Vec<String> = rp
            .assignees
            .into_iter()
            .map(|u| {
                if let Some(ref url) = u.avatar_url {
                    avatar_map.insert(u.login.clone(), url.clone());
                }
                u.login
            })
            .collect();
        let assignees = if assignee_logins.is_empty() {
            vec![author.clone()]
        } else {
            assignee_logins
        };
        let pr = PullRequest {
            number: rp.number,
            title: rp.title,
            body: rp.body,
            draft: rp.draft,
            author,
            assignees,
            requested_reviewers: rp
                .requested_reviewers
                .into_iter()
                .map(|u| u.login)
                .collect(),
            reviews: Vec::new(),
            ci_status: CiStatus::Pending,
            merged,
            closed,
            branch: rp.head.map(|h| h.branch_ref).unwrap_or_default(),
            labels: rp
                .labels
                .into_iter()
                .map(|l| Label {
                    name: l.name,
                    color: l.color,
                })
                .collect(),
            updated_at: chrono::Utc::now(),
        };
        cards.push(map_card(owner, repo, CardSource::PullRequest(pr), &config));
    }

    cards.sort_by(|a, b| a.priority.cmp(&b.priority));

    let col_defs: Vec<ColDef> = [
        ("🧊", "Icebox"),
        ("⏳", "Prebacklog"),
        ("📥", "Backlog"),
        ("❌", "Failed"),
        ("🚧", "In Progress"),
        ("👀", "Code review"),
        ("⏳", "QA Backlog"),
        ("🔍", "QA Review"),
        ("☑\u{fe0f}", "Ready for STG"),
        ("✅", "Ready for deploy"),
        ("📦", "In Release"),
    ]
    .iter()
    .map(|(emoji, name)| ColDef {
        emoji: emoji.to_string(),
        name: name.to_string(),
    })
    .collect();

    let mapped: Vec<MappedCard> = cards
        .into_iter()
        .filter(|c| c.column.name != "Closed")
        .map(|c| {
            let (number, title, is_pr, labels, assignees) = match &c.source {
                CardSource::Issue(i) => (
                    i.number,
                    i.title.clone(),
                    false,
                    i.labels
                        .iter()
                        .map(|l| CardLabel {
                            name: l.name.clone(),
                            color: l.color.clone(),
                        })
                        .collect::<Vec<_>>(),
                    i.assignees
                        .iter()
                        .map(|a| CardAssignee {
                            login: a.clone(),
                            avatar_url: avatar_map.get(a).cloned().unwrap_or_default(),
                        })
                        .collect::<Vec<_>>(),
                ),
                CardSource::PullRequest(pr) => (
                    pr.number,
                    pr.title.clone(),
                    true,
                    pr.labels
                        .iter()
                        .map(|l| CardLabel {
                            name: l.name.clone(),
                            color: l.color.clone(),
                        })
                        .collect::<Vec<_>>(),
                    pr.assignees
                        .iter()
                        .map(|a| CardAssignee {
                            login: a.clone(),
                            avatar_url: avatar_map.get(a).cloned().unwrap_or_default(),
                        })
                        .collect::<Vec<_>>(),
                ),
            };
            MappedCard {
                column: c.column.name,
                emoji: c.column.emoji,
                number,
                title,
                is_pr,
                owner: c.owner,
                repo: c.repo,
                labels,
                assignees,
                priority: c.priority.map(|p| p.0),
            }
        })
        .collect();

    serde_json::to_string(&MapResult {
        columns: col_defs,
        cards: mapped,
    })
    .unwrap_or_default()
}

// ── classify_repos_json ──────────────────────────────────────────────

/// Classify repositories into organizations and personal repos.
///
/// Takes raw `/user/repos` and `/user/orgs` API JSON arrays, uses
/// `owner.type == "Organization"` to detect org repos (including
/// collaborator orgs), and returns a classified structure.
#[wasm_bindgen]
pub fn classify_repos_json(repos_json: &str, orgs_json: &str) -> String {
    #[derive(serde::Deserialize)]
    struct RawRepo {
        name: String,
        full_name: Option<String>,
        #[serde(default)]
        archived: bool,
        owner: Option<RawOwner>,
    }

    #[derive(serde::Deserialize)]
    struct RawOwner {
        login: String,
        #[serde(rename = "type", default)]
        owner_type: String,
    }

    #[derive(serde::Deserialize)]
    struct RawOrg {
        login: String,
    }

    #[derive(serde::Serialize)]
    struct RepoEntry {
        owner: String,
        repo: String,
        full: String,
    }

    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ClassifyResult {
        orgs: Vec<String>,
        personal_repos: Vec<RepoEntry>,
        org_repos: HashMap<String, Vec<RepoEntry>>,
    }

    let repos: Vec<RawRepo> = serde_json::from_str::<Vec<RawRepo>>(repos_json)
        .unwrap_or_default()
        .into_iter()
        .filter(|r| !r.archived)
        .collect();
    let member_orgs: Vec<RawOrg> = serde_json::from_str(orgs_json).unwrap_or_default();

    let mut org_set: Vec<String> = member_orgs.into_iter().map(|o| o.login).collect();

    for r in &repos {
        if let Some(ref owner) = r.owner
            && owner.owner_type == "Organization"
            && !org_set.iter().any(|o| o.eq_ignore_ascii_case(&owner.login))
        {
            org_set.push(owner.login.clone());
        }
    }
    org_set.sort();

    let mut personal_repos = Vec::new();
    let mut org_repos: HashMap<String, Vec<RepoEntry>> = HashMap::new();

    for r in repos {
        let owner_login = r
            .owner
            .as_ref()
            .map(|o| o.login.clone())
            .unwrap_or_default();
        let full = r
            .full_name
            .unwrap_or_else(|| format!("{}/{}", owner_login, r.name));
        let entry = RepoEntry {
            owner: owner_login.clone(),
            repo: r.name,
            full,
        };

        if org_set.iter().any(|o| o.eq_ignore_ascii_case(&owner_login)) {
            org_repos
                .entry(
                    org_set
                        .iter()
                        .find(|o| o.eq_ignore_ascii_case(&owner_login))
                        .cloned()
                        .unwrap_or(owner_login),
                )
                .or_default()
                .push(entry);
        } else {
            personal_repos.push(entry);
        }
    }

    serde_json::to_string(&ClassifyResult {
        orgs: org_set,
        personal_repos,
        org_repos,
    })
    .unwrap_or_default()
}
