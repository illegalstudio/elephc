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

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
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
    autoload, codegen, debug_info, dom_prelude, errors, exports, func_args, ir, ir_lower, ir_passes,
    lexer, linker, list_id_prelude, mysqli_prelude, name_resolver, opcache_prelude, optimize,
    parser, pdo_prelude, resolver, runtime_cache, source_map, tz_prelude, types, var_export_prelude,
    web_prelude,
};

mod artifact_io;
pub(crate) use artifact_io::write_artifact;
mod backend;
mod eir_output;
mod frontend;
mod mbstring_configuration;
mod output;

use output::{dynamic_eval_capability_warning, output_paths, OutputPaths};

/// Keeps source validation and native link rejection consistent for unsupported producer inputs.
const PHPEXT_CORE_RUNTIME_ONLY: &str = "PHP extension output currently requires the core runtime without bridges, hosted extensions, web or process monitoring";

/// Runs the full compilation pipeline from PHP source to native binary.
/// Reads PHP source, tokenizes, parses, resolves names, type-checks, optimizes,
/// generates assembly, and links into a native binary. Exits on any error.
pub(crate) fn compile(config: CliConfig) {
    compile_inner(config, EntryKind::Source);
}

/// Compiles the embedded REPL host with a logical project source and cache-local outputs.
pub(crate) fn compile_repl(config: CliConfig, output: &str) {
    compile_inner(config, EntryKind::ReplHost { output });
}

/// Compiles an `elephc test` suite: `source` is the synthesized entry (the `require_once`
/// of every test file) whose logical path is `config.filename`, the binary goes to
/// `output`, and compile-time discovery appends the driver for the tests `filter` keeps.
pub(crate) fn compile_test(
    config: CliConfig,
    source: &str,
    output: &str,
    options: &crate::testing_prelude::discover::DriverOptions,
) {
    compile_inner(config, EntryKind::TestSuite { source, output, options });
}

/// How the entry program reaches the pipeline and where its outputs go.
///
/// The REPL host and a test suite are both VIRTUAL entries: generated source with a logical
/// path in the project, outputs under the cache. They differ in what the project contributes:
/// the host compiles no project code, so it skips the autoload registry; a suite is project
/// code, so it keeps it and additionally runs test discovery after autoloading.
#[derive(Clone, Copy)]
pub(crate) enum EntryKind<'a> {
    /// The ordinary command: the entry file on disk, outputs beside it.
    Source,
    /// The embedded REPL host: generated bootstrap, no autoload, outputs at `output`.
    ReplHost { output: &'a str },
    /// An `elephc test` suite: generated entry, ordinary autoload, outputs at `output`,
    /// and the driver synthesized with `options` (filter, isolation, timeout, report).
    TestSuite {
        source: &'a str,
        output: &'a str,
        options: &'a crate::testing_prelude::discover::DriverOptions,
    },
}

impl<'a> EntryKind<'a> {
    /// The generated source for a virtual entry, `None` when the file on disk is read.
    fn generated_source(self) -> Option<&'a str> {
        match self {
            Self::Source => None,
            Self::ReplHost { .. } => Some(crate::repl::HOST_SOURCE),
            Self::TestSuite { source, .. } => Some(source),
        }
    }

    /// The output path override, `None` when outputs go beside the entry file.
    fn output(self) -> Option<&'a str> {
        match self {
            Self::Source => None,
            Self::ReplHost { output } | Self::TestSuite { output, .. } => Some(output),
        }
    }

    /// Whether the entry is virtual: only its project directory exists on disk.
    fn is_virtual(self) -> bool {
        !matches!(self, Self::Source)
    }

    /// Whether this is the embedded REPL host build.
    fn is_repl_host(self) -> bool {
        matches!(self, Self::ReplHost { .. })
    }

    /// Whether this is an `elephc test` build.
    fn is_test_suite(self) -> bool {
        matches!(self, Self::TestSuite { .. })
    }
}

/// Shares every compiler pass while separating generated host storage from project discovery.
fn compile_inner(config: CliConfig, entry: EntryKind<'_>) {
    optimize::with_hosted_output(config.emit == Emit::Phpext, || compile_config(config, entry));
}

