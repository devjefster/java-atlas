//! Markdown rendering for parsed Java models.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use crate::java_ast::{
    JavaAnnotation, JavaAnnotationElement, JavaArgument, JavaConstructor, JavaDoc, JavaField,
    JavaFile, JavaMethod, JavaType, JavaTypeParameter, TypeKind,
};
use crate::markdown::validate_markdown;

use super::{FileOutput, OutputError};

/// Render the whole codebase as a single Markdown document.
///
/// One `# Java Atlas` heading at the top, then one compact `##` section per
/// package. Files are listed under their package to avoid repeating package
/// paths across large generated documents.
pub(super) fn render(files: &[FileOutput<'_>]) -> Result<String, OutputError> {
    let mut out = String::from("# Java Atlas\n\n");
    for (package, package_files) in group_by_package(files) {
        out.push_str(&format!("## `{}`\n\n", package));
        for file in package_files {
            out.push_str(&format!("- `{}`\n", file_name(file.path)));
            render_file_body(file.ast, &mut out, 1);
        }
        out.push('\n');
    }
    validate_markdown(&out)?;
    Ok(out)
}

/// Append one file's compact type summaries to the shared buffer.
fn render_file_body(file: &JavaFile, out: &mut String, level: usize) {
    for ty in &file.types {
        render_type(ty, level, out);
    }
}

/// Render one Java type and recursively include nested types as compact bullets.
fn render_type(ty: &JavaType, level: usize, out: &mut String) {
    let indent = indent(level);
    let accessors = field_accessors(ty);
    out.push_str(&format!(
        "{}- {}: `{}`{}\n",
        indent,
        type_label(ty.kind),
        type_signature(ty),
        format_doc_suffix(&ty.documentation)
    ));

    if !ty.record_components.is_empty() {
        render_compact_section(
            "components",
            ty.record_components
                .iter()
                .map(|comp| format!("`{}`", format_arg(comp)))
                .collect(),
            level + 1,
            out,
        );
    }
    if !ty.fields.is_empty() {
        render_compact_section(
            "fields",
            ty.fields
                .iter()
                .map(|field| format_field(field, accessors_for_field(field, &accessors)))
                .collect(),
            level + 1,
            out,
        );
    }
    if !ty.constructors.is_empty() {
        render_compact_section(
            "constructors",
            ty.constructors
                .iter()
                .filter(|cons| !is_boring_no_arg_constructor(cons))
                .map(format_constructor)
                .collect(),
            level + 1,
            out,
        );
    }
    if !ty.methods.is_empty() {
        render_compact_section(
            "methods",
            ty.methods
                .iter()
                .filter(|method| !is_compacted_accessor(method, ty))
                .map(format_method)
                .collect(),
            level + 1,
            out,
        );
    }
    if !ty.enum_constants.is_empty() {
        render_compact_section(
            "constants",
            ty.enum_constants
                .iter()
                .map(|constant| format!("`{constant}`"))
                .collect(),
            level + 1,
            out,
        );
    }
    if !ty.annotation_elements.is_empty() {
        render_compact_section(
            "elements",
            ty.annotation_elements
                .iter()
                .map(format_annotation_element)
                .collect(),
            level + 1,
            out,
        );
    }
    for nested in &ty.nested_types {
        render_type(nested, level + 1, out);
    }
}

fn group_by_package<'a>(files: &'a [FileOutput<'a>]) -> BTreeMap<String, Vec<&'a FileOutput<'a>>> {
    let mut groups: BTreeMap<String, Vec<&'a FileOutput<'a>>> = BTreeMap::new();
    for file in files {
        groups.entry(package_name(file.ast)).or_default().push(file);
    }
    groups
}

fn package_name(file: &JavaFile) -> String {
    file.package
        .clone()
        .filter(|pkg| !pkg.is_empty())
        .unwrap_or_else(|| "_default_".to_string())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| path.as_os_str().to_string_lossy().replace('\\', "/"))
}

