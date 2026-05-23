//! Markdown rendering for parsed Java models.

use super::error::JavaAstError;
use std::fmt;

use super::model::{JavaArgument, JavaFile, JavaType, TypeKind};

/// Renders a parsed Java file model as Markdown.
pub fn render_markdown(file: &JavaFile) -> Result<String, JavaAstError> {
    let mut out = String::from("# Java Atlas\n\n");

    if let Some(pkg) = &file.package {
        out.push_str(&format!("**Package:** `{}`\n\n", pkg));
    } else {
        out.push_str("**Package:** `_default_`\n\n");
    }

    if !file.imports.is_empty() {
        out.push_str("## Imports\n\n");
        for imp in &file.imports {
            out.push_str(&format!("- `{}`\n", imp));
        }
        out.push('\n');
    }

    for ty in &file.types {
        out.push_str(&render_type(ty, 0));
    }

    Ok(out)
}

/// Renders one Java type and recursively includes nested types.
fn render_type(ty: &JavaType, level: usize) -> String {
    let mut out = String::new();
    // Type, member-section, and per-member headings each step one level deeper.
    // Clamp at h6 since Markdown has no h7+ and pulldown-cmark would treat it
    // as text, silently breaking the heading structure for deep nesting.
    let type_heading = "#".repeat((level + 2).min(6));
    let section_heading = "#".repeat((level + 3).min(6));
    let item_heading = "#".repeat((level + 4).min(6));
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
            if !cons.args.is_empty() {
                let args_str = format_args(&cons.args);
                meta.push(format!("**Arguments:** `{}`", args_str));
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
            if let Some(rt) = &method.return_type {
                meta.push(format!("**Returns:** `{}`", rt));
            }
            if !method.args.is_empty() {
                let args_str = format_args(&method.args);
                meta.push(format!("**Arguments:** `{}`", args_str));
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
