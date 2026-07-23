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
use crate::codegen::platform::{Platform, Target};
use crate::codegen::Emit;
use crate::timings::CompileTimings;
use crate::{
    autoload, codegen, conditional, debug_info, errors, exports, ir, ir_lower, ir_passes, lexer,
    linker, list_id_prelude, magic_constants, name_resolver, optimize, parser, pdo_prelude,
    resolver, runtime_cache, source_map, tz_prelude, types, var_export_prelude, web_prelude,
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
        null_repr,
        emit_asm,
        emit,
        check_only,
        emit_timings,
        emit_source_map,
        emit_debug_info,
        regalloc_linear,
        ir_opt,
        target,
        php_version,
        mut extra_link_libs,
        extra_link_paths,
        extra_frameworks,
        defines,
        strict_php,
        web,
        with_crates,
        quiet,
    } = config;
    let filename = filename.as_str();
    crate::progress::init(quiet);
    codegen::set_null_repr(null_repr);
    crate::strict_php::set_enabled(strict_php);
    let parent = Path::new(filename).parent().unwrap_or(Path::new("."));
    let output_paths = output_paths(filename, target, emit);
    let mut timings = CompileTimings::new(emit_timings);

    crate::progress::phase("read");
    let phase_started = Instant::now();
    let source = match fs::read_to_string(filename) {
        Ok(s) => s,
        Err(e) => {
            crate::progress::clear();
            eprintln!("Error reading '{}': {}", filename, e);
            process::exit(1);
        }
    };
    timings.record_since("read", phase_started);

    crate::progress::phase("tokenize");
    let phase_started = Instant::now();
    let tokens = match lexer::tokenize(&source) {
        Ok(tokens) => tokens,
        Err(e) => {
            crate::progress::clear();
            errors::report(&e.with_file(filename.to_string()));
            process::exit(1);
        }
    };
    timings.record_since("tokenize", phase_started);

    crate::progress::phase("parse");
    let phase_started = Instant::now();
    let parsed = match parser::parse(&tokens) {
        Ok(ast) => ast,
        Err(e) => {
            crate::progress::clear();
            errors::report(&e.with_file(filename.to_string()));
            process::exit(1);
        }
    };
    timings.record_since("parse", phase_started);

    crate::progress::phase("magic-constants");
    let phase_started = Instant::now();
    let main_file_path = Path::new(filename).to_path_buf();
    let parsed = magic_constants::substitute_file_and_scope_constants(parsed, &main_file_path);
    timings.record_since("magic-constants", phase_started);

    // Strict-PHP audit of the main file: after magic-constant substitution
    // (matching the include/autoload audit sites) and before
    // `conditional::apply` consumes `ifdef` nodes, so every elephc-only
    // construct is reported with its span. Included and autoloaded user files
    // are audited where they are parsed (resolver / autoloader), so injected
    // compiler preludes are never audited.
    if let Err(e) = crate::strict_php::check_file(&parsed, filename) {
        crate::progress::clear();
        errors::report(&e);
        process::exit(1);
    }

    let parsed = conditional::apply(parsed, &defines);

    crate::progress::phase("autoload-build");
    let phase_started = Instant::now();
    let (autoload_registry, parsed) = autoload::Registry::build(parent, parsed);
    codegen::set_autoload_rule_count(autoload_registry.rule_count());
    for warning in autoload_registry.warnings() {
        errors::report_warning(warning);
    }
    timings.record_since("autoload-build", phase_started);

    crate::progress::phase("resolve");
    let phase_started = Instant::now();
    let ast = match resolver::resolve(parsed, parent) {
        Ok(resolved) => resolved,
        Err(e) => {
            crate::progress::clear();
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
    crate::progress::phase("pdo-prelude");
    let phase_started = Instant::now();
    let ast = pdo_prelude::inject_if_used(ast, with_crates.contains("pdo"));
    timings.record_since("pdo-prelude", phase_started);

    // Inject the timezone-introspection prelude (extern block + array marshalling,
    // written in elephc-PHP) only when the program references getLocation /
    // getTransitions / listAbbreviations or their procedural aliases, so other
    // binaries never declare the elephc_tz externs or link the bridge. Runs after
    // include resolution so usage inside includes is detected.
    crate::progress::phase("tz-prelude");
    let phase_started = Instant::now();
    let ast = tz_prelude::inject_if_used(ast, with_crates.contains("tz"));
    timings.record_since("tz-prelude", phase_started);

    // Inject the listIdentifiers-filtering prelude (a pure elephc-PHP function over
    // a baked group/country table) only when the program references
    // DateTimeZone::listIdentifiers or timezone_identifiers_list, so other binaries
    // never carry the table. Runs after include resolution so usage inside includes
    // is detected, and before name resolution, which desugars both call forms to it.
    crate::progress::phase("list-id-prelude");
    let phase_started = Instant::now();
    let ast = list_id_prelude::inject_if_used(ast);
    timings.record_since("list-id-prelude", phase_started);

    // Inject the var_export prelude (a pure elephc-PHP function) only when the program
    // references var_export and does not declare its own, so other binaries carry
    // nothing. Runs after include resolution so usage inside includes is detected, and
    // before name resolution so the call resolves to the injected function.
    crate::progress::phase("var-export-prelude");
    let phase_started = Instant::now();
    let ast = var_export_prelude::inject_if_used(ast);
    timings.record_since("var-export-prelude", phase_started);

    // Inject the image standard-library prelude (elephc_image externs + GD/Exif/
    // Imagick/Gmagick/Cairo surface, written in elephc-PHP) only when the program
    // references an image symbol, so non-image binaries never declare the
    // elephc_image externs or link the bridge. Runs after include resolution so
    // image usage inside includes is detected.
    crate::progress::phase("image-prelude");
    let phase_started = Instant::now();
    let ast = crate::image_prelude::inject_if_used(ast, with_crates.contains("image"));
    timings.record_since("image-prelude", phase_started);

    crate::progress::phase("web-prelude");
    let phase_started = Instant::now();
    let ast = web_prelude::inject_if_web(ast, web, php_version);
    timings.record_since("web-prelude", phase_started);

    crate::progress::phase("name-resolve");
    let phase_started = Instant::now();
    let ast = match name_resolver::resolve(ast) {
        Ok(resolved) => resolved,
        Err(e) => {
            crate::progress::clear();
            errors::report(&e);
            process::exit(1);
        }
    };
    timings.record_since("name-resolve", phase_started);

    crate::progress::phase("autoload-run");
    let phase_started = Instant::now();
    let ast = match autoload::run(ast, parent, &autoload_registry) {
        Ok(resolved) => resolved,
        Err(e) => {
            crate::progress::clear();
            errors::report(&e);
            process::exit(1);
        }
    };
    timings.record_since("autoload-run", phase_started);

    crate::progress::phase("opt-fold");
    let phase_started = Instant::now();
    let ast = optimize::fold_constants(ast);
    timings.record_since("opt-fold", phase_started);

    crate::progress::phase("typecheck");
    let phase_started = Instant::now();
    let check_result = match types::check_with_target(&ast, target) {
        Ok(result) => result,
        Err(e) => {
            crate::progress::clear();
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
        crate::progress::clear();
        eprintln!(
            "Target '{}' is recognized, but it is outside the current supported target matrix",
            target
        );
        process::exit(1);
    }

    crate::progress::phase("exports-scan");
    let phase_started = Instant::now();
    let exported_functions = match exports::collect(&ast, &check_result.functions) {
        Ok(exports) => exports,
        Err(e) => {
            crate::progress::clear();
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
        crate::progress::clear();
        timings.report();
        crate::progress::finish_ok(&format!("Checked '{}'", filename), timings.elapsed());
        return;
    }

    crate::progress::phase("opt-prop");
    let phase_started = Instant::now();
    let ast = optimize::propagate_constants(ast);
    timings.record_since("opt-prop", phase_started);

    crate::progress::phase("opt-post");
    let phase_started = Instant::now();
    let ast = optimize::prune_constant_control_flow(ast);
    timings.record_since("opt-post", phase_started);

    crate::progress::phase("opt-norm");
    let phase_started = Instant::now();
    let ast = optimize::normalize_control_flow(ast);
    timings.record_since("opt-norm", phase_started);

    crate::progress::phase("dce");
    let phase_started = Instant::now();
    let ast = optimize::eliminate_dead_code(ast);
    timings.record_since("dce", phase_started);

    if emit_ir {
        crate::progress::phase("ir-lower");
        let phase_started = Instant::now();
        let mut module = match ir_lower::lower_program_with_source_path_and_web(
            &ast,
            &check_result,
            target,
            Path::new(filename),
            web,
        ) {
            Ok(module) => module,
            Err(err) => {
                crate::progress::clear();
                eprintln!("EIR lowering error: {}", err);
                process::exit(1);
            }
        };
        timings.record_since("ir-lower", phase_started);

        crate::progress::phase("ir-opt");
        let phase_started = Instant::now();
        if ir_opt {
            ir_passes::optimize_module(&mut module);
        }
        timings.record_since("ir-opt", phase_started);

        crate::progress::phase("ir-print");
        let phase_started = Instant::now();
        let text = ir::print_module(&module);
        timings.record_since("ir-print", phase_started);
        crate::progress::clear();
        timings.report();
        print!("{}", text);
        return;
    }

    crate::progress::phase("ir-lower");
    let phase_started = Instant::now();
    let mut ir_module = match ir_lower::lower_program_with_source_path_and_web(
        &ast,
        &check_result,
        target,
        Path::new(filename),
        web,
    ) {
        Ok(module) => module,
        Err(err) => {
            crate::progress::clear();
            eprintln!("EIR lowering error: {}", err);
            process::exit(1);
        }
    };
    timings.record_since("ir-lower", phase_started);

    crate::progress::phase("ir-opt");
    let phase_started = Instant::now();
    if ir_opt {
        ir_passes::optimize_module(&mut ir_module);
    }
    timings.record_since("ir-opt", phase_started);

    let mut runtime_features = ir_module.required_runtime_features;
    // `--web` selects the output-capture variant of `__rt_stdout_write`. This is the
    // sole driver of the web runtime feature: it is CLI-driven, not derived from the
    // program, so the runtime cache (keyed on the generated assembly hash) keeps the
    // web and non-web runtime objects distinct automatically.
    runtime_features.web = web;

    if web && !extra_link_libs.iter().any(|lib| lib == "elephc_web") {
        extra_link_libs.push("elephc_web".to_string());
    }

    // `--with-<crate>` force-links each named bridge staticlib (whole-archived,
    // via `forced_bridge_libs`, so it is not dead-stripped) regardless of feature
    // auto-detection. Crates with a PHP-surface prelude (pdo/tz/image) also had
    // that prelude force-injected above, so their classes/functions are available.
    let mut forced_bridge_libs: Vec<String> = Vec::new();
    for flag in &with_crates {
        if let Some(lib) = linker::bridge_lib_for_flag(flag) {
            if !extra_link_libs.iter().any(|l| l == lib) {
                extra_link_libs.push(lib.to_string());
            }
            forced_bridge_libs.push(lib.to_string());
        }
    }

    let requires_elephc_tls = extra_link_libs.iter().any(|lib| lib == "elephc_tls")
        || check_result
            .required_libraries
            .iter()
            .any(|lib| lib == "elephc_tls");

    crate::progress::phase("runtime-cache");
    let phase_started = Instant::now();
    let runtime_pic = matches!(emit, Emit::Cdylib);
    let runtime_object = match runtime_cache::prepare_runtime_object(heap_size, target, runtime_features, runtime_pic) {
        Ok(runtime_object) => runtime_object,
        Err(err) => {
            crate::progress::clear();
            eprintln!("Runtime cache error: {}", err);
            process::exit(1);
        }
    };
    timings.record_since("runtime-cache", phase_started);
    timings.note(format!("runtime-cache {}", runtime_object.status.as_str()));

    crate::progress::phase("codegen");
    let phase_started = Instant::now();
    let user_asm = match codegen::generate_user_asm_from_ir_with_options(
        &ir_module,
        gc_stats,
        heap_debug,
        requires_elephc_tls,
        emit,
        &exported_functions,
        regalloc_linear,
        web,
    ) {
        Ok(asm) => asm,
        Err(err) => {
            crate::progress::clear();
            eprintln!("EIR backend error: {}", err);
            process::exit(1);
        }
    };
    let user_asm = if emit_debug_info {
        debug_info::inject_line_directives(&user_asm, filename, target.platform)
    } else {
        user_asm
    };
    timings.record_since("codegen", phase_started);

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

    crate::progress::phase("write-asm");
    let phase_started = Instant::now();
    if let Err(e) = fs::write(&output_paths.asm, &user_asm) {
        crate::progress::clear();
        eprintln!("Error writing '{}': {}", output_paths.asm.display(), e);
        process::exit(1);
    }
    timings.record_since("write-asm", phase_started);

    if emit_source_map {
        crate::progress::phase("source-map");
        let phase_started = Instant::now();
        if let Err(err) =
            source_map::write_source_map(
                &user_asm,
                Path::new(filename),
                &output_paths.asm,
                &output_paths.source_map,
            )
        {
            crate::progress::clear();
            eprintln!("Source map error: {}", err);
            process::exit(1);
        }
        timings.record_since("source-map", phase_started);
    }

    if emit_asm {
        crate::progress::clear();
        timings.report();
        crate::progress::finish_ok(
            &format!(
                "Emitted assembly '{}' -> '{}'",
                filename,
                output_paths.asm.display()
            ),
            timings.elapsed(),
        );
        return;
    }

    crate::progress::phase("assemble");
    let phase_started = Instant::now();
    linker::assemble(target, &output_paths.asm, &output_paths.obj);
    timings.record_since("assemble", phase_started);

    for (lib_name, flag_name) in linker::bridges_in(&extra_link_libs) {
        let detail = if forced_bridge_libs.iter().any(|l| l == lib_name) {
            format!("{} (--with-{})", lib_name, flag_name)
        } else {
            format!("{} (auto-detected)", lib_name)
        };
        crate::progress::event("Linking", &detail);
    }

    crate::progress::phase("link");
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
        &forced_bridge_libs,
    );
    timings.record_since("link", phase_started);

    // With --debug-info the DWARF line tables must be preserved past object
    // cleanup: on macOS `dsymutil` bakes them into a .dSYM while the object
    // still exists; if that fails the object is kept so debuggers can follow
    // the binary's debug map to it.
    let keep_obj_for_debug =
        emit_debug_info && !linker::bake_debug_info(target, &output_paths.bin);
    if !keep_obj_for_debug {
        let _ = fs::remove_file(&output_paths.obj);
    }

    crate::progress::clear();
    timings.report();
    crate::progress::finish_ok(
        &format!("Compiled '{}' -> '{}'", filename, output_paths.bin.display()),
        timings.elapsed(),
    );
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
            Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
        },
    };
    OutputPaths {
        asm: parent.join(format!("{}.s", stem)),
        obj: parent.join(format!("{}.o", stem)),
        bin: parent.join(bin_name),
        source_map: parent.join(format!("{}.map", stem)),
    }
}
