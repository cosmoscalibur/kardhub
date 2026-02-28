//! Card detail panel shown on the right side when a card is clicked.
//!
//! Read mode: renders body and comments as GitHub-Flavoured Markdown,
//! shows PR reviewer status, CI badge, and open/copy link actions.
//! Write mode: edit body, add/edit own comments with markdown preview.

use cardman_core::github::RestClient;
use cardman_core::markdown::markdown_to_html;
use cardman_core::models::{Card, CardSource, CiStatus, Comment, IssueState, ReviewState, User};
use dioxus::prelude::*;

use super::card::label_style;
use super::markdown_editor::MarkdownEditor;

/// Properties for the [`CardDetail`] component.
#[derive(Props, Clone, PartialEq)]
pub struct CardDetailProps {
    /// The card to display.
    pub card: Card,
    /// GitHub personal access token for API calls.
    pub token: String,
    /// Authenticated user login (for edit permissions).
    pub user_login: String,
    /// Member `(login, display_name)` pairs for `@` autocomplete.
    #[props(default = Vec::new())]
    pub members: Vec<(String, Option<String>)>,
    /// Card `(number, title)` pairs for `#` autocomplete.
    #[props(default = Vec::new())]
    pub cards: Vec<(u64, String)>,
    /// Callback to close the panel.
    pub on_close: EventHandler<()>,
}

