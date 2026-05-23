//! Markdown validation utilities built on pulldown-cmark.

use std::fmt;

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownError {
    EmptyHeading {
        level: u8,
    },
    HeadingLevelJump {
        previous: u8,
        current: u8,
        text: String,
    },
    NoHeadings,
    UnmatchedHeadingEnd,
    UnterminatedHeading,
}

impl fmt::Display for MarkdownError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MarkdownError::EmptyHeading { level } => write!(f, "empty h{} heading", level),
            MarkdownError::HeadingLevelJump {
                previous,
                current,
                text,
            } => write!(
                f,
                "heading level jumps from h{} to h{} at `{}`",
                previous, current, text
            ),
            MarkdownError::NoHeadings => write!(f, "markdown contains no headings"),
            MarkdownError::UnmatchedHeadingEnd => write!(f, "heading ended without a start event"),
            MarkdownError::UnterminatedHeading => write!(f, "heading started without an end event"),
        }
    }
}

impl std::error::Error for MarkdownError {}

/// Validates Markdown structure expected from java-atlas output.
pub fn validate_markdown(markdown: &str) -> Result<(), MarkdownError> {
    let parser = Parser::new_ext(markdown, Options::ENABLE_TABLES);
    let mut current_heading: Option<(HeadingLevel, String)> = None;
    let mut previous_heading_level = None;
    let mut heading_count = 0;

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                current_heading = Some((level, String::new()));
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, heading)) = &mut current_heading {
                    heading.push_str(&text);
                }
            }
            Event::End(TagEnd::Heading(level)) => {
                let Some((start_level, text)) = current_heading.take() else {
                    return Err(MarkdownError::UnmatchedHeadingEnd);
                };
                let level = heading_level(level.min(start_level));
                let text = text.trim().to_string();
                if text.is_empty() {
                    return Err(MarkdownError::EmptyHeading { level });
                }
                if let Some(previous) = previous_heading_level
                    && level > previous + 1
                {
                    return Err(MarkdownError::HeadingLevelJump {
                        previous,
                        current: level,
                        text,
                    });
                }
                previous_heading_level = Some(level);
                heading_count += 1;
            }
            _ => {}
        }
    }

    if current_heading.is_some() {
        return Err(MarkdownError::UnterminatedHeading);
    }
    if heading_count == 0 {
        return Err(MarkdownError::NoHeadings);
    }

    Ok(())
}

/// Converts pulldown-cmark heading levels into compact numeric values.
fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::{MarkdownError, validate_markdown};

    #[test]
    fn validates_basic_generated_markdown_shape() {
        let markdown = "# Java Atlas\n\n## Class `User`\n\n### Methods\n\n#### `findById`\n";

        validate_markdown(markdown).expect("generated Markdown should be structurally valid");
    }

    #[test]
    fn rejects_empty_headings() {
        let error = validate_markdown("# Java Atlas\n\n##\n\nbody").unwrap_err();

        assert_eq!(error, MarkdownError::EmptyHeading { level: 2 });
    }

    #[test]
    fn rejects_upward_heading_level_jumps() {
        let error = validate_markdown("# Java Atlas\n\n### Methods\n").unwrap_err();

        assert_eq!(
            error,
            MarkdownError::HeadingLevelJump {
                previous: 1,
                current: 3,
                text: "Methods".to_string(),
            }
        );
    }
}
