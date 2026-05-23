//! Plain data model produced by the Java parser.

use std::fmt;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum TypeKind {
    Class,
    Interface,
    Enum,
    Record,
    Annotation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JavaPrimitiveType {
    Boolean,
    Byte,
    Short,
    Int,
    Long,
    Char,
    Float,
    Double,
}

impl JavaPrimitiveType {
    pub fn from_source(text: &str) -> Option<Self> {
        match text {
            "boolean" => Some(Self::Boolean),
            "byte" => Some(Self::Byte),
            "short" => Some(Self::Short),
            "int" => Some(Self::Int),
            "long" => Some(Self::Long),
            "char" => Some(Self::Char),
            "float" => Some(Self::Float),
            "double" => Some(Self::Double),
            _ => None,
        }
    }
}

impl fmt::Display for JavaPrimitiveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            JavaPrimitiveType::Boolean => "boolean",
            JavaPrimitiveType::Byte => "byte",
            JavaPrimitiveType::Short => "short",
            JavaPrimitiveType::Int => "int",
            JavaPrimitiveType::Long => "long",
            JavaPrimitiveType::Char => "char",
            JavaPrimitiveType::Float => "float",
            JavaPrimitiveType::Double => "double",
        };
        f.write_str(text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value")]
pub enum JavaTypeRef {
    Primitive(JavaPrimitiveType),
    Void,
    Reference(JavaReferenceType),
    Array {
        element: Box<JavaTypeRef>,
        dimensions: usize,
    },
    Wildcard {
        annotations: Vec<JavaAnnotation>,
        bound: Option<JavaWildcardBound>,
    },
    Annotated {
        annotations: Vec<JavaAnnotation>,
        inner: Box<JavaTypeRef>,
    },
    Unsupported {
        kind: String,
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JavaReferenceType {
    pub qualifier: Option<Box<JavaTypeRef>>,
    pub name: String,
    pub args: Vec<JavaTypeArgument>,
    pub resolved_fqn: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JavaTypeParameter {
    pub annotations: Vec<JavaAnnotation>,
    pub name: String,
    pub bounds: Vec<JavaTypeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value")]
pub enum JavaTypeArgument {
    Type(JavaTypeRef),
    Wildcard(JavaTypeRef),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value")]
pub enum JavaWildcardBound {
    Extends(Box<JavaTypeRef>),
    Super(Box<JavaTypeRef>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JavaAnnotation {
    pub name: String,
    pub arguments: Vec<JavaAnnotationArgument>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value")]
pub enum JavaAnnotationArgument {
    Default(JavaAnnotationValue),
    Named {
        name: String,
        value: JavaAnnotationValue,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value")]
pub enum JavaAnnotationValue {
    String(String),
    Char(String),
    Number(String),
    Boolean(bool),
    Null,
    ClassLiteral(JavaTypeRef),
    Reference(String),
    Array(Vec<JavaAnnotationValue>),
    Annotation(JavaAnnotation),
    Expression(String),
    Unsupported { kind: String, text: String },
}

impl fmt::Display for JavaTypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JavaTypeRef::Primitive(primitive) => write!(f, "{primitive}"),
            JavaTypeRef::Void => f.write_str("void"),
            JavaTypeRef::Reference(reference) => write!(f, "{reference}"),
            JavaTypeRef::Array {
                element,
                dimensions,
            } => {
                write!(f, "{element}")?;
                for _ in 0..*dimensions {
                    f.write_str("[]")?;
                }
                Ok(())
            }
            JavaTypeRef::Wildcard { annotations, bound } => {
                write_annotations_prefix(f, annotations)?;
                f.write_str("?")?;
                if let Some(bound) = bound {
                    write!(f, " {bound}")?;
                }
                Ok(())
            }
            JavaTypeRef::Annotated { annotations, inner } => {
                write_annotations_prefix(f, annotations)?;
                write!(f, "{inner}")
            }
            JavaTypeRef::Unsupported { text, .. } => f.write_str(text),
        }
    }
}

impl fmt::Display for JavaReferenceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(qualifier) = &self.qualifier {
            write!(f, "{qualifier}.")?;
        }
        f.write_str(&self.name)?;
        if !self.args.is_empty() {
            let args = self
                .args
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            write!(f, "<{args}>")?;
        }
        Ok(())
    }
}

impl fmt::Display for JavaTypeArgument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JavaTypeArgument::Type(ty) | JavaTypeArgument::Wildcard(ty) => write!(f, "{ty}"),
        }
    }
}

impl fmt::Display for JavaWildcardBound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JavaWildcardBound::Extends(ty) => write!(f, "extends {ty}"),
            JavaWildcardBound::Super(ty) => write!(f, "super {ty}"),
        }
    }
}

