//! GitHub API client traits and platform-specific implementations.
//!
//! Defines an async `GitHubClient` trait with methods for listing repositories,
//! issues, pull requests, reviews, and CI status. The native implementation uses
//! `reqwest`; a Wasm implementation using `gloo-net` will follow in Phase 4.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::models::{
    AuthenticatedUser, CiStatus, Comment, Issue, IssueState, Label, Organization, PullRequest,
    Repository, Review, ReviewState, User,
};

/// Errors that may occur when interacting with the GitHub API.
#[derive(Debug)]
pub enum GitHubError {
    /// HTTP request failed.
    Http(String),
    /// JSON deserialization failed.
    Deserialize(String),
    /// Authentication error (e.g. expired token).
    Auth(String),
    /// Rate limit exceeded. Contains the reset timestamp (Unix epoch seconds).
    RateLimit(u64),
    /// Resource not found.
    NotFound(String),
}

impl fmt::Display for GitHubError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(msg) => write!(f, "HTTP error: {msg}"),
            Self::Deserialize(msg) => write!(f, "deserialization error: {msg}"),
            Self::Auth(msg) => write!(f, "authentication error: {msg}"),
            Self::RateLimit(reset) => write!(f, "rate limit exceeded, resets at {reset}"),
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
        }
    }
}

impl std::error::Error for GitHubError {}

// ── Raw GitHub API response types ────────────────────────────────────
// These mirror the GitHub REST v3 JSON schema, converting to domain types.

/// GitHub API user response.
#[derive(Debug, Deserialize)]
struct GhUser {
    login: String,
    avatar_url: String,
}

impl From<GhUser> for User {
    fn from(u: GhUser) -> Self {
        Self {
            login: u.login,
            avatar_url: u.avatar_url,
        }
    }
}

/// GitHub API authenticated user response (includes display name).
#[derive(Debug, Deserialize)]
struct GhAuthUser {
    login: String,
    avatar_url: String,
    name: Option<String>,
}

impl From<GhAuthUser> for AuthenticatedUser {
    fn from(u: GhAuthUser) -> Self {
        Self {
            login: u.login,
            avatar_url: u.avatar_url,
            name: u.name,
        }
    }
}

/// GitHub API repository response.
#[derive(Debug, Deserialize)]
struct GhRepo {
    name: String,
    archived: bool,
    default_branch: String,
    owner: GhRepoOwner,
}

#[derive(Debug, Deserialize)]
struct GhRepoOwner {
    login: String,
    /// Account type: `"User"` or `"Organization"`.
    #[serde(rename = "type")]
    owner_type: String,
}

impl From<GhRepo> for Repository {
    fn from(r: GhRepo) -> Self {
        Self {
            owner: r.owner.login,
            owner_type: r.owner.owner_type,
            name: r.name,
            archived: r.archived,
            default_branch: r.default_branch,
        }
    }
}

/// Filter out archived repositories from a raw API response.
fn exclude_archived(repos: Vec<GhRepo>) -> Vec<Repository> {
    repos
        .into_iter()
        .filter(|r| !r.archived)
        .map(Into::into)
        .collect()
}

/// GitHub API label response.
#[derive(Debug, Deserialize)]
struct GhLabel {
    name: String,
    color: String,
}

impl From<GhLabel> for Label {
    fn from(l: GhLabel) -> Self {
        Self {
            name: l.name,
            color: l.color,
        }
    }
}

/// GitHub API issue response.
#[derive(Debug, Deserialize)]
struct GhIssue {
    number: u64,
    title: String,
    body: Option<String>,
    labels: Vec<GhLabel>,
    assignees: Vec<GhUser>,
    state: String,
    updated_at: DateTime<Utc>,
    /// Issue author.
    user: GhUser,
    /// PR field presence indicates this is actually a PR, not an issue.
    pull_request: Option<serde_json::Value>,
}

impl From<GhIssue> for Issue {
    fn from(i: GhIssue) -> Self {
        Self {
            number: i.number,
            title: i.title,
            body: i.body,
            labels: i.labels.into_iter().map(Into::into).collect(),
            assignees: i.assignees.into_iter().map(|u| u.login).collect(),
            state: match i.state.as_str() {
                "open" => IssueState::Open,
                _ => IssueState::Closed,
            },
            sub_issues: Vec::new(),
            author: i.user.login,
            updated_at: i.updated_at,
        }
    }
}

