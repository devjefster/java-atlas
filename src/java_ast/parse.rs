//! Tree-sitter based extraction from Java source into the data model.

use tree_sitter::{Language, Node, Parser, Query, QueryCursor, StreamingIterator};
use tree_sitter_java::LANGUAGE;

use super::error::JavaAstError;
use super::helpers::{byte_range, get_args, get_identifier_list, get_modifier_list, node_text};
use super::model::{
    JavaAnnotationElement, JavaConstructor, JavaField, JavaFile, JavaMethod, JavaType, TypeKind,
};

/// Parses one Java source file into the internal model.
pub fn parse_java_file(source: &str) -> Result<JavaFile, JavaAstError> {
    let language: Language = LANGUAGE.into();
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|e| JavaAstError::LanguageSetup(e.to_string()))?;

    let tree = parser
        .parse(source, None)
        .ok_or(JavaAstError::ParseFailed)?;
    let root_node = tree.root_node();

    let mut package = None;
    let mut imports = Vec::new();
    let mut cursor = QueryCursor::new();

    // Package/import queries are file-level metadata, independent of type parsing.
    let package_query = Query::new(
        &language,
        r#"
        (package_declaration
          [
            (scoped_identifier) @package
            (identifier) @package
          ])
    "#,
    )
    .map_err(|e| JavaAstError::QueryFailed(e.to_string()))?;
    let package_names = package_query.capture_names();
    let mut package_matches = cursor.matches(&package_query, root_node, source.as_bytes());
    while let Some(m) = package_matches.next() {
        for capture in m.captures {
            if package_names[capture.index as usize] == "package" {
                package = Some(node_text(capture.node, source));
            }
        }
    }

    let import_query = Query::new(
        &language,
        r#"
        (import_declaration
          [
            (scoped_identifier) @import
            (asterisk) @wildcard
          ])
    "#,
    )
    .map_err(|e| JavaAstError::QueryFailed(e.to_string()))?;
    let mut import_matches = cursor.matches(&import_query, root_node, source.as_bytes());
    while let Some(m) = import_matches.next() {
        for capture in m.captures {
            imports.push(node_text(capture.node, source));
        }
    }

    let type_query = type_query(&language)?;
    let type_names = type_query.capture_names();
    let mut top_level_types = Vec::new();
    let mut type_matches = cursor.matches(&type_query, root_node, source.as_bytes());
    while let Some(m) = type_matches.next() {
        for capture in m.captures {
            // Allow parser error wrappers, but do not promote types nested in a body.
            if type_names[capture.index as usize] == "type"
                && is_top_level_type(capture.node, root_node)
            {
                top_level_types.push(capture.node);
            }
        }
    }

    let mut types = Vec::new();
    for node in top_level_types {
        types.push(parse_type(node, source, &mut cursor)?);
    }

    Ok(JavaFile {
        package,
        imports,
        types,
    })
}

