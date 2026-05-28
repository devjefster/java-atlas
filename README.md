# java-atlas

`java-atlas` walks a Java source tree and builds a compact atlas for every `.java` file it finds. Markdown is optimized for
human scanning, JSON is a full structured export, and JSONL is designed for package-scoped loading and search workflows.

## Install

Requires a recent stable Rust toolchain with the 2024 edition.

```bash
cargo build --release
# binary at target/release/java-atlas
```

For local use from a checkout:

```bash
cargo run -- --help
```

Or install the binary into your Cargo bin directory:

```bash
cargo install --path .
```

## Usage

```bash
java-atlas init [path] [--out-dir .atlas]
java-atlas [path] [--format markdown|json|jsonl]
```

- `init` writes the recommended working set into `.atlas`: `atlas.md` plus package-scoped JSONL shards under
  `.atlas/packages`.
- `init` defaults `path` to `src/main/java`.
- Direct rendering scans the source tree at `path`. If `path` is omitted, the CLI tries to read existing `.atlas`
  artifacts where possible: Markdown prints `.atlas/atlas.md`, and JSONL concatenates `.atlas/packages/**/*.jsonl`.
- JSON has no default `.atlas` artifact; pass a source path when using `--format json`.
- `--format` or `-f` selects the direct output format and defaults to `markdown`.
- `target` directories are skipped while walking.
- If `path` is not a directory, the CLI exits non-zero with an error.

### Common Workflow

For a real codebase under `~/projects/flxpoint`, point `java-atlas` at the Java source root, not the repository root:

```bash
java-atlas init ~/projects/flxpoint/my-service/src/main/java
java-atlas ~/projects/flxpoint/my-service/src/main/java > atlas.md
java-atlas ~/projects/flxpoint/my-service/src/main/java --format json > atlas.json
java-atlas ~/projects/flxpoint/my-service/src/main/java --format jsonl > atlas.jsonl
```

If you want the generated artifacts inside the repo:

```bash
java-atlas init ~/projects/flxpoint/my-service/src/main/java --out-dir ~/projects/flxpoint/my-service/.atlas
```

## Output Shapes

- Markdown emits one `# Java Atlas` document with `##` sections per package and file bullets underneath.
- JSON emits a single pretty-printed array of `{ path, ast }` entries.
- JSONL emits one compact file record per line, grouped by package when written through `init`.

Markdown output omits imports and verbose Javadoc tags, folds standard field accessors into field signatures as
`[getter]`, `[setter]`, or `[add]`, and skips plain no-argument constructors. Leading Javadocs on types, fields,
constructors, methods, and annotation elements are captured as structured documentation; Markdown renders their
descriptions inline, while JSON and JSONL retain descriptions and tags.

JSONL omits empty collections and null optional fields. If a key is missing, consumers should treat that collection as
empty. Full JSON keeps the raw structured AST for tooling that needs complete parser output.

## Scope

- Standard Java syntax only. The parser is `tree-sitter-java`; constructs outside the standard grammar are not
  recognized.
- No framework awareness, Maven/Gradle resolution, or multi-file type expansion.
- Cross-file type reference resolution is limited to types found in the parsed file set.

## Architecture

The crate is a small library with a thin CLI on top:

- `src/main.rs` - CLI: argument parsing, directory walking, reading, printing, and `.atlas` writes.
- `src/lib.rs` - declares the public `java_ast`, `markdown`, and `output` modules.
- `src/java_ast/` - Tree-sitter parsing and the data model. Public entry is
  `parse_java_file(source: &str) -> Result<JavaFile, JavaAstError>`.
- `src/output/` - multi-format rendering. Public entry is
  `render(&[FileOutput], Format) -> Result<String, OutputError>`.
- `src/markdown.rs` - generic Markdown validation built on `pulldown-cmark`, used by `output::markdown` before the
  rendered output leaves the library.

## Development

```bash
cargo check
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

There is no Makefile, Justfile, or CI workflow. Use Cargo directly.
