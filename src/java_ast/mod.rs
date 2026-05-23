//! Java source extraction facade. Output rendering lives in `crate::output`.

mod error;
mod helpers;
mod model;
mod parse;
mod resolve;

pub use error::JavaAstError;
pub use model::{
    JavaAnnotation, JavaAnnotationArgument, JavaAnnotationElement, JavaAnnotationValue,
    JavaArgument, JavaConstructor, JavaDoc, JavaDocTag, JavaField, JavaFile, JavaMethod,
    JavaPrimitiveType, JavaReferenceType, JavaType, JavaTypeArgument, JavaTypeParameter,
    JavaTypeRef, JavaWildcardBound, TypeKind,
};
pub use parse::parse_java_file;
pub use resolve::resolve_files;
