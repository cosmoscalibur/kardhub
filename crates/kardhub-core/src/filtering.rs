//! Client-side board filters for narrowing displayed Kanban cards.
//!
//! All matching is performed in-memory on already-fetched cards. The
//! filter uses `AND` semantics: every non-`None` field must match for
//! a card to be included.

use crate::models::{Card, CardSource};

/// Criteria for filtering cards on the Kanban board.
///
/// All fields use `Option` — a `None` field is ignored (matches everything).
/// When multiple fields are set, they combine with `AND` semantics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoardFilter {
    /// Case-insensitive substring match against any assignee login.
    pub assignee: Option<String>,
    /// Case-insensitive substring match against title **or** body.
    pub text: Option<String>,
    /// Substring match on issue/PR number (e.g. "4" matches #4, #42, #142).
    pub number: Option<String>,
}

impl BoardFilter {
    /// Returns `true` when no filter field is set.
    pub fn is_empty(&self) -> bool {
        self.assignee.is_none() && self.text.is_none() && self.number.is_none()
    }
}

/// Check whether a [`Card`] satisfies the given [`BoardFilter`].
///
/// Returns `true` when **all** non-`None` filter fields match the card.
/// An empty filter (all `None`) matches every card.
pub fn matches_filter(card: &Card, filter: &BoardFilter) -> bool {
    if filter.is_empty() {
        return true;
    }

    let (number, title, body, assignees) = match &card.source {
        CardSource::Issue(issue) => (
            issue.number,
            issue.title.as_str(),
            issue.body.as_deref(),
            issue.assignees.as_slice(),
        ),
        CardSource::PullRequest(pr) => (
            pr.number,
            pr.title.as_str(),
            pr.body.as_deref(),
            pr.assignees.as_slice(),
        ),
    };

    // Assignee filter: any login must contain the substring (case-insensitive).
    if let Some(ref pattern) = filter.assignee {
        let pat = pattern.to_lowercase();
        if !assignees
            .iter()
            .any(|login| login.to_lowercase().contains(&pat))
        {
            return false;
        }
    }

    // Text filter: title or body must contain the substring (case-insensitive).
    if let Some(ref pattern) = filter.text {
        let pat = pattern.to_lowercase();
        let title_match = title.to_lowercase().contains(&pat);
        let body_match = body
            .map(|b| b.to_lowercase().contains(&pat))
            .unwrap_or(false);
        if !title_match && !body_match {
            return false;
        }
    }

    // Number filter: substring match against the card number **or** any linked PR number.
    if let Some(ref pattern) = filter.number {
        let num_str = number.to_string();
        let card_match = num_str.contains(pattern);
        let linked_match = card
            .linked_prs
            .iter()
            .any(|lp| lp.number.to_string().contains(pattern));
        if !card_match && !linked_match {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Card, CardSource, Column, Issue, IssueState, PullRequest};

    /// Helper to build a minimal issue card for testing.
    fn issue_card(number: u64, title: &str, body: Option<&str>, assignees: &[&str]) -> Card {
        Card {
            owner: "test-owner".into(),
            repo: "test-repo".into(),
            source: CardSource::Issue(Issue {
                number,
                title: title.into(),
                body: body.map(Into::into),
                labels: Vec::new(),
                assignees: assignees.iter().map(|s| (*s).to_string()).collect(),
                state: IssueState::Open,
                sub_issues: Vec::new(),
                author: String::new(),
                updated_at: chrono::DateTime::UNIX_EPOCH,
            }),
            column: Column {
                name: "Backlog".into(),
                emoji: "📥".into(),
                sort_order: 2,
            },
            priority: None,
            linked_prs: Vec::new(),
            hidden: false,
        }
    }

    /// Helper to build a minimal PR card for testing.
    fn pr_card(number: u64, title: &str, body: Option<&str>, assignees: &[&str]) -> Card {
        Card {
            owner: "test-owner".into(),
            repo: "test-repo".into(),
            source: CardSource::PullRequest(PullRequest {
                number,
                title: title.into(),
                body: body.map(Into::into),
                draft: false,
                author: String::new(),
                assignees: assignees.iter().map(|s| (*s).to_string()).collect(),
                requested_reviewers: Vec::new(),
                reviews: Vec::new(),
                ci_status: crate::models::CiStatus::Pending,
                merged: false,
                closed: false,
                branch: String::new(),
                labels: Vec::new(),
                updated_at: chrono::DateTime::UNIX_EPOCH,
            }),
            column: Column {
                name: "Code review".into(),
                emoji: "👀".into(),
                sort_order: 5,
            },
            priority: None,
            linked_prs: Vec::new(),
            hidden: false,
        }
    }

    #[test]
    fn empty_filter_matches_all() {
        let card = issue_card(1, "Some issue", None, &[]);
        assert!(matches_filter(&card, &BoardFilter::default()));
    }

    #[test]
    fn filter_by_assignee() {
        let card = issue_card(1, "Bug", None, &["alice", "bob"]);
        let filter = BoardFilter {
            assignee: Some("alice".into()),
            ..Default::default()
        };
        assert!(matches_filter(&card, &filter));

        let miss = BoardFilter {
            assignee: Some("charlie".into()),
            ..Default::default()
        };
        assert!(!matches_filter(&card, &miss));
    }

    #[test]
    fn filter_by_assignee_case_insensitive() {
        let card = issue_card(1, "Bug", None, &["Alice"]);
        let filter = BoardFilter {
            assignee: Some("alice".into()),
            ..Default::default()
        };
        assert!(matches_filter(&card, &filter));
    }

    #[test]
    fn filter_by_text_title() {
        let card = issue_card(1, "Fix login button", None, &[]);
        let filter = BoardFilter {
            text: Some("login".into()),
            ..Default::default()
        };
        assert!(matches_filter(&card, &filter));
    }

    #[test]
    fn filter_by_text_body() {
        let card = issue_card(1, "Bug report", Some("The login button is broken"), &[]);
        let filter = BoardFilter {
            text: Some("broken".into()),
            ..Default::default()
        };
        assert!(matches_filter(&card, &filter));
    }

    #[test]
    fn filter_by_text_none_body() {
        let card = issue_card(1, "Bug report", None, &[]);
        let filter = BoardFilter {
            text: Some("broken".into()),
            ..Default::default()
        };
        assert!(!matches_filter(&card, &filter));
    }

    #[test]
    fn filter_by_number() {
        let card = issue_card(42, "Bug", None, &[]);
        let filter = BoardFilter {
            number: Some("42".into()),
            ..Default::default()
        };
        assert!(matches_filter(&card, &filter));

        let miss = BoardFilter {
            number: Some("99".into()),
            ..Default::default()
        };
        assert!(!matches_filter(&card, &miss));
    }

    #[test]
    fn combined_filters_and_semantics() {
        let card = issue_card(10, "Fix auth flow", Some("OAuth bug"), &["alice"]);
        let filter = BoardFilter {
            assignee: Some("alice".into()),
            text: Some("auth".into()),
            number: Some("10".into()),
        };
        assert!(matches_filter(&card, &filter));
    }

    #[test]
    fn combined_filters_partial_miss() {
        let card = issue_card(10, "Fix auth flow", None, &["alice"]);
        let filter = BoardFilter {
            assignee: Some("alice".into()),
            text: Some("auth".into()),
            number: Some("99".into()), // mismatch
        };
        assert!(!matches_filter(&card, &filter));
    }

    #[test]
    fn filter_works_on_pr_cards() {
        let card = pr_card(55, "Add caching", Some("Redis integration"), &["bob"]);
        let filter = BoardFilter {
            assignee: Some("bob".into()),
            text: Some("caching".into()),
            number: Some("55".into()),
        };
        assert!(matches_filter(&card, &filter));
    }

    #[test]
    fn filter_assignee_substring_match() {
        let card = issue_card(1, "Bug", None, &["alice-admin"]);
        let filter = BoardFilter {
            assignee: Some("alice".into()),
            ..Default::default()
        };
        assert!(matches_filter(&card, &filter));
    }

    #[test]
    fn filter_number_matches_linked_pr() {
        use crate::models::{Column, LinkedPr};
        let mut card = issue_card(42, "Feature request", None, &[]);
        card.linked_prs.push(LinkedPr {
            owner: "org".into(),
            repo: "repo".into(),
            number: 100,
            title: "Implement feature".into(),
            column: Column {
                name: "Code review".into(),
                emoji: "👀".into(),
                sort_order: 5,
            },
            merged: false,
            closed: false,
            draft: false,
            assignees: Vec::new(),
        });
        // Searching by PR number should match the issue card.
        let filter = BoardFilter {
            number: Some("100".into()),
            ..Default::default()
        };
        assert!(matches_filter(&card, &filter));
        // Searching by issue number still works.
        let filter2 = BoardFilter {
            number: Some("42".into()),
            ..Default::default()
        };
        assert!(matches_filter(&card, &filter2));
    }
}
