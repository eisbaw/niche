use std::fs;
use std::path::Path;

use post2html::config::PostConfig;

/// Create a temporary directory with a config JSON and a markdown file, then return the paths.
/// Returns (tmp_dir, config_path, content_path, out_dir).
fn setup_fixture(
    config_json: &str,
    markdown: &str,
) -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let config_path = tmp.path().join("config.json");
    let content_path = tmp.path().join("content.md");
    let out_dir = tmp.path().join("out");
    fs::write(&config_path, config_json).expect("write config");
    fs::write(&content_path, markdown).expect("write content");
    (tmp, config_path, content_path, out_dir)
}

// ---------------------------------------------------------------------------
// 1. Full render pipeline produces both output files
// ---------------------------------------------------------------------------

#[test]
fn full_pipeline_produces_content_html_and_computed_json() {
    let config_json = r#"{
        "slug": "hello-world",
        "title": "Hello World",
        "date": "2024-03-15",
        "tags": ["test"],
        "summary": "A test post"
    }"#;
    let markdown = "# Hello\n\nThis is a test post with some words.";

    let (_tmp, config_path, content_path, out_dir) = setup_fixture(config_json, markdown);
    let (html_path, json_path) =
        post2html::run_render(&config_path, &content_path, &out_dir).expect("run_render failed");

    assert!(html_path.exists(), "content.html should exist");
    assert!(json_path.exists(), "computed.json should exist");
    assert_eq!(html_path.file_name().unwrap(), "content.html");
    assert_eq!(json_path.file_name().unwrap(), "computed.json");
}

#[test]
fn full_pipeline_html_contains_rendered_markdown() {
    let config_json = r#"{"slug": "s", "title": "t", "date": "2024-01-01"}"#;
    let markdown = "Hello **bold** world.";

    let (_tmp, config_path, content_path, out_dir) = setup_fixture(config_json, markdown);
    post2html::run_render(&config_path, &content_path, &out_dir).unwrap();

    let html = fs::read_to_string(out_dir.join("content.html")).unwrap();
    assert!(
        html.contains("<strong>bold</strong>"),
        "expected rendered markdown, got: {html}"
    );
}

// ---------------------------------------------------------------------------
// 2. computed.json has correct word_count and reading_time_minutes
// ---------------------------------------------------------------------------

