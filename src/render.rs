use std::path::Path;
use std::sync::LazyLock;

use comrak::{Options, markdown_to_html};
use regex::Regex;
use syntect::html::ClassedHTMLGenerator;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

static WIKILINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\[\]|]+?)(?:\|([^\[\]]+?))?\]\]").unwrap());

/// Matches `<pre><code class="language-XXX">...code...</code></pre>` blocks produced by comrak.
/// Captures: (1) the language name, (2) the code content (HTML-encoded).
static CODE_BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<pre><code class="language-([^"]+)">(.*?)</code></pre>"#).unwrap()
});

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

/// Regex to extract the content between `<body>` and `</body>` tags.
/// Used to strip the full HTML document wrapper produced by rst2html5.
static BODY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?si)<body>(.*)</body>").unwrap());

/// Content format detected from file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentFormat {
    Markdown,
    Rst,
    Html,
    Txt,
}

/// Detect content format from a file extension.
///
/// Returns `Ok(ContentFormat)` for recognized extensions (.md, .rst, .html, .txt),
/// or `Err(RenderError::UnsupportedFormat)` for anything else.
pub fn detect_format(path: &Path) -> Result<ContentFormat, RenderError> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("md") => Ok(ContentFormat::Markdown),
        Some("rst") => Ok(ContentFormat::Rst),
        Some("html") | Some("htm") => Ok(ContentFormat::Html),
        Some("txt") => Ok(ContentFormat::Txt),
        Some(ext) => Err(RenderError::UnsupportedFormat(ext.to_string())),
        None => Err(RenderError::UnsupportedFormat("(no extension)".to_string())),
    }
}

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
    let html = highlight_code_blocks(&html);
    replace_wikilinks(&html)
}

/// Render RST content to an HTML fragment by shelling out to `rst2html5`.
///
/// Passes the file path to `rst2html5`, captures stdout, and extracts the
/// `<body>` content (stripping the full HTML document wrapper).
/// Wiki-link replacement is applied to the output.
fn render_rst(path: &Path) -> Result<String, RenderError> {
    // Check that rst2html5 is on PATH before attempting to run it.
    let which = std::process::Command::new("which")
        .arg("rst2html5")
        .output();
    match which {
        Ok(output) if output.status.success() => {}
        _ => return Err(RenderError::Rst2HtmlNotFound),
    }

    let output = std::process::Command::new("rst2html5")
        .arg(path)
        .output()
        .map_err(|e| RenderError::RstFailed(format!("failed to execute rst2html5: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(RenderError::RstFailed(format!(
            "rst2html5 exited with {}: {stderr}",
            output.status
        )));
    }

    let full_html = String::from_utf8_lossy(&output.stdout);
    let body_content = extract_body(&full_html);
    Ok(replace_wikilinks(&body_content))
}

/// Extract content between `<body>` and `</body>` tags.
/// If no body tags are found, returns the input as-is (trimmed).
fn extract_body(html: &str) -> String {
    match BODY_RE.captures(html) {
        Some(caps) => caps[1].trim().to_string(),
        None => html.trim().to_string(),
    }
}

/// Render HTML content as passthrough -- already HTML, just apply wiki-link replacement.
fn render_html_passthrough(content: &str) -> String {
    replace_wikilinks(content)
}

/// Render plain text by wrapping in a `<pre class="plaintext">` tag.
/// Wiki-link replacement is applied to the output.
fn render_txt(content: &str) -> String {
    let escaped = html_escape(content);
    let wrapped = format!("<pre class=\"plaintext\">{escaped}</pre>");
    replace_wikilinks(&wrapped)
}

/// Read a content file and render it to an HTML fragment.
///
/// Detects the format from the file extension and dispatches to the
/// appropriate renderer:
/// - `.md` -> markdown rendering
/// - `.rst` -> RST rendering via rst2html5
/// - `.html` -> passthrough (content is already HTML)
/// - `.txt` -> wrapped in `<pre class="plaintext">`
/// - Unknown -> error
pub fn render_file(path: &Path) -> Result<String, RenderError> {
    let format = detect_format(path)?;

    let html = match format {
        ContentFormat::Markdown => {
            let content = std::fs::read_to_string(path)
                .map_err(|e| RenderError::ReadFailed(path.to_path_buf(), e))?;
            render_markdown(&content)
        }
        ContentFormat::Rst => render_rst(path)?,
        ContentFormat::Html => {
            let content = std::fs::read_to_string(path)
                .map_err(|e| RenderError::ReadFailed(path.to_path_buf(), e))?;
            render_html_passthrough(&content)
        }
        ContentFormat::Txt => {
            let content = std::fs::read_to_string(path)
                .map_err(|e| RenderError::ReadFailed(path.to_path_buf(), e))?;
            render_txt(&content)
        }
    };

    Ok(html)
}

/// Decode the basic HTML entities that comrak encodes inside `<code>` blocks.
fn html_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
}

