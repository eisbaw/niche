use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

/// A single entry in the links registry.
#[derive(Debug, Clone, Deserialize)]
pub struct LinkEntry {
    pub title: String,
    pub url: String,
}

/// The links registry: slug -> LinkEntry.
pub type LinksRegistry = HashMap<String, LinkEntry>;

/// Parse a links registry from a JSON file.
pub fn load_links_registry(path: &Path) -> Result<LinksRegistry, LinkError> {
    let data =
        std::fs::read_to_string(path).map_err(|e| LinkError::ReadFailed(path.to_path_buf(), e))?;
    let registry: LinksRegistry =
        serde_json::from_str(&data).map_err(|e| LinkError::InvalidJson(path.to_path_buf(), e))?;
    Ok(registry)
}

/// Regex matching wikilink placeholder anchors produced by the render step.
///
/// Captures:
///   1 = slug (from data-slug attribute)
///   2 = display text (inner HTML of the anchor)
static WIKILINK_PLACEHOLDER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<a\s+class="wikilink"\s+data-slug="([^"]+)">([^<]*)</a>"#).unwrap()
});

/// Resolve wikilink placeholders in HTML using the links registry.
///
/// Returns the resolved HTML and a list of broken slugs (not found in registry).
pub fn resolve_wikilinks(html: &str, registry: &LinksRegistry) -> (String, Vec<String>) {
    let mut broken_slugs = Vec::new();

    let result = WIKILINK_PLACEHOLDER_RE
        .replace_all(html, |caps: &regex::Captures| {
            let slug = &caps[1];
            let display_text = &caps[2];

            match registry.get(slug) {
                Some(entry) => {
                    // If display text is [[slug]], replace with the title from registry
                    let final_text = if display_text == format!("[[{slug}]]") {
                        entry.title.as_str()
                    } else {
                        display_text
                    };
                    format!("<a href=\"{}\">{}</a>", entry.url, final_text)
                }
                None => {
                    broken_slugs.push(slug.to_string());
                    format!(
                        "<a class=\"wikilink broken-link\" data-slug=\"{slug}\">{display_text}</a>"
                    )
                }
            }
        })
        .into_owned();

    (result, broken_slugs)
}

/// Run the link resolution pipeline on a posts directory.
///
/// For each subdirectory in `posts_dir`:
/// - Reads `content.html` and resolves wikilink placeholders
/// - Copies `computed.json` unchanged
/// - Copies `assets/` directory unchanged (if present)
/// - Writes everything to the corresponding subdirectory in `out_dir`
///
/// Returns the list of output directories created.
pub fn run_link(
    links_path: &Path,
    posts_dir: &Path,
    out_dir: &Path,
) -> Result<Vec<PathBuf>, LinkError> {
    let registry = load_links_registry(links_path)?;

    let entries = std::fs::read_dir(posts_dir)
        .map_err(|e| LinkError::ReadFailed(posts_dir.to_path_buf(), e))?;

    let mut output_dirs = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| LinkError::ReadFailed(posts_dir.to_path_buf(), e))?;
        let entry_path = entry.path();

        if !entry_path.is_dir() {
            continue;
        }

        let slug = entry_path
            .file_name()
            .expect("directory entry must have a name")
            .to_string_lossy()
            .to_string();

        let post_out_dir = out_dir.join(&slug);
        std::fs::create_dir_all(&post_out_dir)
            .map_err(|e| LinkError::WriteFailed(post_out_dir.clone(), e))?;

        // Resolve wikilinks in content.html
        let content_path = entry_path.join("content.html");
        if content_path.exists() {
            let html = std::fs::read_to_string(&content_path)
                .map_err(|e| LinkError::ReadFailed(content_path.clone(), e))?;

            let (resolved, broken) = resolve_wikilinks(&html, &registry);

            for broken_slug in &broken {
                eprintln!(
                    "warning: broken link to '{broken_slug}' in post '{slug}'"
                );
            }

            std::fs::write(post_out_dir.join("content.html"), &resolved)
                .map_err(|e| LinkError::WriteFailed(post_out_dir.join("content.html"), e))?;
        }

        // Copy computed.json unchanged
        let computed_path = entry_path.join("computed.json");
        if computed_path.exists() {
            std::fs::copy(&computed_path, post_out_dir.join("computed.json"))
                .map_err(|e| LinkError::WriteFailed(post_out_dir.join("computed.json"), e))?;
        }

        // Copy assets/ directory unchanged
        let assets_path = entry_path.join("assets");
        if assets_path.is_dir() {
            copy_dir_recursive(&assets_path, &post_out_dir.join("assets"))?;
        }

        output_dirs.push(post_out_dir);
    }

    Ok(output_dirs)
}

/// Recursively copy a directory and its contents.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), LinkError> {
    std::fs::create_dir_all(dst).map_err(|e| LinkError::WriteFailed(dst.to_path_buf(), e))?;

    for entry in
        std::fs::read_dir(src).map_err(|e| LinkError::ReadFailed(src.to_path_buf(), e))?
    {
        let entry = entry.map_err(|e| LinkError::ReadFailed(src.to_path_buf(), e))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)
                .map_err(|e| LinkError::WriteFailed(dst_path, e))?;
        }
    }

    Ok(())
}

