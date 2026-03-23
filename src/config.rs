use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct PostConfig {
    pub slug: String,
    pub title: String,
    pub date: String,

    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl PostConfig {
    pub fn from_file(path: &Path) -> Result<Self, PostConfigError> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| PostConfigError::ReadFailed(path.to_path_buf(), e))?;
        let config: PostConfig = serde_json::from_str(&contents)
            .map_err(|e| PostConfigError::ParseFailed(path.to_path_buf(), e))?;
        Ok(config)
    }
}

#[derive(Debug)]
pub enum PostConfigError {
    ReadFailed(std::path::PathBuf, std::io::Error),
    ParseFailed(std::path::PathBuf, serde_json::Error),
}

impl std::fmt::Display for PostConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadFailed(path, err) => {
                write!(f, "failed to read config file {}: {err}", path.display())
            }
            Self::ParseFailed(path, err) => {
                write!(f, "failed to parse config file {}: {err}", path.display())
            }
        }
    }
}

impl std::error::Error for PostConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_config() {
        let json = r#"{
            "slug": "hello-world",
            "title": "Hello World",
            "date": "2024-03-15",
            "tags": ["test"],
            "summary": "A test post"
        }"#;
        let config: PostConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.slug, "hello-world");
        assert_eq!(config.title, "Hello World");
        assert_eq!(config.date, "2024-03-15");
        assert_eq!(config.extra.len(), 2);
        assert!(config.extra.contains_key("tags"));
        assert!(config.extra.contains_key("summary"));
    }

    #[test]
    fn rejects_missing_required_field() {
        let json = r#"{"slug": "test", "title": "Test"}"#;
        let result: Result<PostConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("date"),
            "error should mention missing 'date' field: {err}"
        );
    }

    #[test]
    fn handles_no_extra_fields() {
        let json = r#"{"slug": "s", "title": "t", "date": "d"}"#;
        let config: PostConfig = serde_json::from_str(json).unwrap();
        assert!(config.extra.is_empty());
    }

    #[test]
    fn from_file_missing_file() {
        let result = PostConfig::from_file(Path::new("/nonexistent/config.json"));
        assert!(matches!(result, Err(PostConfigError::ReadFailed(_, _))));
    }
}
