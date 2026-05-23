//! Small Tree-sitter node helpers shared by parser extraction steps.

use tree_sitter::Node;

use super::model::JavaArgument;

#[derive(Debug, Default)]
pub(super) struct DeclarationMetadata {
    pub annotations: Vec<String>,
    pub modifiers: Vec<String>,
}

/// Returns the source text covered by a Tree-sitter node.
pub(super) fn node_text(node: Node, source: &str) -> String {
    source[node.byte_range().start..node.byte_range().end].to_string()
}

/// Converts a node byte range into the tuple stored in the model.
pub(super) fn byte_range(node: Node) -> (usize, usize) {
    let range = node.byte_range();
    (range.start, range.end)
}

/// Splits Java modifier keywords from annotations in a `modifiers` node.
pub(super) fn get_declaration_metadata(node: Node, source: &str) -> DeclarationMetadata {
    let mut metadata = DeclarationMetadata::default();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "annotation" | "marker_annotation" => {
                metadata.annotations.push(node_text(child, source));
            }
            _ => metadata.modifiers.push(node_text(child, source)),
        }
    }
    metadata
}

/// Extracts type identifiers from extends/implements nodes.
///
/// `superclass` and `super_interfaces` / `extends_interfaces` wrap their type
/// references either directly or inside a `type_list`. The keyword (`extends`
/// / `implements`) and the commas inside `type_list` are anonymous tokens, so
/// taking every named descendant — recursing into `type_list` — gives us the
/// list of supertypes regardless of how the grammar shape changes per kind.
pub(super) fn get_identifier_list(node: Node, source: &str) -> Vec<String> {
    let mut ids = Vec::new();
    collect_named_types(node, source, &mut ids);
    ids
}

fn collect_named_types(node: Node, source: &str, out: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        if child.kind() == "type_list" {
            collect_named_types(child, source, out);
        } else {
            out.push(node_text(child, source));
        }
    }
}

/// Extracts method, constructor, or record component parameters.
///
/// `formal_parameter` exposes `type` and `name` as field-named children, so
/// we look those up directly. `spread_parameter` (varargs) has no field names
/// and wraps the parameter name in a `variable_declarator`, so we walk its
/// children explicitly and append `...` to the rendered type.
pub(super) fn get_args(node: Node, source: &str) -> Vec<JavaArgument> {
    let mut args = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "formal_parameter" => {
                let annotations = child_modifiers_annotations(child, source);
                let ty = child
                    .child_by_field_name("type")
                    .map(|n| node_text(n, source));
                let name = child
                    .child_by_field_name("name")
                    .map(|n| node_text(n, source));
                if let (Some(ty), Some(name)) = (ty, name) {
                    args.push(JavaArgument {
                        annotations,
                        ty,
                        name,
                    });
                }
            }
            "spread_parameter" => {
                let mut annotations = Vec::new();
                let mut ty: Option<String> = None;
                let mut name: Option<String> = None;
                let mut inner = child.walk();
                for grand in child.children(&mut inner) {
                    match grand.kind() {
                        "modifiers" => {
                            annotations.extend(get_declaration_metadata(grand, source).annotations);
                        }
                        "annotation" | "marker_annotation" => {
                            annotations.push(node_text(grand, source));
                        }
                        "..." => {}
                        "variable_declarator" => {
                            name = grand
                                .child_by_field_name("name")
                                .map(|n| node_text(n, source));
                        }
                        _ if ty.is_none() => {
                            ty = Some(node_text(grand, source));
                        }
                        _ => {}
                    }
                }
                if let (Some(t), Some(n)) = (ty, name) {
                    args.push(JavaArgument {
                        annotations,
                        ty: format!("{}...", t),
                        name: n,
                    });
                }
            }
            _ => {}
        }
    }
    args
}

fn child_modifiers_annotations(node: Node, source: &str) -> Vec<String> {
    let mut annotations = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifiers" {
            annotations.extend(get_declaration_metadata(child, source).annotations);
        }
    }
    annotations
}
