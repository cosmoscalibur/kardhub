//! Collapsible sidebar component.
//!
//! Top: user avatar + name, collapse toggle.
//! Middle: personal/org selector, repository list.
//! Bottom: theme toggle, sign out.

use dioxus::prelude::*;

/// Data source kind for the sidebar.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceKind {
    /// Show personal repositories.
    Personal,
    /// Show repositories from an organization.
    Organization(String),
}

/// Properties for the [`Sidebar`] component.
#[derive(Props, Clone, PartialEq)]
pub struct SidebarProps {
    /// Whether the sidebar is collapsed.
    pub collapsed: bool,
    /// Current user display name.
    pub user_name: String,
    /// Current user login.
    pub user_login: String,
    /// GitHub avatar URL.
    pub avatar_url: String,
    /// Whether dark mode is active.
    pub dark_mode: bool,
    /// Organization names the user belongs to.
    pub orgs: Vec<String>,
    /// Currently selected source kind.
    pub source: SourceKind,
    /// Repository names available for the current source.
    pub repos: Vec<String>,
    /// Currently selected repository index.
    pub selected_repo: Option<usize>,
    /// Callback when a repository is selected.
    pub on_select_repo: EventHandler<usize>,
    /// Callback when the source kind changes.
    pub on_select_source: EventHandler<SourceKind>,
    /// Callback to toggle the sidebar collapsed state.
    pub on_toggle: EventHandler<()>,
    /// Callback to toggle dark/light theme.
    pub on_toggle_theme: EventHandler<()>,
    /// Callback to open settings.
    pub on_settings: EventHandler<()>,
    /// Callback to sign out.
    pub on_sign_out: EventHandler<()>,
}

/// The left sidebar panel.
#[component]
pub fn Sidebar(props: SidebarProps) -> Element {
    let sidebar_class = if props.collapsed {
        "sidebar collapsed"
    } else {
        "sidebar"
    };

    let theme_icon = if props.dark_mode { "☀️" } else { "🌙" };
    let theme_label = if props.dark_mode { "Light" } else { "Dark" };
    let toggle_icon = if props.collapsed { "▶" } else { "◀" };

    // First letter of login for avatar fallback
    let avatar_letter = props
        .user_login
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();

    let on_toggle = props.on_toggle;

    rsx! {
        div { class: "{sidebar_class}",
            // Collapse toggle button
            button {
                class: "sidebar-toggle",
                onclick: move |_| on_toggle.call(()),
                title: if props.collapsed { "Expand" } else { "Collapse" },
                "{toggle_icon}"
            }

            // Header: avatar + user info
            div { class: "sidebar-header",
                if props.avatar_url.is_empty() {
                    div { class: "avatar", "{avatar_letter}" }
                } else {
                    div { class: "avatar",
                        img {
                            src: "{props.avatar_url}",
                            alt: "{props.user_login}",
                        }
                    }
                }
                if !props.collapsed {
                    div { class: "user-info",
                        div { class: "user-name", "{props.user_name}" }
                        div { class: "user-login", "@{props.user_login}" }
                    }
                }
            }

            // Source selector: Personal / Organization
            if !props.collapsed {
                div { class: "source-selector",
                    div { class: "source-selector-title", "Source" }
                    {
                        let is_personal = props.source == SourceKind::Personal;
                        let on_source = props.on_select_source;
                        rsx! {
                            button {
                                class: if is_personal { "active" } else { "" },
                                onclick: move |_| on_source.call(SourceKind::Personal),
                                span { class: "icon", "👤" }
                                "Personal"
                            }
                        }
                    }
                    for org_name in props.orgs.iter() {
                        {
                            let is_org = props.source == SourceKind::Organization(org_name.clone());
                            let on_source = props.on_select_source;
                            let org = org_name.clone();
                            rsx! {
                                button {
                                    class: if is_org { "active" } else { "" },
                                    onclick: move |_| on_source.call(SourceKind::Organization(org.clone())),
                                    span { class: "icon", "🏢" }
                                    "{org_name}"
                                }
                            }
                        }
                    }
                }
            }

            // Repository list
            div { class: "sidebar-nav",
                div { class: "sidebar-section",
                    if !props.collapsed {
                        div { class: "sidebar-section-title", "Repositories" }
                    }
                    for (idx, repo_name) in props.repos.iter().enumerate() {
                        {
                            let is_active = props.selected_repo == Some(idx);
                            let item_class = if is_active {
                                "sidebar-item active"
                            } else {
                                "sidebar-item"
                            };
                            let on_select = props.on_select_repo;
                            rsx! {
                                button {
                                    class: "{item_class}",
                                    onclick: move |_| on_select.call(idx),
                                    span { class: "icon", "📁" }
                                    if !props.collapsed {
                                        "{repo_name}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Footer: theme toggle + sign out
            div { class: "sidebar-footer",
                {
                    let on_theme = props.on_toggle_theme;
                    rsx! {
                        button {
                            class: "theme-toggle",
                            onclick: move |_| on_theme.call(()),
                            span { class: "icon", "{theme_icon}" }
                            if !props.collapsed {
                                "{theme_label}"
                            }
                        }
                    }
                }
                {
                    let on_settings = props.on_settings;
                    rsx! {
                        button {
                            class: "sidebar-item",
                            onclick: move |_| on_settings.call(()),
                            span { class: "icon", "⚙️" }
                            if !props.collapsed {
                                "Settings"
                            }
                        }
                    }
                }
                {
                    let on_sign_out = props.on_sign_out;
                    rsx! {
                        button {
                            class: "sidebar-item",
                            onclick: move |_| on_sign_out.call(()),
                            span { class: "icon", "🚪" }
                            if !props.collapsed {
                                "Sign out"
                            }
                        }
                    }
                }
            }
        }
    }
}