#[derive(Debug)]
pub enum LinkError {
    ReadFailed(PathBuf, std::io::Error),
    WriteFailed(PathBuf, std::io::Error),
    InvalidJson(PathBuf, serde_json::Error),
}

impl std::fmt::Display for LinkError {
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
        }
    }
}

impl std::error::Error for LinkError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_registry() -> LinksRegistry {
        let mut reg = LinksRegistry::new();
        reg.insert(
            "hello-world".to_string(),
            LinkEntry {
                title: "Hello World".to_string(),
                url: "/posts/hello-world/".to_string(),
            },
        );
        reg.insert(
            "second-post".to_string(),
            LinkEntry {
                title: "A Second Post".to_string(),
                url: "/posts/second-post/".to_string(),
            },
        );
        reg
    }

    #[test]
    fn resolves_known_slug_with_default_display() {
        let html = r#"<p>See <a class="wikilink" data-slug="hello-world">[[hello-world]]</a> for more.</p>"#;
        let (resolved, broken) = resolve_wikilinks(html, &sample_registry());
        assert_eq!(
            resolved,
            r#"<p>See <a href="/posts/hello-world/">Hello World</a> for more.</p>"#
        );
        assert!(broken.is_empty());
    }

    #[test]
    fn resolves_known_slug_with_custom_display() {
        let html =
            r#"<a class="wikilink" data-slug="second-post">click here</a>"#;
        let (resolved, broken) = resolve_wikilinks(html, &sample_registry());
        assert_eq!(
            resolved,
            r#"<a href="/posts/second-post/">click here</a>"#
        );
        assert!(broken.is_empty());
    }

    #[test]
    fn broken_link_adds_class_and_reports() {
        let html =
            r#"<a class="wikilink" data-slug="nonexistent">[[nonexistent]]</a>"#;
        let (resolved, broken) = resolve_wikilinks(html, &sample_registry());
        assert_eq!(
            resolved,
            r#"<a class="wikilink broken-link" data-slug="nonexistent">[[nonexistent]]</a>"#
        );
        assert_eq!(broken, vec!["nonexistent"]);
    }

    #[test]
    fn multiple_links_in_one_document() {
        let html = r#"<p><a class="wikilink" data-slug="hello-world">[[hello-world]]</a> and <a class="wikilink" data-slug="second-post">my link</a></p>"#;
        let (resolved, broken) = resolve_wikilinks(html, &sample_registry());
        assert!(
            resolved.contains(r#"<a href="/posts/hello-world/">Hello World</a>"#),
            "got: {resolved}"
        );
        assert!(
            resolved.contains(r#"<a href="/posts/second-post/">my link</a>"#),
            "got: {resolved}"
        );
        assert!(broken.is_empty());
    }

    #[test]
    fn no_wikilinks_passthrough() {
        let html = "<p>No links here.</p>";
        let (resolved, broken) = resolve_wikilinks(html, &sample_registry());
        assert_eq!(resolved, html);
        assert!(broken.is_empty());
    }

    #[test]
    fn mixed_known_and_broken() {
        let html = r#"<a class="wikilink" data-slug="hello-world">[[hello-world]]</a> <a class="wikilink" data-slug="missing">oops</a>"#;
        let (resolved, broken) = resolve_wikilinks(html, &sample_registry());
        assert!(resolved.contains(r#"<a href="/posts/hello-world/">Hello World</a>"#));
        assert!(resolved.contains(r#"class="wikilink broken-link""#));
        assert_eq!(broken, vec!["missing"]);
    }

    #[test]
    fn run_link_end_to_end() {
        let tmp = tempfile::tempdir().unwrap();
        let posts_dir = tmp.path().join("posts");
        let out_dir = tmp.path().join("out");

        // Create a post directory
        let post_dir = posts_dir.join("hello-world");
        std::fs::create_dir_all(&post_dir).unwrap();
        std::fs::write(
            post_dir.join("content.html"),
            r#"<p>See <a class="wikilink" data-slug="second-post">[[second-post]]</a></p>"#,
        )
        .unwrap();
        std::fs::write(post_dir.join("computed.json"), r#"{"slug":"hello-world"}"#).unwrap();

        // Create assets
        let assets_dir = post_dir.join("assets");
        std::fs::create_dir_all(&assets_dir).unwrap();
        std::fs::write(assets_dir.join("image.png"), b"fakepng").unwrap();

        // Write links registry
        let links_path = tmp.path().join("links.json");
        std::fs::write(
            &links_path,
            r#"{"second-post": {"title": "A Second Post", "url": "/posts/second-post/"}}"#,
        )
        .unwrap();

        let result = run_link(&links_path, &posts_dir, &out_dir).unwrap();
        assert_eq!(result.len(), 1);

        // Check resolved HTML
        let resolved = std::fs::read_to_string(out_dir.join("hello-world/content.html")).unwrap();
        assert_eq!(
            resolved,
            r#"<p>See <a href="/posts/second-post/">A Second Post</a></p>"#
        );

        // Check computed.json copied
        let computed =
            std::fs::read_to_string(out_dir.join("hello-world/computed.json")).unwrap();
        assert_eq!(computed, r#"{"slug":"hello-world"}"#);

        // Check assets copied
        let asset = std::fs::read(out_dir.join("hello-world/assets/image.png")).unwrap();
        assert_eq!(asset, b"fakepng");
    }
}
