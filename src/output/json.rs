//! JSON rendering via serde_json.

use serde::Serialize;

use crate::java_ast::JavaFile;

use super::{FileOutput, OutputError};

/// Render the file list as a single pretty-printed JSON array.
pub(super) fn render(files: &[FileOutput<'_>]) -> Result<String, OutputError> {
    let entries: Vec<Entry<'_>> = files
        .iter()
        .map(|f| Entry {
            path: f.path.to_string_lossy().into_owned(),
            ast: f.ast,
        })
        .collect();
    Ok(serde_json::to_string_pretty(&entries)?)
}

#[derive(Serialize)]
struct Entry<'a> {
    path: String,
    ast: &'a JavaFile,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::java_ast::{parse_java_file, resolve_files};
    use crate::test_fixtures::java;

    use super::super::{FileOutput, Format, render as dispatch_render};

    #[test]
    fn json_round_trips_and_contains_expected_keys() {
        let ast = parse_java_file(java::output::ROUND_TRIP_FOO).expect("parse");
        let path = PathBuf::from("src/Foo.java");
        let files = vec![FileOutput {
            path: &path,
            ast: &ast,
        }];
        let json = dispatch_render(&files, Format::Json).expect("render");

        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse json");
        let arr = parsed.as_array().expect("array");
        assert_eq!(arr.len(), 1);
        let entry = &arr[0];
        assert_eq!(entry["path"], "src/Foo.java");
        assert_eq!(entry["ast"]["package"], "com.example");
        let types = entry["ast"]["types"].as_array().expect("types array");
        assert_eq!(types.len(), 1);
        assert_eq!(types[0]["kind"], "Class");
        assert_eq!(types[0]["name"], "Foo");
        let field = &types[0]["fields"][0];
        assert_eq!(field["name"], "x");
        assert_eq!(field["ty"]["kind"], "Primitive");
        assert_eq!(field["ty"]["value"], "int");
    }

    #[test]
    fn json_exposes_resolved_fqn_across_files() {
        let mut asts = java::output::RESOLVED_FQN
            .iter()
            .map(|source| parse_java_file(*source).expect("parse"))
            .collect::<Vec<_>>();
        resolve_files(&mut asts);

        let paths = [
            PathBuf::from("src/User.java"),
            PathBuf::from("src/Service.java"),
        ];
        let files: Vec<FileOutput<'_>> = paths
            .iter()
            .zip(asts.iter())
            .map(|(p, a)| FileOutput { path: p, ast: a })
            .collect();
        let json = dispatch_render(&files, Format::Json).expect("render");

        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse json");
        let service_field_ty = &parsed[1]["ast"]["types"][0]["fields"][0]["ty"]["value"];
        assert_eq!(service_field_ty["name"], "User");
        assert_eq!(service_field_ty["resolved_fqn"], "com.example.model.User");
    }

    #[test]
    fn json_exposes_javadocs() {
        let ast = parse_java_file(java::output::JAVADOC_USER_SERVICE).expect("parse");
        let path = PathBuf::from("src/UserService.java");
        let files = vec![FileOutput {
            path: &path,
            ast: &ast,
        }];
        let json = dispatch_render(&files, Format::Json).expect("render");

        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse json");
        let doc = &parsed[0]["ast"]["types"][0]["documentation"];
        assert_eq!(doc["description"], "User service.");
        assert_eq!(doc["tags"][0]["name"], "since");
        assert_eq!(doc["tags"][0]["text"], "1.0");
    }
}
