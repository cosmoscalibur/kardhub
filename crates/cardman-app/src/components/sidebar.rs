//! Collapsible sidebar component.
//!
//! Top: user avatar + name, collapse toggle.
//! Middle: horizontal Personal/Organization toggle with org dropdown, repo
//! list with checkboxes sorted by card count then alphabetically.
//! Bottom: theme toggle, settings, sign out.

use dioxus::prelude::*;

/// Data source kind for the sidebar.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceKind {
    /// Show personal repositories.
    Personal,
    /// Show repositories from an organization.
    Organization(String),
}

/// Filter for personal repositories.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PersonalFilter {
    /// Show only owned repos.
    #[default]
    Owner,
    /// Show only collaborator repos.
    Collaborator,
}

/// A repository entry with its name and cached card count for sorting.
#[derive(Debug, Clone, PartialEq)]
pub struct RepoEntry {
    /// Repository name.
    pub name: String,
    /// Number of cached cards (used for sorting, 0 if unknown).
    pub card_count: usize,
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
    /// Repository entries (name + cached card count).
    pub repos: Vec<RepoEntry>,
    /// Indices of currently selected (checked) repositories.
    pub selected_repos: Vec<usize>,
    /// Callback when a repository checkbox is toggled.
    pub on_toggle_repo: EventHandler<usize>,
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
    /// Current personal filter.
    #[props(default)]
    pub personal_filter: PersonalFilter,
    /// Callback when the personal filter changes.
    pub on_personal_filter: EventHandler<PersonalFilter>,
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

    // Sort repos: those with card_count > 0 by count desc, then 0-count alphabetically
    let mut sorted_indices: Vec<usize> = (0..props.repos.len()).collect();
    sorted_indices.sort_by(|a, b| {
        let ca = props.repos[*a].card_count;
        let cb = props.repos[*b].card_count;
        match (ca > 0, cb > 0) {
            // Both have counts → higher first
            (true, true) => cb.cmp(&ca),
            // Only one has count → it goes first
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            // Neither has count → alphabetical
            (false, false) => props.repos[*a]
                .name
                .to_lowercase()
                .cmp(&props.repos[*b].name.to_lowercase()),
        }
    });

    // Determine if source is Organization (for toggle state)
    let is_org_mode = matches!(&props.source, SourceKind::Organization(_));

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

            // Source selector: horizontal toggle + org dropdown
            if !props.collapsed {
                div { class: "source-selector",
                    // Horizontal toggle: Personal | Organization
                    div { class: "source-toggle",
                        {
                            let on_source = props.on_select_source;
                            rsx! {
                                button {
                                    class: if !is_org_mode { "source-btn active" } else { "source-btn" },
                                    onclick: move |_| on_source.call(SourceKind::Personal),
                                    "👤 Personal"
                                }
                            }
                        }
                        {
                            let on_source = props.on_select_source;
                            let orgs = props.orgs.clone();
                            let current_org = if let SourceKind::Organization(ref o) = props.source {
                                Some(o.clone())
                            } else {
                                None
                            };
                            rsx! {
                                button {
                                    class: if is_org_mode { "source-btn active" } else { "source-btn" },
                                    onclick: move |_| {
                                        // Switch to org mode; use current or first available org
                                        if let Some(ref org) = current_org {
                                            on_source.call(SourceKind::Organization(org.clone()));
                                        } else if let Some(first) = orgs.first() {
                                            on_source.call(SourceKind::Organization(first.clone()));
                                        }
                                    },
                                    "🏢 Organization"
                                }
                            }
                        }
                    }
                    // Personal filter dropdown (shown only when personal mode)
                    if !is_org_mode {
                        {
                            let on_pf = props.on_personal_filter;
                            let current_filter = match &props.personal_filter {
                                PersonalFilter::Owner => "owner",
                                PersonalFilter::Collaborator => "collaborator",
                            };
                            rsx! {
                                select {
                                    class: "source-org-select",
                                    value: "{current_filter}",
                                    onchange: move |evt: Event<FormData>| {
                                        let val = evt.value();
                                        let filter = match val.as_str() {
                                            "collaborator" => PersonalFilter::Collaborator,
                                            _ => PersonalFilter::Owner,
                                        };
                                        on_pf.call(filter);
                                    },
                                    option { value: "owner", selected: current_filter == "owner", "Owned" }
                                    option { value: "collaborator", selected: current_filter == "collaborator", "Collaborator" }
                                }
                            }
                        }
                    }
                    // Organization dropdown (shown only when org mode is active)
                    if is_org_mode && !props.orgs.is_empty() {
                        {
                            let on_source = props.on_select_source;
                            let current_org = if let SourceKind::Organization(ref o) = props.source {
                                o.clone()
                            } else {
                                String::new()
                            };
                            let mut sorted_orgs = props.orgs.clone();
                            sorted_orgs.sort_by_key(|a| a.to_lowercase());
                            rsx! {
                                select {
                                    class: "source-org-select",
                                    value: "{current_org}",
                                    onchange: move |evt: Event<FormData>| {
                                        let org = evt.value();
                                        if !org.is_empty() {
                                            on_source.call(SourceKind::Organization(org));
                                        }
                                    },
                                    for org in sorted_orgs.iter() {
                                        option {
                                            value: "{org}",
                                            selected: *org == current_org,
                                            "{org}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Repository list with checkboxes
            div { class: "sidebar-nav",
                div { class: "sidebar-section",
                    if !props.collapsed {
                        div { class: "sidebar-section-title", "Repositories" }
                    }
                    for original_idx in sorted_indices.iter() {
                        {
                            let idx = *original_idx;
                            let entry = &props.repos[idx];
                            let is_checked = props.selected_repos.contains(&idx);
                            let item_class = if is_checked {
                                "sidebar-item active"
                            } else {
                                "sidebar-item"
                            };
                            let on_toggle_repo = props.on_toggle_repo;
                            let repo_name = entry.name.clone();
                            let count = entry.card_count;
                            rsx! {
                                button {
                                    class: "{item_class}",
                                    onclick: move |_| on_toggle_repo.call(idx),
                                    span { class: "repo-checkbox",
                                        if is_checked { "☑" } else { "☐" }
                                    }
                                    if !props.collapsed {
                                        span { class: "repo-name", "{repo_name}" }
                                        if count > 0 {
                                            span { class: "repo-badge", "{count}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Footer: theme toggle + settings + sign out
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
