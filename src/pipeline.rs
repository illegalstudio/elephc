//! Purpose:
//! Orchestrates the full PHP source to native binary compilation flow.
//! Runs frontend passes, semantic checks, optimizations, runtime preparation, codegen, and linking in order.
//!
//! Called from:
//! - `crate::main()` after `crate::cli::parse_args()`.
//!
//! Key details:
//! - Pass ordering is observable: magic constants and conditionals run before resolver/name resolution and type checking.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::Instant;

use crate::cli::CliConfig;
use crate::timings::CompileTimings;
use crate::{
    autoload, codegen, conditional, errors, lexer, linker, magic_constants, name_resolver,
    optimize, parser, resolver, runtime_cache, source_map, types,
};

/// Holds the paths for all compilation output files (assembly, object, binary, source map).
struct OutputPaths {
    asm: PathBuf,
    obj: PathBuf,
    bin: PathBuf,
    source_map: PathBuf,
}

/// Runs the full compilation pipeline from PHP source to native binary.
/// Reads PHP source, tokenizes, parses, resolves names, type-checks, optimizes,
/// generates assembly, and links into a native binary. Exits on any error.
pub(crate) fn compile(config: CliConfig) {
    let CliConfig {
        filename,
        heap_size,
        gc_stats,
        heap_debug,
        emit_asm,
        check_only,
        emit_timings,
        emit_source_map,
        target,
        mut extra_link_libs,
        extra_link_paths,
        extra_frameworks,
        defines,
    } = config;
    let filename = filename.as_str();
    let parent = Path::new(filename).parent().unwrap_or(Path::new("."));
    let output_paths = output_paths(filename);
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

    let phase_started = Instant::now();
    let main_file_path = Path::new(filename).to_path_buf();
    let parsed = magic_constants::substitute_file_and_scope_constants(parsed, &main_file_path);
    timings.record_since("magic-constants", phase_started);

    let parsed = conditional::apply(parsed, &defines);

    let phase_started = Instant::now();
    let (autoload_registry, parsed) = autoload::Registry::build(parent, parsed);
    codegen::set_autoload_rule_count(autoload_registry.rule_count());
    for warning in autoload_registry.warnings() {
        errors::report_warning(warning);
    }
    timings.record_since("autoload-build", phase_started);

    let phase_started = Instant::now();
    let ast = match resolver::resolve(parsed, parent) {
        Ok(resolved) => resolved,
        Err(e) => {
            errors::report(&e);
            process::exit(1);
        }
    };
    let ast = autoload::collect_aliases(ast);
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
    let ast = match autoload::run(ast, parent, &autoload_registry) {
        Ok(resolved) => resolved,
        Err(e) => {
            errors::report(&e);
            process::exit(1);
        }
    };
    timings.record_since("autoload-run", phase_started);

    let phase_started = Instant::now();
    let ast = optimize::fold_constants(ast);
    timings.record_since("opt-fold", phase_started);

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
    let ast = optimize::propagate_constants(ast);
    timings.record_since("opt-prop", phase_started);

    let phase_started = Instant::now();
    let ast = optimize::prune_constant_control_flow(ast);
    timings.record_since("opt-post", phase_started);

    let phase_started = Instant::now();
    let ast = optimize::normalize_control_flow(ast);
    timings.record_since("opt-norm", phase_started);

    let phase_started = Instant::now();
    let ast = optimize::eliminate_dead_code(ast);
    timings.record_since("dce", phase_started);

    let phase_started = Instant::now();
    let runtime_object = match runtime_cache::prepare_runtime_object(heap_size, target) {
        Ok(runtime_object) => runtime_object,
        Err(err) => {
            eprintln!("Runtime cache error: {}", err);
            process::exit(1);
        }
    };
    timings.record_since("runtime-cache", phase_started);
    timings.note(format!("runtime-cache {}", runtime_object.status.as_str()));

    let phase_started = Instant::now();
    let user_asm = codegen::generate_user_asm(
        &ast,
        &check_result.global_env,
        &check_result.functions,
        &check_result.callable_param_sigs,
        &check_result.callable_return_sigs,
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

    for lib in &check_result.required_libraries {
        if !extra_link_libs.contains(lib) {
            extra_link_libs.push(lib.clone());
        }
    }

    let phase_started = Instant::now();
    if let Err(e) = fs::write(&output_paths.asm, &user_asm) {
        eprintln!("Error writing '{}': {}", output_paths.asm.display(), e);
        process::exit(1);
    }
    timings.record_since("write-asm", phase_started);

    if emit_source_map {
        let phase_started = Instant::now();
        if let Err(err) =
            source_map::write_source_map(&user_asm, Path::new(filename), &output_paths.source_map)
        {
            eprintln!("Source map error: {}", err);
            process::exit(1);
        }
        timings.record_since("source-map", phase_started);
    }

    if emit_asm {
        timings.report();
        println!(
            "Emitted assembly '{}' -> '{}'",
            filename,
            output_paths.asm.display()
        );
        return;
    }

    let phase_started = Instant::now();
    linker::assemble(target, &output_paths.asm, &output_paths.obj);
    timings.record_since("assemble", phase_started);

    let phase_started = Instant::now();
    linker::link(
        target,
        &output_paths.bin,
        &output_paths.obj,
        &runtime_object.path,
        &extra_link_libs,
        &extra_link_paths,
        &extra_frameworks,
    );
    timings.record_since("link", phase_started);

    let _ = fs::remove_file(&output_paths.obj);

    timings.report();
    println!("Compiled '{}' -> '{}'", filename, output_paths.bin.display());
}

/// Computes output paths for .s (assembly), .o (object), binary, and .map (source map) files
/// derived from the input filename.
fn output_paths(filename: &str) -> OutputPaths {
    let path = Path::new(filename);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
    let parent = path.parent().unwrap_or(Path::new("."));
    OutputPaths {
        asm: parent.join(format!("{}.s", stem)),
        obj: parent.join(format!("{}.o", stem)),
        bin: parent.join(stem),
        source_map: parent.join(format!("{}.map", stem)),
    }
}