/// Right-side detail panel showing full card information.
#[component]
pub fn CardDetail(props: CardDetailProps) -> Element {
    let card = &props.card;
    let on_close = props.on_close;
    let token = props.token.clone();
    let user_login = props.user_login.clone();
    let owner = card.owner.clone();
    let repo = card.repo.clone();
    let members_ac = props.members.clone();
    let cards_ac = props.cards.clone();

    // Extract common fields from the card source.
    let (number, title, body_md, labels, assignees, state_label, state_class) = match &card.source {
        CardSource::Issue(issue) => {
            let (sl, sc) = match issue.state {
                IssueState::Open => ("Open", "open"),
                IssueState::Closed => ("Closed", "closed"),
            };
            (
                issue.number,
                issue.title.as_str(),
                issue.body.as_deref().unwrap_or(""),
                issue.labels.as_slice(),
                issue.assignees.as_slice(),
                sl,
                sc,
            )
        }
        CardSource::PullRequest(pr) => {
            let (sl, sc) = if pr.merged {
                ("Merged", "merged")
            } else if pr.closed {
                ("Closed", "closed")
            } else {
                ("Open", "open")
            };
            (
                pr.number,
                pr.title.as_str(),
                pr.body.as_deref().unwrap_or(""),
                pr.labels.as_slice(),
                pr.assignees.as_slice(),
                sl,
                sc,
            )
        }
    };

    let is_pr = matches!(&card.source, CardSource::PullRequest(_));
    let type_label = if is_pr { "Pull Request" } else { "Issue" };

    // Determine if item is open (editable).
    let is_open = match &card.source {
        CardSource::Issue(i) => i.state == IssueState::Open,
        CardSource::PullRequest(pr) => !pr.merged && !pr.closed,
    };

    // Permission: can edit body?
    let can_edit_body = is_open
        && match &card.source {
            CardSource::Issue(_) => true,
            CardSource::PullRequest(pr) => {
                pr.author.login == user_login || pr.assignees.iter().any(|a| a.login == user_login)
            }
        };

    // Render body markdown to HTML.
    let body_html = if body_md.is_empty() {
        String::new()
    } else {
        markdown_to_html(body_md, &owner, &repo)
    };

    // GitHub URL for this issue/PR.
    let gh_url = format!(
        "https://github.com/{owner}/{repo}/{}/{number}",
        if is_pr { "pull" } else { "issues" }
    );
    let gh_url_copy = gh_url.clone();

    // ── Signals ────────────────────────────────────────────────────
    let comments: Signal<Option<Vec<Comment>>> = use_signal(|| None);
    let loading_comments = use_signal(|| false);
    let mut editing_body = use_signal(|| false);
    let mut body_draft = use_signal(|| body_md.to_string());
    let mut saving_body = use_signal(|| false);
    let mut new_comment_text = use_signal(String::new);
    let mut saving_comment = use_signal(|| false);
    let mut editing_comment_id: Signal<Option<u64>> = use_signal(|| None);
    let mut comment_edit_text = use_signal(String::new);
    let mut saving_edit_comment = use_signal(|| false);

    // Lazy-load comments.
    {
        let mut comments = comments;
        let mut loading_comments = loading_comments;
        let token = token.clone();
        let owner = owner.clone();
        let repo = repo.clone();

        use_effect(move || {
            let token = token.clone();
            let owner = owner.clone();
            let repo = repo.clone();
            spawn(async move {
                loading_comments.set(true);
                let client = RestClient::new(token.clone());
                match client.list_comments(&owner, &repo, number).await {
                    Ok(c) => comments.set(Some(c)),
                    Err(_) => comments.set(Some(Vec::new())),
                }
                loading_comments.set(false);
            });
        });
    }

    // Filter out priority labels for display.
    let display_labels: Vec<_> = labels
        .iter()
        .filter(|l| cardman_core::models::Priority::from_label(&l.name).is_none())
        .collect();

    // Clones for closures.
    let token_body = token.clone();
    let owner_body = owner.clone();
    let repo_body = repo.clone();
    let token_add = token.clone();
    let owner_add = owner.clone();
    let repo_add = repo.clone();
    let token_edit = token.clone();
    let owner_edit = owner.clone();
    let repo_edit = repo.clone();

    rsx! {
        // Backdrop overlay
        div {
            class: "detail-overlay",
            onclick: move |_| on_close.call(()),
        }

        // Panel
        div { class: "detail-panel",
            // Header
            div { class: "detail-header",
                span { class: "detail-type", "{type_label}" }
                div { class: "detail-actions",
                    button {
                        class: "detail-action-btn",
                        title: "Copy link",
                        onclick: move |_| {
                            let url = gh_url_copy.clone();
                            spawn(async move {
                                let _ = dioxus::document::eval(
                                    &format!("navigator.clipboard.writeText('{url}')")
                                );
                            });
                        },
                        "📋"
                    }
                    a {
                        class: "detail-action-btn",
                        href: "{gh_url}",
                        target: "_blank",
                        title: "Open in browser",
                        "🔗"
                    }
                    button {
                        class: "detail-close",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }
            }

            // Body
            div { class: "detail-body",
                div { class: "detail-title", "{title}" }
                div { class: "detail-number", "#{number}" }

                // State
                div { class: "detail-section",
                    div { class: "detail-section-title", "Status" }
                    span { class: "detail-state {state_class}", "{state_label}" }
                }

                // Priority
                if let Some(priority) = &card.priority {
                    div { class: "detail-section",
                        div { class: "detail-section-title", "Priority" }
                        span { class: "card-priority", "#{priority.0}" }
                    }
                }

                // Labels
                if !display_labels.is_empty() {
                    div { class: "detail-section",
                        div { class: "detail-section-title", "Labels" }
                        div { class: "detail-labels",
                            for label in &display_labels {
                                span {
                                    class: "card-label",
                                    style: "{label_style(&label.color)}",
                                    "{label.name}"
                                }
                            }
                        }
                    }
                }

                // Assignees
                if !assignees.is_empty() {
                    div { class: "detail-section",
                        div { class: "detail-section-title", "Assignees" }
                        div { class: "detail-assignees",
                            for assignee in assignees {
                                {render_user_badge(assignee)}
                            }
                        }
                    }
                }

                // PR-specific: reviewers and CI
                if let CardSource::PullRequest(pr) = &card.source {
                    if !pr.requested_reviewers.is_empty() {
                        div { class: "detail-section",
                            div { class: "detail-section-title", "Reviewers" }
                            div { class: "detail-reviewers",
                                for reviewer in &pr.requested_reviewers {
                                    {render_reviewer_badge(reviewer, &pr.reviews)}
                                }
                            }
                        }
                    }

                    {
                        let all_approved = !pr.reviews.is_empty()
                            && pr.reviews.iter().all(|r| r.state == ReviewState::Approved);
                        rsx! {
                            if all_approved {
                                div { class: "detail-section",
                                    div { class: "detail-section-title", "CI Status" }
                                    span {
                                        class: "detail-ci-badge {ci_class(pr.ci_status)}",
                                        "{ci_label(pr.ci_status)}"
                                    }
                                }
                            }
                        }
                    }
                }

                // ── Body / Description ──────────────────────────────
                div { class: "detail-section",
                    div { class: "detail-section-header",
                        div { class: "detail-section-title", "Description" }
                        if can_edit_body && !editing_body() {
                            button {
                                class: "detail-edit-btn",
                                onclick: move |_| editing_body.set(true),
                                "✏️ Edit"
                            }
                        }
                    }
                    if editing_body() {
                        MarkdownEditor {
                            value: body_draft(),
                            placeholder: "Describe the issue…",
                            owner: owner.clone(),
                            repo: repo.clone(),
                            members: members_ac.clone(),
                            cards: cards_ac.clone(),
                            on_change: move |v: String| body_draft.set(v),
                        }
                        div { class: "detail-edit-actions",
                            button {
                                class: "modal-btn modal-btn-secondary",
                                onclick: move |_| editing_body.set(false),
                                "Cancel"
                            }
                            button {
                                class: "modal-btn modal-btn-primary",
                                disabled: saving_body(),
                                onclick: move |_| {
                                    let token = token_body.clone();
                                    let owner = owner_body.clone();
                                    let repo = repo_body.clone();
                                    let text = body_draft().clone();
                                    let is_pr_val = is_pr;
                                    spawn(async move {
                                        saving_body.set(true);
                                        let client = RestClient::new(token);
                                        let ok = if is_pr_val {
                                            client.update_pr(&owner, &repo, number, None, Some(&text)).await.is_ok()
                                        } else {
                                            let update = cardman_core::github::IssueUpdate {
                                                title: None,
                                                body: Some(text.clone()),
                                                state: None,
                                                labels: None,
                                            };
                                            client.update_issue(&owner, &repo, number, &update).await.is_ok()
                                        };
                                        if ok {
                                            editing_body.set(false);
                                        }
                                        saving_body.set(false);
                                    });
                                },
                                if saving_body() { "Saving…" } else { "Save" }
                            }
                        }
                    } else if body_html.is_empty() {
                        div { class: "detail-content detail-empty",
                            "No description provided."
                        }
                    } else {
                        div {
                            class: "detail-content detail-markdown",
                            dangerous_inner_html: "{body_html}",
                        }
                    }
                }

                // ── Comments ────────────────────────────────────────
                div { class: "detail-section",
                    div { class: "detail-section-title", "Comments" }
                    if loading_comments() {
                        div { class: "detail-loading", "Loading comments…" }
                    } else if let Some(comment_list) = comments() {
                        if comment_list.is_empty() {
                            div { class: "detail-empty", "No comments." }
                        } else {
                            div { class: "detail-comments",
                                for comment in &comment_list {
                                    {
                                        let is_own = comment.user.login == user_login;
                                        let cid = comment.id;
                                        let is_editing = editing_comment_id() == Some(cid);
                                        let comment_body_for_edit = comment.body.clone();
                                        let token_ec = token_edit.clone();
                                        let owner_ec = owner_edit.clone();
                                        let repo_ec = repo_edit.clone();
                                        rsx! {
                                            div { class: "detail-comment",
                                                div { class: "detail-comment-header",
                                                    {render_user_badge(&comment.user)}
                                                    div { class: "detail-comment-meta",
                                                        span { class: "detail-comment-time",
                                                            "{relative_time(&comment.created_at)}"
                                                        }
                                                        if is_own && is_open && !is_editing {
                                                            button {
                                                                class: "detail-edit-btn",
                                                                onclick: move |_| {
                                                                    editing_comment_id.set(Some(cid));
                                                                    comment_edit_text.set(comment_body_for_edit.clone());
                                                                },
                                                                "✏️"
                                                            }
                                                        }
                                                    }
                                                }
                                                if is_editing {
                                                    div { class: "detail-comment-edit",
                                                        MarkdownEditor {
                                                            value: comment_edit_text(),
                                                            placeholder: "Edit comment…",
                                                            owner: owner.clone(),
                                                            repo: repo.clone(),
                                                            members: members_ac.clone(),
                                                            cards: cards_ac.clone(),
                                                            on_change: move |v: String| comment_edit_text.set(v),
                                                        }
                                                        div { class: "detail-edit-actions",
                                                            button {
                                                                class: "modal-btn modal-btn-secondary",
                                                                onclick: move |_| editing_comment_id.set(None),
                                                                "Cancel"
                                                            }
                                                            button {
                                                                class: "modal-btn modal-btn-primary",
                                                                disabled: saving_edit_comment(),
                                                                onclick: move |_| {
                                                                    let token = token_ec.clone();
                                                                    let owner = owner_ec.clone();
                                                                    let repo = repo_ec.clone();
                                                                    let text = comment_edit_text();
                                                                    let mut comments = comments;
                                                                    spawn(async move {
                                                                        saving_edit_comment.set(true);
                                                                        let client = RestClient::new(token.clone());
                                                                        if let Ok(updated) = client.update_comment(&owner, &repo, cid, &text).await
                                                                            && let Some(ref mut list) = *comments.write()
                                                                            && let Some(c) = list.iter_mut().find(|c| c.id == cid)
                                                                        {
                                                                            *c = updated;
                                                                        }
                                                                        editing_comment_id.set(None);
                                                                        saving_edit_comment.set(false);
                                                                    });
                                                                },
                                                                if saving_edit_comment() { "Saving…" } else { "Save" }
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    div {
                                                        class: "detail-comment-body detail-markdown",
                                                        dangerous_inner_html: "{markdown_to_html(&comment.body, &owner, &repo)}",
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Add new comment
                        if is_open {
                            div { class: "detail-new-comment",
                                MarkdownEditor {
                                    value: new_comment_text(),
                                    placeholder: "Add a comment…",
                                    owner: owner.clone(),
                                    repo: repo.clone(),
                                    members: members_ac.clone(),
                                    cards: cards_ac.clone(),
                                    on_change: move |v: String| new_comment_text.set(v),
                                }
                                div { class: "detail-edit-actions",
                                    button {
                                        class: "modal-btn modal-btn-primary",
                                        disabled: new_comment_text().trim().is_empty() || saving_comment(),
                                        onclick: move |_| {
                                            let token = token_add.clone();
                                            let owner = owner_add.clone();
                                            let repo = repo_add.clone();
                                            let text = new_comment_text().trim().to_string();
                                            let mut comments = comments;
                                            spawn(async move {
                                                saving_comment.set(true);
                                                let client = RestClient::new(token);
                                                if let Ok(new_c) = client.add_comment(&owner, &repo, number, &text).await {
                                                    if let Some(ref mut list) = *comments.write() {
                                                        list.push(new_c);
                                                    }
                                                    new_comment_text.set(String::new());
                                                }
                                                saving_comment.set(false);
                                            });
                                        },
                                        if saving_comment() { "Adding…" } else { "Comment" }
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

/// Render a user avatar + login badge.
fn render_user_badge(user: &User) -> Element {
    rsx! {
        span { class: "detail-user-badge",
            if !user.avatar_url.is_empty() {
                img {
                    class: "detail-user-avatar",
                    src: "{user.avatar_url}",
                    alt: "{user.login}",
                }
            } else {
                span { class: "detail-user-initial",
                    "{user.login.chars().next().unwrap_or('?').to_uppercase()}"
                }
            }
            span { class: "detail-user-login", "@{user.login}" }
        }
    }
}

/// Render a reviewer badge with approval status from reviews.
fn render_reviewer_badge(reviewer: &User, reviews: &[cardman_core::models::Review]) -> Element {
    let status = reviews
        .iter()
        .filter(|r| r.user.login == reviewer.login)
        .next_back()
        .map(|r| &r.state);

    let (icon, class) = match status {
        Some(ReviewState::Approved) => ("✅", "reviewer-approved"),
        Some(ReviewState::ChangesRequested) => ("❌", "reviewer-changes"),
        Some(ReviewState::Commented) => ("💬", "reviewer-commented"),
        _ => ("⏳", "reviewer-pending"),
    };

    rsx! {
        span { class: "detail-reviewer {class}",
            {render_user_badge(reviewer)}
            span { class: "reviewer-status", "{icon}" }
        }
    }
}

/// Map CI status to CSS class.
fn ci_class(status: CiStatus) -> &'static str {
    match status {
        CiStatus::Success => "ci-success",
        CiStatus::Failure => "ci-failure",
        CiStatus::Pending => "ci-pending",
    }
}

/// Map CI status to display label.
fn ci_label(status: CiStatus) -> &'static str {
    match status {
        CiStatus::Success => "✅ Passing",
        CiStatus::Failure => "❌ Failed",
        CiStatus::Pending => "⏳ Pending",
    }
}

/// Format a DateTime as a relative time string (e.g. "2h ago", "3d ago").
fn relative_time(dt: &chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let diff = now.signed_duration_since(*dt);

    let minutes = diff.num_minutes();
    if minutes < 1 {
        return "just now".to_string();
    }
    if minutes < 60 {
        return format!("{minutes}m ago");
    }
    let hours = diff.num_hours();
    if hours < 24 {
        return format!("{hours}h ago");
    }
    let days = diff.num_days();
    if days < 30 {
        return format!("{days}d ago");
    }
    let months = days / 30;
    if months < 12 {
        return format!("{months}mo ago");
    }
    let years = days / 365;
    format!("{years}y ago")
}
