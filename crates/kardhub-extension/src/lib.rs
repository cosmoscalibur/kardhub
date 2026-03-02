//! KardHub browser extension entry point.
//!
//! Launches Dioxus web components that are mounted into GitHub pages
//! by the content script bootloader (`content_loader.js`).

mod bridge;
mod dashboard;
mod linker;

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

use dashboard::Dashboard;
use linker::IssueLinker;

/// Entry point called by the content loader to initialize on a specific element.
///
/// `context_json` is a JSON string like:
/// ```json
/// {"type":"repo","owner":"foo","repo":"bar"}
/// {"type":"pr","owner":"foo","repo":"bar","prNumber":42}
/// ```
#[wasm_bindgen]
pub fn kardhub_init(context_json: &str) {
    let val: serde_json::Value = match serde_json::from_str(context_json) {
        Ok(v) => v,
        Err(_) => return,
    };

    let page_type = val
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let owner = val
        .get("owner")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let repo = val
        .get("repo")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let pr_number = val.get("prNumber").and_then(|v| v.as_u64()).unwrap_or(0);

    if owner.is_empty() || repo.is_empty() {
        return;
    }

    // Store context in global state for the root component to read.
    GLOBAL_CTX.with(|ctx| {
        *ctx.borrow_mut() = Some(PageCtx {
            page_type,
            owner,
            repo,
            pr_number,
        });
    });

    dioxus::launch(App);
}

// ── Hybrid WASM API ──────────────────────────────────────────────────
//
// Pure JSON-in / JSON-out functions consumed by `content_loader.js`.
// The JS side handles DOM, API requests, and rendering; these functions
// provide the core logic (mapping, classification) without Dioxus.

/// Map raw GitHub API card data to Kanban columns using `kardhub-core`.
///
/// # Input (`raw_json`)
///
/// ```json
/// {
///   "issues": [ /* GitHub /repos/:owner/:repo/issues response items */ ],
///   "prs":    [ /* GitHub /repos/:owner/:repo/pulls  response items */ ]
/// }
/// ```
///
/// # Output
///
/// JSON object with `columns` (ordered definitions) and `cards` (mapped items):
/// ```json
/// {
///   "columns": [{ "emoji": "🧊", "name": "Icebox" }, …],
///   "cards": [{
///     "column": "Icebox", "emoji": "🧊", "number": 42, "title": "…",
///     "isPr": false, "owner": "…", "repo": "…",
///     "labels": [{ "name": "…", "color": "…" }],
///     "assignees": [{ "login": "…", "avatar_url": "…" }],
///     "priority": 3
///   }, …]
/// }
/// ```
#[wasm_bindgen]
pub fn map_cards_json(raw_json: &str, owner: &str, repo: &str) -> String {
    use kardhub_core::mapping::{MappingConfig, map_card};
    use kardhub_core::models::{Card, CardSource, CiStatus, Issue, IssueState, Label, PullRequest};

    // ── Deserialisation types (mirrors GitHub API) ────────────────────

    #[derive(serde::Deserialize)]
    struct RawData {
        #[serde(default)]
        issues: Vec<RawIssue>,
        #[serde(default)]
        prs: Vec<RawPr>,
    }

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

    // ── Serialisation types (returned to JS) ─────────────────────────

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

    // ── Parse and map ────────────────────────────────────────────────

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
            assignees: ri.assignees.into_iter().map(|u| u.login).collect(),
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
        let raw_assignees: Vec<RawUser> = rp.assignees;
        let assignee_logins: Vec<String> = raw_assignees.iter().map(|u| u.login.clone()).collect();
        let assignee_list: Vec<CardAssignee> = raw_assignees
            .into_iter()
            .map(|u| CardAssignee {
                login: u.login,
                avatar_url: u.avatar_url.unwrap_or_default(),
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
        // Store assignees before moving pr into CardSource
        let _ = &assignee_list;
        cards.push(map_card(owner, repo, CardSource::PullRequest(pr), &config));
    }

    cards.sort_by(|a, b| a.priority.cmp(&b.priority));

    // Build column definitions from mapping engine
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

    // Convert Card → MappedCard
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
                            avatar_url: String::new(),
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
                            avatar_url: String::new(),
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

    let result = MapResult {
        columns: col_defs,
        cards: mapped,
    };

    serde_json::to_string(&result).unwrap_or_default()
}

/// Classify repositories into organizations and personal repos.
///
/// # Input
///
/// - `repos_json`: Raw `/user/repos?type=all` API response (array).
/// - `orgs_json`:  Raw `/user/orgs` API response (array).
///
/// # Output
///
/// ```json
/// {
///   "orgs": ["orgA", "orgB"],
///   "personalRepos": [{ "owner": "user", "repo": "my-repo", "full": "user/my-repo" }],
///   "orgRepos": {
///     "orgA": [{ "owner": "orgA", "repo": "proj", "full": "orgA/proj" }]
///   }
/// }
/// ```
#[wasm_bindgen]
pub fn classify_repos_json(repos_json: &str, orgs_json: &str) -> String {
    #[derive(serde::Deserialize)]
    struct RawRepo {
        name: String,
        full_name: Option<String>,
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
        org_repos: std::collections::HashMap<String, Vec<RepoEntry>>,
    }

    let repos: Vec<RawRepo> = serde_json::from_str(repos_json).unwrap_or_default();
    let member_orgs: Vec<RawOrg> = serde_json::from_str(orgs_json).unwrap_or_default();

    // Start with member orgs
    let mut org_set: Vec<String> = member_orgs.into_iter().map(|o| o.login).collect();

    // Add collaborator orgs (owner.type == "Organization" not already in list)
    for r in &repos {
        if let Some(ref owner) = r.owner
            && owner.owner_type == "Organization"
            && !org_set.iter().any(|o| o.eq_ignore_ascii_case(&owner.login))
        {
            org_set.push(owner.login.clone());
        }
    }
    org_set.sort();

    // Classify repos
    let mut personal_repos = Vec::new();
    let mut org_repos: std::collections::HashMap<String, Vec<RepoEntry>> =
        std::collections::HashMap::new();

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

    let result = ClassifyResult {
        orgs: org_set,
        personal_repos,
        org_repos,
    };

    serde_json::to_string(&result).unwrap_or_default()
}

// Global page context, set once before launch and read by the root component.
thread_local! {
    static GLOBAL_CTX: std::cell::RefCell<Option<PageCtx>> = const { std::cell::RefCell::new(None) };
}

/// Parsed page context.
#[derive(Debug, Clone)]
struct PageCtx {
    page_type: String,
    owner: String,
    repo: String,
    pr_number: u64,
}

/// Root application component dispatching on page context.
#[component]
fn App() -> Element {
    let ctx = GLOBAL_CTX.with(|c| c.borrow().clone());
    let ctx = match ctx {
        Some(c) => c,
        None => return rsx! { div { "No context" } },
    };

    match ctx.page_type.as_str() {
        "repo" | "repo-sub" => {
            rsx! {
                Dashboard { owner: ctx.owner, repo: ctx.repo }
            }
        }
        "pr" => {
            rsx! {
                Dashboard { owner: ctx.owner.clone(), repo: ctx.repo.clone() }
                IssueLinker { owner: ctx.owner, repo: ctx.repo, pr_number: ctx.pr_number }
            }
        }
        _ => rsx! { div { "Unknown page type" } },
    }
}
