use clap::Parser;
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Codebase root directory to scan
    path: Option<PathBuf>,
}

/// Walks the selected codebase root and prints Markdown for each Java file.
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

    for file_path in files_to_parse {
        match fs::read_to_string(&file_path) {
            Ok(source) => match java_atlas::java_ast::extract_markdown(&source) {
                Ok(markdown) => {
                    println!("--- File: {:?} ---", file_path);
                    println!("{}", markdown);
                }
                Err(e) => {
                    eprintln!("Error parsing {:?}: {}", file_path, e);
                }
            },
            Err(e) => {
                eprintln!("Error reading {:?}: {}", file_path, e);
            }
        }
    }
}
