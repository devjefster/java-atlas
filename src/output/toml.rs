//! TOML rendering via the toml crate.

use std::collections::BTreeMap;
use std::path::{Component, Path};

use serde::Serialize;

use crate::java_ast::{JavaFile, JavaType};

use super::{FileOutput, OutputError, compact};

/// Render the file list as package-grouped compact TOML.
pub(super) fn render(files: &[FileOutput<'_>]) -> Result<String, OutputError> {
    let entries: Vec<Entry<'_>> = files
        .iter()
        .map(|f| Entry {
            path: f.path.to_string_lossy().into_owned(),
            ast: f.ast,
        })
        .collect();
    let document = Document { files: entries };
    let mut value = ::toml::Value::try_from(&document)?;
    compact_toml_members(files, &mut value);
    group_files_by_package(files, &mut value);
    prune_empty_arrays(&mut value);
    let rendered = ::toml::to_string_pretty(&value)?;
    Ok(inline_compact_arrays(&rendered))
}

fn prune_empty_arrays(value: &mut ::toml::Value) {
    match value {
        ::toml::Value::Array(values) => {
            for value in values {
                prune_empty_arrays(value);
            }
        }
        ::toml::Value::Table(table) => {
            table.retain(|_, value| {
                prune_empty_arrays(value);
                !matches!(value, ::toml::Value::Array(values) if values.is_empty())
            });
        }
        _ => {}
    }
}

fn compact_toml_members(files: &[FileOutput<'_>], value: &mut ::toml::Value) {
    let Some(rendered_files) = value.get_mut("files").and_then(::toml::Value::as_array_mut) else {
        return;
    };
    for (file, rendered_file) in files.iter().zip(rendered_files.iter_mut()) {
        let Some(types) = rendered_file
            .get_mut("ast")
            .and_then(|ast| ast.get_mut("types"))
            .and_then(::toml::Value::as_array_mut)
        else {
            continue;
        };
        compact_type_members(&file.ast.types, types);
    }
}

fn group_files_by_package(files: &[FileOutput<'_>], value: &mut ::toml::Value) {
    let Some(rendered_files) = value.get_mut("files").and_then(::toml::Value::as_array_mut) else {
        return;
    };

    let mut packages: BTreeMap<String, Vec<::toml::Value>> = BTreeMap::new();
    for (file, mut rendered_file) in files.iter().zip(std::mem::take(rendered_files)) {
        let package = package_name(file.ast);
        shorten_file_path(file, &package, &mut rendered_file);
        flatten_file_ast(&mut rendered_file);
        packages.entry(package).or_default().push(rendered_file);
    }

    let package_values = packages
        .into_iter()
        .map(|(name, files)| {
            let mut package = ::toml::map::Map::new();
            package.insert("name".to_string(), ::toml::Value::String(name));
            package.insert("files".to_string(), ::toml::Value::Array(files));
            ::toml::Value::Table(package)
        })
        .collect();

    if let Some(document) = value.as_table_mut() {
        document.remove("files");
        document.insert("packages".to_string(), ::toml::Value::Array(package_values));
    }
}

fn package_name(file: &JavaFile) -> String {
    file.package
        .clone()
        .filter(|package| !package.is_empty())
        .unwrap_or_else(|| "_default_".to_string())
}

fn shorten_file_path(file: &FileOutput<'_>, package: &str, rendered_file: &mut ::toml::Value) {
    let path = if package == "_default_" {
        file_name(file.path)
    } else {
        path_relative_to_package(file.path, package).unwrap_or_else(|| file_name(file.path))
    };
    if let Some(table) = rendered_file.as_table_mut() {
        table.insert("path".to_string(), ::toml::Value::String(path));
    }
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

fn flatten_file_ast(rendered_file: &mut ::toml::Value) {
    let Some(file_table) = rendered_file.as_table_mut() else {
        return;
    };
    let Some(mut ast) = file_table
        .remove("ast")
        .and_then(|ast| ast.as_table().cloned())
    else {
        return;
    };
    ast.remove("package");
    for (key, value) in ast {
        file_table.insert(key, value);
    }
}

fn compact_type_members(types: &[JavaType], rendered_types: &mut [::toml::Value]) {
    for (ty, rendered_ty) in types.iter().zip(rendered_types.iter_mut()) {
        add_field_accessors(ty, rendered_ty);
        remove_boring_constructors(ty, rendered_ty);
        remove_compacted_methods(ty, rendered_ty);
        compact_nested_types(ty, rendered_ty);
    }
}

fn add_field_accessors(ty: &JavaType, rendered_ty: &mut ::toml::Value) {
    let Some(fields) = rendered_ty
        .get_mut("fields")
        .and_then(::toml::Value::as_array_mut)
    else {
        return;
    };
    for (field, rendered_field) in ty.fields.iter().zip(fields.iter_mut()) {
        let labels = compact::field_accessor_labels(ty, field);
        if labels.is_empty() {
            continue;
        }
        let accessors = labels
            .into_iter()
            .map(|label| ::toml::Value::String(label.to_string()))
            .collect();
        if let Some(table) = rendered_field.as_table_mut() {
            table.insert("accessors".to_string(), ::toml::Value::Array(accessors));
        }
    }
}

fn remove_boring_constructors(ty: &JavaType, rendered_ty: &mut ::toml::Value) {
    let Some(constructors) = rendered_ty
        .get_mut("constructors")
        .and_then(::toml::Value::as_array_mut)
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

fn remove_compacted_methods(ty: &JavaType, rendered_ty: &mut ::toml::Value) {
    let Some(methods) = rendered_ty
        .get_mut("methods")
        .and_then(::toml::Value::as_array_mut)
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

fn compact_nested_types(ty: &JavaType, rendered_ty: &mut ::toml::Value) {
    let Some(nested_types) = rendered_ty
        .get_mut("nested_types")
        .and_then(::toml::Value::as_array_mut)
    else {
        return;
    };
    compact_type_members(&ty.nested_types, nested_types);
}

fn inline_compact_arrays(toml: &str) -> String {
    let lines = toml.lines().collect::<Vec<_>>();
    let mut out = String::with_capacity(toml.len());
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if (line.ends_with("range = [") || line.ends_with("body_range = ["))
            && index + 3 < lines.len()
            && lines[index + 3].trim() == "]"
        {
            let Some(first) = range_number(lines[index + 1]) else {
                push_line(&mut out, line);
                index += 1;
                continue;
            };
            let Some(second) = range_number(lines[index + 2]) else {
                push_line(&mut out, line);
                index += 1;
                continue;
            };
            let key = line.trim_end_matches(" [");
            push_line(&mut out, &format!("{key} [{first}, {second}]"));
            index += 4;
            continue;
        }
        if line.ends_with("accessors = [")
            && let Some((labels, next_index)) = accessor_labels(&lines, index)
        {
            let key = line.trim_end_matches(" [");
            push_line(&mut out, &format!("{key} [{}]", labels.join(", ")));
            index = next_index;
            continue;
        }

        push_line(&mut out, line);
        index += 1;
    }
    out
}

fn accessor_labels(lines: &[&str], start: usize) -> Option<(Vec<String>, usize)> {
    let mut labels = Vec::new();
    let mut index = start + 1;
    while index < lines.len() {
        let line = lines[index].trim();
        if line == "]" {
            return Some((labels, index + 1));
        }
        let label = line.trim_end_matches(',');
        if !(label.starts_with('"') && label.ends_with('"')) {
            return None;
        }
        labels.push(label.to_string());
        index += 1;
    }
    None
}

fn range_number(line: &str) -> Option<&str> {
    line.trim().trim_end_matches(',').parse::<usize>().ok()?;
    Some(line.trim().trim_end_matches(','))
}

fn push_line(out: &mut String, line: &str) {
    out.push_str(line);
    out.push('\n');
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

    use crate::java_ast::{parse_java_file, resolve_files};
    use crate::test_fixtures::java;

    use super::super::{FileOutput, Format, render as dispatch_render};

    #[test]
    fn toml_round_trips_and_contains_expected_keys() {
        let ast = parse_java_file(java::output::ROUND_TRIP_FOO).expect("parse");
        let path = PathBuf::from("src/main/java/com/example/Foo.java");
        let files = vec![FileOutput {
            path: &path,
            ast: &ast,
        }];
        let rendered = dispatch_render(&files, Format::Toml).expect("render");

        let parsed: ::toml::Value = ::toml::from_str(&rendered).expect("parse toml");
        assert!(parsed.get("files").is_none());
        let packages = parsed["packages"].as_array().expect("packages array");
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0]["name"].as_str(), Some("com.example"));
        let package_files = packages[0]["files"].as_array().expect("package files");
        assert_eq!(package_files.len(), 1);
        assert_eq!(package_files[0]["path"].as_str(), Some("Foo.java"));
        assert!(package_files[0].get("ast").is_none());
        assert!(package_files[0].get("package").is_none());
        let types = package_files[0]["types"].as_array().expect("types array");
        assert_eq!(types.len(), 1);
        assert_eq!(types[0]["kind"].as_str(), Some("Class"));
        assert_eq!(types[0]["name"].as_str(), Some("Foo"));
    }

    #[test]
    fn toml_exposes_resolved_fqn_across_files() {
        let mut asts = java::output::RESOLVED_FQN
            .iter()
            .map(|source| parse_java_file(source).expect("parse"))
            .collect::<Vec<_>>();
        resolve_files(&mut asts);

        let paths = [
            PathBuf::from("src/main/java/com/example/model/User.java"),
            PathBuf::from("src/main/java/com/example/service/Service.java"),
        ];
        let files: Vec<FileOutput<'_>> = paths
            .iter()
            .zip(asts.iter())
            .map(|(p, a)| FileOutput { path: p, ast: a })
            .collect();
        let rendered = dispatch_render(&files, Format::Toml).expect("render");

        let parsed: ::toml::Value = ::toml::from_str(&rendered).expect("parse toml");
        let packages = parsed["packages"].as_array().expect("packages array");
        assert_eq!(packages[0]["name"].as_str(), Some("com.example.model"));
        assert_eq!(packages[1]["name"].as_str(), Some("com.example.service"));
        let service_field_ty = &packages[1]["files"][0]["types"][0]["fields"][0]["ty"]["value"];
        assert_eq!(service_field_ty["name"].as_str(), Some("User"));
        assert_eq!(
            service_field_ty["resolved_fqn"].as_str(),
            Some("com.example.model.User")
        );
    }

    #[test]
    fn toml_exposes_javadocs() {
        let ast = parse_java_file(java::output::JAVADOC_USER_SERVICE).expect("parse");
        let path = PathBuf::from("src/UserService.java");
        let files = vec![FileOutput {
            path: &path,
            ast: &ast,
        }];
        let rendered = dispatch_render(&files, Format::Toml).expect("render");

        let parsed: ::toml::Value = ::toml::from_str(&rendered).expect("parse toml");
        let doc = &parsed["packages"][0]["files"][0]["types"][0]["documentation"];
        assert_eq!(doc["description"].as_str(), Some("User service."));
        assert_eq!(doc["tags"][0]["name"].as_str(), Some("since"));
        assert_eq!(doc["tags"][0]["text"].as_str(), Some("1.0"));
    }

    #[test]
    fn toml_omits_empty_arrays() {
        let ast = parse_java_file(java::output::TOML_EMPTY_SHAPE).expect("parse");
        let path = PathBuf::from("src/EmptyShape.java");
        let files = vec![FileOutput {
            path: &path,
            ast: &ast,
        }];
        let rendered = dispatch_render(&files, Format::Toml).expect("render");

        assert!(!rendered.contains("imports = []"));
        assert!(!rendered.contains("annotations = []"));
        assert!(!rendered.contains("type_parameters = []"));
        assert!(!rendered.contains("extends = []"));
        assert!(!rendered.contains("implements = []"));
        assert!(!rendered.contains("constructors = []"));
        assert!(!rendered.contains("enum_constants = []"));
        assert!(!rendered.contains("record_components = []"));
        assert!(!rendered.contains("annotation_elements = []"));
        assert!(!rendered.contains("nested_types = []"));

        let parsed: ::toml::Value = ::toml::from_str(&rendered).expect("parse toml");
        let ty = &parsed["packages"][0]["files"][0]["types"][0];
        assert_eq!(ty["kind"].as_str(), Some("Class"));
        assert_eq!(ty["name"].as_str(), Some("EmptyShape"));
        assert!(ty.get("constructors").is_none());
        assert!(ty.get("nested_types").is_none());
    }

    #[test]
    fn toml_keeps_non_empty_arrays() {
        let ast = parse_java_file(java::output::TOML_NON_EMPTY_SHAPE).expect("parse");
        let path = PathBuf::from("src/NonEmptyShape.java");
        let files = vec![FileOutput {
            path: &path,
            ast: &ast,
        }];
        let rendered = dispatch_render(&files, Format::Toml).expect("render");

        let parsed: ::toml::Value = ::toml::from_str(&rendered).expect("parse toml");
        let entry = &parsed["packages"][0]["files"][0];
        assert_eq!(entry["imports"].as_array().map(Vec::len), Some(1));
        let ty = &entry["types"][0];
        assert_eq!(ty["annotations"].as_array().map(Vec::len), Some(1));
        assert_eq!(ty["modifiers"].as_array().map(Vec::len), Some(1));
        assert_eq!(ty["type_parameters"].as_array().map(Vec::len), Some(1));
        assert_eq!(ty["extends"].as_array().map(Vec::len), Some(1));
        assert_eq!(ty["implements"].as_array().map(Vec::len), Some(1));
        assert_eq!(ty["fields"].as_array().map(Vec::len), Some(1));
        assert_eq!(ty["methods"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn toml_compacts_standard_field_accessors() {
        let ast = parse_java_file(java::output::MARKDOWN_STANDARD_FIELD_ACCESSORS).expect("parse");
        let path = PathBuf::from("src/UserSendCodeRequest.java");
        let files = vec![FileOutput {
            path: &path,
            ast: &ast,
        }];
        let rendered = dispatch_render(&files, Format::Toml).expect("render");

        let parsed: ::toml::Value = ::toml::from_str(&rendered).expect("parse toml");
        let ty = &parsed["packages"][0]["files"][0]["types"][0];
        let fields = ty["fields"].as_array().expect("fields");
        assert_eq!(fields[0]["accessors"].as_array().map(Vec::len), Some(2));
        assert_eq!(fields[0]["accessors"][0].as_str(), Some("getter"));
        assert_eq!(fields[0]["accessors"][1].as_str(), Some("setter"));
        assert_eq!(fields[1]["accessors"].as_array().map(Vec::len), Some(2));
        assert_eq!(fields[2]["accessors"].as_array().map(Vec::len), Some(1));
        assert!(ty.get("methods").is_none());
        assert!(rendered.contains("accessors = [\"getter\", \"setter\"]"));
        assert!(!rendered.contains("accessors = [\n"));
    }

    #[test]
    fn toml_omits_only_boring_no_arg_constructors() {
        let ast =
            parse_java_file(java::output::MARKDOWN_BORING_NO_ARG_CONSTRUCTORS).expect("parse");
        let path = PathBuf::from("src/PlainDto.java");
        let files = vec![FileOutput {
            path: &path,
            ast: &ast,
        }];
        let rendered = dispatch_render(&files, Format::Toml).expect("render");

        let parsed: ::toml::Value = ::toml::from_str(&rendered).expect("parse toml");
        let plain = &parsed["packages"][0]["files"][0]["types"][0];
        let plain_constructors = plain["constructors"].as_array().expect("constructors");
        assert_eq!(plain_constructors.len(), 1);
        assert_eq!(
            plain_constructors[0]["args"].as_array().map(Vec::len),
            Some(1)
        );

        let configured = &parsed["packages"][0]["files"][0]["types"][1];
        let configured_constructors = configured["constructors"]
            .as_array()
            .expect("configured constructors");
        assert_eq!(configured_constructors.len(), 1);
        assert_eq!(
            configured_constructors[0]["annotations"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn toml_renders_ranges_inline() {
        let ast = parse_java_file(java::output::ROUND_TRIP_FOO).expect("parse");
        let path = PathBuf::from("src/Foo.java");
        let files = vec![FileOutput {
            path: &path,
            ast: &ast,
        }];
        let rendered = dispatch_render(&files, Format::Toml).expect("render");

        assert!(rendered.contains("range = ["));
        assert!(!rendered.contains("range = [\n"));
        assert!(!rendered.contains("body_range = [\n"));
        ::toml::from_str::<::toml::Value>(&rendered).expect("parse toml");
    }

    #[test]
    fn toml_groups_files_by_package_and_shortens_paths() {
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
        let rendered = dispatch_render(&files, Format::Toml).expect("render");

        let parsed: ::toml::Value = ::toml::from_str(&rendered).expect("parse toml");
        let packages = parsed["packages"].as_array().expect("packages array");
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0]["name"].as_str(), Some("_default_"));
        assert_eq!(
            packages[0]["files"][0]["path"].as_str(),
            Some("DefaultPackage.java")
        );
        assert_eq!(packages[1]["name"].as_str(), Some("a"));
        assert_eq!(packages[1]["files"][0]["path"].as_str(), Some("A.java"));
        assert_eq!(
            packages[1]["files"][1]["path"].as_str(),
            Some("internal/Nested.java")
        );
    }
}