/// Parses a class/interface/enum/record/annotation declaration and its members.
fn parse_type(
    node: Node,
    source: &str,
    cursor: &mut QueryCursor,
) -> Result<JavaType, JavaAstError> {
    let language: Language = LANGUAGE.into();
    let mut name = String::new();
    let mut modifiers = Vec::new();
    let mut extends = Vec::new();
    let mut implements = Vec::new();
    let mut components = Vec::new();
    let mut body_node = None;

    // One metadata query covers all supported Java type declarations.
    let metadata_query = Query::new(
        &language,
        r#"
        [
          (class_declaration
            (modifiers)? @modifiers
            name: (identifier) @name
            superclass: (superclass)? @extends
            interfaces: (super_interfaces)? @implements
            body: (class_body) @body)
          (interface_declaration
            (modifiers)? @modifiers
            name: (identifier) @name
            interfaces: (extends_interfaces)? @extends
            body: (interface_body) @body)
          (enum_declaration
            (modifiers)? @modifiers
            name: (identifier) @name
            interfaces: (super_interfaces)? @implements
            body: (enum_body) @body)
          (record_declaration
            (modifiers)? @modifiers
            name: (identifier) @name
            parameters: (formal_parameters) @components
            interfaces: (super_interfaces)? @implements
            body: (class_body) @body)
          (annotation_type_declaration
            (modifiers)? @modifiers
            name: (identifier) @name
            body: (annotation_type_body) @body)
        ]
    "#,
    )
    .map_err(|e| JavaAstError::QueryFailed(e.to_string()))?;

    let metadata_names = metadata_query.capture_names();
    let mut metadata_matches = cursor.matches(&metadata_query, node, source.as_bytes());
    while let Some(m) = metadata_matches.next() {
        for capture in m.captures {
            if !capture_belongs_to_type(capture.node, node) {
                continue;
            }
            match metadata_names[capture.index as usize] {
                "name" => name = node_text(capture.node, source),
                "modifiers" => modifiers = get_modifier_list(capture.node, source),
                "extends" => extends = get_identifier_list(capture.node, source),
                "implements" => implements = get_identifier_list(capture.node, source),
                "components" => components = get_args(capture.node, source),
                "body" => body_node = Some(capture.node),
                _ => {}
            }
        }
    }

    let kind = match node.kind() {
        "class_declaration" => TypeKind::Class,
        "interface_declaration" => TypeKind::Interface,
        "enum_declaration" => TypeKind::Enum,
        "record_declaration" => TypeKind::Record,
        "annotation_type_declaration" => TypeKind::Annotation,
        _ => TypeKind::Class,
    };

    let mut java_type = JavaType {
        kind,
        name,
        modifiers,
        extends,
        implements,
        fields: Vec::new(),
        constructors: Vec::new(),
        methods: Vec::new(),
        enum_constants: Vec::new(),
        record_components: components,
        annotation_elements: Vec::new(),
        nested_types: Vec::new(),
        range: byte_range(node),
        body_range: body_node.map(byte_range),
    };

    if let Some(body) = body_node {
        // Member extraction is scoped to the captured type body.
        extract_fields(&mut java_type, body, source, cursor, &language)?;
        extract_methods(&mut java_type, body, source, cursor, &language)?;
        extract_constructors(&mut java_type, body, source, cursor, &language)?;
        extract_enum_constants(&mut java_type, body, source, cursor, &language)?;
        extract_annotation_elements(&mut java_type, body, source, cursor, &language)?;
        extract_nested_types(&mut java_type, body, source, cursor, &language)?;
    }

    Ok(java_type)
}

/// Extracts field declarations from a type body.
fn extract_fields(
    java_type: &mut JavaType,
    body: Node,
    source: &str,
    cursor: &mut QueryCursor,
    language: &Language,
) -> Result<(), JavaAstError> {
    let field_query = Query::new(
        language,
        r#"
        (field_declaration
          (modifiers)? @modifiers
          type: (_) @type
          declarator: (variable_declarator
            name: (identifier) @name)) @field
    "#,
    )
    .map_err(|e| JavaAstError::QueryFailed(e.to_string()))?;
    let field_names = field_query.capture_names();
    let mut field_matches = cursor.matches(&field_query, body, source.as_bytes());
    while let Some(m) = field_matches.next() {
        if !match_has_direct_capture(m.captures, field_names, "field", body) {
            continue;
        }
        let mut modifiers = Vec::new();
        let mut ty = String::new();
        let mut name = String::new();
        let mut range = (0, 0);
        for capture in m.captures {
            match field_names[capture.index as usize] {
                "field" => range = byte_range(capture.node),
                "modifiers" => modifiers = get_modifier_list(capture.node, source),
                "type" => ty = node_text(capture.node, source),
                "name" => name = node_text(capture.node, source),
                _ => {}
            }
        }
        java_type.fields.push(JavaField {
            modifiers,
            ty,
            name,
            range,
        });
    }
    Ok(())
}

/// Extracts method declarations from a type body.
fn extract_methods(
    java_type: &mut JavaType,
    body: Node,
    source: &str,
    cursor: &mut QueryCursor,
    language: &Language,
) -> Result<(), JavaAstError> {
    let method_query = Query::new(
        language,
        r#"
        (method_declaration
          (modifiers)? @modifiers
          type: (_) @return_type
          name: (identifier) @name
          parameters: (formal_parameters) @parameters) @method
    "#,
    )
    .map_err(|e| JavaAstError::QueryFailed(e.to_string()))?;
    let method_names = method_query.capture_names();
    let mut method_matches = cursor.matches(&method_query, body, source.as_bytes());
    while let Some(m) = method_matches.next() {
        if !match_has_direct_capture(m.captures, method_names, "method", body) {
            continue;
        }
        let mut modifiers = Vec::new();
        let mut return_type = None;
        let mut name = String::new();
        let mut args = Vec::new();
        let mut range = (0, 0);
        for capture in m.captures {
            match method_names[capture.index as usize] {
                "method" => range = byte_range(capture.node),
                "modifiers" => modifiers = get_modifier_list(capture.node, source),
                "return_type" => return_type = Some(node_text(capture.node, source)),
                "name" => name = node_text(capture.node, source),
                "parameters" => args = get_args(capture.node, source),
                _ => {}
            }
        }
        java_type.methods.push(JavaMethod {
            modifiers,
            return_type,
            name,
            args,
            range,
        });
    }
    Ok(())
}

