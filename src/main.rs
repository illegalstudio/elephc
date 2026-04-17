mod codegen;
mod conditional;
mod errors;
mod lexer;
mod name_resolver;
mod names;
mod parser;
mod resolver;
mod runtime_cache;
mod source_map;
mod span;
mod types;

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::Path;
use std::process::{self, Command};
use std::time::{Duration, Instant};

use codegen::platform::{Platform, Target};

const USAGE: &str = "Usage: elephc [--target TARGET] [--heap-size=BYTES] [--gc-stats] [--heap-debug] [--emit-asm] [--check] [--timings] [--source-map] [--define SYMBOL] [--link LIB|-lLIB] [--link-path DIR|-LDIR] [--framework NAME] <source.php>";

struct CompileTimings {
    enabled: bool,
    started_at: Instant,
    notes: Vec<String>,
    phases: Vec<(&'static str, Duration)>,
}

impl CompileTimings {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            started_at: Instant::now(),
            notes: Vec::new(),
            phases: Vec::new(),
        }
    }

    fn record_since(&mut self, phase: &'static str, started_at: Instant) {
        if self.enabled {
            self.phases.push((phase, started_at.elapsed()));
        }
    }

    fn note(&mut self, note: impl Into<String>) {
        if self.enabled {
            self.notes.push(note.into());
        }
    }

    fn report(&self) {
        if !self.enabled {
            return;
        }

        eprintln!("Compiler timings:");
        for note in &self.notes {
            eprintln!("  {}", note);
        }
        for (phase, duration) in &self.phases {
            eprintln!("  {:<12} {:>8.2} ms", phase, duration.as_secs_f64() * 1000.0);
        }
        eprintln!(
            "  {:<12} {:>8.2} ms",
            "total",
            self.started_at.elapsed().as_secs_f64() * 1000.0
        );
    }
}

