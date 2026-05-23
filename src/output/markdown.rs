//! Markdown rendering for parsed Java models.

use std::fmt;

use crate::java_ast::{JavaArgument, JavaFile, JavaType, TypeKind};
use crate::markdown::validate_markdown;

use super::{FileOutput, OutputError};

/// Render the whole codebase as a single Markdown document.
///
/// One `# Java Atlas` heading at the top, then one `## File: \`<path>\`` section
/// per source file. Inside each file, types start at h3 and step down one
/// heading level per nesting level (clamped at h6).
pub(super) fn render(files: &[FileOutput<'_>]) -> Result<String, OutputError> {
    let mut out = String::from("# Java Atlas\n\n");
    for file in files {
        out.push_str(&format!("## File: `{}`\n\n", file.path.display()));
        render_file_body(file.ast, &mut out);
    }
    validate_markdown(&out)?;
    Ok(out)
}

/// Append one file's body (package, imports, types) to the shared output buffer.
fn render_file_body(file: &JavaFile, out: &mut String) {
    if let Some(pkg) = &file.package {
        out.push_str(&format!("**Package:** `{}`\n\n", pkg));
    } else {
        out.push_str("**Package:** `_default_`\n\n");
    }

    if !file.imports.is_empty() {
        out.push_str("### Imports\n\n");
        for imp in &file.imports {
            out.push_str(&format!("- `{}`\n", imp));
        }
        out.push('\n');
    }

    for ty in &file.types {
        out.push_str(&render_type(ty, 0));
    }
}

/// Render one Java type and recursively include nested types.
fn render_type(ty: &JavaType, level: usize) -> String {
    let mut out = String::new();
    // Type, member-section, and per-member headings each step one level deeper.
    // Base depth is h3 (`level + 3`) so top-level types sit under the per-file
    // h2 heading. Clamp at h6 since Markdown has no h7+ and pulldown-cmark
    // would treat it as text, silently breaking the heading structure for deep
    // nesting.
    let type_heading = "#".repeat((level + 3).min(6));
    let section_heading = "#".repeat((level + 4).min(6));
    let item_heading = "#".repeat((level + 5).min(6));
    let type_name_str = match ty.kind {
        TypeKind::Class => "Class",
        TypeKind::Interface => "Interface",
        TypeKind::Enum => "Enum",
        TypeKind::Record => "Record",
        TypeKind::Annotation => "Annotation",
    };

    out.push_str(&format!(
        "{} {} `{}`\n\n",
        type_heading, type_name_str, ty.name
    ));

    let mut meta = Vec::new();
    if !ty.annotations.is_empty() {
        meta.push(format!(
            "**Annotations:** `{}`",
            join_display(&ty.annotations, " ")
        ));
    }
    if !ty.modifiers.is_empty() {
        meta.push(format!("**Modifiers:** `{}`", ty.modifiers.join(" ")));
    }
    if !ty.type_parameters.is_empty() {
        meta.push(format!(
            "**Type Parameters:** `{}`",
            join_display(&ty.type_parameters, ", ")
        ));
    }
    if !ty.extends.is_empty() {
        meta.push(format!(
            "**Extends:** `{}`",
            join_display(&ty.extends, ", ")
        ));
    }
    if !ty.implements.is_empty() {
        meta.push(format!(
            "**Implements:** `{}`",
            join_display(&ty.implements, ", ")
        ));
    }
    if !meta.is_empty() {
        out.push_str(&format!("{}\n\n", meta.join("\n")));
    }

    if !ty.fields.is_empty() {
        out.push_str(&format!("{} Fields\n\n", section_heading));
        out.push_str("| Annotations | Modifiers | Type | Name |\n");
        out.push_str("| --- | --- | --- | --- |\n");
        for field in &ty.fields {
            out.push_str(&format!(
                "| `{}` | `{}` | `{}` | `{}` |\n",
                join_display(&field.annotations, " "),
                field.modifiers.join(" "),
                field.ty,
                field.name
            ));
        }
        out.push('\n');
    }

    if !ty.constructors.is_empty() {
        out.push_str(&format!("{} Constructors\n\n", section_heading));
        for cons in &ty.constructors {
            out.push_str(&format!("{} `{}`\n\n", item_heading, cons.name));
            let mut meta = Vec::new();
            if !cons.annotations.is_empty() {
                meta.push(format!(
                    "**Annotations:** `{}`",
                    join_display(&cons.annotations, " ")
                ));
            }
            if !cons.modifiers.is_empty() {
                meta.push(format!("**Modifiers:** `{}`", cons.modifiers.join(" ")));
            }
            if !cons.type_parameters.is_empty() {
                meta.push(format!(
                    "**Type Parameters:** `{}`",
                    join_display(&cons.type_parameters, ", ")
                ));
            }
            if !cons.args.is_empty() {
                let args_str = format_args(&cons.args);
                meta.push(format!("**Arguments:** `{}`", args_str));
            }
            if !cons.throws.is_empty() {
                meta.push(format!(
                    "**Throws:** `{}`",
                    join_display(&cons.throws, ", ")
                ));
            }
            if !meta.is_empty() {
                out.push_str(&format!("{}\n\n", meta.join("\n")));
            } else {
                out.push('\n');
            }
        }
    }

    if !ty.methods.is_empty() {
        out.push_str(&format!("{} Methods\n\n", section_heading));
        for method in &ty.methods {
            out.push_str(&format!("{} `{}`\n\n", item_heading, method.name));
            let mut meta = Vec::new();
            if !method.annotations.is_empty() {
                meta.push(format!(
                    "**Annotations:** `{}`",
                    join_display(&method.annotations, " ")
                ));
            }
            if !method.modifiers.is_empty() {
                meta.push(format!("**Modifiers:** `{}`", method.modifiers.join(" ")));
            }
            if !method.type_parameters.is_empty() {
                meta.push(format!(
                    "**Type Parameters:** `{}`",
                    join_display(&method.type_parameters, ", ")
                ));
            }
            if let Some(rt) = &method.return_type {
                meta.push(format!("**Returns:** `{}`", rt));
            }
            if !method.args.is_empty() {
                let args_str = format_args(&method.args);
                meta.push(format!("**Arguments:** `{}`", args_str));
            }
            if !method.throws.is_empty() {
                meta.push(format!(
                    "**Throws:** `{}`",
                    join_display(&method.throws, ", ")
                ));
            }
            if !meta.is_empty() {
                out.push_str(&format!("{}\n\n", meta.join("\n")));
            } else {
                out.push('\n');
            }
        }
    }

    if !ty.enum_constants.is_empty() {
        out.push_str(&format!("{} Constants\n\n", section_heading));
        for constant in &ty.enum_constants {
            out.push_str(&format!("- `{}`\n", constant));
        }
        out.push('\n');
    }

    if !ty.record_components.is_empty() {
        out.push_str(&format!("{} Components\n\n", section_heading));
        out.push_str("| Annotations | Type | Name |\n");
        out.push_str("| --- | --- | --- |\n");
        for comp in &ty.record_components {
            out.push_str(&format!(
                "| `{}` | `{}` | `{}` |\n",
                join_display(&comp.annotations, " "),
                comp.ty,
                comp.name
            ));
        }
        out.push('\n');
    }

    if !ty.annotation_elements.is_empty() {
        out.push_str(&format!("{} Elements\n\n", section_heading));
        for elem in &ty.annotation_elements {
            out.push_str(&format!("{} `{}`\n\n", item_heading, elem.name));
            if !elem.annotations.is_empty() {
                out.push_str(&format!(
                    "- **Annotations:** `{}`\n",
                    join_display(&elem.annotations, " ")
                ));
            }
            out.push_str(&format!("- **Returns:** `{}`\n\n", elem.return_type));
        }
    }

    for nested in &ty.nested_types {
        out.push_str(&render_type(nested, level + 1));
    }

    out
}

fn format_args(args: &[JavaArgument]) -> String {
    args.iter()
        .map(|arg| {
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
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn join_display<T: fmt::Display>(items: &[T], separator: &str) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(separator)
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
        assert!(markdown.contains("## File: `src/UserService.java`"));
        assert!(markdown.contains("### Class `UserService`"));
        assert!(markdown.contains("**Annotations:** `@Service`"));
        assert!(markdown.contains("**Type Parameters:** `T extends AutoCloseable`"));
        assert!(markdown.contains("#### Fields"));
        assert!(
            markdown.contains("| `@Inject` | `private final` | `UserRepository` | `repository` |")
        );
        assert!(markdown.contains("**Throws:** `ConfigurationException`"));
        assert!(markdown.contains("**Arguments:** `@NotNull Long id`"));
        assert!(markdown.contains("#### Class `Inner`"));
        assert!(markdown.contains("### Enum `Status`"));
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
        assert!(markdown.contains("## File: `src/A.java`"));
        assert!(markdown.contains("## File: `src/B.java`"));
        assert!(markdown.contains("### Class `A`"));
        assert!(markdown.contains("### Class `B`"));
    }
}
