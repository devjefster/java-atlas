use clap::{Parser, Subcommand, ValueEnum};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use java_atlas::java_ast::{JavaFile, parse_java_file, resolve_files};
use java_atlas::output::{self, FileOutput, Format};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Codebase root directory to scan
    path: Option<PathBuf>,

    /// Output format
    #[arg(short = 'f', long, value_enum, default_value_t = OutputFormat::Markdown)]
    format: OutputFormat,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Generate .atlas artifacts for a Java source tree
    Init {
        /// Java source root to scan
        path: Option<PathBuf>,

        /// Directory where atlas artifacts are written
        #[arg(long, default_value = ".atlas")]
        out_dir: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Markdown,
    Json,
    Jsonl,
}

impl From<OutputFormat> for Format {
    fn from(value: OutputFormat) -> Self {
        match value {
            OutputFormat::Markdown => Format::Markdown,
            OutputFormat::Json => Format::Json,
            OutputFormat::Jsonl => Format::Jsonl,
        }
    }
}

/// Walks the selected codebase root and prints the parsed files in the chosen format.
fn main() {
    let args = Args::parse();
    if let Err(e) = run(args) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), String> {
    match args.command {
        Some(Command::Init { path, out_dir }) => {
            let root = path.unwrap_or_else(|| PathBuf::from("src/main/java"));
            init_atlas(&root, &out_dir)
        }
        None => render_to_stdout(args.path, args.format),
    }
}

fn render_to_stdout(path: Option<PathBuf>, format: OutputFormat) -> Result<(), String> {
    let Some(root) = path else {
        return print_existing_atlas(format);
    };
    let parsed = parse_codebase(&root)?;
    let outputs = parsed.outputs();
    let rendered = output::render(&outputs, format.into())
        .map_err(|e| format!("Error rendering output: {e}"))?;
    print!("{rendered}");
    Ok(())
}

fn init_atlas(root: &Path, out_dir: &Path) -> Result<(), String> {
    let parsed = parse_codebase(root)?;
    let outputs = parsed.outputs();

    fs::create_dir_all(out_dir)
        .map_err(|e| format!("Error creating {}: {e}", out_dir.display()))?;
    let markdown = output::render(&outputs, Format::Markdown)
        .map_err(|e| format!("Error rendering Markdown output: {e}"))?;
    fs::write(out_dir.join("atlas.md"), markdown)
        .map_err(|e| format!("Error writing {}: {e}", out_dir.join("atlas.md").display()))?;

    let packages_dir = out_dir.join("packages");
    fs::create_dir_all(&packages_dir)
        .map_err(|e| format!("Error creating {}: {e}", packages_dir.display()))?;
    for package in output::render_jsonl_packages(&outputs)
        .map_err(|e| format!("Error rendering JSONL output: {e}"))?
    {
        let path = packages_dir.join(&package.relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Error creating {}: {e}", parent.display()))?;
        }
        fs::write(&path, package.contents)
            .map_err(|e| format!("Error writing {}: {e}", path.display()))?;
    }

    println!(
        "Wrote atlas artifacts to {} from {}.",
        out_dir.display(),
        root.display()
    );
    Ok(())
}

fn print_existing_atlas(format: OutputFormat) -> Result<(), String> {
    let atlas_dir = PathBuf::from(".atlas");
    match format {
        OutputFormat::Markdown => {
            let path = atlas_dir.join("atlas.md");
            let contents = fs::read_to_string(&path).map_err(|e| {
                format!(
                    "Error reading {}: {e}. Run `java-atlas init` first.",
                    path.display()
                )
            })?;
            print!("{contents}");
            Ok(())
        }
        OutputFormat::Jsonl => {
            let packages_dir = atlas_dir.join("packages");
            let mut package_files = WalkDir::new(&packages_dir)
                .into_iter()
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.into_path())
                .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "jsonl"))
                .collect::<Vec<_>>();
            package_files.sort();
            if package_files.is_empty() {
                return Err(format!(
                    "No JSONL packages found in {}. Run `java-atlas init` first.",
                    packages_dir.display()
                ));
            }
            for path in package_files {
                let contents = fs::read_to_string(&path)
                    .map_err(|e| format!("Error reading {}: {e}", path.display()))?;
                print!("{contents}");
            }
            Ok(())
        }
        OutputFormat::Json => Err(
            "No default .atlas artifact exists for this format. Pass a source path, or use `--format jsonl` after `java-atlas init`."
                .to_string(),
        ),
    }
}

