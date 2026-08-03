//! Purpose:
//! Orchestrates the full PHP source to native binary compilation flow.
//! Resolves typed managed dependencies only when the selected path performs a final link.
//!
//! Called from:
//! - `crate::main()` after `crate::cli::parse_args()`.
//!
//! Key details:
//! - Pass ordering is observable: magic constants and conditionals run before resolver/name resolution and type checking.
//! - Check/EIR/assembly-only paths return before read-only native artifact resolution.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::Instant;

use crate::cli::CliConfig;
use crate::codegen::platform::{Platform, Target};
use crate::codegen::Emit;
use crate::codegen::LinkRequirement;
use crate::native_deps::NativeRequirement;
use crate::span::Span;
use crate::source::SourceMode;
use crate::timings::CompileTimings;
use crate::{
    autoload, codegen, codegen_wasm, debug_info, errors, exports, ir, ir_lower, ir_passes, lexer,
    linker, list_id_prelude, name_resolver, opcache_prelude, optimize, parser, pdo_prelude,
    resolver, runtime_cache, source_map, tz_prelude, types, var_export_prelude, web_prelude,
};

/// Holds the paths for compilation outputs and an optional generated package directory.
struct OutputPaths {
    asm: PathBuf,
    obj: PathBuf,
    bin: PathBuf,
    source_map: PathBuf,
    package_dir: Option<PathBuf>,
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
        extra_link_libs,
        extra_link_paths,
        extra_frameworks,
        defines,
        strict_php,
        web,
        with_crates,
        quiet,
        ini_overrides,
    } = config;
    let filename = filename.as_str();
    crate::progress::init(quiet);
    codegen::set_null_repr(null_repr);
    // Record the PHP language profile and SAPI mode BEFORE any prelude or lowering runs: it is
    // the single source of truth for the reported version surface (`PHP_VERSION` and friends,
    // `PHP_SAPI`, `phpversion()`), which is baked far below this function's parameter list — in
    // `codegen_support::prescan::collect_constants` and in the `phpversion()` const-fold.
    codegen::set_compile_profile(php_version, web);
    // Clear the typing mode before parsing: the parser only ever sets it, so without
    // this a strict compilation would leak into the next one on the same thread.
    crate::codegen_support::set_strict_types(false);
    crate::strict_php::set_enabled(strict_php);
    let parent = Path::new(filename).parent().unwrap_or(Path::new("."));
    let source_mode = SourceMode::from_path(Path::new(filename));
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
    let tokens = match lexer::tokenize_with_mode(&source, source_mode) {
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
    let parsed = match parser::parse_with_mode(&tokens, source_mode) {
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
    let parsed = match crate::source::finalize_physical_program(
        parsed,
        &main_file_path,
        source_mode,
        &defines,
    ) {
        Ok(parsed) => parsed,
        Err(e) => {
            crate::progress::clear();
            errors::report(&e);
            process::exit(1);
        }
    };
    timings.record_since("magic-constants", phase_started);

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
    // `resolve_collecting_includes` also hands back the canonical path of every file the
    // resolver statically inlined — group 2 of the OPcache script manifest.
    let (ast, opcache_included_files) =
        match resolver::resolve_collecting_includes_with_defines(parsed, parent, &defines) {
        Ok(resolved) => resolved,
        Err(e) => {
            crate::progress::clear();
            errors::report(&e);
            process::exit(1);
        }
    };
    let ast = autoload::collect_aliases(ast);
    timings.record_since("resolve", phase_started);

    // Snapshot the USER-declared function/class names for `opcache.preload`'s
    // `preload_statistics`, taken HERE — after include resolution but BEFORE any compiler prelude
    // is injected — so the reported lists can never contain `var_export`, the PDO surface, or the
    // OPcache functions the opcache prelude itself adds. Reference PHP reports the DELTA preloading
    // added to the symbol tables, which likewise never contains a built-in. The walk visits only
    // statement lists that can host a hoisted declaration, so it is cheap on every build; it is
    // consumed only when `opcache.preload` is set (see `opcache_prelude::preload_statistics`).
    let opcache_preload_symbols = opcache_prelude::collect_preload_symbols(&ast);

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

    // Inject the OPcache preludes (pure elephc-PHP functions): `opcache_get_configuration()`
    // returns a compile-time array literal built from the version-keyed OPcache
    // directive matrix, and `opcache_reset()` returns the compile-time cache-enabled
    // boolean. Each is injected only when the program references it, so other binaries
    // carry nothing. Runs after include resolution so usage inside includes is detected,
    // and before name resolution so the call resolves to the injected function. The
    // reported directive set/version follows the compile target `php_version`; the
    // `opcache_reset()` result follows the SAPI (`web`): CLI disabled, web enabled.
    // Build the PLACEHOLDER OPcache script manifest: the canonicalized main entry file, every
    // statically-resolved include/require target, and Composer `autoload.files`. The PSR-4 /
    // SPL-rule class files are still unknown here — `autoload::run` produces them below, after
    // name resolution — so this manifest is completed and re-baked by
    // `opcache_prelude::bake_manifest` further down. The declarations themselves MUST be
    // injected here, before `name_resolver`, or a namespaced `opcache_get_status()` caller
    // would not resolve to them (see `opcache_prelude::bake_manifest` for the full argument).
    // The manifest feeds `opcache_get_status().scripts`, `opcache_is_script_cached`, and
    // `opcache_compile_file`.
    crate::progress::phase("opcache-prelude");
    let phase_started = Instant::now();
    let opcache_manifest = opcache_prelude::collect_manifest(
        filename,
        &opcache_included_files,
        autoload_registry.always_included_files(),
    );
    // The canonicalized entry script, resolved separately from the manifest: it is the operand
    // `opcache.restrict_api` compares its prefix against (reference PHP uses
    // `SG(request_info).path_translated`, the ENTRY script — not the executing file), and the
    // manifest deliberately drops entries it cannot stat, so its first element is not a
    // dependable stand-in. See `opcache_prelude::restrict_api_denies`.
    let opcache_entry_path = opcache_prelude::canonical_entry_path(filename);
    // `opcache.preload` is a COMPILE-TIME decision, resolved here for the same reason
    // `restrict_api` is: reference PHP preloads during STARTUP, before the script runs, and
    // elephc's INI is fixed when the binary is built. The three outcomes mirror reference exactly
    // (see `opcache_prelude::PreloadVerdict` for the verified matrix):
    // - unresolvable path with the cache enabled → HARD COMPILE ERROR, the AOT equivalent of
    //   reference's startup fatal. It fires whether or not the program calls an OPcache function,
    //   because reference's fatal does not depend on that either.
    // - resolvable but outside the compile-time script manifest → a WARNING only: preloading a file
    //   this program never includes is a legitimate configuration and must not break a build. That
    //   arm depends on the COMPLETE manifest, so it is evaluated after `autoload::run` below; only
    //   the manifest-independent compile ERROR is decided here, matching reference PHP, which
    //   fatals at startup regardless of what the script does.
    // - empty directive, or a disabled cache → nothing at all happens (reference does not preload
    //   when the accelerator is off, and does not even validate the path).
    let opcache_preload =
        opcache_prelude::preload_verdict(php_version, web, &ini_overrides, &opcache_manifest);
    if let Some(message) = opcache_preload.compile_error() {
        errors::report(&errors::CompileError::new(Span::new(0, 0), &message).with_file(filename.to_string()));
        process::exit(1);
    }
    let opcache_preload_statistics = opcache_prelude::preload_statistics(
        &opcache_preload,
        &opcache_manifest,
        &opcache_preload_symbols,
    );
    let (ast, opcache_bake_sites) = opcache_prelude::inject_if_used(
        ast,
        php_version,
        web,
        opcache_entry_path.as_deref(),
        &opcache_manifest,
        &ini_overrides,
        opcache_preload_statistics.as_ref(),
    );
    timings.record_since("opcache-prelude", phase_started);

    // Inject the image standard-library prelude (elephc_image externs + GD/Exif/
    // Imagick/Gmagick/Cairo surface, written in elephc-PHP) only when the program
    // references an image symbol, so non-image binaries never declare the
    // elephc_image externs or link the bridge. Runs after include resolution so
    // image usage inside includes is detected.
    crate::progress::phase("image-prelude");
    let phase_started = Instant::now();
    let ast = crate::image_prelude::inject_if_used(ast, with_crates.contains("image"));
    timings.record_since("image-prelude", phase_started);

    // Inject the incremental-hashing prelude (the `HashContext` class and the
    // `hash_init`/`hash_update`/`hash_final`/`hash_copy` wrappers over the internal
    // `__elephc_hash_ctx_*` builtins) only when the program references that surface,
    // so non-hashing binaries never declare `HashContext` and never link
    // `-lelephc_crypto`. Runs after include resolution so hashing inside includes is
    // detected, and before name resolution so a namespaced caller resolves to it.
    crate::progress::phase("hash-prelude");
    let phase_started = Instant::now();
    let ast = crate::hash_prelude::inject_if_used(ast, false);
    timings.record_since("hash-prelude", phase_started);

    crate::progress::phase("web-prelude");
    let phase_started = Instant::now();
    let ast = web_prelude::inject_if_web(ast, web, php_version, &ini_overrides);
    timings.record_since("web-prelude", phase_started);

    // Inject the PHP version-surface functions (`zend_version`, `php_sapi_name`,
    // `ini_restore`) the program actually references. Runs AFTER the web prelude so a
    // `--web` build's own declarations are already present and the redeclaration guard sees
    // them, and before name resolution so a namespaced caller resolves to the injection.
    crate::progress::phase("version-prelude");
    let phase_started = Instant::now();
    let ast = crate::version_prelude::inject_if_used(ast, php_version);
    timings.record_since("version-prelude", phase_started);

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
    // `run_collecting_included` also hands back the canonical path of every file the autoload
    // pass loaded — Composer `autoload.files`, PSR-4 / SPL-rule class files, and their own
    // include targets: group 3 of the OPcache script manifest, and the last one to become
    // knowable.
    let (ast, opcache_autoloaded_files) =
        match autoload::run_collecting_included_with_defines(
            ast,
            parent,
            &autoload_registry,
            &defines,
        ) {
            Ok(resolved) => resolved,
            Err(e) => {
                crate::progress::clear();
                errors::report(&e);
                process::exit(1);
            }
        };
    timings.record_since("autoload-run", phase_started);

    // Complete the OPcache script manifest now that all three groups exist, and re-render the
    // manifest-dependent functions injected above against it. This is a pure substitution of
    // already-declared, already-name-resolved top-level functions, so it cannot disturb the
    // name resolution that has already happened (see `opcache_prelude::bake_manifest`). It runs
    // before `optimize::fold_constants` so the baked literals meet every later pass exactly as
    // the placeholder ones would have.
    crate::progress::phase("opcache-manifest-bake");
    let phase_started = Instant::now();
    let opcache_manifest = opcache_prelude::collect_manifest(
        filename,
        &opcache_included_files,
        &opcache_autoloaded_files,
    );
    // Re-decide `opcache.preload` against the complete manifest. Only the `in_manifest` arm can
    // differ from the verdict taken above (the directive, the SAPI gate and the path resolution
    // are all manifest-independent), so this second call exists purely to emit the
    // outside-the-manifest WARNING against the truthful set — reporting it against the
    // placeholder manifest would warn about files that are, in fact, compiled in.
    let opcache_preload =
        opcache_prelude::preload_verdict(php_version, web, &ini_overrides, &opcache_manifest);
    if let Some(message) = opcache_preload.compile_warning() {
        errors::report_warning(&errors::CompileWarning::new(Span::new(0, 0), &message));
    }
    let opcache_preload_statistics = opcache_prelude::preload_statistics(
        &opcache_preload,
        &opcache_manifest,
        &opcache_preload_symbols,
    );
    let ast = opcache_prelude::bake_manifest(
        ast,
        &opcache_bake_sites,
        php_version,
        web,
        &opcache_manifest,
        &ini_overrides,
        opcache_preload_statistics.as_ref(),
    );
    timings.record_since("opcache-manifest-bake", phase_started);

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
    // WASM capability validation must observe every PHP coercion and diagnostic
    // boundary; target-agnostic pruning has no profile-aware proof for all of them.
    let ast = if target.is_wasm() {
        ast
    } else {
        optimize::prune_constant_control_flow(ast)
    };
    timings.record_since("opt-post", phase_started);

    crate::progress::phase("opt-norm");
    let phase_started = Instant::now();
    // Empty control-flow bodies can still observe PHP diagnostics while their
    // conditions are evaluated (notably PHP 8.5 NAN-to-bool warnings).
    let ast = if target.is_wasm() {
        ast
    } else {
        optimize::normalize_control_flow(ast)
    };
    timings.record_since("opt-norm", phase_started);

    crate::progress::phase("dce");
    let phase_started = Instant::now();
    // Keep unused diagnostic-producing expressions until the WASM static gate.
    let ast = if target.is_wasm() {
        ast
    } else {
        optimize::eliminate_dead_code(ast)
    };
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
        // The WASM capability pass currently runs after IR optimization, so keep
        // every transfer/coercion visible until that validation boundary.
        if ir_opt && !target.is_wasm() {
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
    // See the emit-IR path above: WASM must audit the unelided EIR graph.
    if ir_opt && !target.is_wasm() {
        ir_passes::optimize_module(&mut ir_module);
    }
    timings.record_since("ir-opt", phase_started);

    // WebAssembly target: lower EIR to a `.wat` module and emit `.wat`/`.wasm`.
    // This bypasses the native runtime object, assembler, and linker entirely.
    if target.is_wasm() {
        emit_wasm_artifacts(
            &ir_module,
            emit,
            emit_asm,
            filename,
            &output_paths,
            &mut timings,
        );
        return;
    }

    let mut runtime_features = ir_module.required_runtime_features;
    // `--web` selects the output-capture variant of `__rt_stdout_write`. This is the
    // sole driver of the web runtime feature: it is CLI-driven, not derived from the
    // program, so the runtime cache (keyed on the generated assembly hash) keeps the
    // web and non-web runtime objects distinct automatically.
    runtime_features.web = web;

    let runtime_link_requirements =
        codegen::link_requirements_for_runtime_features(runtime_features);

    // `--with-<crate>` force-links each named bridge staticlib (whole-archived,
    // via `forced_bridge_libs`, so it is not dead-stripped) regardless of feature
    // auto-detection. Crates with a PHP-surface prelude (pdo/tz/image) also had
    // that prelude force-injected above, so their classes/functions are available.
    let mut forced_bridge_libs: Vec<String> = Vec::new();
    let mut sorted_with_crates: Vec<&String> = with_crates.iter().collect();
    sorted_with_crates.sort();
    for flag in sorted_with_crates {
        if let Some(lib) = linker::bridge_lib_for_flag(flag) {
            forced_bridge_libs.push(lib.to_string());
        }
    }

    // Collect the named libraries that the typed link planner will consider.
    // This preserves codegen-time bridge feature reporting without flattening
    // those inputs back into the legacy user-library list.
    let mut planned_link_libraries = Vec::new();
    for library in extra_link_libs
        .iter()
        .chain(check_result.required_libraries.iter())
        .chain(forced_bridge_libs.iter())
    {
        if !planned_link_libraries.contains(library) {
            planned_link_libraries.push(library.clone());
        }
    }
    if web && !planned_link_libraries.iter().any(|library| library == "elephc_web") {
        planned_link_libraries.push("elephc_web".to_string());
    }
    for requirement in &runtime_link_requirements {
        if let LinkRequirement::Bridge(library) = requirement {
            if !planned_link_libraries
                .iter()
                .any(|existing| existing.as_str() == *library)
            {
                planned_link_libraries.push((*library).to_string());
            }
        }
    }

    let requires_elephc_tls = planned_link_libraries
        .iter()
        .any(|library| library == "elephc_tls");

    // Report the bridges actually linked into THIS compilation to
    // `extension_loaded()` / `get_loaded_extensions()`. Each planned bridge is
    // mapped through the single-source bridge table; bridges with no distinct
    // PHP extension (tz -> date, eval) are skipped. Seeded into a codegen
    // thread-local because extension folding happens during instruction lowering.
    let mut linked_extensions: Vec<String> = Vec::new();
    for lib in &planned_link_libraries {
        if let Some(ext) = linker::php_extension_for_lib(lib) {
            if !linked_extensions.iter().any(|existing| existing == ext) {
                linked_extensions.push(ext.to_string());
            }
        }
    }
    codegen::set_linked_extensions(linked_extensions);

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
    timings.note(format!("Runtime cache: {}", runtime_object.status.as_str()));

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

    let native_requirements: Vec<NativeRequirement> = runtime_link_requirements
        .iter()
        .filter_map(|requirement| match requirement {
            LinkRequirement::NativePackage(package) => {
                Some(NativeRequirement::package(*package))
            }
            LinkRequirement::Bridge(_) | LinkRequirement::SystemLibrary(_) => None,
        })
        .collect();
    let resolved_native = match crate::native_deps::resolve_for_compilation(
        Path::new(filename),
        target,
        &native_requirements,
    ) {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("{error}");
            process::exit(1);
        }
    };
    let link_plan = crate::link_planning::build(crate::link_planning::LinkPlanningInputs {
        user_libraries: &extra_link_libs,
        user_search_paths: &extra_link_paths,
        user_frameworks: &extra_frameworks,
        checker_libraries: &check_result.required_libraries,
        runtime_requirements: &runtime_link_requirements,
        managed_packages: &resolved_native,
        forced_bridges: &forced_bridge_libs,
        web,
    });

    crate::progress::phase("assemble");
    let phase_started = Instant::now();
    linker::assemble(target, &output_paths.asm, &output_paths.obj);
    timings.record_since("assemble", phase_started);

    for (lib_name, flag_name) in linker::bridges_in(&planned_link_libraries) {
        let detail = if forced_bridge_libs.iter().any(|l| l == lib_name) {
            format!("{} (--with-{})", lib_name, flag_name)
        } else {
            format!("{} (auto-detected)", lib_name)
        };
        crate::progress::event("Linking", &detail);
    }

    crate::progress::phase("link");
    let phase_started = Instant::now();
    if let Err(error) = linker::link_with_plan(
        target,
        emit,
        &output_paths.bin,
        &output_paths.obj,
        &runtime_object.path,
        &link_plan,
        &forced_bridge_libs,
    ) {
        eprintln!("Linker error: {error}");
        process::exit(1);
    }
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

/// Lowers an EIR module to WebAssembly, validates the assembled bytes, and only
/// then publishes the target artifacts (requirement WASM-ART-001).
///
/// The WAT is generated in memory, assembled to a `.wasm` binary with the
/// pure-Rust `wat` crate, and type-validated with `wasmparser` before any file
/// is written. `--emit-asm` performs the same assembly and validation even when
/// only `.wat` is requested. The `.wat`, `.wasm`, and npm package are written
/// through staged writes and renames, so backend, assembly, and validation
/// failures publish nothing and write failures never expose partial destinations.
/// Cleanup failures after publication are emitted as non-fatal warnings because
/// every requested destination has already committed. The native runtime object,
/// assembler, and linker are never involved. Exits the process with a diagnostic
/// on any backend, validation, or pre-commit publication error.
fn emit_wasm_artifacts(
    module: &ir::Module,
    emit: Emit,
    emit_asm: bool,
    filename: &str,
    output_paths: &OutputPaths,
    timings: &mut CompileTimings,
) {
    let phase_started = Instant::now();
    let wat = match codegen_wasm::generate(module, emit) {
        Ok(wat) => wat,
        Err(err) => {
            eprintln!("WebAssembly backend error: {}", err);
            process::exit(1);
        }
    };
    timings.record_since("codegen-wasm", phase_started);

    let source_stem = Path::new(filename)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("output");

    // Assemble, type-validate, and publish in one step: nothing is written until
    // the bytes validate (`--emit-asm` included), and every artifact is written
    // through staged writes so a failure never exposes a partial destination.
    let phase_started = Instant::now();
    match codegen_wasm::publish_wasm_artifacts(
        &wat,
        emit,
        emit_asm,
        source_stem,
        &output_paths.asm,
        &output_paths.bin,
        output_paths.package_dir.as_deref(),
    ) {
        Ok(outcome) => {
            if !outcome.is_clean() {
                for warning in outcome.cleanup_warnings() {
                    eprintln!("warning: {warning}");
                }
            }
        }
        Err(err) => {
            eprintln!("{}", err);
            process::exit(1);
        }
    }
    timings.record_since("publish-wasm", phase_started);

    timings.report();
    if emit_asm {
        println!(
            "Emitted WebAssembly text '{}' -> '{}'",
            filename,
            output_paths.asm.display()
        );
    } else if matches!(emit, Emit::NpmPackage) {
        let package_dir = output_paths
            .package_dir
            .as_deref()
            .expect("NPM output has a package directory");
        println!(
            "Compiled '{}' -> NPM package '{}'",
            filename,
            package_dir.display()
        );
    } else {
        println!("Compiled '{}' -> '{}'", filename, output_paths.bin.display());
    }
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

    // The WebAssembly target emits a `.wat` text module (the readable form, the
    // analogue of `.s`) and a `.wasm` binary (the artifact). It never produces a
    // `.o` object or runs the native linker. NPM packaging writes into a
    // `<stem>-npm/` directory to avoid colliding with the ordinary binary path.
    if target.is_wasm() {
        let package_dir = matches!(emit, Emit::NpmPackage)
            .then(|| parent.join(format!("{}-npm", stem)));
        let bin = package_dir
            .as_ref()
            .map(|dir| dir.join("module.wasm"))
            .unwrap_or_else(|| parent.join(format!("{}.wasm", stem)));
        return OutputPaths {
            asm: parent.join(format!("{}.wat", stem)),
            obj: parent.join(format!("{}.o", stem)),
            bin,
            source_map: parent.join(format!("{}.map", stem)),
            package_dir,
        };
    }

    let bin_name = match emit {
        // NpmPackage is wasm-only; for any native build it never occurs, but keep
        // the match exhaustive by treating it like an executable.
        Emit::Executable | Emit::NpmPackage => stem.to_string(),
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
        package_dir: None,
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for target-specific compilation output path selection.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - These tests only calculate paths and never write compilation artifacts.

    use super::output_paths;
    use crate::codegen::platform::Target;
    use crate::codegen::Emit;
    use std::path::Path;

    /// Verifies ordinary WASM output remains adjacent to the PHP source.
    #[test]
    fn wasm_executable_uses_wat_and_wasm_paths() {
        let paths = output_paths("examples/hello.php", Target::wasm(), Emit::Executable);
        assert_eq!(paths.asm, Path::new("examples/hello.wat"));
        assert_eq!(paths.bin, Path::new("examples/hello.wasm"));
        assert_eq!(paths.package_dir, None);
    }

    /// Verifies NPM output uses an isolated directory containing `module.wasm`.
    #[test]
    fn wasm_npm_uses_isolated_package_directory() {
        let paths = output_paths("examples/hello.php", Target::wasm(), Emit::NpmPackage);
        assert_eq!(
            paths.package_dir.as_deref(),
            Some(Path::new("examples/hello-npm"))
        );
        assert_eq!(paths.bin, Path::new("examples/hello-npm/module.wasm"));
    }
}
