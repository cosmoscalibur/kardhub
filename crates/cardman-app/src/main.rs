//! Cardman desktop application entry point.
//!
//! Manages authentication state and renders either the login screen
//! (GitHub PAT input) or the main dashboard (sidebar + kanban board).

mod cache;
mod components;

use cache::{
    AppSettings, OrgSources, SourceMap, SourceRepos, cached_card_count, clear_all_cache,
    closed_sync_time, is_cards_fresh, is_labels_fresh, is_members_fresh, is_profile_fresh,
    is_source_map_fresh, load_cards, load_closed_issues, load_labels, load_members,
    load_merged_prs, load_open_issues, load_profile, load_prs, load_settings, load_source_map,
    merged_sync_time, open_sync_time, prs_sync_time, save_cards, save_closed_issues, save_labels,
    save_members, save_merged_prs, save_open_issues, save_profile, save_prs, save_settings,
    save_source_map, source_key,
};
use cardman_core::github::RestClient;
use cardman_core::mapping::{MappingConfig, map_card};
use cardman_core::models::{AuthenticatedUser, Card, CardSource, Label, User};
use components::board::Board;
use components::create_issue::CreateIssue;
use components::detail::CardDetail;
use components::login::LoginScreen;
use components::settings::Settings;
use components::sidebar::{PersonalFilter, RepoEntry, Sidebar, SourceKind};
use dioxus::prelude::*;

/// Application-wide state.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
enum AppState {
    /// Not authenticated, show login screen.
    Login,
    /// Loading data after authentication.
    Loading,
    /// Authenticated, show the dashboard.
    Dashboard {
        user: AuthenticatedUser,
        token: String,
        source_map: SourceMap,
        source: SourceKind,
        repos: Vec<String>,
        /// Indices of currently selected (checked) repos.
        selected_repos: Vec<usize>,
        /// Aggregated cards from all selected repos.
        cards: Vec<Card>,
    },
}

fn main() {
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            dioxus::desktop::Config::new().with_window(
                dioxus::desktop::WindowBuilder::new()
                    .with_title("Cardman")
                    .with_inner_size(dioxus::desktop::LogicalSize::new(1280.0, 800.0))
                    .with_min_inner_size(dioxus::desktop::LogicalSize::new(900.0, 600.0)),
            ),
        )
        .launch(app);
}

/// Derive the owner string from the source + user.
fn owner_for_source(source: &SourceKind, user_login: &str) -> String {
    match source {
        SourceKind::Personal => user_login.to_string(),
        SourceKind::Organization(org) => org.clone(),
    }
}

/// Source key string for the sidebar source (used in cache filenames).
fn source_key_for(source: &SourceKind) -> String {
    match source {
        SourceKind::Personal => "personal".to_string(),
        SourceKind::Organization(org) => source_key(org),
    }
}

/// Build repo entries with cached card counts.
fn build_repo_entries(repos: &[String], owner: &str) -> Vec<RepoEntry> {
    repos
        .iter()
        .map(|name| RepoEntry {
            name: name.clone(),
            card_count: cached_card_count(owner, name),
        })
        .collect()
}

