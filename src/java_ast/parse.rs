//! Tree-sitter based extraction from Java source into the data model.

use tree_sitter::{Language, Node, Parser, Query, QueryCursor, StreamingIterator};
use tree_sitter_java::LANGUAGE;

use super::error::JavaAstError;
use super::helpers::{
    add_dimensions, byte_range, get_args, get_declaration_metadata, get_identifier_list,
    get_throws, get_type_parameters, node_text, parse_annotation_value, parse_type_ref,
};
use super::model::{
    JavaAnnotationElement, JavaConstructor, JavaDoc, JavaDocTag, JavaField, JavaFile, JavaMethod,
    JavaType, TypeKind,
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
    let mut annotations = Vec::new();
    let mut modifiers = Vec::new();
    let mut type_parameters = Vec::new();
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
            type_parameters: (type_parameters)? @type_parameters
            superclass: (superclass)? @extends
            interfaces: (super_interfaces)? @implements
            body: (class_body) @body)
          (interface_declaration
            (modifiers)? @modifiers
            name: (identifier) @name
            type_parameters: (type_parameters)? @type_parameters
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
            type_parameters: (type_parameters)? @type_parameters
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
                "modifiers" => {
                    let metadata = get_declaration_metadata(capture.node, source);
                    annotations = metadata.annotations;
                    modifiers = metadata.modifiers;
                }
                "extends" => extends = get_identifier_list(capture.node, source),
                "implements" => implements = get_identifier_list(capture.node, source),
                "type_parameters" => type_parameters = get_type_parameters(capture.node, source),
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
        documentation: leading_javadoc(node, source),
        annotations,
        modifiers,
        type_parameters,
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
            name: (identifier) @name
            dimensions: (dimensions)? @dimensions)) @field
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
        let mut annotations = Vec::new();
        let mut ty = None;
        let mut name = String::new();
        let mut range = (0, 0);
        let mut field_node = None;
        for capture in m.captures {
            match field_names[capture.index as usize] {
                "field" => {
                    range = byte_range(capture.node);
                    field_node = Some(capture.node);
                }
                "modifiers" => {
                    let metadata = get_declaration_metadata(capture.node, source);
                    annotations = metadata.annotations;
                    modifiers = metadata.modifiers;
                }
                "type" => ty = Some(parse_type_ref(capture.node, source)),
                "dimensions" => {
                    if let Some(current) = ty.take() {
                        ty = Some(add_dimensions(current, capture.node, source));
                    }
                }
                "name" => name = node_text(capture.node, source),
                _ => {}
            }
        }
        java_type.fields.push(JavaField {
            documentation: field_node.and_then(|node| leading_javadoc(node, source)),
            annotations,
            modifiers,
            ty: ty.unwrap_or_else(|| parse_type_ref(body, source)),
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
          type_parameters: (type_parameters)? @type_parameters
          type: (_) @return_type
          name: (identifier) @name
          parameters: (formal_parameters) @parameters
          dimensions: (dimensions)? @dimensions
          (throws)? @throws) @method
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
        let mut annotations = Vec::new();
        let mut type_parameters = Vec::new();
        let mut return_type = None;
        let mut name = String::new();
        let mut args = Vec::new();
        let mut throws = Vec::new();
        let mut range = (0, 0);
        let mut method_node = None;
        for capture in m.captures {
            match method_names[capture.index as usize] {
                "method" => {
                    range = byte_range(capture.node);
                    method_node = Some(capture.node);
                }
                "modifiers" => {
                    let metadata = get_declaration_metadata(capture.node, source);
                    annotations = metadata.annotations;
                    modifiers = metadata.modifiers;
                }
                "return_type" => return_type = Some(parse_type_ref(capture.node, source)),
                "type_parameters" => type_parameters = get_type_parameters(capture.node, source),
                "dimensions" => {
                    if let Some(current) = return_type.take() {
                        return_type = Some(add_dimensions(current, capture.node, source));
                    }
                }
                "name" => name = node_text(capture.node, source),
                "parameters" => args = get_args(capture.node, source),
                "throws" => throws = get_throws(capture.node, source),
                _ => {}
            }
        }
        java_type.methods.push(JavaMethod {
            documentation: method_node.and_then(|node| leading_javadoc(node, source)),
            annotations,
            modifiers,
            type_parameters,
            return_type,
            name,
            args,
            throws,
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
        [
          (constructor_declaration
            (modifiers)? @modifiers
            type_parameters: (type_parameters)? @type_parameters
            name: (identifier) @name
            parameters: (formal_parameters) @parameters
            (throws)? @throws)
          (compact_constructor_declaration
            (modifiers)? @modifiers
            name: (identifier) @name)
        ] @constructor
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
        let mut annotations = Vec::new();
        let mut type_parameters = Vec::new();
        let mut name = String::new();
        let mut args = Vec::new();
        let mut throws = Vec::new();
        let mut range = (0, 0);
        let mut constructor_node = None;
        for capture in m.captures {
            match constructor_names[capture.index as usize] {
                "constructor" => {
                    range = byte_range(capture.node);
                    constructor_node = Some(capture.node);
                }
                "modifiers" => {
                    let metadata = get_declaration_metadata(capture.node, source);
                    annotations = metadata.annotations;
                    modifiers = metadata.modifiers;
                }
                "type_parameters" => type_parameters = get_type_parameters(capture.node, source),
                "name" => name = node_text(capture.node, source),
                "parameters" => args = get_args(capture.node, source),
                "throws" => throws = get_throws(capture.node, source),
                _ => {}
            }
        }
        java_type.constructors.push(JavaConstructor {
            documentation: constructor_node.and_then(|node| leading_javadoc(node, source)),
            annotations,
            modifiers,
            type_parameters,
            name,
            args,
            throws,
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
          (modifiers)? @modifiers
          type: (_) @return_type
          name: (identifier) @name
          dimensions: (dimensions)? @dimensions
          value: (_)? @default_value) @annotation_element
    "#,
    )
    .map_err(|e| JavaAstError::QueryFailed(e.to_string()))?;
    let ann_element_names = ann_element_query.capture_names();
    let mut ann_element_matches = cursor.matches(&ann_element_query, body, source.as_bytes());
    while let Some(m) = ann_element_matches.next() {
        if !match_has_direct_capture(m.captures, ann_element_names, "annotation_element", body) {
            continue;
        }
        let mut annotations = Vec::new();
        let mut name = String::new();
        let mut return_type = None;
        let mut default_value = None;
        let mut range = (0, 0);
        let mut element_node = None;
        for capture in m.captures {
            match ann_element_names[capture.index as usize] {
                "annotation_element" => {
                    range = byte_range(capture.node);
                    element_node = Some(capture.node);
                }
                "modifiers" => {
                    annotations = get_declaration_metadata(capture.node, source).annotations;
                }
                "name" => name = node_text(capture.node, source),
                "return_type" => return_type = Some(parse_type_ref(capture.node, source)),
                "dimensions" => {
                    if let Some(current) = return_type.take() {
                        return_type = Some(add_dimensions(current, capture.node, source));
                    }
                }
                "default_value" => {
                    default_value = Some(parse_annotation_value(capture.node, source))
                }
                _ => {}
            }
        }
        java_type.annotation_elements.push(JavaAnnotationElement {
            documentation: element_node.and_then(|node| leading_javadoc(node, source)),
            annotations,
            name,
            return_type: return_type.unwrap_or_else(|| parse_type_ref(body, source)),
            default_value,
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

/// Finds and parses a Javadoc block immediately preceding a declaration.
fn leading_javadoc(node: Node, source: &str) -> Option<JavaDoc> {
    let before = source.get(..node.start_byte())?;
    let trimmed = before.trim_end();
    if !trimmed.ends_with("*/") {
        return None;
    }

    let start = trimmed.rfind("/**")?;
    parse_javadoc_block(&trimmed[start..])
}

/// Normalizes a raw `/** ... */` block into description text and block tags.
fn parse_javadoc_block(raw: &str) -> Option<JavaDoc> {
    if !raw.starts_with("/**") || !raw.ends_with("*/") {
        return None;
    }

    let inner = &raw[3..raw.len() - 2];
    let mut description_lines = Vec::new();
    let mut tags = Vec::new();
    let mut in_tags = false;

    for line in inner.lines() {
        let cleaned = clean_javadoc_line(line);
        if let Some((name, text)) = cleaned.strip_prefix('@').and_then(split_javadoc_tag) {
            in_tags = true;
            tags.push(JavaDocTag { name, text });
        } else if in_tags {
            if let Some(tag) = tags.last_mut() {
                append_javadoc_continuation(tag, &cleaned);
            }
        } else {
            description_lines.push(cleaned);
        }
    }

    Some(JavaDoc {
        description: description_lines.join("\n").trim().to_string(),
        tags,
    })
}

fn clean_javadoc_line(line: &str) -> String {
    let trimmed = line.trim_start();
    trimmed
        .strip_prefix('*')
        .map(|line| line.strip_prefix(' ').unwrap_or(line))
        .unwrap_or(trimmed)
        .trim_end()
        .to_string()
}

fn split_javadoc_tag(text: &str) -> Option<(String, String)> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let name = parts.next()?.to_string();
    let text = parts.next().unwrap_or("").trim().to_string();
    Some((name, text))
}

fn append_javadoc_continuation(tag: &mut JavaDocTag, line: &str) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    if !tag.text.is_empty() {
        tag.text.push(' ');
    }
    tag.text.push_str(line);
}

#[cfg(test)]
mod tests {
    use crate::java_ast::model::{
        JavaAnnotationArgument, JavaAnnotationValue, JavaPrimitiveType, JavaTypeArgument,
        JavaTypeRef, JavaWildcardBound,
    };

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

    #[test]
    fn parses_annotations_separately_from_modifiers() {
        let source = r#"
            @Entity
            public record User(
                @NotNull String name,
                @Size(max = 20) String... tags
            ) {
                @Inject
                public User {}

                @Column(name = "email")
                private final String email;

                @Deprecated
                public String email(@NotNull String fallback) {
                    return email;
                }
            }
        "#;

        let file = parse_java_file(source).expect("source should parse");
        let ty = &file.types[0];

        assert_eq!(ty.kind, TypeKind::Record);
        assert_eq!(ty.annotations[0].name, "Entity");
        assert_eq!(ty.modifiers, vec!["public"]);
        assert_eq!(ty.record_components[0].annotations[0].name, "NotNull");
        assert_eq!(ty.record_components[1].annotations[0].name, "Size");
        assert_eq!(
            ty.record_components[1].annotations[0].to_string(),
            "@Size(max = 20)"
        );
        assert_eq!(ty.record_components[1].ty.to_string(), "String");
        assert!(ty.record_components[1].varargs);

        assert_eq!(ty.constructors[0].annotations[0].name, "Inject");
        assert_eq!(ty.constructors[0].modifiers, vec!["public"]);

        assert_eq!(
            ty.fields[0].annotations[0].to_string(),
            "@Column(name = \"email\")"
        );
        assert_eq!(ty.fields[0].modifiers, vec!["private", "final"]);

        assert_eq!(ty.methods[0].annotations[0].name, "Deprecated");
        assert_eq!(ty.methods[0].modifiers, vec!["public"]);
        assert_eq!(
            ty.methods[0].return_type.as_ref().map(ToString::to_string),
            Some("String".to_string())
        );
        assert_eq!(ty.methods[0].args[0].annotations[0].name, "NotNull");
    }

    #[test]
    fn parses_annotation_element_annotations() {
        let source = r#"
            public @interface Labels {
                @Deprecated
                String value();
            }
        "#;

        let file = parse_java_file(source).expect("source should parse");
        let element = &file.types[0].annotation_elements[0];

        assert_eq!(file.types[0].kind, TypeKind::Annotation);
        assert_eq!(element.annotations[0].name, "Deprecated");
        assert_eq!(element.name, "value");
    }

    #[test]
    fn parses_javadocs_on_declarations() {
        let source = r#"
            /**
             * Service for users.
             *
             * @since 1.0
             */
            public class UserService {
                /** Repository storage. */
                private final UserRepository repository;

                /**
                 * Creates the service.
                 *
                 * @param repository storage dependency
                 */
                public UserService(UserRepository repository) {}

                /**
                 * Finds a user.
                 * Continues the description.
                 *
                 * @param id user id
                 *   continued tag text
                 * @return optional user
                 */
                public Optional<User> findById(Long id) {
                    return Optional.empty();
                }
            }

            public @interface Labels {
                /** Label value. */
                String value();
            }
        "#;

        let file = parse_java_file(source).expect("source should parse");
        let service = &file.types[0];
        let service_doc = service.documentation.as_ref().expect("type javadoc");
        assert_eq!(service_doc.description, "Service for users.");
        assert_eq!(service_doc.tags[0].name, "since");
        assert_eq!(service_doc.tags[0].text, "1.0");

        assert_eq!(
            service.fields[0]
                .documentation
                .as_ref()
                .expect("field javadoc")
                .description,
            "Repository storage."
        );
        assert_eq!(
            service.constructors[0]
                .documentation
                .as_ref()
                .expect("constructor javadoc")
                .tags[0]
                .text,
            "repository storage dependency"
        );

        let method_doc = service.methods[0]
            .documentation
            .as_ref()
            .expect("method javadoc");
        assert_eq!(
            method_doc.description,
            "Finds a user.\nContinues the description."
        );
        assert_eq!(method_doc.tags[0].name, "param");
        assert_eq!(method_doc.tags[0].text, "id user id continued tag text");
        assert_eq!(method_doc.tags[1].name, "return");

        assert_eq!(
            file.types[1].annotation_elements[0]
                .documentation
                .as_ref()
                .expect("annotation element javadoc")
                .description,
            "Label value."
        );
    }

    #[test]
    fn ignores_non_javadoc_and_non_leading_comments() {
        let source = r#"
            /* Ordinary block comment. */
            public class Ordinary {
                /* Ordinary field comment. */
                private int x;

                /** This documents the field, not the method. */
                private int y;

                public void run() {}
            }
        "#;

        let file = parse_java_file(source).expect("source should parse");
        let ty = &file.types[0];

        assert!(ty.documentation.is_none());
        assert!(ty.fields[0].documentation.is_none());
        assert!(ty.fields[1].documentation.is_some());
        assert!(ty.methods[0].documentation.is_none());
    }

    #[test]
    fn parses_structured_type_references() {
        let source = r#"
            import java.util.List;
            import java.util.Map;

            public class Types extends Base<String> implements Handler<Map<String, List<Integer[]>>> {
                private List<? extends Number>[] numbers;
                private String legacy[];
                public void accept(List<? super User> users, @Valid String[] names) {}
                public String legacyReturn()[];
            }
        "#;

        let file = parse_java_file(source).expect("source should parse");
        let ty = &file.types[0];

        assert_eq!(ty.extends[0].to_string(), "Base<String>");
        assert_eq!(
            ty.implements[0].to_string(),
            "Handler<Map<String, List<Integer[]>>>"
        );
        assert_eq!(ty.fields[0].ty.to_string(), "List<? extends Number>[]");
        assert_eq!(ty.fields[1].ty.to_string(), "String[]");
        assert_eq!(ty.methods[0].return_type, Some(JavaTypeRef::Void));
        assert_eq!(ty.methods[0].args[0].ty.to_string(), "List<? super User>");
        assert_eq!(ty.methods[0].args[1].ty.to_string(), "String[]");
        assert_eq!(
            ty.methods[1].return_type.as_ref().map(ToString::to_string),
            Some("String[]".to_string())
        );

        let JavaTypeRef::Array { element, .. } = &ty.fields[0].ty else {
            panic!("field should be an array type");
        };
        let JavaTypeRef::Reference(reference) = element.as_ref() else {
            panic!("field array element should be a generic reference");
        };
        let JavaTypeArgument::Wildcard(JavaTypeRef::Wildcard { bound, .. }) = &reference.args[0]
        else {
            panic!("field generic argument should be a wildcard");
        };
        assert!(matches!(bound, Some(JavaWildcardBound::Extends(_))));
    }

    #[test]
    fn parses_throws_type_parameters_and_nested_generics() {
        let source = r#"
            import java.io.IOException;
            import java.io.Serializable;
            import java.util.List;
            import java.util.Map;

            public class Types<T extends Serializable & Comparable<T>> {
                public <C extends Config & AutoCloseable> Types() throws ConfigurationException {}

                public <E extends Exception> Map.Entry<String, List<User.Id>> read()
                        throws IOException, Outer.InnerException {
                    return null;
                }
            }
        "#;

        let file = parse_java_file(source).expect("source should parse");
        let ty = &file.types[0];

        assert_eq!(
            ty.type_parameters[0].to_string(),
            "T extends Serializable & Comparable<T>"
        );
        assert_eq!(ty.type_parameters[0].bounds.len(), 2);

        let constructor = &ty.constructors[0];
        assert_eq!(
            constructor.type_parameters[0].to_string(),
            "C extends Config & AutoCloseable"
        );
        assert_eq!(constructor.throws[0].to_string(), "ConfigurationException");

        let method = &ty.methods[0];
        assert_eq!(method.type_parameters[0].to_string(), "E extends Exception");
        assert_eq!(
            method.return_type.as_ref().map(ToString::to_string),
            Some("Map.Entry<String, List<User.Id>>".to_string())
        );
        assert_eq!(
            method
                .throws
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["IOException", "Outer.InnerException"]
        );

        let JavaTypeRef::Reference(return_ref) = method.return_type.as_ref().unwrap() else {
            panic!("return type should be a reference");
        };
        assert_eq!(return_ref.name, "Entry");
        assert_eq!(
            return_ref.qualifier.as_ref().map(ToString::to_string),
            Some("Map".to_string())
        );
    }

    #[test]
    fn parses_structured_annotation_values_and_defaults() {
        let source = r#"
            @Column(name = "email", nullable = false, roles = {"ADMIN", "USER"}, nested = @Inner(count = 2), type = String.class)
            public @interface Labels {
                @Deprecated
                String value() default "user";
                int count() default 1 + 2;
            }
        "#;

        let file = parse_java_file(source).expect("source should parse");
        let annotation = &file.types[0].annotations[0];
        assert_eq!(annotation.name, "Column");
        assert_eq!(
            annotation.to_string(),
            "@Column(name = \"email\", nullable = false, roles = {\"ADMIN\", \"USER\"}, nested = @Inner(count = 2), type = String.class)"
        );

        assert!(matches!(
            &annotation.arguments[0],
            JavaAnnotationArgument::Named {
                name,
                value: JavaAnnotationValue::String(_),
            } if name == "name"
        ));
        assert!(matches!(
            &annotation.arguments[1],
            JavaAnnotationArgument::Named {
                name,
                value: JavaAnnotationValue::Boolean(false),
            } if name == "nullable"
        ));
        assert!(matches!(
            &annotation.arguments[2],
            JavaAnnotationArgument::Named {
                name,
                value: JavaAnnotationValue::Array(values),
            } if name == "roles" && values.len() == 2
        ));
        assert!(matches!(
            &annotation.arguments[4],
            JavaAnnotationArgument::Named {
                name,
                value: JavaAnnotationValue::ClassLiteral(JavaTypeRef::Reference(_)),
            } if name == "type"
        ));

        assert!(matches!(
            file.types[0].annotation_elements[0].default_value,
            Some(JavaAnnotationValue::String(_))
        ));
        assert!(matches!(
            file.types[0].annotation_elements[1].return_type,
            JavaTypeRef::Primitive(JavaPrimitiveType::Int)
        ));
        assert!(matches!(
            file.types[0].annotation_elements[1].default_value,
            Some(JavaAnnotationValue::Expression(_))
        ));
    }
}
