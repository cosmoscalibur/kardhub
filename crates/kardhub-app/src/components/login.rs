//! Login screen component.
//!
//! Accepts a GitHub Personal Access Token (PAT) from the user.

use dioxus::prelude::*;

/// Properties for the [`LoginScreen`] component.
#[derive(Props, Clone, PartialEq)]
pub struct LoginScreenProps {
    /// Callback triggered with the entered token string.
    pub on_submit: EventHandler<String>,
    /// Whether currently validating the token.
    pub loading: bool,
    /// Error message to display.
    pub error: Option<String>,
}

/// The login screen shown when the user is not authenticated.
#[component]
pub fn LoginScreen(props: LoginScreenProps) -> Element {
    let mut token_value = use_signal(String::new);
    let on_submit = props.on_submit;
    let is_empty = token_value().is_empty();

    rsx! {
        div { class: "login-screen",
            div { class: "login-card",
                div { class: "login-title", "🃏 KardHub" }
                div { class: "login-subtitle", "Kanban task manager synced with GitHub" }

                input {
                    class: "token-input",
                    r#type: "password",
                    placeholder: "ghp_xxxxxxxxxxxxxxxxxxxx",
                    value: "{token_value}",
                    oninput: move |e| token_value.set(e.value()),
                }

                div { class: "token-help",
                    "Create a token at "
                    a {
                        href: "https://github.com/settings/tokens",
                        target: "_blank",
                        "github.com/settings/tokens"
                    }
                    " with "
                    strong { "repo" }
                    " and "
                    strong { "read:org" }
                    " scopes."
                }

                button {
                    class: "login-btn",
                    disabled: is_empty || props.loading,
                    onclick: move |_| {
                        let val = token_value();
                        if !val.is_empty() {
                            on_submit.call(val);
                        }
                    },
                    if props.loading {
                        div { class: "spinner" }
                        "Connecting…"
                    } else {
                        "Sign in"
                    }
                }

                if let Some(err) = &props.error {
                    div {
                        style: "margin-top: 16px; color: var(--danger); font-size: 13px;",
                        "⚠ {err}"
                    }
                }
            }
        }
    }
}
