//! Kanban board component rendering columns of cards.

use cardman_core::models::{Card, CardSource, IssueState, User};
use dioxus::prelude::*;

use super::card::CardItem;

/// All columns on the board in display order.
const COLUMNS: &[(&str, &str)] = &[
    ("🧊", "Icebox"),
    ("⏳", "Prebacklog"),
    ("📥", "Backlog"),
    ("🔙", "Pending"),
    ("🚧", "In Progress"),
    ("👀", "Code review"),
    ("⏳", "QA Backlog"),
    ("🔍", "QA Review"),
    ("☑️", "Ready for STG"),
    ("✅", "Ready for deploy"),
    ("📦", "In Release"),
    ("🚪", "Closed"),
];

/// Properties for the [`Board`] component.
#[derive(Props, Clone, PartialEq)]
pub struct BoardProps {
    /// All cards to display, already mapped to columns.
    pub cards: Vec<Card>,
    /// Current repository display name.
    pub repo_name: String,
    /// Whether the board is currently loading data.
    pub loading: bool,
    /// Callback to trigger a sync/refresh.
    pub on_refresh: EventHandler<()>,
    /// Callback when a card is clicked.
    pub on_card_click: EventHandler<Card>,
    /// Callback to open the create issue panel.
    pub on_create: EventHandler<()>,
    /// Cached members for resolving login → avatar.
    #[props(default = Vec::new())]
    pub members: Vec<User>,
}

/// Check if a card is from a closed issue.
fn is_closed(card: &Card) -> bool {
    matches!(
        &card.source,
        CardSource::Issue(issue) if issue.state == IssueState::Closed
    )
}

/// The main Kanban board with columns and cards.
#[component]
pub fn Board(props: BoardProps) -> Element {
    let on_refresh = props.on_refresh;
    let on_create = props.on_create;

    rsx! {
        div { class: "main-content",
            // Board header
            div { class: "board-header",
                h1 { "Board" }
                div { class: "board-actions",
                    button {
                        class: "create-btn",
                        onclick: move |_| on_create.call(()),
                        "➕ New Issue"
                    }
                    if props.loading {
                        div { class: "board-status",
                            div { class: "sync-icon" }
                            "Syncing…"
                        }
                    }
                    button {
                        class: "refresh-btn",
                        onclick: move |_| on_refresh.call(()),
                        "🔃 Refresh"
                    }
                    span { class: "repo-badge", "{props.repo_name}" }
                }
            }

            // Columns
            div { class: "board",
                for (emoji, col_name) in COLUMNS.iter() {
                    {
                        let is_closed_col = *col_name == "Closed";
                        let mut col_cards: Vec<&Card> = if is_closed_col {
                            // Closed column: show all closed issues
                            props.cards.iter().filter(|c| is_closed(c)).collect()
                        } else {
                            // Normal columns: show non-closed cards matching the column
                            props
                                .cards
                                .iter()
                                .filter(|c| c.column.name == *col_name && !is_closed(c))
                                .collect()
                        };
                        col_cards.sort_by(|a, b| a.priority.cmp(&b.priority));
                        let count = col_cards.len();

                        rsx! {
                            div { class: "column",
                                div { class: "column-header",
                                    div { class: "column-title",
                                        span { class: "emoji", "{emoji}" }
                                        "{col_name}"
                                    }
                                    span { class: "column-count", "{count}" }
                                }
                                div { class: "column-cards",
                                    for card in col_cards {
                                        {
                                            let on_card_click = props.on_card_click;
                                            rsx! {
                                                CardItem {
                                                    card: card.clone(),
                                                    members: props.members.clone(),
                                                    on_click: move |c: Card| on_card_click.call(c),
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
    }
}
