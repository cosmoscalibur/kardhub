//! Reusable markdown editor with write/preview tabs and autocomplete.
//!
//! Supports `@mention` autocomplete from a members list and `#issue/PR`
//! autocomplete from dashboard cards. Uses JS eval for caret position.

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
    /// Repository owner (for auto-linking in preview).
    #[props(default = String::new())]
    pub owner: String,
    /// Repository name (for auto-linking in preview).
    #[props(default = String::new())]
    pub repo: String,
    /// Member `(login, display_name)` pairs for `@` autocomplete.
    #[props(default = Vec::new())]
    pub members: Vec<(String, Option<String>)>,
    /// Card `(number, title)` pairs for `#` autocomplete.
    #[props(default = Vec::new())]
    pub cards: Vec<(u64, String)>,
    /// Callback when text changes.
    pub on_change: EventHandler<String>,
}

/// Maximum number of autocomplete suggestions to show.
const MAX_SUGGESTIONS: usize = 8;

/// Textarea with write/preview tabs and inline autocomplete popup.
#[component]
pub fn MarkdownEditor(props: MarkdownEditorProps) -> Element {
    let mut preview_mode = use_signal(|| false);
    let html = markdown_to_html(&props.value, &props.owner, &props.repo);
    let on_change = props.on_change;
    let members = props.members.clone();
    let cards = props.cards.clone();
    let value = props.value.clone();

    // Autocomplete state.
    let mut ac_visible = use_signal(|| false);
    let mut ac_items: Signal<Vec<AcItem>> = use_signal(Vec::new);
    let mut ac_index = use_signal(|| 0usize);
    // Start position of the trigger token (byte offset into value).
    let mut ac_trigger_pos: Signal<Option<usize>> = use_signal(|| None);

    // Insert the selected autocomplete item into the value.
    let insert_completion = {
        let value = value.clone();
        move |item: &AcItem, trigger_pos: usize| {
            let before = &value[..trigger_pos];
            // Find cursor position (just past the trigger + typed query).
            // The query extends from trigger_pos to the next whitespace or end.
            let rest = &value[trigger_pos..];
            let query_end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
            let after = &value[trigger_pos + query_end..];
            let replacement = match item {
                AcItem::Member(login, _) => format!("@{login} "),
                AcItem::Card(num, _) => format!("#{num} "),
            };
            let new_value = format!("{before}{replacement}{after}");
            on_change.call(new_value);
            ac_visible.set(false);
            ac_trigger_pos.set(None);
        }
    };

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
                div { class: "md-editor-wrap",
                    textarea {
                        class: "md-textarea",
                        id: "md-textarea-active",
                        placeholder: "{props.placeholder}",
                        value: "{props.value}",
                        oninput: {
                            let members = members.clone();
                            let cards = cards.clone();
                            move |e: Event<FormData>| {
                                let new_val = e.value();
                                on_change.call(new_val.clone());
                                update_autocomplete(
                                    &new_val,
                                    &members,
                                    &cards,
                                    &mut ac_visible,
                                    &mut ac_items,
                                    &mut ac_index,
                                    &mut ac_trigger_pos,
                                );
                            }
                        },
                        onkeydown: {
                            let items_for_key = ac_items;
                            let trigger_pos_for_key = ac_trigger_pos;
                            let mut insert = insert_completion.clone();
                            move |e: Event<KeyboardData>| {
                                if !ac_visible() {
                                    return;
                                }
                                let key = e.key();
                                match key {
                                    Key::ArrowDown => {
                                        e.prevent_default();
                                        let len = items_for_key().len();
                                        if len > 0 {
                                            ac_index.set((ac_index() + 1) % len);
                                        }
                                    }
                                    Key::ArrowUp => {
                                        e.prevent_default();
                                        let len = items_for_key().len();
                                        if len > 0 {
                                            ac_index.set(ac_index().checked_sub(1).unwrap_or(len - 1));
                                        }
                                    }
                                    Key::Enter | Key::Tab => {
                                        e.prevent_default();
                                        let items = items_for_key();
                                        if let Some(item) = items.get(ac_index())
                                            && let Some(tp) = trigger_pos_for_key()
                                        {
                                            insert(item, tp);
                                        }
                                    }
                                    Key::Escape => {
                                        ac_visible.set(false);
                                    }
                                    _ => {}
                                }
                            }
                        },
                    }
                    // Autocomplete popup
                    if ac_visible() && !ac_items().is_empty() {
                        div { class: "md-autocomplete",
                            for (i, item) in ac_items().iter().enumerate() {
                                {
                                    let is_active = i == ac_index();
                                    let label = match item {
                                        AcItem::Member(login, name) => {
                                            match name {
                                                Some(n) => format!("@{login} ({n})"),
                                                None => format!("@{login}"),
                                            }
                                        }
                                        AcItem::Card(num, title) => format!("#{num} {title}"),
                                    };
                                    let item_clone = item.clone();
                                    let mut insert = insert_completion.clone();
                                    let trigger_pos_click = ac_trigger_pos;
                                    rsx! {
                                        div {
                                            class: if is_active { "md-ac-item active" } else { "md-ac-item" },
                                            onmousedown: move |e| {
                                                e.prevent_default();
                                                if let Some(tp) = trigger_pos_click() {
                                                    insert(&item_clone, tp);
                                                }
                                            },
                                            "{label}"
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

/// Autocomplete item variants.
#[derive(Clone, Debug, PartialEq)]
enum AcItem {
    /// A user login and optional display name.
    Member(String, Option<String>),
    /// An issue/PR number and title.
    Card(u64, String),
}

/// Detect `@` or `#` trigger in the text and populate autocomplete items.
fn update_autocomplete(
    text: &str,
    members: &[(String, Option<String>)],
    cards: &[(u64, String)],
    visible: &mut Signal<bool>,
    items: &mut Signal<Vec<AcItem>>,
    index: &mut Signal<usize>,
    trigger_pos: &mut Signal<Option<usize>>,
) {
    // Find the last trigger character before cursor (we approximate cursor = end of text).
    let last_at = text.rfind('@');
    let last_hash = text.rfind('#');

    // Determine which trigger is active (the one closest to end, after any whitespace).
    let trigger = match (last_at, last_hash) {
        (Some(a), Some(h)) => {
            if a > h {
                Some(('@', a))
            } else {
                Some(('#', h))
            }
        }
        (Some(a), None) => Some(('@', a)),
        (None, Some(h)) => Some(('#', h)),
        (None, None) => None,
    };

    let Some((trigger_char, pos)) = trigger else {
        visible.set(false);
        trigger_pos.set(None);
        return;
    };

    // Trigger must be preceded by whitespace or be at start.
    if pos > 0 {
        let prev = text.as_bytes()[pos - 1];
        if !prev.is_ascii_whitespace() && prev != b'(' && prev != b'>' {
            visible.set(false);
            trigger_pos.set(None);
            return;
        }
    }

    // Extract the query after the trigger character.
    let query_start = pos + trigger_char.len_utf8();
    let query = &text[query_start..];
    // Query must not contain whitespace (single token).
    if query.contains(char::is_whitespace) {
        visible.set(false);
        trigger_pos.set(None);
        return;
    }
    let query_lower = query.to_lowercase();

    let suggestions: Vec<AcItem> = match trigger_char {
        '@' => {
            // Require at least 1 character after @.
            if query.is_empty() {
                visible.set(false);
                trigger_pos.set(None);
                return;
            }
            members
                .iter()
                .filter(|(login, name)| {
                    login.to_lowercase().starts_with(&query_lower)
                        || name
                            .as_ref()
                            .is_some_and(|n| n.to_lowercase().starts_with(&query_lower))
                })
                .take(MAX_SUGGESTIONS)
                .map(|(login, name)| AcItem::Member(login.clone(), name.clone()))
                .collect()
        }
        '#' => {
            if query.is_empty() || !query.chars().all(|c| c.is_ascii_digit()) {
                visible.set(false);
                trigger_pos.set(None);
                return;
            }
            let mut matched: Vec<(u64, String)> = cards
                .iter()
                .filter(|(num, _)| num.to_string().starts_with(query))
                .cloned()
                .collect();
            // Sort by number descending (most recent first).
            matched.sort_by(|a, b| b.0.cmp(&a.0));
            matched
                .into_iter()
                .take(MAX_SUGGESTIONS)
                .map(|(n, t)| AcItem::Card(n, t))
                .collect()
        }
        _ => Vec::new(),
    };

    if suggestions.is_empty() {
        visible.set(false);
        trigger_pos.set(None);
    } else {
        items.set(suggestions);
        index.set(0);
        trigger_pos.set(Some(pos));
        visible.set(true);
    }
}
