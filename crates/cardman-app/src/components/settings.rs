//! Settings view component.
//!
//! Shows default repo per source, cache status, and refresh buttons.

use dioxus::prelude::*;

/// Properties for the [`Settings`] component.
#[derive(Props, Clone, PartialEq)]
pub struct SettingsProps {
    /// Current source display name (e.g. "Personal" or org name).
    pub source_name: String,
    /// All repos for the current source (for the dropdown).
    pub repos: Vec<String>,
    /// Currently configured default repo for this source (if any).
    pub default_repo: Option<String>,
    /// Number of cached sources (orgs).
    pub cached_sources_count: Option<usize>,
    /// Number of cached repos for the current source.
    pub cached_repos_count: Option<usize>,
    /// Number of cached closed issues for the current repo.
    pub cached_closed_count: Option<usize>,
    /// Callback to set the default repo for the current source.
    pub on_set_default_repo: EventHandler<Option<String>>,
    /// Callback to refresh sources and repos from API.
    pub on_refresh_sources: EventHandler<()>,
    /// Callback to refresh closed issues from API.
    pub on_refresh_closed: EventHandler<()>,
    /// Callback to close the settings view.
    pub on_close: EventHandler<()>,
}

/// The settings panel.
#[component]
pub fn Settings(props: SettingsProps) -> Element {
    let on_close = props.on_close;
    let on_set_default = props.on_set_default_repo;
    let on_refresh_sources = props.on_refresh_sources;
    let on_refresh_closed = props.on_refresh_closed;

    let default_val = props.default_repo.clone().unwrap_or_default();

    rsx! {
        div { class: "settings-panel",
            // Header
            div { class: "settings-header",
                h2 { "Settings" }
                button {
                    class: "detail-close",
                    onclick: move |_| on_close.call(()),
                    "✕"
                }
            }

            div { class: "settings-body",
                // Default repo section
                div { class: "settings-section",
                    div { class: "settings-section-title",
                        "Default Repository — {props.source_name}"
                    }
                    div { class: "settings-description",
                        "Auto-selected when switching to this source."
                    }
                    select {
                        class: "settings-select",
                        value: "{default_val}",
                        onchange: move |e| {
                            let val: String = e.value();
                            if val.is_empty() {
                                on_set_default.call(None);
                            } else {
                                on_set_default.call(Some(val));
                            }
                        },
                        option { value: "", "— None —" }
                        for repo in props.repos.iter() {
                            option {
                                value: "{repo}",
                                selected: *repo == default_val,
                                "{repo}"
                            }
                        }
                    }
                }

                // Cache status section
                div { class: "settings-section",
                    div { class: "settings-section-title", "Cache Status" }

                    // Sources & Repos
                    div { class: "settings-cache-row",
                        div { class: "settings-cache-info",
                            span { class: "settings-cache-label", "Sources & Repos" }
                            span { class: "settings-cache-count",
                                if let Some(count) = props.cached_sources_count {
                                    "{count} sources"
                                } else {
                                    "not cached"
                                }
                                " / "
                                if let Some(count) = props.cached_repos_count {
                                    "{count} repos"
                                } else {
                                    "not cached"
                                }
                            }
                        }
                        button {
                            class: "refresh-btn",
                            onclick: move |_| on_refresh_sources.call(()),
                            "🔃 Refresh"
                        }
                    }

                    // Closed issues
                    div { class: "settings-cache-row",
                        div { class: "settings-cache-info",
                            span { class: "settings-cache-label", "Closed Issues" }
                            span { class: "settings-cache-count",
                                if let Some(count) = props.cached_closed_count {
                                    "{count} cached"
                                } else {
                                    "not cached"
                                }
                            }
                        }
                        button {
                            class: "refresh-btn",
                            onclick: move |_| on_refresh_closed.call(()),
                            "🔃 Refresh"
                        }
                    }
                }
            }
        }
    }
}
