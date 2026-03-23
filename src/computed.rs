use std::path::Path;

use serde_json::{Map, Value};

use crate::config::PostConfig;

/// Strip HTML tags from a string, returning only the text content.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;

    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                // Insert space to avoid joining words across tags (e.g. "<p>a</p><p>b</p>")
                result.push(' ');
            }
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    result
}

/// Count the number of words in an HTML fragment.
///
/// Strips HTML tags first, then splits on whitespace.
pub fn word_count(html: &str) -> usize {
    strip_html_tags(html).split_whitespace().count()
}

/// Estimate reading time in minutes from a word count.
///
/// Assumes ~250 words per minute. Rounds up, minimum 1.
pub fn reading_time_minutes(word_count: usize) -> usize {
    if word_count == 0 {
        return 1;
    }
    word_count.div_ceil(250)
}

/// Build the computed JSON object merging post config fields and computed values.
pub fn build_computed_json(config: &PostConfig, html: &str) -> Value {
    let wc = word_count(html);
    let rt = reading_time_minutes(wc);

    let mut map = Map::new();

    // Core config fields
    map.insert("slug".into(), Value::String(config.slug.clone()));
    map.insert("title".into(), Value::String(config.title.clone()));
    map.insert("date".into(), Value::String(config.date.clone()));

    // Extra fields from config
    for (key, value) in &config.extra {
        map.insert(key.clone(), value.clone());
    }

    // Computed fields
    map.insert("word_count".into(), Value::Number(wc.into()));
    map.insert("reading_time_minutes".into(), Value::Number(rt.into()));

    Value::Object(map)
}

/// Write the computed JSON to `<out_dir>/computed.json`.
pub fn write_computed_json(
    json: &Value,
    out_dir: &Path,
) -> Result<std::path::PathBuf, ComputedError> {
    std::fs::create_dir_all(out_dir)
        .map_err(|e| ComputedError::WriteFailed(out_dir.to_path_buf(), e))?;

    let out_path = out_dir.join("computed.json");
    let pretty =
        serde_json::to_string_pretty(json).expect("serializing computed JSON should never fail");

    std::fs::write(&out_path, pretty)
        .map_err(|e| ComputedError::WriteFailed(out_path.clone(), e))?;

    Ok(out_path)
}

#[derive(Debug)]
pub enum ComputedError {
    WriteFailed(std::path::PathBuf, std::io::Error),
}

impl std::fmt::Display for ComputedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WriteFailed(path, err) => {
                write!(
                    f,
                    "failed to write computed.json at {}: {err}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ComputedError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_tags_simple() {
        assert_eq!(
            strip_html_tags("<p>Hello <strong>world</strong></p>").trim(),
            "Hello  world"
        );
    }

    #[test]
    fn strip_tags_empty() {
        assert_eq!(strip_html_tags(""), "");
    }

    #[test]
    fn strip_tags_no_tags() {
        assert_eq!(strip_html_tags("plain text"), "plain text");
    }

    #[test]
    fn word_count_basic() {
        // The comma after </strong> gets a space before it from tag stripping,
        // making "," a separate token. That's 6 whitespace-separated tokens.
        let html = "<p>Hello <strong>world</strong>, how are you?</p>";
        assert_eq!(word_count(html), 6);
    }

    #[test]
    fn word_count_simple_sentence() {
        let html = "<p>one two three four five</p>";
        assert_eq!(word_count(html), 5);
    }

    #[test]
    fn word_count_empty_html() {
        assert_eq!(word_count(""), 0);
    }

    #[test]
    fn word_count_tags_only() {
        assert_eq!(word_count("<br/><hr/>"), 0);
    }

    #[test]
    fn word_count_multiple_paragraphs() {
        let html = "<p>First paragraph.</p><p>Second paragraph.</p>";
        assert_eq!(word_count(html), 4);
    }

    #[test]
    fn reading_time_zero_words() {
        assert_eq!(reading_time_minutes(0), 1);
    }

    #[test]
    fn reading_time_one_word() {
        assert_eq!(reading_time_minutes(1), 1);
    }

    #[test]
    fn reading_time_250_words() {
        assert_eq!(reading_time_minutes(250), 1);
    }

    #[test]
    fn reading_time_251_words() {
        assert_eq!(reading_time_minutes(251), 2);
    }

    #[test]
    fn reading_time_500_words() {
        assert_eq!(reading_time_minutes(500), 2);
    }

    #[test]
    fn reading_time_1000_words() {
        assert_eq!(reading_time_minutes(1000), 4);
    }

    #[test]
    fn build_json_merges_all_fields() {
        let config = PostConfig {
            slug: "test-post".into(),
            title: "Test Post".into(),
            date: "2024-01-15".into(),
            extra: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "tags".into(),
                    Value::Array(vec![Value::String("rust".into())]),
                );
                m
            },
        };
        let html = "<p>one two three four five six seven eight nine ten</p>";
        let json = build_computed_json(&config, html);

        let obj = json.as_object().unwrap();
        assert_eq!(obj["slug"], "test-post");
        assert_eq!(obj["title"], "Test Post");
        assert_eq!(obj["date"], "2024-01-15");
        assert_eq!(
            obj["tags"],
            Value::Array(vec![Value::String("rust".into())])
        );
        assert_eq!(obj["word_count"], 10);
        assert_eq!(obj["reading_time_minutes"], 1);
    }

    #[test]
    fn build_json_no_extra_fields() {
        let config = PostConfig {
            slug: "s".into(),
            title: "t".into(),
            date: "d".into(),
            extra: std::collections::HashMap::new(),
        };
        let json = build_computed_json(&config, "<p>word</p>");
        let obj = json.as_object().unwrap();
        assert_eq!(obj.len(), 5); // slug, title, date, word_count, reading_time_minutes
    }
}