/// Post-process comrak HTML to apply syntect-based syntax highlighting to fenced code blocks.
///
/// Finds every `<pre><code class="language-XXX">...</code></pre>` block, looks up language XXX
/// in syntect's default syntax set, and replaces the code content with `<span class="...">`
/// wrapped tokens using `ClassedHTMLGenerator`.
///
/// If the language is not recognized, the block is left unchanged.
/// Code blocks without a language annotation and inline `<code>` elements are not affected.
fn highlight_code_blocks(html: &str) -> String {
    CODE_BLOCK_RE
        .replace_all(html, |caps: &regex::Captures| {
            let lang = &caps[1];
            let code_html = &caps[2];

            // Look up the syntax; if not found, return the original block unchanged.
            let syntax = match SYNTAX_SET.find_syntax_by_token(lang) {
                Some(s) => s,
                None => return caps[0].to_string(),
            };

            // Decode HTML entities so syntect sees the real source code.
            let code = html_decode(code_html);

            let mut generator = ClassedHTMLGenerator::new_with_class_style(
                syntax,
                &SYNTAX_SET,
                syntect::html::ClassStyle::Spaced,
            );
            for line in LinesWithEndings::from(&code) {
                // parse_html_for_line_which_includes_newline cannot fail for valid syntaxes.
                let _ = generator.parse_html_for_line_which_includes_newline(line);
            }
            let highlighted = generator.finalize();

            format!("<pre><code class=\"language-{lang}\">{highlighted}</code></pre>")
        })
        .into_owned()
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

#[derive(Debug)]
pub enum RenderError {
    ReadFailed(std::path::PathBuf, std::io::Error),
    WriteFailed(std::path::PathBuf, std::io::Error),
    UnsupportedFormat(String),
    Rst2HtmlNotFound,
    RstFailed(String),
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
            Self::UnsupportedFormat(ext) => {
                write!(
                    f,
                    "unsupported content format: {ext} (supported: .md, .rst, .html, .txt)"
                )
            }
            Self::Rst2HtmlNotFound => {
                write!(
                    f,
                    "rst2html5 not found on PATH (install python3Packages.docutils)"
                )
            }
            Self::RstFailed(msg) => write!(f, "RST rendering failed: {msg}"),
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

    // --- format detection tests ---

    #[test]
    fn detect_format_markdown() {
        assert_eq!(
            detect_format(Path::new("post.md")).unwrap(),
            ContentFormat::Markdown
        );
    }

    #[test]
    fn detect_format_rst() {
        assert_eq!(
            detect_format(Path::new("post.rst")).unwrap(),
            ContentFormat::Rst
        );
    }

    #[test]
    fn detect_format_html() {
        assert_eq!(
            detect_format(Path::new("post.html")).unwrap(),
            ContentFormat::Html
        );
    }

    #[test]
    fn detect_format_htm() {
        assert_eq!(
            detect_format(Path::new("post.htm")).unwrap(),
            ContentFormat::Html
        );
    }

    #[test]
    fn detect_format_txt() {
        assert_eq!(
            detect_format(Path::new("post.txt")).unwrap(),
            ContentFormat::Txt
        );
    }

    #[test]
    fn detect_format_unknown() {
        let err = detect_format(Path::new("post.docx")).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unsupported content format: docx"),
            "expected unsupported format error, got: {msg}"
        );
    }

    #[test]
    fn detect_format_no_extension() {
        let err = detect_format(Path::new("README")).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("(no extension)"),
            "expected no-extension error, got: {msg}"
        );
    }

    // --- HTML passthrough tests ---

    #[test]
    fn html_passthrough_returns_content_as_is() {
        let content = "<h1>Title</h1>\n<p>Some paragraph.</p>";
        let result = render_html_passthrough(content);
        assert_eq!(result, content);
    }

    #[test]
    fn html_passthrough_replaces_wikilinks() {
        let content = "<p>See [[other-page]] for details.</p>";
        let result = render_html_passthrough(content);
        assert!(
            result.contains("<a class=\"wikilink\" data-slug=\"other-page\">[[other-page]]</a>"),
            "expected wikilink in HTML passthrough, got: {result}"
        );
    }

    #[test]
    fn html_passthrough_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("page.html");
        std::fs::write(&path, "<p>Hello [[wiki]]</p>").unwrap();

        let result = render_file(&path).unwrap();
        assert!(
            result.contains("<a class=\"wikilink\" data-slug=\"wiki\">[[wiki]]</a>"),
            "expected wikilink in HTML file, got: {result}"
        );
        assert!(
            result.contains("<p>Hello"),
            "expected HTML content, got: {result}"
        );
    }

    // --- txt wrapping tests ---

    #[test]
    fn txt_wraps_in_pre() {
        let content = "Hello world\nSecond line";
        let result = render_txt(content);
        assert!(
            result.starts_with("<pre class=\"plaintext\">"),
            "expected <pre> wrapper, got: {result}"
        );
        assert!(
            result.ends_with("</pre>"),
            "expected </pre> closing, got: {result}"
        );
    }

    #[test]
    fn txt_escapes_html_entities() {
        let content = "<script>alert('xss')</script>";
        let result = render_txt(content);
        assert!(
            !result.contains("<script>"),
            "should escape HTML tags in txt, got: {result}"
        );
        assert!(
            result.contains("&lt;script&gt;"),
            "expected escaped tags, got: {result}"
        );
    }

    #[test]
    fn txt_replaces_wikilinks() {
        // Wiki-links in txt: the [[ and ]] are not HTML-escaped because
        // html_escape only escapes &, <, >, " — so wiki-link replacement works.
        let content = "See [[my-page]] for info.";
        let result = render_txt(content);
        assert!(
            result.contains("<a class=\"wikilink\" data-slug=\"my-page\">"),
            "expected wikilink in txt, got: {result}"
        );
    }

    #[test]
    fn txt_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, "Plain text content").unwrap();

        let result = render_file(&path).unwrap();
        assert!(
            result.contains("<pre class=\"plaintext\">"),
            "expected pre wrapper from txt file, got: {result}"
        );
        assert!(
            result.contains("Plain text content"),
            "expected text content, got: {result}"
        );
    }

    // --- RST rendering tests ---

    /// Helper: returns true if rst2html5 is available on PATH.
    fn rst2html5_available() -> bool {
        std::process::Command::new("which")
            .arg("rst2html5")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn rst_body_extraction() {
        let full = r#"<!DOCTYPE html>
<html><head><title>T</title></head>
<body>
<div class="document">
<p>Hello world</p>
</div>
</body>
</html>"#;
        let body = extract_body(full);
        assert!(
            body.contains("<p>Hello world</p>"),
            "expected body content, got: {body}"
        );
        assert!(
            !body.contains("<html"),
            "should not contain html tag, got: {body}"
        );
        assert!(
            !body.contains("<head"),
            "should not contain head tag, got: {body}"
        );
    }

    #[test]
    fn rst_body_extraction_no_body_tag() {
        let fragment = "<p>Just a fragment</p>";
        let result = extract_body(fragment);
        assert_eq!(result, "<p>Just a fragment</p>");
    }

    #[test]
    fn rst_render_file() {
        if !rst2html5_available() {
            eprintln!("SKIP: rst2html5 not available");
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.rst");
        std::fs::write(&path, "Title\n=====\n\nA paragraph with [[wiki-link]].\n").unwrap();

        let result = render_file(&path).unwrap();
        assert!(
            result.contains("<a class=\"wikilink\" data-slug=\"wiki-link\">"),
            "expected wikilink in RST output, got: {result}"
        );
        // Should be a fragment, not a full document
        assert!(
            !result.contains("<html"),
            "should not contain <html> tag, got: {result}"
        );
    }

    #[test]
    fn rst_missing_binary() {
        // Test with a path that won't have rst2html5 -- we test the error path
        // by directly testing the error variant format.
        let err = RenderError::Rst2HtmlNotFound;
        let msg = err.to_string();
        assert!(
            msg.contains("rst2html5 not found on PATH"),
            "expected clear error message, got: {msg}"
        );
        assert!(
            msg.contains("python3Packages.docutils"),
            "expected install hint, got: {msg}"
        );
    }

    // --- unsupported format via render_file ---

    #[test]
    fn render_file_unsupported_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.docx");
        std::fs::write(&path, "content").unwrap();

        let err = render_file(&path).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unsupported content format: docx"),
            "expected unsupported format error, got: {msg}"
        );
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

    // --- syntax highlighting tests ---

    #[test]
    fn highlight_known_language() {
        let md = "```rust\nfn main() {}\n```";
        let html = render_markdown(md);
        // Should contain syntect span elements inside the code block.
        assert!(
            html.contains("<span class="),
            "expected highlighted spans for Rust code, got: {html}"
        );
        // Should preserve the language class on the wrapper.
        assert!(
            html.contains("class=\"language-rust\""),
            "expected language-rust class, got: {html}"
        );
    }

    #[test]
    fn highlight_unknown_language_passthrough() {
        let md = "```nosuchlanguage\nsome code\n```";
        let html = render_markdown(md);
        // Should still have the code block, just without span-based highlighting.
        assert!(
            html.contains("<pre><code class=\"language-nosuchlanguage\">some code"),
            "expected unchanged code block for unknown language, got: {html}"
        );
        assert!(
            !html.contains("<span class="),
            "should not contain highlighted spans for unknown language, got: {html}"
        );
    }

    #[test]
    fn highlight_no_language_passthrough() {
        let md = "```\nplain code\n```";
        let html = render_markdown(md);
        // comrak emits <pre><code> without a class for un-annotated blocks.
        assert!(
            html.contains("<pre><code>plain code"),
            "expected plain code block without language, got: {html}"
        );
        assert!(
            !html.contains("<span class="),
            "should not contain highlighted spans for no-language block, got: {html}"
        );
    }

    #[test]
    fn highlight_does_not_affect_inline_code() {
        let md = "Use `fn main()` in Rust.";
        let html = render_markdown(md);
        // Inline code should be <code>...</code> without <pre> wrapper.
        assert!(
            html.contains("<code>fn main()</code>"),
            "expected inline code untouched, got: {html}"
        );
        assert!(
            !html.contains("<span class="),
            "should not contain highlighted spans for inline code, got: {html}"
        );
    }
}
