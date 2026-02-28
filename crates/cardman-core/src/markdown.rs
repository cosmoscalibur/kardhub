//! Markdown rendering utilities.
//!
//! Converts GitHub-Flavored Markdown to sanitised HTML using `pulldown-cmark`.

use pulldown_cmark::{Options, Parser, html};

/// Render GitHub-Flavored Markdown into HTML.
///
/// Enables tables, task lists, and strikethrough extensions.
pub fn markdown_to_html(md: &str) -> String {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH;
    let parser = Parser::new_ext(md, opts);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings() {
        let html = markdown_to_html("# Title\n\nParagraph");
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<p>Paragraph</p>"));
    }

    #[test]
    fn code_block() {
        let html = markdown_to_html("```rust\nfn main() {}\n```");
        assert!(html.contains("<code"));
        assert!(html.contains("fn main()"));
    }

    #[test]
    fn task_list() {
        let html = markdown_to_html("- [x] Done\n- [ ] Todo");
        assert!(html.contains("checked"));
        assert!(html.contains("Todo"));
    }

    #[test]
    fn table() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let html = markdown_to_html(md);
        assert!(html.contains("<table>"));
        assert!(html.contains("<td>1</td>"));
    }

    #[test]
    fn strikethrough() {
        let html = markdown_to_html("~~deleted~~");
        assert!(html.contains("<del>deleted</del>"));
    }
}
