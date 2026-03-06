//! PR ↔ Issue linking engine.
//!
//! Parses full GitHub issue URLs and closing keywords from PR bodies,
//! then post-processes mapped cards to embed linked PR summaries in issue
//! cards and optionally override the issue's column with the PR's position.

use regex::Regex;
use std::sync::LazyLock;

use crate::models::{Card, CardSource, LinkedPr};

// ── Compiled regexes ─────────────────────────────────────────────────

/// Full GitHub issue URL: `https://github.com/owner/repo/issues/N`.
static RE_GITHUB_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?mi)https?://github\.com/(?P<owner>[a-zA-Z0-9_.-]+)/(?P<repo>[a-zA-Z0-9_.-]+)/issues/(?P<num>\d+)",
    )
    .expect("github-url regex")
});

/// GitHub closing keywords with cross-repo reference: `Closes owner/repo#N`.
static RE_CLOSE_CROSS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?mi)(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\s+(?P<owner>[a-zA-Z0-9_.-]+)/(?P<repo>[a-zA-Z0-9_.-]+)#(?P<num>\d+)",
    )
    .expect("close-cross regex")
});

/// GitHub closing keywords with same-repo reference: `Closes #N`.
static RE_CLOSE_SAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?mi)(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\s+#(?P<num>\d+)")
        .expect("close-same regex")
});

// ── Public types ─────────────────────────────────────────────────────

/// An issue reference parsed from a PR body (ephemeral, not serialized).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueRef {
    /// Repository owner.
    pub owner: String,
    /// Repository name.
    pub repo: String,
    /// Issue number.
    pub number: u64,
}

// ── Parsing ──────────────────────────────────────────────────────────

/// Parse issue references from a PR body.
///
/// Recognises:
/// - `https://github.com/owner/repo/issues/N` (full GitHub issue URL)
/// - `Closes owner/repo#N`, `Fixes owner/repo#N`, `Resolves owner/repo#N`
/// - `Closes #N`, `Fixes #N`, `Resolves #N` (same-repo, uses defaults)
///
/// `default_owner` and `default_repo` are used for same-repo `#N` refs.
pub fn parse_issue_refs(body: &str, default_owner: &str, default_repo: &str) -> Vec<IssueRef> {
    let mut refs = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Deduplicate by (owner, repo, number).
    let mut push = |owner: String, repo: String, number: u64| {
        let key = (owner.clone(), repo.clone(), number);
        if seen.insert(key) {
            refs.push(IssueRef {
                owner,
                repo,
                number,
            });
        }
    };

    for caps in RE_GITHUB_URL.captures_iter(body) {
        if let (Some(o), Some(r), Some(n)) =
            (caps.name("owner"), caps.name("repo"), caps.name("num"))
            && let Ok(num) = n.as_str().parse::<u64>()
        {
            push(o.as_str().to_string(), r.as_str().to_string(), num);
        }
    }

    for caps in RE_CLOSE_CROSS.captures_iter(body) {
        if let (Some(o), Some(r), Some(n)) =
            (caps.name("owner"), caps.name("repo"), caps.name("num"))
            && let Ok(num) = n.as_str().parse::<u64>()
        {
            push(o.as_str().to_string(), r.as_str().to_string(), num);
        }
    }

    for caps in RE_CLOSE_SAME.captures_iter(body) {
        if let Some(n) = caps.name("num")
            && let Ok(num) = n.as_str().parse::<u64>()
        {
            push(default_owner.to_string(), default_repo.to_string(), num);
        }
    }

    refs
}

// ── Card linking ─────────────────────────────────────────────────────

