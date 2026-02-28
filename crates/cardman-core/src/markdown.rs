//! Markdown rendering utilities.
//!
//! Converts GitHub-Flavored Markdown to sanitised HTML using `pulldown-cmark`,
//! then post-processes the output to auto-link bare URLs, `@mentions`,
//! `#shortlinks`, cross-repo references, and 7-char commit SHAs.

use pulldown_cmark::{Options, Parser, html};
use regex::Regex;
use std::sync::LazyLock;

// ── Compiled regexes (initialised once) ───────────────────────────────

/// Bare URL (protocol required).
static RE_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"https?://[^\s<)\]'""]+"#).expect("url regex"));

/// `@mention` preceded by whitespace, start-of-string, or `>`.
/// Group `pre` = boundary char (or empty at start), `user` = login.
static RE_MENTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:^|(?P<pre>[>\s(]))@(?P<user>[a-zA-Z0-9](?:[a-zA-Z0-9_-]*[a-zA-Z0-9])?)"#)
        .expect("mention regex")
});

/// Cross-repo shortlink: `owner/repo#123`.
/// Preceded by whitespace or start-of-string.
static RE_CROSS_REF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?:^|(?P<pre>\s))(?P<owner>[a-zA-Z0-9_.-]+)/(?P<repo>[a-zA-Z0-9_.-]+)#(?P<num>\d+)"#,
    )
    .expect("cross-ref regex")
});

/// Same-repo `#123`, preceded by whitespace, start, or `>`.
static RE_ISSUE_REF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?:^|(?P<pre>[>\s(]))#(?P<num>\d+)"#).expect("issue-ref regex"));

/// 7-char hex SHA on word boundary.
static RE_SHA: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\b(?P<sha>[0-9a-f]{7})\b"#).expect("sha regex"));

// ── Public API ────────────────────────────────────────────────────────

/// Render GitHub-Flavored Markdown into HTML with auto-linked references.
///
/// Enables tables, task lists, and strikethrough extensions, then
/// post-processes the output for GitHub-style auto-linking.
pub fn markdown_to_html(md: &str, owner: &str, repo: &str) -> String {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH;
    let parser = Parser::new_ext(md, opts);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    post_process_github_links(&html_output, owner, repo)
}

/// Post-process HTML to auto-link GitHub references.
///
/// Skips content inside `<code>` and `<a>` tags to avoid breaking
/// pre-existing links and code blocks.
fn post_process_github_links(html: &str, owner: &str, repo: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut pos = 0;
    let bytes = html.as_bytes();

    while pos < bytes.len() {
        // Check for protected tag openings: <code or <a
        if bytes[pos] == b'<'
            && let Some(end) = find_protected_end(html, pos)
        {
            // Flush protected span verbatim.
            result.push_str(&html[pos..end]);
            pos = end;
            continue;
        }
        // Find next `<` to delimit a processable gap.
        let next_lt = html[pos..].find('<').map_or(html.len(), |i| pos + i);
        let gap = &html[pos..next_lt];
        result.push_str(&linkify_segment(gap, owner, repo));
        pos = next_lt;
        if pos < html.len() && find_protected_end(html, pos).is_none() {
            // Not a protected tag; copy the `<` and advance.
            // Find end of this non-protected tag.
            let tag_end = html[pos..].find('>').map_or(html.len(), |i| pos + i + 1);
            result.push_str(&html[pos..tag_end]);
            pos = tag_end;
        }
    }
    result
}

/// If `html[pos..]` starts with `<code` or `<a ` / `<a>`, return the
/// byte index just past the matching closing tag.
fn find_protected_end(html: &str, pos: usize) -> Option<usize> {
    let rest = &html[pos..];
    for (open, close) in [("<code", "</code>"), ("<a ", "</a>"), ("<a>", "</a>")] {
        if rest.starts_with(open) {
            return rest.find(close).map(|i| pos + i + close.len());
        }
    }
    None
}

