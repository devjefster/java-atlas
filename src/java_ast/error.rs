//! Error types for Java parsing and rendering.

use std::fmt;

#[derive(Debug)]
pub enum JavaAstError {
    LanguageSetup(String),
    MarkdownInvalid(String),
    ParseFailed,
    QueryFailed(String),
}

impl fmt::Display for JavaAstError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JavaAstError::LanguageSetup(s) => write!(f, "Language setup error: {}", s),
            JavaAstError::MarkdownInvalid(s) => write!(f, "Markdown validation failed: {}", s),
            JavaAstError::ParseFailed => write!(f, "Failed to parse Java source"),
            JavaAstError::QueryFailed(s) => write!(f, "Query failed: {}", s),
        }
    }
}

impl std::error::Error for JavaAstError {}
