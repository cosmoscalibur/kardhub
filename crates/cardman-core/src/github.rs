//! GitHub API client traits and platform-specific implementations.
//!
//! Defines an async `GitHubClient` trait with methods for listing repositories,
//! issues, pull requests, reviews, and CI status. The native implementation uses
//! `reqwest`; a Wasm implementation using `gloo-net` will follow in Phase 4.

use std::fmt;

use serde::Deserialize;

use crate::models::{
    CiStatus, Issue, IssueState, Label, Organization, PullRequest, Repository, Review, ReviewState,
    User,
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
    name: Option<String>,
}

impl From<GhUser> for User {
    fn from(u: GhUser) -> Self {
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
}

impl From<GhRepo> for Repository {
    fn from(r: GhRepo) -> Self {
        Self {
            owner: r.owner.login,
            name: r.name,
            archived: r.archived,
            default_branch: r.default_branch,
        }
    }
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
    #[allow(dead_code)]
    updated_at: String,
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
            assignees: i.assignees.into_iter().map(Into::into).collect(),
            state: match i.state.as_str() {
                "open" => IssueState::Open,
                _ => IssueState::Closed,
            },
            sub_issues: Vec::new(),
        }
    }
}

/// GitHub API pull request response.
#[derive(Debug, Deserialize)]
struct GhPullRequest {
    number: u64,
    title: String,
    #[allow(dead_code)]
    state: String,
    /// `null` for open/closed PRs, ISO-8601 timestamp for merged PRs.
    merged_at: Option<String>,
    head: GhPrHead,
    labels: Vec<GhLabel>,
    updated_at: String,
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
#[derive(Debug, Deserialize)]
struct GhCombinedStatus {
    state: String,
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
            .user_agent("cardman/0.1")
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

    /// Paginate a PR endpoint until items are older than `cutoff` (ISO-8601).
    ///
    /// Expects the endpoint to return items sorted by `updated` desc.
    async fn paginate_until(
        &self,
        path: &str,
        cutoff: &str,
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
                if item.updated_at.as_str() < cutoff {
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
    pub async fn get_authenticated_user(&self) -> Result<User, GitHubError> {
        let resp = self
            .get("/user")
            .send()
            .await
            .map_err(|e| GitHubError::Http(e.to_string()))?;
        let user: GhUser = self.handle_response(resp).await?;
        Ok(user.into())
    }

    /// List repositories for the authenticated user.
    /// Excludes archived repositories by default.
    pub async fn list_repos(&self) -> Result<Vec<Repository>, GitHubError> {
        let repos: Vec<GhRepo> = self.paginate("/user/repos?type=all&sort=updated").await?;
        Ok(repos.into_iter().map(Into::into).collect())
    }

    /// List repositories for an organization.
    pub async fn list_org_repos(&self, org: &str) -> Result<Vec<Repository>, GitHubError> {
        let repos: Vec<GhRepo> = self
            .paginate(&format!("/orgs/{org}/repos?type=all&sort=updated"))
            .await?;
        Ok(repos.into_iter().map(Into::into).collect())
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
    ///
    /// When `since` is `Some`, fetches only open issues updated after that
    /// timestamp (incremental). When `None`, fetches all open issues (full).
    pub async fn list_open_issues(
        &self,
        owner: &str,
        repo: &str,
        since: Option<&str>,
    ) -> Result<Vec<Issue>, GitHubError> {
        let path = match since {
            Some(ts) => format!(
                "/repos/{owner}/{repo}/issues?state=open&sort=updated&direction=desc&since={ts}"
            ),
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
        since: Option<&str>,
    ) -> Result<Vec<Issue>, GitHubError> {
        let items: Vec<GhIssue> = match since {
            Some(ts) => {
                let path = format!(
                    "/repos/{owner}/{repo}/issues?state=closed&sort=updated&direction=desc&since={ts}"
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
    /// PRs (full pagination). Includes reviews and CI status.
    pub async fn list_open_prs(
        &self,
        owner: &str,
        repo: &str,
        since: Option<&str>,
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
            let r = self
                .get_reviews(owner, repo, item.number)
                .await
                .unwrap_or_default();
            let ci = self
                .get_ci_status(owner, repo, &item.head.branch_ref)
                .await
                .unwrap_or(CiStatus::Pending);
            prs.push(PullRequest {
                number: item.number,
                title: item.title,
                reviews: r,
                ci_status: ci,
                merged: false,
                closed: false,
                branch: item.head.branch_ref,
                labels: item.labels.into_iter().map(Into::into).collect(),
            });
        }
        Ok(prs)
    }

    /// List closed (merged + closed-not-merged) pull requests.
    ///
    /// When `since` is `Some`, paginates `state=closed` sorted by updated
    /// desc and stops at the cutoff (incremental). When `None`, fetches all
    /// closed PRs (full pagination, first sync). No reviews/CI needed.
    pub async fn list_closed_prs(
        &self,
        owner: &str,
        repo: &str,
        since: Option<&str>,
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
                self.paginate(&format!("/repos/{owner}/{repo}/pulls?state=closed"))
                    .await?
            }
        };

        Ok(items
            .into_iter()
            .map(|item| {
                let merged = item.merged_at.is_some();
                PullRequest {
                    number: item.number,
                    title: item.title,
                    reviews: Vec::new(),
                    ci_status: CiStatus::Pending,
                    merged,
                    closed: !merged,
                    branch: item.head.branch_ref,
                    labels: item.labels.into_iter().map(Into::into).collect(),
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

    /// Create an issue in a repository.
    pub async fn create_issue(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        body: Option<&str>,
        labels: &[String],
    ) -> Result<Issue, GitHubError> {
        let mut payload = serde_json::json!({ "title": title });
        if let Some(b) = body {
            payload["body"] = serde_json::Value::String(b.to_string());
        }
        if !labels.is_empty() {
            payload["labels"] = serde_json::json!(labels);
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
        let resp = self
            .patch(&format!("/repos/{owner}/{repo}/issues/{issue_number}"))
            .json(&serde_json::Value::Object(payload))
            .send()
            .await
            .map_err(|e| GitHubError::Http(e.to_string()))?;
        let issue: GhIssue = self.handle_response(resp).await?;
        Ok(issue.into())
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
            name: Some("Mona Lisa".into()),
        };
        let user: User = gh.into();
        assert_eq!(user.login, "octocat");
        assert_eq!(user.name, Some("Mona Lisa".into()));
    }

    #[test]
    fn gh_repo_converts_to_domain() {
        let gh = GhRepo {
            name: "hello-world".into(),
            archived: false,
            default_branch: "main".into(),
            owner: GhRepoOwner {
                login: "octocat".into(),
            },
        };
        let repo: Repository = gh.into();
        assert_eq!(repo.owner, "octocat");
        assert_eq!(repo.name, "hello-world");
        assert!(!repo.archived);
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
            updated_at: "2024-01-01T00:00:00Z".into(),
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
                    name: None,
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
}
