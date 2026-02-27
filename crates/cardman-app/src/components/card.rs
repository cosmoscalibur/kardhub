//! Individual card component rendering an issue or pull request.

use cardman_core::models::{Card, CardSource};
use dioxus::prelude::*;

/// Render a label with its color as background.
pub fn label_style(color: &str) -> String {
    // GitHub label colors are hex without '#'
    let r = u8::from_str_radix(&color[0..2], 16).unwrap_or(128);
    let g = u8::from_str_radix(&color[2..4], 16).unwrap_or(128);
    let b = u8::from_str_radix(&color[4..6], 16).unwrap_or(128);
    // Use a semi-transparent background with contrasting text
    let luminance = (0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64) / 255.0;
    let text_color = if luminance > 0.5 {
        "#1f2937"
    } else {
        "#f9fafb"
    };
    format!("background:#{color};color:{text_color};")
}

/// Properties for the [`CardItem`] component.
#[derive(Props, Clone, PartialEq)]
pub struct CardItemProps {
    /// The card to render.
    pub card: Card,
    /// Callback when the card is clicked.
    pub on_click: EventHandler<Card>,
}

/// A single card on the Kanban board.
#[component]
pub fn CardItem(props: CardItemProps) -> Element {
    let card = &props.card;
    let (number, title, labels, assignees) = match &card.source {
        CardSource::Issue(issue) => (
            issue.number,
            issue.title.as_str(),
            &issue.labels,
            &issue.assignees,
        ),
        CardSource::PullRequest(pr) => (
            pr.number,
            pr.title.as_str(),
            &pr.labels,
            &vec![], // PRs don't carry assignees in our model
        ),
    };

    // Separate priority labels from display labels
    let display_labels: Vec<_> = labels
        .iter()
        .filter(|l| cardman_core::models::Priority::from_label(&l.name).is_none())
        .collect();

    let is_pr = matches!(&card.source, CardSource::PullRequest(_));
    let type_icon = if is_pr { "⤴" } else { "●" };

    let card_clone = props.card.clone();
    let on_click = props.on_click;

    rsx! {
        div {
            class: "card",
            onclick: move |_| on_click.call(card_clone.clone()),
            // Labels row
            if !display_labels.is_empty() {
                div { class: "card-labels",
                    for label in &display_labels {
                        span {
                            class: "card-label",
                            style: "{label_style(&label.color)}",
                            "{label.name}"
                        }
                    }
                }
            }

            // Title
            div { class: "card-title",
                span { class: "card-number", "{type_icon} #{number}" }
                "{title}"
            }

            // Meta row
            div { class: "card-meta",
                // Priority badge
                if let Some(priority) = &card.priority {
                    span { class: "card-priority", "#{priority.0}" }
                }

                // Assignees
                if !assignees.is_empty() {
                    div { class: "card-assignees",
                        for assignee in assignees {
                            span {
                                class: "card-assignee",
                                title: "{assignee.login}",
                                "{assignee.login.chars().next().unwrap_or('?').to_uppercase()}"
                            }
                        }
                    }
                }
            }
        }
    }
}
