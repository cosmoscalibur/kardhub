//! Card detail panel shown on the right side when a card is clicked.
//!
//! Read mode: renders body and comments as GitHub-Flavoured Markdown,
//! shows PR reviewer status, CI badge, and open/copy link actions.

use cardman_core::github::RestClient;
use cardman_core::markdown::markdown_to_html;
use cardman_core::models::{Card, CardSource, CiStatus, Comment, IssueState, ReviewState, User};
use dioxus::prelude::*;

use super::card::label_style;

/// Properties for the [`CardDetail`] component.
#[derive(Props, Clone, PartialEq)]
pub struct CardDetailProps {
    /// The card to display.
    pub card: Card,
    /// GitHub personal access token for API calls.
    pub token: String,
    /// Authenticated user login (for edit permissions).
    pub user_login: String,
    /// Callback to close the panel.
    pub on_close: EventHandler<()>,
}

/// Right-side detail panel showing full card information.
#[component]
pub fn CardDetail(props: CardDetailProps) -> Element {
    let card = &props.card;
    let on_close = props.on_close;
    let token = props.token.clone();
    let owner = card.owner.clone();
    let repo = card.repo.clone();

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

    // Render body markdown to HTML.
    let body_html = if body_md.is_empty() {
        String::new()
    } else {
        markdown_to_html(body_md)
    };

    // GitHub URL for this issue/PR.
    let gh_url = format!(
        "https://github.com/{owner}/{repo}/{}/{number}",
        if is_pr { "pull" } else { "issues" }
    );
    let gh_url_copy = gh_url.clone();

    // Lazy-load comments on first open.
    let comments: Signal<Option<Vec<Comment>>> = use_signal(|| None);
    let loading_comments = use_signal(|| false);
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
                    // Requested reviewers with status
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

                    // CI status (shown if reviewers all approved)
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

                // Body / Description (rendered markdown)
                div { class: "detail-section",
                    div { class: "detail-section-title", "Description" }
                    if body_html.is_empty() {
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

                // Comments
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
                                    div { class: "detail-comment",
                                        div { class: "detail-comment-header",
                                            {render_user_badge(&comment.user)}
                                            span { class: "detail-comment-time",
                                                "{relative_time(&comment.created_at)}"
                                            }
                                        }
                                        div {
                                            class: "detail-comment-body detail-markdown",
                                            dangerous_inner_html: "{markdown_to_html(&comment.body)}",
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
