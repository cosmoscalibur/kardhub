//! Domain types for KardHub.
//!
//! All types derive `Serialize` and `Deserialize` for JSON round-tripping
//! with the GitHub API and local caching.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Provide a default `DateTime<Utc>` (Unix epoch) for serde.
fn epoch_default() -> DateTime<Utc> {
    DateTime::UNIX_EPOCH
}

/// A GitHub user.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    /// GitHub login (username).
    pub login: String,
    /// URL to the user's avatar image.
    pub avatar_url: String,
}

/// The authenticated GitHub user (includes display name).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthenticatedUser {
    /// GitHub login (username).
    pub login: String,
    /// URL to the user's avatar image.
    pub avatar_url: String,
    /// Display name (may differ from login).
    pub name: Option<String>,
}

/// A GitHub repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    /// Repository owner login.
    pub owner: String,
    /// Owner account type: `"User"` or `"Organization"`.
    #[serde(default)]
    pub owner_type: String,
    /// Repository name.
    pub name: String,
    /// Whether the repository is archived.
    pub archived: bool,
    /// Name of the default branch (e.g. "main").
    pub default_branch: String,
}

/// A GitHub label attached to issues or pull requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    /// Label text.
    pub name: String,
    /// Hex color code without leading `#` (e.g. "d73a4a").
    pub color: String,
}

/// State of a GitHub issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueState {
    /// The issue is open.
    Open,
    /// The issue is closed.
    Closed,
}

/// A GitHub issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    /// Issue number within the repository.
    pub number: u64,
    /// Issue title.
    pub title: String,
    /// Issue body (markdown).
    pub body: Option<String>,
    /// Labels attached to this issue.
    pub labels: Vec<Label>,
    /// Logins of users assigned to this issue.
    pub assignees: Vec<String>,
    /// Current state.
    pub state: IssueState,
    /// Sub-issue numbers (for epics).
    pub sub_issues: Vec<u64>,
    /// Issue author login.
    #[serde(default)]
    pub author: String,
    /// Timestamp of last update.
    #[serde(default = "epoch_default")]
    pub updated_at: DateTime<Utc>,
}

/// Review verdict on a pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewState {
    /// The reviewer approved.
    Approved,
    /// The reviewer requested changes.
    ChangesRequested,
    /// The reviewer left a comment without verdict.
    Commented,
    /// Review is pending.
    Pending,
    /// Review was dismissed.
    Dismissed,
}

/// A single review on a pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    /// The reviewer.
    pub user: User,
    /// Review verdict.
    pub state: ReviewState,
}

/// A comment on an issue or pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    /// Comment ID.
    pub id: u64,
    /// Comment author.
    pub user: User,
    /// Comment body (GitHub-flavored markdown).
    pub body: String,
    /// Time the comment was created.
    pub created_at: DateTime<Utc>,
    /// Time the comment was last updated.
    pub updated_at: DateTime<Utc>,
}

/// Aggregated CI status for a pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CiStatus {
    /// All checks passed.
    Success,
    /// At least one check failed.
    Failure,
    /// Checks are still running.
    Pending,
}

/// A GitHub pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequest {
    /// PR number within the repository.
    pub number: u64,
    /// PR title.
    pub title: String,
    /// PR body (GitHub-flavored markdown).
    #[serde(default)]
    pub body: Option<String>,
    /// Whether this PR is a draft.
    #[serde(default)]
    pub draft: bool,
    /// PR author login.
    #[serde(default)]
    pub author: String,
    /// Logins of assigned users.
    #[serde(default)]
    pub assignees: Vec<String>,
    /// Logins of users requested to review this PR.
    #[serde(default)]
    pub requested_reviewers: Vec<String>,
    /// Reviews submitted on this PR.
    pub reviews: Vec<Review>,
    /// Aggregated CI status.
    pub ci_status: CiStatus,
    /// Whether this PR has been merged into the default branch.
    pub merged: bool,
    /// Whether this PR was closed without merging.
    #[serde(default)]
    pub closed: bool,
    /// Head branch name.
    pub branch: String,
    /// Labels attached to this PR.
    pub labels: Vec<Label>,
    /// Timestamp of last update.
    #[serde(default = "epoch_default")]
    pub updated_at: DateTime<Utc>,
}

