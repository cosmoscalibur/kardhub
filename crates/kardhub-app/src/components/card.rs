//! Individual card component rendering an issue or pull request.

use dioxus::prelude::*;
use kardhub_core::models::{Card, CardSource, User};

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
    /// Cached members for resolving login → avatar.
    #[props(default = Vec::new())]
    pub members: Vec<User>,
    /// Hidden PR cards (for looking up full data on mini-card click).
    #[props(default = Vec::new())]
    pub hidden_cards: Vec<Card>,
    /// Callback when the card is clicked.
    pub on_click: EventHandler<Card>,
}

/// A single card on the Kanban board.
#[component]
pub fn CardItem(props: CardItemProps) -> Element {
    let card = &props.card;
    let repo = &card.repo;
    let (number, title, labels, logins) = match &card.source {
        CardSource::Issue(issue) => (
            issue.number,
            issue.title.as_str(),
            &issue.labels,
            issue.assignees.clone(),
        ),
        CardSource::PullRequest(pr) => {
            // Fallback: if no explicit assignees, show author.
            let assignees = if pr.assignees.is_empty() {
                vec![pr.author.clone()]
            } else {
                pr.assignees.clone()
            };
            (pr.number, pr.title.as_str(), &pr.labels, assignees)
        }
    };

    // Resolve logins to User (for avatar display) via members cache.
    let resolved: Vec<User> = logins
        .iter()
        .map(|login| {
            props
                .members
                .iter()
                .find(|m| m.login == *login)
                .cloned()
                .unwrap_or_else(|| User {
                    login: login.clone(),
                    avatar_url: String::new(),
                })
        })
        .collect();

    // Separate priority labels from display labels
    let display_labels: Vec<_> = labels
        .iter()
        .filter(|l| kardhub_core::models::Priority::from_label(&l.name).is_none())
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
                span { class: "card-number", "{type_icon} {repo}#{number}" }
                "{title}"
            }

            // Meta row
            div { class: "card-meta",
                // Priority badge
                if let Some(priority) = &card.priority {
                    span { class: "card-priority", "#{priority.0}" }
                }

                // Assignee avatars
                if !resolved.is_empty() {
                    div { class: "card-assignees",
                        for u in &resolved {
                            if u.avatar_url.is_empty() {
                                span {
                                    class: "card-assignee",
                                    title: "{u.login}",
                                    "{u.login.chars().next().unwrap_or('?').to_uppercase()}"
                                }
                            } else {
                                img {
                                    class: "card-avatar",
                                    src: "{u.avatar_url}",
                                    alt: "{u.login}",
                                    title: "{u.login}",
                                    width: "20",
                                    height: "20",
                                }
                            }
                        }
                    }
                }
            }

            // Review semaphore (PR cards only): colored circles per reviewer.
            if let CardSource::PullRequest(pr) = &card.source {
                {
                    use kardhub_core::models::ReviewState;
                    // Build unified reviewer list: submitted reviews + pending.
                    let mut dots: Vec<(&str, &str)> = Vec::new(); // (login, css_class)
                    for review in &pr.reviews {
                        let cls = match review.state {
                            ReviewState::Approved => "dot-approved",
                            ReviewState::ChangesRequested => "dot-changes",
                            _ => continue,
                        };
                        dots.push((&review.user.login, cls));
                    }
                    for login in &pr.requested_reviewers {
                        // Skip if already in reviews.
                        if !dots.iter().any(|(l, _)| *l == login.as_str()) {
                            dots.push((login, "dot-pending"));
                        }
                    }
                    rsx! {
                        if !dots.is_empty() {
                            div { class: "card-review-dots",
                                for (login, cls) in &dots {
                                    span {
                                        class: "review-dot {cls}",
                                        title: "{login}",
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Linked PRs (only shown on issue cards)
            if !card.linked_prs.is_empty() {
                div { class: "card-linked-prs",
                    for lp in &card.linked_prs {
                        {
                            let status_class = if lp.merged {
                                "merged"
                            } else if lp.closed {
                                "closed"
                            } else if lp.draft {
                                "draft"
                            } else {
                                "open"
                            };
                            // Look up the full hidden PR card for the detail panel.
                            let full_pr_card = props.hidden_cards.iter().find(|c| {
                                c.owner == lp.owner
                                    && c.repo == lp.repo
                                    && matches!(&c.source, CardSource::PullRequest(pr) if pr.number == lp.number)
                            }).cloned();
                            let on_click = props.on_click;
                            rsx! {
                                div {
                                    class: "card-linked-pr {status_class}",
                                    onclick: move |e| {
                                        e.stop_propagation();
                                        if let Some(ref pr_card) = full_pr_card {
                                            on_click.call(pr_card.clone());
                                        }
                                    },
                                    span { class: "card-linked-pr-icon", "⤴" }
                                    span { class: "card-linked-pr-number", "{lp.repo}#{lp.number}" }
                                    span { class: "card-linked-pr-title", "{lp.title}" }
                                    if !lp.assignees.is_empty() {
                                        div { class: "card-linked-pr-assignees",
                                            for login in &lp.assignees {
                                                img {
                                                    class: "card-linked-pr-avatar",
                                                    src: "https://github.com/{login}.png?size=32",
                                                    alt: "{login}",
                                                    title: "{login}",
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
