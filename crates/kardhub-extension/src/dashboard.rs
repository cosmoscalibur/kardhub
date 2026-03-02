//! Kanban dashboard Dioxus component.
//!
//! Renders a board with columns mirroring the `kardhub-core::mapping` rules.
//! Receives raw issue/PR data via the background worker bridge, maps them
//! to cards using the core mapping engine, and displays them.

use dioxus::prelude::*;

use kardhub_core::mapping::{MappingConfig, map_card};
use kardhub_core::models::{
    Card, CardSource, CiStatus, Column, Issue, IssueState, Label, PullRequest,
};

use crate::bridge;

/// Props for the [`Dashboard`] component.
#[derive(Props, Clone, PartialEq)]
pub struct DashboardProps {
    /// Repository owner.
    pub owner: String,
    /// Repository name.
    pub repo: String,
}

/// Kanban board dashboard component.
///
/// Fetches cards from the background worker, maps them to columns using
/// `kardhub-core`, and renders a multi-column board.
#[component]
pub fn Dashboard(props: DashboardProps) -> Element {
    let mut cards = use_signal(Vec::<Card>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| Option::<String>::None);

    let owner = props.owner.clone();
    let repo = props.repo.clone();

    // Fetch cards on mount
    use_effect(move || {
        let owner = owner.clone();
        let repo = repo.clone();
        spawn(async move {
            loading.set(true);
            error.set(None);

            match bridge::check_token().await {
                Ok(false) => {
                    error.set(Some(
                        "No GitHub token configured. Click the KardHub extension icon to set up."
                            .to_string(),
                    ));
                    loading.set(false);
                    return;
                }
                Err(e) => {
                    error.set(Some(format!("Token check failed: {e}")));
                    loading.set(false);
                    return;
                }
                _ => {}
            }

            match bridge::fetch_cards_raw(&owner, &repo).await {
                Ok(data) => {
                    let parsed = parse_cards_from_js(data, &owner, &repo);
                    cards.set(parsed);
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });

    // Column definitions
    let columns = column_defs();
    let all_cards = cards();

    if loading() {
        return rsx! {
            div { class: "kardhub-loading",
                div { class: "kardhub-spinner" }
                "Loading board…"
            }
        };
    }

    if let Some(err) = error() {
        return rsx! {
            div { class: "kardhub-error", "{err}" }
        };
    }

    rsx! {
        div { class: "kardhub-board",
            for col in columns.iter() {
                {
                    let col_cards: Vec<&Card> = all_cards
                        .iter()
                        .filter(|c| c.column.name == col.name && c.column.emoji == col.emoji)
                        .collect();
                    rsx! {
                        BoardColumn {
                            column: col.clone(),
                            cards: col_cards.into_iter().cloned().collect(),
                            owner: props.owner.clone(),
                            repo: props.repo.clone(),
                        }
                    }
                }
            }
        }
    }
}

/// Props for [`BoardColumn`].
#[derive(Props, Clone, PartialEq)]
struct BoardColumnProps {
    column: Column,
    cards: Vec<Card>,
    owner: String,
    repo: String,
}

/// A single Kanban column.
#[component]
fn BoardColumn(props: BoardColumnProps) -> Element {
    rsx! {
        div { class: "kardhub-column",
            div { class: "kardhub-column-header",
                span { class: "kardhub-column-emoji", "{props.column.emoji}" }
                span { class: "kardhub-column-name", "{props.column.name}" }
                span { class: "kardhub-column-count", "{props.cards.len()}" }
            }
            div { class: "kardhub-column-cards",
                for card in props.cards.iter() {
                    {
                        let (num, title, is_pr) = match &card.source {
                            CardSource::Issue(i) => (i.number, i.title.clone(), false),
                            CardSource::PullRequest(pr) => (pr.number, pr.title.clone(), true),
                        };
                        let icon = if is_pr { "🔀" } else { "📋" };
                        let path = if is_pr { "pull" } else { "issues" };
                        let url = format!(
                            "https://github.com/{}/{}/{path}/{num}",
                            props.owner, props.repo,
                        );
                        let labels: Vec<&Label> = match &card.source {
                            CardSource::Issue(i) => i.labels.iter().collect(),
                            CardSource::PullRequest(pr) => pr.labels.iter().collect(),
                        };

                        rsx! {
                            div { class: "kardhub-card",
                                div { class: "kardhub-card-header",
                                    span { class: "kardhub-card-icon", "{icon}" }
                                    a {
                                        class: "kardhub-card-number",
                                        href: "{url}",
                                        target: "_self",
                                        "#{num}",
                                    }
                                }
                                div { class: "kardhub-card-title", "{title}" }
                                if !labels.is_empty() {
                                    div { class: "kardhub-card-labels",
                                        for label in labels.iter() {
                                            span {
                                                class: "kardhub-label",
                                                style: "background:#{label.color}",
                                                "{label.name}",
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Produce the column definitions list in display order.
fn column_defs() -> Vec<Column> {
    vec![
        Column {
            emoji: "🧊".into(),
            name: "Icebox".into(),
            sort_order: 0,
        },
        Column {
            emoji: "⏳".into(),
            name: "Prebacklog".into(),
            sort_order: 1,
        },
        Column {
            emoji: "📥".into(),
            name: "Backlog".into(),
            sort_order: 2,
        },
        Column {
            emoji: "❌".into(),
            name: "Failed".into(),
            sort_order: 3,
        },
        Column {
            emoji: "🚧".into(),
            name: "In Progress".into(),
            sort_order: 4,
        },
        Column {
            emoji: "👀".into(),
            name: "Code review".into(),
            sort_order: 5,
        },
        Column {
            emoji: "⏳".into(),
            name: "QA Backlog".into(),
            sort_order: 6,
        },
        Column {
            emoji: "🔍".into(),
            name: "QA Review".into(),
            sort_order: 7,
        },
        Column {
            emoji: "☑\u{fe0f}".into(),
            name: "Ready for STG".into(),
            sort_order: 8,
        },
        Column {
            emoji: "✅".into(),
            name: "Ready for deploy".into(),
            sort_order: 9,
        },
        Column {
            emoji: "📦".into(),
            name: "In Release".into(),
            sort_order: 10,
        },
    ]
}

/// Parse raw JS card data into domain `Card` objects using `map_card`.
fn parse_cards_from_js(data: wasm_bindgen::JsValue, owner: &str, repo: &str) -> Vec<Card> {
    // Serialise JsValue to JSON string, then deserialise
    let json_str = match js_sys::JSON::stringify(&data) {
        Ok(s) => s.as_string().unwrap_or_default(),
        Err(_) => return Vec::new(),
    };

    #[derive(serde::Deserialize)]
    struct RawData {
        issues: Vec<RawIssue>,
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
    }

    #[derive(serde::Deserialize)]
    struct RawHead {
        #[serde(rename = "ref")]
        branch_ref: String,
    }

    let raw: RawData = match serde_json::from_str(&json_str) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let config = MappingConfig::default();
    let mut cards = Vec::new();

    // Issues → cards (filter out PR entries via pull_request field absence)
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

    // PRs → cards
    for rp in raw.prs {
        let merged = rp.merged_at.is_some();
        let closed = rp.state == "closed" && !merged;
        let author = rp.user.map(|u| u.login).unwrap_or_default();
        let assignees: Vec<String> = rp.assignees.into_iter().map(|u| u.login).collect();
        let assignees = if assignees.is_empty() {
            vec![author.clone()]
        } else {
            assignees
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
    cards
}
