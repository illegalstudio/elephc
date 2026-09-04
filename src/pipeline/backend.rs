//! Purpose:
//! Materializes runtime/codegen outputs and performs native assembly and linking.
//!
//! Called from:
//! - `crate::pipeline::compile()` after EIR lowering and optimization.
//!
//! Key details:
//! - Runtime feature selection, bridge planning, assembly emission, and linking preserve their original order.

use std::collections::{HashMap, HashSet};

use super::*;

/// Inputs consumed by the post-EIR backend and linker pipeline.
pub(super) struct BackendInputs<'a> {
    pub(super) filename: &'a str,
    pub(super) with_crates: &'a HashSet<String>,
    /// PHP surfaces injected into this compilation ("PDO", "mysqli"), reported to
    /// `extension_loaded()` alongside archive-derived bridge extensions. Needed
    /// because the shared `elephc_pdo` archive cannot identify a surface by itself.
    pub(super) linked_php_surfaces: &'a [String],
    pub(super) ir_module: ir::Module,
    pub(super) web: bool,
    pub(super) web_isolation: codegen::WebIsolation,
    pub(super) extra_link_libs: &'a [String],
    pub(super) extra_link_paths: &'a [String],
    pub(super) extra_frameworks: &'a [String],
    pub(super) required_libraries: &'a [String],
    pub(super) target: Target,
    pub(super) emit: Emit,
    pub(super) heap_size: usize,
    pub(super) gc_stats: bool,
    pub(super) counters: bool,
    pub(super) instrument: crate::codegen::Instrumentation,
    pub(super) heap_debug: bool,
    pub(super) exported_functions: &'a HashMap<String, exports::ExportedFunction>,
    pub(super) regalloc_linear: bool,
    pub(super) emit_debug_info: bool,
    pub(super) keep_symbols: bool,
    pub(super) output_paths: &'a OutputPaths,
    pub(super) emit_source_map: bool,
    pub(super) emit_asm: bool,
    pub(super) timings: &'a mut CompileTimings,
}

