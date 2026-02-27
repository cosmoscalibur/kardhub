//! Domain types for Cardman.
//!
//! All types derive `Serialize` and `Deserialize` for JSON round-tripping
//! with the GitHub API and local caching.

use serde::{Deserialize, Serialize};

/// A GitHub user.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    /// GitHub login (username).
    pub login: String,
    /// URL to the user's avatar image.
    pub avatar_url: String,
    /// Display name (may be empty).
    pub name: Option<String>,
}

/// A GitHub repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    /// Repository owner login.
    pub owner: String,
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
    /// Users assigned to this issue.
    pub assignees: Vec<User>,
    /// Current state.
    pub state: IssueState,
    /// Sub-issue numbers (for epics).
    pub sub_issues: Vec<u64>,
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
    /// Whether this PR is a draft.
    #[serde(default)]
    pub draft: bool,
    /// PR author (includes avatar URL for display).
    #[serde(default)]
    pub author: User,
    /// Assigned users (includes avatar URLs for display).
    #[serde(default)]
    pub assignees: Vec<User>,
    /// Users requested to review this PR (drives Code Review column).
    #[serde(default)]
    pub requested_reviewers: Vec<User>,
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
    /// The underlying issue or pull request.
    pub source: CardSource,
    /// The column this card belongs to (computed by the mapping engine).
    pub column: Column,
    /// Priority (from `#N` labels). `None` if no priority label is present.
    pub priority: Option<Priority>,
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
            name: Some("Mona Lisa".to_string()),
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