/// GitHub API pull request response.
#[derive(Debug, Deserialize)]
struct GhPullRequest {
    number: u64,
    title: String,
    /// PR body (GitHub-flavored markdown).
    body: Option<String>,
    #[allow(dead_code)]
    state: String,
    /// `null` for open/closed PRs, UTC timestamp for merged PRs.
    merged_at: Option<DateTime<Utc>>,
    /// Whether this is a draft PR.
    #[serde(default)]
    draft: bool,
    /// PR author.
    user: GhUser,
    /// Assigned users.
    #[serde(default)]
    assignees: Vec<GhUser>,
    /// Users requested to review.
    #[serde(default)]
    requested_reviewers: Vec<GhUser>,
    head: GhPrHead,
    labels: Vec<GhLabel>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct GhPrHead {
    #[serde(rename = "ref")]
    branch_ref: String,
}

/// GitHub API review response.
#[derive(Debug, Deserialize)]
struct GhReview {
    user: GhUser,
    state: String,
}

impl From<GhReview> for Review {
    fn from(r: GhReview) -> Self {
        Self {
            user: r.user.into(),
            state: match r.state.as_str() {
                "APPROVED" => ReviewState::Approved,
                "CHANGES_REQUESTED" => ReviewState::ChangesRequested,
                "COMMENTED" => ReviewState::Commented,
                "DISMISSED" => ReviewState::Dismissed,
                _ => ReviewState::Pending,
            },
        }
    }
}

/// GitHub API combined commit status response.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Deserialize)]
struct GhCombinedStatus {
    state: String,
}

/// GitHub API comment response.
#[derive(Debug, Deserialize)]
struct GhComment {
    id: u64,
    user: GhUser,
    body: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<GhComment> for Comment {
    fn from(c: GhComment) -> Self {
        Self {
            id: c.id,
            user: c.user.into(),
            body: c.body.unwrap_or_default(),
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

/// GitHub API organization response.
#[derive(Debug, Deserialize)]
struct GhOrg {
    login: String,
}

// ── Issue update payload ─────────────────────────────────────────────

/// Partial update payload for a GitHub issue.
///
/// All fields are optional; only set fields are sent to the API.
#[derive(Debug, Clone, Default)]
pub struct IssueUpdate {
    /// New title.
    pub title: Option<String>,
    /// New body.
    pub body: Option<String>,
    /// New state.
    pub state: Option<IssueState>,
    /// Replace labels.
    pub labels: Option<Vec<String>>,
    /// Replace assignees (login names).
    pub assignees: Option<Vec<String>>,
}

// ── Native (reqwest) client ──────────────────────────────────────────

/// GitHub REST API client using `reqwest` (native targets only).
#[cfg(not(target_arch = "wasm32"))]
pub struct RestClient {
    http: reqwest::Client,
    token: String,
    base_url: String,
}

#[cfg(not(target_arch = "wasm32"))]
impl RestClient {
    /// Create a new GitHub REST client.
    ///
    /// `token` is a GitHub OAuth access token.
    pub fn new(token: String) -> Self {
        Self::with_base_url(token, "https://api.github.com".to_string())
    }

    /// Create a client pointing at a custom base URL (for testing).
    pub fn with_base_url(token: String, base_url: String) -> Self {
        let http = reqwest::Client::builder()
            .user_agent("kardhub/0.1")
            .build()
            .expect("failed to build reqwest client");
        Self {
            http,
            token,
            base_url,
        }
    }

    /// Build an authenticated GET request.
    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.http
            .get(format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
    }

    /// Build an authenticated POST request.
    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.http
            .post(format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
    }

    /// Build an authenticated PATCH request.
    fn patch(&self, path: &str) -> reqwest::RequestBuilder {
        self.http
            .patch(format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
    }

    /// Build an authenticated DELETE request.
    fn delete_req(&self, path: &str) -> reqwest::RequestBuilder {
        self.http
            .delete(format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
    }

    /// Handle a response, mapping HTTP errors to [`GitHubError`].
    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self,
        resp: reqwest::Response,
    ) -> Result<T, GitHubError> {
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            // Check for rate limiting via header
            if let Some(reset) = resp
                .headers()
                .get("x-ratelimit-reset")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
            {
                let remaining = resp
                    .headers()
                    .get("x-ratelimit-remaining")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok());
                if remaining == Some(0) {
                    return Err(GitHubError::RateLimit(reset));
                }
            }
            let text = resp.text().await.unwrap_or_default();
            return Err(GitHubError::Auth(text));
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            let text = resp.text().await.unwrap_or_default();
            return Err(GitHubError::NotFound(text));
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(GitHubError::Http(format!("{status}: {text}")));
        }
        resp.json::<T>()
            .await
            .map_err(|e| GitHubError::Deserialize(e.to_string()))
    }

    /// Fetch all pages from a paginated GitHub API endpoint.
    async fn paginate<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<Vec<T>, GitHubError> {
        let mut all = Vec::new();
        let mut page = 1u32;
        loop {
            let separator = if path.contains('?') { '&' } else { '?' };
            let url = format!("{path}{separator}per_page=100&page={page}");
            let resp = self
                .get(&url)
                .send()
                .await
                .map_err(|e| GitHubError::Http(e.to_string()))?;
            let items: Vec<T> = self.handle_response(resp).await?;
            if items.is_empty() {
                break;
            }
            all.extend(items);
            page += 1;
        }
        Ok(all)
    }

    /// Paginate a PR endpoint until items are older than `cutoff`.
    ///
    /// Expects the endpoint to return items sorted by `updated` desc.
    async fn paginate_until(
        &self,
        path: &str,
        cutoff: DateTime<Utc>,
    ) -> Result<Vec<GhPullRequest>, GitHubError> {
        let mut all = Vec::new();
        let mut page = 1u32;
        loop {
            let separator = if path.contains('?') { '&' } else { '?' };
            let url = format!("{path}{separator}per_page=100&page={page}");
            let resp = self
                .get(&url)
                .send()
                .await
                .map_err(|e| GitHubError::Http(e.to_string()))?;
            let items: Vec<GhPullRequest> = self.handle_response(resp).await?;
            if items.is_empty() {
                break;
            }
            let mut hit_cutoff = false;
            for item in items {
                if item.updated_at < cutoff {
                    hit_cutoff = true;
                    break;
                }
                all.push(item);
            }
            if hit_cutoff {
                break;
            }
            page += 1;
        }
        Ok(all)
    }

    // ── Public API ───────────────────────────────────────────────────

    /// Get the authenticated user.
    pub async fn get_authenticated_user(&self) -> Result<AuthenticatedUser, GitHubError> {
        let resp = self
            .get("/user")
            .send()
            .await
            .map_err(|e| GitHubError::Http(e.to_string()))?;
        let user: GhAuthUser = self.handle_response(resp).await?;
        Ok(user.into())
    }

    /// List repositories for the authenticated user.
    ///
    /// Excludes archived repositories.
    pub async fn list_repos(&self) -> Result<Vec<Repository>, GitHubError> {
        let repos: Vec<GhRepo> = self.paginate("/user/repos?type=all&sort=updated").await?;
        Ok(exclude_archived(repos))
    }

    /// List all repositories accessible to the authenticated user.
    ///
    /// Returns every repo (owned, member-org, collaborator) excluding
    /// archived ones. Used to build the unified [`SourceMap`].
    pub async fn list_all_repos(&self) -> Result<Vec<Repository>, GitHubError> {
        let repos: Vec<GhRepo> = self.paginate("/user/repos?type=all&sort=updated").await?;
        Ok(exclude_archived(repos))
    }

    /// List repositories for an organization.
    ///
    /// Excludes archived repositories.
    pub async fn list_org_repos(&self, org: &str) -> Result<Vec<Repository>, GitHubError> {
        let repos: Vec<GhRepo> = self
            .paginate(&format!("/orgs/{org}/repos?type=all&sort=updated"))
            .await?;
        Ok(exclude_archived(repos))
    }

    /// List organizations the authenticated user belongs to.
    pub async fn list_orgs(&self) -> Result<Vec<Organization>, GitHubError> {
        let orgs: Vec<GhOrg> = self.paginate("/user/orgs").await?;
        Ok(orgs
            .into_iter()
            .map(|o| Organization {
                login: o.login,
                is_main: false,
            })
            .collect())
    }

    /// List organization members and outside collaborators.
    ///
    /// Fetches both `/orgs/{org}/members` and `/orgs/{org}/outside_collaborators`,
    /// deduplicates by login, and returns the combined set.
    pub async fn list_members(&self, org: &str) -> Result<Vec<User>, GitHubError> {
        let members: Vec<GhUser> = self
            .paginate(&format!("/orgs/{org}/members"))
            .await
            .unwrap_or_default();
        let collaborators: Vec<GhUser> = self
            .paginate(&format!("/orgs/{org}/outside_collaborators"))
            .await
            .unwrap_or_default();

        let mut all: Vec<User> = members.into_iter().map(Into::into).collect();
        for c in collaborators {
            let user: User = c.into();
            if !all.iter().any(|m| m.login == user.login) {
                all.push(user);
            }
        }
        Ok(all)
    }

    /// List open issues for a repository (excludes pull requests).
    ///
    /// When `since` is `Some`, fetches only open issues updated after that
    /// timestamp (incremental). When `None`, fetches all open issues (full).
    pub async fn list_open_issues(
        &self,
        owner: &str,
        repo: &str,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<Issue>, GitHubError> {
        let path = match since {
            Some(ts) => {
                let ts_str = ts.to_rfc3339();
                format!(
                    "/repos/{owner}/{repo}/issues?state=open&sort=updated&direction=desc&since={ts_str}"
                )
            }
            None => format!("/repos/{owner}/{repo}/issues?state=open"),
        };
        let items: Vec<GhIssue> = self.paginate(&path).await?;

        Ok(items
            .into_iter()
            .filter(|i| i.pull_request.is_none())
            .map(Into::into)
            .collect())
    }

    /// List closed issues for a repository (excludes pull requests).
    ///
    /// When `since` is `Some`, fetches closed issues updated after that
    /// timestamp (incremental). When `None`, fetches a single page of 100
    /// most recently updated closed issues (first sync).
    pub async fn list_closed_issues(
        &self,
        owner: &str,
        repo: &str,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<Issue>, GitHubError> {
        let items: Vec<GhIssue> = match since {
            Some(ts) => {
                let ts_str = ts.to_rfc3339();
                let path = format!(
                    "/repos/{owner}/{repo}/issues?state=closed&sort=updated&direction=desc&since={ts_str}"
                );
                self.paginate(&path).await?
            }
            None => {
                // First sync: single page of 100
                let resp = self
                    .get(&format!(
                        "/repos/{owner}/{repo}/issues?state=closed&sort=updated&direction=desc&per_page=100"
                    ))
                    .send()
                    .await
                    .map_err(|e| GitHubError::Http(e.to_string()))?;
                self.handle_response(resp).await?
            }
        };

        Ok(items
            .into_iter()
            .filter(|i| i.pull_request.is_none())
            .map(Into::into)
            .collect())
    }

    /// List open pull requests for a repository.
    ///
    /// When `since` is `Some`, paginates `state=open` sorted by updated desc
    /// and stops at the cutoff (incremental). When `None`, fetches all open
    /// PRs (full pagination).
    ///
    /// Fetch logic per PR:
    /// - Draft → skip reviews & CI
    /// - No `requested_reviewers` → skip reviews & CI
    /// - Has reviewers → fetch reviews → `changes_requested` → skip CI
    ///   → otherwise → fetch CI
    pub async fn list_open_prs(
        &self,
        owner: &str,
        repo: &str,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<PullRequest>, GitHubError> {
        let items: Vec<GhPullRequest> = match since {
            Some(ts) => {
                self.paginate_until(
                    &format!("/repos/{owner}/{repo}/pulls?state=open&sort=updated&direction=desc"),
                    ts,
                )
                .await?
            }
            None => {
                self.paginate(&format!("/repos/{owner}/{repo}/pulls?state=open"))
                    .await?
            }
        };

        let mut prs = Vec::with_capacity(items.len());
        for item in items {
            let requested: Vec<String> = item
                .requested_reviewers
                .into_iter()
                .map(|u| u.login)
                .collect();

            // Draft or no requested reviewers → skip reviews & CI
            let (reviews, ci) = if item.draft || requested.is_empty() {
                (Vec::new(), CiStatus::Pending)
            } else {
                let r = self
                    .get_reviews(owner, repo, item.number)
                    .await
                    .unwrap_or_default();
                let ci = if r.is_empty() {
                    CiStatus::Pending
                } else {
                    self.get_ci_status(owner, repo, &item.head.branch_ref)
                        .await
                        .unwrap_or(CiStatus::Pending)
                };
                (r, ci)
            };

            let author = item.user.login;
            let assignees: Vec<String> = item.assignees.into_iter().map(|u| u.login).collect();
            // Fallback: if no explicit assignees, treat author as assignee.
            let assignees = if assignees.is_empty() {
                vec![author.clone()]
            } else {
                assignees
            };

            prs.push(PullRequest {
                number: item.number,
                title: item.title,
                body: item.body,
                draft: item.draft,
                author,
                assignees,
                requested_reviewers: requested,
                reviews,
                ci_status: ci,
                merged: false,
                closed: false,
                branch: item.head.branch_ref,
                labels: item.labels.into_iter().map(Into::into).collect(),
                updated_at: item.updated_at,
            });
        }
        Ok(prs)
    }

    /// List closed (merged + closed-not-merged) pull requests.
    ///
    /// When `since` is `Some`, paginates `state=closed` sorted by updated
    /// desc and stops at the cutoff (incremental). When `None`, fetches
    /// only the most recent 100 closed PRs (first sync optimisation).
    pub async fn list_closed_prs(
        &self,
        owner: &str,
        repo: &str,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<PullRequest>, GitHubError> {
        let items: Vec<GhPullRequest> = match since {
            Some(ts) => {
                self.paginate_until(
                    &format!(
                        "/repos/{owner}/{repo}/pulls?state=closed&sort=updated&direction=desc"
                    ),
                    ts,
                )
                .await?
            }
            None => {
                // First sync: fetch only the latest page (100 results).
                let path = format!(
                    "/repos/{owner}/{repo}/pulls?state=closed&sort=updated&direction=desc&per_page=100"
                );
                let resp = self
                    .get(&path)
                    .send()
                    .await
                    .map_err(|e| GitHubError::Http(e.to_string()))?;
                self.handle_response(resp).await?
            }
        };

        Ok(items
            .into_iter()
            .map(|item| {
                let merged = item.merged_at.is_some();
                let author = item.user.login;
                let assignees: Vec<String> = item.assignees.into_iter().map(|u| u.login).collect();
                let assignees = if assignees.is_empty() {
                    vec![author.clone()]
                } else {
                    assignees
                };
                PullRequest {
                    number: item.number,
                    title: item.title,
                    body: item.body,
                    draft: false,
                    author,
                    assignees,
                    requested_reviewers: Vec::new(),
                    reviews: Vec::new(),
                    ci_status: CiStatus::Pending,
                    merged,
                    closed: !merged,
                    branch: item.head.branch_ref,
                    labels: item.labels.into_iter().map(Into::into).collect(),
                    updated_at: item.updated_at,
                }
            })
            .collect())
    }

    /// Get reviews for a pull request.
    pub async fn get_reviews(
        &self,
        owner: &str,
        repo: &str,
        pr_number: u64,
    ) -> Result<Vec<Review>, GitHubError> {
        let reviews: Vec<GhReview> = self
            .paginate(&format!("/repos/{owner}/{repo}/pulls/{pr_number}/reviews"))
            .await?;
        Ok(reviews.into_iter().map(Into::into).collect())
    }

    /// Get the combined CI status for a ref (branch or SHA).
    pub async fn get_ci_status(
        &self,
        owner: &str,
        repo: &str,
        git_ref: &str,
    ) -> Result<CiStatus, GitHubError> {
        let resp = self
            .get(&format!("/repos/{owner}/{repo}/commits/{git_ref}/status"))
            .send()
            .await
            .map_err(|e| GitHubError::Http(e.to_string()))?;
        let status: GhCombinedStatus = self.handle_response(resp).await?;
        Ok(match status.state.as_str() {
            "success" => CiStatus::Success,
            "failure" | "error" => CiStatus::Failure,
            _ => CiStatus::Pending,
        })
    }

    /// List labels for a repository.
    pub async fn list_labels(&self, owner: &str, repo: &str) -> Result<Vec<Label>, GitHubError> {
        let labels: Vec<GhLabel> = self
            .paginate(&format!("/repos/{owner}/{repo}/labels"))
            .await?;
        Ok(labels.into_iter().map(Into::into).collect())
    }

    /// Create an issue in a repository.
    pub async fn create_issue(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        body: Option<&str>,
        labels: &[String],
        assignees: &[String],
    ) -> Result<Issue, GitHubError> {
        let mut payload = serde_json::json!({ "title": title });
        if let Some(b) = body {
            payload["body"] = serde_json::Value::String(b.to_string());
        }
        if !labels.is_empty() {
            payload["labels"] = serde_json::json!(labels);
        }
        if !assignees.is_empty() {
            payload["assignees"] = serde_json::json!(assignees);
        }
        let resp = self
            .post(&format!("/repos/{owner}/{repo}/issues"))
            .json(&payload)
            .send()
            .await
            .map_err(|e| GitHubError::Http(e.to_string()))?;
        let issue: GhIssue = self.handle_response(resp).await?;
        Ok(issue.into())
    }

    /// Update an issue using an [`IssueUpdate`] payload.
    pub async fn update_issue(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
        update: &IssueUpdate,
    ) -> Result<Issue, GitHubError> {
        let mut payload = serde_json::Map::new();
        if let Some(t) = &update.title {
            payload.insert("title".into(), serde_json::Value::String(t.clone()));
        }
        if let Some(b) = &update.body {
            payload.insert("body".into(), serde_json::Value::String(b.clone()));
        }
        if let Some(s) = &update.state {
            let s_str = match s {
                IssueState::Open => "open",
                IssueState::Closed => "closed",
            };
            payload.insert("state".into(), serde_json::Value::String(s_str.to_string()));
        }
        if let Some(l) = &update.labels {
            payload.insert("labels".into(), serde_json::json!(l));
        }
        if let Some(a) = &update.assignees {
            payload.insert("assignees".into(), serde_json::json!(a));
        }
        let resp = self
            .patch(&format!("/repos/{owner}/{repo}/issues/{issue_number}"))
            .json(&serde_json::Value::Object(payload))
            .send()
            .await
            .map_err(|e| GitHubError::Http(e.to_string()))?;
        let issue: GhIssue = self.handle_response(resp).await?;
        Ok(issue.into())
    }

    /// List comments on an issue or pull request.
    pub async fn list_comments(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Vec<Comment>, GitHubError> {
        let items: Vec<GhComment> = self
            .paginate(&format!("/repos/{owner}/{repo}/issues/{number}/comments"))
            .await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    /// Add a comment to an issue or pull request.
    pub async fn add_comment(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &str,
    ) -> Result<Comment, GitHubError> {
        let payload = serde_json::json!({ "body": body });
        let resp = self
            .post(&format!("/repos/{owner}/{repo}/issues/{number}/comments"))
            .json(&payload)
            .send()
            .await
            .map_err(|e| GitHubError::Http(e.to_string()))?;
        let comment: GhComment = self.handle_response(resp).await?;
        Ok(comment.into())
    }

    /// Update an existing comment.
    pub async fn update_comment(
        &self,
        owner: &str,
        repo: &str,
        comment_id: u64,
        body: &str,
    ) -> Result<Comment, GitHubError> {
        let payload = serde_json::json!({ "body": body });
        let resp = self
            .patch(&format!(
                "/repos/{owner}/{repo}/issues/comments/{comment_id}"
            ))
            .json(&payload)
            .send()
            .await
            .map_err(|e| GitHubError::Http(e.to_string()))?;
        let comment: GhComment = self.handle_response(resp).await?;
        Ok(comment.into())
    }

    /// Update a pull request title, body, and/or state.
    pub async fn update_pr(
        &self,
        owner: &str,
        repo: &str,
        pr_number: u64,
        title: Option<&str>,
        body: Option<&str>,
        state: Option<&str>,
    ) -> Result<(), GitHubError> {
        let mut payload = serde_json::Map::new();
        if let Some(t) = title {
            payload.insert("title".into(), serde_json::Value::String(t.to_string()));
        }
        if let Some(b) = body {
            payload.insert("body".into(), serde_json::Value::String(b.to_string()));
        }
        if let Some(s) = state {
            payload.insert("state".into(), serde_json::Value::String(s.to_string()));
        }
        let resp = self
            .patch(&format!("/repos/{owner}/{repo}/pulls/{pr_number}"))
            .json(&serde_json::Value::Object(payload))
            .send()
            .await
            .map_err(|e| GitHubError::Http(e.to_string()))?;
        self.handle_response::<serde_json::Value>(resp).await?;
        Ok(())
    }

    /// Delete a branch by reference name.
    ///
    /// Uses `DELETE /repos/{owner}/{repo}/git/refs/heads/{branch}`.
    pub async fn delete_branch(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<(), GitHubError> {
        let resp = self
            .delete_req(&format!("/repos/{owner}/{repo}/git/refs/heads/{branch}"))
            .send()
            .await
            .map_err(|e| GitHubError::Http(e.to_string()))?;
        let status = resp.status();
        if status.is_success() || status == reqwest::StatusCode::NO_CONTENT {
            return Ok(());
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            // Branch already deleted — treat as success.
            return Ok(());
        }
        let text = resp.text().await.unwrap_or_default();
        Err(GitHubError::Http(format!("{status}: {text}")))
    }

    /// Close a pull request and delete its head branch.
    ///
    /// Combines `update_pr(state="closed")` with `delete_branch`.
    pub async fn close_pr(
        &self,
        owner: &str,
        repo: &str,
        pr_number: u64,
        branch: &str,
    ) -> Result<(), GitHubError> {
        self.update_pr(owner, repo, pr_number, None, None, Some("closed"))
            .await?;
        self.delete_branch(owner, repo, branch).await?;
        Ok(())
    }
}

// ── Search response type ─────────────────────────────────────────────

/// GitHub search issues/PRs response wrapper.
#[derive(Debug, Deserialize)]
struct GhSearchResult {
    items: Vec<GhIssue>,
}

// ── search_issues on RestClient ──────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
impl RestClient {
    /// Search issues in repositories by query string.
    ///
    /// Uses `GET /search/issues?q={query}`. The query should include
    /// qualifiers like `repo:owner/name` for scoped searches.
    pub async fn search_issues(&self, query: &str) -> Result<Vec<Issue>, GitHubError> {
        let encoded: String = query
            .bytes()
            .flat_map(|b| match b {
                b' ' => b"+".to_vec(),
                b'A'..=b'Z'
                | b'a'..=b'z'
                | b'0'..=b'9'
                | b'-'
                | b'_'
                | b'.'
                | b':'
                | b'/'
                | b'#' => vec![b],
                _ => format!("%{b:02X}").into_bytes(),
            })
            .map(|b| b as char)
            .collect();
        let resp = self
            .get(&format!("/search/issues?q={encoded}&per_page=30"))
            .send()
            .await
            .map_err(|e| GitHubError::Http(e.to_string()))?;
        let result: GhSearchResult = self.handle_response(resp).await?;
        Ok(result
            .items
            .into_iter()
            .filter(|i| i.pull_request.is_none())
            .map(Into::into)
            .collect())
    }
}

// ── Wasm (gloo-net) client ───────────────────────────────────────────

/// GitHub REST API client using `gloo-net` (wasm targets only).
///
/// Mirrors the [`RestClient`] surface for the subset of endpoints needed
/// by the browser extension: authentication, repository listing, issue/PR
/// fetching, issue search, and PR body updates.
#[cfg(target_arch = "wasm32")]
pub struct WasmClient {
    token: String,
    base_url: String,
}

#[cfg(target_arch = "wasm32")]
impl WasmClient {
    /// Create a new wasm GitHub client.
    ///
    /// `token` is a GitHub Personal Access Token.
    pub fn new(token: String) -> Self {
        Self {
            token,
            base_url: "https://api.github.com".to_string(),
        }
    }

    /// Build and send an authenticated GET request, returning deserialized JSON.
    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, GitHubError> {
        let url = format!("{}{path}", self.base_url);
        let resp = gloo_net::http::Request::get(&url)
            .header("Authorization", &format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|e| GitHubError::Http(e.to_string()))?;
        Self::handle_response(resp).await
    }

    /// Build and send an authenticated PATCH request with a JSON body.
    async fn patch<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T, GitHubError> {
        let url = format!("{}{path}", self.base_url);
        let resp = gloo_net::http::Request::patch(&url)
            .header("Authorization", &format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(body)
            .map_err(|e| GitHubError::Http(e.to_string()))?
            .send()
            .await
            .map_err(|e| GitHubError::Http(e.to_string()))?;
        Self::handle_response(resp).await
    }

    /// Handle a gloo-net response, mapping HTTP errors to [`GitHubError`].
    async fn handle_response<T: serde::de::DeserializeOwned>(
        resp: gloo_net::http::Response,
    ) -> Result<T, GitHubError> {
        let status = resp.status();
        if status == 401 || status == 403 {
            let text = resp.text().await.unwrap_or_default();
            return Err(GitHubError::Auth(text));
        }
        if status == 404 {
            let text = resp.text().await.unwrap_or_default();
            return Err(GitHubError::NotFound(text));
        }
        if status < 200 || status >= 300 {
            let text = resp.text().await.unwrap_or_default();
            return Err(GitHubError::Http(format!("{status}: {text}")));
        }
        resp.json::<T>()
            .await
            .map_err(|e| GitHubError::Deserialize(e.to_string()))
    }

    /// Fetch all pages from a paginated endpoint.
    async fn paginate<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<Vec<T>, GitHubError> {
        let mut all = Vec::new();
        let mut page = 1u32;
        loop {
            let separator = if path.contains('?') { '&' } else { '?' };
            let url = format!("{path}{separator}per_page=100&page={page}");
            let items: Vec<T> = self.get(&url).await?;
            if items.is_empty() {
                break;
            }
            all.extend(items);
            page += 1;
        }
        Ok(all)
    }

    // ── Public API ───────────────────────────────────────────────────

    /// Get the authenticated user.
    pub async fn get_authenticated_user(&self) -> Result<AuthenticatedUser, GitHubError> {
        let user: GhAuthUser = self.get("/user").await?;
        Ok(user.into())
    }

    /// List all repositories accessible to the authenticated user.
    ///
    /// Excludes archived repositories.
    pub async fn list_all_repos(&self) -> Result<Vec<Repository>, GitHubError> {
        let repos: Vec<GhRepo> = self.paginate("/user/repos?type=all&sort=updated").await?;
        Ok(exclude_archived(repos))
    }

    /// List organizations the authenticated user belongs to.
    pub async fn list_orgs(&self) -> Result<Vec<Organization>, GitHubError> {
        let orgs: Vec<GhOrg> = self.paginate("/user/orgs").await?;
        Ok(orgs
            .into_iter()
            .map(|o| Organization {
                login: o.login,
                is_main: false,
            })
            .collect())
    }

    /// List open issues for a repository (excludes pull requests).
    pub async fn list_open_issues(
        &self,
        owner: &str,
        repo: &str,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<Issue>, GitHubError> {
        let path = match since {
            Some(ts) => {
                let ts_str = ts.to_rfc3339();
                format!(
                    "/repos/{owner}/{repo}/issues?state=open&sort=updated&direction=desc&since={ts_str}"
                )
            }
            None => format!("/repos/{owner}/{repo}/issues?state=open"),
        };
        let items: Vec<GhIssue> = self.paginate(&path).await?;
        Ok(items
            .into_iter()
            .filter(|i| i.pull_request.is_none())
            .map(Into::into)
            .collect())
    }

    /// List closed issues for a repository (excludes pull requests).
    pub async fn list_closed_issues(
        &self,
        owner: &str,
        repo: &str,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<Issue>, GitHubError> {
        let items: Vec<GhIssue> = match since {
            Some(ts) => {
                let ts_str = ts.to_rfc3339();
                let path = format!(
                    "/repos/{owner}/{repo}/issues?state=closed&sort=updated&direction=desc&since={ts_str}"
                );
                self.paginate(&path).await?
            }
            None => {
                self.get(&format!(
                    "/repos/{owner}/{repo}/issues?state=closed&sort=updated&direction=desc&per_page=100"
                ))
                .await?
            }
        };
        Ok(items
            .into_iter()
            .filter(|i| i.pull_request.is_none())
            .map(Into::into)
            .collect())
    }

    /// List open pull requests for a repository (simplified, no review/CI enrichment).
    pub async fn list_open_prs(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<PullRequest>, GitHubError> {
        let items: Vec<GhPullRequest> = self
            .paginate(&format!(
                "/repos/{owner}/{repo}/pulls?state=open&sort=updated&direction=desc"
            ))
            .await?;
        Ok(items
            .into_iter()
            .map(|item| {
                let author = item.user.login;
                let assignees: Vec<String> = item.assignees.into_iter().map(|u| u.login).collect();
                let assignees = if assignees.is_empty() {
                    vec![author.clone()]
                } else {
                    assignees
                };
                PullRequest {
                    number: item.number,
                    title: item.title,
                    body: item.body,
                    draft: item.draft,
                    author,
                    assignees,
                    requested_reviewers: item
                        .requested_reviewers
                        .into_iter()
                        .map(|u| u.login)
                        .collect(),
                    reviews: Vec::new(),
                    ci_status: CiStatus::Pending,
                    merged: false,
                    closed: false,
                    branch: item.head.branch_ref,
                    labels: item.labels.into_iter().map(Into::into).collect(),
                    updated_at: item.updated_at,
                }
            })
            .collect())
    }

    /// List closed pull requests for a repository.
    pub async fn list_closed_prs(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<PullRequest>, GitHubError> {
        let items: Vec<GhPullRequest> = self
            .get(&format!(
                "/repos/{owner}/{repo}/pulls?state=closed&sort=updated&direction=desc&per_page=100"
            ))
            .await?;
        Ok(items
            .into_iter()
            .map(|item| {
                let merged = item.merged_at.is_some();
                let author = item.user.login;
                let assignees: Vec<String> = item.assignees.into_iter().map(|u| u.login).collect();
                let assignees = if assignees.is_empty() {
                    vec![author.clone()]
                } else {
                    assignees
                };
                PullRequest {
                    number: item.number,
                    title: item.title,
                    body: item.body,
                    draft: false,
                    author,
                    assignees,
                    requested_reviewers: Vec::new(),
                    reviews: Vec::new(),
                    ci_status: CiStatus::Pending,
                    merged,
                    closed: !merged,
                    branch: item.head.branch_ref,
                    labels: item.labels.into_iter().map(Into::into).collect(),
                    updated_at: item.updated_at,
                }
            })
            .collect())
    }

    /// Search issues in repositories by query string.
    ///
    /// Uses `GET /search/issues?q={query}`. The query should include
    /// qualifiers like `repo:owner/name` for scoped searches.
    pub async fn search_issues(&self, query: &str) -> Result<Vec<Issue>, GitHubError> {
        let encoded = js_sys::encode_uri_component(query);
        let result: GhSearchResult = self
            .get(&format!("/search/issues?q={encoded}&per_page=30"))
            .await?;
        Ok(result
            .items
            .into_iter()
            .filter(|i| i.pull_request.is_none())
            .map(Into::into)
            .collect())
    }

    /// Update a pull request body.
    pub async fn update_pr(
        &self,
        owner: &str,
        repo: &str,
        pr_number: u64,
        title: Option<&str>,
        body: Option<&str>,
        state: Option<&str>,
    ) -> Result<(), GitHubError> {
        let mut payload = serde_json::Map::new();
        if let Some(t) = title {
            payload.insert("title".into(), serde_json::Value::String(t.to_string()));
        }
        if let Some(b) = body {
            payload.insert("body".into(), serde_json::Value::String(b.to_string()));
        }
        if let Some(s) = state {
            payload.insert("state".into(), serde_json::Value::String(s.to_string()));
        }
        let _resp: serde_json::Value = self
            .patch(
                &format!("/repos/{owner}/{repo}/pulls/{pr_number}"),
                &serde_json::Value::Object(payload),
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gh_user_converts_to_domain() {
        let gh = GhUser {
            login: "octocat".into(),
            avatar_url: "https://example.com/avatar.png".into(),
        };
        let user: User = gh.into();
        assert_eq!(user.login, "octocat");
        assert_eq!(user.avatar_url, "https://example.com/avatar.png");
    }

    #[test]
    fn gh_repo_converts_to_domain() {
        let gh = GhRepo {
            name: "hello-world".into(),
            archived: false,
            default_branch: "main".into(),
            owner: GhRepoOwner {
                login: "octocat".into(),
                owner_type: "User".into(),
            },
        };
        let repo: Repository = gh.into();
        assert_eq!(repo.owner, "octocat");
        assert_eq!(repo.owner_type, "User");
        assert_eq!(repo.name, "hello-world");
        assert!(!repo.archived);
    }

    #[test]
    fn exclude_archived_filters_correctly() {
        let repos = vec![
            GhRepo {
                name: "active".into(),
                archived: false,
                default_branch: "main".into(),
                owner: GhRepoOwner {
                    login: "user".into(),
                    owner_type: "User".into(),
                },
            },
            GhRepo {
                name: "old".into(),
                archived: true,
                default_branch: "main".into(),
                owner: GhRepoOwner {
                    login: "user".into(),
                    owner_type: "User".into(),
                },
            },
        ];
        let result = exclude_archived(repos);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "active");
    }

    #[test]
    fn gh_repo_owner_type_deserializes() {
        let json = r#"{"login":"my-org","type":"Organization"}"#;
        let owner: GhRepoOwner = serde_json::from_str(json).unwrap();
        assert_eq!(owner.login, "my-org");
        assert_eq!(owner.owner_type, "Organization");
    }

    #[test]
    fn gh_issue_filters_pr_field() {
        let gh = GhIssue {
            number: 1,
            title: "Bug fix".into(),
            body: None,
            labels: vec![],
            assignees: vec![],
            state: "open".into(),
            updated_at: "2024-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap(),
            user: GhUser {
                login: "author".into(),
                avatar_url: String::new(),
            },
            pull_request: Some(serde_json::json!({})),
        };
        // pull_request field is present, so this should be filtered out when listing issues
        assert!(gh.pull_request.is_some());
    }

    #[test]
    fn gh_review_converts_states() {
        let cases = vec![
            ("APPROVED", ReviewState::Approved),
            ("CHANGES_REQUESTED", ReviewState::ChangesRequested),
            ("COMMENTED", ReviewState::Commented),
            ("DISMISSED", ReviewState::Dismissed),
            ("PENDING", ReviewState::Pending),
            ("unknown", ReviewState::Pending),
        ];
        for (raw, expected) in cases {
            let review = GhReview {
                user: GhUser {
                    login: "test".into(),
                    avatar_url: String::new(),
                },
                state: raw.into(),
            };
            let domain: Review = review.into();
            assert_eq!(domain.state, expected, "failed for state '{raw}'");
        }
    }

    #[test]
    fn github_error_display() {
        let err = GitHubError::RateLimit(1700000000);
        assert!(err.to_string().contains("rate limit"));
        let err = GitHubError::NotFound("repo".into());
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn issue_update_default_omits_assignees() {
        let update = IssueUpdate::default();
        assert!(update.assignees.is_none());
        assert!(update.title.is_none());
        assert!(update.body.is_none());
        assert!(update.state.is_none());
        assert!(update.labels.is_none());
    }

    #[test]
    fn issue_update_serializes_assignees() {
        let update = IssueUpdate {
            assignees: Some(vec!["octocat".to_string()]),
            ..Default::default()
        };
        assert_eq!(
            update.assignees.as_ref().unwrap(),
            &vec!["octocat".to_string()]
        );
    }
}