/// Generates user assembly, resolves native requirements, and links the requested artifact.
/// Restricts a file to its owner (0600).
///
/// The build key is written with whatever the umask allows, which on a normal
/// system is world-readable — and possession of that file is the entire remote
/// credential. Anyone on the host could read the key out of the deployed binary
/// too, which is by design, but a sidecar sitting at 0644 next to it makes that
/// a `cat` rather than a hex dump.
fn restrict_to_owner(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

/// Runs the backend half of a build: emit the assembly, assemble it, and link
/// the result against the runtime and whichever bridge crates were requested.
pub(super) fn emit_and_link(inputs: BackendInputs<'_>) {
    let BackendInputs {
        filename,
        with_crates,
        linked_php_surfaces,
        mut ir_module,
        web,
        web_isolation,
        extra_link_libs,
        extra_link_paths,
        extra_frameworks,
        required_libraries,
        target,
        emit,
        heap_size,
        gc_stats,
        counters,
        instrument,
        heap_debug,
        exported_functions,
        regalloc_linear,
        emit_debug_info,
        keep_symbols,
        output_paths,
        emit_source_map,
        emit_asm,
        timings,
    } = inputs;

    // Dynamic source is opaque to AOT feature detection. `--with-regex`
    // explicitly enables the ordinary regex runtime/native requirement and
    // lets eval setup register that managed provider with Magician.
    if with_crates.contains("regex") {
        ir_module.required_runtime_features.regex = true;
    }
    // `unlink()`'s lowering publishes the PHAR deletion bridge by taking the ADDRESS of an extern
    // only the `elephc-phar` staticlib defines, so it may only do so when that staticlib is being
    // linked. The CHECKER is what decides that — `file_put_contents("phar://…")` requires it just
    // as `new Phar` does — and its verdict never reached the lowering, which published for every
    // dynamic path and left CI unable to link any mysqli program.
    if required_libraries.iter().any(|library| library == "elephc_phar") {
        ir_module.required_runtime_features.phar_archive = true;
    }
    let probe = with_crates.contains("probe");
    if probe {
        // A build that cannot produce a real key does not produce a binary. The
        // key is the only thing standing between a production endpoint and
        // anyone who can reach it, so a weaker one is worse than none: it looks
        // like a credential in every message and holds like nothing.
        let key = match crate::probe_key::build_key() {
            Ok(key) => key,
            Err(error) => {
                crate::progress::clear();
                eprintln!(
                    "Error: --with-monitoring needs a build key and {error}.\n  \
                     Set ELEPHC_PROBE_KEY to 64 hex characters to supply one."
                );
                process::exit(1);
            }
        };
        eprintln!("probe build fingerprint: {}", crate::probe_key::fingerprint(&key));
        ir_module.probe_key = Some(key);
    }
    let mut runtime_features = ir_module.required_runtime_features;
    // `--web` selects the output-capture variant of `__rt_stdout_write`. This is the
    // sole driver of the web runtime feature: it is CLI-driven, not derived from the
    // program, so the runtime cache (keyed on the generated assembly hash) keeps the
    // web and non-web runtime objects distinct automatically.
    runtime_features.web = web;

    let runtime_link_requirements =
        codegen::link_requirements_for_runtime_features(runtime_features);

    // Bridge-backed `--with-<name>` values force-link their staticlib
    // (whole-archived via `forced_bridge_libs`) regardless of feature
    // auto-detection. Runtime-only capabilities such as regex have already
    // updated `runtime_features` and intentionally do not map to a bridge.
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
        .chain(required_libraries.iter())
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
    // PHP extension (tz -> date, eval) are skipped, and so is `elephc_pdo`,
    // whose archive backs more than one PHP surface (its table row maps to
    // None). The injected PHP surfaces ("PDO", "mysqli") are appended instead.
    // Seeded into a codegen thread-local because extension folding happens
    // during instruction lowering.
    let mut linked_extensions: Vec<String> = Vec::new();
    for lib in &planned_link_libraries {
        if let Some(ext) = linker::php_extension_for_lib(lib) {
            if !linked_extensions.iter().any(|existing| existing == ext) {
                linked_extensions.push(ext.to_string());
            }
        }
    }
    for surface in linked_php_surfaces {
        if !linked_extensions.iter().any(|existing| existing == surface) {
            linked_extensions.push(surface.clone());
        }
    }
    codegen::set_linked_extensions(linked_extensions);

    crate::progress::phase("codegen");
    let phase_started = Instant::now();
    let user_asm = match codegen::generate_user_asm_from_ir_with_options(
        &ir_module,
        gc_stats,
        counters,
        instrument,
        probe,
        heap_debug,
        requires_elephc_tls,
        emit,
        exported_functions,
        regalloc_linear,
        web,
        web_isolation,
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

    crate::progress::phase("runtime-cache");
    let phase_started = Instant::now();
    let runtime_object = match runtime_cache::prepare_runtime_object_for_emit(
        heap_size,
        target,
        runtime_features,
        emit,
    ) {
        Ok(runtime_object) => runtime_object,
        Err(err) => {
            crate::progress::clear();
            eprintln!("Runtime cache error: {}", err);
            process::exit(1);
        }
    };
    timings.record_since("runtime-cache", phase_started);
    timings.note(format!("Runtime cache: {}", runtime_object.status.as_str()));

    let mut native_requirements: Vec<NativeRequirement> = runtime_link_requirements
        .iter()
        .filter_map(|requirement| match requirement {
            LinkRequirement::NativePackage(package) => {
                Some(NativeRequirement::package(*package))
            }
            LinkRequirement::Bridge(_) | LinkRequirement::SystemLibrary(_) => None,
        })
        .collect();
    // The curl bridge is a Rust `staticlib` (`elephc_curl`, planned above like any
    // other bridge) that itself links against the managed native `curl` package
    // (which pulls in `openssl`/`zlib` transitively through the catalog). Unlike
    // `regex`, curl has no `RuntimeFeatures` bit: `elephc_curl` reaches
    // `planned_link_libraries` either because the program actually uses curl (every
    // curl `RuntimeFnId` declares `BuiltinRequirement::Bridge("elephc_curl")`, and the
    // curl prelude is injected only when `src/curl_prelude/detect.rs` finds a `curl_*`
    // reference) or because `--with-curl` forces it explicitly with no such reference;
    // this mirrors that into the native requirement so the
    // final link resolves `libcurl.a`/`libssl.a`/`libcrypto.a`/`libz.a` instead of
    // failing on `elephc_curl`'s undefined libcurl symbols.
    if planned_link_libraries
        .iter()
        .any(|library| library == "elephc_curl")
    {
        native_requirements.push(NativeRequirement::package("curl"));
    }
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
        user_libraries: extra_link_libs,
        user_search_paths: extra_link_paths,
        user_frameworks: extra_frameworks,
        checker_libraries: &required_libraries,
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
    if matches!(emit, Emit::Staticlib) {
        // The consuming project performs the final link and supplies bridge and
        // managed-native archives alongside this library.
        linker::archive(&output_paths.bin, &output_paths.obj, &runtime_object.path);
    } else {
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
    }
    timings.record_since("link", phase_started);

    if let Some(header_path) = &output_paths.header {
        let library_stem = header_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("libelephc_module");
        let exports = exported_functions.values().collect::<Vec<_>>();
        let header = exports::render_c_header(library_stem, &exports);
        if let Err(error) = fs::write(header_path, header) {
            crate::progress::clear();
            eprintln!("Error writing '{}': {}", header_path.display(), error);
            process::exit(1);
        }
    }

    // With --debug-info the DWARF line tables must be preserved past object
    // cleanup: on macOS `dsymutil` bakes them into a .dSYM while the object
    // still exists; if that fails the object is kept so debuggers can follow
    // the binary's debug map to it.
    let keep_obj_for_debug = emit_debug_info
        && !matches!(emit, Emit::Staticlib)
        && !linker::bake_debug_info(target, &output_paths.bin);

    // Strip after the dSYM is baked, never before: `dsymutil` reads the binary's debug map, and
    // a stripped binary has none. `--debug-info` and `--keep-symbols` both opt out — the first
    // because stripping would undo what it was asked for, the second for profilers, which read
    // the symbol table and have no other way to get names.
    if !emit_debug_info && !keep_symbols {
        if let Err(error) = linker::strip_symbols(target, emit, &output_paths.bin) {
            eprintln!("Warning: could not strip symbols ({error}); keeping the larger binary");
        }
    }
    if !keep_obj_for_debug {
        let _ = fs::remove_file(&output_paths.obj);
    }

    // Write the build key next to the binary: `elephc monitor <address> --key`
    // reads it to run the HMAC handshake. Keep it like a `.env` secret.
    if let Some(key) = ir_module.probe_key {
        let sidecar = output_paths.bin.with_extension("key");
        if let Err(err) = fs::write(&sidecar, crate::probe_key::to_hex(&key)) {
            eprintln!("warning: could not write the build key {}: {err}", sidecar.display());
        } else if let Err(err) = restrict_to_owner(&sidecar) {
            // Not fatal — the key is still usable — but say it, because the
            // whole point of the file is that only its owner can read it.
            eprintln!(
                "warning: could not restrict {} to its owner: {err}",
                sidecar.display()
            );
        }
    }

    crate::progress::clear();
    timings.report();
    if let Some(warning) = dynamic_eval_capability_warning(runtime_features) {
        eprintln!("{warning}");
    }
    crate::progress::finish_ok(
        &format!("Compiled '{}' -> '{}'", filename, output_paths.bin.display()),
        timings.elapsed(),
    );
}