/// Extracts constructor declarations from a class or record body.
fn extract_constructors(
    java_type: &mut JavaType,
    body: Node,
    source: &str,
    cursor: &mut QueryCursor,
    language: &Language,
) -> Result<(), JavaAstError> {
    let constructor_query = Query::new(
        language,
        r#"
        (constructor_declaration
          (modifiers)? @modifiers
          name: (identifier) @name
          parameters: (formal_parameters) @parameters) @constructor
    "#,
    )
    .map_err(|e| JavaAstError::QueryFailed(e.to_string()))?;
    let constructor_names = constructor_query.capture_names();
    let mut constructor_matches = cursor.matches(&constructor_query, body, source.as_bytes());
    while let Some(m) = constructor_matches.next() {
        if !match_has_direct_capture(m.captures, constructor_names, "constructor", body) {
            continue;
        }
        let mut modifiers = Vec::new();
        let mut name = String::new();
        let mut args = Vec::new();
        let mut range = (0, 0);
        for capture in m.captures {
            match constructor_names[capture.index as usize] {
                "constructor" => range = byte_range(capture.node),
                "modifiers" => modifiers = get_modifier_list(capture.node, source),
                "name" => name = node_text(capture.node, source),
                "parameters" => args = get_args(capture.node, source),
                _ => {}
            }
        }
        java_type.constructors.push(JavaConstructor {
            modifiers,
            name,
            args,
            range,
        });
    }
    Ok(())
}

/// Extracts enum constants when the current type is an enum.
fn extract_enum_constants(
    java_type: &mut JavaType,
    body: Node,
    source: &str,
    cursor: &mut QueryCursor,
    language: &Language,
) -> Result<(), JavaAstError> {
    if java_type.kind != TypeKind::Enum {
        return Ok(());
    }

    let enum_query = Query::new(
        language,
        r#"
        (enum_constant
          name: (identifier) @name) @enum_constant
    "#,
    )
    .map_err(|e| JavaAstError::QueryFailed(e.to_string()))?;
    let enum_names = enum_query.capture_names();
    let mut enum_matches = cursor.matches(&enum_query, body, source.as_bytes());
    while let Some(m) = enum_matches.next() {
        if !match_has_direct_capture(m.captures, enum_names, "enum_constant", body) {
            continue;
        }
        for capture in m.captures {
            if enum_names[capture.index as usize] == "enum_constant" {
                java_type
                    .enum_constants
                    .push(node_text(capture.node, source));
            }
        }
    }
    Ok(())
}

/// Extracts annotation element declarations when the current type is an annotation.
fn extract_annotation_elements(
    java_type: &mut JavaType,
    body: Node,
    source: &str,
    cursor: &mut QueryCursor,
    language: &Language,
) -> Result<(), JavaAstError> {
    if java_type.kind != TypeKind::Annotation {
        return Ok(());
    }

    let ann_element_query = Query::new(
        language,
        r#"
        (annotation_type_element_declaration
          type: (_) @return_type
          name: (identifier) @name) @annotation_element
    "#,
    )
    .map_err(|e| JavaAstError::QueryFailed(e.to_string()))?;
    let ann_element_names = ann_element_query.capture_names();
    let mut ann_element_matches = cursor.matches(&ann_element_query, body, source.as_bytes());
    while let Some(m) = ann_element_matches.next() {
        if !match_has_direct_capture(m.captures, ann_element_names, "annotation_element", body) {
            continue;
        }
        let mut name = String::new();
        let mut return_type = String::new();
        let mut range = (0, 0);
        for capture in m.captures {
            match ann_element_names[capture.index as usize] {
                "annotation_element" => range = byte_range(capture.node),
                "name" => name = node_text(capture.node, source),
                "return_type" => return_type = node_text(capture.node, source),
                _ => {}
            }
        }
        java_type.annotation_elements.push(JavaAnnotationElement {
            name,
            return_type,
            default_value: None,
            range,
        });
    }
    Ok(())
}

