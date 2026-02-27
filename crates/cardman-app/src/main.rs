//! Cardman desktop application entry point.
//!
//! Manages authentication state and renders either the login screen
//! (GitHub PAT input) or the main dashboard (sidebar + kanban board).

mod cache;
mod components;

use cache::{
    has_closed_issues, load_cards, load_closed_issues, load_repos, load_settings, load_sources,
    save_cards, save_closed_issues, save_repos, save_settings, save_sources, source_key,
};
use cardman_core::github::RestClient;
use cardman_core::mapping::{MappingConfig, map_card};
use cardman_core::models::{Card, CardSource, IssueState, User};
use components::board::Board;
use components::detail::CardDetail;
use components::login::LoginScreen;
use components::settings::Settings;
use components::sidebar::{Sidebar, SourceKind};
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
        user: User,
        token: String,
        orgs: Vec<String>,
        source: SourceKind,
        repos: Vec<String>,
        selected_repo: Option<usize>,
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

/// Root application component.
fn app() -> Element {
    let mut state = use_signal(|| AppState::Login);
    let mut dark_mode = use_signal(|| true);
    let mut sidebar_collapsed = use_signal(|| false);
    let mut login_error = use_signal(|| Option::<String>::None);
    let board_loading = use_signal(|| false);
    let mut selected_card = use_signal(|| Option::<Card>::None);
    let mut show_settings = use_signal(|| false);
    let mut app_settings = use_signal(load_settings);

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
                            spawn(async move {
                                match authenticate_and_load(token.clone()).await {
                                    Ok(dashboard) => state.set(dashboard),
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
            orgs,
            source,
            repos,
            selected_repo,
            cards,
        } => {
            let repo_display = selected_repo
                .and_then(|i| repos.get(i))
                .cloned()
                .unwrap_or_else(|| "No repository".into());

            let source_display = match &source {
                SourceKind::Personal => "Personal".to_string(),
                SourceKind::Organization(org) => org.clone(),
            };

            // Compute cache counts for settings
            let src_key = match &source {
                SourceKind::Personal => "personal".to_string(),
                SourceKind::Organization(org) => source_key(org),
            };
            let cached_sources_count = load_sources().map(|s| s.len());
            let cached_repos_count = load_repos(&src_key).map(|r| r.len());
            let cached_closed_count = selected_repo.and_then(|i| {
                repos.get(i).and_then(|repo_name| {
                    let owner = match &source {
                        SourceKind::Personal => user.login.clone(),
                        SourceKind::Organization(org) => org.clone(),
                    };
                    load_closed_issues(&owner, repo_name).map(|c| c.len())
                })
            });

            let default_repo = app_settings().default_repos.get(&src_key).cloned();

            // Pre-clone values captured by multiple closures
            let token_for_repo = token.clone();
            let token_for_source = token.clone();
            let token_for_refresh = token.clone();
            let token_for_rs = token.clone();
            let token_for_rr = token.clone();
            let token_for_rc = token.clone();
            let repos_for_select = repos.clone();
            let source_for_select = source.clone();
            let source_for_refresh = source.clone();
            let user_login_for_source = user.login.clone();
            let user_for_refresh = user.clone();
            let selected_repo_for_refresh = selected_repo;
            let repos_for_refresh = repos.clone();

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
                            repos: repos.clone(),
                            selected_repo: selected_repo,
                            on_select_repo: move |idx: usize| {
                                let token = token_for_repo.clone();
                                let repos = repos_for_select.clone();
                                let source = source_for_select.clone();
                                let mut state = state;
                                let mut board_loading = board_loading;
                                if let Some(repo_name) = repos.get(idx) {
                                    let repo_name = repo_name.clone();

                                    // Compute owner for cache lookup
                                    let owner = if let AppState::Dashboard {
                                        ref user, ..
                                    } = state()
                                    {
                                        match &source {
                                            SourceKind::Personal => user.login.clone(),
                                            SourceKind::Organization(org) => org.clone(),
                                        }
                                    } else {
                                        String::new()
                                    };

                                    // Show cached cards instantly
                                    let cached = load_cards(&owner, &repo_name);
                                    if let AppState::Dashboard {
                                        ref mut selected_repo,
                                        cards: ref mut c,
                                        ..
                                    } = *state.write()
                                    {
                                        *selected_repo = Some(idx);
                                        if let Some(ref cached_cards) = cached {
                                            *c = cached_cards.clone();
                                        }
                                    }

                                    // Background fetch fresh data
                                    spawn(async move {
                                        board_loading.set(true);
                                        let cards =
                                            fetch_cards(&token, &owner, &repo_name).await;
                                        save_cards(&owner, &repo_name, &cards);
                                        if let AppState::Dashboard {
                                            cards: ref mut c, ..
                                        } = *state.write()
                                        {
                                            *c = cards;
                                        }
                                        board_loading.set(false);
                                    });
                                }
                            },
                            on_select_source: move |new_source: SourceKind| {
                                let token = token_for_source.clone();
                                let user_login = user_login_for_source.clone();
                                let mut state = state;
                                let mut board_loading = board_loading;
                                let new_source_clone = new_source.clone();
                                let settings = app_settings();
                                spawn(async move {
                                    board_loading.set(true);

                                    // Check cache for repos
                                    let sk = match &new_source_clone {
                                        SourceKind::Personal => "personal".to_string(),
                                        SourceKind::Organization(org) => source_key(org),
                                    };
                                    let repos = if let Some(cached) = load_repos(&sk) {
                                        cached
                                    } else {
                                        let fetched = fetch_repos_for_source(
                                            &token,
                                            &new_source_clone,
                                            &user_login,
                                        )
                                        .await;
                                        save_repos(&sk, &fetched);
                                        fetched
                                    };

                                    // Auto-select default repo if configured
                                    let default_idx = settings
                                        .default_repos
                                        .get(&sk)
                                        .and_then(|name| repos.iter().position(|r| r == name));

                                    if let AppState::Dashboard {
                                        source: ref mut s,
                                        repos: ref mut r,
                                        selected_repo: ref mut sr,
                                        cards: ref mut c,
                                        ..
                                    } = *state.write()
                                    {
                                        *s = new_source;
                                        *r = repos;
                                        *sr = default_idx;
                                        *c = Vec::new();
                                    }
                                    board_loading.set(false);
                                });
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
                                let selected = selected_repo_for_refresh;
                                let mut state = state;
                                let mut board_loading = board_loading;
                                if let Some(idx) = selected
                                    && let Some(repo_name) = repos.get(idx)
                                {
                                    let repo_name = repo_name.clone();
                                    spawn(async move {
                                        board_loading.set(true);
                                        let owner = match &source {
                                            SourceKind::Personal => user.login.clone(),
                                            SourceKind::Organization(org) => org.clone(),
                                        };
                                        let cards =
                                            fetch_cards(&token, &owner, &repo_name).await;
                                        if let AppState::Dashboard {
                                            cards: ref mut c, ..
                                        } = *state.write()
                                        {
                                            *c = cards;
                                        }
                                        board_loading.set(false);
                                    });
                                }
                            },
                            on_card_click: move |card: Card| {
                                selected_card.set(Some(card));
                            },
                        }

                        // Card detail panel (right side)
                        if let Some(card) = selected_card() {
                            CardDetail {
                                card: card,
                                on_close: move |_| {
                                    selected_card.set(None);
                                },
                            }
                        }

                        // Settings panel (right side)
                        if show_settings() {
                            {
                                let source_display = source_display.clone();
                                let repos = repos.clone();
                                let src_key_for_default = src_key.clone();
                                let token_rs = token_for_rs.clone();
                                let token_rr = token_for_rr.clone();
                                let token_rc = token_for_rc.clone();
                                let source_rr = source.clone();
                                let source_rc = source.clone();
                                let user_rr = user.clone();
                                let user_rc = user.clone();
                                let repos_rc = repos.clone();
                                let selected_rc = selected_repo;
                                rsx! {
                                    Settings {
                                        source_name: source_display,
                                        repos: repos,
                                        default_repo: default_repo,
                                        cached_sources_count: cached_sources_count,
                                        cached_repos_count: cached_repos_count,
                                        cached_closed_count: cached_closed_count,
                                        on_set_default_repo: move |repo: Option<String>| {
                                            let sk = src_key_for_default.clone();
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
                                            let mut state = state;
                                            let mut board_loading = board_loading;
                                            spawn(async move {
                                                board_loading.set(true);
                                                let client = RestClient::new(token);
                                                let orgs = client
                                                    .list_orgs()
                                                    .await
                                                    .map(|o| {
                                                        o.into_iter()
                                                            .map(|org| org.login)
                                                            .collect::<Vec<_>>()
                                                    })
                                                    .unwrap_or_default();
                                                save_sources(&orgs);
                                                if let AppState::Dashboard {
                                                    orgs: ref mut o, ..
                                                } = *state.write()
                                                {
                                                    *o = orgs;
                                                }
                                                board_loading.set(false);
                                            });
                                        },
                                        on_refresh_repos: move |_| {
                                            let token = token_rr.clone();
                                            let source = source_rr.clone();
                                            let user = user_rr.clone();
                                            let mut state = state;
                                            let mut board_loading = board_loading;
                                            spawn(async move {
                                                board_loading.set(true);
                                                let repos = fetch_repos_for_source(
                                                    &token, &source, &user.login,
                                                )
                                                .await;
                                                let sk = match &source {
                                                    SourceKind::Personal => "personal".to_string(),
                                                    SourceKind::Organization(org) => {
                                                        source_key(org)
                                                    }
                                                };
                                                save_repos(&sk, &repos);
                                                if let AppState::Dashboard {
                                                    repos: ref mut r,
                                                    selected_repo: ref mut sr,
                                                    cards: ref mut c,
                                                    ..
                                                } = *state.write()
                                                {
                                                    *r = repos;
                                                    *sr = None;
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
                                            let selected = selected_rc;
                                            let mut state = state;
                                            let mut board_loading = board_loading;
                                            if let Some(idx) = selected
                                                && let Some(repo_name) = repos.get(idx)
                                            {
                                                let repo_name = repo_name.clone();
                                                spawn(async move {
                                                    board_loading.set(true);
                                                    let owner = match &source {
                                                        SourceKind::Personal => {
                                                            user.login.clone()
                                                        }
                                                        SourceKind::Organization(org) => {
                                                            org.clone()
                                                        }
                                                    };
                                                    let closed =
                                                        fetch_closed_only(&token, &owner, &repo_name)
                                                            .await;
                                                    save_closed_issues(&owner, &repo_name, &closed);
                                                    // Re-fetch full cards to reflect
                                                    let cards = fetch_cards(
                                                        &token, &owner, &repo_name,
                                                    )
                                                    .await;
                                                    if let AppState::Dashboard {
                                                        cards: ref mut c, ..
                                                    } = *state.write()
                                                    {
                                                        *c = cards;
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

/// Validate the token and load initial data (user, orgs, repos).
/// Uses cache when available.
async fn authenticate_and_load(token: String) -> Result<AppState, String> {
    let client = RestClient::new(token.clone());

    // Validate token by fetching the authenticated user
    let user = client
        .get_authenticated_user()
        .await
        .map_err(|e| format!("Authentication failed: {e}"))?;

    // Orgs: try cache, fall back to API
    let orgs = if let Some(cached) = load_sources() {
        cached
    } else {
        let fetched = client
            .list_orgs()
            .await
            .map(|o| o.into_iter().map(|org| org.login).collect::<Vec<_>>())
            .unwrap_or_default();
        save_sources(&fetched);
        fetched
    };

    // Repos: try cache, fall back to API
    let user_login = user.login.clone();
    let repos = if let Some(cached) = load_repos("personal") {
        cached
    } else {
        let fetched = client
            .list_repos()
            .await
            .map(|r| {
                r.into_iter()
                    .filter(|repo| !repo.archived && repo.owner == user_login)
                    .map(|repo| repo.name)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        save_repos("personal", &fetched);
        fetched
    };

    // Auto-select default repo if configured
    let settings = load_settings();
    let default_idx = settings
        .default_repos
        .get("personal")
        .and_then(|name| repos.iter().position(|r| r == name));

    Ok(AppState::Dashboard {
        user,
        token,
        orgs,
        source: SourceKind::Personal,
        repos,
        selected_repo: default_idx,
        cards: Vec::new(),
    })
}

/// Fetch repos for a given source kind.
async fn fetch_repos_for_source(token: &str, source: &SourceKind, user_login: &str) -> Vec<String> {
    let client = RestClient::new(token.to_string());
    match source {
        SourceKind::Personal => client
            .list_repos()
            .await
            .map(|r| {
                r.into_iter()
                    .filter(|repo| !repo.archived && repo.owner == user_login)
                    .map(|repo| repo.name)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        SourceKind::Organization(org) => client
            .list_org_repos(org)
            .await
            .map(|r| {
                r.into_iter()
                    .filter(|repo| !repo.archived)
                    .map(|repo| repo.name)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    }
}

/// Fetch only recent closed issues for caching.
async fn fetch_closed_only(
    token: &str,
    owner: &str,
    repo: &str,
) -> Vec<cardman_core::models::Issue> {
    let client = RestClient::new(token.to_string());
    client
        .list_issues(owner, repo)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|i| i.state == IssueState::Closed)
        .collect()
}

/// Fetch issues and PRs for a given repo and map them to cards.
/// Uses cached closed issues when available.
async fn fetch_cards(token: &str, owner: &str, repo: &str) -> Vec<Card> {
    let client = RestClient::new(token.to_string());
    let config = MappingConfig::default();

    // If closed issues are cached, only fetch open from API
    let issues = if has_closed_issues(owner, repo) {
        // Fetch open issues only
        let open = client
            .list_issues(owner, repo)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|i| i.state == IssueState::Open)
            .collect::<Vec<_>>();
        // Merge with cached closed
        let closed = load_closed_issues(owner, repo).unwrap_or_default();
        let mut all = open;
        all.extend(closed);
        all
    } else {
        let fetched = client.list_issues(owner, repo).await.unwrap_or_default();
        // Cache the closed ones for next time
        let closed: Vec<_> = fetched
            .iter()
            .filter(|i| i.state == IssueState::Closed)
            .cloned()
            .collect();
        if !closed.is_empty() {
            save_closed_issues(owner, repo, &closed);
        }
        fetched
    };

    let prs = client
        .list_pull_requests(owner, repo)
        .await
        .unwrap_or_default();

    let mut cards: Vec<Card> = issues
        .into_iter()
        .map(|i| map_card(CardSource::Issue(i), &config))
        .chain(
            prs.into_iter()
                .map(|p| map_card(CardSource::PullRequest(p), &config)),
        )
        .collect();

    cards.sort_by(|a, b| a.priority.cmp(&b.priority));
    cards
}
