use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;
use tera::{Context, Tera};

// ---------------------------------------------------------------------------
// Site config types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
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
    /// Path to the post's source directory in posts-dir (for copying assets).
    source_dir: std::path::PathBuf,
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
// Tag slugification
// ---------------------------------------------------------------------------

/// Slugify a tag name for use in filesystem paths and URLs:
/// lowercase, replace spaces with hyphens, strip non-alphanumeric chars
/// (except hyphens), and collapse consecutive hyphens.
fn slugify_tag(tag: &str) -> String {
    let s: String = tag
        .to_lowercase()
        .chars()
        .map(|c| if c == ' ' { '-' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    // Collapse consecutive hyphens and trim leading/trailing hyphens.
    let mut result = String::with_capacity(s.len());
    let mut prev_hyphen = true; // treat start as if preceded by hyphen to trim leading
    for c in s.chars() {
        if c == '-' {
            if !prev_hyphen {
                result.push('-');
            }
            prev_hyphen = true;
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }
    // Trim trailing hyphen
    if result.ends_with('-') {
        result.pop();
    }
    result
}

// ---------------------------------------------------------------------------
// Tag collection
// ---------------------------------------------------------------------------

/// Collect all tags from posts and return a map of tag -> count, sorted by tag name.
fn collect_tags(posts: &[PostEntry]) -> BTreeMap<String, usize> {
    let mut tags: BTreeMap<String, usize> = BTreeMap::new();
    for post in posts {
        if let Some(tag_array) = post.metadata.get("tags").and_then(|v| v.as_array()) {
            for tag_val in tag_array {
                if let Some(tag) = tag_val.as_str() {
                    *tags.entry(tag.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
    tags
}

/// Filter posts that have a given tag.
fn posts_with_tag<'a>(posts: &'a [PostEntry], tag: &str) -> Vec<&'a PostEntry> {
    posts
        .iter()
        .filter(|p| {
            p.metadata
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().any(|t| t.as_str() == Some(tag)))
                .unwrap_or(false)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Archive grouping
// ---------------------------------------------------------------------------

/// Group posts by year (extracted from date field). Returns years in descending order.
fn group_posts_by_year(posts: &[PostEntry]) -> Vec<(String, Vec<&Value>)> {
    let mut by_year: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for post in posts {
        let year = post
            .metadata
            .get("date")
            .and_then(|v| v.as_str())
            .and_then(|d| d.split('-').next())
            .unwrap_or("Unknown")
            .to_string();
        by_year.entry(year).or_default().push(&post.metadata);
    }
    // Reverse to get descending order (newest year first).
    let mut years: Vec<(String, Vec<&Value>)> = by_year.into_iter().collect();
    years.reverse();
    years
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
    let site_value =
        serde_json::to_value(&site_config).expect("SiteConfig serialization should not fail");

    // Collect tags for use in all templates.
    let tags_map = collect_tags(&posts);
    let tags_value =
        serde_json::to_value(&tags_map).expect("tags map serialization should not fail");

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
        context.insert("tags", &tags_value);

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

        // Copy per-post assets (images, SVGs, etc.) if present.
        let assets_src = post.source_dir.join("assets");
        if assets_src.is_dir() {
            let assets_dst = post_out_dir.join("assets");
            crate::fs_utils::copy_dir_recursive(&assets_src, &assets_dst)
                .map_err(|e| ComposeError::WriteFailed(assets_dst.clone(), e))?;
        }
    }

    // --- Render all paginated index pages ---
    {
        let posts_per_page = site_config.posts_per_page;
        let total_posts = posts.len();
        let total_pages = if total_posts == 0 {
            1
        } else {
            total_posts.div_ceil(posts_per_page)
        };

        for page_num in 1..=total_pages {
            let start = (page_num - 1) * posts_per_page;
            let end = (start + posts_per_page).min(total_posts);

            let page_posts: Vec<&Value> = posts[start..end].iter().map(|p| &p.metadata).collect();

            let prev_url = match page_num {
                1 => None,
                2 => Some("/".to_string()),
                n => Some(format!("/page/{}/", n - 1)),
            };

            let next_url = if page_num < total_pages {
                Some(format!("/page/{}/", page_num + 1))
            } else {
                None
            };

            let pagination = Pagination {
                current: page_num,
                total_pages,
                prev_url,
                next_url,
            };

            let current_url = if page_num == 1 {
                "/".to_string()
            } else {
                format!("/page/{}/", page_num)
            };

            let mut context = Context::new();
            context.insert("site", &site_value);
            context.insert("current_url", &current_url);
            context.insert("posts", &page_posts);
            context.insert("pagination", &pagination);
            context.insert("tags", &tags_value);

            let rendered = tera.render("index.html", &context)?;

            let page_dir = if page_num == 1 {
                out_dir.to_path_buf()
            } else {
                out_dir.join(format!("page/{}", page_num))
            };
            std::fs::create_dir_all(&page_dir)
                .map_err(|e| ComposeError::WriteFailed(page_dir.clone(), e))?;

            let index_path = page_dir.join("index.html");
            std::fs::write(&index_path, &rendered)
                .map_err(|e| ComposeError::WriteFailed(index_path.clone(), e))?;

            output_paths.push(index_path);
        }
    }

    // --- Render tag pages ---
    for tag in tags_map.keys() {
        let tagged_posts = posts_with_tag(&posts, tag);
        let tag_post_values: Vec<&Value> = tagged_posts.iter().map(|p| &p.metadata).collect();

        let tag_slug = slugify_tag(tag);
        let tag_dir = out_dir.join("tags").join(&tag_slug);
        std::fs::create_dir_all(&tag_dir)
            .map_err(|e| ComposeError::WriteFailed(tag_dir.clone(), e))?;

        let current_url = format!("/tags/{tag_slug}/");

        let mut context = Context::new();
        context.insert("site", &site_value);
        context.insert("current_url", &current_url);
        context.insert("tag_name", tag);
        context.insert("tag_slug", &tag_slug);
        context.insert("posts", &tag_post_values);
        context.insert("tags", &tags_value);

        let rendered = tera.render("tag.html", &context)?;
        let index_path = tag_dir.join("index.html");
        std::fs::write(&index_path, &rendered)
            .map_err(|e| ComposeError::WriteFailed(index_path.clone(), e))?;

        output_paths.push(index_path);
    }

    // --- Render archive page ---
    {
        let archive_dir = out_dir.join("archive");
        std::fs::create_dir_all(&archive_dir)
            .map_err(|e| ComposeError::WriteFailed(archive_dir.clone(), e))?;

        let years = group_posts_by_year(&posts);

        // Build a serializable structure: Vec of {year, posts}.
        let years_value: Vec<Value> = years
            .iter()
            .map(|(year, year_posts)| {
                serde_json::json!({
                    "year": year,
                    "posts": year_posts,
                })
            })
            .collect();

        let current_url = "/archive/".to_string();

        let mut context = Context::new();
        context.insert("site", &site_value);
        context.insert("current_url", &current_url);
        context.insert("years", &years_value);
        context.insert("tags", &tags_value);

        let rendered = tera.render("archive.html", &context)?;
        let index_path = archive_dir.join("index.html");
        std::fs::write(&index_path, &rendered)
            .map_err(|e| ComposeError::WriteFailed(index_path.clone(), e))?;

        output_paths.push(index_path);
    }

    // --- Render Atom feed ---
    {
        let feed_enabled = site_config.feed.as_ref().map(|f| f.enable).unwrap_or(false);

        if feed_enabled {
            let feed_title = site_config
                .feed
                .as_ref()
                .and_then(|f| f.title.clone())
                .unwrap_or_else(|| site_config.site_name.clone());

            let feed_description = site_config
                .feed
                .as_ref()
                .and_then(|f| f.description.clone())
                .unwrap_or_default();

            let author_name = site_config
                .author
                .as_ref()
                .map(|a| a.name.clone())
                .unwrap_or_default();

            let author_email = site_config
                .author
                .as_ref()
                .and_then(|a| a.email.clone())
                .unwrap_or_default();

            // Take up to 20 most recent posts for the feed.
            let feed_post_count = 20.min(posts.len());
            let feed_posts: Vec<Value> = posts[..feed_post_count]
                .iter()
                .map(|p| {
                    let mut val = p.metadata.clone();
                    if let Some(obj) = val.as_object_mut() {
                        obj.insert("content".into(), Value::String(p.content.clone()));
                    }
                    val
                })
                .collect();

            // Use the date of the most recent post as the feed updated time.
            let updated = posts
                .first()
                .and_then(|p| p.metadata.get("date"))
                .and_then(|v| v.as_str())
                .map(|d| format!("{d}T00:00:00Z"))
                .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());

            let mut context = Context::new();
            context.insert("site", &site_value);
            context.insert("feed_title", &feed_title);
            context.insert("feed_description", &feed_description);
            context.insert("author_name", &author_name);
            context.insert("author_email", &author_email);
            context.insert("posts", &feed_posts);
            context.insert("updated", &updated);

            let rendered = tera.render("feed.xml", &context)?;
            let feed_path = out_dir.join("feed.xml");
            std::fs::write(&feed_path, &rendered)
                .map_err(|e| ComposeError::WriteFailed(feed_path.clone(), e))?;

            output_paths.push(feed_path);
        }
    }

    // --- Copy static assets ---
    if static_dir.is_dir() {
        let static_out = out_dir.join("static");
        crate::fs_utils::copy_dir_recursive(static_dir, &static_out)
            .map_err(|e| ComposeError::WriteFailed(static_out.clone(), e))?;
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
            let slug = entry_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| entry_path.display().to_string());
            eprintln!(
                "warning: skipping {}: missing content.html or computed.json",
                slug
            );
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
            source_dir: entry_path,
        });
    }

    Ok(posts)
}

fn slugify_filter(
    value: &Value,
    _args: &std::collections::HashMap<String, Value>,
) -> tera::Result<Value> {
    let s = tera::try_get_value!("slugify_tag", "value", String, value);
    Ok(Value::String(slugify_tag(&s)))
}

fn xml_escape_filter(
    value: &Value,
    _args: &std::collections::HashMap<String, Value>,
) -> tera::Result<Value> {
    let s = tera::try_get_value!("xml_escape", "value", String, value);
    let escaped = s
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;");
    Ok(Value::String(escaped))
}

fn load_templates(template_dir: &Path) -> Result<Tera, ComposeError> {
    let glob = template_dir
        .join("**")
        .join("*")
        .to_string_lossy()
        .to_string();
    let mut tera = Tera::new(&glob)?;
    // Only auto-escape HTML files; XML templates (e.g. feed.xml) should not
    // have their values escaped since they manage their own encoding.
    tera.autoescape_on(vec![".html", ".htm"]);
    tera.register_filter("xml_escape", xml_escape_filter);
    tera.register_filter("slugify_tag", slugify_filter);
    Ok(tera)
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
  },
  "feed": {
    "enable": true,
    "title": "Test Blog Feed",
    "description": "A test feed"
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

        // Templates
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

        std::fs::write(
            template_dir.join("tag.html"),
            r#"{% extends "base.html" %}
{% block title %}Posts tagged "{{ tag_name }}" — {{ site.site_name }}{% endblock %}
{% block content %}
<h1>Posts tagged "{{ tag_name }}"</h1>
<div class="post-list">
  {% for p in posts %}
  <article>
    <h2><a href="/posts/{{ p.slug }}/">{{ p.title }}</a></h2>
    <time>{{ p.date }}</time>
    {% if p.summary %}<p>{{ p.summary }}</p>{% endif %}
  </article>
  {% endfor %}
</div>
{% endblock %}"#,
        )
        .unwrap();

        std::fs::write(
            template_dir.join("archive.html"),
            r#"{% extends "base.html" %}
{% block title %}Archive — {{ site.site_name }}{% endblock %}
{% block content %}
<h1>Archive</h1>
{% for group in years %}
<section>
  <h2>{{ group.year }}</h2>
  <ul>
    {% for p in group.posts %}
    <li>
      <time>{{ p.date }}</time>
      <a href="/posts/{{ p.slug }}/">{{ p.title }}</a>
    </li>
    {% endfor %}
  </ul>
</section>
{% endfor %}
{% endblock %}"#,
        )
        .unwrap();

        std::fs::write(
            template_dir.join("feed.xml"),
            r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>{{ feed_title | xml_escape }}</title>
  <subtitle>{{ feed_description | xml_escape }}</subtitle>
  <link href="{{ site.base_url }}/feed.xml" rel="self" type="application/atom+xml"/>
  <link href="{{ site.base_url }}/" rel="alternate" type="text/html"/>
  <id>{{ site.base_url }}/</id>
  <updated>{{ updated }}</updated>
  <author>
    <name>{{ author_name | xml_escape }}</name>
    {% if author_email %}<email>{{ author_email }}</email>{% endif %}
  </author>
  {% for p in posts %}
  <entry>
    <title>{{ p.title | xml_escape }}</title>
    <link href="{{ site.base_url }}/posts/{{ p.slug }}/" rel="alternate" type="text/html"/>
    <id>{{ site.base_url }}/posts/{{ p.slug }}/</id>
    <published>{{ p.date }}T00:00:00Z</published>
    <updated>{{ p.date }}T00:00:00Z</updated>
    {% if p.summary %}<summary>{{ p.summary | xml_escape }}</summary>{% endif %}
  </entry>
  {% endfor %}
</feed>"#,
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

    /// Fixture with 3 posts to test pagination (posts_per_page=2 means 2 pages).
    fn setup_pagination_fixture() -> (TempDir, PathBuf, PathBuf, PathBuf, PathBuf, PathBuf) {
        let (tmp, config_path, posts_dir, template_dir, static_dir, out_dir) = setup_fixture();

        // Add a third post so pagination kicks in.
        let post3_dir = posts_dir.join("third-post");
        std::fs::create_dir_all(&post3_dir).unwrap();
        std::fs::write(post3_dir.join("content.html"), "<p>Third post content.</p>").unwrap();
        std::fs::write(
            post3_dir.join("computed.json"),
            r#"{
  "slug": "third-post",
  "title": "Third Post",
  "date": "2023-12-01",
  "tags": ["nix"],
  "summary": "The third post.",
  "word_count": 3,
  "reading_time_minutes": 1
}"#,
        )
        .unwrap();

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

        // Should produce post pages + index + tag pages + archive + feed
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
        std::fs::write(
            tpl_dir.join("archive.html"),
            "{% extends \"base.html\" %}{% block content %}archive{% endblock %}",
        )
        .unwrap();
        std::fs::write(
            tpl_dir.join("tag.html"),
            "{% extends \"base.html\" %}{% block content %}tag{% endblock %}",
        )
        .unwrap();

        let static_dir = base.join("static");
        std::fs::create_dir_all(&static_dir).unwrap();

        let out = base.join("out");
        let result = run_compose(&config_path, &posts_dir, &tpl_dir, &static_dir, &out).unwrap();
        // Index page + archive page (no tag pages since no posts have tags, no feed since not enabled)
        assert_eq!(result.len(), 2);
    }

    // --- Pagination tests ---

    #[test]
    fn compose_pagination_produces_multiple_pages() {
        let (_tmp, config, posts_dir, templates, static_dir, out) = setup_pagination_fixture();
        run_compose(&config, &posts_dir, &templates, &static_dir, &out).unwrap();

        // Page 1 at /index.html
        let page1 = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(
            page1.contains("Page 1 of 2"),
            "page 1 should show pagination: got {}",
            page1
        );

        // Page 2 at /page/2/index.html
        let page2_path = out.join("page/2/index.html");
        assert!(page2_path.exists(), "page 2 should exist");
        let page2 = std::fs::read_to_string(&page2_path).unwrap();
        assert!(
            page2.contains("Page 2 of 2"),
            "page 2 should show pagination: got {}",
            page2
        );
    }

    // --- Tag tests ---

    #[test]
    fn collect_tags_from_posts() {
        let posts = vec![
            PostEntry {
                slug: "a".into(),
                metadata: serde_json::json!({"tags": ["rust", "nix"]}),
                content: String::new(),
                source_dir: std::path::PathBuf::new(),
            },
            PostEntry {
                slug: "b".into(),
                metadata: serde_json::json!({"tags": ["rust"]}),
                content: String::new(),
                source_dir: std::path::PathBuf::new(),
            },
            PostEntry {
                slug: "c".into(),
                metadata: serde_json::json!({}),
                content: String::new(),
                source_dir: std::path::PathBuf::new(),
            },
        ];
        let tags = collect_tags(&posts);
        assert_eq!(tags.get("rust"), Some(&2));
        assert_eq!(tags.get("nix"), Some(&1));
        assert_eq!(tags.len(), 2);
    }

    #[test]
    fn collect_tags_empty_posts() {
        let tags = collect_tags(&[]);
        assert!(tags.is_empty());
    }

    #[test]
    fn posts_with_tag_filters_correctly() {
        let posts = vec![
            PostEntry {
                slug: "a".into(),
                metadata: serde_json::json!({"tags": ["rust", "nix"]}),
                content: String::new(),
                source_dir: std::path::PathBuf::new(),
            },
            PostEntry {
                slug: "b".into(),
                metadata: serde_json::json!({"tags": ["rust"]}),
                content: String::new(),
                source_dir: std::path::PathBuf::new(),
            },
            PostEntry {
                slug: "c".into(),
                metadata: serde_json::json!({"tags": ["nix"]}),
                content: String::new(),
                source_dir: std::path::PathBuf::new(),
            },
        ];
        let rust_posts = posts_with_tag(&posts, "rust");
        assert_eq!(rust_posts.len(), 2);

        let nix_posts = posts_with_tag(&posts, "nix");
        assert_eq!(nix_posts.len(), 2);

        let missing = posts_with_tag(&posts, "python");
        assert!(missing.is_empty());
    }

    #[test]
    fn compose_generates_tag_pages() {
        let (_tmp, config, posts_dir, templates, static_dir, out) = setup_fixture();
        run_compose(&config, &posts_dir, &templates, &static_dir, &out).unwrap();

        // Tag "rust" should have both posts
        let rust_tag = std::fs::read_to_string(out.join("tags/rust/index.html")).unwrap();
        assert!(
            rust_tag.contains("Posts tagged"),
            "tag page should have heading"
        );
        assert!(
            rust_tag.contains("First Post"),
            "rust tag should include first post"
        );
        assert!(
            rust_tag.contains("Second Post"),
            "rust tag should include second post"
        );

        // Tag "nix" should have only first post
        let nix_tag = std::fs::read_to_string(out.join("tags/nix/index.html")).unwrap();
        assert!(
            nix_tag.contains("First Post"),
            "nix tag should include first post"
        );
        assert!(
            !nix_tag.contains("Second Post"),
            "nix tag should not include second post"
        );
    }

    // --- Archive tests ---

    #[test]
    fn group_posts_by_year_correct() {
        let posts = vec![
            PostEntry {
                slug: "a".into(),
                metadata: serde_json::json!({"date": "2024-04-01", "title": "A"}),
                content: String::new(),
                source_dir: std::path::PathBuf::new(),
            },
            PostEntry {
                slug: "b".into(),
                metadata: serde_json::json!({"date": "2024-01-15", "title": "B"}),
                content: String::new(),
                source_dir: std::path::PathBuf::new(),
            },
            PostEntry {
                slug: "c".into(),
                metadata: serde_json::json!({"date": "2023-06-01", "title": "C"}),
                content: String::new(),
                source_dir: std::path::PathBuf::new(),
            },
        ];
        let years = group_posts_by_year(&posts);
        assert_eq!(years.len(), 2);
        // Descending: 2024 first, then 2023
        assert_eq!(years[0].0, "2024");
        assert_eq!(years[0].1.len(), 2);
        assert_eq!(years[1].0, "2023");
        assert_eq!(years[1].1.len(), 1);
    }

    #[test]
    fn group_posts_by_year_empty() {
        let years = group_posts_by_year(&[]);
        assert!(years.is_empty());
    }

    #[test]
    fn compose_generates_archive_page() {
        let (_tmp, config, posts_dir, templates, static_dir, out) = setup_fixture();
        run_compose(&config, &posts_dir, &templates, &static_dir, &out).unwrap();

        let archive = std::fs::read_to_string(out.join("archive/index.html")).unwrap();
        assert!(archive.contains("Archive"), "archive should have heading");
        assert!(
            archive.contains("2024"),
            "archive should group by year 2024"
        );
        assert!(
            archive.contains("First Post"),
            "archive should list first post"
        );
        assert!(
            archive.contains("Second Post"),
            "archive should list second post"
        );
    }

    #[test]
    fn compose_archive_multiple_years() {
        let (_tmp, config, posts_dir, templates, static_dir, out) = setup_pagination_fixture();
        run_compose(&config, &posts_dir, &templates, &static_dir, &out).unwrap();

        let archive = std::fs::read_to_string(out.join("archive/index.html")).unwrap();
        assert!(archive.contains("2024"), "archive should have year 2024");
        assert!(archive.contains("2023"), "archive should have year 2023");
        // 2024 should appear before 2023 (descending)
        let pos_2024 = archive.find("2024").unwrap();
        let pos_2023 = archive.find("2023").unwrap();
        assert!(
            pos_2024 < pos_2023,
            "2024 should appear before 2023 in archive"
        );
    }

    // --- Feed tests ---

    #[test]
    fn compose_generates_atom_feed() {
        let (_tmp, config, posts_dir, templates, static_dir, out) = setup_fixture();
        run_compose(&config, &posts_dir, &templates, &static_dir, &out).unwrap();

        let feed = std::fs::read_to_string(out.join("feed.xml")).unwrap();
        assert!(
            feed.contains("<?xml version=\"1.0\""),
            "feed should be valid XML"
        );
        assert!(
            feed.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\">"),
            "feed should be Atom format"
        );
        assert!(
            feed.contains("<title>Test Blog Feed</title>"),
            "feed should have title"
        );
        assert!(feed.contains("<entry>"), "feed should contain entries");
        assert!(
            feed.contains("First Post"),
            "feed should contain first post"
        );
        assert!(
            feed.contains("Second Post"),
            "feed should contain second post"
        );
        assert!(
            feed.contains("<published>"),
            "entries should have published date"
        );
        assert!(feed.contains("<summary>"), "entries should have summary");
        assert!(feed.contains("<author>"), "feed should have author");
        assert!(feed.contains("Test Author"), "feed should have author name");
    }

    #[test]
    fn compose_no_feed_when_disabled() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        let config_path = base.join("config.json");
        std::fs::write(
            &config_path,
            r#"{
  "site_name": "No Feed Blog",
  "base_url": "https://example.com",
  "language": "en",
  "posts_per_page": 10
}"#,
        )
        .unwrap();

        let posts_dir = base.join("posts");
        std::fs::create_dir_all(&posts_dir).unwrap();

        let tpl_dir = base.join("tpl");
        std::fs::create_dir_all(&tpl_dir).unwrap();
        std::fs::write(
            tpl_dir.join("base.html"),
            "{% block content %}{% endblock %}",
        )
        .unwrap();
        std::fs::write(
            tpl_dir.join("index.html"),
            "{% extends \"base.html\" %}{% block content %}idx{% endblock %}",
        )
        .unwrap();
        std::fs::write(
            tpl_dir.join("archive.html"),
            "{% extends \"base.html\" %}{% block content %}arc{% endblock %}",
        )
        .unwrap();
        std::fs::write(
            tpl_dir.join("tag.html"),
            "{% extends \"base.html\" %}{% block content %}tag{% endblock %}",
        )
        .unwrap();

        let static_dir = base.join("static");
        std::fs::create_dir_all(&static_dir).unwrap();

        let out = base.join("out");
        run_compose(&config_path, &posts_dir, &tpl_dir, &static_dir, &out).unwrap();

        assert!(
            !out.join("feed.xml").exists(),
            "feed.xml should not be generated when feed is not configured"
        );
    }

    #[test]
    fn compose_feed_xml_structure() {
        let (_tmp, config, posts_dir, templates, static_dir, out) = setup_fixture();
        run_compose(&config, &posts_dir, &templates, &static_dir, &out).unwrap();

        let feed = std::fs::read_to_string(out.join("feed.xml")).unwrap();

        // Check required Atom elements
        assert!(feed.contains("<link href=\"https://example.com/feed.xml\" rel=\"self\""));
        assert!(feed.contains("<link href=\"https://example.com/\" rel=\"alternate\""));
        assert!(feed.contains("<id>https://example.com/</id>"));
        assert!(feed.contains("<updated>"));

        // Check entry structure
        assert!(feed.contains("<entry>"));
        assert!(feed.contains("</entry>"));
        assert!(feed.contains("<id>https://example.com/posts/"));
    }

    // --- Tags available in all templates ---

    #[test]
    fn tags_variable_available_in_all_pages() {
        let (_tmp, config, posts_dir, templates, static_dir, out) = setup_fixture();

        // Rewrite templates to output the tags variable.
        std::fs::write(
            templates.join("base.html"),
            "{% block content %}{% endblock %}",
        )
        .unwrap();
        std::fs::write(
            templates.join("post.html"),
            r#"{% extends "base.html" %}
{% block content %}TAGS:{% for t, c in tags %}{{t}}={{c}},{% endfor %}{% endblock %}"#,
        )
        .unwrap();
        std::fs::write(
            templates.join("index.html"),
            r#"{% extends "base.html" %}
{% block content %}TAGS:{% for t, c in tags %}{{t}}={{c}},{% endfor %}{% endblock %}"#,
        )
        .unwrap();
        std::fs::write(
            templates.join("archive.html"),
            r#"{% extends "base.html" %}
{% block content %}TAGS:{% for t, c in tags %}{{t}}={{c}},{% endfor %}{% endblock %}"#,
        )
        .unwrap();
        std::fs::write(
            templates.join("tag.html"),
            r#"{% extends "base.html" %}
{% block content %}TAGS:{% for t, c in tags %}{{t}}={{c}},{% endfor %}{% endblock %}"#,
        )
        .unwrap();

        run_compose(&config, &posts_dir, &templates, &static_dir, &out).unwrap();

        let index = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(index.contains("nix=1"), "index should have tags: {index}");
        assert!(index.contains("rust=2"), "index should have tags: {index}");

        let post = std::fs::read_to_string(out.join("posts/first-post/index.html")).unwrap();
        assert!(
            post.contains("rust=2"),
            "post page should have tags: {post}"
        );

        let archive = std::fs::read_to_string(out.join("archive/index.html")).unwrap();
        assert!(
            archive.contains("rust=2"),
            "archive should have tags: {archive}"
        );

        let tag_page = std::fs::read_to_string(out.join("tags/rust/index.html")).unwrap();
        assert!(
            tag_page.contains("nix=1"),
            "tag page should have tags: {tag_page}"
        );
    }
}
