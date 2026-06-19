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

use crate::cli::{CliConfig, CodegenBackend};
use crate::codegen::platform::{Platform, Target};
use crate::codegen::Emit;
use crate::timings::CompileTimings;
use crate::{
    autoload, codegen, codegen_ir, conditional, errors, exports, ir, ir_lower, ir_passes, lexer,
    linker, list_id_prelude, magic_constants, name_resolver, optimize, parser, pdo_prelude,
    resolver, runtime_cache, source_map, tz_prelude, types, var_export_prelude,
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
        emit_ir,
        backend,
        null_repr,
        emit_asm,
        emit,
        check_only,
        emit_timings,
        emit_source_map,
        regalloc_linear,
        ir_opt,
        target,
        mut extra_link_libs,
        extra_link_paths,
        extra_frameworks,
        defines,
    } = config;
    let filename = filename.as_str();
    codegen::set_null_repr(null_repr);
    let parent = Path::new(filename).parent().unwrap_or(Path::new("."));
    let output_paths = output_paths(filename, target, emit);
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

    // Inject the PDO standard-library prelude (extern bridge + PDO classes,
    // written in elephc-PHP) only when the program references PDO, so non-PDO
    // binaries never declare the elephc_pdo externs or link the bridge.
    // Runs after include resolution so PDO usage inside includes is detected.
    let phase_started = Instant::now();
    let ast = pdo_prelude::inject_if_used(ast);
    timings.record_since("pdo-prelude", phase_started);

    // Inject the timezone-introspection prelude (extern block + array marshalling,
    // written in elephc-PHP) only when the program references getLocation /
    // getTransitions / listAbbreviations or their procedural aliases, so other
    // binaries never declare the elephc_tz externs or link the bridge. Runs after
    // include resolution so usage inside includes is detected.
    let phase_started = Instant::now();
    let ast = tz_prelude::inject_if_used(ast);
    timings.record_since("tz-prelude", phase_started);

    // Inject the listIdentifiers-filtering prelude (a pure elephc-PHP function over
    // a baked group/country table) only when the program references
    // DateTimeZone::listIdentifiers or timezone_identifiers_list, so other binaries
    // never carry the table. Runs after include resolution so usage inside includes
    // is detected, and before name resolution, which desugars both call forms to it.
    let phase_started = Instant::now();
    let ast = list_id_prelude::inject_if_used(ast);
    timings.record_since("list-id-prelude", phase_started);

    // Inject the var_export prelude (a pure elephc-PHP function) only when the program
    // references var_export and does not declare its own, so other binaries carry
    // nothing. Runs after include resolution so usage inside includes is detected, and
    // before name resolution so the call resolves to the injected function.
    let phase_started = Instant::now();
    let ast = var_export_prelude::inject_if_used(ast);
    timings.record_since("var-export-prelude", phase_started);

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
    codegen::prepare_declared_name_order(
        &ast,
        &check_result.classes,
        &check_result.interfaces,
    );

    if !target.supports_current_backend() {
        eprintln!(
            "Target '{}' is recognized, but it is outside the current supported target matrix",
            target
        );
        process::exit(1);
    }

    let phase_started = Instant::now();
    let exported_functions = match exports::collect(&ast, &check_result.functions) {
        Ok(exports) => exports,
        Err(e) => {
            errors::report(&e.with_file(filename.to_string()));
            process::exit(1);
        }
    };
    timings.record_since("exports-scan", phase_started);
    if matches!(emit, Emit::Executable) && !exported_functions.is_empty() {
        let names: Vec<&str> = exported_functions.keys().map(String::as_str).collect();
        eprintln!(
            "warning: ignoring #[Export] on functions {:?} — --emit cdylib is required to expose them",
            names
        );
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

    if emit_ir {
        let phase_started = Instant::now();
        let mut module = match ir_lower::lower_program(&ast, &check_result, target) {
            Ok(module) => module,
            Err(err) => {
                eprintln!("EIR lowering error: {}", err);
                process::exit(1);
            }
        };
        timings.record_since("ir-lower", phase_started);

        let phase_started = Instant::now();
        if ir_opt {
            ir_passes::optimize_module(&mut module);
        }
        timings.record_since("ir-opt", phase_started);

        let phase_started = Instant::now();
        let text = ir::print_module(&module);
        timings.record_since("ir-print", phase_started);
        timings.report();
        print!("{}", text);
        return;
    }

    let ir_module = if matches!(backend, CodegenBackend::Eir) {
        let phase_started = Instant::now();
        let mut module = match ir_lower::lower_program(&ast, &check_result, target) {
            Ok(module) => module,
            Err(err) => {
                eprintln!("EIR lowering error: {}", err);
                process::exit(1);
            }
        };
        timings.record_since("ir-lower", phase_started);

        let phase_started = Instant::now();
        if ir_opt {
            ir_passes::optimize_module(&mut module);
        }
        timings.record_since("ir-opt", phase_started);
        Some(module)
    } else {
        None
    };

    let runtime_features = ir_module
        .as_ref()
        .map(|module| module.required_runtime_features)
        .unwrap_or_else(|| {
            codegen::runtime_features_for_program_and_classes(&ast, &check_result.classes)
        });

    let requires_elephc_tls = extra_link_libs.iter().any(|lib| lib == "elephc_tls")
        || check_result
            .required_libraries
            .iter()
            .any(|lib| lib == "elephc_tls");

    let phase_started = Instant::now();
    let runtime_pic = matches!(emit, Emit::Cdylib);
    let runtime_object = match runtime_cache::prepare_runtime_object(heap_size, target, runtime_features, runtime_pic) {
        Ok(runtime_object) => runtime_object,
        Err(err) => {
            eprintln!("Runtime cache error: {}", err);
            process::exit(1);
        }
    };
    timings.record_since("runtime-cache", phase_started);
    timings.note(format!("runtime-cache {}", runtime_object.status.as_str()));

    let phase_started = Instant::now();
    let codegen_timing = if ir_module.is_some() {
        "codegen-ir"
    } else {
        "codegen"
    };
    let user_asm = if let Some(module) = &ir_module {
        match codegen_ir::generate_user_asm_from_ir_with_options(
            module,
            gc_stats,
            heap_debug,
            requires_elephc_tls,
            emit,
            &exported_functions,
            regalloc_linear,
        ) {
            Ok(asm) => asm,
            Err(err) => {
                eprintln!("EIR backend error: {}", err);
                process::exit(1);
            }
        }
    } else {
        codegen::generate_user_asm(
            &ast,
            &check_result.global_env,
            &check_result.functions,
            &check_result.callable_param_sigs,
            &check_result.callable_return_sigs,
            &check_result.callable_array_return_sigs,
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
            requires_elephc_tls,
            null_repr,
            emit,
            &exported_functions,
        )
    };
    timings.record_since(codegen_timing, phase_started);

    for lib in &check_result.required_libraries {
        if !extra_link_libs.contains(lib) {
            extra_link_libs.push(lib.clone());
        }
    }
    for lib in codegen::required_libraries_for_runtime_features(runtime_features) {
        if !extra_link_libs.contains(&lib) {
            extra_link_libs.push(lib);
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
        emit,
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
///
/// Executable mode produces `<stem>` (no extension). Cdylib mode produces
/// `lib<stem>.so` (Linux) or `lib<stem>.dylib` (macOS), matching the conventional
/// shared-library naming that `dlopen(3)` and linker `-l` flags expect.
fn output_paths(filename: &str, target: Target, emit: Emit) -> OutputPaths {
    let path = Path::new(filename);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
    let parent = path.parent().unwrap_or(Path::new("."));
    let bin_name = match emit {
        Emit::Executable => stem.to_string(),
        Emit::Cdylib => match target.platform {
            Platform::MacOS => format!("lib{}.dylib", stem),
            Platform::Linux => format!("lib{}.so", stem),
        },
    };
    OutputPaths {
        asm: parent.join(format!("{}.s", stem)),
        obj: parent.join(format!("{}.o", stem)),
        bin: parent.join(bin_name),
        source_map: parent.join(format!("{}.map", stem)),
    }
}
