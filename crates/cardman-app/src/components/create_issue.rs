//! Create issue modal triggered from the board toolbar.

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
    /// Repository name.
    pub repo: String,
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

    let token = props.token.clone();
    let owner = props.owner.clone();
    let repo = props.repo.clone();

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

                // Body (markdown editor)
                div { class: "modal-field",
                    label { class: "modal-label", "Description" }
                    MarkdownEditor {
                        value: body(),
                        placeholder: "Describe the issue…",
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
                        let repo = repo.clone();
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
