//! Card detail panel shown on the right side when a card is clicked.

use cardman_core::models::{Card, CardSource, IssueState};
use dioxus::prelude::*;

use super::card::label_style;

/// Properties for the [`CardDetail`] component.
#[derive(Props, Clone, PartialEq)]
pub struct CardDetailProps {
    /// The card to display.
    pub card: Card,
    /// Callback to close the panel.
    pub on_close: EventHandler<()>,
}

/// Right-side detail panel showing full card information.
#[component]
pub fn CardDetail(props: CardDetailProps) -> Element {
    let card = &props.card;
    let on_close = props.on_close;

    let (number, title, body, labels, assignees, state_label, state_class) = match &card.source {
        CardSource::Issue(issue) => {
            let (sl, sc) = match issue.state {
                IssueState::Open => ("Open", "open"),
                IssueState::Closed => ("Closed", "closed"),
            };
            (
                issue.number,
                issue.title.as_str(),
                issue.body.as_deref().unwrap_or("No description provided."),
                issue.labels.as_slice(),
                issue.assignees.as_slice(),
                sl,
                sc,
            )
        }
        CardSource::PullRequest(pr) => {
            let (sl, sc) = if pr.merged {
                ("Merged", "merged")
            } else {
                ("Open", "open")
            };
            (
                pr.number,
                pr.title.as_str(),
                "",
                pr.labels.as_slice(),
                &[] as &[_],
                sl,
                sc,
            )
        }
    };

    let is_pr = matches!(&card.source, CardSource::PullRequest(_));
    let type_label = if is_pr { "Pull Request" } else { "Issue" };

    // Filter out priority labels for display
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
                button {
                    class: "detail-close",
                    onclick: move |_| on_close.call(()),
                    "✕"
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
                                span { class: "detail-assignee",
                                    "@{assignee.login}"
                                }
                            }
                        }
                    }
                }

                // Body / Description
                if !body.is_empty() {
                    div { class: "detail-section",
                        div { class: "detail-section-title", "Description" }
                        div { class: "detail-content", "{body}" }
                    }
                }
            }
        }
    }
}
