# java-atlas

A small command-line tool that walks a Java codebase and prints a Markdown summary of every `.java` file it finds: package, imports, and the shape of each declared type (class, interface, enum, record, annotation) with its modifiers, supertypes, fields, constructors, methods, and kind-specific members.

## Install

Requires a recent stable Rust toolchain (2024 edition).

```bash
cargo build --release
# binary at target/release/java-atlas
```

Or run directly from a checkout:

```bash
cargo run -- [path]
```

## Usage

```bash
java-atlas [path]
```

- `path` is the codebase root directory to scan. If omitted, the current directory is used.
- The walker recurses through subdirectories, picks up every file with a `.java` extension, and skips any path component named `target`.
- If `path` is not a directory the tool prints an error and exits non-zero.

For each Java file, the rendered Markdown is printed to stdout, preceded by a `--- File: "<path>" ---` header. Redirect to a file to capture the output:

```bash
java-atlas ./my-service/src/main/java > atlas.md
```

## Example

Given:

```java
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
}
```

The tool emits a Markdown document with this shape:

```markdown
# Java Atlas

**Package:** `com.example`

## Imports

- `java.util.Optional`

## Class `UserService`

### Fields

| Modifiers | Type | Name |
| --- | --- | --- |
| `private final` | `UserRepository` | `repository` |

### Constructors

#### `UserService`

**Arguments:** `UserRepository repository`

### Methods

#### `findById`

**Returns:** `Optional<User>`
**Arguments:** `Long id`
```

Nested types are rendered recursively at the next heading depth.

## Scope

- Standard Java syntax only. The parser is `tree-sitter-java`; constructs that don't appear in the standard grammar aren't recognized.
- No framework awareness (no Spring, no Maven/Gradle resolution).
- No cross-file linking — every file is summarized in isolation.

## Architecture

The crate is a small library with a thin CLI on top:

- `src/main.rs` — CLI: argument parsing, directory walking, reading, printing.
- `src/lib.rs` — declares the public `java_ast` and `markdown` modules.
- `src/java_ast/` — Tree-sitter parsing and Markdown rendering. Public entry is `extract_markdown(source: &str) -> Result<String, JavaAstError>`, which runs `parse_java_file` → `render_markdown` → `markdown::validate_markdown` and returns the validated output.
- `src/markdown.rs` — generic Markdown validation built on `pulldown-cmark`, used to assert that the rendered output has a well-formed heading structure before it leaves the library.

See [CLAUDE.md](CLAUDE.md) and [AGENTS.md](AGENTS.md) for the longer architectural notes.

## Development

```bash
cargo check                            # type-check
cargo test                             # run unit + integration tests
cargo fmt --check                      # formatting
cargo clippy --all-targets -- -D warnings
```

There is no Makefile, Justfile, or CI workflow — use Cargo directly.
