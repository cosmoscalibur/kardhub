//! Create issue panel shown on the right side (unified with detail panel).
//!
//! Provides a repo dropdown (when multiple repos are available), title,
//! priority, labels, assignees, and markdown body editor with autocomplete.

use dioxus::prelude::*;
use kardhub_core::github::RestClient;
use kardhub_core::models::Label;

use super::card::label_style;
use super::markdown_editor::MarkdownEditor;

/// Properties for the [`CreateIssue`] component.
#[derive(Props, Clone, PartialEq)]
pub struct CreateIssueProps {
    /// GitHub personal access token.
    pub token: String,
    /// Repository owner.
    pub owner: String,
    /// Available repository names to create the issue in.
    pub repos: Vec<String>,
    /// Member logins for `@` autocomplete.
    #[props(default = Vec::new())]
    pub members: Vec<String>,
    /// Card `(number, title)` pairs for `#` autocomplete.
    #[props(default = Vec::new())]
    pub cards: Vec<(u64, String)>,
    /// Available repository labels for selection.
    #[props(default = Vec::new())]
    pub repo_labels: Vec<Label>,
    /// Authenticated user login (default assignee).
    #[props(default = String::new())]
    pub user_login: String,
    /// Callback when issue is created (triggers board refresh).
    pub on_created: EventHandler<()>,
    /// Callback to close the panel.
    pub on_close: EventHandler<()>,
}

/// Right-side panel for creating a new GitHub issue.
#[component]
pub fn CreateIssue(props: CreateIssueProps) -> Element {
    let on_close = props.on_close;
    let on_created = props.on_created;
    let mut title = use_signal(String::new);
    let mut body = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);
    let mut selected_repo = use_signal(|| props.repos.first().cloned().unwrap_or_default());
    let mut priority: Signal<Option<u8>> = use_signal(|| None);
    let mut selected_labels: Signal<Vec<String>> = use_signal(Vec::new);
    let mut selected_assignees: Signal<Vec<String>> = use_signal(|| vec![props.user_login.clone()]);

    let token = props.token.clone();
    let owner = props.owner.clone();
    let repos = props.repos.clone();
    let members = props.members.clone();
    let cards = props.cards.clone();
    let repo_labels = props.repo_labels.clone();

    let can_save = !title().trim().is_empty() && priority().is_some() && !saving();

    rsx! {
        // Backdrop
        div {
            class: "detail-overlay",
            onclick: move |_| on_close.call(()),
        }

        // Right-side panel (same layout as CardDetail)
        div { class: "detail-panel",
            div { class: "detail-header",
                span { class: "detail-type", "NEW ISSUE" }
                div { class: "detail-actions",
                    button {
                        class: "detail-close",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }
            }

            div { class: "detail-body",
                // Repo selector (only when multiple repos available)
                if repos.len() > 1 {
                    div { class: "detail-section",
                        div { class: "detail-section-title", "REPOSITORY" }
                        select {
                            class: "modal-select",
                            value: "{selected_repo}",
                            onchange: move |e| selected_repo.set(e.value()),
                            for repo_name in &repos {
                                option { value: "{repo_name}", "{repo_name}" }
                            }
                        }
                    }
                }

                // Title
                div { class: "detail-section",
                    div { class: "detail-section-title", "TITLE *" }
                    input {
                        class: "modal-input",
                        r#type: "text",
                        placeholder: "Issue title",
                        value: "{title}",
                        oninput: move |e| title.set(e.value()),
                    }
                }

                // Priority (obligatory for issues)
                div { class: "detail-section",
                    div { class: "detail-section-title", "PRIORITY *" }
                    div { class: "priority-select",
                        for n in 1u8..=6 {
                            {
                                let is_active = priority() == Some(n);
                                let cls = if is_active { "priority-btn active" } else { "priority-btn" };
                                rsx! {
                                    button {
                                        class: "{cls}",
                                        onclick: move |_| priority.set(Some(n)),
                                        "#{n}"
                                    }
                                }
                            }
                        }
                    }
                }

                // Assignees
                div { class: "detail-section",
                    div { class: "detail-section-title", "ASSIGNEES *" }
                    div { class: "multi-select",
                        for login in &members {
                            {
                                let login_clone = login.clone();
                                let is_checked = selected_assignees().contains(login);
                                rsx! {
                                    label { class: "multi-select-item",
                                        input {
                                            r#type: "checkbox",
                                            checked: is_checked,
                                            onchange: move |_| {
                                                let mut current = selected_assignees();
                                                if let Some(pos) = current.iter().position(|l| l == &login_clone) {
                                                    current.remove(pos);
                                                } else {
                                                    current.push(login_clone.clone());
                                                }
                                                selected_assignees.set(current);
                                            },
                                        }
                                        "{login}"
                                    }
                                }
                            }
                        }
                    }
                }

                // Labels (optional)
                div { class: "detail-section",
                    div { class: "detail-section-title", "LABELS" }
                    if repo_labels.is_empty() {
                        div { class: "detail-empty", "No labels available in this repository." }
                    } else {
                        div { class: "multi-select",
                            for label in &repo_labels {
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
                    }
                }

                // Body (markdown editor with autocomplete)
                div { class: "detail-section",
                    div { class: "detail-section-title", "DESCRIPTION *" }
                    MarkdownEditor {
                        value: body(),
                        placeholder: "Describe the issue…",
                        owner: owner.clone(),
                        repo: selected_repo(),
                        members: members.clone(),
                        cards: cards.clone(),
                        on_change: move |v: String| body.set(v),
                    }
                }

                // Error
                if let Some(err) = error_msg() {
                    div { class: "modal-error", "{err}" }
                }

                // Actions at bottom
                div { class: "detail-edit-actions",
                    button {
                        class: "modal-btn modal-btn-secondary",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "modal-btn modal-btn-primary",
                        disabled: !can_save,
                        onclick: move |_| {
                            let token = token.clone();
                            let owner = owner.clone();
                            let repo = selected_repo().clone();
                            let title_val = title().trim().to_string();
                            let body_val = body().trim().to_string();
                            let body_opt: Option<String> = if body_val.is_empty() { None } else { Some(body_val) };
                            let mut all_labels = selected_labels();
                            // Prepend the priority label.
                            if let Some(p) = priority() {
                                all_labels.insert(0, format!("#{p}"));
                            }
                            let assignees = selected_assignees();
                            spawn(async move {
                                saving.set(true);
                                error_msg.set(None);
                                let client = RestClient::new(token);
                                match client.create_issue(&owner, &repo, &title_val, body_opt.as_deref(), &all_labels, &assignees).await {
                                    Ok(_) => {
                                        on_created.call(());
                                        on_close.call(());
                                    }
                                    Err(e) => {
                                        error_msg.set(Some(format!("Failed: {e}")));
                                    }
                                }
                                saving.set(false);
                            });
                        },
                        if saving() { "Creating…" } else { "Create Issue" }
                    }
                }
            }
        }
    }
}