fn render_compact_section(label: &str, items: Vec<String>, level: usize, out: &mut String) {
    if items.is_empty() {
        return;
    }
    out.push_str(&format!(
        "{}- {}: {}\n",
        indent(level),
        label,
        items.join(", ")
    ));
}

fn type_signature(ty: &JavaType) -> String {
    let mut parts = Vec::new();
    if !ty.annotations.is_empty() {
        parts.push(join_display(&ty.annotations, " "));
    }
    if !ty.modifiers.is_empty() {
        parts.push(ty.modifiers.join(" "));
    }
    let mut name = ty.name.clone();
    if !ty.type_parameters.is_empty() {
        name.push_str(&format!("<{}>", join_display(&ty.type_parameters, ", ")));
    }
    parts.push(name);
    if !ty.extends.is_empty() {
        parts.push(format!("extends {}", join_display(&ty.extends, ", ")));
    }
    if !ty.implements.is_empty() {
        parts.push(format!("implements {}", join_display(&ty.implements, ", ")));
    }
    parts.join(" ")
}

fn format_field(field: &JavaField, accessors: FieldAccessors) -> String {
    format!(
        "`{}{} {}{}`{}",
        prefix(&field.annotations, &field.modifiers, &[]),
        field.ty,
        field.name,
        accessors_suffix(accessors),
        doc_suffix_text(&field.documentation)
    )
}

fn format_constructor(cons: &JavaConstructor) -> String {
    format!(
        "`{}{}({}){}`{}",
        prefix(&cons.annotations, &cons.modifiers, &cons.type_parameters),
        cons.name,
        format_args(&cons.args),
        throws_suffix(&cons.throws),
        doc_suffix_text(&cons.documentation)
    )
}

fn format_method(method: &JavaMethod) -> String {
    let return_type = method
        .return_type
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "void".to_string());
    format!(
        "`{}{} {}({}){}`{}",
        prefix(
            &method.annotations,
            &method.modifiers,
            &method.type_parameters
        ),
        return_type,
        method.name,
        format_args(&method.args),
        throws_suffix(&method.throws),
        doc_suffix_text(&method.documentation)
    )
}

fn format_annotation_element(elem: &JavaAnnotationElement) -> String {
    let mut rendered = format!(
        "`{}{} {}()",
        prefix(&elem.annotations, &[], &[]),
        elem.return_type,
        elem.name
    );
    if let Some(default_value) = &elem.default_value {
        rendered.push_str(&format!(" default {default_value}"));
    }
    rendered.push('`');
    rendered.push_str(&doc_suffix_text(&elem.documentation));
    rendered
}

fn prefix(
    annotations: &[JavaAnnotation],
    modifiers: &[String],
    type_parameters: &[JavaTypeParameter],
) -> String {
    let mut parts = Vec::new();
    if !annotations.is_empty() {
        parts.push(join_display(annotations, " "));
    }
    if !modifiers.is_empty() {
        parts.push(join_display(modifiers, " "));
    }
    if !type_parameters.is_empty() {
        parts.push(format!("<{}>", join_display(type_parameters, ", ")));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("{} ", parts.join(" "))
    }
}

fn throws_suffix<T: fmt::Display>(throws: &[T]) -> String {
    if throws.is_empty() {
        String::new()
    } else {
        format!(" throws {}", join_display(throws, ", "))
    }
}

fn format_doc_suffix(doc: &Option<JavaDoc>) -> String {
    doc_summary(doc)
        .map(|summary| format!(" - {}", summary))
        .unwrap_or_default()
}

fn doc_suffix_text(doc: &Option<JavaDoc>) -> String {
    doc_summary(doc)
        .map(|summary| format!(" - {summary}"))
        .unwrap_or_default()
}

fn doc_summary(doc: &Option<JavaDoc>) -> Option<String> {
    let summary = doc
        .as_ref()?
        .description
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!summary.is_empty()).then_some(summary)
}

fn indent(level: usize) -> String {
    "  ".repeat(level)
}

fn type_label(kind: TypeKind) -> &'static str {
    match kind {
        TypeKind::Class => "class",
        TypeKind::Interface => "interface",
        TypeKind::Enum => "enum",
        TypeKind::Record => "record",
        TypeKind::Annotation => "annotation",
    }
}

