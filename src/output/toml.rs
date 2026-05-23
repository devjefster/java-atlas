//! TOML rendering via the toml crate.

use serde::Serialize;

use crate::java_ast::JavaFile;

use super::{FileOutput, OutputError};

/// Render the file list as a single TOML document with a `[[files]]` table array.
pub(super) fn render(files: &[FileOutput<'_>]) -> Result<String, OutputError> {
    let entries: Vec<Entry<'_>> = files
        .iter()
        .map(|f| Entry {
            path: f.path.to_string_lossy().into_owned(),
            ast: f.ast,
        })
        .collect();
    let document = Document { files: entries };
    Ok(::toml::to_string_pretty(&document)?)
}

#[derive(Serialize)]
struct Document<'a> {
    files: Vec<Entry<'a>>,
}

#[derive(Serialize)]
struct Entry<'a> {
    path: String,
    ast: &'a JavaFile,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::java_ast::parse_java_file;

    use super::super::{FileOutput, Format, render as dispatch_render};

    #[test]
    fn toml_round_trips_and_contains_expected_keys() {
        let source = r#"
            package com.example;
            public class Foo {
                private int x;
            }
        "#;

        let ast = parse_java_file(source).expect("parse");
        let path = PathBuf::from("src/Foo.java");
        let files = vec![FileOutput {
            path: &path,
            ast: &ast,
        }];
        let rendered = dispatch_render(&files, Format::Toml).expect("render");

        let parsed: ::toml::Value = ::toml::from_str(&rendered).expect("parse toml");
        let files_arr = parsed["files"].as_array().expect("files array");
        assert_eq!(files_arr.len(), 1);
        let entry = &files_arr[0];
        assert_eq!(entry["path"].as_str(), Some("src/Foo.java"));
        assert_eq!(entry["ast"]["package"].as_str(), Some("com.example"));
        let types = entry["ast"]["types"].as_array().expect("types array");
        assert_eq!(types.len(), 1);
        assert_eq!(types[0]["kind"].as_str(), Some("Class"));
        assert_eq!(types[0]["name"].as_str(), Some("Foo"));
    }
}
