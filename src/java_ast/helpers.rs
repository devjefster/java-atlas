//! Small Tree-sitter node helpers shared by parser extraction steps.

use tree_sitter::Node;

use super::model::JavaArgument;

/// Returns the source text covered by a Tree-sitter node.
pub(super) fn node_text(node: Node, source: &str) -> String {
    source[node.byte_range().start..node.byte_range().end].to_string()
}

/// Converts a node byte range into the tuple stored in the model.
pub(super) fn byte_range(node: Node) -> (usize, usize) {
    let range = node.byte_range();
    (range.start, range.end)
}

/// Extracts Java modifier keywords and annotations from a `modifiers` node.
///
/// In tree-sitter-java the `modifiers` node's children are the modifier tokens
/// themselves (anonymous nodes like `"public"`, `"static"`) plus any leading
/// annotations (named nodes like `marker_annotation`). We keep both and let
/// the renderer join them with spaces, matching source order.
pub(super) fn get_modifier_list(node: Node, source: &str) -> Vec<String> {
    let mut mods = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        mods.push(node_text(child, source));
    }
    mods
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
                let ty = child
                    .child_by_field_name("type")
                    .map(|n| node_text(n, source));
                let name = child
                    .child_by_field_name("name")
                    .map(|n| node_text(n, source));
                if let (Some(ty), Some(name)) = (ty, name) {
                    args.push(JavaArgument { ty, name });
                }
            }
            "spread_parameter" => {
                let mut ty: Option<String> = None;
                let mut name: Option<String> = None;
                let mut inner = child.walk();
                for grand in child.children(&mut inner) {
                    match grand.kind() {
                        "modifiers" | "..." => {}
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
