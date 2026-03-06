//! Card detail panel shown on the right side when a card is clicked.
//!
//! Read mode: renders body and comments as GitHub-Flavoured Markdown,
//! shows PR reviewer status, CI badge, and open/copy link actions.
//! Write mode: edit body, add/edit own comments with markdown preview.

use dioxus::prelude::*;
use kardhub_core::github::RestClient;
use kardhub_core::markdown::markdown_to_html;
use kardhub_core::models::{
    Card, CardSource, CiStatus, Comment, IssueState, Label, ReviewState, User,
};

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
    /// Cached members for resolving login → avatar.
    #[props(default = Vec::new())]
    pub members: Vec<User>,
    /// Card `(number, title)` pairs for `#` autocomplete.
    #[props(default = Vec::new())]
    pub cards: Vec<(u64, String)>,
    /// Available repository labels for editing.
    #[props(default = Vec::new())]
    pub repo_labels: Vec<Label>,
    /// Callback to close the panel.
    pub on_close: EventHandler<()>,
    /// Callback when the issue/PR is closed (provides updated card for local move).
    pub on_closed: EventHandler<Card>,
    /// Callback when fresh data is fetched from the API (card re-synced).
    pub on_synced: EventHandler<Card>,
}

/// Right-side detail panel showing full card information.
#[component]
pub fn CardDetail(props: CardDetailProps) -> Element {
    let card = &props.card;
    let on_close = props.on_close;
    let on_closed = props.on_closed;
    let token = props.token.clone();
    let user_login = props.user_login.clone();
    let owner = card.owner.clone();
    let repo = card.repo.clone();
    let members_list = props.members.clone();
    let cards_ac = props.cards.clone();
    let repo_labels = props.repo_labels.clone();
    // Login strings for autocomplete.
    let members_ac: Vec<String> = members_list.iter().map(|u| u.login.clone()).collect();

    // Extract common fields from the card source.
    let (number, title, body_md, labels, assignee_logins, author, state_label, state_class) =
        match &card.source {
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
                    issue.author.as_str(),
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
                    pr.author.as_str(),
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
                pr.author == user_login || pr.assignees.contains(&user_login)
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
    let mut closing = use_signal(|| false);
    let mut close_error: Signal<Option<String>> = use_signal(|| None);

    // Inline editing signals.
    let mut editing_title = use_signal(|| false);
    let mut title_draft = use_signal(|| title.to_string());
    let mut saving_title = use_signal(|| false);
    let mut editing_priority = use_signal(|| false);
    let mut priority_draft: Signal<u8> = use_signal(|| card.priority.map_or(1, |p| p.0));
    let mut saving_priority = use_signal(|| false);
    let mut editing_labels = use_signal(|| false);
    let mut selected_labels: Signal<Vec<String>> = use_signal(|| {
        // Only track non-priority labels; priority labels are managed separately.
        labels
            .iter()
            .filter(|l| kardhub_core::models::Priority::from_label(&l.name).is_none())
            .map(|l| l.name.clone())
            .collect()
    });
    let mut saving_labels = use_signal(|| false);
    let mut editing_assignees = use_signal(|| false);
    let mut selected_assignees: Signal<Vec<String>> = use_signal(|| assignee_logins.to_vec());
    let mut saving_assignees = use_signal(|| false);
    let mut syncing = use_signal(|| true);

    // Auto-sync: fetch fresh data from the API on mount.
    {
        let token = token.clone();
        let owner = owner.clone();
        let repo = repo.clone();
        let on_synced = props.on_synced;

        use_effect(move || {
            let token = token.clone();
            let owner = owner.clone();
            let repo = repo.clone();
            spawn(async move {
                syncing.set(true);
                let client = RestClient::new(token);
                let config = kardhub_core::mapping::MappingConfig::default();
                let result = if is_pr {
                    client.get_pr(&owner, &repo, number).await.map(|pr| {
                        kardhub_core::mapping::map_card(
                            &owner,
                            &repo,
                            CardSource::PullRequest(pr),
                            &config,
                        )
                    })
                } else {
                    client.get_issue(&owner, &repo, number).await.map(|issue| {
                        kardhub_core::mapping::map_card(
                            &owner,
                            &repo,
                            CardSource::Issue(issue),
                            &config,
                        )
                    })
                };
                if let Ok(fresh) = result {
                    on_synced.call(fresh);
                }
                syncing.set(false);
            });
        });
    }

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
        .filter(|l| kardhub_core::models::Priority::from_label(&l.name).is_none())
        .collect();

    // Owned list of priority label names (for preserving in label save).
    let priority_label_names: Vec<String> = labels
        .iter()
        .filter(|l| kardhub_core::models::Priority::from_label(&l.name).is_some())
        .map(|l| l.name.clone())
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
                if syncing() {
                    span { class: "detail-syncing", "⟳" }
                }
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
                // ── Title (editable) ───────────────────────────────
                if editing_title() {
                    div { class: "detail-title-edit",
                        input {
                            class: "modal-input",
                            r#type: "text",
                            value: "{title_draft}",
                            oninput: move |e| title_draft.set(e.value()),
                        }
                        div { class: "detail-edit-actions",
                            button {
                                class: "modal-btn modal-btn-secondary",
                                onclick: move |_| editing_title.set(false),
                                "Cancel"
                            }
                            button {
                                class: "modal-btn modal-btn-primary",
                                disabled: saving_title() || title_draft().trim().is_empty(),
                                onclick: {
                                    let token = token.clone();
                                    let owner = owner.clone();
                                    let repo = repo.clone();
                                    #[allow(clippy::redundant_locals)]
                                    let is_pr = is_pr;
                                    move |_| {
                                        let token = token.clone();
                                        let owner = owner.clone();
                                        let repo = repo.clone();
                                        let new_title = title_draft().trim().to_string();
                                        spawn(async move {
                                            saving_title.set(true);
                                            let client = RestClient::new(token);
                                            let ok = if is_pr {
                                                client.update_pr(&owner, &repo, number, Some(&new_title), None, None).await.is_ok()
                                            } else {
                                                let update = kardhub_core::github::IssueUpdate {
                                                    title: Some(new_title),
                                                    ..Default::default()
                                                };
                                                client.update_issue(&owner, &repo, number, &update).await.is_ok()
                                            };
                                            if ok {
                                                editing_title.set(false);
                                            }
                                            saving_title.set(false);
                                        });
                                    }
                                },
                                if saving_title() { "Saving…" } else { "Save" }
                            }
                        }
                    }
                } else {
                    div { class: "detail-title",
                        "{title}"
                        if can_edit_body {
                            button {
                                class: "detail-edit-btn",
                                onclick: move |_| editing_title.set(true),
                                "✏️"
                            }
                        }
                    }
                }
                div { class: "detail-number", "#{number}" }

                // State
                div { class: "detail-section",
                    div { class: "detail-section-title", "Status" }
                    span { class: "detail-state {state_class}", "{state_label}" }
                }

                // Author
                div { class: "detail-section",
                    div { class: "detail-section-title", "Author" }
                    {render_user_badge(&resolve_login(author, &members_list))}
                }

                // Priority (editable, issues only)
                if !is_pr {
                    div { class: "detail-section",
                        div { class: "detail-section-header",
                            div { class: "detail-section-title", "Priority" }
                            if is_open && !editing_priority() {
                                button {
                                    class: "detail-edit-btn",
                                    onclick: move |_| editing_priority.set(true),
                                    "✏️"
                                }
                            }
                        }
                        if editing_priority() {
                            div { class: "priority-select",
                                select {
                                    value: "{priority_draft}",
                                    onchange: move |e| {
                                        if let Ok(v) = e.value().parse::<u8>() {
                                            priority_draft.set(v);
                                        }
                                    },
                                    for i in 1u8..=6 {
                                        option { value: "{i}", selected: priority_draft() == i, "#{i}" }
                                    }
                                }
                            }
                            div { class: "detail-edit-actions",
                                button {
                                    class: "modal-btn modal-btn-secondary",
                                    onclick: move |_| editing_priority.set(false),
                                    "Cancel"
                                }
                                button {
                                    class: "modal-btn modal-btn-primary",
                                    disabled: saving_priority(),
                                    onclick: {
                                        let token = token.clone();
                                        let owner = owner.clone();
                                        let repo = repo.clone();
                                        let priority_label_names = priority_label_names.clone();
                                        move |_| {
                                            let token = token.clone();
                                            let owner = owner.clone();
                                            let repo = repo.clone();
                                            let new_priority = priority_draft();
                                            let new_priority_label = format!("#{new_priority}");
                                            // Build updated label list: remove old priority, add new.
                                            let mut all_labels: Vec<String> = selected_labels()
                                                .into_iter()
                                                .chain(std::iter::once(new_priority_label))
                                                .collect();
                                            // Remove any old priority labels that aren't the new one.
                                            for old in &priority_label_names {
                                                all_labels.retain(|l| l != old);
                                            }
                                            // Re-add the new priority.
                                            all_labels.push(format!("#{new_priority}"));
                                            all_labels.dedup();
                                            spawn(async move {
                                                saving_priority.set(true);
                                                let client = RestClient::new(token);
                                                let update = kardhub_core::github::IssueUpdate {
                                                    labels: Some(all_labels),
                                                    ..Default::default()
                                                };
                                                if client.update_issue(&owner, &repo, number, &update).await.is_ok() {
                                                    editing_priority.set(false);
                                                }
                                                saving_priority.set(false);
                                            });
                                        }
                                    },
                                    if saving_priority() { "Saving…" } else { "Save" }
                                }
                            }
                        } else if let Some(priority) = &card.priority {
                            span { class: "card-priority", "#{priority.0}" }
                        } else {
                            div { class: "detail-empty", "Not set" }
                        }
                    }
                }

                // Labels (editable)
                div { class: "detail-section",
                    div { class: "detail-section-header",
                        div { class: "detail-section-title", "Labels" }
                        if is_open && !editing_labels() {
                            button {
                                class: "detail-edit-btn",
                                onclick: move |_| editing_labels.set(true),
                                "✏️"
                            }
                        }
                    }
                    if editing_labels() {
                        {
                            // Only show non-priority labels in the multi-select.
                            let editable_repo_labels: Vec<_> = repo_labels
                                .iter()
                                .filter(|l| kardhub_core::models::Priority::from_label(&l.name).is_none())
                                .collect();
                            rsx! {
                                div { class: "multi-select",
                                    for label in &editable_repo_labels {
                                        {
                                            let label_name = label.name.clone();
                                            let is_checked = selected_labels().contains(&label.name);
                                            rsx! {
                                                label { class: "multi-select-item",
                                                    input {
                                                        r#type: "checkbox",
                                                        checked: is_checked,
                                                        onchange: move |_| {
                                                            let mut current = selected_labels();
                                                            if let Some(pos) = current.iter().position(|l| l == &label_name) {
                                                                current.remove(pos);
                                                            } else {
                                                                current.push(label_name.clone());
                                                            }
                                                            selected_labels.set(current);
                                                        },
                                                    }
                                                    span {
                                                        class: "card-label",
                                                        style: "{label_style(&label.color)}",
                                                        "{label.name}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                div { class: "detail-edit-actions",
                                    button {
                                        class: "modal-btn modal-btn-secondary",
                                        onclick: move |_| editing_labels.set(false),
                                        "Cancel"
                                    }
                                    button {
                                        class: "modal-btn modal-btn-primary",
                                        disabled: saving_labels(),
                                        onclick: {
                                            let token = token.clone();
                                            let owner = owner.clone();
                                            let repo = repo.clone();
                                            move |_| {
                                                let token = token.clone();
                                                let owner = owner.clone();
                                                let repo = repo.clone();
                                                let new_labels = selected_labels();
                                                // Preserve existing priority labels when saving.
                                                let all_labels: Vec<String> = priority_label_names.iter().cloned().chain(new_labels).collect();
                                                spawn(async move {
                                                    saving_labels.set(true);
                                                    let client = RestClient::new(token);
                                                    let update = kardhub_core::github::IssueUpdate {
                                                        labels: Some(all_labels),
                                                        ..Default::default()
                                                    };
                                                    if client.update_issue(&owner, &repo, number, &update).await.is_ok() {
                                                        editing_labels.set(false);
                                                    }
                                                    saving_labels.set(false);
                                                });
                                            }
                                        },
                                        if saving_labels() { "Saving…" } else { "Save" }
                                    }
                                }
                            }
                        }
                    } else if display_labels.is_empty() {
                        div { class: "detail-empty", "No labels." }
                    } else {
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

                // Assignees (editable)
                div { class: "detail-section",
                    div { class: "detail-section-header",
                        div { class: "detail-section-title", "Assignees" }
                        if is_open && !editing_assignees() {
                            button {
                                class: "detail-edit-btn",
                                onclick: move |_| editing_assignees.set(true),
                                "✏️"
                            }
                        }
                    }
                    if editing_assignees() {
                        div { class: "multi-select",
                            for member in &members_list {
                                {
                                    let login = member.login.clone();
                                    let is_checked = selected_assignees().contains(&member.login);
                                    rsx! {
                                        label { class: "multi-select-item",
                                            input {
                                                r#type: "checkbox",
                                                checked: is_checked,
                                                onchange: move |_| {
                                                    let mut current = selected_assignees();
                                                    if let Some(pos) = current.iter().position(|l| l == &login) {
                                                        current.remove(pos);
                                                    } else {
                                                        current.push(login.clone());
                                                    }
                                                    selected_assignees.set(current);
                                                },
                                            }
                                            "{member.login}"
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "detail-edit-actions",
                            button {
                                class: "modal-btn modal-btn-secondary",
                                onclick: move |_| editing_assignees.set(false),
                                "Cancel"
                            }
                            button {
                                class: "modal-btn modal-btn-primary",
                                disabled: saving_assignees(),
                                onclick: {
                                    let token = token.clone();
                                    let owner = owner.clone();
                                    let repo = repo.clone();
                                    move |_| {
                                        let token = token.clone();
                                        let owner = owner.clone();
                                        let repo = repo.clone();
                                        let new_assignees = selected_assignees();
                                        spawn(async move {
                                            saving_assignees.set(true);
                                            let client = RestClient::new(token);
                                            let update = kardhub_core::github::IssueUpdate {
                                                assignees: Some(new_assignees),
                                                ..Default::default()
                                            };
                                            if client.update_issue(&owner, &repo, number, &update).await.is_ok() {
                                                editing_assignees.set(false);
                                            }
                                            saving_assignees.set(false);
                                        });
                                    }
                                },
                                if saving_assignees() { "Saving…" } else { "Save" }
                            }
                        }
                    } else if assignee_logins.is_empty() {
                        div { class: "detail-empty", "No assignees." }
                    } else {
                        div { class: "detail-assignees",
                            for login in assignee_logins {
                                {render_user_badge(&resolve_login(login, &members_list))}
                            }
                        }
                    }
                }

                // PR-specific: reviewers and CI
                if let CardSource::PullRequest(pr) = &card.source {
                    // Build unified reviewer list: submitted reviews + pending.
                    {
                        use std::collections::HashSet;
                        let reviewed: HashSet<&str> = pr.reviews.iter().map(|r| r.user.login.as_str()).collect();
                        let has_any = !pr.reviews.is_empty() || !pr.requested_reviewers.is_empty();
                        rsx! {
                            if has_any {
                                div { class: "detail-section",
                                    div { class: "detail-section-title", "Reviewers" }
                                    div { class: "detail-reviewers",
                                        // Submitted reviews
                                        for review in &pr.reviews {
                                            {render_reviewer_badge(&review.user.login, &pr.reviews, &members_list)}
                                        }
                                        // Pending reviewers (not yet submitted)
                                        for login in &pr.requested_reviewers {
                                            if !reviewed.contains(login.as_str()) {
                                                {render_reviewer_badge(login, &pr.reviews, &members_list)}
                                            }
                                        }
                                    }
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
                                            client.update_pr(&owner, &repo, number, None, Some(&text), None).await.is_ok()
                                        } else {
                                            let update = kardhub_core::github::IssueUpdate {
                                                title: None,
                                                body: Some(text.clone()),
                                                state: None,
                                                labels: None,
                                                assignees: None,
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
                                    // "Close with comment" button
                                    {
                                        let token_close = token.clone();
                                        let owner_close = owner.clone();
                                        let repo_close = repo.clone();
                                        #[allow(clippy::redundant_locals)]
                                        let is_pr_close = is_pr;
                                        let pr_branch = match &card.source {
                                            CardSource::PullRequest(pr) => pr.branch.clone(),
                                            _ => String::new(),
                                        };
                                        let card_for_close = card.clone();
                                        rsx! {
                                            button {
                                                class: "modal-btn close-with-comment-btn",
                                                disabled: new_comment_text().trim().is_empty() || closing(),
                                                onclick: move |_| {
                                                    let token = token_close.clone();
                                                    let owner = owner_close.clone();
                                                    let repo = repo_close.clone();
                                                    let comment = new_comment_text().trim().to_string();
                                                    let branch = pr_branch.clone();
                                                    let mut card_updated = card_for_close.clone();
                                                    spawn(async move {
                                                        closing.set(true);
                                                        close_error.set(None);
                                                        let client = RestClient::new(token);
                                                        // Add closing comment.
                                                        if let Err(e) = client.add_comment(&owner, &repo, number, &comment).await {
                                                            close_error.set(Some(format!("Failed to add comment: {e}")));
                                                            closing.set(false);
                                                            return;
                                                        }
                                                        // Close the item.
                                                        let close_ok = if is_pr_close {
                                                            client.close_pr(&owner, &repo, number, &branch).await.is_ok()
                                                        } else {
                                                            let update = kardhub_core::github::IssueUpdate {
                                                                state: Some(IssueState::Closed),
                                                                ..Default::default()
                                                            };
                                                            client.update_issue(&owner, &repo, number, &update).await.is_ok()
                                                        };
                                                        closing.set(false);
                                                        if close_ok {
                                                            // Update local card state to closed.
                                                            match &mut card_updated.source {
                                                                CardSource::Issue(i) => i.state = IssueState::Closed,
                                                                CardSource::PullRequest(pr) => pr.closed = true,
                                                            }
                                                            on_closed.call(card_updated);
                                                        } else {
                                                            close_error.set(Some("GitHub rejected the close request.".to_string()));
                                                        }
                                                    });
                                                },
                                                if closing() { "Closing…" } else { "Close with comment" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Error modal for close failures.
                if let Some(err) = close_error() {
                    div { class: "modal-error", "{err}" }
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

/// Resolve a login string to a `User` from the members list.
fn resolve_login(login: &str, members: &[User]) -> User {
    members
        .iter()
        .find(|u| u.login == login)
        .cloned()
        .unwrap_or_else(|| User {
            login: login.to_string(),
            avatar_url: String::new(),
        })
}

/// Render a reviewer badge with approval status from reviews.
fn render_reviewer_badge(
    login: &str,
    reviews: &[kardhub_core::models::Review],
    members: &[User],
) -> Element {
    let status = reviews
        .iter()
        .filter(|r| r.user.login == login)
        .next_back()
        .map(|r| &r.state);

    let (icon, class) = match status {
        Some(ReviewState::Approved) => ("✅", "reviewer-approved"),
        Some(ReviewState::ChangesRequested) => ("❌", "reviewer-changes"),
        Some(ReviewState::Commented) => ("💬", "reviewer-commented"),
        _ => ("⏳", "reviewer-pending"),
    };

    let user = resolve_login(login, members);
    rsx! {
        span { class: "detail-reviewer {class}",
            {render_user_badge(&user)}
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
