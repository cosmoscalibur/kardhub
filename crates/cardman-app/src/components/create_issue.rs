//! Create issue modal triggered from the board toolbar.
//!
//! Provides a repo dropdown (when multiple repos are available), title
//! input, markdown body editor with autocomplete, and create/cancel actions.

use cardman_core::github::RestClient;
use dioxus::prelude::*;

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
    /// Callback when issue is created (triggers board refresh).
    pub on_created: EventHandler<()>,
    /// Callback to close the modal.
    pub on_close: EventHandler<()>,
}

/// Modal for creating a new GitHub issue.
#[component]
pub fn CreateIssue(props: CreateIssueProps) -> Element {
    let on_close = props.on_close;
    let on_created = props.on_created;
    let mut title = use_signal(String::new);
    let mut body = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);
    let mut selected_repo = use_signal(|| props.repos.first().cloned().unwrap_or_default());

    let token = props.token.clone();
    let owner = props.owner.clone();
    let repos = props.repos.clone();
    let members = props.members.clone();
    let cards = props.cards.clone();

    let can_save = !title().trim().is_empty() && !saving();

    rsx! {
        // Backdrop
        div {
            class: "modal-overlay",
            onclick: move |_| on_close.call(()),
        }

        // Modal
        div { class: "modal-panel",
            div { class: "modal-header",
                h2 { "New Issue" }
                button {
                    class: "detail-close",
                    onclick: move |_| on_close.call(()),
                    "✕"
                }
            }

            div { class: "modal-body",
                // Repo selector (only when multiple repos available)
                if repos.len() > 1 {
                    div { class: "modal-field",
                        label { class: "modal-label", "Repository" }
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
                div { class: "modal-field",
                    label { class: "modal-label", "Title" }
                    input {
                        class: "modal-input",
                        r#type: "text",
                        placeholder: "Issue title",
                        value: "{title}",
                        oninput: move |e| title.set(e.value()),
                    }
                }

                // Body (markdown editor with autocomplete)
                div { class: "modal-field",
                    label { class: "modal-label", "Description" }
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

            div { class: "modal-footer",
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
                        spawn(async move {
                            saving.set(true);
                            error_msg.set(None);
                            let client = RestClient::new(token);
                            match client.create_issue(&owner, &repo, &title_val, body_opt.as_deref(), &[]).await {
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
