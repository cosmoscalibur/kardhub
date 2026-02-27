//! Card-to-column mapping engine.
//!
//! Determines which Kanban column a card belongs to based on issue/PR state,
//! labels, reviews, and CI status. Rules follow the priority order defined in
//! the Cardman spec — later rules override earlier ones when conditions match.

use crate::models::{
    Card, CardSource, CiStatus, Column, Issue, Label, Priority, PullRequest, ReviewState,
};

/// Configuration for the mapping engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingConfig {
    /// Number of approved reviews required to move a PR to QA Backlog.
    /// Configurable globally, overridable per repository.
    pub required_approvals: u8,
    /// Logins of users designated as QA reviewers.
    pub qa_users: Vec<String>,
}

impl Default for MappingConfig {
    fn default() -> Self {
        Self {
            required_approvals: 2,
            qa_users: Vec::new(),
        }
    }
}

/// All possible columns on the Kanban board, ordered from right to left
/// by priority of the mapping rule (highest-priority rule wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColumnKind {
    Icebox,
    Prebacklog,
    Backlog,
    InProgress,
    CodeReview,
    QaBacklog,
    QaReview,
    ReadyForStg,
    ReadyForDeploy,
    InRelease,
}

impl ColumnKind {
    /// Convert to a displayable [`Column`] with emoji and sort order.
    fn to_column(self) -> Column {
        let (name, emoji, sort_order) = match self {
            Self::Icebox => ("Icebox", "🧊", 0),
            Self::Prebacklog => ("Prebacklog", "⏳", 1),
            Self::Backlog => ("Backlog", "📥", 2),
            Self::InProgress => ("In Progress", "🚧", 3),
            Self::CodeReview => ("Code review", "👀", 4),
            Self::QaBacklog => ("QA Backlog", "⏳", 5),
            Self::QaReview => ("QA Review", "🔍", 6),
            Self::ReadyForStg => ("Ready for STG", "☑️", 7),
            Self::ReadyForDeploy => ("Ready for deploy", "✅", 8),
            Self::InRelease => ("In Release", "📦", 9),
        };
        Column {
            name: name.to_string(),
            emoji: emoji.to_string(),
            sort_order,
        }
    }
}

/// Extract the highest (lowest-numbered) priority from a set of labels.
fn extract_priority(labels: &[Label]) -> Option<Priority> {
    labels
        .iter()
        .filter_map(|l| Priority::from_label(&l.name))
        .min()
}

/// Determine the column for an issue (no associated PR).
fn map_issue(issue: &Issue) -> ColumnKind {
    let priority = extract_priority(&issue.labels);
    match priority.map(|p| p.0) {
        Some(6) => ColumnKind::Icebox,
        Some(1..=3) => ColumnKind::Backlog,
        Some(4..=5) | None => ColumnKind::Prebacklog,
        // Priority values outside 1..=6 are prevented by Priority::from_label,
        // but the match must be exhaustive.
        Some(_) => ColumnKind::Prebacklog,
    }
}

/// Check whether a PR branch is contained in a release branch.
///
/// Heuristic: the branch name starts with "release/" or "release-".
fn is_in_release_branch(branch: &str) -> bool {
    branch.starts_with("release/") || branch.starts_with("release-")
}

/// Determine the column for a pull request.
fn map_pull_request(pr: &PullRequest, config: &MappingConfig) -> ColumnKind {
    // Highest-priority rules checked first (right-most columns).

    // In release branch → 📦 In Release
    if is_in_release_branch(&pr.branch) {
        return ColumnKind::InRelease;
    }

    // Merged to default branch → ✅ Ready for deploy
    if pr.merged {
        return ColumnKind::ReadyForDeploy;
    }

    // A QA user approved → ☑️ Ready for STG
    let qa_approved = pr.reviews.iter().any(|r| {
        r.state == ReviewState::Approved
            && config
                .qa_users
                .iter()
                .any(|qa| qa.eq_ignore_ascii_case(&r.user.login))
    });
    if qa_approved {
        return ColumnKind::ReadyForStg;
    }

    // Label "QA" present → 🔍 QA Review
    let has_qa_label = pr.labels.iter().any(|l| l.name.eq_ignore_ascii_case("QA"));
    if has_qa_label {
        return ColumnKind::QaReview;
    }

    let approval_count = pr
        .reviews
        .iter()
        .filter(|r| r.state == ReviewState::Approved)
        .count();
    let has_failed_ci = pr.ci_status == CiStatus::Failure;
    let has_pending_review = pr.reviews.iter().any(|r| r.state == ReviewState::Pending);
    let has_reviewers = !pr.reviews.is_empty();

    // N approved reviewers, no failed CI, no pending review → ⏳ QA Backlog
    if approval_count >= config.required_approvals as usize && !has_failed_ci && !has_pending_review
    {
        return ColumnKind::QaBacklog;
    }

    // Has reviewers, no failed CI, no pending review → 👀 Code review
    if has_reviewers && !has_failed_ci && !has_pending_review {
        return ColumnKind::CodeReview;
    }

    // Has PR (default for any open PR) → 🚧 In Progress
    ColumnKind::InProgress
}

