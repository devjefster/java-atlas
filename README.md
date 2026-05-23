# java-atlas

A small command-line tool that walks a Java codebase and prints a summary of every `.java` file it finds. Markdown
output is compact by default for developer scanning and AI context usage; JSON and TOML expose the full structured AST
with packages, imports, declarations, annotations, modifiers, Javadocs, type references, and members.

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
java-atlas [path] [--format markdown|json|toml]
```

- `path` is the codebase root directory to scan. If omitted, the current directory is used.
- The walker recurses through subdirectories, picks up every file with a `.java` extension, and skips any path component
  named `target`.
- If `path` is not a directory the tool prints an error and exits non-zero.
- `--format` (or `-f`) selects the output format; defaults to `markdown`. JSON emits a single pretty-printed array of
  `{ path, ast }` entries; TOML emits package-grouped `[[packages]]` sections with compact file entries and omitted
  empty collections. Redirect to a file to capture the output:

```bash
java-atlas ./my-service/src/main/java > atlas.md
java-atlas ./my-service/src/main/java --format json > atlas.json
java-atlas ./my-service/src/main/java --format toml > atlas.toml
```

## Example

Given:

```java
package com.example;

import java.util.Optional;

@Service
public class UserService<T extends AutoCloseable> {
    /** Repository storage. */
    @Inject
    private final UserRepository repository;

    @Autowired
    public UserService(UserRepository repository) throws ConfigurationException {
        this.repository = repository;
    }

    /**
     * Finds a user.
     *
     * @param id user id
     */
    @Deprecated
    public <E extends Exception> Optional<User> findById(@NotNull Long id) throws E {
        return Optional.empty();
    }
}
```

The tool emits a Markdown document with this shape:

```markdown
# Java Atlas

## `com.example`

- `UserService.java`
  - class: `@Service public UserService<T extends AutoCloseable>`
    - fields: `@Inject private final UserRepository repository` - Repository storage.
    - constructors: `@Autowired public UserService(UserRepository repository) throws ConfigurationException`
    - methods: `@Deprecated public <E extends Exception> Optional<User> findById(@NotNull Long id) throws E` - Finds a user.
```

Markdown groups files by package, omits imports and verbose Javadoc tags, and uses indented bullets to keep large
codebase summaries practical. Standard field accessors are folded into field signatures as `[getter]`, `[setter]`, or
`[add]`, and plain no-argument constructors are omitted. Leading Javadocs on types, fields, constructors, methods, and
annotation elements are captured as structured documentation; Markdown renders their descriptions inline, while
JSON/TOML retain descriptions and tags. Declaration and parameter annotations are rendered separately from Java keyword
modifiers.

The library model keeps Java type references and annotations structured. Type references distinguish primitives,
references, generics, arrays, wildcards, and type-use annotations. Javadocs expose a description plus block tags.
Annotations expose their name plus default or named arguments, including arrays, nested annotations, class literals,
primitive literals, references, and constant-expression text.

TOML output keeps the structured AST shape but groups files under `[[packages]]`, lists package files under
`[[packages.files]]`, shortens file paths relative to the package directory, and does not repeat the package name inside
each file. If a collection key is not present, consumers should treat it as empty; for example omitted
`type_parameters`, `extends`, `implements`, `constructors`, `enum_constants`, `record_components`, `annotation_elements`,
and `nested_types` fields are equivalent to `[]`. TOML also folds deterministic JavaBean-style accessors into field
`accessors` arrays, renders those accessors inline, and omits plain no-argument constructors. Source `range` and
`body_range` offsets remain present, rendered inline as two-number arrays.

## Scope

- Standard Java syntax only. The parser is `tree-sitter-java`; constructs that don't appear in the standard grammar
  aren't recognized.
- No framework awareness (no Spring, no Maven/Gradle resolution).
- Cross-file type reference resolution is limited to types found in the parsed file set.

## Architecture

The crate is a small library with a thin CLI on top:

- `src/main.rs` — CLI: argument parsing (`--format`), directory walking, reading, printing.
- `src/lib.rs` — declares the public `java_ast`, `markdown`, and `output` modules.
- `src/java_ast/` — Tree-sitter parsing and the data model. Public entry is
  `parse_java_file(source: &str) -> Result<JavaFile, JavaAstError>`. All model types derive `Serialize` so the same AST
  drives every output format.
- `src/output/` — multi-format rendering. Public entry is
  `render(&[FileOutput], Format) -> Result<String, OutputError>`. Submodules: `markdown`, `json`, `toml`.
- `src/markdown.rs` — generic Markdown validation built on `pulldown-cmark`, used by `output::markdown` to assert that
  the rendered output has a well-formed heading structure before it leaves the library.

See [CLAUDE.md](CLAUDE.md) and [AGENTS.md](AGENTS.md) for the longer architectural notes.

## Development

```bash
cargo check                            # type-check
cargo test                             # run unit + integration tests
cargo fmt --check                      # formatting
cargo clippy --all-targets -- -D warnings
```

There is no Makefile, Justfile, or CI workflow — use Cargo directly.
