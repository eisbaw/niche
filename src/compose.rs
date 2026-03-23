use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;
use tera::{Context, Tera};

// ---------------------------------------------------------------------------
// Site config types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct SiteConfig {
    pub site_name: String,
    pub base_url: String,
    pub language: String,
    #[serde(default)]
    pub nav: Vec<NavItem>,
    #[serde(default = "default_posts_per_page")]
    pub posts_per_page: usize,
    #[serde(default)]
    pub feed: Option<FeedConfig>,
    #[serde(default)]
    pub author: Option<AuthorConfig>,
}

fn default_posts_per_page() -> usize {
    10
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct NavItem {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct FeedConfig {
    pub enable: bool,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct AuthorConfig {
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
}

// ---------------------------------------------------------------------------
// Post metadata loaded from computed.json + content.html
// ---------------------------------------------------------------------------

/// A single post ready for template rendering.
#[derive(Debug, Clone)]
struct PostEntry {
    slug: String,
    /// All fields from computed.json as a serde_json::Value (Object).
    metadata: Value,
    /// HTML fragment from content.html.
    content: String,
}

// ---------------------------------------------------------------------------
// Pagination
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
struct Pagination {
    current: usize,
    total_pages: usize,
    prev_url: Option<String>,
    next_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum ComposeError {
    ReadFailed(PathBuf, std::io::Error),
    WriteFailed(PathBuf, std::io::Error),
    InvalidJson(PathBuf, serde_json::Error),
    TemplateFailed(tera::Error),
}

impl std::fmt::Display for ComposeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadFailed(path, err) => {
                write!(f, "failed to read {}: {err}", path.display())
            }
            Self::WriteFailed(path, err) => {
                write!(f, "failed to write {}: {err}", path.display())
            }
            Self::InvalidJson(path, err) => {
                write!(f, "invalid JSON in {}: {err}", path.display())
            }
            Self::TemplateFailed(err) => {
                write!(f, "template error: {err}")
            }
        }
    }
}

impl std::error::Error for ComposeError {}

impl From<tera::Error> for ComposeError {
    fn from(e: tera::Error) -> Self {
        Self::TemplateFailed(e)
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the compose pipeline: load site config, posts, templates, and produce
/// the final site output.
pub fn run_compose(
    config_path: &Path,
    posts_dir: &Path,
    template_dir: &Path,
    static_dir: &Path,
    out_dir: &Path,
) -> Result<Vec<PathBuf>, ComposeError> {
    let site_config = load_site_config(config_path)?;
    let mut posts = load_posts(posts_dir)?;
    let tera = load_templates(template_dir)?;

    // Sort posts by date descending (newest first).
    posts.sort_by(|a, b| {
        let date_a = a
            .metadata
            .get("date")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let date_b = b
            .metadata
            .get("date")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        date_b.cmp(date_a)
    });

    let mut output_paths = Vec::new();

    // Build a serializable site context value that Tera can consume.
    let site_value = build_site_value(&site_config);

    // --- Render individual post pages ---
    for post in &posts {
        let slug = &post.slug;
        let post_out_dir = out_dir.join("posts").join(slug);
        std::fs::create_dir_all(&post_out_dir)
            .map_err(|e| ComposeError::WriteFailed(post_out_dir.clone(), e))?;

        let current_url = format!("/posts/{slug}/");

        let mut context = Context::new();
        context.insert("site", &site_value);
        context.insert("current_url", &current_url);

        // Build post context: metadata fields + content.
        let mut post_value = post.metadata.clone();
        if let Some(obj) = post_value.as_object_mut() {
            obj.insert("content".into(), Value::String(post.content.clone()));
        }
        context.insert("post", &post_value);

        let rendered = tera.render("post.html", &context)?;
        let index_path = post_out_dir.join("index.html");
        std::fs::write(&index_path, &rendered)
            .map_err(|e| ComposeError::WriteFailed(index_path.clone(), e))?;

        output_paths.push(index_path);
    }

    // --- Render index page (first page of posts) ---
    {
        let posts_per_page = site_config.posts_per_page;
        let total_posts = posts.len();
        let total_pages = if total_posts == 0 {
            1
        } else {
            total_posts.div_ceil(posts_per_page)
        };

        let page_posts: Vec<&Value> = posts
            .iter()
            .take(posts_per_page)
            .map(|p| &p.metadata)
            .collect();

        let pagination = Pagination {
            current: 1,
            total_pages,
            prev_url: None,
            next_url: if total_pages > 1 {
                Some("/page/2/".to_string())
            } else {
                None
            },
        };

        let mut context = Context::new();
        context.insert("site", &site_value);
        context.insert("current_url", &"/");
        context.insert("posts", &page_posts);
        context.insert("pagination", &pagination);

        let rendered = tera.render("index.html", &context)?;
        let index_path = out_dir.join("index.html");
        std::fs::create_dir_all(out_dir)
            .map_err(|e| ComposeError::WriteFailed(out_dir.to_path_buf(), e))?;
        std::fs::write(&index_path, &rendered)
            .map_err(|e| ComposeError::WriteFailed(index_path.clone(), e))?;

        output_paths.push(index_path);
    }

    // --- Copy static assets ---
    if static_dir.is_dir() {
        let static_out = out_dir.join("static");
        copy_dir_recursive(static_dir, &static_out)?;
    }

    Ok(output_paths)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn load_site_config(path: &Path) -> Result<SiteConfig, ComposeError> {
    let data = std::fs::read_to_string(path)
        .map_err(|e| ComposeError::ReadFailed(path.to_path_buf(), e))?;
    let config: SiteConfig = serde_json::from_str(&data)
        .map_err(|e| ComposeError::InvalidJson(path.to_path_buf(), e))?;
    Ok(config)
}

fn load_posts(posts_dir: &Path) -> Result<Vec<PostEntry>, ComposeError> {
    let entries = std::fs::read_dir(posts_dir)
        .map_err(|e| ComposeError::ReadFailed(posts_dir.to_path_buf(), e))?;

    let mut posts = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| ComposeError::ReadFailed(posts_dir.to_path_buf(), e))?;
        let entry_path = entry.path();

        if !entry_path.is_dir() {
            continue;
        }

        let content_path = entry_path.join("content.html");
        let computed_path = entry_path.join("computed.json");

        // Both files must exist for a valid post.
        if !content_path.exists() || !computed_path.exists() {
            continue;
        }

        let content = std::fs::read_to_string(&content_path)
            .map_err(|e| ComposeError::ReadFailed(content_path.clone(), e))?;

        let computed_data = std::fs::read_to_string(&computed_path)
            .map_err(|e| ComposeError::ReadFailed(computed_path.clone(), e))?;

        let metadata: Value = serde_json::from_str(&computed_data)
            .map_err(|e| ComposeError::InvalidJson(computed_path.clone(), e))?;

        let slug = metadata
            .get("slug")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| {
                entry_path
                    .file_name()
                    .expect("directory must have name")
                    .to_str()
                    .expect("directory name must be valid utf-8")
            })
            .to_string();

        posts.push(PostEntry {
            slug,
            metadata,
            content,
        });
    }

    Ok(posts)
}

fn load_templates(template_dir: &Path) -> Result<Tera, ComposeError> {
    let glob = template_dir
        .join("**")
        .join("*")
        .to_string_lossy()
        .to_string();
    let tera = Tera::new(&glob)?;
    Ok(tera)
}

/// Build a serde_json::Value representing the site config for use in templates.
fn build_site_value(config: &SiteConfig) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("site_name".into(), Value::String(config.site_name.clone()));
    map.insert("base_url".into(), Value::String(config.base_url.clone()));
    map.insert("language".into(), Value::String(config.language.clone()));
    map.insert(
        "posts_per_page".into(),
        Value::Number(config.posts_per_page.into()),
    );

    // nav
    let nav: Vec<Value> = config
        .nav
        .iter()
        .map(|item| {
            serde_json::json!({
                "label": item.label,
                "url": item.url,
            })
        })
        .collect();
    map.insert("nav".into(), Value::Array(nav));

    // author
    if let Some(author) = &config.author {
        let mut author_map = serde_json::Map::new();
        author_map.insert("name".into(), Value::String(author.name.clone()));
        if let Some(email) = &author.email {
            author_map.insert("email".into(), Value::String(email.clone()));
        }
        map.insert("author".into(), Value::Object(author_map));
    }

    // feed
    if let Some(feed) = &config.feed {
        let feed_val = serde_json::to_value(feed).unwrap_or(Value::Null);
        map.insert("feed".into(), feed_val);
    }

    Value::Object(map)
}