fn format_args(args: &[JavaArgument]) -> String {
    args.iter().map(format_arg).collect::<Vec<_>>().join(", ")
}

fn format_arg(arg: &JavaArgument) -> String {
    let prefix = if arg.annotations.is_empty() {
        String::new()
    } else {
        format!("{} ", join_display(&arg.annotations, " "))
    };
    if arg.varargs {
        format!("{}{}... {}", prefix, arg.ty, arg.name)
    } else {
        format!("{}{} {}", prefix, arg.ty, arg.name)
    }
}

fn join_display<T: fmt::Display>(items: &[T], separator: &str) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(separator)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FieldAccessors {
    getter: bool,
    setter: bool,
    add: bool,
}

fn field_accessors(ty: &JavaType) -> Vec<(&str, FieldAccessors)> {
    ty.fields
        .iter()
        .map(|field| {
            let mut accessors = FieldAccessors::default();
            for method in &ty.methods {
                match classify_accessor(method, field, &ty.name) {
                    Some(AccessorKind::Getter) => accessors.getter = true,
                    Some(AccessorKind::Setter) => accessors.setter = true,
                    Some(AccessorKind::Add) => accessors.add = true,
                    None => {}
                }
            }
            (field.name.as_str(), accessors)
        })
        .collect()
}

fn accessors_for_field(field: &JavaField, accessors: &[(&str, FieldAccessors)]) -> FieldAccessors {
    accessors
        .iter()
        .find_map(|(name, accessors)| (*name == field.name).then_some(*accessors))
        .unwrap_or_default()
}

fn accessors_suffix(accessors: FieldAccessors) -> String {
    let mut labels = Vec::new();
    if accessors.getter {
        labels.push("getter");
    }
    if accessors.setter {
        labels.push("setter");
    }
    if accessors.add {
        labels.push("add");
    }
    if labels.is_empty() {
        String::new()
    } else {
        format!(" [{}]", labels.join(","))
    }
}

