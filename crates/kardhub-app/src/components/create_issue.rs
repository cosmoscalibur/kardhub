//! Create issue panel shown on the right side (unified with detail panel).
//!
//! Provides a repo dropdown (with optional template selector), title,
//! priority, labels (chip pool), assignees (autocomplete chips), and
//! markdown body editor with autocomplete.

use dioxus::prelude::*;
use kardhub_core::github::RestClient;
use kardhub_core::models::{IssueTemplate, Label};

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
    /// Issue templates fetched from `.github/ISSUE_TEMPLATE/`.
    #[props(default = Vec::new())]
    pub templates: Vec<IssueTemplate>,
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

    // Autocomplete state for assignees.
    let mut assignee_input = use_signal(String::new);
    let mut assignee_show = use_signal(|| false);
    // Filter input for label pool.
    let mut label_filter = use_signal(String::new);

    let token = props.token.clone();
    let owner = props.owner.clone();
    let repos = props.repos.clone();
    let members = props.members.clone();
    let cards = props.cards.clone();
    let repo_labels = props.repo_labels.clone();
    let templates = props.templates.clone();

    // Filter labels: exclude #N priority labels.
    let visible_labels: Vec<&Label> = repo_labels
        .iter()
        .filter(|l| {
            let name = l.name.trim();
            !(name.starts_with('#') && name[1..].chars().all(|c| c.is_ascii_digit()))
        })
        .collect();

    // Compute assignee suggestions.
    let assignee_input_lower = assignee_input().to_lowercase();
    let assignee_suggestions: Vec<String> = if assignee_show() && !assignee_input_lower.is_empty() {
        let already = selected_assignees();
        members
            .iter()
            .filter(|login| {
                !already.contains(login) && login.to_lowercase().contains(&assignee_input_lower)
            })
            .take(6)
            .cloned()
            .collect()
    } else {
        Vec::new()
    };

    // Compute available (unselected) labels, filtered by search input.
    let label_filter_lower = label_filter().to_lowercase();
    let available_labels: Vec<&Label> = {
        let already = selected_labels();
        visible_labels
            .iter()
            .filter(|l| {
                !already.contains(&l.name)
                    && (label_filter_lower.is_empty()
                        || l.name.to_lowercase().contains(&label_filter_lower))
            })
            .copied()
            .collect()
    };

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
                // Repo + Template row
                div { class: "detail-section",
                    div { class: "detail-row",
                        div { class: "detail-row-item",
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
                        if !templates.is_empty() {
                            div { class: "detail-row-item",
                                div { class: "detail-section-title", "TEMPLATE" }
                                select {
                                    class: "modal-select",
                                    onchange: {
                                        let templates = templates.clone();
                                        move |e: Event<FormData>| {
                                            let idx: usize = e.value().parse().unwrap_or(0);
                                            if idx > 0
                                                && let Some(tpl) = templates.get(idx - 1)
                                            {
                                                body.set(tpl.body.clone());
                                            }
                                        }
                                    },
                                    option { value: "0", "— None —" }
                                    for (i, tpl) in templates.iter().enumerate() {
                                        {
                                            let v = (i + 1).to_string();
                                            let name = tpl.name.clone();
                                            rsx! { option { value: "{v}", "{name}" } }
                                        }
                                    }
                                }
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

                // Assignees (autocomplete chips inline with search)
                div { class: "detail-section",
                    div { class: "detail-section-title", "ASSIGNEES *" }
                    div { class: "chip-autocomplete",
                        for login in selected_assignees().iter() {
                            {
                                let login_val = login.clone();
                                rsx! {
                                    span { class: "assignee-chip",
                                        "{login}"
                                        button {
                                            class: "assignee-chip-remove",
                                            onclick: move |_| {
                                                let mut current = selected_assignees();
                                                current.retain(|l| l != &login_val);
                                                selected_assignees.set(current);
                                            },
                                            "✕"
                                        }
                                    }
                                }
                            }
                        }
                        input {
                            class: "chip-autocomplete-input",
                            r#type: "text",
                            placeholder: "Search assignee…",
                            value: "{assignee_input}",
                            oninput: move |e| {
                                assignee_input.set(e.value());
                                assignee_show.set(true);
                            },
                            onfocusin: move |_| assignee_show.set(true),
                            onfocusout: move |_| {
                                spawn(async move {
                                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                                    assignee_show.set(false);
                                });
                            },
                        }
                        if !assignee_suggestions.is_empty() {
                            div { class: "filter-suggestions",
                                for login in &assignee_suggestions {
                                    {
                                        let login_val = login.clone();
                                        rsx! {
                                            div {
                                                class: "filter-suggestion-item",
                                                onmousedown: move |_| {
                                                    let mut current = selected_assignees();
                                                    if !current.contains(&login_val) {
                                                        current.push(login_val.clone());
                                                    }
                                                    selected_assignees.set(current);
                                                    assignee_input.set(String::new());
                                                    assignee_show.set(false);
                                                },
                                                "{login}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Labels (chip pool — click to select, ✕ to remove)
                div { class: "detail-section",
                    div { class: "detail-section-title", "LABELS" }
                    if visible_labels.is_empty() {
                        div { class: "detail-empty", "No labels available in this repository." }
                    } else {
                        // Selected label chips
                        if !selected_labels().is_empty() {
                            div { class: "chip-autocomplete label-selected-chips",
                                for label_name in selected_labels().iter() {
                                    {
                                        let lname = label_name.clone();
                                        let color = repo_labels
                                            .iter()
                                            .find(|l| l.name == lname)
                                            .map(|l| l.color.clone())
                                            .unwrap_or_default();
                                        rsx! {
                                            span {
                                                class: "label-chip",
                                                style: "{label_style(&color)}",
                                                "{lname}"
                                                button {
                                                    class: "label-chip-remove",
                                                    onclick: move |_| {
                                                        let mut current = selected_labels();
                                                        current.retain(|l| l != &lname);
                                                        selected_labels.set(current);
                                                    },
                                                    "✕"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        // Search filter for the pool
                        input {
                            class: "modal-input label-pool-filter",
                            r#type: "text",
                            placeholder: "Filter labels…",
                            value: "{label_filter}",
                            oninput: move |e| label_filter.set(e.value()),
                        }
                        // Available labels pool
                        div { class: "label-pool",
                            for lbl in &available_labels {
                                {
                                    let lname = lbl.name.clone();
                                    let lcolor = lbl.color.clone();
                                    rsx! {
                                        span {
                                            class: "label-pool-item",
                                            style: "{label_style(&lcolor)}",
                                            onclick: move |_| {
                                                let mut current = selected_labels();
                                                if !current.contains(&lname) {
                                                    current.push(lname.clone());
                                                }
                                                selected_labels.set(current);
                                            },
                                            "{lbl.name}"
                                        }
                                    }
                                }
                            }
                            if available_labels.is_empty() && !label_filter_lower.is_empty() {
                                div { class: "detail-empty", "No matching labels." }
                            }
                        }
                    }
                }

                // Body (markdown editor with autocomplete) — grows to fill space
                div { class: "detail-section detail-section-grow",
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
            }

            // Actions pinned at bottom (outside detail-body for flex layout)
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
