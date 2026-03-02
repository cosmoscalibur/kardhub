//! PR issue linker Dioxus component.
//!
//! Provides a floating button on PR pages that opens a dialog for
//! searching issues across repos and appending non-closing `Issue:`
//! references to the PR body.

use dioxus::prelude::*;

use crate::bridge;

/// Props for the [`IssueLinker`] component.
#[derive(Props, Clone, PartialEq)]
pub struct IssueLinkerProps {
    /// Repository owner.
    pub owner: String,
    /// Repository name.
    pub repo: String,
    /// Pull request number.
    pub pr_number: u64,
}

/// Represents a selected issue to link.
#[derive(Debug, Clone, PartialEq)]
struct SelectedIssue {
    /// Full repo identifier (owner/repo).
    repo: String,
    /// Issue number.
    number: u64,
    /// Issue title.
    title: String,
}

/// Floating issue linker button + dialog component.
///
/// When clicked, opens a dialog with:
/// - Repository picker dropdown
/// - Issue search input (debounced)
/// - Search results list with "Add" buttons
/// - Selected issues list
/// - "Add to PR body" button that appends `Issue: owner/repo#N` lines
#[component]
pub fn IssueLinker(props: IssueLinkerProps) -> Element {
    let mut dialog_open = use_signal(|| false);
    let mut search_query = use_signal(String::new);
    let mut search_results = use_signal(Vec::<(u64, String)>::new);
    let mut selected_issues = use_signal(Vec::<SelectedIssue>::new);
    let mut searching = use_signal(|| false);
    let mut applying = use_signal(|| false);
    let mut apply_error = use_signal(|| Option::<String>::None);
    let mut target_repo = use_signal(|| format!("{}/{}", props.owner, props.repo));

    let owner = props.owner.clone();
    let repo = props.repo.clone();
    let pr_number = props.pr_number;

    // Triggers a search using current query and target repo signals.
    let mut fire_search = move || {
        let query = search_query();
        let target = target_repo();
        if query.trim().is_empty() {
            search_results.set(Vec::new());
            return;
        }
        let full_query = format!("repo:{target} is:issue {query}");
        spawn(async move {
            searching.set(true);
            match bridge::search_issues_raw(&full_query).await {
                Ok(data) => {
                    let results = parse_search_results(data);
                    search_results.set(results);
                }
                Err(_) => {
                    search_results.set(Vec::new());
                }
            }
            searching.set(false);
        });
    };

    // Apply handler — append issues to PR body
    let apply = move |_: Event<MouseData>| {
        let selected = selected_issues();
        let owner = owner.clone();
        let repo = repo.clone();
        if selected.is_empty() {
            return;
        }
        spawn(async move {
            applying.set(true);
            apply_error.set(None);

            // Fetch current PR body
            match bridge::get_pr_raw(&owner, &repo, pr_number).await {
                Ok(pr_data) => {
                    let body_str = js_sys::Reflect::get(&pr_data, &"body".into())
                        .ok()
                        .and_then(|v| v.as_string())
                        .unwrap_or_default();

                    // Build issue lines
                    let issue_lines: String = selected
                        .iter()
                        .map(|s| format!("Issue: {}#{}", s.repo, s.number))
                        .collect::<Vec<_>>()
                        .join("\n");

                    // Append or update the Issues section
                    let new_body = if body_str.contains("## Issues") {
                        format!("{body_str}\n{issue_lines}")
                    } else {
                        format!("{body_str}\n\n## Issues\n{issue_lines}")
                    };

                    match bridge::update_pr_body(&owner, &repo, pr_number, &new_body).await {
                        Ok(()) => {
                            dialog_open.set(false);
                            // Reload the page to show updated PR body
                            if let Some(window) = web_sys::window() {
                                let _ = window.location().reload();
                            }
                        }
                        Err(e) => apply_error.set(Some(e)),
                    }
                }
                Err(e) => apply_error.set(Some(e)),
            }
            applying.set(false);
        });
    };

    rsx! {
        // Floating button
        if !dialog_open() {
            button {
                class: "cardman-linker-btn",
                onclick: move |_| dialog_open.set(true),
                "🃏 Link Issues",
            }
        }

        // Dialog overlay
        if dialog_open() {
            div {
                class: "cardman-dialog-overlay",
                onclick: move |_| dialog_open.set(false),
                div {
                    class: "cardman-dialog",
                    // Stop propagation so clicking dialog doesn't close it
                    onclick: move |e| e.stop_propagation(),

                    // Header
                    div { class: "cardman-dialog-header",
                        h3 { "🃏 Link Issues to PR #{pr_number}" }
                        button {
                            class: "cardman-dialog-close",
                            onclick: move |_| dialog_open.set(false),
                            "✕",
                        }
                    }

                    // Body
                    div { class: "cardman-dialog-body",
                        // Repo picker
                        div { class: "cardman-linker-row",
                            label { r#for: "cardman-repo-select", "Repository" }
                            input {
                                id: "cardman-repo-select",
                                class: "cardman-input",
                                r#type: "text",
                                value: "{target_repo}",
                                oninput: move |e| target_repo.set(e.value()),
                            }
                        }

                        // Search input
                        div { class: "cardman-linker-row",
                            label { r#for: "cardman-issue-search", "Search issues" }
                            input {
                                id: "cardman-issue-search",
                                class: "cardman-input",
                                r#type: "text",
                                placeholder: "Type to search…",
                                value: "{search_query}",
                                oninput: move |e| search_query.set(e.value()),
                                onkeydown: move |e: Event<KeyboardData>| {
                                    if e.key() == Key::Enter {
                                        fire_search();
                                    }
                                },
                            }
                        }

                        // Search button
                        div { class: "cardman-linker-row",
                            button {
                                class: "cardman-btn cardman-btn-primary",
                                disabled: searching(),
                                onclick: move |_: Event<MouseData>| fire_search(),
                                if searching() { "Searching…" } else { "Search" },
                            }
                        }

                        // Results
                        div { class: "cardman-search-results",
                            if search_results().is_empty() && !searching() {
                                p { class: "cardman-hint", "Start typing and click Search" }
                            }
                            for (num, title) in search_results().iter() {
                                {
                                    let already = selected_issues().iter().any(|s| s.number == *num && s.repo == target_repo());
                                    let num = *num;
                                    let title = title.clone();
                                    let target = target_repo();
                                    rsx! {
                                        div { class: "cardman-search-item",
                                            span { class: "cardman-search-number", "#{num}" }
                                            span { class: "cardman-search-title", "{title}" }
                                            button {
                                                class: "cardman-btn-add",
                                                disabled: already,
                                                onclick: move |_| {
                                                    let mut current = selected_issues();
                                                    current.push(SelectedIssue {
                                                        repo: target.clone(),
                                                        number: num,
                                                        title: title.clone(),
                                                    });
                                                    selected_issues.set(current);
                                                },
                                                if already { "✓" } else { "+" },
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Selected issues
                        div { class: "cardman-linker-row",
                            label { "Selected issues" }
                            div { class: "cardman-selected-issues",
                                if selected_issues().is_empty() {
                                    p { class: "cardman-hint", "No issues selected" }
                                }
                                for (i, issue) in selected_issues().iter().enumerate() {
                                    div { class: "cardman-selected-item",
                                        span { "Issue: {issue.repo}#{issue.number}" }
                                        button {
                                            class: "cardman-btn-remove",
                                            onclick: move |_| {
                                                let mut current = selected_issues();
                                                current.remove(i);
                                                selected_issues.set(current);
                                            },
                                            "✕",
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Footer
                    div { class: "cardman-dialog-footer",
                        if let Some(err) = apply_error() {
                            p { class: "cardman-error-sm", "{err}" }
                        }
                        button {
                            class: "cardman-btn cardman-btn-primary",
                            disabled: selected_issues().is_empty() || applying(),
                            onclick: apply,
                            if applying() { "Updating…" } else { "Add to PR body" },
                        }
                        button {
                            class: "cardman-btn",
                            onclick: move |_| dialog_open.set(false),
                            "Cancel",
                        }
                    }
                }
            }
        }
    }
}

/// Parse search results JsValue into (number, title) pairs.
fn parse_search_results(data: wasm_bindgen::JsValue) -> Vec<(u64, String)> {
    let json_str = match js_sys::JSON::stringify(&data) {
        Ok(s) => s.as_string().unwrap_or_default(),
        Err(_) => return Vec::new(),
    };

    #[derive(serde::Deserialize)]
    struct SearchResponse {
        items: Vec<SearchItem>,
    }

    #[derive(serde::Deserialize)]
    struct SearchItem {
        number: u64,
        title: String,
        #[serde(default)]
        pull_request: Option<serde_json::Value>,
    }

    let resp: SearchResponse = match serde_json::from_str(&json_str) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    resp.items
        .into_iter()
        // Filter out PRs from search results
        .filter(|i| i.pull_request.is_none())
        .map(|i| (i.number, i.title))
        .collect()
}