fn is_compacted_accessor(method: &JavaMethod, ty: &JavaType) -> bool {
    ty.fields
        .iter()
        .any(|field| classify_accessor(method, field, &ty.name).is_some())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccessorKind {
    Getter,
    Setter,
    Add,
}

fn classify_accessor(
    method: &JavaMethod,
    field: &JavaField,
    declaring_type: &str,
) -> Option<AccessorKind> {
    if !is_plain_method(method) {
        return None;
    }

    let property = property_suffix(&field.name);
    let field_ty = field.ty.to_string();

    if method.name == format!("get{property}")
        && method.args.is_empty()
        && method
            .return_type
            .as_ref()
            .is_some_and(|return_type| return_type.to_string() == field_ty)
    {
        return Some(AccessorKind::Getter);
    }

    if method.name == format!("is{property}")
        && method.args.is_empty()
        && is_boolean_type(&field_ty)
        && method
            .return_type
            .as_ref()
            .is_some_and(|return_type| is_boolean_type(&return_type.to_string()))
    {
        return Some(AccessorKind::Getter);
    }

    if method.name == format!("set{property}")
        && method.args.len() == 1
        && method.args[0].ty.to_string() == field_ty
        && method.return_type.as_ref().is_some_and(|return_type| {
            is_void_or_declaring_type(return_type.to_string(), declaring_type)
        })
    {
        return Some(AccessorKind::Setter);
    }

    if method.name == format!("add{property}")
        && method.args.len() == 1
        && method.args[0].ty.to_string() == field_ty
        && method.return_type.as_ref().is_some_and(|return_type| {
            is_void_or_declaring_type(return_type.to_string(), declaring_type)
        })
    {
        return Some(AccessorKind::Add);
    }

    None
}

fn is_plain_method(method: &JavaMethod) -> bool {
    method.documentation.is_none()
        && method.annotations.is_empty()
        && method.type_parameters.is_empty()
        && method.throws.is_empty()
}

fn is_boring_no_arg_constructor(cons: &JavaConstructor) -> bool {
    cons.args.is_empty()
        && cons.documentation.is_none()
        && cons.annotations.is_empty()
        && cons.type_parameters.is_empty()
        && cons.throws.is_empty()
}

fn property_suffix(field_name: &str) -> String {
    let mut chars = field_name.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_uppercase().collect::<String>() + chars.as_str()
}

fn is_boolean_type(ty: &str) -> bool {
    ty == "boolean" || ty == "Boolean"
}

fn is_void_or_declaring_type(return_type: String, declaring_type: &str) -> bool {
    return_type == "void" || return_type == declaring_type
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::java_ast::parse_java_file;

    use super::super::{FileOutput, Format, render as dispatch_render};

    #[test]
    fn full_markdown_contains_expected_sections() {
        let source = r#"
            package com.example;

            import java.util.Optional;

            @Service
            public class UserService<T extends AutoCloseable> {
                @Inject
                private final UserRepository repository;

                @Autowired
                public UserService(UserRepository repository) throws ConfigurationException {
                    this.repository = repository;
                }

                @Deprecated
                public <E extends Exception> Optional<User> findById(@NotNull Long id) throws E {
                    return Optional.empty();
                }

                public static class Inner {
                    private int value;
                }
            }

            public enum Status {
                ACTIVE,
                INACTIVE
            }
        "#;

        let ast = parse_java_file(source).expect("parse");
        let path = PathBuf::from("src/UserService.java");
        let files = vec![FileOutput {
            path: &path,
            ast: &ast,
        }];
        let markdown = dispatch_render(&files, Format::Markdown).expect("render");

        assert!(markdown.starts_with("# Java Atlas\n\n"));
        assert!(markdown.contains("## `com.example`"));
        assert!(markdown.contains("- `UserService.java`"));
        assert!(!markdown.contains("com/example/UserService.java"));
        assert!(!markdown.contains("- package: `com.example`"));
        assert!(
            markdown.contains("  - class: `@Service public UserService<T extends AutoCloseable>`")
        );
        assert!(
            markdown.contains("    - fields: `@Inject private final UserRepository repository`")
        );
        assert!(markdown.contains(
            "    - constructors: `@Autowired public UserService(UserRepository repository) throws ConfigurationException`"
        ));
        assert!(markdown.contains(
            "    - methods: `@Deprecated public <E extends Exception> Optional<User> findById(@NotNull Long id) throws E`"
        ));
        assert!(markdown.contains("    - class: `public static Inner`"));
        assert!(markdown.contains("  - enum: `public Status`"));
        assert!(markdown.contains("    - constants: `ACTIVE`, `INACTIVE`"));
        // Only one top-level Atlas heading regardless of input file count.
        assert_eq!(markdown.matches("# Java Atlas").count(), 1);
    }

    #[test]
    fn multiple_files_share_one_atlas_heading() {
        let src_a = "package a; public class A {}";
        let src_b = "package b; public class B {}";
        let ast_a = parse_java_file(src_a).expect("parse a");
        let ast_b = parse_java_file(src_b).expect("parse b");
        let path_a = PathBuf::from("src/A.java");
        let path_b = PathBuf::from("src/B.java");
        let files = vec![
            FileOutput {
                path: &path_a,
                ast: &ast_a,
            },
            FileOutput {
                path: &path_b,
                ast: &ast_b,
            },
        ];
        let markdown = dispatch_render(&files, Format::Markdown).expect("render");

        assert_eq!(markdown.matches("# Java Atlas").count(), 1);
        assert!(markdown.contains("## `a`"));
        assert!(markdown.contains("## `b`"));
        assert!(markdown.contains("- `A.java`"));
        assert!(markdown.contains("- `B.java`"));
        assert!(markdown.contains("  - class: `public A`"));
        assert!(markdown.contains("  - class: `public B`"));
    }

    #[test]
    fn markdown_renders_javadocs() {
        let source = r#"
            /** Service docs. */
            public class UserService {
                /**
                 * Repository docs.
                 * @see UserRepository
                 */
                private final UserRepository repository;

                /**
                 * Finds a user.
                 *
                 * @param id user id
                 * @return optional user
                 */
                public Optional<User> findById(Long id) {
                    return Optional.empty();
                }
            }
        "#;

        let ast = parse_java_file(source).expect("parse");
        let path = PathBuf::from("src/UserService.java");
        let files = vec![FileOutput {
            path: &path,
            ast: &ast,
        }];
        let markdown = dispatch_render(&files, Format::Markdown).expect("render");

        assert!(markdown.contains("  - class: `public UserService` - Service docs."));
        assert!(markdown.contains("`private final UserRepository repository` - Repository docs."));
        assert!(markdown.contains("`public Optional<User> findById(Long id)` - Finds a user."));
        assert!(!markdown.contains("@param"));
        assert!(!markdown.contains("@return"));
        assert!(!markdown.contains("@see"));
    }

    #[test]
    fn markdown_compacts_standard_field_accessors() {
        let source = r#"
            package com.example;

            public class UserSendCodeRequest {
                private String to;
                private String channel;
                private boolean active;

                public String getTo() { return to; }
                public UserSendCodeRequest setTo(String to) { this.to = to; return this; }
                public String getChannel() { return channel; }
                public void setChannel(String channel) { this.channel = channel; }
                public boolean isActive() { return active; }
            }
        "#;

        let ast = parse_java_file(source).expect("parse");
        let path = PathBuf::from("src/UserSendCodeRequest.java");
        let files = vec![FileOutput {
            path: &path,
            ast: &ast,
        }];
        let markdown = dispatch_render(&files, Format::Markdown).expect("render");

        assert!(markdown.contains("`private String to [getter,setter]`"));
        assert!(markdown.contains("`private String channel [getter,setter]`"));
        assert!(markdown.contains("`private boolean active [getter]`"));
        assert!(!markdown.contains("getTo()"));
        assert!(!markdown.contains("setTo(String to)"));
        assert!(!markdown.contains("getChannel()"));
        assert!(!markdown.contains("setChannel(String channel)"));
        assert!(!markdown.contains("isActive()"));
        assert!(!markdown.contains("    - methods:"));
    }

    #[test]
    fn markdown_keeps_non_plain_or_non_matching_accessor_like_methods() {
        let source = r#"
            package com.example;

            public class UserService {
                private String name;
                private List<String> items;

                /** Name docs. */
                public String getName() { return name; }
                @Deprecated
                public void setName(String name) { this.name = name; }
                public String getOther() { return "other"; }
                public UserService addItems(List<String> items) { this.items = items; return this; }
            }
        "#;

        let ast = parse_java_file(source).expect("parse");
        let path = PathBuf::from("src/UserService.java");
        let files = vec![FileOutput {
            path: &path,
            ast: &ast,
        }];
        let markdown = dispatch_render(&files, Format::Markdown).expect("render");

        assert!(markdown.contains("`private String name`"));
        assert!(markdown.contains("`private List<String> items [add]`"));
        assert!(markdown.contains("public String getName()"));
        assert!(markdown.contains("@Deprecated public void setName(String name)"));
        assert!(markdown.contains("public String getOther()"));
        assert!(!markdown.contains("addItems(List<String> items)"));
    }

    #[test]
    fn markdown_omits_only_boring_no_arg_constructors() {
        let source = r#"
            package com.example;

            public class PlainDto {
                public PlainDto() {}
                public PlainDto(String name) {}
            }

            public class ConfiguredDto {
                @Inject
                public ConfiguredDto() {}
            }
        "#;

        let ast = parse_java_file(source).expect("parse");
        let path = PathBuf::from("src/PlainDto.java");
        let files = vec![FileOutput {
            path: &path,
            ast: &ast,
        }];
        let markdown = dispatch_render(&files, Format::Markdown).expect("render");

        assert!(!markdown.contains("public PlainDto()`"));
        assert!(markdown.contains("`public PlainDto(String name)`"));
        assert!(markdown.contains("`@Inject public ConfiguredDto()`"));
    }
}