#[test]
fn computed_json_word_count_and_reading_time() {
    let config_json = r#"{"slug": "wc-test", "title": "WC Test", "date": "2024-01-01"}"#;
    // 10 words in a single paragraph
    let markdown = "one two three four five six seven eight nine ten";

    let (_tmp, config_path, content_path, out_dir) = setup_fixture(config_json, markdown);
    post2html::run_render(&config_path, &content_path, &out_dir).unwrap();

    let json_str = fs::read_to_string(out_dir.join("computed.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(json["word_count"], 10, "word_count mismatch: {json_str}");
    assert_eq!(
        json["reading_time_minutes"], 1,
        "reading_time_minutes mismatch: {json_str}"
    );
}

#[test]
fn computed_json_reading_time_rounds_up() {
    let config_json = r#"{"slug": "long", "title": "Long Post", "date": "2024-01-01"}"#;
    // Generate 501 words -> should be ceil(501/250) = 3 minutes
    let words: Vec<&str> = std::iter::repeat_n("word", 501).collect();
    let markdown = words.join(" ");

    let (_tmp, config_path, content_path, out_dir) = setup_fixture(config_json, &markdown);
    post2html::run_render(&config_path, &content_path, &out_dir).unwrap();

    let json_str = fs::read_to_string(out_dir.join("computed.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(json["word_count"], 501);
    assert_eq!(json["reading_time_minutes"], 3);
}

#[test]
fn computed_json_preserves_extra_config_fields() {
    let config_json = r#"{
        "slug": "extras",
        "title": "Extras",
        "date": "2024-06-01",
        "tags": ["a", "b"],
        "draft": true
    }"#;
    let markdown = "Some content.";

    let (_tmp, config_path, content_path, out_dir) = setup_fixture(config_json, markdown);
    post2html::run_render(&config_path, &content_path, &out_dir).unwrap();

    let json_str = fs::read_to_string(out_dir.join("computed.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(json["tags"], serde_json::json!(["a", "b"]));
    assert_eq!(json["draft"], true);
}

// ---------------------------------------------------------------------------
// 3. Wiki-link placeholders appear in content.html
// ---------------------------------------------------------------------------

#[test]
fn wikilinks_appear_as_placeholders_in_html() {
    let config_json = r#"{"slug": "wl", "title": "WL", "date": "2024-01-01"}"#;
    let markdown = "See [[other-post]] for details and [[another|Another Page]] too.";

    let (_tmp, config_path, content_path, out_dir) = setup_fixture(config_json, markdown);
    post2html::run_render(&config_path, &content_path, &out_dir).unwrap();

    let html = fs::read_to_string(out_dir.join("content.html")).unwrap();

    assert!(
        html.contains(r#"<a class="wikilink" data-slug="other-post">[[other-post]]</a>"#),
        "expected simple wikilink placeholder, got: {html}"
    );
    assert!(
        html.contains(r#"<a class="wikilink" data-slug="another">Another Page</a>"#),
        "expected display-text wikilink placeholder, got: {html}"
    );
}

#[test]
fn wikilinks_inside_rendered_paragraphs() {
    let config_json = r#"{"slug": "wl2", "title": "WL2", "date": "2024-01-01"}"#;
    let markdown = "Before [[link-target]] after.";

    let (_tmp, config_path, content_path, out_dir) = setup_fixture(config_json, markdown);
    post2html::run_render(&config_path, &content_path, &out_dir).unwrap();

    let html = fs::read_to_string(out_dir.join("content.html")).unwrap();

    // The wikilink should be inside a <p> tag
    assert!(html.contains("<p>"), "expected paragraph tag");
    assert!(
        html.contains("data-slug=\"link-target\""),
        "expected wikilink data-slug attribute, got: {html}"
    );
}

// ---------------------------------------------------------------------------
// 4. Missing required config fields produce clear errors
// ---------------------------------------------------------------------------

#[test]
fn missing_slug_produces_error() {
    let json = r#"{"title": "T", "date": "D"}"#;
    let result: Result<PostConfig, _> = serde_json::from_str(json);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("slug"),
        "error should mention missing 'slug': {err}"
    );
}

#[test]
fn missing_title_produces_error() {
    let json = r#"{"slug": "s", "date": "D"}"#;
    let result: Result<PostConfig, _> = serde_json::from_str(json);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("title"),
        "error should mention missing 'title': {err}"
    );
}

#[test]
fn missing_date_produces_error() {
    let json = r#"{"slug": "s", "title": "T"}"#;
    let result: Result<PostConfig, _> = serde_json::from_str(json);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("date"),
        "error should mention missing 'date': {err}"
    );
}

#[test]
fn invalid_json_produces_parse_error() {
    let (_tmp, config_path, content_path, out_dir) = setup_fixture("not json{{{", "# Hi");
    let result = post2html::run_render(&config_path, &content_path, &out_dir);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("parse") || err.contains("expected"),
        "error should indicate parse failure: {err}"
    );
}

#[test]
fn missing_config_file_produces_read_error() {
    let tmp = tempfile::tempdir().unwrap();
    let content_path = tmp.path().join("content.md");
    fs::write(&content_path, "# Test").unwrap();
    let result = post2html::run_render(
        Path::new("/nonexistent/config.json"),
        &content_path,
        &tmp.path().join("out"),
    );
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("read") || err.contains("No such file"),
        "error should indicate read failure: {err}"
    );
}

#[test]
fn missing_content_file_produces_read_error() {
    let config_json = r#"{"slug": "s", "title": "t", "date": "2024-01-01"}"#;
    let tmp = tempfile::tempdir().unwrap();
    let config_path = tmp.path().join("config.json");
    fs::write(&config_path, config_json).unwrap();
    let result = post2html::run_render(
        &config_path,
        Path::new("/nonexistent/content.md"),
        &tmp.path().join("out"),
    );
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("read") || err.contains("No such file"),
        "error should indicate read failure: {err}"
    );
}

// ---------------------------------------------------------------------------
// 5. Special characters in title (quotes, backslashes) handled in JSON
// ---------------------------------------------------------------------------

#[test]
fn special_characters_in_title_roundtrip_through_json() {
    let config_json = r#"{
        "slug": "special",
        "title": "He said \"hello\" and then\\left",
        "date": "2024-01-01"
    }"#;
    let markdown = "Content here.";

    let (_tmp, config_path, content_path, out_dir) = setup_fixture(config_json, markdown);
    post2html::run_render(&config_path, &content_path, &out_dir).unwrap();

    let json_str = fs::read_to_string(out_dir.join("computed.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    let title = json["title"].as_str().unwrap();
    assert!(
        title.contains('"'),
        "title should contain literal quotes: {title}"
    );
    assert!(
        title.contains('\\'),
        "title should contain literal backslash: {title}"
    );
    assert_eq!(title, r#"He said "hello" and then\left"#);
}

#[test]
fn unicode_in_title_preserved() {
    let config_json = r#"{
        "slug": "unicode",
        "title": "Rust er fedt \u00e6\u00f8\u00e5",
        "date": "2024-01-01"
    }"#;
    let markdown = "Content.";

    let (_tmp, config_path, content_path, out_dir) = setup_fixture(config_json, markdown);
    post2html::run_render(&config_path, &content_path, &out_dir).unwrap();

    let json_str = fs::read_to_string(out_dir.join("computed.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    let title = json["title"].as_str().unwrap();
    assert!(
        title.contains('\u{00e6}'),
        "title should preserve unicode: {title}"
    );
}

#[test]
fn title_with_angle_brackets() {
    let config_json = r#"{
        "slug": "angles",
        "title": "Vec<String> is great",
        "date": "2024-01-01"
    }"#;
    let markdown = "Content.";

    let (_tmp, config_path, content_path, out_dir) = setup_fixture(config_json, markdown);
    post2html::run_render(&config_path, &content_path, &out_dir).unwrap();

    let json_str = fs::read_to_string(out_dir.join("computed.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(json["title"].as_str().unwrap(), "Vec<String> is great");
}

// ---------------------------------------------------------------------------
// Full pipeline with the existing test fixtures
// ---------------------------------------------------------------------------

#[test]
fn full_pipeline_with_project_fixtures() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let config_path = fixtures.join("post-config.json");
    let content_path = fixtures.join("test-content.md");

    let tmp = tempfile::tempdir().unwrap();
    let out_dir = tmp.path().join("out");

    let (html_path, json_path) =
        post2html::run_render(&config_path, &content_path, &out_dir).expect("run_render failed");

    // Verify outputs exist
    assert!(html_path.exists());
    assert!(json_path.exists());

    // Verify HTML has expected features
    let html = fs::read_to_string(&html_path).unwrap();
    assert!(html.contains("<h1"), "expected h1 heading");
    assert!(html.contains("<table>"), "expected GFM table");
    assert!(html.contains("<del>"), "expected strikethrough");
    assert!(
        html.contains("class=\"custom-block\""),
        "expected raw HTML passthrough"
    );

    // Verify computed.json structure
    let json_str = fs::read_to_string(&json_path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(json["slug"], "hello-world");
    assert_eq!(json["title"], "Hello World");
    assert_eq!(json["date"], "2024-03-15");
    assert!(json["word_count"].as_u64().unwrap() > 0);
    assert!(json["reading_time_minutes"].as_u64().unwrap() >= 1);
}