fn parse_codebase(root: &Path) -> Result<ParsedCodebase, String> {
    if !root.is_dir() {
        return Err(format!("Error: {} is not a directory.", root.display()));
    }

    let files_to_parse: Vec<PathBuf> = WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path();
            // Avoid parsing generated build output when scanning a codebase root.
            p.is_file()
                && p.extension().is_some_and(|ext| ext == "java")
                && !p.components().any(|c| c.as_os_str() == "target")
        })
        .map(|e| e.into_path())
        .collect();

    if files_to_parse.is_empty() {
        return Err("No Java files found to parse.".to_string());
    }

    let mut paths: Vec<PathBuf> = Vec::with_capacity(files_to_parse.len());
    let mut asts: Vec<JavaFile> = Vec::with_capacity(files_to_parse.len());
    for file_path in files_to_parse {
        match fs::read_to_string(&file_path) {
            Ok(source) => match parse_java_file(&source) {
                Ok(ast) => {
                    paths.push(file_path);
                    asts.push(ast);
                }
                Err(e) => eprintln!("Error parsing {:?}: {}", file_path, e),
            },
            Err(e) => eprintln!("Error reading {:?}: {}", file_path, e),
        }
    }

    if asts.is_empty() {
        return Err("No Java files were parsed successfully.".to_string());
    }

    resolve_files(&mut asts);

    Ok(ParsedCodebase { paths, asts })
}

struct ParsedCodebase {
    paths: Vec<PathBuf>,
    asts: Vec<JavaFile>,
}

impl ParsedCodebase {
    fn outputs(&self) -> Vec<FileOutput<'_>> {
        self.paths
            .iter()
            .zip(self.asts.iter())
            .map(|(path, ast)| FileOutput {
                path: path.as_path(),
                ast,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{init_atlas, parse_codebase};

    fn temp_path(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("java-atlas-{name}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn parse_codebase_uses_java_files_under_root() {
        let root = temp_path("parse");
        let source_dir = root.join("src/main/java/com/example");
        std::fs::create_dir_all(&source_dir).expect("create source dir");
        std::fs::write(
            source_dir.join("User.java"),
            "package com.example; public class User {}",
        )
        .expect("write source");

        let parsed = parse_codebase(&root.join("src/main/java")).expect("parse codebase");

        assert_eq!(parsed.paths.len(), 1);
        assert_eq!(parsed.asts[0].package.as_deref(), Some("com.example"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn init_atlas_writes_markdown_and_package_jsonl() {
        let root = temp_path("init-root");
        let source_dir = root.join("src/main/java/com/example");
        let out_dir = temp_path("init-out");
        std::fs::create_dir_all(&source_dir).expect("create source dir");
        std::fs::write(
            source_dir.join("User.java"),
            r#"
            package com.example;
            public class User {
                private String name;
                public String getName() { return name; }
            }
            "#,
        )
        .expect("write source");

        init_atlas(&root.join("src/main/java"), &out_dir).expect("init atlas");

        let markdown = std::fs::read_to_string(out_dir.join("atlas.md")).expect("read markdown");
        let jsonl = std::fs::read_to_string(out_dir.join("packages/com/example.jsonl"))
            .expect("read jsonl");
        assert!(markdown.contains("## `com.example`"));
        assert!(jsonl.contains("\"path\":\"User.java\""));
        assert!(jsonl.contains("\"accessors\":[\"getter\"]"));
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(out_dir);
    }
}
