mod codegen;
mod errors;
mod lexer;
mod parser;
mod resolver;
mod span;
mod types;

use std::env;
use std::fs;
use std::path::Path;
use std::process::{self, Command};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: elephc [--heap-size=BYTES] <source.php>");
        process::exit(1);
    }

    // Parse optional flags
    let mut heap_size: usize = 8_388_608; // 8MB default
    let mut filename_arg = None;

    for arg in &args[1..] {
        if let Some(val) = arg.strip_prefix("--heap-size=") {
            heap_size = match val.parse::<usize>() {
                Ok(n) if n >= 65536 => n,
                _ => {
                    eprintln!("Invalid --heap-size: must be a number >= 65536");
                    process::exit(1);
                }
            };
        } else if arg.starts_with("--") {
            eprintln!("Unknown flag: {}", arg);
            process::exit(1);
        } else {
            filename_arg = Some(arg.as_str());
        }
    }

    let filename = match filename_arg {
        Some(f) => f,
        None => {
            eprintln!("Usage: elephc [--heap-size=BYTES] <source.php>");
            process::exit(1);
        }
    };
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let parent = Path::new(filename)
        .parent()
        .unwrap_or(Path::new("."));
    let asm_path = parent.join(format!("{}.s", stem));
    let obj_path = parent.join(format!("{}.o", stem));
    let bin_path = parent.join(stem);

    let source = match fs::read_to_string(filename) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading '{}': {}", filename, e);
            process::exit(1);
        }
    };

    let tokens = match lexer::tokenize(&source) {
        Ok(tokens) => tokens,
        Err(e) => {
            errors::report(&e);
            process::exit(1);
        }
    };

    let parsed = match parser::parse(&tokens) {
        Ok(ast) => ast,
        Err(e) => {
            errors::report(&e);
            process::exit(1);
        }
    };

    let ast = match resolver::resolve(parsed, parent) {
        Ok(resolved) => resolved,
        Err(e) => {
            errors::report(&e);
            process::exit(1);
        }
    };

    let check_result = match types::check(&ast) {
        Ok(result) => result,
        Err(e) => {
            errors::report(&e);
            process::exit(1);
        }
    };

    let asm = codegen::generate(
        &ast,
        &check_result.global_env,
        &check_result.functions,
        heap_size,
    );

    if let Err(e) = fs::write(&asm_path, &asm) {
        eprintln!("Error writing '{}': {}", asm_path.display(), e);
        process::exit(1);
    }

    // Assemble
    let sdk_path = Command::new("xcrun")
        .args(["--show-sdk-path"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let as_status = Command::new("as")
        .args(["-arch", "arm64", "-o"])
        .arg(&obj_path)
        .arg(&asm_path)
        .status();

    match as_status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("Assembler failed with exit code {}", s);
            process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to run assembler: {}", e);
            process::exit(1);
        }
    }

    // Link
    let ld_status = Command::new("ld")
        .args([
            "-arch", "arm64",
            "-e", "_main",
            "-o",
        ])
        .arg(&bin_path)
        .arg(&obj_path)
        .args(["-lSystem", "-syslibroot"])
        .arg(&sdk_path)
        .status();

    match ld_status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("Linker failed with exit code {}", s);
            process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to run linker: {}", e);
            process::exit(1);
        }
    }

    // Clean up intermediate files
    let _ = fs::remove_file(&obj_path);

    println!("Compiled '{}' -> '{}'", filename, bin_path.display());
}
