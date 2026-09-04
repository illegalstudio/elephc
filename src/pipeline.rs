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

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process;
use std::time::Instant;

use crate::cli::CliConfig;
use crate::codegen::platform::Target;
use crate::codegen::Emit;
use crate::codegen::LinkRequirement;
use crate::native_deps::NativeRequirement;
use crate::span::Span;
use crate::source::SourceMode;
use crate::timings::CompileTimings;
use crate::{
    autoload, codegen, debug_info, errors, exports, func_args, ir, ir_lower, ir_passes, lexer,
    linker, list_id_prelude, mysqli_prelude, name_resolver, opcache_prelude, optimize, parser,
    pdo_prelude, resolver, runtime_cache, source_map, tz_prelude, types, var_export_prelude,
    web_prelude,
};

mod backend;
mod eir_output;
mod frontend;
mod output;

use output::{dynamic_eval_capability_warning, output_paths, OutputPaths};

/// Runs the full compilation pipeline from PHP source to native binary.
/// Reads PHP source, tokenizes, parses, resolves names, type-checks, optimizes,
/// generates assembly, and links into a native binary. Exits on any error.
pub(crate) fn compile(config: CliConfig) {
    let CliConfig {
        filename,
        heap_size,
        gc_stats,
        counters,
        instrument,
        heap_debug,
        strict_opcache,
        emit_ir,
        null_repr,
        emit_asm,
        emit,
        check_only,
        emit_timings,
        emit_source_map,
        emit_debug_info,
        keep_symbols,
        regalloc_linear,
        ir_opt,
        target,
        php_version,
        php_version_provenance,
        extra_link_libs,
        extra_link_paths,
        extra_frameworks,
        defines,
        strict_php,
        strict_locals,
        web,
        web_isolation,
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
    crate::superglobals::set_compiling_for_web(web);
    crate::strict_php::set_enabled(strict_php);
    let parent = Path::new(filename).parent().unwrap_or(Path::new("."));
    let source_mode = SourceMode::from_path(Path::new(filename));
    let output_paths = output_paths(filename, target, emit);
    let mut timings = CompileTimings::new(emit_timings);

    let parsed = frontend::read_and_parse(filename, source_mode, &defines, &mut timings);

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

    // Report how the PHP profile is observable in THIS program, while `ast` is still the
    // user's own code: after include resolution, but before any compiler prelude is injected.
    // The `--web` prelude both calls `__elephc_php_version_id()` and defines the whole session
    // surface, so scanning any later would report every `--web` build as profile-dependent on
    // the strength of elephc's own generated code. Silent unless the profile actually changes
    // what this program computes.
    crate::php_profile::report(&ast, web, php_version, php_version_provenance);

    // Reject a profile the program's own syntax could never have run under. elephc's parser
    // accepts the whole language whatever `--php-version` says, so without this a file using
    // 8.4 property hooks compiles under `--php-version 8.2` and bakes `PHP_VERSION = "8.2.0"`
    // into a binary its source contradicts.
    if let Some(error) = crate::php_profile::floor_violation(&ast, php_version) {
        crate::progress::clear();
        errors::report(&error);
        process::exit(1);
    }

    let mut prelude_inventory = optimize::reachability::PreludeInventory::new();
    // `curl` belongs here for the same reason `pdo` does, and NOT listing it is a silent
    // no-op rather than a smaller binary: `--with-curl` force-injects the whole surface for
    // a program that reaches curl only dynamically, and declaration-reachability would then
    // delete every one of those declarations again (measured: 40 functions in, 1 out,
    // `curl_init` and `CurlHandle` both gone). See
    // `curl_prelude::reachability_tests::forcing_the_curl_group_keeps_the_whole_surface`.
    let mut forced_groups: HashSet<String> = [
        (with_crates.contains("pdo"), "pdo"),
        (with_crates.contains("mysqli"), "mysqli"),
        (with_crates.contains("tz"), "tz"),
        (with_crates.contains("image"), "image"),
        (with_crates.contains("curl"), "curl"),
    ]
    .into_iter()
    .filter_map(|(forced, group)| forced.then_some(group.to_string()))
    .collect();

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
    // `pdo_used` is decided BEFORE injection and recorded as a PHP surface:
    // extension reporting is surface-based because the `elephc_pdo` archive
    // backs more than one PHP surface (PDO and mysqli).
    crate::progress::phase("pdo-prelude");
    let phase_started = Instant::now();
    let pdo_force = with_crates.contains("pdo");
    // Detect once, then pass the result as `force` to injection: `inject_if_used`
    // injects when `force || detect(...)`, so `force = pdo_used` reproduces the
    // exact decision without a second AST walk (the injected PDO prelude would
    // otherwise be re-scanned by the mysqli detection below too).
    let pdo_used = pdo_force || pdo_prelude::program_uses_pdo(&ast);
    let ast = if php_version == crate::web_prelude::PhpVersion::default() {
        pdo_prelude::inject_if_used(ast, pdo_used, &mut prelude_inventory)
    } else {
        pdo_prelude::inject_if_used_for_version(
            ast,
            pdo_used,
            php_version,
            &mut prelude_inventory,
        )
    };
    let mut linked_php_surfaces: Vec<String> = Vec::new();
    if pdo_used {
        linked_php_surfaces.push("PDO".to_string());
    }
    timings.record_since("pdo-prelude", phase_started);

    // Inject the mysqli prelude (a second PHP surface over the same elephc_pdo
    // bridge) only when the program references a mysqli symbol or
    // `--with-mysqli` forces it. Runs AFTER the PDO injection so the shared
    // extern block — prepended idempotently by whichever surface injects — is
    // declared exactly once, and never injects the PDO classes.
    crate::progress::phase("mysqli-prelude");
    let phase_started = Instant::now();
    let mysqli_force = with_crates.contains("mysqli");
    let mysqli_used = mysqli_force || mysqli_prelude::program_uses_mysqli(&ast);
    let ast =
        mysqli_prelude::inject_if_used(ast, mysqli_used, php_version, &mut prelude_inventory);
    if mysqli_used {
        linked_php_surfaces.push("mysqli".to_string());
    }
    timings.record_since("mysqli-prelude", phase_started);

    // Inject the timezone-introspection prelude (extern block + array marshalling,
    // written in elephc-PHP) only when the program references getLocation /
    // getTransitions / listAbbreviations or their procedural aliases, so other
    // binaries never declare the elephc_tz externs or link the bridge. Runs after
    // include resolution so usage inside includes is detected.
    crate::progress::phase("tz-prelude");
    let phase_started = Instant::now();
    let ast = tz_prelude::inject_if_used(
        ast,
        with_crates.contains("tz"),
        &mut prelude_inventory,
    );
    timings.record_since("tz-prelude", phase_started);

    // Inject the listIdentifiers-filtering prelude (a pure elephc-PHP function over
    // a baked group/country table) only when the program references
    // DateTimeZone::listIdentifiers or timezone_identifiers_list, so other binaries
    // never carry the table. Runs after include resolution so usage inside includes
    // is detected, and before name resolution, which desugars both call forms to it.
    crate::progress::phase("list-id-prelude");
    let phase_started = Instant::now();
    let ast = list_id_prelude::inject_if_used(ast, &mut prelude_inventory);
    timings.record_since("list-id-prelude", phase_started);

    // Inject the var_export prelude (a pure elephc-PHP function) only when the program
    // references var_export and does not declare its own, so other binaries carry
    // nothing. Runs after include resolution so usage inside includes is detected, and
    // before name resolution so the call resolves to the injected function.
    crate::progress::phase("var-export-prelude");
    let phase_started = Instant::now();
    let ast = var_export_prelude::inject_if_used(ast, &mut prelude_inventory);
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
        strict_opcache,
        &mut prelude_inventory,
    );
    timings.record_since("opcache-prelude", phase_started);

    // Inject the image standard-library prelude (elephc_image externs + GD/Exif/
    // Imagick/Gmagick/Cairo surface, written in elephc-PHP) only when the program
    // references an image symbol, so non-image binaries never declare the
    // elephc_image externs or link the bridge. Runs after include resolution so
    // image usage inside includes is detected.
    crate::progress::phase("image-prelude");
    let phase_started = Instant::now();
    let ast = crate::image_prelude::inject_if_used(
        ast,
        with_crates.contains("image"),
        &mut prelude_inventory,
    );
    timings.record_since("image-prelude", phase_started);

    // Inject the incremental-hashing prelude (the `HashContext` class and the
    // `hash_init`/`hash_update`/`hash_final`/`hash_copy` wrappers over the internal
    // `__elephc_hash_ctx_*` builtins) only when the program references that surface,
    // so non-hashing binaries never declare `HashContext` and never link
    // `-lelephc_crypto`. Runs after include resolution so hashing inside includes is
    // detected, and before name resolution so a namespaced caller resolves to it.
    crate::progress::phase("hash-prelude");
    let phase_started = Instant::now();
    let ast = crate::hash_prelude::inject_if_used(ast, false, &mut prelude_inventory);
    timings.record_since("hash-prelude", phase_started);

    // The `Directory` class and the `dir()` that mints one, injected only when the program
    // references either. Runs beside the other class preludes, after include resolution so a
    // reference inside an include is detected, and before name resolution so a namespaced caller
    // resolves to it.
    crate::progress::phase("dir-prelude");
    let phase_started = Instant::now();
    let ast = crate::dir_prelude::inject_if_used(ast);
    timings.record_since("dir-prelude", phase_started);

    // php's `gz*` stream surface, spelled as the `compress.zlib://` wrapper the stream builtins
    // already serve, injected only when the program references one of those functions. Runs beside
    // the other preludes, after include resolution so a reference inside an include is detected,
    // and before name resolution so a namespaced caller resolves to it.
    crate::progress::phase("gz-prelude");
    let phase_started = Instant::now();
    let ast = crate::gz_prelude::inject_if_used(ast);
    timings.record_since("gz-prelude", phase_started);

    // php's scanf engine, injected only when the program references `sscanf()`/`fscanf()`, whose
    // registry lowerings call into it. Runs after include resolution so a scan inside an include
    // is detected, and before name resolution so the emitted call resolves to a declared function.
    crate::progress::phase("scanf-prelude");
    let phase_started = Instant::now();
    let ast = crate::scanf_prelude::inject_if_used(ast, &mut prelude_inventory);
    // The engine is named only by the lowerings of `sscanf()`/`fscanf()`, never by PHP source, so
    // reachability has no edge to follow to it. The group exists only when the prelude was
    // injected, which happens only when the program references one of those two names.
    if prelude_inventory
        .groups
        .contains_key(crate::scanf_prelude::PRELUDE_GROUP_ID)
    {
        forced_groups.insert(crate::scanf_prelude::PRELUDE_GROUP_ID.to_string());
    }
    timings.record_since("scanf-prelude", phase_started);

    // php's `similar_text()` engine, injected only when the program references it. Same shape as
    // the scanf prelude: the declarations are named by the builtin's lowering and never by PHP
    // source, so the group has to be forced or reachability prunes what the backend then calls.
    crate::progress::phase("similar-text-prelude");
    let phase_started = Instant::now();
    let ast = crate::similar_text_prelude::inject_if_used(ast, &mut prelude_inventory);
    if prelude_inventory
        .groups
        .contains_key(crate::similar_text_prelude::PRELUDE_GROUP_ID)
    {
        forced_groups.insert(crate::similar_text_prelude::PRELUDE_GROUP_ID.to_string());
    }
    timings.record_since("similar-text-prelude", phase_started);
    // Inject the `ext/curl` prelude (the `CurlHandle` class and the `curl_*` wrappers over
    // the internal `__elephc_curl_*` builtins) only when the program references that
    // surface, so non-curl binaries never declare `CurlHandle`, never link
    // `-lelephc_curl`, and never require the managed native `curl` package. Runs after
    // the hash prelude (both are order-independent declaration-only preludes) and before
    // name resolution so a namespaced caller resolves to it. `--with-curl` forces the
    // injection for a program that only reaches curl dynamically. The version selects the
    // curl SURFACE too: `curl_multi_get_handles()` is 8.5-only.
    crate::progress::phase("curl-prelude");
    let phase_started = Instant::now();
    let ast = if php_version == crate::php_version::PhpVersion::default() {
        crate::curl_prelude::inject_if_used(
            ast,
            with_crates.contains("curl"),
            &mut prelude_inventory,
        )
    } else {
        crate::curl_prelude::inject_if_used_for_version(
            ast,
            with_crates.contains("curl"),
            php_version,
            &mut prelude_inventory,
        )
    };
    timings.record_since("curl-prelude", phase_started);

    crate::progress::phase("web-prelude");
    let phase_started = Instant::now();
    let ast = web_prelude::inject_if_web(
        ast,
        web,
        php_version,
        &ini_overrides,
        &mut prelude_inventory,
    );
    timings.record_since("web-prelude", phase_started);

    // Inject the PHP version-surface functions (`zend_version`, `php_sapi_name`,
    // `ini_restore`) the program actually references. Runs AFTER the web prelude so a
    // `--web` build's own declarations are already present and the redeclaration guard sees
    // them, and before name resolution so a namespaced caller resolves to the injection.
    crate::progress::phase("version-prelude");
    let phase_started = Instant::now();
    let ast = crate::version_prelude::inject_if_used(
        ast,
        php_version,
        &mut prelude_inventory,
    );
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

    // Desugar PHP's argument-introspection constructs (`func_num_args`, `func_get_args`,
    // `func_get_arg`) into plain PHP: every function scope that uses one gains the hidden
    // `mixed ...$__elephc_func_args` parameter, so the surplus positional arguments PHP
    // allows are collected by the existing variadic machinery. Runs after `autoload::run`
    // so autoloaded declarations are covered too — which means call names are already
    // resolved here and are matched on their unqualified last segment — and before the AST
    // optimizer and the checker, which then only ever see ordinary PHP.
    crate::progress::phase("func-args");
    let phase_started = Instant::now();
    let ast = match func_args::desugar(ast) {
        Ok(desugared) => desugared,
        Err(e) => {
            crate::progress::clear();
            errors::report(&e);
            process::exit(1);
        }
    };
    timings.record_since("func-args", phase_started);

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
        strict_opcache,
    );
    timings.record_since("opcache-manifest-bake", phase_started);

    crate::progress::phase("opt-fold");
    let phase_started = Instant::now();
    let ast = optimize::fold_constants(ast);
    timings.record_since("opt-fold", phase_started);

    crate::progress::phase("typecheck");
    let phase_started = Instant::now();
    let check_options = types::CheckOptions { strict_locals };
    let mut check_result = match types::check_with_target_and_options(&ast, target, check_options) {
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
    if matches!(emit, Emit::Executable)
        && !check_only
        && !emit_ir
        && !exported_functions.is_empty()
    {
        let names: Vec<&str> = exported_functions.keys().map(String::as_str).collect();
        eprintln!(
            "warning: ignoring #[Export] on functions {:?} — --emit cdylib is required to expose them",
            names
        );
    }

    if check_only && exported_functions.is_empty() {
        crate::progress::clear();
        timings.report();
        crate::progress::finish_ok(&format!("Checked '{}'", filename), timings.elapsed());
        return;
    }

    crate::progress::phase("opt-prop");
    let phase_started = Instant::now();
    let post_typecheck_optimizer = optimize::PostTypecheckOptimizer::new_with_type_metadata(
        &ast,
        &check_result.functions,
        &check_result.classes,
        &check_result.interfaces,
    );
    // Substituting a literal for a read of a local the checker boxed as `mixed` would hand EIR
    // lowering a concrete type the checker never approved for that name, so the pass is told which
    // names those are and refuses to record a fact for them.
    let ast = post_typecheck_optimizer.propagate(ast, check_result.mixed_storage_local_names());
    timings.record_since("opt-prop", phase_started);

    crate::progress::phase("opt-post");
    let phase_started = Instant::now();
    // Pruning and normalization both run the single-case switch rewrite, which materializes the
    // default body into BOTH branches of the synthesized `if` with the original's spans. The
    // checker's local-binding decisions are keyed BY SPAN, so these phases are told which spans
    // carry one and the rewrite vetoes itself rather than duplicating a decision.
    let ast = post_typecheck_optimizer.prune(ast, check_result.local_binding_decision_spans());
    timings.record_since("opt-post", phase_started);

    crate::progress::phase("opt-norm");
    let phase_started = Instant::now();
    let ast = post_typecheck_optimizer.normalize(ast, check_result.local_binding_decision_spans());
    timings.record_since("opt-norm", phase_started);

    crate::progress::phase("dce");
    let phase_started = Instant::now();
    // Tail-sinking clones the tail of an `if`/`switch`/`try` into every branch, and a clone keeps
    // the original's spans — the same span-keyed hazard, in the other pass that clones.
    let ast = post_typecheck_optimizer
        .eliminate_dead_code(ast, check_result.local_binding_decision_spans());
    timings.record_since("dce", phase_started);

    crate::progress::phase("decl-reach");
    let phase_started = Instant::now();
    let exported_function_names: HashSet<String> = exported_functions.keys().cloned().collect();
    let ast = optimize::prune_unreachable_declarations(
        ast,
        &mut check_result,
        optimize::reachability::PruneOptions {
            inventory: &prelude_inventory,
            forced_groups: &forced_groups,
            exported_functions: &exported_function_names,
            eval_forced: with_crates.contains("eval"),
        },
    );
    timings.record_since("decl-reach", phase_started);
    codegen::prepare_declared_name_order(
        &ast,
        &check_result.classes,
        &check_result.interfaces,
    );

    if emit_ir {
        eir_output::emit(
            &ast,
            &check_result,
            target,
            filename,
            web,
            ir_opt,
            &exported_functions,
            &mut timings,
        );
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

    if emit.is_library() || (check_only && !exported_functions.is_empty()) {
        if let Err(error) = exports::validate_cdylib_call_graph(&ir_module, &exported_functions) {
            crate::progress::clear();
            errors::report(&error.with_file(filename.to_string()));
            process::exit(1);
        }
    }

    if check_only {
        crate::progress::clear();
        timings.report();
        crate::progress::finish_ok(&format!("Checked '{}'", filename), timings.elapsed());
        return;
    }

    crate::progress::phase("ir-opt");
    let phase_started = Instant::now();
    if ir_opt {
        ir_passes::optimize_module(&mut ir_module);
    }
    timings.record_since("ir-opt", phase_started);

    backend::emit_and_link(backend::BackendInputs {
        filename,
        with_crates: &with_crates,
        linked_php_surfaces: &linked_php_surfaces,
        ir_module,
        web,
        web_isolation,
        extra_link_libs: &extra_link_libs,
        extra_link_paths: &extra_link_paths,
        extra_frameworks: &extra_frameworks,
        required_libraries: &check_result.required_libraries,
        target,
        emit,
        heap_size,
        gc_stats,
        counters,
        instrument,
        heap_debug,
        exported_functions: &exported_functions,
        regalloc_linear,
        emit_debug_info,
        keep_symbols,
        output_paths: &output_paths,
        emit_source_map,
        emit_asm,
        timings: &mut timings,
    });
}