fn run_tool(name: &str, cmd: &mut Command) {
    match cmd.status() {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("{} failed with exit code {}", name, s);
            process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to run {}: {}", name, e);
            process::exit(1);
        }
    }
}

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
        eprintln!("{USAGE}");
        process::exit(1);
    }

    // Parse optional flags
    let mut heap_size: usize = 8_388_608; // 8MB default
    let mut gc_stats = false;
    let mut heap_debug = false;
    let mut emit_asm = false;
    let mut check_only = false;
    let mut emit_timings = false;
    let mut emit_source_map = false;
    let mut filename_arg = None;
    let mut target = Target::detect_host();
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
        } else if arg == "--target" {
            i += 1;
            if i < args.len() {
                target = match Target::parse(&args[i]) {
                    Ok(target) => target,
                    Err(err) => {
                        eprintln!("{}", err);
                        process::exit(1);
                    }
                };
            } else {
                eprintln!("Missing target after --target");
                process::exit(1);
            }
        } else if let Some(value) = arg.strip_prefix("--target=") {
            target = match Target::parse(value) {
                Ok(target) => target,
                Err(err) => {
                    eprintln!("{}", err);
                    process::exit(1);
                }
            };
        } else if arg == "--gc-stats" {
            gc_stats = true;
        } else if arg == "--heap-debug" {
            heap_debug = true;
        } else if arg == "--emit-asm" {
            emit_asm = true;
        } else if arg == "--check" {
            check_only = true;
        } else if arg == "--timings" {
            emit_timings = true;
        } else if arg == "--source-map" {
            emit_source_map = true;
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
            eprintln!("{USAGE}");
            process::exit(1);
        }
    };
    if emit_asm && check_only {
        eprintln!("--emit-asm and --check are mutually exclusive");
        process::exit(1);
    }
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let parent = Path::new(filename).parent().unwrap_or(Path::new("."));
    let asm_path = parent.join(format!("{}.s", stem));
    let obj_path = parent.join(format!("{}.o", stem));
    let bin_path = parent.join(stem);
    let source_map_path = parent.join(format!("{}.map", stem));
    let mut timings = CompileTimings::new(emit_timings);

    let phase_started = Instant::now();
    let source = match fs::read_to_string(filename) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading '{}': {}", filename, e);
            process::exit(1);
        }
    };
    timings.record_since("read", phase_started);

    let phase_started = Instant::now();
    let tokens = match lexer::tokenize(&source) {
        Ok(tokens) => tokens,
        Err(e) => {
            errors::report(&e.with_file(filename.to_string()));
            process::exit(1);
        }
    };
    timings.record_since("tokenize", phase_started);

    let phase_started = Instant::now();
    let parsed = match parser::parse(&tokens) {
        Ok(ast) => ast,
        Err(e) => {
            errors::report(&e.with_file(filename.to_string()));
            process::exit(1);
        }
    };
    timings.record_since("parse", phase_started);

    let parsed = conditional::apply(parsed, &defines);

    let phase_started = Instant::now();
    let ast = match resolver::resolve(parsed, parent) {
        Ok(resolved) => resolved,
        Err(e) => {
            errors::report(&e);
            process::exit(1);
        }
    };
    timings.record_since("resolve", phase_started);

    let phase_started = Instant::now();
    let ast = match name_resolver::resolve(ast) {
        Ok(resolved) => resolved,
        Err(e) => {
            errors::report(&e);
            process::exit(1);
        }
    };
    timings.record_since("name-resolve", phase_started);

    let phase_started = Instant::now();
    let check_result = match types::check_with_target(&ast, target) {
        Ok(result) => result,
        Err(e) => {
            errors::report(&e);
            process::exit(1);
        }
    };
    timings.record_since("typecheck", phase_started);
    for warning in &check_result.warnings {
        errors::report_warning(warning);
    }

    if !target.supports_current_backend() {
        eprintln!(
            "Target '{}' is recognized, but it is outside the current supported target matrix",
            target
        );
        process::exit(1);
    }

    if check_only {
        timings.report();
        println!("Checked '{}'", filename);
        return;
    }

    let phase_started = Instant::now();
    let runtime_object = match runtime_cache::prepare_runtime_object(heap_size, target) {
        Ok(runtime_object) => runtime_object,
        Err(err) => {
            eprintln!("Runtime cache error: {}", err);
            process::exit(1);
        }
    };
    timings.record_since("runtime-cache", phase_started);
    timings.note(format!(
        "runtime-cache {}",
        runtime_object.status.as_str()
    ));

    let phase_started = Instant::now();
    let user_asm = codegen::generate_user_asm(
        &ast,
        &check_result.global_env,
        &check_result.functions,
        &check_result.interfaces,
        &check_result.classes,
        &check_result.enums,
        &check_result.packed_classes,
        &check_result.extern_functions,
        &check_result.extern_classes,
        &check_result.extern_globals,
        heap_size,
        gc_stats,
        heap_debug,
        target,
    );
    timings.record_since("codegen", phase_started);

    // Merge extern-required libraries with CLI-specified ones
    for lib in &check_result.required_libraries {
        if !extra_link_libs.contains(lib) {
            extra_link_libs.push(lib.clone());
        }
    }

    let phase_started = Instant::now();
    if let Err(e) = fs::write(&asm_path, &user_asm) {
        eprintln!("Error writing '{}': {}", asm_path.display(), e);
        process::exit(1);
    }
    timings.record_since("write-asm", phase_started);

    if emit_source_map {
        let phase_started = Instant::now();
        if let Err(err) = source_map::write_source_map(&user_asm, Path::new(filename), &source_map_path) {
            eprintln!("Source map error: {}", err);
            process::exit(1);
        }
        timings.record_since("source-map", phase_started);
    }

    if emit_asm {
        timings.report();
        println!("Emitted assembly '{}' -> '{}'", filename, asm_path.display());
        return;
    }

    // Assemble
    let phase_started = Instant::now();
    let mut as_cmd = Command::new(target.assembler_cmd());
    if target.platform == Platform::MacOS {
        as_cmd.args(["-arch", target.darwin_arch_name()]);
    }
    as_cmd.arg("-o").arg(&obj_path).arg(&asm_path);
    run_tool("Assembler", &mut as_cmd);
    timings.record_since("assemble", phase_started);

    // Link
    let phase_started = Instant::now();
    let mut ld_cmd = match target.platform {
        Platform::MacOS => {
            let sdk_path = macos_sdk_path();
            let sdk_version = macos_sdk_version();
            let mut cmd = Command::new("ld");
            cmd.args(["-arch", target.darwin_arch_name(), "-e", "_main", "-o"]);
            cmd.arg(&bin_path);
            cmd.arg(&obj_path);
            cmd.arg(&runtime_object.path);
            cmd.args(["-lSystem", "-syslibroot"]);
            cmd.arg(&sdk_path);
            cmd.args(["-platform_version", "macos", &sdk_version, &sdk_version]);
            cmd
        }
        Platform::Linux => {
            let mut cmd = Command::new(target.linker_cmd());
            cmd.arg("-o").arg(&bin_path).arg(&obj_path).arg(&runtime_object.path);
            if extra_link_libs.is_empty() {
                cmd.arg("-static");
            }
            if !extra_link_libs.is_empty() {
                cmd.arg("-Wl,--no-as-needed");
            }
            cmd.args(["-lm", "-lpthread"]);
            cmd
        }
    };
    for path in &extra_link_paths {
        ld_cmd.arg(format!("-L{}", path));
    }
    for lib in &extra_link_libs {
        if lib != "System" {
            ld_cmd.arg(format!("-l{}", lib));
        }
    }
    if target.platform == Platform::Linux && !extra_link_libs.is_empty() {
        ld_cmd.arg("-Wl,--as-needed");
    }
    if target.platform == Platform::MacOS {
        for fw in &extra_frameworks {
            ld_cmd.args(["-framework", fw]);
        }
    }
    run_tool("Linker", &mut ld_cmd);
    timings.record_since("link", phase_started);

    // Clean up intermediate files
    let _ = fs::remove_file(&obj_path);

    timings.report();
    println!("Compiled '{}' -> '{}'", filename, bin_path.display());
}