/// Map a card source to its column and priority, producing a full [`Card`].
pub fn map_card(source: CardSource, config: &MappingConfig) -> Card {
    let (column_kind, labels) = match &source {
        CardSource::Issue(issue) => (map_issue(issue), &issue.labels),
        CardSource::PullRequest(pr) => (map_pull_request(pr, config), &pr.labels),
    };

    Card {
        priority: extract_priority(labels),
        column: column_kind.to_column(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{IssueState, Review, User};

    fn label(name: &str) -> Label {
        Label {
            name: name.to_string(),
            color: "000000".to_string(),
        }
    }

    fn user(login: &str) -> User {
        User {
            login: login.to_string(),
            avatar_url: String::new(),
            name: None,
        }
    }

    fn default_config() -> MappingConfig {
        MappingConfig::default()
    }

    fn base_issue() -> Issue {
        Issue {
            number: 1,
            title: "Test issue".to_string(),
            body: None,
            labels: vec![],
            assignees: vec![],
            state: IssueState::Open,
            sub_issues: vec![],
        }
    }

    fn base_pr() -> PullRequest {
        PullRequest {
            number: 10,
            title: "Test PR".to_string(),
            reviews: vec![],
            ci_status: CiStatus::Success,
            merged: false,
            branch: "feature/test".to_string(),
            labels: vec![],
        }
    }

    // ── Issue mapping ────────────────────────────────────────────────

    #[test]
    fn issue_no_labels_maps_to_prebacklog() {
        let issue = base_issue();
        let card = map_card(CardSource::Issue(issue), &default_config());
        assert_eq!(card.column.name, "Prebacklog");
        assert!(card.priority.is_none());
    }

    #[test]
    fn issue_priority_6_maps_to_icebox() {
        let mut issue = base_issue();
        issue.labels = vec![label("#6")];
        let card = map_card(CardSource::Issue(issue), &default_config());
        assert_eq!(card.column.name, "Icebox");
        assert_eq!(card.priority, Some(Priority(6)));
    }

    #[test]
    fn issue_priority_4_maps_to_prebacklog() {
        let mut issue = base_issue();
        issue.labels = vec![label("#4")];
        let card = map_card(CardSource::Issue(issue), &default_config());
        assert_eq!(card.column.name, "Prebacklog");
    }

    #[test]
    fn issue_priority_5_maps_to_prebacklog() {
        let mut issue = base_issue();
        issue.labels = vec![label("#5")];
        let card = map_card(CardSource::Issue(issue), &default_config());
        assert_eq!(card.column.name, "Prebacklog");
    }

    #[test]
    fn issue_priority_1_maps_to_backlog() {
        let mut issue = base_issue();
        issue.labels = vec![label("#1")];
        let card = map_card(CardSource::Issue(issue), &default_config());
        assert_eq!(card.column.name, "Backlog");
        assert_eq!(card.priority, Some(Priority(1)));
    }

    #[test]
    fn issue_priority_2_maps_to_backlog() {
        let mut issue = base_issue();
        issue.labels = vec![label("#2")];
        let card = map_card(CardSource::Issue(issue), &default_config());
        assert_eq!(card.column.name, "Backlog");
    }

    #[test]
    fn issue_priority_3_maps_to_backlog() {
        let mut issue = base_issue();
        issue.labels = vec![label("#3")];
        let card = map_card(CardSource::Issue(issue), &default_config());
        assert_eq!(card.column.name, "Backlog");
    }

    #[test]
    fn issue_multiple_priorities_uses_highest() {
        let mut issue = base_issue();
        issue.labels = vec![label("#3"), label("#1"), label("#5")];
        let card = map_card(CardSource::Issue(issue), &default_config());
        // Highest priority (#1) determines column
        assert_eq!(card.column.name, "Backlog");
        assert_eq!(card.priority, Some(Priority(1)));
    }

    // ── PR mapping ───────────────────────────────────────────────────

    #[test]
    fn pr_open_no_reviewers_maps_to_in_progress() {
        let pr = base_pr();
        let card = map_card(CardSource::PullRequest(pr), &default_config());
        assert_eq!(card.column.name, "In Progress");
    }

    #[test]
    fn pr_with_reviewers_no_failure_maps_to_code_review() {
        let mut pr = base_pr();
        pr.reviews = vec![Review {
            user: user("reviewer1"),
            state: ReviewState::Commented,
        }];
        let card = map_card(CardSource::PullRequest(pr), &default_config());
        assert_eq!(card.column.name, "Code review");
    }

    #[test]
    fn pr_with_failed_ci_stays_in_progress() {
        let mut pr = base_pr();
        pr.ci_status = CiStatus::Failure;
        pr.reviews = vec![Review {
            user: user("reviewer1"),
            state: ReviewState::Commented,
        }];
        let card = map_card(CardSource::PullRequest(pr), &default_config());
        assert_eq!(card.column.name, "In Progress");
    }

    #[test]
    fn pr_with_n_approvals_maps_to_qa_backlog() {
        let mut pr = base_pr();
        pr.reviews = vec![
            Review {
                user: user("dev1"),
                state: ReviewState::Approved,
            },
            Review {
                user: user("dev2"),
                state: ReviewState::Approved,
            },
        ];
        let card = map_card(CardSource::PullRequest(pr), &default_config());
        assert_eq!(card.column.name, "QA Backlog");
    }

    #[test]
    fn pr_with_custom_required_approvals() {
        let mut pr = base_pr();
        pr.reviews = vec![Review {
            user: user("dev1"),
            state: ReviewState::Approved,
        }];
        let config = MappingConfig {
            required_approvals: 1,
            ..default_config()
        };
        let card = map_card(CardSource::PullRequest(pr), &config);
        assert_eq!(card.column.name, "QA Backlog");
    }

    #[test]
    fn pr_with_qa_label_maps_to_qa_review() {
        let mut pr = base_pr();
        pr.labels = vec![label("QA")];
        let card = map_card(CardSource::PullRequest(pr), &default_config());
        assert_eq!(card.column.name, "QA Review");
    }

    #[test]
    fn pr_qa_approved_maps_to_ready_for_stg() {
        let mut pr = base_pr();
        let config = MappingConfig {
            qa_users: vec!["qa-bot".to_string()],
            ..default_config()
        };
        pr.reviews = vec![Review {
            user: user("qa-bot"),
            state: ReviewState::Approved,
        }];
        let card = map_card(CardSource::PullRequest(pr), &config);
        assert_eq!(card.column.name, "Ready for STG");
    }

    #[test]
    fn pr_merged_maps_to_ready_for_deploy() {
        let mut pr = base_pr();
        pr.merged = true;
        let card = map_card(CardSource::PullRequest(pr), &default_config());
        assert_eq!(card.column.name, "Ready for deploy");
    }

    #[test]
    fn pr_in_release_branch_maps_to_in_release() {
        let mut pr = base_pr();
        pr.branch = "release/v1.0".to_string();
        let card = map_card(CardSource::PullRequest(pr), &default_config());
        assert_eq!(card.column.name, "In Release");
    }

    #[test]
    fn pr_pending_review_blocks_code_review() {
        let mut pr = base_pr();
        pr.reviews = vec![
            Review {
                user: user("dev1"),
                state: ReviewState::Approved,
            },
            Review {
                user: user("dev2"),
                state: ReviewState::Pending,
            },
        ];
        let card = map_card(CardSource::PullRequest(pr), &default_config());
        // Pending review blocks both Code Review and QA Backlog → In Progress
        assert_eq!(card.column.name, "In Progress");
    }
}
