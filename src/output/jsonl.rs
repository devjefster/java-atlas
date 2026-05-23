//! Compact JSON Lines rendering for searchable package shards.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::java_ast::{JavaFile, JavaType};

use super::{FileOutput, JsonlPackage, OutputError, compact};

/// Render all files as compact JSONL in deterministic package order.
pub(super) fn render(files: &[FileOutput<'_>]) -> Result<String, OutputError> {
    let mut out = String::new();
    for package in render_packages(files)? {
        out.push_str(&package.contents);
    }
    Ok(out)
}

/// Render files into one JSONL artifact per Java package.
pub(super) fn render_packages(files: &[FileOutput<'_>]) -> Result<Vec<JsonlPackage>, OutputError> {
    let mut packages: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in files {
        let package = package_name(file.ast);
        let line = render_file_line(file, &package)?;
        packages.entry(package).or_default().push(line);
    }

    Ok(packages
        .into_iter()
        .map(|(package, lines)| JsonlPackage {
            relative_path: package_jsonl_path(&package),
            package,
            contents: lines.join(""),
        })
        .collect())
}

fn render_file_line(file: &FileOutput<'_>, package: &str) -> Result<String, OutputError> {
    let entry = Entry {
        path: short_file_path(file.path, package),
        package,
        imports: &file.ast.imports,
        types: &file.ast.types,
    };
    let mut value = serde_json::to_value(&entry)?;
    compact_file_value(&file.ast.types, &mut value);
    prune_empty_values(&mut value);
    Ok(format!("{}\n", serde_json::to_string(&value)?))
}

fn compact_file_value(types: &[JavaType], value: &mut serde_json::Value) {
    let Some(rendered_types) = value
        .get_mut("types")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    compact_type_members(types, rendered_types);
}

fn compact_type_members(types: &[JavaType], rendered_types: &mut [serde_json::Value]) {
    for (ty, rendered_ty) in types.iter().zip(rendered_types.iter_mut()) {
        add_field_accessors(ty, rendered_ty);
        remove_boring_constructors(ty, rendered_ty);
        remove_compacted_methods(ty, rendered_ty);
        compact_nested_types(ty, rendered_ty);
    }
}

fn add_field_accessors(ty: &JavaType, rendered_ty: &mut serde_json::Value) {
    let Some(fields) = rendered_ty
        .get_mut("fields")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    for (field, rendered_field) in ty.fields.iter().zip(fields.iter_mut()) {
        let labels = compact::field_accessor_labels(ty, field);
        if labels.is_empty() {
            continue;
        }
        if let Some(object) = rendered_field.as_object_mut() {
            object.insert(
                "accessors".to_string(),
                serde_json::Value::Array(
                    labels
                        .into_iter()
                        .map(|label| serde_json::Value::String(label.to_string()))
                        .collect(),
                ),
            );
        }
    }
}

fn remove_boring_constructors(ty: &JavaType, rendered_ty: &mut serde_json::Value) {
    let Some(constructors) = rendered_ty
        .get_mut("constructors")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    let mut rendered_iter = std::mem::take(constructors).into_iter();
    *constructors = ty
        .constructors
        .iter()
        .filter_map(|constructor| {
            let rendered_constructor = rendered_iter.next()?;
            (!compact::is_boring_no_arg_constructor(constructor)).then_some(rendered_constructor)
        })
        .collect();
}

fn remove_compacted_methods(ty: &JavaType, rendered_ty: &mut serde_json::Value) {
    let Some(methods) = rendered_ty
        .get_mut("methods")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    let mut rendered_iter = std::mem::take(methods).into_iter();
    *methods = ty
        .methods
        .iter()
        .filter_map(|method| {
            let rendered_method = rendered_iter.next()?;
            (!compact::is_compacted_accessor(method, ty)).then_some(rendered_method)
        })
        .collect();
}

fn compact_nested_types(ty: &JavaType, rendered_ty: &mut serde_json::Value) {
    let Some(nested_types) = rendered_ty
        .get_mut("nested_types")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    compact_type_members(&ty.nested_types, nested_types);
}

fn prune_empty_values(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                prune_empty_values(value);
            }
        }
        serde_json::Value::Object(object) => {
            object.retain(|_, value| {
                prune_empty_values(value);
                !matches!(value, serde_json::Value::Array(values) if values.is_empty())
                    && !value.is_null()
            });
        }
        _ => {}
    }
}

fn package_name(file: &JavaFile) -> String {
    file.package
        .clone()
        .filter(|package| !package.is_empty())
        .unwrap_or_else(|| "_default_".to_string())
}

fn package_jsonl_path(package: &str) -> PathBuf {
    if package == "_default_" {
        return PathBuf::from("_default_.jsonl");
    }
    package
        .split('.')
        .collect::<PathBuf>()
        .with_extension("jsonl")
}

fn short_file_path(path: &Path, package: &str) -> String {
    if package == "_default_" {
        return file_name(path);
    }
    path_relative_to_package(path, package).unwrap_or_else(|| file_name(path))
}

