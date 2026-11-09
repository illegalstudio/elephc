//! Purpose:
//! Materializes runtime/codegen outputs and performs native assembly and linking.
//!
//! Called from:
//! - `crate::pipeline::compile()` after EIR lowering and optimization.
//!
//! Key details:
//! - Assembly-only output returns before runtime-object preparation, native resolution, assembly, and linking.

use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};

use super::*;

/// Inputs consumed by the post-EIR backend and linker pipeline.
pub(super) struct BackendInputs<'a> {
    pub(super) repl_build: bool,
    /// Whether to print the final `Compiled '...' -> '...'` line. An `elephc test` build
    /// turns it off: the binary lives under the cache and is run immediately, so the line
    /// would only push the suite's own report down the terminal.
    pub(super) announce_success: bool,
    pub(super) filename: &'a str,
    pub(super) with_crates: &'a HashSet<String>,
    pub(super) ini_overrides: &'a [(String, String)],
    /// PHP surfaces injected into this compilation ("PDO", "mysqli"), reported to
    /// `extension_loaded()` alongside archive-derived bridge extensions. Needed
    /// because the shared `elephc_pdo` archive cannot identify a surface by itself.
    pub(super) linked_php_surfaces: &'a [String],
    /// Real PHP extensions the project hosts: their archives, the Zend engine
    /// archive they resolve against, and the C++ runtime when one needs it.
    pub(super) hosted_php_ext: Option<&'a crate::php_ext::install::HostedExtensions>,
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
    pub(super) exported_constants: &'a [crate::phpext::ExportedConstant],
    pub(super) exported_classes: &'a [crate::phpext::classes::ExportedClass],
    pub(super) regalloc_linear: bool,
    pub(super) emit_debug_info: bool,
    pub(super) keep_symbols: bool,
    pub(super) output_paths: &'a OutputPaths,
    pub(super) emit_source_map: bool,
    pub(super) emit_asm: bool,
    pub(super) timings: &'a mut CompileTimings,
    /// Present when bodies the backend refuses may belong to class files the runtime autoloader
    /// can load instead; see `autoload::deferral::BackendDeferral`.
    pub(super) deferral: Option<crate::autoload::deferral::BackendDeferral>,
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
        repl_build,
        announce_success,
        filename,
        with_crates,
        ini_overrides,
        linked_php_surfaces,
        hosted_php_ext,
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
        exported_constants,
        exported_classes,
        regalloc_linear,
        emit_debug_info,
        keep_symbols,
        output_paths,
        emit_source_map,
        emit_asm,
        timings,
        deferral,
    } = inputs;

    // Dynamic source is opaque to AOT feature detection. `--with-regex`
    // explicitly enables the ordinary regex runtime/native requirement and
    // lets eval setup register that managed provider with Magician.
    if with_crates.contains("regex") {
        ir_module.required_runtime_features.regex = true;
    }
    if with_crates.contains("mbstring") {
        ir_module.required_runtime_features.mbstring = true;
        ir_module.required_runtime_features.mbregex = true;
        ir_module.required_runtime_features.mbstring_mime = true;
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
    let configures_mbstring = ir_module.required_runtime_features.mbstring
        || ir_module.required_runtime_features.mbregex
        || ir_module.required_runtime_features.eval_bridge;
    if configures_mbstring {
        ir_module.required_runtime_features.mbstring_mime |= ini_overrides.iter().any(|(name, _)| {
            name == "mbstring.http_output_conv_mimetypes"
        });
        ir_module.mbstring_startup = Some(super::mbstring_configuration::arguments(ini_overrides));
    }
    let mut runtime_features = ir_module.required_runtime_features;
    // `--web` selects the output-capture variant of `__rt_stdout_write`. This is the
    // sole driver of the web runtime feature: it is CLI-driven, not derived from the
    // program, so the runtime cache (keyed on the generated assembly hash) keeps the
    // web and non-web runtime objects distinct automatically.
    runtime_features.web = web;

    let runtime_link_requirements =
        codegen::link_requirements_for_runtime_features(runtime_features);
    let embedded_bridges: &[&str] = if runtime_features.eval_bridge {
        &["elephc_crypto", "elephc_phar"]
    } else {
        &[]
    };

    // Bridge-backed `--with-<name>` values force-link their staticlib
    // (whole-archived via `forced_bridge_libs`) regardless of feature
    // auto-detection. `--web` separately whole-archives the web bridge because it
    // owns the process entry point; CLI session calls use its referenced archive
    // members without pulling in that entry point. Runtime-only capabilities such
    // as regex have already updated `runtime_features` and do not map to a bridge.
    let mut forced_bridge_libs: Vec<String> = Vec::new();
    let mut sorted_with_crates: Vec<&String> = with_crates.iter().collect();
    sorted_with_crates.sort();
    for flag in sorted_with_crates {
        if let Some(lib) = linker::bridge_lib_for_flag(flag) {
            forced_bridge_libs.push(lib.to_string());
        }
    }
    let forced_whole_archive_bridges = whole_archive_bridges(&forced_bridge_libs, web);

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

    if emit == Emit::Phpext && (hosted_php_ext.is_some() || !planned_link_libraries.is_empty()) {
        eprintln!("{PHPEXT_CORE_RUNTIME_ONLY}");
        process::exit(1);
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
        for ext in linker::php_extensions_for_lib(lib) {
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
    // Completed text leaves memory in 32 MB steps; the spill file becomes the `.s` itself.
    let mut spill_path = output_paths.asm.clone().into_os_string();
    spill_path.push(".spill");
    let spill_path = std::path::PathBuf::from(spill_path);
    // Nobody reads this assembly in an ordinary build, so it is written compact (no comments,
    // short local labels). `--emit-asm` hands it to the user, `--debug-info` is stepped
    // through and the source map reads its `@src`/`@block` markers, so all three keep the
    // readable form.
    crate::codegen_support::emit::set_compact_asm(
        !emit_asm
            && !emit_debug_info
            && !emit_source_map
            && !matches!(std::env::var("ELEPHC_COMPACT_ASM").as_deref(), Ok("0") | Ok("off")),
    );
    let generated_user_asm = match codegen::generate_user_asm_from_ir_with_options_and_features(
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
        Some(&spill_path),
    ) {
        Ok(output) => output,
        Err(err) => {
            // Bodies of a class php would have autoloaded lazily are handed to the runtime and
            // the build runs again; this returns only when nothing new can be handed back.
            if let Some(deferral) = &deferral {
                // The re-run replaces this process, so nothing would remove the partial spill.
                let _ = std::fs::remove_file(&spill_path);
                deferral.rerun_without_refused_bodies(err.failed_bodies());
            }
            crate::progress::clear();
            eprintln!("EIR backend error: {}", err);
            process::exit(1);
        }
    };
    codegen::print_op_stats();
    runtime_features.handler_state |= generated_user_asm.handler_state;
    runtime_features.object_clone |= generated_user_asm.object_clone;
    let user_asm = if emit_debug_info {
        codegen::UserAssembly::from_text(debug_info::inject_line_directives(
            &generated_user_asm.asm.into_string(),
            filename,
            target.platform,
        ))
    } else {
        generated_user_asm.asm
    };
    let user_asm = if emit == Emit::Phpext {
        // The extension build combines text, so the structured assembly is flattened first.
        match crate::phpext::build::assembly(&user_asm.into_string(), heap_size, target, runtime_features) {
            Ok(asm) => codegen::UserAssembly::from_text(asm),
            Err(error) => { eprintln!("PHP extension error: {error}"); process::exit(1); }
        }
    } else { user_asm };
    timings.record_since("codegen", phase_started);
    // The EIR module is not read past this point (only its probe key is), and it is one of the
    // largest live structures: freeing it here lets the phases below reuse its pages instead of
    // growing the heap past them.
    let probe_key = ir_module.probe_key;
    let runs_interpreted_code = crate::ir_lower::module_runs_interpreted_code(&ir_module);
    drop(ir_module);

    crate::progress::phase("write-asm");
    let phase_started = Instant::now();
    if let Err(e) = user_asm.write_to(&output_paths.asm) {
        crate::progress::clear();
        eprintln!("Error writing '{}': {}", output_paths.asm.display(), e);
        process::exit(1);
    }
    timings.record_since("write-asm", phase_started);

    if emit_source_map {
        crate::progress::phase("source-map");
        let phase_started = Instant::now();
        // The text is on disk now; this rare path reads it back rather than keeping a copy.
        let user_asm = match fs::read_to_string(&output_paths.asm) {
            Ok(text) => text,
            Err(err) => {
                crate::progress::clear();
                eprintln!("Source map error: {}", err);
                process::exit(1);
            }
        };
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

    if emit == Emit::Phpext {
        crate::progress::phase("php-extension-link");
        let phase_started = Instant::now();
        // The assembly is on disk by now; the extension linker takes its text.
        let linked = fs::read_to_string(&output_paths.asm)
            .map_err(|error| format!("read PHP extension assembly: {error}"))
            .and_then(|text| crate::phpext::build::link(&text, exported_functions, exported_constants, exported_classes, target, &output_paths.bin));
        if let Err(error) = linked {
            crate::progress::clear();
            eprintln!("PHP extension error: {error}");
            process::exit(1);
        }
        timings.record_since("php-extension-link", phase_started);
        crate::progress::clear();
        timings.report();
        crate::progress::finish_ok(&format!("Compiled '{}' -> '{}'", filename, output_paths.bin.display()), timings.elapsed());
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
    // The xml bridge is the same shape as curl: `elephc_xml` is a Rust `staticlib`
    // whose parser is libxml2 itself, reached through the Elephc-owned C shim the
    // managed `libxml2` package builds and archives next to `libxml2.a`. It has no
    // `RuntimeFeatures` bit either: `elephc_xml` reaches `planned_link_libraries`
    // because the program names part of the surface (the xml prelude is injected
    // only when `src/xml_prelude` detects an `xml_*`/`xmlwriter_*` call or an
    // `XMLParser`/`XMLWriter` reference, and its `extern "elephc_xml"` block is what
    // requires the bridge) or because `--with-xml` forces it. Mirroring that into
    // the native requirement makes the final link resolve
    // `libelephc_libxml2_shim.a` + `libxml2.a` from the project's catalog package
    // instead of failing on the bridge's undefined `elephc_libxml2_v1_*`/`xml*`
    // symbols — and, exactly like curl, there is no system `-lxml2` fallback.
    if planned_link_libraries
        .iter()
        .any(|library| library == "elephc_xml")
    {
        native_requirements.push(NativeRequirement::package("libxml2"));
    }
    // The gmp bridge is the same shape again: `elephc_gmp` is a Rust `staticlib`
    // whose raw operations call libgmp through `extern "C"` declarations. The
    // injected GMP prelude's `extern "elephc_gmp"` block is what puts the bridge in
    // `planned_link_libraries`; mirroring it into the native requirement resolves
    // `libgmp.a` from the project's managed `gmp` package. There is deliberately no
    // system `-lgmp` fallback.
    if planned_link_libraries
        .iter()
        .any(|library| library == "elephc_gmp")
    {
        native_requirements.push(NativeRequirement::package("gmp"));
    }
    if planned_link_libraries.iter().any(|lib| lib == "elephc_fileinfo") {
        native_requirements.push(NativeRequirement::package("libmagic"));
    }
    let mut resolved_native = match crate::native_deps::resolve_for_compilation(
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
    if let Some(hosted) = hosted_php_ext {
        if let Err(error) = append_hosted_extensions(&mut resolved_native, hosted, target) {
            eprintln!("{error}");
            process::exit(1);
        }
    }
    let link_plan = crate::link_planning::build(crate::link_planning::LinkPlanningInputs {
        user_libraries: extra_link_libs,
        user_search_paths: extra_link_paths,
        user_frameworks: extra_frameworks,
        checker_libraries: &required_libraries,
        runtime_requirements: &runtime_link_requirements,
        managed_packages: &resolved_native,
        forced_bridges: &forced_bridge_libs,
        embedded_bridges,
        web,
    });

    crate::progress::phase("assemble");
    let phase_started = Instant::now();
    // `as` is single-threaded and the user object is the largest input in the
    // build, so on a large program this one call is a serial wall. When the
    // target supports it the finished `.s` is cut into contiguous slices that
    // assemble in parallel; every slice object then joins the link line in
    // emission order. `ELEPHC_ASM_JOBS=1` keeps the single call.
    let assembled = linker::assemble_parallel(
        target,
        emit,
        emit_debug_info,
        &output_paths.asm,
        &output_paths.obj,
    );
    timings.record_since("assemble", phase_started);
    if let Some(note) = &assembled.note {
        timings.note(note.clone());
    }
    let extra_objects: &[std::path::PathBuf] = assembled.objects.get(1..).unwrap_or(&[]);

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
        let linked = linker::link_with_plan_and_inputs(
            target,
            emit,
            &output_paths.bin,
            &output_paths.obj,
            extra_objects,
            &runtime_object.path,
            &link_plan,
            &forced_whole_archive_bridges,
        );
        match linked {
            Ok(inputs) if repl_build => {
                let path = output_paths.bin.with_extension("inputs.json");
                let contents = serde_json::to_vec(&inputs).expect("serializable linker inputs");
                if let Err(error) = fs::write(&path, contents) {
                    eprintln!("Cannot record REPL dependencies: {error}");
                    process::exit(1);
                }
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("Linker error: {error}");
                process::exit(1);
            }
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
        if let Err(error) =
            crate::pipeline::artifact_io::write_artifact(header_path, header.as_bytes())
        {
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
        // The extra slice objects only exist for the link that just happened.
        // `--debug-info` never splits, so `keep_obj_for_debug` cannot be true
        // while this list is non-empty; the guard is shared anyway so a future
        // split under a debug map cannot silently delete its own debug objects.
        for object in extra_objects {
            let _ = fs::remove_file(object);
        }
    }

    // Write the build key next to the binary: `elephc monitor <address> --key`
    // reads it to run the HMAC handshake. Keep it like a `.env` secret.
    if let Some(key) = probe_key {
        let sidecar = output_paths.bin.with_extension("key");
        if let Err(err) = crate::pipeline::artifact_io::write_private_artifact(
            &sidecar,
            crate::probe_key::to_hex(&key).as_bytes(),
        ) {
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
    if let Some(warning) = dynamic_eval_capability_warning(
        runtime_features,
        runs_interpreted_code,
    ) {
        eprintln!("{warning}");
    }
    if announce_success {
        crate::progress::finish_ok(
            &format!("Compiled '{}' -> '{}'", filename, output_paths.bin.display()),
            timings.elapsed(),
        );
    }
}

/// Appends each hosted PHP extension's archive to the link, then the engine
/// they all resolve Zend symbols from (`libelephc_zend.a`, the elephc-zend
/// bridge). Static archives resolve in command-line order on GNU ld, so the
/// engine must come after every extension, and after the bridges a program
/// links by name (which precede managed archives). A C++ extension also needs
/// the platform's C++ runtime, which nothing else in a compiled program links.
fn append_hosted_extensions(
    resolved: &mut Vec<crate::native_deps::ResolvedNativePackage>,
    hosted: &crate::php_ext::install::HostedExtensions,
    target: Target,
) -> Result<(), String> {
    let engine = crate::linker::bridge_archive("elephc_zend").map_err(|error| error.to_string())?;
    let cxx_runtime = match target.platform {
        crate::codegen_support::platform::Platform::Linux => "stdc++",
        _ => "c++",
    };
    let packages = hosted.extensions.iter().map(|extension| {
        let mut system_libraries = extension.record.libraries.clone();
        if extension.record.cxx {
            system_libraries.push(cxx_runtime.to_string());
        }
        crate::native_deps::ResolvedNativePackage {
            package: format!("extension:{}", extension.name),
            artifact_root: extension.artifact_dir.clone(),
            archives: vec![extension.archive()],
            system_libraries,
            library_paths: extension.record.library_dirs.iter().map(std::path::PathBuf::from).collect(),
            frameworks: Vec::new(),
        }
    });
    resolved.extend(packages);
    resolved.push(crate::native_deps::ResolvedNativePackage {
        package: crate::link_planning::HOSTED_ENGINE_PACKAGE.to_string(),
        artifact_root: engine.parent().map(Path::to_path_buf).unwrap_or_default(),
        archives: vec![engine],
        library_paths: Vec::new(),
        // The engine's `php_iconv_string` uses the system iconv, a separate
        // library on Apple targets (part of the C library on Linux).
        system_libraries: match target.platform {
            crate::codegen_support::platform::Platform::Linux => vec!["dl".to_string(), "pthread".to_string(), "m".to_string()],
            _ => vec!["iconv".to_string()],
        },
        frameworks: Vec::new(),
    });
    Ok(())
}

/// Adds the web entrypoint bridge to the whole-archive set only in server mode.
fn whole_archive_bridges(forced: &[String], web: bool) -> Vec<String> {
    let mut bridges = forced.to_vec();
    if web && !bridges.iter().any(|name| name == "elephc_web") {
        bridges.push("elephc_web".to_string());
    }
    bridges
}

#[cfg(test)]
mod tests {
    use super::whole_archive_bridges;

    #[test]
    fn web_mode_whole_archives_only_its_entrypoint_in_addition_to_explicit_bridges() {
        assert_eq!(
            whole_archive_bridges(&["elephc_crypto".to_string()], true),
            ["elephc_crypto", "elephc_web"]
        );
        assert!(whole_archive_bridges(&[], false).is_empty());
    }
}
