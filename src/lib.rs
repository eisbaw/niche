pub mod computed;
pub mod config;
pub mod render;

use std::path::Path;

/// Errors that can occur during the full render pipeline.
#[derive(Debug)]
pub enum PipelineError {
    Config(config::PostConfigError),
    Render(render::RenderError),
    Computed(computed::ComputedError),
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(e) => write!(f, "{e}"),
            Self::Render(e) => write!(f, "{e}"),
            Self::Computed(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PipelineError {}

impl From<config::PostConfigError> for PipelineError {
    fn from(e: config::PostConfigError) -> Self {
        Self::Config(e)
    }
}

impl From<render::RenderError> for PipelineError {
    fn from(e: render::RenderError) -> Self {
        Self::Render(e)
    }
}

impl From<computed::ComputedError> for PipelineError {
    fn from(e: computed::ComputedError) -> Self {
        Self::Computed(e)
    }
}

/// Run the full render pipeline: read config + content, produce content.html and computed.json.
///
/// Returns the paths to (content.html, computed.json) on success.
pub fn run_render(
    config_path: &Path,
    content_path: &Path,
    out_dir: &Path,
) -> Result<(std::path::PathBuf, std::path::PathBuf), PipelineError> {
    let post_config = config::PostConfig::from_file(config_path)?;
    let html = render::render_file(content_path)?;
    let html_path = render::write_html(&html, out_dir)?;
    let computed_json = computed::build_computed_json(&post_config, &html);
    let json_path = computed::write_computed_json(&computed_json, out_dir)?;
    Ok((html_path, json_path))
}