/// A GitHub organization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Organization {
    /// Organization login.
    pub login: String,
    /// Whether this is the main organization (shows issues + PRs).
    /// Secondary organizations show PRs only.
    pub is_main: bool,
}

/// Priority level parsed from `#N` labels.
///
/// Lower numeric value = higher priority = shown first on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Priority(pub u8);

impl Priority {
    /// Attempt to parse a priority from a label name.
    ///
    /// Returns `Some(Priority(n))` if the label is exactly `#N` where N ∈ 1..=6.
    pub fn from_label(label_name: &str) -> Option<Self> {
        let stripped = label_name.strip_prefix('#')?;
        let n: u8 = stripped.parse().ok()?;
        if (1..=6).contains(&n) {
            Some(Self(n))
        } else {
            None
        }
    }
}

/// Summary of a PR linked to an issue card (for embedded display).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkedPr {
    /// Repository owner of the PR (may differ from the issue's owner).
    pub owner: String,
    /// Repository name of the PR (may differ from the issue's repo).
    pub repo: String,
    /// PR number.
    pub number: u64,
    /// PR title.
    pub title: String,
    /// The column the PR was mapped to.
    pub column: Column,
    /// Whether the PR is merged.
    pub merged: bool,
    /// Whether the PR is closed (without merge).
    pub closed: bool,
    /// Whether the PR is a draft.
    pub draft: bool,
}

/// Source of a Kanban card — either an issue or a pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CardSource {
    /// Card backed by a GitHub issue.
    Issue(Issue),
    /// Card backed by a GitHub pull request.
    #[serde(rename = "pull_request")]
    PullRequest(PullRequest),
}

/// A Kanban card with computed column and priority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    /// Repository owner (org or user login).
    pub owner: String,
    /// Repository name.
    pub repo: String,
    /// The underlying issue or pull request.
    pub source: CardSource,
    /// The column this card belongs to (computed by the mapping engine).
    pub column: Column,
    /// Priority (from `#N` labels). `None` if no priority label is present.
    pub priority: Option<Priority>,
    /// PRs linked to this card (only populated for Issue cards).
    #[serde(default)]
    pub linked_prs: Vec<LinkedPr>,
    /// Whether this card is hidden from board rendering (e.g. a PR absorbed by an issue).
    #[serde(default)]
    pub hidden: bool,
}

/// A column on the Kanban board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Column {
    /// Display name.
    pub name: String,
    /// Emoji prefix for the column name.
    pub emoji: String,
    /// Sort order (lower = further left on the board).
    pub sort_order: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_parses_valid_labels() {
        assert_eq!(Priority::from_label("#1"), Some(Priority(1)));
        assert_eq!(Priority::from_label("#6"), Some(Priority(6)));
    }

    #[test]
    fn priority_rejects_invalid_labels() {
        assert_eq!(Priority::from_label("#0"), None);
        assert_eq!(Priority::from_label("#7"), None);
        assert_eq!(Priority::from_label("bug"), None);
        assert_eq!(Priority::from_label("#"), None);
        assert_eq!(Priority::from_label(""), None);
    }

    #[test]
    fn priority_ordering() {
        // Lower number = higher priority = sorts first
        assert!(Priority(1) < Priority(2));
        assert!(Priority(1) < Priority(6));
    }

    #[test]
    fn user_round_trip_json() {
        let user = User {
            login: "octocat".to_string(),
            avatar_url: "https://example.com/avatar.png".to_string(),
        };
        let json = serde_json::to_string(&user).unwrap();
        let deserialized: User = serde_json::from_str(&json).unwrap();
        assert_eq!(user, deserialized);
    }

    #[test]
    fn issue_state_serialization() {
        let open = serde_json::to_string(&IssueState::Open).unwrap();
        assert_eq!(open, "\"open\"");
        let closed = serde_json::to_string(&IssueState::Closed).unwrap();
        assert_eq!(closed, "\"closed\"");
    }
}
