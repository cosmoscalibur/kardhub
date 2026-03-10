//! Kanban board component rendering columns of cards.

use dioxus::prelude::*;
use kardhub_core::filtering::{BoardFilter, matches_filter};
use kardhub_core::models::{Card, CardSource, IssueState, User};

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

    // Assignee autocomplete state.
    let mut assignee_input = use_signal(String::new);
    let mut assignee_selected: Signal<Option<String>> = use_signal(|| None);
    let mut assignee_show = use_signal(|| false);

    let mut filter_text = use_signal(String::new);
    let mut filter_number = use_signal(String::new);

    // Compute suggestions from members matching the current input.
    let input_lower = assignee_input().to_lowercase();
    let suggestions: Vec<String> = if assignee_show() && !input_lower.is_empty() {
        props
            .members
            .iter()
            .filter(|u| u.login.to_lowercase().contains(&input_lower))
            .take(6)
            .map(|u| u.login.clone())
            .collect()
    } else {
        Vec::new()
    };

    // Build active filter from current signal values.
    let board_filter = BoardFilter {
        assignee: assignee_selected(),
        text: {
            let v = filter_text();
            if v.is_empty() { None } else { Some(v) }
        },
        number: {
            let v = filter_number();
            let v = v.trim_start_matches('#');
            if v.is_empty() {
                None
            } else {
                Some(v.to_string())
            }
        },
    };

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
                        "↻ Refresh"
                    }
                    span { class: "repo-badge", "{props.repo_name}" }
                }
            }

            // Filter bar
            div { class: "filter-bar",
                // Assignee autocomplete
                div { class: "filter-autocomplete",
                    if let Some(ref login) = assignee_selected() {
                        span { class: "filter-chip",
                            "{login}"
                            button {
                                class: "filter-chip-remove",
                                onclick: move |_| {
                                    assignee_selected.set(None);
                                    assignee_input.set(String::new());
                                },
                                "✕"
                            }
                        }
                    } else {
                        input {
                            class: "filter-input",
                            r#type: "text",
                            placeholder: "Filter by assignee…",
                            value: "{assignee_input}",
                            oninput: move |e| {
                                assignee_input.set(e.value());
                                assignee_show.set(true);
                            },
                            onfocusin: move |_| assignee_show.set(true),
                            onfocusout: move |_| {
                                // Delay to allow click on suggestion.
                                spawn(async move {
                                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                                    assignee_show.set(false);
                                });
                            },
                        }
                        if !suggestions.is_empty() {
                            div { class: "filter-suggestions",
                                for login in &suggestions {
                                    {
                                        let login_val = login.clone();
                                        rsx! {
                                            div {
                                                class: "filter-suggestion-item",
                                                onmousedown: move |_| {
                                                    assignee_selected.set(Some(login_val.clone()));
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
                input {
                    class: "filter-input filter-input-wide",
                    r#type: "text",
                    placeholder: "Search title or body…",
                    value: "{filter_text}",
                    oninput: move |e| filter_text.set(e.value()),
                }
                input {
                    class: "filter-input filter-input-narrow",
                    r#type: "text",
                    placeholder: "#123",
                    value: "{filter_number}",
                    oninput: move |e| filter_number.set(e.value()),
                }
            }

            // Columns
            div { class: "board",
                for (emoji, col_name) in COLUMNS.iter() {
                    {
                        let is_closed_col = *col_name == "Closed";
                        let mut col_cards: Vec<&Card> = if is_closed_col {
                            // Closed column: show all closed issues
                            props.cards.iter()
                                .filter(|c| is_closed(c) && matches_filter(c, &board_filter))
                                .collect()
                        } else {
                            // Normal columns: show non-closed cards matching the column
                            props
                                .cards
                                .iter()
                                .filter(|c| !c.hidden && c.column.name == *col_name && !is_closed(c) && matches_filter(c, &board_filter))
                                .collect()
                        };
                        col_cards.sort_by(|a, b| a.priority.cmp(&b.priority));
                        let count = col_cards.len();

                        // Collect hidden PR cards for detail panel lookup.
                        let hidden: Vec<Card> = props
                            .cards
                            .iter()
                            .filter(|c| c.hidden)
                            .cloned()
                            .collect();

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
                                                    hidden_cards: hidden.clone(),
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
