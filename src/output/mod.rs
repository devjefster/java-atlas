//! Output rendering for parsed Java models in multiple formats.

mod compact;
mod json;
mod jsonl;
mod markdown;

use std::fmt;
use std::path::{Path, PathBuf};

use crate::java_ast::JavaFile;
use crate::markdown::MarkdownError;

/// The serialization format for rendered output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Markdown,
    Json,
    Jsonl,
}

/// One parsed Java file paired with its source path, ready to be rendered.
pub struct FileOutput<'a> {
    pub path: &'a Path,
    pub ast: &'a JavaFile,
}

/// One package-scoped JSONL artifact ready to be written to disk.
pub struct JsonlPackage {
    pub package: String,
    pub relative_path: PathBuf,
    pub contents: String,
}

/// Errors raised by any output renderer.
#[derive(Debug)]
pub enum OutputError {
    Markdown(MarkdownError),
    Json(serde_json::Error),
}

impl fmt::Display for OutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputError::Markdown(e) => write!(f, "Markdown render error: {}", e),
            OutputError::Json(e) => write!(f, "JSON render error: {}", e),
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

/// Render a set of parsed files into a single string in the requested format.
///
/// Aggregation rules per format:
/// - Markdown: emits one compact `# Java Atlas` document with a `##` section per
///   package and file bullets underneath.
/// - JSON: emits a single array `[{ "path": ..., "ast": ... }, ...]`.
/// - JSONL: emits one compact file record per line, grouped by package when
///   written through [`render_jsonl_packages`].
pub fn render(files: &[FileOutput<'_>], format: Format) -> Result<String, OutputError> {
    match format {
        Format::Markdown => markdown::render(files),
        Format::Json => json::render(files),
        Format::Jsonl => jsonl::render(files),
    }
}

/// Render package-scoped JSONL shards for `.atlas/packages`.
pub fn render_jsonl_packages(files: &[FileOutput<'_>]) -> Result<Vec<JsonlPackage>, OutputError> {
    jsonl::render_packages(files)
}