/// Root application component.
fn app() -> Element {
    let mut state = use_signal(|| AppState::Login);
    let mut dark_mode = use_signal(|| true);
    let mut sidebar_collapsed = use_signal(|| false);
    let mut login_error = use_signal(|| Option::<String>::None);
    let board_loading = use_signal(|| false);
    let mut selected_card = use_signal(|| Option::<Card>::None);
    let mut show_settings = use_signal(|| false);
    let mut show_create_issue = use_signal(|| false);
    let mut app_settings = use_signal(load_settings);
    let mut personal_filter = use_signal(PersonalFilter::default);

    // Auto-login from saved token on first render
    let mut auto_login_done = use_signal(|| false);
    if !auto_login_done() {
        auto_login_done.set(true);
        let settings = app_settings();
        if let Some(ref token) = settings.token {
            let token = token.clone();
            state.set(AppState::Loading);
            let mut state = state;
            let mut board_loading = board_loading;
            spawn(async move {
                match authenticate_and_load(token).await {
                    Ok(dashboard) => {
                        // Extract info needed for background fetch
                        let (tkn, sel, rps, src, usr) = if let AppState::Dashboard {
                            ref token,
                            ref selected_repos,
                            ref repos,
                            ref source,
                            ref user,
                            ..
                        } = dashboard
                        {
                            (
                                token.clone(),
                                selected_repos.clone(),
                                repos.clone(),
                                source.clone(),
                                user.login.clone(),
                            )
                        } else {
                            unreachable!()
                        };
                        state.set(dashboard);

                        // Trigger background fetch for restored repos
                        if !sel.is_empty() {
                            board_loading.set(true);
                            let owner = owner_for_source(&src, &usr);
                            for &idx in &sel {
                                if let Some(repo_name) = rps.get(idx)
                                    && !is_cards_fresh(&owner, repo_name)
                                {
                                    let cards = fetch_cards(&tkn, &owner, repo_name).await;
                                    save_cards(&owner, repo_name, &cards);
                                }
                            }
                            // Rebuild all cards from caches
                            let mut all = Vec::new();
                            for &idx in &sel {
                                if let Some(rn) = rps.get(idx)
                                    && let Some(c) = load_cards(&owner, rn)
                                {
                                    all.extend(c);
                                }
                            }
                            all.sort_by(|a, b| a.priority.cmp(&b.priority));
                            if let AppState::Dashboard { ref mut cards, .. } = *state.write() {
                                *cards = all;
                            }
                            board_loading.set(false);
                        }
                    }
                    Err(_) => state.set(AppState::Login),
                }
            });
        }
    }

    let body_class = if dark_mode() { "" } else { "light" };

    match state() {
        AppState::Login => {
            rsx! {
                div { class: "{body_class}",
                    style { { include_str!("../assets/style.css") } }
                    LoginScreen {
                        on_submit: move |token: String| {
                            login_error.set(None);
                            state.set(AppState::Loading);

                            let mut state = state;
                            let mut login_error = login_error;
                            let mut app_settings = app_settings;
                            spawn(async move {
                                match authenticate_and_load(token.clone()).await {
                                    Ok(dashboard) => {
                                        // Persist token for auto-login
                                        let mut s = load_settings();
                                        s.token = Some(token);
                                        save_settings(&s);
                                        app_settings.set(s);
                                        state.set(dashboard);
                                    }
                                    Err(e) => {
                                        login_error.set(Some(e));
                                        state.set(AppState::Login);
                                    }
                                }
                            });
                        },
                        loading: matches!(state(), AppState::Loading),
                        error: login_error(),
                    }
                }
            }
        }
        AppState::Loading => {
            rsx! {
                div { class: "{body_class}",
                    style { { include_str!("../assets/style.css") } }
                    div { class: "login-screen",
                        div { class: "loading",
                            div { class: "spinner" }
                            "Connecting to GitHub…"
                        }
                    }
                }
            }
        }
        AppState::Dashboard {
            user,
            token,
            source_map,
            source,
            repos,
            selected_repos,
            cards,
        } => {
            let owner = owner_for_source(&source, &user.login);
            let sk = source_key_for(&source);
            let orgs = source_map.org_names();
            let repo_entries = build_repo_entries(&repos, &owner);

            let repo_display = if selected_repos.is_empty() {
                "No repository".to_string()
            } else if selected_repos.len() == 1 {
                repos.get(selected_repos[0]).cloned().unwrap_or_default()
            } else {
                format!("{} repos", selected_repos.len())
            };

            // Cache counts for settings
            let cached_sources_count = Some(orgs.len() + 1); // +1 for personal
            let cached_repos_count = Some(repos.len());
            let cached_closed_count = if selected_repos.len() == 1 {
                repos
                    .get(selected_repos[0])
                    .and_then(|r| load_closed_issues(&owner, r).map(|c| c.len()))
            } else {
                None
            };
            let default_repo = app_settings().default_repos.get(&sk).cloned();
            let cards_for_ac = cards.clone();
            // Full members list for avatar resolution.
            // Always include the authenticated user; org sources add cached members.
            let auth_as_user = User {
                login: user.login.clone(),
                avatar_url: user.avatar_url.clone(),
            };
            let mut members_full: Vec<User> = match &source {
                SourceKind::Organization(org) => load_members(org).unwrap_or_default(),
                _ => Vec::new(),
            };
            if !members_full.iter().any(|m| m.login == auth_as_user.login) {
                members_full.push(auth_as_user);
            }
            // Login strings for autocomplete.
            let members_logins: Vec<String> =
                members_full.iter().map(|u| u.login.clone()).collect();
            // Cards for # autocomplete.
            let cards_ac: Vec<(u64, String)> = cards_for_ac
                .iter()
                .map(|c| {
                    let (num, title) = match &c.source {
                        CardSource::Issue(i) => (i.number, i.title.clone()),
                        CardSource::PullRequest(pr) => (pr.number, pr.title.clone()),
                    };
                    (num, title)
                })
                .collect();

            // Aggregate repo labels from selected repos.
            let repo_labels: Vec<Label> = {
                let mut all_labels = Vec::new();
                for &idx in &selected_repos {
                    if let Some(repo_name) = repos.get(idx)
                        && let Some(labels) = load_labels(&owner, repo_name)
                    {
                        for label in labels {
                            if !all_labels.iter().any(|l: &Label| l.name == label.name) {
                                all_labels.push(label);
                            }
                        }
                    }
                }
                all_labels
            };

            // Pre-clone for closures
            let token_for_toggle = token.clone();
            let token_for_source = token.clone();
            let token_for_refresh = token.clone();
            let token_for_rs = token.clone();
            let token_for_rc = token.clone();
            let source_for_toggle = source.clone();
            let source_for_refresh = source.clone();
            let user_for_toggle = user.clone();
            let user_for_refresh = user.clone();
            let repos_for_toggle = repos.clone();
            let repos_for_refresh = repos.clone();
            let selected_for_refresh = selected_repos.clone();
            let source_for_rc = source.clone();
            let user_for_rs = user.clone();
            let user_for_rc = user.clone();
            let repos_for_rc = repos.clone();
            let selected_for_rc = selected_repos.clone();

            rsx! {
                div { class: "{body_class}",
                    style { { include_str!("../assets/style.css") } }
                    div { class: "cardman-app",
                        Sidebar {
                            collapsed: sidebar_collapsed(),
                            user_name: user.name.clone().unwrap_or_else(|| user.login.clone()),
                            user_login: user.login.clone(),
                            avatar_url: user.avatar_url.clone(),
                            dark_mode: dark_mode(),
                            orgs: orgs.clone(),
                            source: source.clone(),
                            repos: repo_entries,
                            selected_repos: selected_repos.clone(),
                            personal_filter: personal_filter(),
                            on_personal_filter: move |filter: PersonalFilter| {
                                // Derive repos based on the new filter
                                let repos = if let AppState::Dashboard {
                                    ref source_map, ..
                                } = state()
                                {
                                    match &filter {
                                        PersonalFilter::Owner => source_map.personal.owner.clone(),
                                        PersonalFilter::Collaborator => {
                                            source_map.personal.collaborator.clone()
                                        }
                                    }
                                } else {
                                    Vec::new()
                                };
                                personal_filter.set(filter);
                                if let AppState::Dashboard {
                                    repos: ref mut r,
                                    selected_repos: ref mut sr,
                                    cards: ref mut c,
                                    ..
                                } = *state.write()
                                {
                                    *r = repos;
                                    *sr = Vec::new();
                                    *c = Vec::new();
                                }
                            },
                            on_toggle_repo: move |idx: usize| {
                                let token = token_for_toggle.clone();
                                let repos = repos_for_toggle.clone();
                                let source = source_for_toggle.clone();
                                let user = user_for_toggle.clone();
                                let mut state = state;
                                let mut board_loading = board_loading;
                                let mut app_settings = app_settings;

                                // Read current selected repos
                                let mut current_selected = if let AppState::Dashboard {
                                    ref selected_repos, ..
                                } = state()
                                {
                                    selected_repos.clone()
                                } else {
                                    return;
                                };

                                if let Some(pos) = current_selected.iter().position(|&i| i == idx)
                                {
                                    // Unchecked → rebuild cards from remaining repos
                                    current_selected.remove(pos);
                                    let owner = owner_for_source(&source, &user.login);

                                    // Rebuild cards from remaining selected repos' caches
                                    let mut rebuilt = Vec::new();
                                    for &si in &current_selected {
                                        if let Some(rn) = repos.get(si)
                                            && let Some(c) = load_cards(&owner, rn)
                                        {
                                            rebuilt.extend(c);
                                        }
                                    }
                                    rebuilt.sort_by(|a, b| a.priority.cmp(&b.priority));

                                    if let AppState::Dashboard {
                                        ref mut selected_repos,
                                        ref mut cards,
                                        ..
                                    } = *state.write()
                                    {
                                        *selected_repos = current_selected.clone();
                                        *cards = rebuilt;
                                    }

                                    // Save last state
                                    let names: Vec<String> = current_selected
                                        .iter()
                                        .filter_map(|&i| repos.get(i).cloned())
                                        .collect();
                                    let mut s = app_settings();
                                    s.last_repos = names;
                                    save_settings(&s);
                                    app_settings.set(s);
                                } else {
                                    // Checked → add and fetch cards for this repo
                                    current_selected.push(idx);
                                    if let AppState::Dashboard {
                                        ref mut selected_repos,
                                        ..
                                    } = *state.write()
                                    {
                                        *selected_repos = current_selected.clone();
                                    }

                                    if let Some(repo_name) = repos.get(idx) {
                                        let repo_name = repo_name.clone();
                                        let owner = owner_for_source(&source, &user.login);

                                        // Show cached cards instantly
                                        if let Some(cached) = load_cards(&owner, &repo_name)
                                            && let AppState::Dashboard {
                                                ref mut cards, ..
                                            } = *state.write()
                                        {
                                            cards.extend(cached);
                                            cards.sort_by(|a, b| {
                                                a.priority.cmp(&b.priority)
                                            });
                                        }

                                        // Skip sync if cache is fresh (< 3h)
                                        if is_cards_fresh(&owner, &repo_name) {
                                            // Save last state and return
                                            let names: Vec<String> = current_selected
                                                .iter()
                                                .filter_map(|&i| repos.get(i).cloned())
                                                .collect();
                                            let mut s = app_settings();
                                            s.last_repos = names;
                                            save_settings(&s);
                                            app_settings.set(s);
                                            return;
                                        }

                                        // Background fetch fresh
                                        spawn(async move {
                                            board_loading.set(true);
                                            let new_cards =
                                                fetch_cards(&token, &owner, &repo_name)
                                                    .await;
                                            save_cards(&owner, &repo_name, &new_cards);

                                            // Fetch labels if cache is stale (1 month TTL).
                                            if !is_labels_fresh(&owner, &repo_name) {
                                                let client = RestClient::new(token.clone());
                                                if let Ok(labels) = client.list_labels(&owner, &repo_name).await {
                                                    save_labels(&owner, &repo_name, &labels);
                                                }
                                            }

                                            if let AppState::Dashboard {
                                                ref selected_repos,
                                                ref mut cards,
                                                ref repos,
                                                ..
                                            } = *state.write()
                                            {
                                                // Rebuild all cards from selected
                                                let mut all = Vec::new();
                                                for &si in selected_repos.iter() {
                                                    if let Some(rn) = repos.get(si) {
                                                        if rn == &repo_name {
                                                            all.extend(new_cards.clone());
                                                        } else if let Some(c) =
                                                            load_cards(&owner, rn)
                                                        {
                                                            all.extend(c);
                                                        }
                                                    }
                                                }
                                                all.sort_by(|a, b| {
                                                    a.priority.cmp(&b.priority)
                                                });
                                                *cards = all;
                                            }
                                            board_loading.set(false);
                                        });
                                    }

                                    // Save last state
                                    let names: Vec<String> = current_selected
                                        .iter()
                                        .filter_map(|&i| repos.get(i).cloned())
                                        .collect();
                                    let mut s = app_settings();
                                    s.last_repos = names;
                                    save_settings(&s);
                                    app_settings.set(s);
                                }
                            },
                            on_select_source: move |new_source: SourceKind| {
                                let token = token_for_source.clone();
                                let mut state = state;
                                let mut app_settings = app_settings;
                                let new_source_clone = new_source.clone();
                                let settings = app_settings();

                                // Derive repos from current source_map (in memory)
                                let repos = if let AppState::Dashboard {
                                    ref source_map, ..
                                } = state()
                                {
                                    match &new_source_clone {
                                        SourceKind::Personal => {
                                            source_map.personal.owner.clone()
                                        }
                                        SourceKind::Organization(org) => {
                                            source_map.repos_for_source(org)
                                        }
                                    }
                                } else {
                                    Vec::new()
                                };

                                // Apply source switch synchronously (repos appear instantly)
                                let default_selected: Vec<usize> = settings
                                    .last_repos
                                    .iter()
                                    .filter_map(|name| repos.iter().position(|r| r == name))
                                    .collect();

                                if let AppState::Dashboard {
                                    source: ref mut s,
                                    repos: ref mut r,
                                    selected_repos: ref mut sr,
                                    cards: ref mut c,
                                    ..
                                } = *state.write()
                                {
                                    *s = new_source.clone();
                                    *r = repos;
                                    *sr = default_selected;
                                    *c = Vec::new();
                                }

                                // Reset personal filter when switching sources
                                personal_filter.set(PersonalFilter::default());

                                // Persist last source
                                let mut sett = load_settings();
                                let source_name = match &new_source {
                                    SourceKind::Personal => "personal".to_string(),
                                    SourceKind::Organization(org) => org.clone(),
                                };
                                sett.last_source = Some(source_name);
                                save_settings(&sett);
                                app_settings.set(sett);

                                // Fetch members in the background if stale
                                if let SourceKind::Organization(ref org) = new_source_clone
                                    && !is_members_fresh(org)
                                {
                                    let org = org.clone();
                                    let mut board_loading = board_loading;
                                    spawn(async move {
                                        board_loading.set(true);
                                        let client = RestClient::new(token);
                                        if let Ok(members) = client.list_members(&org).await {
                                            save_members(&org, &members);
                                        }
                                        board_loading.set(false);
                                    });
                                }
                            },
                            on_toggle: move |_| {
                                sidebar_collapsed.set(!sidebar_collapsed());
                            },
                            on_toggle_theme: move |_| {
                                dark_mode.set(!dark_mode());
                            },
                            on_settings: move |_| {
                                show_settings.set(!show_settings());
                            },
                            on_sign_out: move |_| {
                                // Clear all cached data on sign out
                                clear_all_cache();
                                app_settings.set(AppSettings::default());
                                state.set(AppState::Login);
                            },
                        }
                        Board {
                            cards: cards,
                            repo_name: repo_display,
                            loading: board_loading(),
                            on_refresh: move |_| {
                                let token = token_for_refresh.clone();
                                let source = source_for_refresh.clone();
                                let user = user_for_refresh.clone();
                                let repos = repos_for_refresh.clone();
                                let selected = selected_for_refresh.clone();
                                let mut state = state;
                                let mut board_loading = board_loading;
                                if !selected.is_empty() {
                                    let owner = owner_for_source(&source, &user.login);
                                    spawn(async move {
                                        board_loading.set(true);
                                        let mut all_cards = Vec::new();
                                        for &idx in &selected {
                                            if let Some(repo_name) = repos.get(idx) {
                                                let cards = fetch_cards(
                                                    &token, &owner, repo_name,
                                                )
                                                .await;
                                                save_cards(&owner, repo_name, &cards);
                                                all_cards.extend(cards);
                                            }
                                        }
                                        all_cards.sort_by(|a, b| {
                                            a.priority.cmp(&b.priority)
                                        });
                                        if let AppState::Dashboard {
                                            cards: ref mut c, ..
                                        } = *state.write()
                                        {
                                            *c = all_cards;
                                        }
                                        board_loading.set(false);
                                    });
                                }
                            },
                            on_card_click: move |card: Card| {
                                selected_card.set(Some(card));
                            },
                            on_create: move |_| {
                                show_create_issue.set(true);
                            },
                            members: members_full.clone(),
                        }

                        // Card detail panel
                        if let Some(card) = selected_card() {
                            CardDetail {
                                card: card,
                                token: token.clone(),
                                user_login: user.login.clone(),
                                members: members_full.clone(),
                                cards: cards_ac.clone(),
                                repo_labels: repo_labels.clone(),
                                on_close: move |_| {
                                    selected_card.set(None);
                                },
                                on_closed: move |updated_card: Card| {
                                    selected_card.set(None);
                                    // Move card to closed state locally (no sync).
                                    if let AppState::Dashboard { cards: ref mut c, .. } = *state.write()
                                        && let Some(pos) = c.iter().position(|card| {
                                            let n1 = match &card.source {
                                                CardSource::Issue(i) => i.number,
                                                CardSource::PullRequest(p) => p.number,
                                            };
                                            let n2 = match &updated_card.source {
                                                CardSource::Issue(i) => i.number,
                                                CardSource::PullRequest(p) => p.number,
                                            };
                                            n1 == n2 && card.owner == updated_card.owner && card.repo == updated_card.repo
                                        })
                                    {
                                        c[pos] = updated_card;
                                    }
                                },
                            }
                        }

                        // Create issue modal
                        if show_create_issue() {
                            {
                                let owner_ci = owner_for_source(&source, &user.login);
                                let selected_ci = selected_repos.clone();
                                // Build repos list from selected indices.
                                let repos_list: Vec<String> = selected_ci
                                    .iter()
                                    .filter_map(|&i| repos.get(i).cloned())
                                    .collect();
                                let token_ci = token.clone();
                                let source_refresh = source.clone();
                                let user_refresh = user.clone();
                                let repos_refresh = repos.clone();
                                let selected_refresh = selected_repos.clone();
                                let token_ci_refresh = token_for_rc.clone();
                                rsx! {
                                    CreateIssue {
                                        token: token_ci,
                                        owner: owner_ci.clone(),
                                        repos: repos_list,
                                        members: members_logins,
                                        cards: cards_ac,
                                        repo_labels: repo_labels.clone(),
                                        user_login: user.login.clone(),
                                        on_close: move |_| {
                                            show_create_issue.set(false);
                                        },
                                        on_created: move |_| {
                                            let token = token_ci_refresh.clone();
                                            let source = source_refresh.clone();
                                            let user = user_refresh.clone();
                                            let repos = repos_refresh.clone();
                                            let selected = selected_refresh.clone();
                                            let owner = owner_for_source(&source, &user.login);
                                            let mut state = state;
                                            let mut board_loading = board_loading;
                                            spawn(async move {
                                                board_loading.set(true);
                                                let mut all_cards = Vec::new();
                                                for &idx in &selected {
                                                    if let Some(repo_name) = repos.get(idx) {
                                                        let cards = fetch_cards(&token, &owner, repo_name).await;
                                                        save_cards(&owner, repo_name, &cards);
                                                        all_cards.extend(cards);
                                                    }
                                                }
                                                all_cards.sort_by(|a, b| a.priority.cmp(&b.priority));
                                                if let AppState::Dashboard { cards: ref mut c, .. } = *state.write() {
                                                    *c = all_cards;
                                                }
                                                board_loading.set(false);
                                            });
                                        },
                                    }
                                }
                            }
                        }

                        // Settings panel
                        if show_settings() {
                            {
                                let source_display = match &source {
                                    SourceKind::Personal => "Personal".to_string(),
                                    SourceKind::Organization(org) => org.clone(),
                                };
                                let repos_for_settings = repos.clone();
                                let sk_for_default = sk.clone();
                                let token_rs = token_for_rs.clone();
                                let user_rs = user_for_rs.clone();
                                let token_rc = token_for_rc.clone();
                                let source_rc = source_for_rc.clone();
                                let user_rc = user_for_rc.clone();
                                let repos_rc = repos_for_rc.clone();
                                let selected_rc = selected_for_rc.clone();
                                rsx! {
                                    Settings {
                                        source_name: source_display,
                                        repos: repos_for_settings,
                                        default_repo: default_repo,
                                        cached_sources_count: cached_sources_count,
                                        cached_repos_count: cached_repos_count,
                                        cached_closed_count: cached_closed_count,
                                        on_set_default_repo: move |repo: Option<String>| {
                                            let sk = sk_for_default.clone();
                                            let mut s = app_settings();
                                            if let Some(name) = repo {
                                                s.default_repos.insert(sk, name);
                                            } else {
                                                s.default_repos.remove(&sk);
                                            }
                                            save_settings(&s);
                                            app_settings.set(s);
                                        },
                                        on_refresh_sources: move |_| {
                                            let token = token_rs.clone();
                                            let user = user_rs.clone();
                                            let mut state = state;
                                            let mut board_loading = board_loading;
                                            spawn(async move {
                                                board_loading.set(true);
                                                let new_map = build_source_map(
                                                    &token, &user.login,
                                                )
                                                .await;
                                                save_source_map(&new_map);

                                                // Derive repos for current source
                                                let current_source = if let AppState::Dashboard {
                                                    ref source, ..
                                                } = state()
                                                {
                                                    source.clone()
                                                } else {
                                                    SourceKind::Personal
                                                };
                                                let src_key = match &current_source {
                                                    SourceKind::Personal => {
                                                        "personal".to_string()
                                                    }
                                                    SourceKind::Organization(org) => org.clone(),
                                                };
                                                let new_repos =
                                                    new_map.repos_for_source(&src_key);

                                                if let AppState::Dashboard {
                                                    source_map: ref mut sm,
                                                    repos: ref mut r,
                                                    selected_repos: ref mut sr,
                                                    cards: ref mut c,
                                                    ..
                                                } = *state.write()
                                                {
                                                    *sm = new_map;
                                                    *r = new_repos;
                                                    *sr = Vec::new();
                                                    *c = Vec::new();
                                                }
                                                board_loading.set(false);
                                            });
                                        },
                                        on_refresh_closed: move |_| {
                                            let token = token_rc.clone();
                                            let source = source_rc.clone();
                                            let user = user_rc.clone();
                                            let repos = repos_rc.clone();
                                            let selected = selected_rc.clone();
                                            let mut state = state;
                                            let mut board_loading = board_loading;
                                            let owner =
                                                owner_for_source(&source, &user.login);
                                            if !selected.is_empty() {
                                                spawn(async move {
                                                    board_loading.set(true);
                                                    for &idx in &selected {
                                                        if let Some(repo_name) = repos.get(idx)
                                                        {
                                                            let closed = fetch_closed_only(
                                                                &token, &owner, repo_name,
                                                            )
                                                            .await;
                                                            save_closed_issues(
                                                                &owner, repo_name, &closed,
                                                            );
                                                        }
                                                    }
                                                    // Rebuild cards
                                                    let mut all_cards = Vec::new();
                                                    for &idx in &selected {
                                                        if let Some(repo_name) = repos.get(idx)
                                                        {
                                                            let cards = fetch_cards(
                                                                &token, &owner, repo_name,
                                                            )
                                                            .await;
                                                            save_cards(
                                                                &owner, repo_name, &cards,
                                                            );
                                                            all_cards.extend(cards);
                                                        }
                                                    }
                                                    all_cards.sort_by(|a, b| {
                                                        a.priority.cmp(&b.priority)
                                                    });
                                                    if let AppState::Dashboard {
                                                        cards: ref mut c, ..
                                                    } = *state.write()
                                                    {
                                                        *c = all_cards;
                                                    }
                                                    board_loading.set(false);
                                                });
                                            }
                                        },
                                        on_close: move |_| {
                                            show_settings.set(false);
                                        },
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

// ── GitHub API integration ───────────────────────────────────────────

/// Validate token and load initial data. Uses per-type cache TTLs.
async fn authenticate_and_load(token: String) -> Result<AppState, String> {
    let client = RestClient::new(token.clone());

    // Profile: cache-first with 1-month TTL
    let user = if is_profile_fresh() {
        if let Some(cached) = load_profile() {
            cached
        } else {
            let fetched = client
                .get_authenticated_user()
                .await
                .map_err(|e| format!("Authentication failed: {e}"))?;
            save_profile(&fetched);
            fetched
        }
    } else {
        let fetched = client
            .get_authenticated_user()
            .await
            .map_err(|e| format!("Authentication failed: {e}"))?;
        save_profile(&fetched);
        fetched
    };

    // Unified source map: cache-first with 6-month TTL
    let source_map = if is_source_map_fresh() {
        load_source_map().unwrap_or_default()
    } else {
        let map = build_source_map(&token, &user.login).await;
        save_source_map(&map);
        map
    };

    // Restore last state
    let settings = load_settings();
    let orgs = source_map.org_names();
    let source = match &settings.last_source {
        Some(s) if s != "personal" && orgs.contains(s) => SourceKind::Organization(s.clone()),
        _ => SourceKind::Personal,
    };

    // Derive repos from source map (personal defaults to owned only)
    let repos = match &source {
        SourceKind::Personal => source_map.personal.owner.clone(),
        SourceKind::Organization(org) => source_map.repos_for_source(org),
    };

    // Fetch members when restoring an org source (if not cached)
    if let SourceKind::Organization(ref org) = source
        && !is_members_fresh(org)
        && let Ok(members) = client.list_members(org).await
    {
        save_members(org, &members);
    }

    // Restore last selected repos
    let selected_repos: Vec<usize> = settings
        .last_repos
        .iter()
        .filter_map(|name| repos.iter().position(|r| r == name))
        .collect();

    // Load cached cards for selected repos; re-fetch inline if cache is
    // missing or corrupt (seamless cache migration).
    let user_login = user.login.clone();
    let owner = owner_for_source(&source, &user_login);
    let mut cards = Vec::new();
    for &idx in &selected_repos {
        if let Some(repo_name) = repos.get(idx) {
            let repo_cards = match load_cards(&owner, repo_name) {
                Some(cached) => cached,
                None => {
                    let fetched = fetch_cards(&token, &owner, repo_name).await;
                    save_cards(&owner, repo_name, &fetched);
                    fetched
                }
            };
            cards.extend(repo_cards);
        }
    }
    cards.sort_by(|a, b| a.priority.cmp(&b.priority));

    Ok(AppState::Dashboard {
        user,
        token,
        source_map,
        source,
        repos,
        selected_repos,
        cards,
    })
}

/// Build a unified [`SourceMap`] from parallel `/user/repos` + `/user/orgs`.
///
/// Categorises each repo as owned/member or collaborator based on
/// owner type and the set of org memberships.
async fn build_source_map(token: &str, user_login: &str) -> SourceMap {
    use std::collections::{BTreeMap, HashSet};

    let client = RestClient::new(token.to_string());

    // Parallel fetch
    let (all_repos_res, orgs_res) = tokio::join!(client.list_all_repos(), client.list_orgs());

    let all_repos = all_repos_res.unwrap_or_default();
    let member_orgs: HashSet<String> = orgs_res
        .unwrap_or_default()
        .into_iter()
        .map(|o| o.login)
        .collect();

    let mut personal = SourceRepos::default();
    let mut member_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut collaborator_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for repo in all_repos {
        if repo.archived {
            continue;
        }
        if repo.owner_type == "Organization" {
            if member_orgs.contains(&repo.owner) {
                member_map
                    .entry(repo.owner.clone())
                    .or_default()
                    .push(repo.name);
            } else {
                collaborator_map
                    .entry(repo.owner.clone())
                    .or_default()
                    .push(repo.name);
            }
        } else if repo.owner == user_login {
            personal.owner.push(repo.name);
        } else {
            personal.collaborator.push(repo.name);
        }
    }

    SourceMap {
        personal,
        organizations: OrgSources {
            member: member_map,
            collaborator: collaborator_map,
        },
    }
}

/// Fetch all closed issues for initial caching.
async fn fetch_closed_only(
    token: &str,
    owner: &str,
    repo: &str,
) -> Vec<cardman_core::models::Issue> {
    let client = RestClient::new(token.to_string());
    client
        .list_closed_issues(owner, repo, None)
        .await
        .unwrap_or_default()
}

/// Fetch issues and PRs for a repo, map to cards.
///
/// Uses incremental sync per data type:
/// - Open issues: full first time, `since` filter after (replaced)
/// - Closed issues: 1 page of 100 first time, `since` filter after (cumulative)
/// - Open PRs: always full pagination (with reviews/CI)
/// - Closed PRs: full first time, `paginate_until` after (cumulative)
async fn fetch_cards(token: &str, owner: &str, repo: &str) -> Vec<Card> {
    let client = RestClient::new(token.to_string());
    let config = MappingConfig::default();

    // ── Open issues ────────────────────────────────────────────────
    let open_since = open_sync_time(owner, repo);
    let fetched_open = client
        .list_open_issues(owner, repo, open_since)
        .await
        .unwrap_or_default();

    // Merge with cached open issues (replace by number, add new)
    let open_issues = if open_since.is_some() {
        let mut cached = load_open_issues(owner, repo).unwrap_or_default();
        for issue in &fetched_open {
            if let Some(pos) = cached.iter().position(|e| e.number == issue.number) {
                cached[pos] = issue.clone();
            } else {
                cached.push(issue.clone());
            }
        }
        // Remove issues that are now closed (they appear in closed fetch)
        cached.retain(|i| i.state == cardman_core::models::IssueState::Open);
        cached
    } else {
        fetched_open
    };
    save_open_issues(owner, repo, &open_issues);

    // ── Closed issues (cumulative) ───────────────────────────────
    let closed_since = closed_sync_time(owner, repo);
    let new_closed = client
        .list_closed_issues(owner, repo, closed_since)
        .await
        .unwrap_or_default();
    if !new_closed.is_empty() {
        save_closed_issues(owner, repo, &new_closed);
    }
    let cached_closed = load_closed_issues(owner, repo).unwrap_or_default();
    let all_issues: Vec<_> = open_issues.into_iter().chain(cached_closed).collect();

    // ── Open PRs (incremental, with reviews/CI) ─────────────────
    let pr_open_since = prs_sync_time(owner, repo);
    let fetched_open_prs = client
        .list_open_prs(owner, repo, pr_open_since)
        .await
        .unwrap_or_default();

    // Merge with cached open PRs (replace by number, add new)
    let open_prs = if pr_open_since.is_some() {
        let mut cached = load_prs(owner, repo).unwrap_or_default();
        for pr in &fetched_open_prs {
            if let Some(pos) = cached.iter().position(|e| e.number == pr.number) {
                cached[pos] = pr.clone();
            } else {
                cached.push(pr.clone());
            }
        }
        cached
    } else {
        fetched_open_prs
    };

    // ── Closed PRs (cumulative, no reviews/CI) ──────────────────
    let pr_since = merged_sync_time(owner, repo);
    let new_closed_prs = client
        .list_closed_prs(owner, repo, pr_since)
        .await
        .unwrap_or_default();
    if !new_closed_prs.is_empty() {
        save_merged_prs(owner, repo, &new_closed_prs);
    }
    let cached_merged = load_merged_prs(owner, repo).unwrap_or_default();

    // Dedup: remove from open PRs any that are now in closed cache
    let open_prs: Vec<_> = open_prs
        .into_iter()
        .filter(|p| !cached_merged.iter().any(|c| c.number == p.number))
        .collect();
    save_prs(owner, repo, &open_prs);

    let all_prs: Vec<_> = open_prs.into_iter().chain(cached_merged).collect();

    // ── Map to cards ──────────────────────────────────────────────
    let mut cards: Vec<Card> = all_issues
        .into_iter()
        .map(|i| map_card(owner, repo, CardSource::Issue(i), &config))
        .chain(
            all_prs
                .into_iter()
                .map(|p| map_card(owner, repo, CardSource::PullRequest(p), &config)),
        )
        .collect();

    cards.sort_by(|a, b| a.priority.cmp(&b.priority));
    cards
}