/// Runs compiler passes with the output kind's host-callback profile installed.
fn compile_config(config: CliConfig, entry: EntryKind<'_>) {
    // One numbering of included/autoloaded files per compile; see `resolver::resolve_*`.
    crate::span::Span::reset_source_ids();
    crate::source::reset_source_memos();
    // Before any file is parsed: an earlier build's PHPDoc erasures apply from the first round.
    crate::docblock::erasure::set_entry(Path::new(&config.filename));
    let CliConfig {
        filename,
        heap_size,
        gc_stats,
        counters,
        instrument,
        heap_debug,
        strict_opcache,
        emit_ir,
        output_dir,
        null_repr,
        emit_asm,
        emit,
        export_constants,
        check_only,
        emit_timings,
        emit_source_map,
        report_generics,
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
        lint,
        web,
        closed_world,
        web_isolation,
        web_application,
        with_crates,
        quiet,
        ini_overrides,
    } = config;
    // A REPL host evaluates what the user types: it always links the interpreter.
    let closed_world_mode = if entry.is_repl_host() { crate::closed_world::Mode::Off } else { closed_world };
    let mut closed_world = closed_world_mode != crate::closed_world::Mode::Off;
    crate::closed_world::set_enabled(closed_world);
    let filename = filename.as_str();
    crate::progress::init(quiet);
    // Reject incompatible options before check/EIR output can return without the backend.
    if emit == Emit::Phpext && (web || !extra_link_libs.is_empty()
        || !extra_link_paths.is_empty() || !extra_frameworks.is_empty()
        || !with_crates.is_empty() || counters || gc_stats
        || instrument != crate::codegen::Instrumentation::Off) {
        crate::progress::clear();
        eprintln!("{PHPEXT_CORE_RUNTIME_ONLY}");
        process::exit(1);
    }
    codegen::set_null_repr(null_repr);
    // Record the PHP language profile and SAPI mode BEFORE any prelude or lowering runs: it is
    // the single source of truth for the reported version surface (`PHP_VERSION` and friends,
    // `PHP_SAPI`, `phpversion()`), which is baked far below this function's parameter list — in
    // `codegen_support::prescan::collect_constants` and in the `phpversion()` const-fold.
    codegen::set_compile_profile(php_version, web);
    codegen::set_ini_overrides(ini_overrides.clone());
    crate::superglobals::set_compiling_for_web(web);
    // The SAPI also reaches the LEXER: php's CLI removes a leading `#!` line from every file it
    // compiles and a web SAPI echoes it (`crate::sapi`). This runs on the dedicated compile
    // thread that owns the whole front end, which is the thread that will scan those files.
    crate::sapi::set_web(web);
    // php's `$_SERVER` path keys name the SCRIPT, and a compiled program's script is the entry
    // it was built from. Absolute, because code that re-reads it (`autoload_runtime.php` does
    // `require $_SERVER['SCRIPT_FILENAME']`) may run from any working directory.
    crate::superglobals::set_entry_script(
        &std::fs::canonicalize(filename)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| filename.to_string()),
    );
    crate::strict_php::set_enabled(strict_php);
    let parent = Path::new(filename).parent().unwrap_or(Path::new("."));
    let source_mode = SourceMode::from_path(Path::new(filename));
    // A virtual entry (REPL host, test suite) writes to its own cache-local `output`.
    let output_dir = output_dir.filter(|_| !entry.is_virtual()).map(|raw| {
        if raw.is_absolute() {
            raw
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(&raw))
                .unwrap_or(raw)
        }
    });
    if let Some(directory) = &output_dir {
        if let Err(error) = fs::create_dir_all(directory) {
            crate::progress::clear();
            eprintln!(
                "error: cannot create output directory '{}': {}",
                directory.display(),
                error
            );
            process::exit(1);
        }
    }
    let output_paths = output_paths(
        entry.output().unwrap_or(filename),
        target,
        emit,
        output_dir.as_deref(),
    );
    // An input with no extension makes its own stem: `bin/console` compiles to `bin/console`,
    // and the link step would write the executable over the PHP file it was built from. Refuse
    // before anything is read, and name the flag that resolves it -- a destroyed source is not
    // something a later error can undo. A virtual entry has no file of its own on disk.
    for (label, produced) in [
        ("executable", &output_paths.bin),
        ("assembly", &output_paths.asm),
        ("object", &output_paths.obj),
    ] {
        if !entry.is_virtual() && same_file(produced, Path::new(filename)) {
            eprintln!(
                "error: the {label} output would overwrite the input {}; \
                 give the build its own directory with --output-dir DIR",
                produced.display()
            );
            process::exit(1);
        }
    }
    // BEFORE ANY WORK RUNS. Every generated path is derived from the source filename, so a
    // tree someone else controls chooses them; refusing a symlink destination up front is
    // the only way to cover the ones an external assembler or linker writes, which this
    // process never opens itself (issue #888).
    let artifact_plan = artifact_io::ArtifactPlan::for_run(
        check_only,
        emit_ir,
        emit_asm,
        emit_source_map,
        with_crates.contains("probe"),
    );
    if let Err(error) = artifact_io::reject_unsafe_destinations(&output_paths, &artifact_plan) {
        eprintln!("error: {error}");
        process::exit(1);
    }
    let mut timings = CompileTimings::new(emit_timings);

    // The entry file uses source ID 0. Register it before any frontend or autoload diagnostic,
    // including warnings from rejected autoload closures. The resolver only resets IDs from 1
    // upwards and leaves the entry's path registered.
    Span::register_source_path(0, filename.to_string());
    let (parsed, entry_source_unit) = frontend::read_and_parse(
        filename, source_mode, &defines, &mut timings,
        entry.generated_source(),
    );

    // `opcache.preload` becomes an implicit `require_once` at the very top of the entry program,
    // which is what reference PHP's startup preload pass IS for a compiler: the resolver inlines
    // the target below, so its declarations are baked into the binary and its top-level code runs
    // first. Injected BEFORE the autoload registry so a preload file that registers an autoloader
    // or declares an autoloadable class is seen by both, and before `resolve` so its transitive
    // requires join the OPcache script manifest. An unresolvable path is refused in here, at the
    // compile-time position closest to reference's startup fatal.
    let parsed = opcache_prelude::inject_preload_require(
        parsed,
        php_version,
        web,
        &ini_overrides,
        filename,
    );

    crate::progress::phase("autoload-build");
    let phase_started = Instant::now();
    let (autoload_registry, parsed) = if entry.is_repl_host() {
        (autoload::Registry::empty(), parsed)
    } else if entry.is_test_suite() {
        // The compiler stands in for PHPUnit: an installed `phpunit/phpunit` (and what only
        // it requires) must not be autoloaded into the suite. See `autoload::dev_packages`.
        autoload::Registry::build_excluding(parent, parsed, &autoload::phpunit_closure(parent))
    } else {
        autoload::Registry::build(parent, parsed)
    };
    codegen::set_autoload_rule_count(autoload_registry.rule_count());
    for warning in autoload_registry.warnings() {
        errors::report_warning(warning);
    }
    timings.record_since("autoload-build", phase_started);

    crate::progress::phase("resolve");
    let phase_started = Instant::now();
    // `resolve_collecting_includes` also hands back the canonical path of every file the
    // resolver statically inlined — group 2 of the OPcache script manifest.
    let (ast, opcache_included_files, mut entry_included_sources) =
        match resolver::resolve_collecting_includes_with_defines_and_sources(
            parsed, parent, &defines,
        ) {
        Ok(resolved) => resolved,
        Err(e) => {
            crate::progress::clear();
            errors::report(&e);
            process::exit(1);
        }
    };
    let ast = match autoload::collect_aliases(ast) {
        Ok(ast) => ast,
        Err(error) => {
            crate::progress::clear();
            errors::report(&error);
            process::exit(1);
        }
    };
    // The runtime self-include (`autoload_runtime.php` requires `$_SERVER['SCRIPT_FILENAME']`) is
    // value-producing, so the compiler-included skip cannot answer it. Give the entry's trailing
    // capture-less closure a synthetic unit function the runtime include-unit dispatch can call,
    // and keep the entry's own statements untouched. Runs on the USER program only, before any
    // compiler prelude is injected: the preludes' own top-level statements would otherwise look
    // like entry side effects and disqualify the shape. CLI entries take it too (`bin/console` is
    // the same shape): the unit is only injected when re-running the entry's prefix would do
    // nothing, so serving the closure IS php's re-execution.
    let (ast, entry_return_injected) = crate::entry_return::inject(ast);
    timings.record_since("resolve", phase_started);
    // The entry program (with what it includes statically) evaluates a computed string: it needs
    // the interpreter, decided before anything is compiled for a closed world.
    if closed_world_mode == crate::closed_world::Mode::Auto
        && crate::closed_world::project_computed_eval(&ast).is_some()
    {
        closed_world = false;
        crate::closed_world::set_enabled(false);
    }


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

    // The preludes below are decided on the entry program, but the autoload pass that loads the
    // Composer class files runs after name resolution, much later. Probe that closure now, so a
    // vendor file that uses PDO or curl gets its prelude as if the entry file used it.
    crate::progress::phase("autoload-prelude-probe");
    let phase_started = Instant::now();
    let autoloaded_preludes = if entry.is_repl_host() {
        HashSet::new()
    } else {
        autoload::prelude_probe::autoloaded_prelude_groups(
            &ast,
            parent,
            &autoload_registry,
            &defines,
        )
    };
    timings.record_since("autoload-prelude-probe", phase_started);
    let autoloaded = |group: &str| autoloaded_preludes.contains(group);

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
        (with_crates.contains("xml"), "xml"),
        (with_crates.contains("bcmath"), "bcmath"),
        (with_crates.contains("gmp"), "gmp"),
        (with_crates.contains("random"), "random"),
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

    // Standard DOM classes are built as AST so they participate in name resolution, type
    // checking, EIR lowering, and closed-world pruning like user classes. They are injected only
    // when the program names one (or a `libxml_*` function); the late gate below covers a program
    // that only an autoloaded class reaches them from.
    let ast = dom_prelude::inject_if_used(ast, &mut prelude_inventory);

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
    let pdo_used = pdo_force || autoloaded("pdo") || pdo_prelude::program_uses_pdo(&ast);
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
    let mysqli_used =
        mysqli_force || autoloaded("mysqli") || mysqli_prelude::program_uses_mysqli(&ast);
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
        with_crates.contains("tz") || autoloaded("tz"),
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
    let ast = var_export_prelude::inject_if_used_or_referenced_elsewhere(
        ast,
        autoloaded("var_export"),
        &mut prelude_inventory,
    );
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
    // statically-resolved include/require target, and eager autoload files. The PSR-4 /
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
    // The embedded REPL entry is virtual, so only its project directory exists on disk.
    let opcache_entry_path = if entry.is_virtual() {
        crate::repl::canonical_entry_path(filename)
    } else {
        opcache_prelude::canonical_entry_path(filename)
    };
    // The SAME verdict for the eval bridge: an OPcache call hidden in a runtime-provided
    // `eval()` source gets no native body, so the interpreter must refuse it itself.
    codegen::set_opcache_api_restricted(opcache_prelude::restrict_api_denies(
        opcache_entry_path.as_deref(),
        php_version.version_id(),
        &ini_overrides,
    ));
    // `opcache.preload` is a COMPILE-TIME decision, resolved here for the same reason
    // `restrict_api` is: reference PHP preloads during STARTUP, before the script runs, and
    // elephc's INI is fixed when the binary is built. The three outcomes mirror reference exactly
    // (see `opcache_prelude::PreloadVerdict` for the verified matrix):
    // - unresolvable path with the cache enabled → HARD COMPILE ERROR, the AOT equivalent of
    //   reference's startup fatal. It fires whether or not the program calls an OPcache function,
    //   because reference's fatal does not depend on that either.
    // - resolvable → the file is preloaded. `inject_preload_require` makes it part of this binary,
    //   so it is always a member of the script manifest and there is no out-of-manifest diagnostic.
    //   Only the manifest-independent compile ERROR is decided here, matching reference PHP, which
    //   fatals at startup regardless of what the script does; the verdict is taken again against
    //   the COMPLETE manifest after `autoload::run`, for `preload_statistics`.
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
        with_crates.contains("image") || autoloaded("image"),
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
    let ast = crate::hash_prelude::inject_if_used(ast, autoloaded("hash"), &mut prelude_inventory);
    let ast = crate::tokenizer_prelude::inject_if_used(
        ast,
        with_crates.contains("tokenizer") || autoloaded("tokenizer"),
        &mut prelude_inventory,
    );
    let ast = crate::core_prelude::inject_if_used(ast, &mut prelude_inventory);
    let ast = crate::bcmath_prelude::inject_if_used(ast, with_crates.contains("bcmath"), &mut prelude_inventory);
    // A `gmp_*` use only an autoloaded vendor class makes (league/uri's `GMPCalculator`) still
    // needs the helpers its calls lower to.
    let ast = crate::gmp_prelude::inject_if_used(
        ast,
        with_crates.contains("gmp") || autoloaded("gmp"),
        &mut prelude_inventory,
    );
    let ast = crate::random_prelude::inject_if_used(ast, with_crates.contains("random"), &mut prelude_inventory);
    // The sodium sealed-box surface is injected here too, so name resolution sees the
    // functions and folds a differently cased call (`\SODIUM_CRYPTO_BOX_SEAL()`) onto them.
    // The late gate after autoload expansion covers code only an autoloaded class reaches.
    let ast = crate::sodium_prelude::inject_if_used(ast, &mut prelude_inventory);
    // `password_hash()`/`password_verify()` and friends, bcrypt over `elephc_crypto`; the late
    // gate below covers a call only an autoloaded class makes.
    let ast = crate::password_prelude::inject_if_used(ast, &mut prelude_inventory);
    // php 8.5's `array_first()`/`array_last()`; the late gate below covers autoloaded callers.
    let ast = crate::array_edge_prelude::inject_if_used(ast, &mut prelude_inventory);
    let ast = crate::array_gradual_prelude::inject_if_used(ast, &mut prelude_inventory);
    let ast = crate::array_ucompare_prelude::inject_if_used(ast, &mut prelude_inventory);
    // php 8.4's `Dom\HTMLDocument` and its node classes; the late gate below covers a program
    // that only an autoloaded class (symfony/html-sanitizer's `NativeParser`) names it from.
    let ast = crate::dom_html_prelude::inject_if_used(ast, php_version, false, &mut prelude_inventory);
    timings.record_since("hash-prelude", phase_started);

    // Inject the testing runtime (`Elephc\Testing\TestCase`, `Runner`, the PHPUnit alias)
    // only when the program references it, so ordinary binaries never carry the runner.
    // Declaration-only and order-independent like the hash prelude; `elephc test` forces it.
    crate::progress::phase("testing-prelude");
    let phase_started = Instant::now();
    let ast = crate::testing_prelude::inject_if_used(ast, entry.is_test_suite(), &mut prelude_inventory);
    timings.record_since("testing-prelude", phase_started);

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
            with_crates.contains("curl") || autoloaded("curl"),
            &mut prelude_inventory,
        )
    } else {
        crate::curl_prelude::inject_if_used_for_version(
            ast,
            with_crates.contains("curl") || autoloaded("curl"),
            php_version,
            &mut prelude_inventory,
        )
    };
    timings.record_since("curl-prelude", phase_started);

    // Inject the `ext/xml` / `ext/xmlwriter` prelude (the `XMLParser` and `XMLWriter`
    // classes and the `xml_*` / `xmlwriter_*` wrappers over the `elephc_xml` extern block)
    // only when the program references that surface, so XML-free binaries never declare
    // the classes and never link `-lelephc_xml`. Order-independent like the hash and curl
    // preludes; `--with-xml` forces the injection for opaque dynamic use.
    crate::progress::phase("xml-prelude");
    let phase_started = Instant::now();
    let ast = crate::xml_prelude::inject_if_used(
        ast,
        with_crates.contains("xml") || autoloaded("xml"),
        &mut prelude_inventory,
    );
    timings.record_since("xml-prelude", phase_started);
    let phase_started = Instant::now();
    let fileinfo_force = with_crates.contains("fileinfo") || autoloaded("fileinfo");
    let ast = if php_version == crate::php_version::PhpVersion::default() {
        crate::fileinfo_prelude::inject_if_used(ast, fileinfo_force, &mut prelude_inventory)
    } else {
        crate::fileinfo_prelude::inject_if_used_for_version(
            ast, fileinfo_force, php_version, &mut prelude_inventory,
        )
    };
    timings.record_since("fileinfo-prelude", phase_started);
    // `Dom\HTMLDocument` and its node classes are injected with the other early preludes above
    // (`dom_html_prelude::inject_if_used`); main's separate Termwind-subset phase is superseded
    // by the branch's `dom_prelude`.

    // The resolved `opcache.preload` path, so the web prelude can find that file's guard by
    // label and lift it out of the per-request handler. Recomputed rather than threaded down
    // from `inject_preload_require`: the verdict is a pure function of the directives and the
    // entry, and passing it through six intervening phases to save one call would be worse.
    let preload_startup_path = if web {
        match opcache_prelude::preload_verdict(php_version, web, &ini_overrides, &[]) {
            opcache_prelude::PreloadVerdict::Preloading { resolved, .. }
                if opcache_prelude::canonical_entry_path(filename)
                    .is_none_or(|entry| entry != resolved) =>
            {
                Some(std::path::PathBuf::from(resolved))
            }
            _ => None,
        }
    } else {
        None
    };

    crate::progress::phase("web-prelude");
    let phase_started = Instant::now();
    let ast = if web_application && autoloaded("web_environment") {
        web_prelude::application::inject_with_autoload_usage(
            ast, php_version, &ini_overrides, Path::new(filename), &mut prelude_inventory,
            preload_startup_path.as_deref(), true,
        )
    } else if web_application {
        web_prelude::application::inject(
            ast, php_version, &ini_overrides, Path::new(filename), &mut prelude_inventory,
            preload_startup_path.as_deref(),
        )
    } else if autoloaded("web_environment") {
        web_prelude::inject_if_web_with_autoload_usage(
            ast, web, php_version, &ini_overrides, Path::new(filename), &mut prelude_inventory,
            preload_startup_path.as_deref(), true,
        )
    } else {
        web_prelude::inject_if_web(
            ast,
            web,
            php_version,
            &ini_overrides,
            Path::new(filename),
            &mut prelude_inventory,
            preload_startup_path.as_deref(),
        )
    };
    timings.record_since("web-prelude", phase_started);
    decl_trace("after web-prelude", &ast);

    // The PHP CLI SAPI ships `ext/session` independently of the built-in web server.
    // Share its PHP-level implementation and state bridge without importing HTTP request
    // setup or the `elephc_web_run` entry point into a one-shot CLI executable.
    crate::progress::phase("session-prelude");
    let phase_started = Instant::now();
    let session_reached_unseen = !web && crate::session_prelude::unseen_source_may_call_sessions(&ast, &prelude_inventory);
    let ast = crate::session_prelude::inject_cli_session_surface(
        ast,
        web,
        php_version,
        &ini_overrides,
        Path::new(filename),
        &mut prelude_inventory,
    );
    if !web {
        // Session APIs are callable from source loaded dynamically as well as statically visible
        // code; preserve their implementation as PHP's built-in extension surface does. A
        // program with no such source keeps only what it names, through ordinary reachability.
        if session_reached_unseen {
            forced_groups.insert(crate::session_prelude::SESSION_PRELUDE_GROUP.to_string());
        }
        if !linked_php_surfaces
            .iter()
            .any(|surface| surface.eq_ignore_ascii_case("session"))
        {
            linked_php_surfaces.push("session".to_string());
        }
    }
    timings.record_since("session-prelude", phase_started);

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

    // Declare the real PHP extensions the project hosts (`[extension]` in
    // elephc.toml): each registered function becomes a PHP function calling it
    // through the Zend engine archive. Injected whenever the project declares
    // any — reachability prunes the wrappers a program never calls, and an
    // archive nothing references contributes nothing to the link.
    crate::progress::phase("extension-prelude");
    let phase_started = Instant::now();
    let hosted_php_ext = match crate::php_ext::install::resolve_hosted(Path::new(filename), target, web_application) {
        Ok(hosted) => hosted,
        Err(error) => {
            crate::progress::clear();
            eprintln!("{error}");
            process::exit(1);
        }
    };
    let ast = match &hosted_php_ext {
        Some(hosted) => {
            let mut combined = crate::php_ext::prelude::declarations(hosted);
            prelude_inventory.record_program(crate::php_ext::prelude::PRELUDE_GROUP, &combined);
            combined.extend(ast);
            for extension in &hosted.extensions {
                linked_php_surfaces.push(extension.surface.module.clone());
            }
            // The wrappers name their live instance by `WeakReference`.
            crate::core_prelude::ensure_weak_reference(combined, &mut prelude_inventory)
        }
        None => ast,
    };
    timings.record_since("extension-prelude", phase_started);

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
    // The entry file's own interfaces are the last declarations with no activation event: neither
    // the resolver's include stripping nor the autoload pass has seen them.
    let ast =
        autoload::activate_entry_interface_declarations(ast, &entry_source_unit.canonical_path);
    timings.record_since("name-resolve", phase_started);
    decl_trace("after name-resolve", &ast);

    // php preloads at STARTUP, before any request, and what survives is the symbol table it built.
    // A compiled binary has no startup, so the AOT equivalent is to take the preloaded files'
    // DECLARATIONS into the closed world and to run none of the preload graph's own statements.
    // They go in ahead of the entry's, and any name the entry declares for itself is dropped from
    // them first: both halves reach `vendor/autoload.php`, and php declares each class once.
    // Placed after NAME RESOLUTION, because that is what makes the two halves comparable: before
    // it the entry still spells a declaration `ClassLoader` while the preload half, resolved on
    // its own pass, already spells it `Composer\\Autoload\\ClassLoader`.
    crate::progress::phase("opcache-preload");
    let phase_started = Instant::now();
    let mut preload_compiled_whole: Vec<std::path::PathBuf> = Vec::new();
    let mut preload_include_units: Vec<crate::ir::IncludeUnit> = Vec::new();
    let mut preload_class_files: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let preload_cache_enabled = crate::opcache::state::opcache_cache_enabled_with_overrides(
        php_version.version_id(),
        web,
        &ini_overrides,
    );
    let ast = match preloaded_declarations(
        &ini_overrides,
        preload_cache_enabled,
        parent,
        &defines,
        &autoload_registry,
    ) {
        Ok(Some(preloaded)) => {
            preload_compiled_whole = preloaded.compiled_whole;
            preload_include_units = preloaded.include_units;
            preload_class_files = preloaded.class_files;
            let declared = crate::opcache_preload_sources::declared_names(&ast);
            let mut combined = crate::opcache_preload_sources::without_redeclarations(
                preloaded.declarations,
                &declared,
            );
            combined.extend(ast);
            combined
        }
        Ok(None) => ast,
        Err(e) => {
            crate::progress::clear();
            errors::report(&e);
            process::exit(1);
        }
    };
    timings.record_since("opcache-preload", phase_started);
    decl_trace("after opcache-preload", &ast);

    // A closed-world binary has no interpreter to run a file it did not compile, and an include
    // path computed at run time can name any of the project's data files
    // (`opcache_preload_sources::project_data_units`). Built BEFORE the autoload pass, so the
    // classes their bodies name (a bundle's `Resources/config/*.php` names every service class
    // as `X::class`) are walked like any other reference.
    let ast = if crate::closed_world::is_enabled() {
        let mut ast = ast;
        if let Some(root) = crate::autoload::project_root(parent) {
            match crate::opcache_preload_sources::project_data_units(
                &root,
                &defines,
                &preload_include_units,
            ) {
                Ok((functions, units)) => {
                    ast.splice(0..0, functions);
                    preload_include_units.extend(units);
                }
                Err(e) => {
                    crate::progress::clear();
                    errors::report(&e);
                    process::exit(1);
                }
            }
        }
        ast
    } else {
        ast
    };

    crate::progress::phase("autoload-run");
    let phase_started = Instant::now();
    // `run_collecting_included` also hands back the canonical path of every file the autoload
    // pass loaded — eager files, PSR-4 / SPL-rule class files, and their own
    // include targets: group 3 of the OPcache script manifest, and the last one to become
    // knowable.
    let (ast, opcache_autoloaded_files, declaration_source_files) =
        match autoload::run_collecting_included_with_defines_and_sources(
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
    autoload::forget_loaded_files();
    timings.record_since("autoload-run", phase_started);
    decl_trace("after autoload-run", &ast);
    // Uses of a class whose file-scope code was hoisted into an initializer call it first.
    let mut ast = ast;
    for (path, missing) in &declaration_source_files.unbindable_class_files {
        add_missing_parent_unit(&mut ast, &mut preload_include_units, path, missing);
    }
    autoload::lazy_init::call_lazy_initializers(&mut ast);
    // A closed world has no interpreter scope for `extract(..., EXTR_REFS)` to publish into.
    if crate::closed_world::is_enabled() {
        crate::closed_world_extract_refs::rewrite(&mut ast);
    }
    // Which classes the program names ONLY through existence probes, taken from the program as
    // the autoload pass left it, before the check rounds rewrite it. By the time a class file is
    // refused, the `X::class` of a service config (`->set('test.client', KernelBrowser::class)`)
    // is no longer in the program being walked, so the class looked unnamed rather than
    // probe-only and the file declaring it refused the closed-world build instead of being left
    // to php's include Error.
    let probe_only_names: HashSet<String> = if crate::closed_world::is_enabled() {
        crate::autoload::probe_only_class_names(&ast)
            .iter()
            .map(|name| crate::names::php_symbol_key(name.trim_start_matches('\\')))
            .collect()
    } else {
        HashSet::new()
    };
    // Which eager Composer `files` entry declared each function, for handing one back to the
    // runtime when the backend cannot lower a body it declares (`BackendDeferral`).
    let eager_function_files = declaration_source_files.eager_function_files.clone();

    // Inject compatibility functions only after autoload expansion has exposed the complete
    // closed-world program. Their EIR routing is type-directed, so the declarations must be
    // present before checking even when the triggering call came from an autoloaded class.
    crate::progress::phase("compat-preludes");
    let phase_started = Instant::now();
    // PHP's error/exception-handling surface exists in EVERY SAPI, and without this a build
    // off `--web` has none of it: `error_reporting()` is an undefined function, which alone
    // stops a console entry point. The declarations are the ones the `--web` prelude injects,
    // so this is a no-op under `--web`; off it, the injection is pay-for-use on what the
    // program names.
    //
    // It belongs HERE and not in the web-prelude phase, for the reason this phase exists: the
    // program that names the surface is usually not the entry file. Symfony's console calls
    // `error_reporting()` from `Runtime\Internal\BasicErrorHandler`, a class the autoload pass
    // splices in at `autoload-run` — which is AFTER the web-prelude phase, so the gate saw a
    // 21-line entry that mentions nothing and injected nothing, and the binary died at run
    // time on the undefined function. A `require`d file was visible and an autoloaded class
    // was not, which is exactly the distinction this phase's existing comment draws.
    let ast = crate::error_handling_prelude::inject_if_used(
        ast,
        web,
        Path::new(filename),
        &mut prelude_inventory,
    );
    // `__elephc_shutdown_run` is likewise reached from EMITTED CODE and never from PHP off
    // `--web`: `lower_exit` and `emit_main_epilogue` call it by symbol. Forcing its group is
    // what keeps `register_shutdown_function()`'s queue drained at `exit()` and at the normal
    // end of the script. The group only exists when the program spelled the registration
    // function, so this stays pay-for-use.
    if prelude_inventory
        .groups
        .contains_key(crate::error_handling_prelude::SHUTDOWN_RUN_GROUP)
    {
        forced_groups.insert(crate::error_handling_prelude::SHUTDOWN_RUN_GROUP.to_string());
    }
    // PHP's `ini_get()` / `ini_set()` / `ini_get_all()` exist in every SAPI. Off `--web` elephc
    // has had them all along (the `opcache.*`-backed wrappers in `opcache_prelude`), but their
    // only gate ran in the `opcache-prelude` phase — before autoload expansion — so a program
    // that names them from an AUTOLOADED class got nothing and died at run time with `Call to
    // undefined function ini_set()`. Measured on `examples/symfony-app/bin/console`. This second
    // gate reads the COMPLETE program and injects only what the first one could not see; it is
    // a no-op under `--web` and a no-op whenever the early site already fired.
    let ast = opcache_prelude::inject_cli_ini_if_used(
        ast,
        php_version,
        &ini_overrides,
        web,
        &mut prelude_inventory,
    );
    let ast = crate::assert_prelude::inject_if_used(ast);
    // PDO and mysqli, which php loads as extensions: Doctrine DBAL is autoloaded, so the early
    // gates saw only the entry program, and symfony/demo's `PDO::connect()` became a static call
    // to a class the closed world does not define. Both preludes carry `namespace`/`use`
    // statements, and name resolution already ran: they are built alone and resolved first.
    let late_pdo = !pdo_used && pdo_prelude::program_uses_pdo(&ast);
    let late_mysqli = !mysqli_used && mysqli_prelude::program_uses_mysqli(&ast);
    let ast = if late_pdo || late_mysqli {
        let mut prelude = Vec::new();
        if late_pdo {
            linked_php_surfaces.push("PDO".to_string());
            prelude = pdo_prelude::inject_if_used_for_version(
                prelude, true, php_version, &mut prelude_inventory,
            );
        }
        if late_mysqli {
            linked_php_surfaces.push("mysqli".to_string());
            prelude =
                mysqli_prelude::inject_if_used(prelude, true, php_version, &mut prelude_inventory);
        }
        let prelude = match name_resolver::resolve(prelude) {
            Ok(resolved) => resolved,
            Err(e) => {
                crate::progress::clear();
                errors::report(&e);
                process::exit(1);
            }
        };
        // An early surface already declared the shared bridge externs.
        let declared_externs: HashSet<String> = ast
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                crate::parser::ast::StmtKind::ExternFunctionDecl { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        let mut combined: crate::parser::ast::Program = prelude
            .into_iter()
            .filter(|stmt| match &stmt.kind {
                crate::parser::ast::StmtKind::ExternFunctionDecl { name, .. } => {
                    !declared_externs.contains(name)
                }
                _ => true,
            })
            .collect();
        combined.extend(ast);
        combined
    } else {
        ast
    };
    let ast = crate::throwable_string_prelude::inject(ast, &mut prelude_inventory);
    // `ErrorException` is what a user error handler throws for a warning; Symfony's
    // `ErrorHandler` does it for every warning it is configured to throw on, from a class the
    // autoload pass splices in, so the gate has to read the complete program like the rest here.
    // Before the preludes that follow: the wrappers call builtins that need their helpers.
    let ast = match crate::builtin_callable_prelude::inject_if_used(ast, &mut prelude_inventory) {
        Ok(injected) => injected,
        Err(e) => {
            crate::progress::clear();
            errors::report(&e);
            process::exit(1);
        }
    };
    if prelude_inventory.groups.contains_key(crate::builtin_callable_prelude::GROUP) {
        forced_groups.insert(crate::builtin_callable_prelude::GROUP.to_string());
    }
    let ast = crate::error_exception_prelude::inject_if_used(ast, &mut prelude_inventory);
    // `ext/sodium` sealed boxes: Symfony's `SodiumVault` is autoloaded, so this gate reads the
    // complete program too, and it keeps `SodiumException` for code the interpreter runs.
    let ast = crate::sodium_prelude::inject_if_used(ast, &mut prelude_inventory);
    // Symfony's `NativePasswordHasher` is autoloaded, so its `password_verify()` call is only
    // visible here.
    let ast = crate::password_prelude::inject_if_used(ast, &mut prelude_inventory);
    // An autoloaded class that streams a hash (`hash_init()` / `HashContext`) is only visible
    // here; the early hash gate saw the entry program alone.
    let ast = crate::hash_prelude::inject_late_if_used(ast, &mut prelude_inventory);
    let ast = crate::array_edge_prelude::inject_if_used(ast, &mut prelude_inventory);
    let ast = crate::array_gradual_prelude::inject_if_used(ast, &mut prelude_inventory);
    let ast = crate::array_ucompare_prelude::inject_if_used(ast, &mut prelude_inventory);
    // The classic DOM classes: an autoloaded vendor class (Symfony's `XmlDescriptor`, Termwind's
    // `HtmlRenderer`) is usually the first to name them.
    let ast = crate::dom_prelude::inject_if_used(ast, &mut prelude_inventory);
    if prelude_inventory.groups.contains_key(crate::dom_prelude::GROUP) {
        // eval()-interpreted vendor code reaches these classes through the native-class bridge.
        forced_groups.insert(crate::dom_prelude::GROUP.to_string());
    }
    let ast = crate::dom_html_prelude::inject_if_used(ast, php_version, false, &mut prelude_inventory);
    if prelude_inventory.groups.contains_key(crate::dom_html_prelude::GROUP) {
        // eval()-interpreted vendor code reaches these classes through the native-class bridge.
        forced_groups.insert(crate::dom_html_prelude::GROUP.to_string());
    }
    // `Attribute`'s TARGET_* constants are named by every attribute class declaration, including
    // ones only the interpreter reads at run time.
    let ast = crate::attribute_prelude::inject_if_used(ast, &mut prelude_inventory);
    let ast = crate::array_merge_prelude::inject_if_used(ast, &mut prelude_inventory);
    if prelude_inventory.groups.contains_key("array_merge") {
        forced_groups.insert("array_merge".to_string());
    }
    let ast = crate::filter_var_prelude::inject_if_used(ast, &mut prelude_inventory);
    if prelude_inventory
        .groups
        .contains_key(crate::filter_var_prelude::FILTER_VAR_GROUP)
    {
        forced_groups.insert(crate::filter_var_prelude::FILTER_VAR_GROUP.to_string());
    }
    let ast = crate::backend_gap_prelude::inject_if_used(ast, &mut prelude_inventory);
    // The early injection only saw the entry program: a `var_export()` that first appears in an
    // autoloaded file (Nette's `Helpers::formatValue`) found no declaration, fell through to the
    // language-construct path the backend refuses, and cost the demo a whole recompile round.
    // Idempotent: a program that already carries the helpers is returned unchanged.
    let ast = var_export_prelude::inject_if_used(ast, &mut prelude_inventory);
    // The gradual array helpers are called by synthesized lowering, which reachability cannot
    // see; the group holds only the helpers the program's call shapes need.
    if prelude_inventory
        .groups
        .contains_key(crate::array_gradual_prelude::GROUP)
    {
        forced_groups.insert(crate::array_gradual_prelude::GROUP.to_string());
    }
    if prelude_inventory
        .groups
        .contains_key(crate::backend_gap_prelude::BACKEND_GAP_GROUP)
    {
        forced_groups.insert(crate::backend_gap_prelude::BACKEND_GAP_GROUP.to_string());
    }
    timings.record_since("compat-preludes", phase_started);
    decl_trace("after compat-preludes", &ast);

    // `elephc test`: discover the test classes now that every class they extend, including
    // the ones autoloading just brought in, is in the AST with resolved names, and append
    // the synthesized driver. An empty run is a compile error here, not a binary that
    // reports it.
    let ast = if let EntryKind::TestSuite { options, .. } = entry {
        crate::progress::phase("test-discovery");
        let phase_started = Instant::now();
        let ast = match crate::testing_prelude::discover::append_driver(ast, options) {
            Ok(ast) => ast,
            Err(e) => {
                crate::progress::clear();
                errors::report(&e);
                process::exit(1);
            }
        };
        timings.record_since("test-discovery", phase_started);
        ast
    } else {
        ast
    };

    // Inject the object-cast prelude (two pure elephc-PHP functions) only when the program
    // spells a `(object)` cast, so other binaries carry nothing.
    //
    // Runs AFTER `autoload::run`, unlike every prelude above, because the cast is detected
    // syntactically and an autoloaded class file is not part of the AST until here: a program
    // whose only `(object)` cast lives in a PSR-4 class saw no injection, and the call
    // `ir_lower` synthesizes for the cast then failed codegen with
    // `unsupported EIR backend feature: language construct __elephc_cast_object`. The
    // declarations are name-resolved on the way in, exactly as `autoload::run` resolves the
    // files it splices, because the position is past the pipeline's own name-resolution pass.
    // The testing runtime, when only an autoloaded file references it: the pass before name
    // resolution saw the entry and its includes, not the classes autoloading just spliced in.
    crate::progress::phase("testing-autoload-prelude");
    let phase_started = Instant::now();
    let ast = match crate::testing_prelude::inject_after_autoload(ast, &mut prelude_inventory) {
        Ok(injected) => injected,
        Err(e) => {
            crate::progress::clear();
            errors::report(&e);
            process::exit(1);
        }
    };
    timings.record_since("testing-autoload-prelude", phase_started);

    crate::progress::phase("object-cast-prelude");
    let phase_started = Instant::now();
    let ast = match crate::object_cast_prelude::inject_if_used(ast, &mut prelude_inventory) {
        Ok(injected) => injected,
        Err(e) => {
            crate::progress::clear();
            errors::report(&e);
            process::exit(1);
        }
    };
    timings.record_since("object-cast-prelude", phase_started);

    // The timezone-introspection prelude again, for a use that only an autoloaded class file
    // makes (see `tz_prelude::inject_late_if_used`).
    let phase_started = Instant::now();
    let ast = match tz_prelude::inject_late_if_used(ast, &mut prelude_inventory) {
        Ok(injected) => injected,
        Err(e) => {
            crate::progress::clear();
            errors::report(&e);
            process::exit(1);
        }
    };
    timings.record_since("tz-prelude-late", phase_started);

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
    decl_trace("after func-args", &ast);
    // A static default naming a class only the run time knows is evaluated at first use.
    let ast = crate::lazy_static_defaults::apply(ast);

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
    // The manifest's scripts hold hash slots, so the runtime tier gets what they leave.
    codegen::set_opcache_manifest_len(opcache_manifest.len());
    // Re-decide `opcache.preload` against the COMPLETE manifest, which is the set
    // `preload_statistics` reports. Only the `in_manifest` arm can differ from the verdict taken
    // above (the directive, the SAPI gate and the path resolution are all manifest-independent).
    // There is no out-of-manifest diagnostic to emit any more: `inject_preload_require` has
    // already made the preload file part of this binary, so it is always a manifest member.
    let opcache_preload =
        opcache_prelude::preload_verdict(php_version, web, &ini_overrides, &opcache_manifest);
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
    decl_trace("after manifest-bake", &ast);

    let ast = if web_application {
        web_prelude::application::wrap_bootstrap(ast)
    } else { ast };

    crate::progress::phase("opt-fold");
    let phase_started = Instant::now();
    let ast = optimize::fold_constants_for_target(ast, target);
    timings.record_since("opt-fold", phase_started);
    decl_trace("after opt-fold", &ast);

    // php binds an unconditional top-level `function f() {}` when the file is COMPILED and one
    // nested in a conditional statement only when that statement EXECUTES. Every polyfill in PHP
    // relies on the second rule. Routing those declarations into the existing function-variant
    // mechanism is what keeps `function_exists()` and the call site telling a program the same
    // thing. Placed after folding, so a guard the target already decided is never given a group,
    // and before the checker, which is the first stage that reads the declaration set.
    // A `static` a closure captures by reference becomes a hidden global bound by reference,
    // which every backend path supports (see `static_ref_capture`).
    let ast = crate::static_ref_capture::rewrite_captured_statics(ast);

    crate::progress::phase("conditional-functions");
    let phase_started = Instant::now();
    let ast = crate::conditional_functions::hoist_autoload_callback_function_marks(ast);
    let ast = crate::conditional_functions::bind_conditional_declarations(ast, target);
    timings.record_since("conditional-functions", phase_started);
    decl_trace("after conditional-functions", &ast);

    // A class-like declared under a condition is missing from the class table unless it is lifted
    // to file scope; a name declared only once has one possible body, so lifting it is safe.
    let ast = crate::conditional_classes::hoist_unique_conditional_class_likes(ast);
    decl_trace("after conditional-classes", &ast);

    crate::progress::phase("typecheck");
    let phase_started = Instant::now();
    // The autoload registry has already CONSUMED (and removed from the AST) every
    // `spl_autoload_register` call it could collect, so the checker cannot rediscover them by
    // scanning. It needs to know, because a registered loader means an unknown class name is a
    // name that arrives at run time rather than one that does not exist — see
    // `Checker::program_defers_unknown_classes`.
    let check_options = types::CheckOptions {
        strict_locals,
        lint,
        registers_autoloader: autoload_registry.rule_count() > 0,
    };
    // A class file the autoload pass loaded is one php would only have parsed when something
    // asked for the class. When the checker refuses one, that file is handed back to the runtime
    // autoloader (see `autoload::deferral`) and the rest of the program is checked again; the
    // classes that depended on it fall the same way on the next round.
    // A class `opcache.preload` supplied is one php's autoloader would equally have loaded, so the
    // same holds for the preloaded graph. The entry's own static includes and Composer's eager
    // `files` are never deferrable: nothing at run time would load them again.
    let mut ast = ast;
    // Literal-path includes left in function and method bodies run at run time; each one whose
    // file can be a unit is compiled as one (`opcache_preload_sources::literal_include_units`).
    match crate::opcache_preload_sources::literal_include_units(
        &ast,
        &defines,
        &preload_include_units,
    ) {
        Ok((functions, units)) => {
            // At the FRONT: the optimizer drops what follows a top-level `return`, and an entry
            // such as `bin/console` ends with one.
            ast.splice(0..0, functions);
            preload_include_units.extend(units);
        }
        Err(e) => {
            crate::progress::clear();
            errors::report(&e);
            process::exit(1);
        }
    }
    // A computed-path include of a template directory, which a unit cannot answer because the
    // template reads its caller's scope, runs the template as a closure bound to that scope
    // instead (`scoped_include_units`). Only a closed world needs it: elsewhere the interpreter
    // includes the file.
    if crate::closed_world::is_enabled() {
        match crate::scoped_include_units::rewrite_scoped_include_sites(
            &mut ast,
            &defines,
            &preload_include_units,
        ) {
            Ok(functions) => {
                ast.splice(0..0, functions);
            }
            Err(e) => {
                crate::progress::clear();
                errors::report(&e);
                process::exit(1);
            }
        }
    }
    // Taken before any deferral removes a declaration: `runtime_faults` must know which compiled
    // classes a deferred class descended from, and nothing else keeps that once it is gone.
    let class_parent_edges = crate::autoload::deferral::class_parent_edges(&ast);
    let mut opcache_autoloaded_files = opcache_autoloaded_files;
    // The preloaded class files stay deferrable even though `inject_preload_require` makes the
    // preload script a static include of the entry, which files every class file it requires
    // among the entry's own includes: php's autoloader loads a preloaded class's file on demand
    // all the same. Without this, one unsupported construct in any preloaded class (Laravel's
    // whole framework is preloaded) failed the build instead of leaving that file to run time.
    let preloaded_class_paths: HashSet<PathBuf> = preload_class_files
        .values()
        .map(|path| Path::new(path).canonicalize().unwrap_or_else(|_| PathBuf::from(path)))
        .collect();
    // Kept for reflection: a preloaded class's `getFileName()` names its own file. Without it the
    // lookup fell back to the entry script, and symfony/demo's source panel showed `index.php`
    // with lines from `BlogController.php` (and `Undefined array key` warnings past its end).
    let preload_class_source_files = preload_class_files.clone();
    let mut deferral_class_files = preload_class_files;
    deferral_class_files.extend(
        declaration_source_files
            .class_likes
            .iter()
            .map(|(name, path)| (name.clone(), path.clone())),
    );
    let deferrable_class_files: HashSet<PathBuf> = {
        let canonical = |path: &Path| crate::source::canonical(&path).unwrap_or_else(|_| path.to_path_buf());
        let mut fixed: HashSet<PathBuf> = autoload_registry
            .always_included_files()
            .iter()
            .map(|path| canonical(path))
            .collect();
        fixed.extend(
            entry_included_sources
                .class_likes
                .values()
                .chain(entry_included_sources.functions.values())
                .map(|path| canonical(Path::new(path)))
                .filter(|path| !preloaded_class_paths.contains(path)),
        );
        fixed.insert(canonical(Path::new(filename)));
        opcache_autoloaded_files
            .iter()
            .map(|path| canonical(path))
            .chain(deferral_class_files.values().map(|path| canonical(Path::new(path))))
            .filter(|path| !fixed.contains(path))
            .collect()
    };
    let mut deferred_class_files: HashSet<PathBuf> = HashSet::new();
    // Closed world only: refused files whose classes the program names only through existence
    // probes (`X::class`, `class_exists()`). php loads such a file only when something uses one of
    // its classes, so leaving it out keeps php's behaviour everywhere else; a run-time use raises
    // php's include Error. They are reported as notes and do not refuse the build.
    let mut probe_only_deferred: HashSet<PathBuf> = HashSet::new();
    // Closed world only: files php itself refuses to load (`Declaration of X::m() must be
    // compatible with ...`, psysh's Hoa `FileLink::create()`) and the files whose classes extend
    // one of theirs. php fatals the moment one is loaded, so none of them is ever run; like the
    // probe-only files they are notes, not reasons to refuse the build.
    let mut php_load_fatal: HashSet<PathBuf> = HashSet::new();
    // Why each file was handed back, for the deferral memo this build leaves for the next one.
    let mut deferral_reasons: HashMap<PathBuf, String> = HashMap::new();
    // A re-run the backend asked for (`autoload::deferral::BackendDeferral`) starts with the files
    // an earlier round already handed back; they were reported when they were first handed back.
    for path in crate::autoload::deferral::inherited_deferred_files() {
        let path = crate::source::canonical(&path).unwrap_or(path);
        if deferrable_class_files.contains(&path) {
            deferral_reasons
                .entry(path.clone())
                .or_insert_with(|| "the backend could not lower it".to_string());
            deferred_class_files.insert(path);
        }
    }
    // The re-run is a new process: what the round that found them decided about them comes along.
    for path in crate::autoload::deferral::inherited_harmless_deferred_files() {
        probe_only_deferred.insert(crate::source::canonical(&path).unwrap_or(path));
    }
    // Files an earlier build of this same program handed back, while their bytes are unchanged:
    // deferring them now saves the type-check rounds and the backend re-run that found them.
    // A remembered file is classified like a newly refused one: closed world keeps a file whose
    // classes are only probed out of the reasons it refuses the build.
    let mut remembered_refusals: Vec<(PathBuf, String)> = Vec::new();
    for remembered in crate::autoload::deferral_memo::load(Path::new(filename)) {
        let path = remembered.path.canonicalize().unwrap_or(remembered.path);
        let missing = crate::opcache_preload_sources::parent_of_missing_parent_refusal(&remembered.reason);
        if let (true, Some(parent)) = (crate::closed_world::is_enabled() && deferrable_class_files.contains(&path), missing) {
            add_missing_parent_unit(&mut ast, &mut preload_include_units, &path, parent);
        }
        if !deferrable_class_files.contains(&path) || !deferred_class_files.insert(path.clone()) {
            continue;
        }
        if crate::closed_world::is_enabled() {
            if declares_only_probed_classes(&path, &deferral_class_files, &probe_only_names) {
                probe_only_deferred.insert(path.clone());
                errors::report_note(&format!(
                    "autoload: '{}' is not compiled ({}; remembered from an earlier build); the \
                     program names its classes only through existence probes, and a run-time use \
                     of one raises php's include Error",
                    path.display(),
                    remembered.reason
                ));
                deferral_reasons.insert(path, remembered.reason);
                continue;
            }
            if php_refuses_to_load(&remembered.reason) {
                php_load_fatal.insert(path.clone());
                probe_only_deferred.insert(path.clone());
                errors::report_note(&format!(
                    "autoload: '{}' is not compiled ({}; remembered from an earlier build); php \
                     refuses to load it, so it never runs",
                    path.display(),
                    remembered.reason
                ));
                deferral_reasons.insert(path, remembered.reason);
                continue;
            }
        }
        remembered_refusals.push((path, remembered.reason));
    }
    // What extends a class php cannot load cannot load either, at any depth: symfony's
    // `TypeTestCase` over `FormIntegrationTestCase` over an uninstalled PHPUnit. The first build
    // decides this as it finds each file, in dependency order; a memo comes in file order.
    if crate::closed_world::is_enabled() {
        loop {
            let (fatal, rest): (Vec<_>, Vec<_>) =
                remembered_refusals.into_iter().partition(|(path, _)| {
                    extends_a_php_load_fatal_class(
                        path,
                        &deferral_class_files,
                        &class_parent_edges,
                        &php_load_fatal,
                    )
                });
            remembered_refusals = rest;
            if fatal.is_empty() {
                break;
            }
            for (path, reason) in fatal {
                php_load_fatal.insert(path.clone());
                probe_only_deferred.insert(path.clone());
                if let Some(parent) =
                    first_parent_declared_by(&path, &deferral_class_files, &class_parent_edges)
                {
                    add_missing_parent_unit(&mut ast, &mut preload_include_units, &path, &parent);
                }
                errors::report_note(&format!(
                    "autoload: '{}' is not compiled; a class it declares extends one php refuses \
                     to load, so it never runs",
                    path.display()
                ));
                deferral_reasons.insert(path, reason);
            }
        }
    }
    for (path, reason) in remembered_refusals {
        errors::report_warning(&errors::CompileWarning::new(
            Span::dummy(),
            &format!(
                "autoload: '{}' is not compiled ({}; remembered from an earlier build, \
                 ELEPHC_DEFERRAL_MEMO=0 retries it); its classes are loaded at run time by the \
                 autoloader instead",
                path.display(),
                reason
            ),
        ));
        deferral_reasons.insert(path, reason);
    }
    let by_ref_reason = "it passes a property or array element to a by-reference constructor \
                         parameter, which a native call cannot keep linked";
    let by_ref_files = crate::autoload::deferral::propagate_by_ref_place_constructions(
        &ast,
        &deferral_class_files,
        &deferrable_class_files,
        &mut deferred_class_files,
    );
    for path in &by_ref_files {
        deferral_reasons.insert(path.clone(), by_ref_reason.to_string());
    }
    report_by_ref_place_deferrals(by_ref_files);
    if !deferred_class_files.is_empty() {
        crate::autoload::deferral::propagate_to_subclasses(
            &ast,
            &deferral_class_files,
            &deferrable_class_files,
            &mut deferred_class_files,
        );
        ast = crate::autoload::deferral::remove_deferred_declarations(
            ast,
            &deferral_class_files,
            &deferred_class_files,
        );
    }
    // Each `(span, message)` `runtime_faults` already rewrote: an error that comes back for one
    // of them is not helped by rewriting, and the file is deferred like any other.
    let mut demotion_attempts: HashSet<(Span, String)> = HashSet::new();
    let mut runtime_fault_helpers_injected = false;
    let mut check_result = loop {
        // A class whose parent nothing provides is refused by the checker's inheritance
        // flattening; finding those first spends a program walk on the deferral round instead of
        // a whole failed type check (a second full check of every Symfony build).
        // Used only when it hands back a file not yet deferred: anything else (a parent the
        // walk cannot see, a file that cannot be deferred) is left to the checker to decide.
        let precheck = crate::closed_world::is_enabled()
            .then(|| crate::autoload::deferral::unknown_parent_refusals(&ast))
            .flatten()
            .filter(|refusals| {
                crate::autoload::deferral::deferrable_class_files(
                    refusals,
                    &deferral_class_files,
                    &deferrable_class_files,
                )
                .keys()
                .any(|path| {
                    !deferred_class_files.contains(
                        &Path::new(path).canonicalize().unwrap_or_else(|_| PathBuf::from(path)),
                    )
                })
            });
        let e = match precheck {
            Some(e) => e,
            // Type checking runs to a generic-instantiation fixpoint, so every reachable template
            // has become an ordinary monomorphic declaration before anything downstream sees the
            // program. The program is moved in and comes back with the result; a refusal hands it
            // back too, so the deferral below can remove the refused file and check again.
            None => match crate::generics::monomorphize_recovering(
                std::mem::take(&mut ast),
                |program, bounds| {
                    types::check_with_target_and_options(program, target, check_options, bounds)
                },
            ) {
                Ok((checked, result)) => {
                    ast = checked;
                    break result;
                }
                Err((Some(refused), e)) => {
                    ast = refused;
                    e
                }
                // A refusal raised outside the first check round (generic expansion, a later
                // instantiation round) cannot be handed back to the autoloader: it is final.
                Err((None, e)) => {
                    // An adopted PHPDoc generic is not final, though: its file can still be
                    // compiled with its written types (`docblock::erasure`).
                    crate::docblock::erasure::rerun_erasing(
                        &crate::docblock::erasure::implicated_files(&e, &deferral_class_files),
                        &e,
                    );
                    crate::progress::clear();
                    if !crate::docblock::erasure::report_original_instead() {
                        errors::report(&e);
                    }
                    process::exit(1);
                }
            },
        };
        // Before a refused class file is handed back, each refused construct the checker could
        // name php's run-time fault for is compiled as that fault instead (`autoload::
        // runtime_faults`), and the program is checked again with the file still in it. Only
        // the errors nothing could be compiled for go on to defer their files, in the SAME
        // round: a full check of a Laravel program is 85 s, and a round that only demoted would
        // meet the very same refusals again on the next one.
        let e = {
            let autoload_lookup =
                |name: &str| autoload_registry.psr4().lookup(name).is_some();
            let world = crate::autoload::runtime_faults::World {
                class_files: &deferral_class_files,
                deferrable: &deferrable_class_files,
                deferred: &deferred_class_files,
                class_parents: &class_parent_edges,
                autoload_lookup: &autoload_lookup,
            };
            let demotions = crate::autoload::runtime_faults::demote(
                &mut ast,
                &e,
                &world,
                &mut demotion_attempts,
            );
            if demotions.is_empty() {
                e
            } else {
                for demotion in &demotions {
                    errors::report_note(&demotion.note());
                }
                if !runtime_fault_helpers_injected {
                    runtime_fault_helpers_injected = true;
                    let helpers = crate::autoload::runtime_faults::declarations();
                    prelude_inventory
                        .record_program(crate::autoload::runtime_faults::PRELUDE_GROUP, &helpers);
                    ast.splice(0..0, helpers);
                }
                let demoted: HashSet<(Span, &str)> = demotions
                    .iter()
                    .map(|demotion| (demotion.span, demotion.message.as_str()))
                    .collect();
                // An error left in a file this round just rewrote can be a cascade of the
                // rewritten construct: a refused `if` condition never installed the locals its
                // `||` assigns, so the read after it was "undefined" too (symfony/framework-
                // bundle's `DebugAutowiringCommand::getFileLink()`). Such a file is checked again
                // before it is deferred; an error that is real comes back on that round.
                let demoted_files: HashSet<PathBuf> = demotions
                    .iter()
                    .map(|demotion| {
                        Path::new(&demotion.path)
                            .canonicalize()
                            .unwrap_or_else(|_| PathBuf::from(&demotion.path))
                    })
                    .collect();
                let remaining: Vec<errors::CompileError> = e
                    .flatten()
                    .into_iter()
                    .filter(|error| !demoted.contains(&(error.span, error.message.as_str())))
                    .filter(|error| {
                        crate::autoload::deferral::error_class_file(error, &deferral_class_files)
                            .is_none_or(|path| !demoted_files.contains(&path))
                    })
                    .collect();
                if remaining.is_empty() {
                    continue;
                }
                errors::CompileError::from_many(remaining)
            }
        };
        // An adopted PHPDoc generic must not refuse php-valid code: before any file is handed to
        // the runtime, the files whose annotations the refusal implicates are compiled again
        // with their written types (`docblock::erasure`). Returns only when no re-run happened.
        crate::docblock::erasure::rerun_erasing(
            &crate::docblock::erasure::implicated_files(&e, &deferral_class_files),
            &e,
        );
        let mut newly_deferred = crate::autoload::deferral::deferrable_class_files(
            &e,
            &deferral_class_files,
            &deferrable_class_files,
        );
        // A file already handed back cannot be handed back again: an error still naming one is
        // not going away by another round, so it is reported.
        newly_deferred.retain(|path, _| {
            !deferred_class_files.contains(
                &Path::new(path)
                    .canonicalize()
                    .unwrap_or_else(|_| PathBuf::from(path)),
            )
        });
        if !newly_deferred.is_empty() {
            for (path, message) in &newly_deferred {
                let canonical = Path::new(path)
                    .canonicalize()
                    .unwrap_or_else(|_| PathBuf::from(path));
                let missing = crate::opcache_preload_sources::parent_of_missing_parent_refusal(message);
                if let (true, Some(parent)) = (crate::closed_world::is_enabled(), missing) {
                    add_missing_parent_unit(&mut ast, &mut preload_include_units, &canonical, parent);
                }
                if declares_only_probed_classes(&canonical, &deferral_class_files, &probe_only_names) {
                    probe_only_deferred.insert(canonical.clone());
                    errors::report_note(&format!(
                        "autoload: '{path}' is not compiled ({message}); the program names its \
                         classes only through existence probes, and a run-time use of one raises \
                         php's include Error"
                    ));
                } else if crate::closed_world::is_enabled()
                    && (php_refuses_to_load(message)
                        || extends_a_php_load_fatal_class(
                            &canonical,
                            &deferral_class_files,
                            &class_parent_edges,
                            &php_load_fatal,
                        ))
                {
                    php_load_fatal.insert(canonical.clone());
                    probe_only_deferred.insert(canonical.clone());
                    errors::report_note(&format!(
                        "autoload: '{path}' is not compiled ({message}); php refuses to load it \
                         (an incompatible declaration in it or in a parent), so it never runs"
                    ));
                } else {
                    errors::report_warning(&errors::CompileWarning::new(
                        Span::dummy(),
                        &format!(
                            "autoload: '{path}' is not compiled ({message}); its classes are loaded \
                             at run time by the autoloader instead"
                        ),
                    ));
                }
                deferral_reasons.insert(canonical.clone(), message.clone());
                deferred_class_files.insert(canonical);
            }
            for path in crate::autoload::deferral::propagate_to_subclasses(
                &ast,
                &deferral_class_files,
                &deferrable_class_files,
                &mut deferred_class_files,
            ) {
                deferral_reasons.insert(
                    path.clone(),
                    "a class it declares extends, implements or uses one that is not".to_string(),
                );
                if crate::closed_world::is_enabled()
                    && extends_a_php_load_fatal_class(
                        &path,
                        &deferral_class_files,
                        &class_parent_edges,
                        &php_load_fatal,
                    )
                {
                    php_load_fatal.insert(path.clone());
                    probe_only_deferred.insert(path.clone());
                    if let Some(parent) =
                        first_parent_declared_by(&path, &deferral_class_files, &class_parent_edges)
                    {
                        add_missing_parent_unit(&mut ast, &mut preload_include_units, &path, &parent);
                    }
                    errors::report_note(&format!(
                        "autoload: '{}' is not compiled; a class it declares extends one php \
                         refuses to load, so it never runs",
                        path.display()
                    ));
                    continue;
                }
                errors::report_warning(&errors::CompileWarning::new(
                    Span::dummy(),
                    &format!(
                        "autoload: '{}' is not compiled (a class it declares extends, implements \
                         or uses one that is not); its classes are loaded at run time by the \
                         autoloader instead",
                        path.display()
                    ),
                ));
            }
            let by_ref_files = crate::autoload::deferral::propagate_by_ref_place_constructions(
                &ast,
                &deferral_class_files,
                &deferrable_class_files,
                &mut deferred_class_files,
            );
            for path in &by_ref_files {
                deferral_reasons.insert(path.clone(), by_ref_reason.to_string());
            }
            report_by_ref_place_deferrals(by_ref_files);
            ast = crate::autoload::deferral::remove_deferred_declarations(
                ast,
                &deferral_class_files,
                &deferred_class_files,
            );
            continue;
        }
        crate::progress::clear();
        // Name the file each error was written in. The autoload pass spliced every discovered
        // file into one program, so the `line:col` the checker reports lives in a coordinate
        // space shared by hundreds of files and identifies nothing on its own. The checker
        // recorded the enclosing declaration; `declaration_source_files` is the map from that
        // name to its path — the same one `Reflection*::getFileName()` reads.
        let mut declaration_paths = declaration_source_file_paths(
            &declaration_source_files,
            &entry_included_sources,
        );
        // Preloaded classes name their files too; the autoloaded and entry maps win.
        for (name, path) in &deferral_class_files {
            declaration_paths.entry(name.clone()).or_insert_with(|| path.clone());
        }
        if !crate::docblock::erasure::report_original_instead() {
            errors::report(&e.resolve_declaration_files(&declaration_paths));
        }
        process::exit(1);
    };
    {
        let mut remembered: Vec<crate::autoload::deferral_memo::MemoEntry> = deferred_class_files
            .iter()
            .map(|path| crate::autoload::deferral_memo::MemoEntry {
                path: path.clone(),
                reason: deferral_reasons.get(path).cloned().unwrap_or_default(),
            })
            .collect();
        remembered.sort_by(|left, right| left.path.cmp(&right.path));
        crate::autoload::deferral_memo::store(Path::new(filename), &remembered);
        crate::docblock::erasure::store_memo();
    }
    // A deferred file was never compiled, so the runtime must not treat it as already included.
    opcache_autoloaded_files.retain(|path| {
        !deferred_class_files.contains(&crate::source::canonical(&path).unwrap_or_else(|_| path.clone()))
    });
    preload_compiled_whole.retain(|path| {
        !deferred_class_files.contains(&crate::source::canonical(&path).unwrap_or_else(|_| path.clone()))
    });
    timings.record_since("typecheck", phase_started);
    decl_trace("after typecheck", &ast);
    if let Ok(target) = std::env::var("ELEPHC_DECL_TRACE") {
        let key = crate::names::php_symbol_key(target.trim_start_matches('\\'));
        let names: Vec<&String> = check_result
            .functions
            .keys()
            .filter(|name| crate::names::php_symbol_key(name.trim_start_matches('\\')) == key)
            .collect();
        decl_trace_note("after typecheck", &format!("checker_functions={names:?}"));
    }
    if emit == Emit::Phpext && hosted_php_ext.is_some() {
        crate::progress::clear();
        eprintln!("{PHPEXT_CORE_RUNTIME_ONLY}");
        process::exit(1);
    }
    for warning in &check_result.warnings {
        errors::report_warning(warning);
    }
    // The instantiation report is printed here, before declaration pruning and codegen: every
    // template is already gone and every remaining `<...>` name is a concrete instantiation, so
    // this is the complete monomorphization set. It is diagnostic only and changes nothing.
    if report_generics {
        eprintln!("{}", crate::generics::instantiation_report(&ast));
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
    let exported_classes = if emit == Emit::Phpext {
        crate::phpext::constants::materialize_native_constants(
            &mut ast, &mut check_result.classes, &mut check_result.interfaces, target,
        );
        match crate::phpext::classes::collect(&ast, &check_result.classes, target) {
            Ok(classes) => classes,
            Err(error) => {
                crate::progress::clear();
                errors::report(&error.with_file(filename.to_string()));
                process::exit(1);
            }
        }
    } else { Vec::new() };
    let mut exported_functions = match if emit == Emit::Phpext {
        exports::collect_for_php(&ast, &check_result.functions)
    } else { exports::collect(&ast, &check_result.functions) } {
        Ok(exports) => exports,
        Err(e) => {
            crate::progress::clear();
            errors::report(&e.with_file(filename.to_string()));
            process::exit(1);
        }
    };
    if emit == Emit::Phpext {
        let names = exported_classes.iter().map(|class| class.name.clone()).collect();
        for export in exported_functions.values() {
            if let Err(error) = crate::phpext::classes::validate_signature_classes(
                &export.name, &export.source_sig, &names, export.span) {
                crate::progress::clear();
                errors::report(&error.with_file(filename.to_string()));
                process::exit(1);
            }
        }
        for method in exported_classes.iter().flat_map(|class| &class.methods) {
            exported_functions.insert(method.export.name.clone(), method.export.clone());
        }
        for helper in exported_classes.iter().flat_map(|class| &class.helpers) {
            exported_functions.insert(helper.export.name.clone(), helper.export.clone());
        }
    }
    timings.record_since("exports-scan", phase_started);
    let exported_constants = if emit == Emit::Phpext {
        match crate::phpext::constants::collect(&ast, &export_constants, target) {
            Ok(constants) => constants,
            Err(error) => {
                crate::progress::clear();
                errors::report(&error.with_file(filename.to_string()));
                process::exit(1);
            }
        }
    } else { Vec::new() };
    if emit == Emit::Phpext && exported_functions.is_empty() && exported_constants.is_empty() && exported_classes.is_empty() {
        crate::progress::clear();
        eprintln!("--emit phpext requires at least one #[Export] function or class, or an --export-constant selection");
        process::exit(1);
    }
    decl_trace("after exports-scan", &ast);
    decl_trace_truncation("before opt-prop", &ast);
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

    if check_only && exported_functions.is_empty() && exported_classes.is_empty() {
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
    let ast = post_typecheck_optimizer.propagate(
        ast, check_result.mixed_storage_local_names(), check_result.buffer_read_sites.clone(),
    );
    timings.record_since("opt-prop", phase_started);
    decl_trace("after opt-prop", &ast);

    crate::progress::phase("opt-post");
    let phase_started = Instant::now();
    // Pruning and normalization both run the single-case switch rewrite, which materializes the
    // default body into BOTH branches of the synthesized `if` with the original's spans. The
    // checker's local-binding decisions are keyed BY SPAN, so these phases are told which spans
    // carry one and the rewrite vetoes itself rather than duplicating a decision.
    let ast = post_typecheck_optimizer.prune(ast, check_result.local_binding_decision_spans());
    timings.record_since("opt-post", phase_started);
    decl_trace("after opt-post", &ast);

    crate::progress::phase("opt-norm");
    let phase_started = Instant::now();
    let ast = post_typecheck_optimizer.normalize(ast, check_result.local_binding_decision_spans());
    timings.record_since("opt-norm", phase_started);
    decl_trace("after opt-norm", &ast);

    crate::progress::phase("dce");
    let phase_started = Instant::now();
    // Tail-sinking clones the tail of an `if`/`switch`/`try` into every branch, and a clone keeps
    // the original's spans — the same span-keyed hazard, in the other pass that clones.
    let ast = post_typecheck_optimizer
        .eliminate_dead_code(ast, check_result.local_binding_decision_spans());
    timings.record_since("dce", phase_started);
    decl_trace("after dce", &ast);

    crate::progress::phase("decl-reach");
    let phase_started = Instant::now();
    let metadata_trace_target = std::env::var("ELEPHC_METADATA_TRACE").ok();
    let metadata_trace_before = metadata_trace_target.as_deref().map(|target| {
        let normalized = target.trim_start_matches('\\');
        (
            check_result
                .interfaces
                .keys()
                .any(|name| name.eq_ignore_ascii_case(normalized)),
            check_result
                .classes
                .keys()
                .any(|name| name.eq_ignore_ascii_case(normalized)),
        )
    });
    let mut exported_function_names: HashSet<String> = exported_functions.keys().cloned().collect();
    exported_function_names.extend(exported_classes.iter().map(|class| format!("{}::", class.name)));
    // Nothing in the program calls an include unit by name -- a runtime include reaches it through
    // the unit table -- so it is rooted here, like an export, or pruning would drop it.
    exported_function_names.extend(preload_include_units.iter().map(|unit| unit.function.clone()));
    if entry_return_injected {
        exported_function_names.insert(crate::entry_return::ENTRY_RETURN_FUNCTION.to_string());
    }
    // The hoisted `opcache.preload` body is a reachability ROOT. Nothing in PHP calls it --
    // the `--web` entry stub does, in assembly, before the request loop -- so the declaration
    // pruner sees an unreferenced function and removes it, taking the preload's side effects
    // with it. That failure is silent: the responses come out clean, which is what the hoist
    // was for, and the preload simply never runs.
    if preload_startup_path.is_some() {
        exported_function_names.insert(crate::web_prelude::PRELOAD_STARTUP_FN.to_string());
    }
    if web_application {
        exported_function_names.extend([
            web_prelude::application::READY,
            web_prelude::application::REQUEST,
            web_prelude::application::SHUTDOWN,
            web_prelude::application::CONTEXT_OPTION,
        ].map(str::to_string));
    }
    // A generic instantiation is reachable by construction — it exists only because a call
    // site asked for it — but no call site NAMES it: the source still says `identity(5)` and
    // lowering is what resolves that to `identity<int>`. Without this the pruner drops every
    // instantiation as unreferenced and lowering finds nothing to call.
    exported_function_names.extend(crate::generics::instantiation_roots(
        &check_result.requested_instantiations,
    ));
    // Lowering calls the Throwable string helpers for `getTraceAsString()`, `__toString()` and a
    // Throwable's string conversion; no declaration-level call edge leads to them.
    exported_function_names.extend(
        crate::throwable_string_prelude::FUNCTIONS
            .iter()
            .map(|name| name.to_string()),
    );
    // `assert($x)` is routed to its prelude helper during lowering, after this pass; no call edge
    // names the helper in the AST, so without a root the pruner removed it and the compiled binary
    // died with `Call to undefined function __elephc_assert_one_arg()`.
    exported_function_names.insert(crate::assert_prelude::ASSERT_ONE_ARG_NAME.to_string());
    let exported_function_names = exported_function_names;
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
    // Reachability removes bridge requirements belonging only to dead declarations.
    if emit == Emit::Phpext && !check_result.required_libraries.is_empty() {
        crate::progress::clear();
        eprintln!("{PHPEXT_CORE_RUNTIME_ONLY}");
        process::exit(1);
    }
    if emit == Emit::Phpext && !exported_classes.is_empty() {
        if let Err(error) = crate::phpext::constants::validate_native_widening(
            &check_result.classes, &check_result.interfaces,
        ) {
            crate::progress::clear();
            errors::report(&error.with_file(filename.to_string()));
            process::exit(1);
        }
    }
    decl_trace("after decl-reach", &ast);
    if let Ok(target) = std::env::var("ELEPHC_DECL_TRACE") {
        let key = crate::names::php_symbol_key(target.trim_start_matches('\\'));
        let in_checker = check_result
            .functions
            .keys()
            .filter(|name| crate::names::php_symbol_key(name.trim_start_matches('\\')) == key)
            .count();
        let forced = forced_groups
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        let groups_holding: Vec<String> = prelude_inventory
            .groups
            .values()
            .filter(|group| group.functions.contains(&key))
            .map(|group| group.id.clone())
            .collect();
        decl_trace_note(
            "after decl-reach",
            &format!(
                "checker_functions={in_checker} inventory_groups_holding={groups_holding:?} forced_groups=[{forced}]"
            ),
        );
    }
    if let (Some(target), Some((interface_before, class_before))) =
        (metadata_trace_target.as_deref(), metadata_trace_before)
    {
        let normalized = target.trim_start_matches('\\');
        let interface_after = check_result
            .interfaces
            .keys()
            .any(|name| name.eq_ignore_ascii_case(normalized));
        let class_after = check_result
            .classes
            .keys()
            .any(|name| name.eq_ignore_ascii_case(normalized));
        eprintln!(
            "[elephc-metadata-trace] target={normalized} interface_before={interface_before} interface_after={interface_after} class_before={class_before} class_after={class_after}"
        );
    }
    codegen::prepare_declared_name_order(
        &ast,
        &check_result.classes,
        &check_result.interfaces,
    );

    crate::progress::phase("ir-lower");
    let phase_started = Instant::now();
    let mut source_units = BTreeMap::new();
    for unit in std::iter::once(entry_source_unit)
        .chain(entry_included_sources.source_units.into_values())
        .chain(declaration_source_files.source_units.into_values())
    {
        if let Err(error) = unit.insert_into(&mut source_units, Span::dummy()) {
            crate::progress::clear();
            errors::report(&error);
            process::exit(1);
        }
    }
    // `run_collecting_included` reports every source it actually loaded. Its
    // declaration map can legitimately omit a file whose declarations were
    // later eliminated, but a typed activation event still needs that physical
    // source identity. Fill only absent manifest paths; duplicate snapshots keep
    // the existing conflict check above rather than silently replacing content.
    let activation_source_paths = classlike_activation_source_paths(&ast);
    // Preloaded class files and the preload graph's fully-declared sources are physical inputs of
    // this module: a class declaration merged from the preload keeps its span, and the runtime
    // activation/reflection tables need the canonical source row. Without it a preloaded class
    // reached through a runtime `include_once` (Symfony's generated service factories do exactly
    // that) fell back to the autoloader and became an eval-declared class, whose reflection lost
    // the method visibility. Adding a single extra preload class was enough to shift which pass
    // loaded the app controller and exposed this.
    let preload_source_paths: Vec<std::path::PathBuf> = preload_class_source_files
        .values()
        .map(std::path::PathBuf::from)
        .chain(preload_compiled_whole.iter().cloned())
        .collect();
    // A unit's file is a source of this module too: the closed-world include table has one row
    // per catalog source, and a unit whose file has no row is never reached.
    let unit_source_paths: Vec<std::path::PathBuf> = preload_include_units
        .iter()
        .map(|unit| unit.canonical_path.clone())
        .collect();
    for path in opcache_included_files
        .iter()
        .chain(opcache_autoloaded_files.iter())
        .chain(activation_source_paths.iter())
        .chain(preload_source_paths.iter())
        .chain(unit_source_paths.iter())
    {
        let canonical = crate::source::canonical(&path).unwrap_or_else(|_| path.clone());
        if source_units.contains_key(&canonical) {
            continue;
        }
        let source = match crate::source::read_physical_source(path) {
            Ok(source) => source,
            Err(error) => {
                crate::progress::clear();
                errors::report(&crate::errors::CompileError::new(
                    Span::dummy(),
                    &format!("Autoload source snapshot: cannot read '{}': {}", path.display(), error),
                ));
                process::exit(1);
            }
        };
        let unit = resolver::SourceUnit {
            canonical_path: canonical,
            mode: SourceMode::from_path(path),
            source: source.into(),
        };
        if let Err(error) = unit.insert_into(&mut source_units, Span::dummy()) {
            crate::progress::clear();
            errors::report(&error);
            process::exit(1);
        }
    }
    let source_catalog = match ir::SourceCatalog::from_units(source_units.into_values()) {
        Ok(catalog) => catalog,
        Err(error) => {
            crate::progress::clear();
            errors::report(&error);
            process::exit(1);
        }
    };
    // With class files the runtime could load instead, a body that fails EIR validation is handed
    // back like a body the backend refuses: its class file goes to the runtime autoloader and the
    // build runs again. Without them, validation stays fail-fast.
    ir_lower::collect_invalid_bodies(!deferrable_class_files.is_empty());
    ir_lower::set_class_files_deferred(!deferred_class_files.is_empty());
    let mut ir_module = match ir_lower::lower_program_with_source_catalog(
        &ast,
        &check_result,
        target,
        Path::new(filename),
        web,
        source_catalog,
    ) {
        Ok(module) => module,
        Err(ir_lower::LoweringError::InvalidBodies { first, failures, module }) => {
            crate::progress::clear();
            crate::autoload::deferral::BackendDeferral::for_module(
                &module,
                &deferral_class_files,
                &deferrable_class_files,
                &deferred_class_files,
                &probe_only_deferred,
                &eager_function_files,
            )
            .rerun_without_refused_bodies(&failures);
            for (body, error) in &failures {
                eprintln!("EIR validation failure in {body}: {error}");
                // The whole refused body: an operand id alone does not say which source built it.
                if std::env::var("ELEPHC_EIR_DUMP_INVALID").as_deref() == Ok("1") {
                    if let Some(function) = module
                        .functions
                        .iter()
                        .chain(module.class_methods.iter())
                        .chain(module.closures.iter())
                        .find(|function| &function.name == body)
                    {
                        eprintln!("{}", crate::ir::print_function(function));
                    }
                }
            }
            eprintln!("EIR lowering error: EIR validation failed: {first:?}");
            process::exit(1);
        }
        Err(ir_lower::LoweringError::Unsupported(error)) => {
            // A refused shape is an ordinary source-level diagnostic, so it is reported with the
            // same file/line formatting as a checker error instead of a bare backend message.
            crate::progress::clear();
            errors::report(&error.with_file(filename.to_string()));
            process::exit(1);
        }
        Err(err) => {
            crate::progress::clear();
            eprintln!("EIR lowering error: {}", err);
            process::exit(1);
        }
    };
    ir_module.web_application = web_application;
    crate::phpext::class_helpers::lower(&mut ir_module, &exported_classes);
    ir_module.declared_class_source_files =
        std::mem::take(&mut entry_included_sources.class_likes);
    ir_module
        .declared_class_source_files
        .extend(declaration_source_files.class_likes);
    for (class_key, path) in &preload_class_source_files {
        ir_module
            .declared_class_source_files
            .entry(class_key.clone())
            .or_insert_with(|| path.clone());
    }
    ir_module.declared_function_source_files =
        std::mem::take(&mut entry_included_sources.functions);
    ir_module
        .declared_function_source_files
        .extend(declaration_source_files.functions);
    // The autoload pass PERFORMED these inclusions: it opened each file and spliced its
    // declarations into the program. At runtime they are already-included files, and an
    // `include_once` reaching one through a computed path must answer "already included"
    // instead of re-running it into a redeclaration fatal.
    // The preload graph's fully-declared files belong in the same set: the compiler took each
    // of them whole, so an `include_once` reaching one at runtime must answer "already included"
    // rather than re-run a top level whose declarations are already in the binary.
    ir_module.include_units = preload_include_units
        .iter()
        .filter(|unit| {
            ir_module
                .functions
                .iter()
                .any(|function| function.name.eq_ignore_ascii_case(&unit.function))
        })
        .cloned()
        .collect();
    if entry_return_injected {
        if let Some(entry_path) = ir_module.source_path.clone() {
            if ir_module.functions.iter().any(|function| {
                function
                    .name
                    .eq_ignore_ascii_case(crate::entry_return::ENTRY_RETURN_FUNCTION)
            }) {
                ir_module.include_units.push(crate::ir::IncludeUnit {
                    canonical_path: std::path::PathBuf::from(entry_path),
                    function: crate::entry_return::ENTRY_RETURN_FUNCTION.to_string(),
                });
            }
        }
    }
    let mut compile_time_inclusions = opcache_autoloaded_files.clone();
    compile_time_inclusions.extend(preload_compiled_whole);
    // A class the preload named through the container's `$classes` list is compiled here, but its
    // file is not in `preload_compiled_whole` (that set only carries fully-declared includes of
    // the preload's own require graph). Symfony's generated service factories `include_once` the
    // class file; without this mark the interpreter executed it and redeclared the AOT class as an
    // eval class. Only files whose class actually reached the module are marked, so a deferred
    // class keeps its runtime include.
    // Every class-like counts: Symfony's list names interfaces and traits too
    // (`ContainerInterface`, `CompiledUrlMatcherTrait`), and a closed-world binary refuses the
    // autoloader's include of a file the table cannot answer for.
    let compiled_class_keys: std::collections::HashSet<String> = ir_module
        .class_infos
        .keys()
        .chain(ir_module.interface_infos.keys())
        .chain(ir_module.enum_infos.keys())
        .chain(ir_module.declared_trait_names.iter())
        .map(|name| crate::names::php_symbol_key(name.trim_start_matches('\\')))
        .collect();
    compile_time_inclusions.extend(
        preload_class_source_files
            .iter()
            .filter(|(class_key, _)| compiled_class_keys.contains(class_key.as_str()))
            .map(|(_, path)| std::path::PathBuf::from(path)),
    );
    crate::autoload::record_compile_time_inclusions(
        &mut ir_module,
        &ast,
        &compile_time_inclusions,
    );
    ir_module.required_runtime_features.class_introspection |= ir_module
        .interface_infos
        .values()
        .any(|info| info.declaration_span != crate::span::Span::dummy());
    ir_module.included_files = opcache_manifest
        .iter()
        .map(|entry| entry.path.clone())
        .collect();
    if closed_world {
        // A runtime include is answered from the compiled-source table; the rest of the world
        // must not need the interpreter at all, and the build says why when it does.
        if ir_lower::module_has_dynamic_include(&ir_module) {
            ir_module.closed_includes = Some(crate::closed_world::include_table(&ir_module));
        }
        let mut reasons = Vec::new();
        let refused_class_files: Vec<&PathBuf> = deferred_class_files
            .iter()
            .filter(|path| !probe_only_deferred.contains(*path))
            .collect();
        if !refused_class_files.is_empty() {
            let mut files = refused_class_files
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>();
            files.sort();
            reasons.push(format!(
                "{} class file(s) handed to the runtime autoloader instead of compiled: {}",
                files.len(),
                files.join(", ")
            ));
        }
        reasons.extend(ir_lower::closed_world_bridge_reasons(&ir_module));
        // A class the autoloader brought in from the project itself can evaluate a computed
        // string too; the entry's own were caught before anything was compiled.
        if closed_world_mode == crate::closed_world::Mode::Auto {
            if let Some(site) = crate::closed_world::project_computed_eval(&ast) {
                reasons.push(format!("eval() of a computed string in the project's code at {site}"));
            }
        }
        if !reasons.is_empty() && closed_world_mode == crate::closed_world::Mode::Auto {
            crate::closed_world::rerun_with_interpreter(&reasons);
        }
        if !reasons.is_empty() {
            crate::progress::clear();
            eprintln!(
                "error: the program still needs the interpreter ({} reason{}):",
                reasons.len(),
                if reasons.len() == 1 { "" } else { "s" }
            );
            for reason in &reasons {
                eprintln!("  - {reason}");
            }
            process::exit(1);
        }
    }
    timings.record_since("ir-lower", phase_started);
    if let Ok(target) = std::env::var("ELEPHC_DECL_TRACE") {
        let key = crate::names::php_symbol_key(target.trim_start_matches('\\'));
        let in_module: Vec<&str> = ir_module
            .functions
            .iter()
            .map(|function| function.name.as_str())
            .filter(|name| crate::names::php_symbol_key(name.trim_start_matches('\\')) == key)
            .collect();
        decl_trace_note("after ir-lower", &format!("module_functions={in_module:?}"));
    }

    // EIR owns every backend-visible declaration, signature and source-identity datum now.
    // Keeping the optimized AST and the complete checker graph through assembly doubles the
    // peak working set on large applications, even though the backend only still needs the
    // requested native library names. Move that small list out, then release both graphs before
    // EIR optimization/codegen starts.
    let required_libraries = std::mem::take(&mut check_result.required_libraries);
    drop(ast);
    drop(check_result);

    if emit_ir {
        eir_output::emit(ir_module, emit, filename, ir_opt, &exported_functions, &mut timings);
        return;
    }

    if emit.is_library() || (check_only && !exported_functions.is_empty()) {
        let validate = if emit == Emit::Phpext { exports::validate_phpext_call_graph } else { exports::validate_cdylib_call_graph };
        if let Err(error) = validate(&ir_module, &exported_functions) {
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
    // The shape merge has now folded everything the EIR comparison could prove identical, so what
    // remains is the measured case for widening it: candidates whose lowered bodies agree.
    if report_generics {
        eprintln!(
            "{}",
            codegen::generic_body_sharing_oracle(&ir_module, regalloc_linear)
        );
    }

    // With class files the runtime could load instead, the backend collects EVERY body it
    // refuses rather than stopping at the first, so one re-run can hand them all back.
    let deferral = if deferrable_class_files.is_empty() {
        None
    } else {
        codegen::collect_backend_refusals(true);
        Some(crate::autoload::deferral::BackendDeferral::for_module(
            &ir_module,
            &deferral_class_files,
            &deferrable_class_files,
            &deferred_class_files,
            &probe_only_deferred,
            &eager_function_files,
        ))
    };
    backend::emit_and_link(backend::BackendInputs {
        repl_build: entry.is_repl_host(),
        announce_success: !entry.is_test_suite(),
        deferral,
        filename,
        with_crates: &with_crates,
        ini_overrides: &ini_overrides,
        linked_php_surfaces: &linked_php_surfaces,
        hosted_php_ext: hosted_php_ext.as_ref(),
        ir_module,
        web,
        web_isolation,
        extra_link_libs: &extra_link_libs,
        extra_link_paths: &extra_link_paths,
        extra_frameworks: &extra_frameworks,
        required_libraries: &required_libraries,
        target,
        emit,
        heap_size,
        gc_stats,
        counters,
        instrument,
        heap_debug,
        exported_functions: &exported_functions,
        exported_constants: &exported_constants,
        exported_classes: &exported_classes,
        regalloc_linear,
        emit_debug_info,
        keep_symbols,
        output_paths: &output_paths,
        emit_source_map,
        emit_asm,
        timings: &mut timings,
    });
}


/// Returns whether two paths name the same file on disk, or the same text when either is absent.
///
/// `canonicalize` resolves `.`, `..` and symlinks, which a textual comparison misses --
/// `./bin/console` and `bin/console` are the same file. It fails on a path that does not exist
/// yet, which is the normal case for an output, so the textual comparison stays as the fallback.
fn same_file(left: &Path, right: &Path) -> bool {
    match (crate::source::canonical(&left), crate::source::canonical(&right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

/// Returns every physical source path named by a retained typed source marker.
///
/// The resolver has already established these paths while it still owned the
/// original source. This final inventory is intentionally after optimization:
/// only events that can reach EIR need a SourceId, and it closes the gap where
/// an autoload manifest does not retain a source unit for a nested declaration
/// or include-once marker.
fn classlike_activation_source_paths(
    program: &crate::parser::ast::Program,
) -> std::collections::BTreeSet<std::path::PathBuf> {
    use crate::parser::ast::StmtKind;

    // Every depth, closures included: a compiled DI container keeps `include_once` factories in
    // closure literals, and a marker there whose source was never registered failed EIR
    // validation with `UnresolvedSource`.
    let mut paths = std::collections::BTreeSet::new();
    crate::parser::ast::visit::walk_stmts(program, &mut |statement| match &statement.kind {
        StmtKind::ClassLikeActivate { source_path, .. }
        | StmtKind::IncludeOnceMark { source_path }
        | StmtKind::IncludeOnceGuard { source_path, .. } => {
            paths.insert(source_path.clone());
        }
        _ => {}
    });
    paths
}

/// Returns the declarations `opcache.preload` supplies, or `None` when the directive is unset.
///
/// The directive is the only input: a program that does not set it compiles exactly as before, and
/// one that does gets the same world a preloaded php-fpm serves. Nothing here knows what framework
/// wrote the file.
///
/// Every `opcache.preload` given is a root, in order, not only the last one: a binary has no
/// startup per environment, so one that must answer like php under more than one configuration
/// (Symfony's `--env=prod` and `--env=dev`, each with the preload file its own container dump
/// wrote) needs the declarations of every one of them. A later preload contributes only what the
/// earlier ones did not already declare, as php declares each class once.
fn preloaded_declarations(
    ini_overrides: &[(String, String)],
    cache_enabled: bool,
    base_dir: &Path,
    defines: &std::collections::HashSet<String>,
    autoload: &autoload::Registry,
) -> Result<Option<crate::opcache_preload_sources::PreloadedDeclarations>, errors::CompileError> {
    // php does not preload when the accelerator is off, and does not even look at the path: a
    // CLI build without `opcache.enable_cli=1` preloads nothing, as `preload_verdict` reports.
    if !cache_enabled {
        return Ok(None);
    }
    let mut merged: Option<crate::opcache_preload_sources::PreloadedDeclarations> = None;
    for (_, directive) in ini_overrides
        .iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case("opcache.preload"))
    {
        let Some(path) = crate::opcache_preload_sources::resolve_preload_path(directive, base_dir)
        else {
            continue;
        };
        if !path.exists() {
            // php treats an unreadable preload file as a startup FATAL, and the AOT equivalent of
            // "not there" is a compile error -- the same verdict `opcache_prelude::preload_verdict`
            // already reaches for the directive's own reporting.
            return Err(errors::CompileError::new(
                crate::span::Span::dummy(),
                &format!("opcache.preload: '{}' does not exist", path.display()),
            ));
        }
        let next =
            crate::opcache_preload_sources::preload_declarations(&path, base_dir, defines, autoload)?;
        merged = Some(match merged {
            None => next,
            Some(mut acc) => {
                let declared = crate::opcache_preload_sources::declared_names(&acc.declarations);
                acc.declarations.extend(crate::opcache_preload_sources::without_redeclarations(
                    next.declarations,
                    &declared,
                ));
                for file in next.compiled_whole {
                    if !acc.compiled_whole.contains(&file) {
                        acc.compiled_whole.push(file);
                    }
                }
                for (class, file) in next.class_files {
                    acc.class_files.entry(class).or_insert(file);
                }
                acc.include_units.extend(next.include_units);
                acc
            }
        });
    }
    Ok(merged)
}

// ===== `ELEPHC_DECL_TRACE`: where one function declaration is lost =====
//
// The FUNCTION-side twin of `ELEPHC_METADATA_TRACE` above, which answers the same question for a
// class or an interface. Set `ELEPHC_DECL_TRACE=<function name>` and every phase below prints how
// many declarations of that name the program still holds, plus whether the checker's function
// table and the EIR module agree with it.
//
// It exists because the disagreement is what a backend error actually reports, four passes late
// and under another function's name. `call to unknown function error_log` on a Symfony `--web`
// build meant the AST had lost `error_log` while `check_result.functions` had kept it; the trace
// named the pass that dropped it (`opt-prop`) in one 60-second run, after two agents had spent a
// day on the wrong hypothesis. Every line is behind the env lookup, so an unset variable costs one
// `std::env::var` per phase.

/// Counts the `FunctionDecl`s for one normalized name anywhere in a program.
fn decl_trace_count(body: &[crate::parser::ast::Stmt], key: &str) -> usize {
    use crate::parser::ast::StmtKind;
    let mut total = 0;
    for stmt in body {
        match &stmt.kind {
            StmtKind::FunctionDecl { name, .. } => {
                if crate::names::php_symbol_key(name.trim_start_matches('\\')) == key {
                    total += 1;
                }
            }
            StmtKind::NamespaceBlock { body, .. } => total += decl_trace_count(body, key),
            StmtKind::Synthetic(body) => total += decl_trace_count(body, key),
            StmtKind::IncludeOnceGuard { body, .. } => total += decl_trace_count(body, key),
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                total += decl_trace_count(then_body, key);
                for (_, arm) in elseif_clauses {
                    total += decl_trace_count(arm, key);
                }
                if let Some(arm) = else_body {
                    total += decl_trace_count(arm, key);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                total += decl_trace_count(try_body, key);
                for catch in catches {
                    total += decl_trace_count(&catch.body, key);
                }
                if let Some(body) = finally_body {
                    total += decl_trace_count(body, key);
                }
            }
            _ => {}
        }
    }
    total
}

/// Counts `FunctionVariantGroup` / `FunctionVariantMark` statements naming one key.
fn decl_trace_variants(body: &[crate::parser::ast::Stmt], key: &str) -> (usize, usize) {
    use crate::parser::ast::StmtKind;
    let mut groups = 0;
    let mut marks = 0;
    for stmt in body {
        match &stmt.kind {
            StmtKind::FunctionVariantGroup { name, .. } => {
                if crate::names::php_symbol_key(name.trim_start_matches('\\')) == key {
                    groups += 1;
                }
            }
            StmtKind::FunctionVariantMark { name, .. } => {
                if crate::names::php_symbol_key(name.trim_start_matches('\\')) == key {
                    marks += 1;
                }
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => {
                let (g, m) = decl_trace_variants(body, key);
                groups += g;
                marks += m;
            }
            _ => {}
        }
    }
    (groups, marks)
}

/// Prints how many declarations of `ELEPHC_DECL_TRACE` the program holds at one pipeline point.
fn decl_trace(label: &str, program: &[crate::parser::ast::Stmt]) {
    let Ok(target) = std::env::var("ELEPHC_DECL_TRACE") else {
        return;
    };
    let key = crate::names::php_symbol_key(target.trim_start_matches('\\'));
    let (groups, marks) = decl_trace_variants(program, &key);
    eprintln!(
        "[elephc-decl-trace] {label:<22} ast_decls={} variant_groups={groups} variant_marks={marks}",
        decl_trace_count(program, &key)
    );
}

/// Names a statement kind for the trace.
fn decl_trace_kind(stmt: &crate::parser::ast::Stmt) -> String {
    use crate::parser::ast::StmtKind;
    match &stmt.kind {
        StmtKind::FunctionDecl { name, .. } => format!("FunctionDecl({name})"),
        StmtKind::ClassDecl { name, .. } => format!("ClassDecl({name})"),
        StmtKind::Synthetic(body) => format!("Synthetic[{}]", body.len()),
        StmtKind::NamespaceBlock { name, body } => {
            format!("NamespaceBlock({name:?})[{}]", body.len())
        }
        StmtKind::NamespaceDecl { name } => format!("NamespaceDecl({name:?})"),
        StmtKind::IncludeOnceGuard { source_path, body, .. } => {
            format!("IncludeOnceGuard({source_path:?})[{}]", body.len())
        }
        StmtKind::ExprStmt(_) => "ExprStmt".to_string(),
        StmtKind::Return(_) => "Return".to_string(),
        StmtKind::Throw(_) => "Throw".to_string(),
        StmtKind::If { .. } => "If".to_string(),
        StmtKind::Try { .. } => "Try".to_string(),
        StmtKind::Echo(_) => "Echo".to_string(),
        other => format!("{other:?}").chars().take(60).collect(),
    }
}

/// Reports where the top level first stops falling through, and where the traced name is
/// declared relative to that point.
fn decl_trace_truncation(label: &str, program: &[crate::parser::ast::Stmt]) {
    let Ok(target) = std::env::var("ELEPHC_DECL_TRACE") else {
        return;
    };
    let key = crate::names::php_symbol_key(target.trim_start_matches('\\'));
    let terminal = program
        .iter()
        .position(|stmt| crate::termination::stmt_guarantees_termination(stmt));
    let declares = program
        .iter()
        .position(|stmt| decl_trace_count(std::slice::from_ref(stmt), &key) > 0);
    let terminal_kind = terminal
        .map(|index| decl_trace_kind(&program[index]))
        .unwrap_or_else(|| "none".to_string());
    let terminal_span = terminal
        .map(|index| format!("{:?}", program[index].span))
        .unwrap_or_default();
    eprintln!(
        "[elephc-decl-trace] {label:<22} top_level={} first_terminal={terminal:?} kind={terminal_kind} span={terminal_span} declares_at={declares:?}",
        program.len()
    );
}

/// Prints an arbitrary note at one pipeline point, under the same gate.
fn decl_trace_note(label: &str, note: &str) {
    if std::env::var("ELEPHC_DECL_TRACE").is_err() {
        return;
    }
    eprintln!("[elephc-decl-trace] {label:<22} {note}");
}

/// Warns once per class file the by-reference-place rule handed to the runtime autoloader.
fn report_by_ref_place_deferrals(files: Vec<PathBuf>) {
    for path in files {
        errors::report_warning(&errors::CompileWarning::new(
            Span::dummy(),
            &format!(
                "autoload: '{}' is not compiled (it passes a property or array element to a \
                 by-reference constructor parameter, which a native call cannot keep \
                 linked); its classes are loaded at run time by the autoloader instead",
                path.display()
            ),
        ));
    }
}

/// Merges the autoloaded and entry-file declaration maps into one name-to-path lookup.
///
/// Both halves are needed: the entry file's own declarations never pass through the autoload
/// registry, and the autoloaded ones are the bulk of a real application. A name declared in both
/// resolves to the autoloaded path, which is the one the program actually runs.
fn declaration_source_file_paths(
    autoloaded: &autoload::DeclarationSourceFiles,
    entry: &crate::resolver::IncludedDeclarationSources,
) -> std::collections::HashMap<String, String> {
    let mut paths = std::collections::HashMap::new();
    let halves = [
        (&entry.class_likes, &entry.functions),
        (&autoloaded.class_likes, &autoloaded.functions),
    ];
    for (class_likes, functions) in halves {
        paths.extend(
            class_likes
                .iter()
                .chain(functions.iter())
                .map(|(name, path)| (name.clone(), path.clone())),
        );
    }
    paths
}

/// Whether every class `file` declares is named by the program only through existence probes.
fn declares_only_probed_classes<V: AsRef<str>>(
    file: &Path,
    class_files: &HashMap<String, V>,
    probe_only_names: &HashSet<String>,
) -> bool {
    let declared: Vec<&String> = class_files
        .iter()
        .filter(|(_, declaring)| {
            Path::new(declaring.as_ref())
                .canonicalize()
                .is_ok_and(|declaring| declaring == file)
        })
        .map(|(class, _)| class)
        .collect();
    !declared.is_empty()
        && declared
            .iter()
            .all(|class| probe_only_names.contains(&crate::names::php_symbol_key(class)))
}

/// Gives a class file refused for a missing parent the unit that fails like php's load of it.
/// Once per file: a remembered refusal and a new one can both name it.
fn add_missing_parent_unit(
    ast: &mut crate::parser::ast::Program,
    units: &mut Vec<crate::ir::IncludeUnit>,
    canonical: &Path,
    parent: &str,
) {
    if units.iter().any(|unit| unit.canonical_path == canonical) {
        return;
    }
    let (declaration, unit) = crate::opcache_preload_sources::missing_parent_unit(canonical, parent);
    ast.insert(0, declaration);
    units.push(unit);
}

/// Whether a checker refusal is a declaration php itself rejects when it loads the file: an
/// override whose signature is incompatible with the method it overrides, or a parent class-like
/// no file declares (symfony/framework-bundle's `KernelBrowser` over the dev-only browser-kit):
/// php raises `Class "…" not found` when the file is loaded, and the closed world sees every file
/// the autoloader could load.
fn php_refuses_to_load(message: &str) -> bool {
    message.contains("when overriding")
        || message.contains("must be compatible with")
        || message.contains("Unknown class referenced during inheritance flattening")
}

/// The first parent of a class declared in `path`: what loading that file asks the autoloaders
/// for, so the unit of a file over an unloadable ancestor fails down the chain as php's load does.
fn first_parent_declared_by(
    path: &Path,
    class_files: &HashMap<String, String>,
    parent_edges: &[(String, Vec<String>)],
) -> Option<String> {
    let mut classes: Vec<&String> = class_files
        .iter()
        .filter(|(_, file)| Path::new(file).canonicalize().is_ok_and(|file| file == path))
        .map(|(name, _)| name)
        .collect();
    classes.sort();
    classes.iter().find_map(|class| {
        let key = crate::names::php_symbol_key(class.trim_start_matches('\\'));
        parent_edges
            .iter()
            .find(|(name, _)| crate::names::php_symbol_key(name.trim_start_matches('\\')) == key)
            .and_then(|(_, parents)| parents.first().cloned())
    })
}

/// Whether a class declared in `path` has, among its ancestors, a class declared in a file php
/// refuses to load (`php_load_fatal`).
fn extends_a_php_load_fatal_class(
    path: &Path,
    class_files: &HashMap<String, String>,
    parent_edges: &[(String, Vec<String>)],
    php_load_fatal: &HashSet<PathBuf>,
) -> bool {
    if php_load_fatal.is_empty() {
        return false;
    }
    let canonical = |file: &str| Path::new(file).canonicalize().unwrap_or_else(|_| PathBuf::from(file));
    let file_of = |class: &str| {
        class_files
            .iter()
            .find(|(name, _)| crate::names::php_symbol_key(name.trim_start_matches('\\')) == crate::names::php_symbol_key(class.trim_start_matches('\\')))
            .map(|(_, file)| canonical(file))
    };
    let parents_of = |class: &str| -> Vec<String> {
        let key = crate::names::php_symbol_key(class.trim_start_matches('\\'));
        parent_edges
            .iter()
            .filter(|(name, _)| crate::names::php_symbol_key(name.trim_start_matches('\\')) == key)
            .flat_map(|(_, parents)| parents.iter().cloned())
            .collect()
    };
    let declared_here: Vec<&String> = class_files
        .iter()
        .filter(|(_, file)| canonical(file) == path)
        .map(|(name, _)| name)
        .collect();
    let mut stack: Vec<String> = declared_here.iter().flat_map(|class| parents_of(class)).collect();
    let mut seen = HashSet::new();
    while let Some(class) = stack.pop() {
        if !seen.insert(crate::names::php_symbol_key(class.trim_start_matches('\\'))) {
            continue;
        }
        if file_of(&class).is_some_and(|file| php_load_fatal.contains(&file)) {
            return true;
        }
        stack.extend(parents_of(&class));
    }
    false
}