impl fmt::Display for JavaAnnotation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", self.name)?;
        if !self.arguments.is_empty() {
            let args = self
                .arguments
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            write!(f, "({args})")?;
        }
        Ok(())
    }
}

impl fmt::Display for JavaAnnotationArgument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JavaAnnotationArgument::Default(value) => write!(f, "{value}"),
            JavaAnnotationArgument::Named { name, value } => write!(f, "{name} = {value}"),
        }
    }
}

impl fmt::Display for JavaAnnotationValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JavaAnnotationValue::String(text)
            | JavaAnnotationValue::Char(text)
            | JavaAnnotationValue::Number(text)
            | JavaAnnotationValue::Reference(text)
            | JavaAnnotationValue::Expression(text) => f.write_str(text),
            JavaAnnotationValue::Boolean(value) => write!(f, "{value}"),
            JavaAnnotationValue::Null => f.write_str("null"),
            JavaAnnotationValue::ClassLiteral(ty) => write!(f, "{ty}.class"),
            JavaAnnotationValue::Array(values) => {
                let values = values
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{{{values}}}")
            }
            JavaAnnotationValue::Annotation(annotation) => write!(f, "{annotation}"),
            JavaAnnotationValue::Unsupported { text, .. } => f.write_str(text),
        }
    }
}

impl fmt::Display for JavaTypeParameter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_annotations_prefix(f, &self.annotations)?;
        f.write_str(&self.name)?;
        if !self.bounds.is_empty() {
            let bounds = self
                .bounds
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" & ");
            write!(f, " extends {bounds}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JavaDoc {
    pub description: String,
    pub tags: Vec<JavaDocTag>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JavaDocTag {
    pub name: String,
    pub text: String,
}

fn write_annotations_prefix(
    f: &mut fmt::Formatter<'_>,
    annotations: &[JavaAnnotation],
) -> fmt::Result {
    if !annotations.is_empty() {
        let rendered = annotations
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        write!(f, "{rendered} ")?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct JavaArgument {
    pub annotations: Vec<JavaAnnotation>,
    pub ty: JavaTypeRef,
    pub name: String,
    pub varargs: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct JavaField {
    pub documentation: Option<JavaDoc>,
    pub annotations: Vec<JavaAnnotation>,
    pub modifiers: Vec<String>,
    pub ty: JavaTypeRef,
    pub name: String,
    pub range: (usize, usize),
}

#[derive(Debug, Clone, Serialize)]
pub struct JavaConstructor {
    pub documentation: Option<JavaDoc>,
    pub annotations: Vec<JavaAnnotation>,
    pub modifiers: Vec<String>,
    pub type_parameters: Vec<JavaTypeParameter>,
    pub name: String,
    pub args: Vec<JavaArgument>,
    pub throws: Vec<JavaTypeRef>,
    pub range: (usize, usize),
}

#[derive(Debug, Clone, Serialize)]
pub struct JavaMethod {
    pub documentation: Option<JavaDoc>,
    pub annotations: Vec<JavaAnnotation>,
    pub modifiers: Vec<String>,
    pub type_parameters: Vec<JavaTypeParameter>,
    pub return_type: Option<JavaTypeRef>,
    pub name: String,
    pub args: Vec<JavaArgument>,
    pub throws: Vec<JavaTypeRef>,
    pub range: (usize, usize),
}

#[derive(Debug, Clone, Serialize)]
pub struct JavaAnnotationElement {
    pub documentation: Option<JavaDoc>,
    pub annotations: Vec<JavaAnnotation>,
    pub name: String,
    pub return_type: JavaTypeRef,
    pub default_value: Option<JavaAnnotationValue>,
    pub range: (usize, usize),
}

#[derive(Debug, Clone, Serialize)]
pub struct JavaType {
    pub kind: TypeKind,
    pub name: String,
    pub documentation: Option<JavaDoc>,
    pub annotations: Vec<JavaAnnotation>,
    pub modifiers: Vec<String>,
    pub type_parameters: Vec<JavaTypeParameter>,
    pub extends: Vec<JavaTypeRef>,
    pub implements: Vec<JavaTypeRef>,
    pub fields: Vec<JavaField>,
    pub constructors: Vec<JavaConstructor>,
    pub methods: Vec<JavaMethod>,
    pub enum_constants: Vec<String>,
    pub record_components: Vec<JavaArgument>,
    pub annotation_elements: Vec<JavaAnnotationElement>,
    pub nested_types: Vec<JavaType>,
    pub range: (usize, usize),
    pub body_range: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JavaFile {
    pub package: Option<String>,
    pub imports: Vec<String>,
    pub types: Vec<JavaType>,
}
