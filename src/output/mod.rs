//! Output rendering for parsed Java models in multiple formats.

mod json;
mod markdown;
mod toml;

use std::fmt;
use std::path::Path;

use crate::java_ast::JavaFile;
use crate::markdown::MarkdownError;

/// The serialization format for rendered output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Markdown,
    Json,
    Toml,
}

/// One parsed Java file paired with its source path, ready to be rendered.
pub struct FileOutput<'a> {
    pub path: &'a Path,
    pub ast: &'a JavaFile,
}

/// Errors raised by any output renderer.
#[derive(Debug)]
pub enum OutputError {
    Markdown(MarkdownError),
    Json(serde_json::Error),
    Toml(::toml::ser::Error),
}

impl fmt::Display for OutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputError::Markdown(e) => write!(f, "Markdown render error: {}", e),
            OutputError::Json(e) => write!(f, "JSON render error: {}", e),
            OutputError::Toml(e) => write!(f, "TOML render error: {}", e),
        }
    }
}

impl std::error::Error for OutputError {}

impl From<MarkdownError> for OutputError {
    fn from(e: MarkdownError) -> Self {
        OutputError::Markdown(e)
    }
}

impl From<serde_json::Error> for OutputError {
    fn from(e: serde_json::Error) -> Self {
        OutputError::Json(e)
    }
}

impl From<::toml::ser::Error> for OutputError {
    fn from(e: ::toml::ser::Error) -> Self {
        OutputError::Toml(e)
    }
}

/// Render a set of parsed files into a single string in the requested format.
///
/// Aggregation rules per format:
/// - Markdown: streams one rendered document per file, separated by
///   `--- File: "<path>" ---` headers.
/// - JSON: emits a single array `[{ "path": ..., "ast": ... }, ...]`.
/// - TOML: emits a single document with a `[[files]]` table array.
pub fn render(files: &[FileOutput<'_>], format: Format) -> Result<String, OutputError> {
    match format {
        Format::Markdown => markdown::render(files),
        Format::Json => json::render(files),
        Format::Toml => toml::render(files),
    }
}