fn path_relative_to_package(path: &Path, package: &str) -> Option<String> {
    let path_parts = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().replace('\\', "/")),
            _ => None,
        })
        .collect::<Vec<_>>();
    let package_parts = package.split('.').collect::<Vec<_>>();
    let package_len = package_parts.len();
    if package_len == 0 || path_parts.len() <= package_len {
        return None;
    }

    path_parts
        .windows(package_len)
        .position(|window| {
            window
                .iter()
                .map(String::as_str)
                .eq(package_parts.iter().copied())
        })
        .and_then(|index| {
            let relative = &path_parts[index + package_len..];
            (!relative.is_empty()).then(|| relative.join("/"))
        })
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| path.as_os_str().to_string_lossy().replace('\\', "/"))
}

#[derive(Serialize)]
struct Entry<'a> {
    path: String,
    package: &'a str,
    imports: &'a [String],
    types: &'a [JavaType],
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::java_ast::{parse_java_file, resolve_files};
    use crate::test_fixtures::java;

    use super::super::{FileOutput, Format, render as dispatch_render, render_jsonl_packages};

    #[test]
    fn jsonl_renders_one_compact_file_record_per_line() {
        let ast = parse_java_file(java::output::ROUND_TRIP_FOO).expect("parse");
        let path = PathBuf::from("src/main/java/com/example/Foo.java");
        let files = vec![FileOutput {
            path: &path,
            ast: &ast,
        }];

        let rendered = dispatch_render(&files, Format::Jsonl).expect("render");

        let lines = rendered.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 1);
        let parsed: serde_json::Value = serde_json::from_str(lines[0]).expect("parse jsonl line");
        assert_eq!(parsed["path"], "Foo.java");
        assert_eq!(parsed["package"], "com.example");
        assert!(parsed.get("imports").is_none());
        assert!(!lines[0].contains(":null"));
        assert_eq!(parsed["types"][0]["kind"], "Class");
        assert_eq!(parsed["types"][0]["name"], "Foo");
    }

    #[test]
    fn jsonl_packages_are_grouped_and_path_shortened() {
        let ast_a = parse_java_file(java::output::MARKDOWN_MULTI_FILE_A).expect("parse");
        let ast_b = parse_java_file("package a; public class Nested {}").expect("parse");
        let ast_default = parse_java_file("public class DefaultPackage {}").expect("parse");
        let paths = [
            PathBuf::from("/repo/src/main/java/a/A.java"),
            PathBuf::from("/repo/src/main/java/a/internal/Nested.java"),
            PathBuf::from("/repo/src/main/java/DefaultPackage.java"),
        ];
        let files = vec![
            FileOutput {
                path: &paths[0],
                ast: &ast_a,
            },
            FileOutput {
                path: &paths[1],
                ast: &ast_b,
            },
            FileOutput {
                path: &paths[2],
                ast: &ast_default,
            },
        ];

        let packages = render_jsonl_packages(&files).expect("render packages");

        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].package, "_default_");
        assert_eq!(packages[0].relative_path, PathBuf::from("_default_.jsonl"));
        assert_eq!(packages[1].package, "a");
        assert_eq!(packages[1].relative_path, PathBuf::from("a.jsonl"));
        assert_eq!(packages[1].contents.lines().count(), 2);
        assert!(packages[1].contents.contains("\"path\":\"A.java\""));
        assert!(
            packages[1]
                .contents
                .contains("\"path\":\"internal/Nested.java\"")
        );
    }

    #[test]
    fn jsonl_compacts_accessors_and_boring_constructors() {
        let ast = parse_java_file(java::output::MARKDOWN_STANDARD_FIELD_ACCESSORS).expect("parse");
        let path = PathBuf::from("src/main/java/com/example/UserSendCodeRequest.java");
        let files = vec![FileOutput {
            path: &path,
            ast: &ast,
        }];

        let rendered = dispatch_render(&files, Format::Jsonl).expect("render");
        let parsed: serde_json::Value =
            serde_json::from_str(rendered.lines().next().expect("line")).expect("parse line");
        let ty = &parsed["types"][0];

        assert_eq!(ty["fields"][0]["accessors"][0], "getter");
        assert_eq!(ty["fields"][0]["accessors"][1], "setter");
        assert!(ty.get("methods").is_none());
    }

    #[test]
    fn jsonl_exposes_resolved_fqn_across_files() {
        let mut asts = java::output::RESOLVED_FQN
            .iter()
            .map(|source| parse_java_file(source).expect("parse"))
            .collect::<Vec<_>>();
        resolve_files(&mut asts);
        let paths = [
            PathBuf::from("src/main/java/com/example/model/User.java"),
            PathBuf::from("src/main/java/com/example/service/Service.java"),
        ];
        let files = paths
            .iter()
            .zip(asts.iter())
            .map(|(path, ast)| FileOutput {
                path: path.as_path(),
                ast,
            })
            .collect::<Vec<_>>();

        let packages = render_jsonl_packages(&files).expect("render packages");
        let service = packages
            .iter()
            .find(|package| package.package == "com.example.service")
            .expect("service package");
        let parsed: serde_json::Value =
            serde_json::from_str(service.contents.lines().next().expect("line"))
                .expect("parse line");

        assert_eq!(
            parsed["types"][0]["fields"][0]["ty"]["value"]["resolved_fqn"],
            "com.example.model.User"
        );
    }
}
