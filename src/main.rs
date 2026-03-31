mod codegen;
mod conditional;
mod errors;
mod lexer;
mod parser;
mod resolver;
mod span;
mod types;

use std::env;
use std::fs;
use std::collections::HashSet;
use std::path::Path;
use std::process::{self, Command};

fn macos_sdk_path() -> String {
    Command::new("xcrun")
        .args(["--show-sdk-path"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn macos_sdk_version() -> String {
    match Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-version"])
        .output()
    {
        Ok(output) => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if version.is_empty() {
                "15.0".to_string()
            } else {
                version
            }
        }
        Err(_) => "15.0".to_string(),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: elephc [--heap-size=BYTES] [--gc-stats] [--heap-debug] [--define SYMBOL] [--link LIB|-lLIB] [--link-path DIR|-LDIR] [--framework NAME] <source.php>");
        process::exit(1);
    }

    // Parse optional flags
    let mut heap_size: usize = 8_388_608; // 8MB default
    let mut gc_stats = false;
    let mut heap_debug = false;
    let mut filename_arg = None;
    let mut extra_link_libs: Vec<String> = Vec::new();
    let mut extra_link_paths: Vec<String> = Vec::new();
    let mut extra_frameworks: Vec<String> = Vec::new();
    let mut defines: HashSet<String> = HashSet::new();

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if let Some(val) = arg.strip_prefix("--heap-size=") {
            heap_size = match val.parse::<usize>() {
                Ok(n) if n >= 65536 => n,
                _ => {
                    eprintln!("Invalid --heap-size: must be a number >= 65536");
                    process::exit(1);
                }
            };
        } else if arg == "--gc-stats" {
            gc_stats = true;
        } else if arg == "--heap-debug" {
            heap_debug = true;
        } else if arg == "--define" {
            i += 1;
            if i < args.len() {
                defines.insert(args[i].clone());
            } else {
                eprintln!("Missing symbol after --define");
                process::exit(1);
            }
        } else if let Some(symbol) = arg.strip_prefix("--define=") {
            if symbol.is_empty() {
                eprintln!("Invalid --define: symbol cannot be empty");
                process::exit(1);
            }
            defines.insert(symbol.to_string());
        } else if arg == "--link" || arg == "-l" {
            i += 1;
            if i < args.len() {
                extra_link_libs.push(args[i].clone());
            } else {
                eprintln!("Missing library name after {}", arg);
                process::exit(1);
            }
        } else if let Some(lib) = arg.strip_prefix("-l") {
            extra_link_libs.push(lib.to_string());
        } else if arg == "--link-path" || arg == "-L" {
            i += 1;
            if i < args.len() {
                extra_link_paths.push(args[i].clone());
            } else {
                eprintln!("Missing path after {}", arg);
                process::exit(1);
            }
        } else if let Some(path) = arg.strip_prefix("-L") {
            extra_link_paths.push(path.to_string());
        } else if arg == "--framework" {
            i += 1;
            if i < args.len() {
                extra_frameworks.push(args[i].clone());
            } else {
                eprintln!("Missing framework name after --framework");
                process::exit(1);
            }
        } else if arg.starts_with("--") {
            eprintln!("Unknown flag: {}", arg);
            process::exit(1);
        } else {
            filename_arg = Some(arg.as_str());
        }
        i += 1;
    }

    let filename = match filename_arg {
        Some(f) => f,
        None => {
            eprintln!("Usage: elephc [--heap-size=BYTES] [--gc-stats] [--heap-debug] [--define SYMBOL] [--link LIB|-lLIB] [--link-path DIR|-LDIR] [--framework NAME] <source.php>");
            process::exit(1);
        }
    };
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let parent = Path::new(filename).parent().unwrap_or(Path::new("."));
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

    let parsed = conditional::apply(parsed, &defines);

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
        &check_result.interfaces,
        &check_result.classes,
        &check_result.extern_functions,
        &check_result.extern_classes,
        &check_result.extern_globals,
        heap_size,
        gc_stats,
        heap_debug,
    );

    // Merge extern-required libraries with CLI-specified ones
    for lib in &check_result.required_libraries {
        if !extra_link_libs.contains(lib) {
            extra_link_libs.push(lib.clone());
        }
    }

    if let Err(e) = fs::write(&asm_path, &asm) {
        eprintln!("Error writing '{}': {}", asm_path.display(), e);
        process::exit(1);
    }

    // Assemble
    let sdk_path = macos_sdk_path();
    let sdk_version = macos_sdk_version();

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
    let mut ld_cmd = Command::new("ld");
    ld_cmd.args(["-arch", "arm64", "-e", "_main", "-o"]);
    ld_cmd.arg(&bin_path);
    ld_cmd.arg(&obj_path);
    ld_cmd.args(["-lSystem", "-syslibroot"]);
    ld_cmd.arg(&sdk_path);
    ld_cmd.args(["-platform_version", "macos", &sdk_version, &sdk_version]);
    for path in &extra_link_paths {
        ld_cmd.arg(format!("-L{}", path));
    }
    for lib in &extra_link_libs {
        if lib != "System" {
            ld_cmd.arg(format!("-l{}", lib));
        }
    }
    for fw in &extra_frameworks {
        ld_cmd.args(["-framework", fw]);
    }
    let ld_status = ld_cmd.status();

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
