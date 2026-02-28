//! Reusable markdown editor with write/preview tabs.

use cardman_core::markdown::markdown_to_html;
use dioxus::prelude::*;

/// Properties for the [`MarkdownEditor`] component.
#[derive(Props, Clone, PartialEq)]
pub struct MarkdownEditorProps {
    /// Current markdown text.
    pub value: String,
    /// Placeholder text for the textarea.
    #[props(default = "Write markdown…".to_string())]
    pub placeholder: String,
    /// Callback when text changes.
    pub on_change: EventHandler<String>,
}

/// Textarea with a write/preview tab toggle.
#[component]
pub fn MarkdownEditor(props: MarkdownEditorProps) -> Element {
    let mut preview_mode = use_signal(|| false);
    let html = markdown_to_html(&props.value);
    let on_change = props.on_change;

    rsx! {
        div { class: "md-editor",
            div { class: "md-editor-tabs",
                button {
                    class: if !preview_mode() { "md-tab active" } else { "md-tab" },
                    onclick: move |_| preview_mode.set(false),
                    "Write"
                }
                button {
                    class: if preview_mode() { "md-tab active" } else { "md-tab" },
                    onclick: move |_| preview_mode.set(true),
                    "Preview"
                }
            }
            if preview_mode() {
                div {
                    class: "md-preview detail-markdown",
                    dangerous_inner_html: "{html}",
                }
            } else {
                textarea {
                    class: "md-textarea",
                    placeholder: "{props.placeholder}",
                    value: "{props.value}",
                    oninput: move |e| on_change.call(e.value()),
                }
            }
        }
    }
}
