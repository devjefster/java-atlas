//! Small Tree-sitter node helpers shared by parser extraction steps.

use tree_sitter::Node;

use super::model::{
    JavaAnnotation, JavaAnnotationArgument, JavaAnnotationValue, JavaArgument, JavaPrimitiveType,
    JavaReferenceType, JavaTypeArgument, JavaTypeRef, JavaWildcardBound,
};

#[derive(Debug, Default)]
pub(super) struct DeclarationMetadata {
    pub annotations: Vec<JavaAnnotation>,
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
                metadata.annotations.push(parse_annotation(child, source));
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
pub(super) fn get_identifier_list(node: Node, source: &str) -> Vec<JavaTypeRef> {
    let mut ids = Vec::new();
    collect_named_types(node, source, &mut ids);
    ids
}

fn collect_named_types(node: Node, source: &str, out: &mut Vec<JavaTypeRef>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        if child.kind() == "type_list" {
            collect_named_types(child, source, out);
        } else {
            out.push(parse_type_ref(child, source));
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
                    .map(|n| parse_type_ref(n, source))
                    .map(|ty| {
                        child
                            .child_by_field_name("dimensions")
                            .map(|dimensions| add_dimensions(ty.clone(), dimensions, source))
                            .unwrap_or(ty)
                    });
                let name = child
                    .child_by_field_name("name")
                    .map(|n| node_text(n, source));
                if let (Some(ty), Some(name)) = (ty, name) {
                    args.push(JavaArgument {
                        annotations,
                        ty,
                        name,
                        varargs: false,
                    });
                }
            }
            "spread_parameter" => {
                let mut annotations = Vec::new();
                let mut ty: Option<String> = None;
                let mut ty_node: Option<Node> = None;
                let mut name: Option<String> = None;
                let mut inner = child.walk();
                for grand in child.children(&mut inner) {
                    match grand.kind() {
                        "modifiers" => {
                            annotations.extend(get_declaration_metadata(grand, source).annotations);
                        }
                        "annotation" | "marker_annotation" => {
                            annotations.push(parse_annotation(grand, source));
                        }
                        "..." => {}
                        "variable_declarator" => {
                            name = grand
                                .child_by_field_name("name")
                                .map(|n| node_text(n, source));
                        }
                        _ if ty.is_none() => {
                            ty = Some(node_text(grand, source));
                            ty_node = Some(grand);
                        }
                        _ => {}
                    }
                }
                if let (Some(t), Some(n)) = (ty, name) {
                    let element = ty_node
                        .map(|node| parse_type_ref(node, source))
                        .unwrap_or_else(|| unsupported_type("spread_parameter", &t));
                    args.push(JavaArgument {
                        annotations,
                        ty: element,
                        name: n,
                        varargs: true,
                    });
                }
            }
            _ => {}
        }
    }
    args
}

pub(super) fn parse_type_ref(node: Node, source: &str) -> JavaTypeRef {
    let text = node_text(node, source);
    if let Some(primitive) = JavaPrimitiveType::from_source(text.as_str()) {
        return JavaTypeRef::Primitive(primitive);
    }

    match node.kind() {
        "void_type" => JavaTypeRef::Void,
        "integral_type" | "floating_point_type" | "boolean_type" => {
            JavaPrimitiveType::from_source(text.as_str())
                .map(JavaTypeRef::Primitive)
                .unwrap_or_else(|| unsupported_type(node.kind(), &text))
        }
        "type_identifier" | "scoped_type_identifier" | "identifier" | "scoped_identifier" => {
            JavaTypeRef::Reference(JavaReferenceType {
                name: text,
                args: Vec::new(),
            })
        }
        "generic_type" => parse_generic_type(node, source),
        "array_type" => parse_array_type(node, source),
        "annotated_type" => parse_annotated_type(node, source),
        "wildcard" => parse_wildcard(node, source),
        _ => unsupported_type(node.kind(), &text),
    }
}

pub(super) fn parse_annotation(node: Node, source: &str) -> JavaAnnotation {
    let name = node
        .child_by_field_name("name")
        .map(|name| node_text(name, source))
        .unwrap_or_else(|| node_text(node, source));
    let arguments = node
        .child_by_field_name("arguments")
        .map(|args| parse_annotation_arguments(args, source))
        .unwrap_or_default();

    JavaAnnotation { name, arguments }
}

pub(super) fn parse_annotation_value(node: Node, source: &str) -> JavaAnnotationValue {
    match node.kind() {
        "string_literal" => JavaAnnotationValue::String(node_text(node, source)),
        "character_literal" => JavaAnnotationValue::Char(node_text(node, source)),
        "decimal_integer_literal"
        | "hex_integer_literal"
        | "octal_integer_literal"
        | "binary_integer_literal"
        | "decimal_floating_point_literal"
        | "hex_floating_point_literal" => JavaAnnotationValue::Number(node_text(node, source)),
        "true" => JavaAnnotationValue::Boolean(true),
        "false" => JavaAnnotationValue::Boolean(false),
        "null_literal" => JavaAnnotationValue::Null,
        "annotation" | "marker_annotation" => {
            JavaAnnotationValue::Annotation(parse_annotation(node, source))
        }
        "element_value_array_initializer" => parse_annotation_array(node, source),
        "class_literal" => parse_class_literal(node, source),
        "identifier" | "scoped_identifier" | "field_access" => {
            JavaAnnotationValue::Reference(node_text(node, source))
        }
        "unary_expression" | "binary_expression" | "parenthesized_expression" => {
            JavaAnnotationValue::Expression(node_text(node, source))
        }
        _ => {
            if let Some(inner) = sole_named_child(node) {
                parse_annotation_value(inner, source)
            } else {
                JavaAnnotationValue::Unsupported {
                    kind: node.kind().to_string(),
                    text: node_text(node, source),
                }
            }
        }
    }
}

