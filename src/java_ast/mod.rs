//! Java source extraction and Markdown rendering facade.

mod error;
mod helpers;
mod model;
mod parse;
mod render;

pub use error::JavaAstError;
pub use model::{
    JavaAnnotationElement, JavaArgument, JavaConstructor, JavaField, JavaFile, JavaMethod,
    JavaType, TypeKind,
};
pub use parse::parse_java_file;
pub use render::render_markdown;

/// Convenience API for callers that only need rendered output.
pub fn extract_markdown(source: &str) -> Result<String, JavaAstError> {
    let file = parse_java_file(source)?;
    let markdown = render_markdown(&file)?;
    crate::markdown::validate_markdown(&markdown)
        .map_err(|e| JavaAstError::MarkdownInvalid(e.to_string()))?;
    Ok(markdown)
}

#[cfg(test)]
mod tests {
    use crate::markdown::validate_markdown;

    use super::extract_markdown;

    #[test]
    fn generated_markdown_is_structurally_valid() {
        let source = r#"
            package com.example;

            import java.util.Optional;

            public class UserService {
                private final UserRepository repository;

                public UserService(UserRepository repository) {
                    this.repository = repository;
                }

                public Optional<User> findById(Long id) {
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

        let markdown = extract_markdown(source).expect("Java source should render to Markdown");

        validate_markdown(&markdown).expect("generated Markdown should be structurally valid");
        assert!(markdown.contains("## Class `UserService`"));
        assert!(markdown.contains("### Fields"));
        assert!(markdown.contains("### Class `Inner`"));
        assert!(markdown.contains("## Enum `Status`"));
    }
}