/// Apply all link patterns to a segment known to be outside code/link tags.
fn linkify_segment(segment: &str, owner: &str, repo: &str) -> String {
    let mut s = segment.to_string();

    // 1) Bare URLs — wrap in <a> tags.
    s = RE_URL
        .replace_all(&s, |caps: &regex::Captures| {
            let url = &caps[0];
            format!(r#"<a href="{url}">{url}</a>"#)
        })
        .into_owned();

    // 2) @mentions.
    s = RE_MENTION
        .replace_all(&s, |caps: &regex::Captures| {
            let pre = caps.name("pre").map_or("", |m| m.as_str());
            let user = &caps["user"];
            format!(r#"{pre}<a href="https://github.com/{user}">@{user}</a>"#)
        })
        .into_owned();

    // 3) Cross-repo shortlinks (before same-repo to avoid partial matches).
    s = RE_CROSS_REF
        .replace_all(&s, |caps: &regex::Captures| {
            let pre = caps.name("pre").map_or("", |m| m.as_str());
            let o = &caps["owner"];
            let r = &caps["repo"];
            let n = &caps["num"];
            format!(r#"{pre}<a href="https://github.com/{o}/{r}/issues/{n}">{o}/{r}#{n}</a>"#)
        })
        .into_owned();

    // 4) Same-repo #N shortlinks.
    let owner_ref = owner.to_string();
    let repo_ref = repo.to_string();
    s = RE_ISSUE_REF
        .replace_all(&s, |caps: &regex::Captures| {
            let pre = caps.name("pre").map_or("", |m| m.as_str());
            let n = &caps["num"];
            format!(
                r#"{pre}<a href="https://github.com/{}/{}/issues/{n}">#{n}</a>"#,
                owner_ref, repo_ref
            )
        })
        .into_owned();

    // 5) 7-char commit SHAs.
    let owner_sha = owner.to_string();
    let repo_sha = repo.to_string();
    s = RE_SHA
        .replace_all(&s, |caps: &regex::Captures| {
            let sha = &caps["sha"];
            format!(
                r#"<a href="https://github.com/{}/{}/commit/{sha}">{sha}</a>"#,
                owner_sha, repo_sha
            )
        })
        .into_owned();

    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings() {
        let html = markdown_to_html("# Title\n\nParagraph", "o", "r");
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<p>Paragraph</p>"));
    }

    #[test]
    fn code_block() {
        let html = markdown_to_html("```rust\nfn main() {}\n```", "o", "r");
        assert!(html.contains("<code"));
        assert!(html.contains("fn main()"));
    }

    #[test]
    fn task_list() {
        let html = markdown_to_html("- [x] Done\n- [ ] Todo", "o", "r");
        assert!(html.contains("checked"));
        assert!(html.contains("Todo"));
    }

    #[test]
    fn table() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let html = markdown_to_html(md, "o", "r");
        assert!(html.contains("<table>"));
        assert!(html.contains("<td>1</td>"));
    }

    #[test]
    fn strikethrough() {
        let html = markdown_to_html("~~deleted~~", "o", "r");
        assert!(html.contains("<del>deleted</del>"));
    }

    // ── Auto-linking tests ────────────────────────────────────────

    #[test]
    fn bare_url() {
        let html = markdown_to_html("Visit https://example.com today", "o", "r");
        assert!(html.contains(r#"<a href="https://example.com">https://example.com</a>"#));
    }

    #[test]
    fn mention() {
        let html = markdown_to_html("Thanks @octocat!", "o", "r");
        assert!(html.contains(r#"<a href="https://github.com/octocat">@octocat</a>"#));
    }

    #[test]
    fn issue_shortlink() {
        let html = markdown_to_html("See #42 for details", "myorg", "myrepo");
        assert!(html.contains(r#"<a href="https://github.com/myorg/myrepo/issues/42">#42</a>"#));
    }

    #[test]
    fn cross_repo_shortlink() {
        let html = markdown_to_html("Related to other/repo#7", "o", "r");
        assert!(
            html.contains(r#"<a href="https://github.com/other/repo/issues/7">other/repo#7</a>"#)
        );
    }

    #[test]
    fn commit_sha() {
        let html = markdown_to_html("Fixed in abc1234", "myorg", "myrepo");
        assert!(
            html.contains(
                r#"<a href="https://github.com/myorg/myrepo/commit/abc1234">abc1234</a>"#
            )
        );
    }

    #[test]
    fn no_link_inside_code() {
        let html = markdown_to_html("`@user #42`", "o", "r");
        // Inside inline code — should NOT be linked.
        assert!(!html.contains("github.com/user"));
        assert!(!html.contains("github.com/o/r/issues/42"));
    }

    #[test]
    fn existing_markdown_link_preserved() {
        let html = markdown_to_html("[link](https://example.com)", "o", "r");
        // pulldown-cmark already creates the <a>, post-processing should not double-wrap.
        let count = html.matches("<a ").count();
        assert_eq!(count, 1, "should have exactly one <a> tag");
    }
}
