//! Plain data model produced by the Java parser.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TypeKind {
    Class,
    Interface,
    Enum,
    Record,
    Annotation,
}

#[derive(Debug, Clone)]
pub struct JavaArgument {
    pub ty: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct JavaField {
    pub modifiers: Vec<String>,
    pub ty: String,
    pub name: String,
    pub range: (usize, usize),
}

#[derive(Debug, Clone)]
pub struct JavaConstructor {
    pub modifiers: Vec<String>,
    pub name: String,
    pub args: Vec<JavaArgument>,
    pub range: (usize, usize),
}

#[derive(Debug, Clone)]
pub struct JavaMethod {
    pub modifiers: Vec<String>,
    pub return_type: Option<String>,
    pub name: String,
    pub args: Vec<JavaArgument>,
    pub range: (usize, usize),
}

#[derive(Debug, Clone)]
pub struct JavaAnnotationElement {
    pub name: String,
    pub return_type: String,
    pub default_value: Option<String>,
    pub range: (usize, usize),
}

#[derive(Debug, Clone)]
pub struct JavaType {
    pub kind: TypeKind,
    pub name: String,
    pub modifiers: Vec<String>,
    pub extends: Vec<String>,
    pub implements: Vec<String>,
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

#[derive(Debug, Clone)]
pub struct JavaFile {
    pub package: Option<String>,
    pub imports: Vec<String>,
    pub types: Vec<JavaType>,
}