pub(super) fn add_dimensions(ty: JavaTypeRef, dimensions: Node, source: &str) -> JavaTypeRef {
    let extra = count_dimensions(dimensions, source);
    match ty {
        JavaTypeRef::Array {
            element,
            dimensions,
        } => JavaTypeRef::Array {
            element,
            dimensions: dimensions + extra,
        },
        _ => JavaTypeRef::Array {
            element: Box::new(ty),
            dimensions: extra,
        },
    }
}

fn parse_generic_type(node: Node, source: &str) -> JavaTypeRef {
    let mut name = None;
    let mut args = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "type_identifier" | "scoped_type_identifier" | "identifier" | "scoped_identifier"
                if name.is_none() =>
            {
                name = Some(node_text(child, source));
            }
            "type_arguments" => args = parse_type_arguments(child, source),
            _ => {}
        }
    }

    JavaTypeRef::Reference(JavaReferenceType {
        name: name.unwrap_or_else(|| node_text(node, source)),
        args,
    })
}

fn parse_type_arguments(node: Node, source: &str) -> Vec<JavaTypeArgument> {
    let mut args = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        let parsed = parse_type_ref(child, source);
        if child.kind() == "wildcard" {
            args.push(JavaTypeArgument::Wildcard(parsed));
        } else {
            args.push(JavaTypeArgument::Type(parsed));
        }
    }
    args
}

fn parse_array_type(node: Node, source: &str) -> JavaTypeRef {
    let element = node
        .child_by_field_name("element")
        .map(|element| parse_type_ref(element, source))
        .unwrap_or_else(|| unsupported_type(node.kind(), &node_text(node, source)));
    let dimensions = node
        .child_by_field_name("dimensions")
        .map(|dimensions| count_dimensions(dimensions, source))
        .unwrap_or(1);

    JavaTypeRef::Array {
        element: Box::new(element),
        dimensions,
    }
}

fn parse_annotated_type(node: Node, source: &str) -> JavaTypeRef {
    let mut annotations = Vec::new();
    let mut inner = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "annotation" | "marker_annotation" => annotations.push(parse_annotation(child, source)),
            _ if child.is_named() => inner = Some(parse_type_ref(child, source)),
            _ => {}
        }
    }

    JavaTypeRef::Annotated {
        annotations,
        inner: Box::new(
            inner.unwrap_or_else(|| unsupported_type(node.kind(), &node_text(node, source))),
        ),
    }
}

fn parse_wildcard(node: Node, source: &str) -> JavaTypeRef {
    let mut annotations = Vec::new();
    let mut bound_kind = None;
    let mut bound_type = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "annotation" | "marker_annotation" => annotations.push(parse_annotation(child, source)),
            "super" => bound_kind = Some("super"),
            _ if child.is_named() => {
                if node_text(child, source) == "extends" {
                    bound_kind = Some("extends");
                } else {
                    bound_type = Some(parse_type_ref(child, source));
                }
            }
            _ if node_text(child, source) == "extends" => bound_kind = Some("extends"),
            _ => {}
        }
    }

    let bound = match (bound_kind, bound_type) {
        (Some("super"), Some(ty)) => Some(JavaWildcardBound::Super(Box::new(ty))),
        (Some("extends"), Some(ty)) => Some(JavaWildcardBound::Extends(Box::new(ty))),
        (_, _) => None,
    };

    JavaTypeRef::Wildcard { annotations, bound }
}

fn parse_annotation_arguments(node: Node, source: &str) -> Vec<JavaAnnotationArgument> {
    let mut args = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        if child.kind() == "element_value_pair" {
            let name = child
                .child_by_field_name("key")
                .map(|key| node_text(key, source))
                .unwrap_or_default();
            if let Some(value) = child.child_by_field_name("value") {
                args.push(JavaAnnotationArgument::Named {
                    name,
                    value: parse_annotation_value(value, source),
                });
            }
        } else {
            args.push(JavaAnnotationArgument::Default(parse_annotation_value(
                child, source,
            )));
        }
    }
    args
}

fn parse_annotation_array(node: Node, source: &str) -> JavaAnnotationValue {
    let mut values = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_named() {
            values.push(parse_annotation_value(child, source));
        }
    }
    JavaAnnotationValue::Array(values)
}

fn parse_class_literal(node: Node, source: &str) -> JavaAnnotationValue {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_named() {
            return JavaAnnotationValue::ClassLiteral(parse_type_ref(child, source));
        }
    }
    JavaAnnotationValue::Unsupported {
        kind: node.kind().to_string(),
        text: node_text(node, source),
    }
}

fn count_dimensions(node: Node, source: &str) -> usize {
    node_text(node, source).matches("[]").count().max(1)
}

fn unsupported_type(kind: &str, text: &str) -> JavaTypeRef {
    JavaTypeRef::Unsupported {
        kind: kind.to_string(),
        text: text.to_string(),
    }
}

fn sole_named_child(node: Node) -> Option<Node> {
    let mut found = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_named() {
            if found.is_some() {
                return None;
            }
            found = Some(child);
        }
    }
    found
}

fn child_modifiers_annotations(node: Node, source: &str) -> Vec<JavaAnnotation> {
    let mut annotations = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifiers" {
            annotations.extend(get_declaration_metadata(child, source).annotations);
        }
    }
    annotations
}
