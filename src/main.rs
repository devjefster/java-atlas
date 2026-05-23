use clap::{Parser, ValueEnum};
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

use java_atlas::java_ast::{JavaFile, parse_java_file, resolve_files};
use java_atlas::output::{self, FileOutput, Format};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Codebase root directory to scan
    path: Option<PathBuf>,

    /// Output format
    #[arg(short = 'f', long, value_enum, default_value_t = OutputFormat::Markdown)]
    format: OutputFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Markdown,
    Json,
    Toml,
}

impl From<OutputFormat> for Format {
    fn from(value: OutputFormat) -> Self {
        match value {
            OutputFormat::Markdown => Format::Markdown,
            OutputFormat::Json => Format::Json,
            OutputFormat::Toml => Format::Toml,
        }
    }
}

/// Walks the selected codebase root and prints the parsed files in the chosen format.
fn main() {
    let args = Args::parse();

    let root = args.path.unwrap_or_else(|| PathBuf::from("."));
    if !root.is_dir() {
        eprintln!("Error: {} is not a directory.", root.display());
        std::process::exit(1);
    }

    let files_to_parse: Vec<PathBuf> = WalkDir::new(&root)
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
        println!("No Java files found to parse.");
        return;
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
        std::process::exit(1);
    }

    resolve_files(&mut asts);

    let outputs: Vec<FileOutput<'_>> = paths
        .iter()
        .zip(asts.iter())
        .map(|(path, ast)| FileOutput {
            path: path.as_path(),
            ast,
        })
        .collect();

    match output::render(&outputs, args.format.into()) {
        Ok(rendered) => println!("{}", rendered),
        Err(e) => {
            eprintln!("Error rendering output: {}", e);
            std::process::exit(1);
        }
    }
}