/// Recursively copy a directory and its contents.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), ComposeError> {
    std::fs::create_dir_all(dst).map_err(|e| ComposeError::WriteFailed(dst.to_path_buf(), e))?;

    for entry in
        std::fs::read_dir(src).map_err(|e| ComposeError::ReadFailed(src.to_path_buf(), e))?
    {
        let entry = entry.map_err(|e| ComposeError::ReadFailed(src.to_path_buf(), e))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)
                .map_err(|e| ComposeError::WriteFailed(dst_path, e))?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Create a minimal test fixture with site config, posts, templates, and static.
    fn setup_fixture() -> (TempDir, PathBuf, PathBuf, PathBuf, PathBuf, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().to_path_buf();

        // Site config
        let config_path = base.join("site-config.json");
        std::fs::write(
            &config_path,
            r#"{
  "site_name": "Test Blog",
  "base_url": "https://example.com",
  "language": "en",
  "posts_per_page": 2,
  "nav": [
    {"label": "Home", "url": "/"},
    {"label": "About", "url": "/about/"}
  ],
  "author": {
    "name": "Test Author",
    "email": "test@example.com"
  }
}"#,
        )
        .unwrap();

        // Posts directory with two posts
        let posts_dir = base.join("posts");

        let post1_dir = posts_dir.join("first-post");
        std::fs::create_dir_all(&post1_dir).unwrap();
        std::fs::write(post1_dir.join("content.html"), "<p>First post content.</p>").unwrap();
        std::fs::write(
            post1_dir.join("computed.json"),
            r#"{
  "slug": "first-post",
  "title": "First Post",
  "date": "2024-03-15",
  "tags": ["rust", "nix"],
  "summary": "The very first post.",
  "word_count": 3,
  "reading_time_minutes": 1
}"#,
        )
        .unwrap();

        let post2_dir = posts_dir.join("second-post");
        std::fs::create_dir_all(&post2_dir).unwrap();
        std::fs::write(
            post2_dir.join("content.html"),
            "<p>Second post content.</p>",
        )
        .unwrap();
        std::fs::write(
            post2_dir.join("computed.json"),
            r#"{
  "slug": "second-post",
  "title": "Second Post",
  "date": "2024-04-01",
  "tags": ["rust"],
  "summary": "Another post.",
  "word_count": 3,
  "reading_time_minutes": 1
}"#,
        )
        .unwrap();

        // Templates - use the actual theme templates
        let template_dir = base.join("templates");
        std::fs::create_dir_all(&template_dir).unwrap();

        std::fs::write(
            template_dir.join("base.html"),
            r#"<!DOCTYPE html>
<html lang="{{ site.language }}">
<head>
  <meta charset="utf-8">
  <title>{% block title %}{{ site.site_name }}{% endblock %}</title>
</head>
<body>
  <nav>
    {% for item in site.nav %}
    <a href="{{ item.url }}"{% if current_url == item.url %} aria-current="page"{% endif %}>{{ item.label }}</a>
    {% endfor %}
  </nav>
  <main>{% block content %}{% endblock %}</main>
  <footer>&copy; {{ site.author.name }}</footer>
</body>
</html>"#,
        )
        .unwrap();

        std::fs::write(
            template_dir.join("post.html"),
            r#"{% extends "base.html" %}
{% block title %}{{ post.title }} — {{ site.site_name }}{% endblock %}
{% block content %}
<article>
  <h1>{{ post.title }}</h1>
  <time>{{ post.date }}</time>
  <div>{{ post.content | safe }}</div>
</article>
{% endblock %}"#,
        )
        .unwrap();

        std::fs::write(
            template_dir.join("index.html"),
            r#"{% extends "base.html" %}
{% block content %}
{% for p in posts %}
<article>
  <h2><a href="/posts/{{ p.slug }}/">{{ p.title }}</a></h2>
  <time>{{ p.date }}</time>
  {% if p.summary %}<p>{{ p.summary }}</p>{% endif %}
</article>
{% endfor %}
{% if pagination.total_pages > 1 %}
<nav>Page {{ pagination.current }} of {{ pagination.total_pages }}</nav>
{% endif %}
{% endblock %}"#,
        )
        .unwrap();

        // Static assets
        let static_dir = base.join("static");
        std::fs::create_dir_all(static_dir.join("css")).unwrap();
        std::fs::write(static_dir.join("css/main.css"), "body { margin: 0; }").unwrap();

        // Output directory
        let out_dir = base.join("out");

        (
            tmp,
            config_path,
            posts_dir,
            template_dir,
            static_dir,
            out_dir,
        )
    }

    #[test]
    fn compose_produces_post_pages() {
        let (_tmp, config, posts_dir, templates, static_dir, out) = setup_fixture();
        let result = run_compose(&config, &posts_dir, &templates, &static_dir, &out).unwrap();

        // Should produce post pages + index
        assert!(
            result.len() >= 3,
            "expected at least 3 output files, got {}",
            result.len()
        );

        // Check first-post page exists and contains content
        let post1 = std::fs::read_to_string(out.join("posts/first-post/index.html")).unwrap();
        assert!(
            post1.contains("First Post"),
            "post page should contain title"
        );
        assert!(
            post1.contains("First post content."),
            "post page should contain content fragment"
        );
        assert!(
            post1.contains("<!DOCTYPE html>"),
            "should be full HTML page"
        );

        // Check second-post page
        let post2 = std::fs::read_to_string(out.join("posts/second-post/index.html")).unwrap();
        assert!(
            post2.contains("Second Post"),
            "second post should contain title"
        );
    }

    #[test]
    fn compose_produces_index_page() {
        let (_tmp, config, posts_dir, templates, static_dir, out) = setup_fixture();
        run_compose(&config, &posts_dir, &templates, &static_dir, &out).unwrap();

        let index = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(
            index.contains("<!DOCTYPE html>"),
            "index should be full HTML"
        );
        // Both posts should appear (posts_per_page = 2, total = 2)
        assert!(index.contains("First Post"), "index should list first post");
        assert!(
            index.contains("Second Post"),
            "index should list second post"
        );
    }

    #[test]
    fn compose_sorts_posts_newest_first() {
        let (_tmp, config, posts_dir, templates, static_dir, out) = setup_fixture();
        run_compose(&config, &posts_dir, &templates, &static_dir, &out).unwrap();

        let index = std::fs::read_to_string(out.join("index.html")).unwrap();
        let pos_second = index
            .find("Second Post")
            .expect("should contain Second Post");
        let pos_first = index.find("First Post").expect("should contain First Post");
        assert!(
            pos_second < pos_first,
            "Second Post (2024-04-01) should appear before First Post (2024-03-15)"
        );
    }

    #[test]
    fn compose_copies_static_assets() {
        let (_tmp, config, posts_dir, templates, static_dir, out) = setup_fixture();
        run_compose(&config, &posts_dir, &templates, &static_dir, &out).unwrap();

        let css = std::fs::read_to_string(out.join("static/css/main.css")).unwrap();
        assert_eq!(css, "body { margin: 0; }");
    }

    #[test]
    fn compose_nav_with_aria_current() {
        let (_tmp, config, posts_dir, templates, static_dir, out) = setup_fixture();
        run_compose(&config, &posts_dir, &templates, &static_dir, &out).unwrap();

        let index = std::fs::read_to_string(out.join("index.html")).unwrap();
        // The Home nav item should have aria-current since current_url is "/"
        assert!(
            index.contains("aria-current=\"page\""),
            "Home link should have aria-current on index page"
        );
    }

    #[test]
    fn compose_post_has_site_chrome() {
        let (_tmp, config, posts_dir, templates, static_dir, out) = setup_fixture();
        run_compose(&config, &posts_dir, &templates, &static_dir, &out).unwrap();

        let post = std::fs::read_to_string(out.join("posts/first-post/index.html")).unwrap();
        assert!(post.contains("<nav>"), "post page should have nav");
        assert!(post.contains("<footer>"), "post page should have footer");
        assert!(
            post.contains("Test Author"),
            "footer should show author name"
        );
    }

    #[test]
    fn load_site_config_parses_valid() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"site_name": "X", "base_url": "https://x.com", "language": "en"}"#,
        )
        .unwrap();
        let config = load_site_config(&path).unwrap();
        assert_eq!(config.site_name, "X");
        assert_eq!(config.posts_per_page, 10); // default
    }

    #[test]
    fn load_site_config_missing_file() {
        let result = load_site_config(Path::new("/nonexistent/config.json"));
        assert!(matches!(result, Err(ComposeError::ReadFailed(_, _))));
    }

    #[test]
    fn compose_skips_dirs_without_required_files() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        // Config
        let config_path = base.join("config.json");
        std::fs::write(
            &config_path,
            r#"{"site_name": "X", "base_url": "https://x.com", "language": "en"}"#,
        )
        .unwrap();

        // Posts dir with incomplete post (no computed.json)
        let posts_dir = base.join("posts");
        let incomplete = posts_dir.join("incomplete");
        std::fs::create_dir_all(&incomplete).unwrap();
        std::fs::write(incomplete.join("content.html"), "<p>hi</p>").unwrap();
        // No computed.json

        // Minimal templates
        let tpl_dir = base.join("tpl");
        std::fs::create_dir_all(&tpl_dir).unwrap();
        std::fs::write(
            tpl_dir.join("base.html"),
            "{% block content %}{% endblock %}",
        )
        .unwrap();
        std::fs::write(
            tpl_dir.join("index.html"),
            "{% extends \"base.html\" %}{% block content %}ok{% endblock %}",
        )
        .unwrap();

        let static_dir = base.join("static");
        std::fs::create_dir_all(&static_dir).unwrap();

        let out = base.join("out");
        let result = run_compose(&config_path, &posts_dir, &tpl_dir, &static_dir, &out).unwrap();
        // Only the index page should be produced (no posts rendered)
        assert_eq!(result.len(), 1);
    }
}