/// Post-process mapped cards to establish PR → Issue links.
///
/// For each PR card whose body contains issue references, the matching
/// issue card gains:
/// 1. A [`LinkedPr`] entry for display.
/// 2. A column override: the issue adopts the **lowest-progress**
///    (smallest `sort_order`) linked PR column. When multiple PRs link
///    to the same issue, the least-advanced PR determines the column.
pub fn link_cards(cards: &mut [Card]) {
    // Reset previous linking state so the function is idempotent.
    for card in cards.iter_mut() {
        card.linked_prs.clear();
        card.hidden = false;
    }

    // Intermediate struct to avoid borrow conflicts.
    struct PrLink {
        issue_idx: usize,
        linked_pr: LinkedPr,
        sort_order: u8,
    }

    let mut links: Vec<PrLink> = Vec::new();

    // Phase 1: scan PRs, parse refs, resolve against issue cards.
    for pr_idx in 0..cards.len() {
        let (body, pr_owner, pr_repo) = match &cards[pr_idx].source {
            CardSource::PullRequest(pr) => (
                pr.body.as_deref().unwrap_or(""),
                cards[pr_idx].owner.as_str(),
                cards[pr_idx].repo.as_str(),
            ),
            _ => continue,
        };

        let refs = parse_issue_refs(body, pr_owner, pr_repo);
        if refs.is_empty() {
            continue;
        }

        let (pr_number, pr_title, pr_merged, pr_closed, pr_draft, pr_assignees) =
            match &cards[pr_idx].source {
                CardSource::PullRequest(pr) => (
                    pr.number,
                    pr.title.clone(),
                    pr.merged,
                    pr.closed,
                    pr.draft,
                    pr.assignees.clone(),
                ),
                _ => unreachable!(),
            };
        let pr_column = cards[pr_idx].column.clone();
        let pr_sort = pr_column.sort_order;

        for issue_ref in &refs {
            if let Some(issue_idx) = cards.iter().position(|c| {
                c.owner == issue_ref.owner
                    && c.repo == issue_ref.repo
                    && matches!(&c.source, CardSource::Issue(i) if i.number == issue_ref.number)
            }) {
                links.push(PrLink {
                    issue_idx,
                    linked_pr: LinkedPr {
                        owner: cards[pr_idx].owner.clone(),
                        repo: cards[pr_idx].repo.clone(),
                        number: pr_number,
                        title: pr_title.clone(),
                        column: pr_column.clone(),
                        merged: pr_merged,
                        closed: pr_closed,
                        draft: pr_draft,
                        assignees: pr_assignees.clone(),
                    },
                    sort_order: pr_sort,
                });
            }
        }
    }

    // Phase 2: apply linked PR entries and determine column overrides.
    //
    // Track the best (lowest sort_order) column per issue card.
    let mut best_col: std::collections::HashMap<usize, (u8, crate::models::Column)> =
        std::collections::HashMap::new();

    for link in links {
        let card = &mut cards[link.issue_idx];
        card.linked_prs.push(link.linked_pr);

        let entry = best_col.entry(link.issue_idx);
        entry
            .and_modify(|(best_sort, best_column)| {
                if link.sort_order < *best_sort {
                    *best_sort = link.sort_order;
                    *best_column = card.linked_prs.last().unwrap().column.clone();
                }
            })
            .or_insert_with(|| {
                (
                    link.sort_order,
                    card.linked_prs.last().unwrap().column.clone(),
                )
            });
    }

    // Phase 3: apply column overrides unconditionally — the issue
    // always adopts the lowest-progress PR column.
    for (idx, (_sort, col)) in best_col {
        cards[idx].column = col;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CardSource, CiStatus, Column, Issue, IssueState, PullRequest};
    use chrono::Utc;

    fn base_issue(number: u64) -> Issue {
        Issue {
            number,
            title: format!("Issue #{number}"),
            body: None,
            labels: vec![],
            assignees: vec![],
            state: IssueState::Open,
            sub_issues: vec![],
            author: "author".to_string(),
            updated_at: Utc::now(),
        }
    }

    fn base_pr(number: u64, body: Option<&str>) -> PullRequest {
        PullRequest {
            number,
            title: format!("PR #{number}"),
            body: body.map(|s| s.to_string()),
            draft: false,
            author: "dev".to_string(),
            assignees: vec![],
            requested_reviewers: vec![],
            reviews: vec![],
            ci_status: CiStatus::Success,
            merged: false,
            closed: false,
            branch: "feature/test".to_string(),
            labels: vec![],
            updated_at: Utc::now(),
        }
    }

    fn col(name: &str, sort_order: u8) -> Column {
        Column {
            name: name.to_string(),
            emoji: String::new(),
            sort_order,
        }
    }

    fn issue_card(number: u64, column: Column) -> Card {
        Card {
            owner: "org".to_string(),
            repo: "repo".to_string(),
            source: CardSource::Issue(base_issue(number)),
            column,
            priority: None,
            linked_prs: vec![],
            hidden: false,
        }
    }

    fn pr_card(number: u64, body: Option<&str>, column: Column) -> Card {
        Card {
            owner: "org".to_string(),
            repo: "repo".to_string(),
            source: CardSource::PullRequest(base_pr(number, body)),
            column,
            priority: None,
            linked_prs: vec![],
            hidden: false,
        }
    }

    // ── parse_issue_refs ─────────────────────────────────────────────

    #[test]
    fn parse_single_github_url() {
        let refs = parse_issue_refs("https://github.com/org/repo/issues/42", "o", "r");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].owner, "org");
        assert_eq!(refs[0].repo, "repo");
        assert_eq!(refs[0].number, 42);
    }

    #[test]
    fn parse_multiple_github_urls() {
        let body = "https://github.com/org/repo/issues/1\nhttps://github.com/org/repo/issues/2\nhttps://github.com/other/lib/issues/3";
        let refs = parse_issue_refs(body, "o", "r");
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[2].owner, "other");
        assert_eq!(refs[2].number, 3);
    }

    #[test]
    fn parse_github_closing_keywords() {
        let body = "Closes org/repo#1\nFixes other/lib#2\nResolves #3";
        let refs = parse_issue_refs(body, "default_owner", "default_repo");
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0].owner, "org");
        assert_eq!(refs[1].owner, "other");
        assert_eq!(refs[2].owner, "default_owner");
        assert_eq!(refs[2].repo, "default_repo");
        assert_eq!(refs[2].number, 3);
    }

    #[test]
    fn parse_ignores_non_matching() {
        let refs = parse_issue_refs("Just a regular PR body with no refs", "o", "r");
        assert!(refs.is_empty());
    }

    #[test]
    fn parse_mixed_body() {
        let body =
            "## Summary\nSome changes.\n\nhttps://github.com/org/repo/issues/10\n\nMore text.";
        let refs = parse_issue_refs(body, "o", "r");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].number, 10);
    }

    #[test]
    fn parse_deduplicates_url_and_keyword() {
        let body = "https://github.com/org/repo/issues/1\nCloses org/repo#1";
        let refs = parse_issue_refs(body, "o", "r");
        assert_eq!(refs.len(), 1);
    }

    // ── link_cards ───────────────────────────────────────────────────

    #[test]
    fn link_cards_populates_linked_prs() {
        let mut cards = vec![
            issue_card(42, col("Backlog", 2)),
            pr_card(
                10,
                Some("https://github.com/org/repo/issues/42"),
                col("Code review", 5),
            ),
        ];
        link_cards(&mut cards);
        assert_eq!(cards[0].linked_prs.len(), 1);
        assert_eq!(cards[0].linked_prs[0].number, 10);
        assert_eq!(cards[0].linked_prs[0].column.name, "Code review");
    }

    #[test]
    fn link_cards_overrides_column() {
        let mut cards = vec![
            issue_card(42, col("Backlog", 2)),
            pr_card(
                10,
                Some("https://github.com/org/repo/issues/42"),
                col("Code review", 5),
            ),
        ];
        link_cards(&mut cards);
        // Single qualifying PR overrides the issue column.
        assert_eq!(cards[0].column.name, "Code review");
    }

    #[test]
    fn link_cards_pending_overrides_backlog() {
        // Pending (sort 3) overrides Backlog (sort 2).
        let mut cards = vec![
            issue_card(42, col("Backlog", 2)),
            pr_card(
                10,
                Some("https://github.com/org/repo/issues/42"),
                col("Pending", 3),
            ),
        ];
        link_cards(&mut cards);
        assert_eq!(cards[0].column.name, "Pending");
        assert_eq!(cards[0].linked_prs.len(), 1);
    }

    #[test]
    fn link_cards_pr_overrides_even_backwards() {
        // PR in Pending (sort 3) overrides issue in In Progress (sort 4).
        let mut cards = vec![
            issue_card(42, col("In Progress", 4)),
            pr_card(
                10,
                Some("https://github.com/org/repo/issues/42"),
                col("Pending", 3),
            ),
        ];
        link_cards(&mut cards);
        assert_eq!(cards[0].column.name, "Pending");
        assert_eq!(cards[0].linked_prs.len(), 1);
    }

    #[test]
    fn link_cards_multiple_prs_lowest_wins() {
        let mut cards = vec![
            issue_card(42, col("Backlog", 2)),
            pr_card(
                10,
                Some("https://github.com/org/repo/issues/42"),
                col("QA Review", 7),
            ),
            pr_card(
                11,
                Some("https://github.com/org/repo/issues/42"),
                col("Code review", 5),
            ),
        ];
        link_cards(&mut cards);
        // Two non-Failed PRs: sort 7 and sort 5. Lowest (5) wins.
        assert_eq!(cards[0].column.name, "Code review");
        assert_eq!(cards[0].linked_prs.len(), 2);
    }
}