/// Extracts nested type declarations and recursively parses each one.
fn extract_nested_types(
    java_type: &mut JavaType,
    body: Node,
    source: &str,
    cursor: &mut QueryCursor,
    language: &Language,
) -> Result<(), JavaAstError> {
    let nested_type_query = type_query(language)?;
    let nested_type_names = nested_type_query.capture_names();
    let mut nested_nodes = Vec::new();
    {
        // Drain query results before recursive parsing reuses the cursor.
        let mut nested_type_matches = cursor.matches(&nested_type_query, body, source.as_bytes());
        while let Some(m) = nested_type_matches.next() {
            for capture in m.captures {
                if nested_type_names[capture.index as usize] == "type"
                    && is_direct_child_of(capture.node, body)
                {
                    nested_nodes.push(capture.node);
                }
            }
        }
    }
    for nested in nested_nodes {
        java_type
            .nested_types
            .push(parse_type(nested, source, cursor)?);
    }
    Ok(())
}

/// Builds the shared query for Java type declarations.
fn type_query(language: &Language) -> Result<Query, JavaAstError> {
    Query::new(
        language,
        r#"
        [
          (class_declaration) @type
          (interface_declaration) @type
          (enum_declaration) @type
          (record_declaration) @type
          (annotation_type_declaration) @type
        ]
    "#,
    )
    .map_err(|e| JavaAstError::QueryFailed(e.to_string()))
}

/// Checks whether a matched declaration belongs at file scope.
fn is_top_level_type(node: Node, root_node: Node) -> bool {
    let mut parent = node.parent();
    while let Some(parent_node) = parent {
        if parent_node.id() == root_node.id() {
            return true;
        }
        if is_type_declaration(parent_node.kind()) || is_type_body(parent_node.kind()) {
            return false;
        }
        parent = parent_node.parent();
    }
    false
}

/// Checks whether a capture came from the declaration currently being parsed.
fn capture_belongs_to_type(capture_node: Node, type_node: Node) -> bool {
    let mut current = Some(capture_node);
    while let Some(node) = current {
        if is_type_declaration(node.kind()) {
            return node.id() == type_node.id();
        }
        current = node.parent();
    }
    false
}

/// Checks whether a query match has a direct declaration capture in this body.
fn match_has_direct_capture(
    captures: &[tree_sitter::QueryCapture],
    capture_names: &[&str],
    expected_name: &str,
    parent: Node,
) -> bool {
    captures.iter().any(|capture| {
        capture_names[capture.index as usize] == expected_name
            && is_direct_child_of(capture.node, parent)
    })
}

/// Checks direct parentage by node id instead of byte range.
fn is_direct_child_of(node: Node, parent: Node) -> bool {
    node.parent().is_some_and(|p| p.id() == parent.id())
}

/// Returns true for Java type declaration node kinds.
fn is_type_declaration(kind: &str) -> bool {
    matches!(
        kind,
        "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration"
    )
}

/// Returns true for Java nodes that own type members.
fn is_type_body(kind: &str) -> bool {
    matches!(
        kind,
        "class_body" | "interface_body" | "enum_body" | "annotation_type_body"
    )
}

#[cfg(test)]
mod tests {
    use super::{TypeKind, parse_java_file};

    #[test]
    fn parses_outer_type_without_promoting_nested_type() {
        let source = r#"
            package com.example;

            public class UserService {
                private final UserRepository repository;

                public UserService(UserRepository repository) {
                    this.repository = repository;
                }

                public User findById(Long id) {
                    return null;
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

        let file = parse_java_file(source).expect("source should parse");

        assert_eq!(file.types.len(), 2);
        assert_eq!(file.types[0].name, "UserService");
        assert_eq!(file.types[0].kind, TypeKind::Class);
        assert_eq!(file.types[0].fields.len(), 1);
        assert_eq!(file.types[0].fields[0].name, "repository");
        assert_eq!(file.types[0].methods.len(), 1);
        assert_eq!(file.types[0].methods[0].name, "findById");
        assert_eq!(file.types[0].nested_types.len(), 1);
        assert_eq!(file.types[0].nested_types[0].name, "Inner");
        assert_eq!(file.types[0].nested_types[0].fields.len(), 1);
        assert_eq!(file.types[0].nested_types[0].fields[0].name, "value");
        assert_eq!(file.types[1].name, "Status");
        assert_eq!(file.types[1].kind, TypeKind::Enum);
    }
}
