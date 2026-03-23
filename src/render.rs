use std::path::Path;
use std::sync::LazyLock;

use comrak::{Options, markdown_to_html};
use regex::Regex;

static WIKILINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\[\]|]+?)(?:\|([^\[\]]+?))?\]\]").unwrap());

/// Render markdown content to an HTML fragment.
///
/// Enables GFM extensions (tables, autolinks, strikethrough, task lists),
/// header IDs (empty prefix), and unsafe raw HTML passthrough.
/// Returns an HTML fragment -- no `<html>`, `<head>`, or `<body>` wrapper.
pub fn render_markdown(markdown: &str) -> String {
    let mut options = Options::default();

    // GFM extensions
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;

    // Header IDs with empty prefix (so `# Foo` gets `id="foo"`)
    options.extension.header_ids = Some(String::new());

    // Allow raw HTML passthrough (no sanitization)
    options.render.unsafe_ = true;

    let html = markdown_to_html(markdown, &options);
    replace_wikilinks(&html)
}

/// Escape characters that are special in HTML attribute values and content.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Replace Obsidian-style wiki-links with placeholder anchor elements.
///
/// - `[[slug]]` becomes `<a class="wikilink" data-slug="slug">[[slug]]</a>`
/// - `[[slug|display text]]` becomes `<a class="wikilink" data-slug="slug">display text</a>`
///
/// The slug value is HTML-escaped before interpolation into the `data-slug` attribute.
///
/// This runs on the final HTML string, so it works regardless of any surrounding
/// tags (e.g. `<p>`) that comrak may have inserted.
fn replace_wikilinks(html: &str) -> String {
    WIKILINK_RE
        .replace_all(html, |caps: &regex::Captures| {
            let slug = &caps[1];
            let escaped_slug = html_escape(slug);
            match caps.get(2) {
                Some(display) => {
                    format!(
                        "<a class=\"wikilink\" data-slug=\"{escaped_slug}\">{}</a>",
                        display.as_str()
                    )
                }
                None => {
                    format!("<a class=\"wikilink\" data-slug=\"{escaped_slug}\">[[{slug}]]</a>")
                }
            }
        })
        .into_owned()
}

/// Read a markdown file and render it to an HTML fragment.
pub fn render_file(path: &Path) -> Result<String, RenderError> {
    let markdown = std::fs::read_to_string(path)
        .map_err(|e| RenderError::ReadFailed(path.to_path_buf(), e))?;
    Ok(render_markdown(&markdown))
}

#[derive(Debug)]
pub enum RenderError {
    ReadFailed(std::path::PathBuf, std::io::Error),
    WriteFailed(std::path::PathBuf, std::io::Error),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadFailed(path, err) => {
                write!(f, "failed to read content file {}: {err}", path.display())
            }
            Self::WriteFailed(path, err) => {
                write!(f, "failed to write output file {}: {err}", path.display())
            }
        }
    }
}

impl std::error::Error for RenderError {}

/// Write an HTML fragment to `<out_dir>/content.html`, creating the directory if needed.
pub fn write_html(html: &str, out_dir: &Path) -> Result<std::path::PathBuf, RenderError> {
    std::fs::create_dir_all(out_dir)
        .map_err(|e| RenderError::WriteFailed(out_dir.to_path_buf(), e))?;
    let out_path = out_dir.join("content.html");
    std::fs::write(&out_path, html).map_err(|e| RenderError::WriteFailed(out_path.clone(), e))?;
    Ok(out_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_basic_paragraph() {
        let html = render_markdown("Hello **world**");
        assert_eq!(html.trim(), "<p>Hello <strong>world</strong></p>");
    }

    #[test]
    fn renders_header_with_id() {
        let html = render_markdown("# My Heading");
        assert!(
            html.contains("id=\"my-heading\""),
            "expected header ID, got: {html}"
        );
        assert!(html.contains("<h1"), "expected h1 tag, got: {html}");
    }

    #[test]
    fn renders_gfm_table() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let html = render_markdown(md);
        assert!(html.contains("<table>"), "expected table, got: {html}");
        assert!(html.contains("<td>1</td>"), "expected cell, got: {html}");
    }

    #[test]
    fn renders_strikethrough() {
        let html = render_markdown("~~deleted~~");
        assert!(
            html.contains("<del>deleted</del>"),
            "expected strikethrough, got: {html}"
        );
    }

    #[test]
    fn renders_autolink() {
        let html = render_markdown("Visit https://example.com for info");
        assert!(
            html.contains("<a href=\"https://example.com\""),
            "expected autolink, got: {html}"
        );
    }

    #[test]
    fn renders_tasklist() {
        let md = "- [x] done\n- [ ] todo";
        let html = render_markdown(md);
        assert!(
            html.contains("checked"),
            "expected checked attribute, got: {html}"
        );
    }

    #[test]
    fn passes_through_raw_html() {
        let md = "before\n\n<div class=\"custom\">raw</div>\n\nafter";
        let html = render_markdown(md);
        assert!(
            html.contains("<div class=\"custom\">raw</div>"),
            "expected raw HTML passthrough, got: {html}"
        );
    }

    #[test]
    fn produces_fragment_not_document() {
        let html = render_markdown("# Hello");
        assert!(!html.contains("<html"), "should not contain <html> tag");
        assert!(!html.contains("<head"), "should not contain <head> tag");
        assert!(!html.contains("<body"), "should not contain <body> tag");
    }

    #[test]
    fn render_file_missing() {
        let result = render_file(Path::new("/nonexistent/file.md"));
        assert!(matches!(result, Err(RenderError::ReadFailed(_, _))));
    }

    // --- wiki-link tests ---

    #[test]
    fn wikilink_simple_slug() {
        let html = replace_wikilinks("check [[my-page]] here");
        assert_eq!(
            html,
            "check <a class=\"wikilink\" data-slug=\"my-page\">[[my-page]]</a> here"
        );
    }

    #[test]
    fn wikilink_with_display_text() {
        let html = replace_wikilinks("see [[my-page|My Page]]");
        assert_eq!(
            html,
            "see <a class=\"wikilink\" data-slug=\"my-page\">My Page</a>"
        );
    }

    #[test]
    fn wikilink_multiple_in_one_string() {
        let html = replace_wikilinks("a [[one]] b [[two|Two]] c");
        assert!(
            html.contains("<a class=\"wikilink\" data-slug=\"one\">[[one]]</a>"),
            "expected first wikilink, got: {html}"
        );
        assert!(
            html.contains("<a class=\"wikilink\" data-slug=\"two\">Two</a>"),
            "expected second wikilink, got: {html}"
        );
    }

    #[test]
    fn wikilink_none_passthrough() {
        let input = "<p>No links here.</p>";
        let html = replace_wikilinks(input);
        assert_eq!(html, input);
    }

    #[test]
    fn wikilink_inside_paragraph() {
        let html = render_markdown("Check [[some-slug]] for details.");
        assert!(
            html.contains("<a class=\"wikilink\" data-slug=\"some-slug\">[[some-slug]]</a>"),
            "expected wikilink inside <p>, got: {html}"
        );
        assert!(
            html.contains("<p>"),
            "expected paragraph wrapper, got: {html}"
        );
    }
}
