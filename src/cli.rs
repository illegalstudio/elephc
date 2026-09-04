//! Purpose:
//! Owns exact top-level command dispatch plus compiler-option parsing and target selection.
//! Keeps `elephc native` isolated while preserving every legacy compile invocation.
//!
//! Called from:
//! - `crate::main()` before invoking `crate::pipeline::compile()`.
//!
//! Key details:
//! - Only an exact `args[1] == "native"` selects native dependency commands.
//! - Exits immediately on invalid CLI state so later stages receive normalized options.

use std::collections::HashSet;
use std::process;

pub(crate) use crate::codegen::Emit;
use crate::codegen::WebIsolation;
use crate::codegen::platform::Target;
use crate::native_deps::{native_help, parse_native_args, NativeCommand, NativeParseOutcome};

/// The bridge crates `--with-monitoring` turns on.
///
/// They are also the two the `--with-<name>` surface must not offer by name.
/// Both are ordinary bridges, so the accepted set — derived from the bridge
/// table — would list `instrument` and `probe` for free, and the error text for
/// a mistyped capability would advertise the two names this whole flag exists to
/// retire. Naming them once here keeps the flag and its exclusion from drifting
/// apart.
const MONITORING_BRIDGES: [&str; 2] = ["instrument", "probe"];

/// Non-bridge runtime capabilities accepted by `--with-<name>`. `mysqli` is not
/// a bridge of its own: it force-injects the mysqli prelude, which links the
/// shared `elephc_pdo` archive (and never injects the PDO classes).
const RUNTIME_CAPABILITY_FLAGS: &[&str] = &["regex", "mysqli"];

/// Short usage line shown after every parameter error, alongside the `--help` hint.
/// The full categorized reference lives in `HELP`.
pub(crate) const USAGE: &str = "Usage: elephc [OPTIONS] <source-file>";

/// Compiler package version embedded into the binary by Cargo.
pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

/// ASCII mascot printed by `--mascotte`, embedded at compile time (not read
/// from a filesystem path — the original source file lives outside this
/// repo, on one contributor's machine, and wouldn't exist anywhere else).
const MASCOTTE_ART: &str = "        _ooOoo_
       o8888888o
       (| -_- |)
       0\\  =  /0
     ___/`---'\\___
  .'  \\\\|     |//  '.
  / \\\\|||  :  |||// \\
  \\  '-.\\\\___//.-'  /
   '-._   '-'   _.-'
     `-.______.-'
        `=---='";

/// Fixed pool of zen/developer quotes `--mascotte` picks from at random.
const ZEN_QUOTES: &[&str] = &[
    "There is no cloud, just someone else's computer.",
    "It works on my machine.",
    "The best code is no code at all.",
    "Premature optimization is the root of all evil.",
    "Simplicity is the ultimate sophistication.",
    "The obstacle is the path.",
    "First, solve the problem. Then, write the code.",
    "A bug in production is worth two in the backlog.",
    "When in doubt, print it out.",
    "Silence is also an answer — usually a segfault.",
];

/// Returns true if `--mascotte` appears anywhere in the argument list.
pub(crate) fn wants_mascotte(args: &[String]) -> bool {
    args.iter().any(|a| a == "--mascotte")
}

/// Prints the ASCII mascot and a randomly chosen quote to stdout. The index
/// comes from the current time's sub-second microseconds — good enough for a
/// cosmetic banner, no `rand` dependency needed.
pub(crate) fn print_mascotte() {
    let micros = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_micros())
        .unwrap_or(0);
    let quote = ZEN_QUOTES[(micros as usize) % ZEN_QUOTES.len()];
    println!("{}\n\n  \"{}\"\n", MASCOTTE_ART, quote);
}

/// Returns true if `-h` or `--help` appears anywhere in the argument list, so
/// help always wins regardless of position or what else was passed alongside
/// it (e.g. `elephc --check --help app.php` still shows help).
fn wants_help(args: &[String]) -> bool {
    args.iter().any(|a| a == "-h" || a == "--help")
}

/// Returns true if `-V` or `--version` appears anywhere in the argument list.
fn wants_version(args: &[String]) -> bool {
    args.iter().any(|a| a == "-V" || a == "--version")
}

/// Returns true if `--print-capabilities` appears anywhere in the argument list.
fn wants_capabilities(args: &[String]) -> bool {
    args.iter().any(|a| a == "--print-capabilities")
}

/// Full `--help` reference text, categorized by section. Printed to stdout
/// with exit code 0 — this is a successful, requested action, not an error.
///
/// `{CAPABILITIES}` is substituted by `help_text()` from the capability tables
/// rather than written out here. The list used to be literal, and it was wrong:
/// it never gained `iconv`, so `--help` advertised a smaller compiler than the
/// one the user had, while the typo error beside it — derived from the table —
/// listed it correctly. Print this const directly and that comes back.
pub(crate) const HELP: &str = concat!("Usage: elephc [OPTIONS] <source-file>

A PHP-to-native AOT compiler
Version: ", env!("CARGO_PKG_VERSION"), "

Arguments:
  <source-file>           Tagged .php or tagless .lfc source file to compile

Subcommands:
  native <COMMAND>        Native dependency management (see `elephc native --help`)
  monitor <TARGET>        Profile a program built with --with-monitoring: a .php
                          source, a binary, or a running service's address
                          (see `elephc monitor --help`)

Modes:
  --web                   Compile as a prefork HTTP server
  --web-isolation MODE    worker (default) | pool | request; requires --web
  --strict-php            Reject elephc extensions in tagged PHP source; .lfc remains extension-enabled
  --strict-locals         Make an incompatible local retype (e.g. int then string) a compile error instead of a warning

Output modes:
  --check                 Type-check only, no codegen (mutually exclusive with --emit-ir/--emit-asm)
  --emit-ir               Emit EIR text instead of compiling
  --emit-asm              Emit assembly (.s) instead of linking
  --emit KIND             Output kind: executable (default) | cdylib | staticlib
                          Aliases: exe/bin | dylib/shared | static/lib (`lib` is staticlib)

Target:
  --target TARGET         macos-aarch64 | ios-arm64 | ios-sim-arm64 |
                          linux-aarch64 | linux-x86_64 (default: host)
  --php-version VERSION   8.0 through 8.6 (detected; fallback: 8.5)

Codegen:
  --heap-size=BYTES       Fixed heap size in bytes (default: 8388608)
  --null-repr=MODE        tagged (default) | sentinel
  --regalloc=MODE         linear (default) | stack
  --ir-opt=on|off         EIR optimization passes (default: on; --no-ir-opt is an alias for --ir-opt=off)
  --gc-stats              Print GC statistics at exit
  --counters              Embed per-function call counters (BSS) and print exact call
                          counts to stderr at exit
  --with-monitoring       Embed the profiling capability, dormant until `elephc monitor`
                          asks. Local/--exact captures report exact wall time,
                          allocations, retained objects, DB queries/wait, stream
                          operations and calls, rooted at {main}. A service's
                          default answer is sampled CPU time. Inlined functions
                          fold into their caller (as with --counters).
  --with-monitoring=NAMES Embed it for the named functions only (comma list; trailing
                          `*` matches by prefix; name `{main}` for the top-level root;
                          or use @file with one name per line)
  --heap-debug            Enable heap debug instrumentation
  --define SYMBOL         Define a symbol for `ifdef` conditional compilation
  --ini KEY=VALUE         Bake an INI directive override (repeatable; opcache.* honored)
  --strict-opcache        Throw when opcache_invalidate() targets AOT-frozen code

Linking:
  --link LIB, -l LIB      Extra library to link
  --link-path DIR, -L DIR Extra library search path
  --framework NAME        macOS framework to link
  --with-NAME             Force an optional capability ({CAPABILITIES})

Diagnostics:
  --timings               Show a per-phase timing table on stderr
  --quiet, -q             Disable progress lines and colorized output
  --source-map            Emit a .map source map alongside the assembly
  --debug-info            Embed DWARF line info for debuggers
  --keep-symbols          Keep the symbol table (stripped by default; for profilers)

Other:
  -h, --help              Print this help and exit
  -V, --version           Print version and exit
  --print-capabilities    List the optional capabilities this binary can link, and
                          the bridge archives each needs, as tab-separated lines
  --mascotte              Print an ASCII mascot and a random quote before output
");

/// Configuration derived from command-line arguments, passed to the compile pipeline.
/// Controls heap allocation size, debug output, code generation options, and linking behavior.
pub(crate) struct CliConfig {
    pub(crate) filename: String,
    pub(crate) heap_size: usize,
    pub(crate) gc_stats: bool,
    /// Embed per-function call counters and print exact counts to stderr at exit.
    pub(crate) counters: bool,
    /// Embed exact per-function instrumentation (enter/exit timing + edges);
    /// prints an exact profile to stderr at exit. Inlined functions fold into
    /// their caller, exactly as with `--counters`.
    pub(crate) instrument: crate::codegen::Instrumentation,
    pub(crate) heap_debug: bool,
    /// Opt-in: make the one documented OPcache divergence (D5) LOUD instead of silent.
    ///
    /// Code compiled into the binary cannot be evicted, so `opcache_invalidate()` on a
    /// manifest member can never do what the caller is asking for. Off (the default) it
    /// reports success exactly as reference PHP does; on, it throws so a program that
    /// RELIES on invalidation fails loudly rather than silently running stale code.
    pub(crate) strict_opcache: bool,
    pub(crate) emit_ir: bool,
    pub(crate) null_repr: crate::codegen::NullRepr,
    pub(crate) emit_asm: bool,
    pub(crate) emit: Emit,
    pub(crate) check_only: bool,
    pub(crate) emit_timings: bool,
    pub(crate) emit_source_map: bool,
    pub(crate) emit_debug_info: bool,
    /// Keep the symbol table in the linked executable; it is stripped by default.
    pub(crate) keep_symbols: bool,
    pub(crate) regalloc_linear: bool,
    pub(crate) ir_opt: bool,
    pub(crate) target: Target,
    /// PHP compatibility profile used by version-dependent language/runtime
    /// surfaces. Session behavior under `--web` currently consumes it.
    pub(crate) php_version: crate::web_prelude::PhpVersion,
    /// Where [`Self::php_version`] came from, so the compiler can distinguish a profile the
    /// user chose from one it assumed. Reported by `php_profile::report`.
    pub(crate) php_version_provenance: crate::php_profile::Provenance,
    pub(crate) extra_link_libs: Vec<String>,
    pub(crate) extra_link_paths: Vec<String>,
    pub(crate) extra_frameworks: Vec<String>,
    pub(crate) defines: HashSet<String>,
    /// Accept only PHP-compatible constructs: elephc extensions (`ptr`, `buffer<T>`,
    /// `packed class`, `extern`, `ifdef`, extension builtins) become compile errors.
    pub(crate) strict_php: bool,
    /// Make an incompatible local retype (e.g. a variable assigned `int` then
    /// later `string`) a compile error instead of a warning.
    pub(crate) strict_locals: bool,
    pub(crate) web: bool,
    /// Process-isolation architecture baked into a `--web` executable.
    pub(crate) web_isolation: WebIsolation,
    /// Optional capabilities the user force-enabled with `--with-<name>` (short
    /// names such as `"pdo"` or `"regex"`). Bridge names force-link their
    /// staticlib; runtime capabilities enable their helper/native requirements.
    /// Crates with a PHP-surface prelude also force that prelude's injection.
    /// `--with-web` is folded into `web` instead, since it aliases `--web`.
    pub(crate) with_crates: HashSet<String>,
    /// Suppresses live/completed progress and bridge-library "Linking" event lines,
    /// forcing plain output regardless of whether stderr is a terminal.
    /// Errors, warnings, and the final success line are unaffected.
    pub(crate) quiet: bool,
    /// Compile-time INI directive overrides from repeated `--ini <key>=<value>` flags, in
    /// the order supplied (last-wins per key is resolved downstream). For this increment only
    /// `opcache.*` keys are meaningful — they are baked into the OPcache configuration surface
    /// (`opcache_get_configuration`/`opcache_get_status`/`ini_get`/enabled-state). A non-opcache
    /// key is stored but ignored by the opcache layer (general INI is a future increment); it is
    /// never an error so a forward-looking `--ini` invocation does not break.
    pub(crate) ini_overrides: Vec<(String, String)>,
}

/// A fully parsed top-level invocation of either the compiler or native package manager.
pub(crate) enum Command {
    /// The existing PHP compilation command and all of its normalized options.
    Compile(CliConfig),
    /// One validated `elephc native` subcommand.
    Native(NativeCommand),
    /// One validated `elephc monitor` sampling invocation.
    Monitor(crate::monitor::MonitorCommand),
}

/// Parses the exact top-level `native` selector before falling back to legacy compilation.
pub(crate) fn parse_args(args: &[String]) -> Command {
    if args.get(1).map(String::as_str) == Some("monitor") {
        return match crate::monitor::parse_monitor_args(&args[2..]) {
            Ok(command) => Command::Monitor(command),
            Err(error) => {
                eprintln!("{error}\n\n{}", crate::monitor::MONITOR_USAGE);
                process::exit(1);
            }
        };
    }
    if args.get(1).map(String::as_str) != Some("native") {
        return Command::Compile(parse_compile_args(args));
    }

    match parse_native_args(&args[2..]) {
        Ok(NativeParseOutcome::Command(command)) => Command::Native(command),
        Ok(NativeParseOutcome::Help(help)) => {
            print!("{help}");
            process::exit(0);
        }
        Err(error) => {
            eprintln!("{error}\n\n{}", native_help());
            process::exit(1);
        }
    }
}

/// Parses legacy compilation arguments into a normalized configuration.
fn parse_compile_args(args: &[String]) -> CliConfig {
    if args.len() < 2 {
        fail("no source file given");
    }
    if wants_help(args) {
        println!("{}", help_text());
        process::exit(0);
    }
    if wants_version(args) {
        println!("elephc {VERSION}");
        process::exit(0);
    }
    if wants_capabilities(args) {
        print!("{}", capability_report());
        process::exit(0);
    }

    let mut heap_size: usize = 8_388_608; // 8MB default
    let mut gc_stats = false;
    let mut counters = false;
    let mut instrument = crate::codegen::Instrumentation::Off;
    let mut heap_debug = false;
    let mut strict_opcache = false;
    let mut emit_ir = false;
    let mut emit_asm = false;
    let mut emit = Emit::Executable;
    let mut check_only = false;
    let mut emit_timings = false;
    let mut emit_source_map = false;
    let mut emit_debug_info = false;
    let mut keep_symbols = false;
    let mut filename_arg = None;
    let mut target = Target::detect_host();
    let mut php_version = crate::web_prelude::PhpVersion::default();
    let mut php_version_provenance = crate::php_profile::Provenance::Default;
    let mut extra_link_libs: Vec<String> = Vec::new();
    let mut extra_link_paths: Vec<String> = Vec::new();
    let mut extra_frameworks: Vec<String> = Vec::new();
    let mut defines: HashSet<String> = HashSet::new();
    let mut strict_php = false;
    let mut strict_locals = false;
    let mut web = false;
    let mut web_isolation = WebIsolation::default();
    let mut web_isolation_explicit = false;
    let mut quiet = false;
    let mut with_crates: HashSet<String> = HashSet::new();
    let mut ini_overrides: Vec<(String, String)> = Vec::new();
    let mut null_repr = match std::env::var("ELEPHC_NULL_REPR").as_deref() {
        Ok("tagged") => crate::codegen::NullRepr::Tagged,
        Ok("sentinel") => crate::codegen::NullRepr::Sentinel,
        _ => crate::codegen::NullRepr::default(),
    };
    // The register allocator is on by default; an env override lets the test
    // harness compile the whole suite under the stack fallback for comparison.
    let mut regalloc_linear = match std::env::var("ELEPHC_REGALLOC").as_deref() {
        Ok("stack") => false,
        Ok("linear") => true,
        _ => true,
    };
    // EIR optimization passes are on by default; an env override lets the test
    // harness or a benchmark compile with the IR pass driver disabled.
    let mut ir_opt = match std::env::var("ELEPHC_IR_OPT").as_deref() {
        Ok("off") => false,
        Ok("on") => true,
        _ => true,
    };

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if let Some(val) = arg.strip_prefix("--heap-size=") {
            heap_size = parse_heap_size(val);
        } else if arg == "--target" {
            i += 1;
            target = parse_required_target(args, i);
        } else if let Some(value) = arg.strip_prefix("--target=") {
            target = parse_target(value);
        } else if arg == "--php-version" {
            i += 1;
            php_version = parse_required_php_version(args, i);
            php_version_provenance = crate::php_profile::Provenance::Flag;
        } else if let Some(value) = arg.strip_prefix("--php-version=") {
            php_version = parse_php_version(value);
            php_version_provenance = crate::php_profile::Provenance::Flag;
        } else if arg == "--gc-stats" {
            gc_stats = true;
        } else if arg == "--counters" {
            counters = true;
        } else if let Some(spec) = arg.strip_prefix("--with-monitoring=") {
            // Selective instrumentation: exactness where it was asked for, full
            // speed everywhere else. `@file` reads one name per line, because a
            // useful set outgrows a command line quickly — and a set produced by
            // a previous sampled run is exactly how you would build one.
            let names = match spec.strip_prefix('@') {
                Some(path) => match std::fs::read_to_string(path) {
                    Ok(text) => text
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty() && !line.starts_with('#'))
                        .map(str::to_string)
                        .collect::<Vec<_>>(),
                    Err(error) => fail(&format!("--with-monitoring=@{path}: {error}")),
                },
                None => spec
                    .split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>(),
            };
            if names.is_empty() {
                fail("--with-monitoring=<names> needs at least one function name");
            }
            instrument = crate::codegen::Instrumentation::Only(names);
            with_crates.extend(MONITORING_BRIDGES.iter().map(|s| s.to_string()));
        } else if arg == "--heap-debug" {
            heap_debug = true;
        } else if arg == "--strict-opcache" {
            strict_opcache = true;
        } else if arg == "--emit-ir" {
            emit_ir = true;
        } else if arg == "--emit-asm" {
            emit_asm = true;
        } else if arg == "--emit" {
            i += 1;
            emit = parse_required_emit(args, i);
        } else if let Some(value) = arg.strip_prefix("--emit=") {
            emit = parse_emit(value);
        } else if arg == "--check" {
            check_only = true;
        } else if arg == "--timings" {
            emit_timings = true;
        } else if arg == "--source-map" {
            emit_source_map = true;
        } else if arg == "--debug-info" {
            emit_debug_info = true;
        } else if arg == "--keep-symbols" {
            keep_symbols = true;
        } else if arg == "--quiet" || arg == "-q" {
            quiet = true;
        } else if arg == "--mascotte" {
            // Already handled in main() before parse_args ran (so the banner
            // prints before --help/errors/compilation); recognized here only
            // so it isn't mistaken for an unknown flag.
        } else if let Some(value) = arg.strip_prefix("--null-repr=") {
            null_repr = parse_null_repr(value);
        } else if let Some(value) = arg.strip_prefix("--regalloc=") {
            regalloc_linear = parse_regalloc(value);
        } else if let Some(value) = arg.strip_prefix("--ir-opt=") {
            ir_opt = parse_ir_opt(value);
        } else if arg == "--no-ir-opt" {
            ir_opt = false;
        } else if arg == "--define" {
            i += 1;
            let symbol = required_value(args, i, "Missing symbol after --define");
            if let Err(message) = validate_define_symbol(&symbol) {
                fail(message);
            }
            defines.insert(symbol);
        } else if let Some(symbol) = arg.strip_prefix("--define=") {
            if let Err(message) = validate_define_symbol(symbol) {
                fail(message);
            }
            defines.insert(symbol.to_string());
        } else if arg == "--ini" {
            i += 1;
            let assignment = required_value(args, i, "Missing key=value after --ini");
            match parse_ini_assignment(&assignment) {
                Ok(entry) => ini_overrides.push(entry),
                Err(message) => fail(&message),
            }
        } else if let Some(assignment) = arg.strip_prefix("--ini=") {
            match parse_ini_assignment(assignment) {
                Ok(entry) => ini_overrides.push(entry),
                Err(message) => fail(&message),
            }
        } else if arg == "--link" || arg == "-l" {
            i += 1;
            extra_link_libs.push(required_value(
                args,
                i,
                &format!("Missing library name after {}", arg),
            ));
        } else if let Some(lib) = arg.strip_prefix("-l") {
            extra_link_libs.push(lib.to_string());
        } else if arg == "--link-path" || arg == "-L" {
            i += 1;
            extra_link_paths.push(required_value(args, i, &format!("Missing path after {}", arg)));
        } else if let Some(path) = arg.strip_prefix("-L") {
            extra_link_paths.push(path.to_string());
        } else if arg == "--framework" {
            i += 1;
            extra_frameworks.push(required_value(
                args,
                i,
                "Missing framework name after --framework",
            ));
        } else if arg == "--strict-php" {
            strict_php = true;
        } else if arg == "--strict-locals" {
            strict_locals = true;
        } else if arg == "--web" {
            web = true;
        } else if arg == "--web-isolation" {
            i += 1;
            web_isolation = parse_web_isolation(&required_value(
                args,
                i,
                "Missing mode after --web-isolation (expected: worker, pool, request)",
            ));
            web_isolation_explicit = true;
        } else if let Some(value) = arg.strip_prefix("--web-isolation=") {
            web_isolation = parse_web_isolation(value);
            web_isolation_explicit = true;
        } else if let Some(name) = arg.strip_prefix("--with-") {
            // `--with-web` aliases the full `--web` mode (it owns the program
            // entry point); every other known bridge or runtime capability is
            // recorded for pipeline forcing. An unknown name is a hard error
            // so a typo never silently no-ops.
            if name == "web" {
                web = true;
            } else if name == "monitoring" {
                // One flag for the whole capability. `--probe` and `--instrument`
                // used to be two ways to ask a related question, and the answer to
                // "which one do I want" was always "it depends where I am running"
                // — which is exactly the distinction this removes. The binary
                // carries both mechanisms and stays dormant; `monitor` decides at
                // run time what to collect.
                instrument = crate::codegen::Instrumentation::All;
                with_crates.extend(MONITORING_BRIDGES.iter().map(|s| s.to_string()));
            } else if with_flag_is_known(name) {
                with_crates.insert(name.to_string());
            } else {
                fail(&format!(
                    "Unknown capability for --with-{}: expected one of: {}",
                    name,
                    with_flag_names().join(", ")
                ));
            }
        } else if arg.starts_with("--") {
            fail(&format!("Unknown flag: {}", arg));
        } else {
            filename_arg = Some(arg.clone());
        }
        i += 1;
    }

    let filename = match filename_arg {
        Some(filename) => filename,
        None => fail("no source file given"),
    };
    let output_modes = usize::from(emit_ir) + usize::from(emit_asm) + usize::from(check_only);
    if output_modes > 1 {
        fail("--emit-ir, --emit-asm, and --check are mutually exclusive");
    }
    if let Err(message) = validate_target_output(target, emit, check_only, emit_ir) {
        fail(message);
    }
    if web && check_only {
        fail("--web cannot be combined with --check");
    }
    // --web restructures the process entry point, which a library artifact does
    // not have: both library kinds are incompatible with it for the same reason.
    if web && emit.is_library() {
        fail("--web cannot be combined with a library --emit kind (cdylib, staticlib)");
    }
    // A library has no `main`, and `main` is where the profiling runtimes are
    // initialized. Accepting this produced a library carrying an enter/exit hook
    // at every call site with nothing able to arm them — the cost of the
    // capability without the capability. Turning a library on would need an
    // initialization ABI the host calls, which does not exist.
    if emit.is_library() && !matches!(instrument, crate::codegen::Instrumentation::Off) {
        fail(
            "--with-monitoring cannot be combined with a library --emit kind: a library has no \
             main, so the profiling runtime is never initialized and the hooks it \
             embeds can never be activated",
        );
    }
    if web && emit_asm {
        fail("--web cannot be combined with --emit-asm");
    }
    if web && emit_ir {
        fail("--web cannot be combined with --emit-ir");
    }
    if web_isolation_explicit && !web {
        fail("--web-isolation requires --web (or --with-web)");
    }

    // With no explicit `--php-version`, take the profile the project already declares. Every
    // source is optional at every level, so a lone `.php` file still resolves to the default
    // without needing a manifest — see `php_profile::resolve`.
    if php_version_provenance == crate::php_profile::Provenance::Default {
        let resolved = crate::php_profile::resolve::resolve(std::path::Path::new(&filename));
        php_version = resolved.profile;
        php_version_provenance = resolved.provenance;
        // Emitted here rather than carried into the config: these report how the ANSWER was
        // reached (a clamped pin, an unreadable manifest), so they belong with the decision
        // and are silent whenever it was unambiguous.
        for note in resolved.notes {
            eprintln!("  note: {note}");
        }
    }

    CliConfig {
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
    }
}

/// Returns whether a `--with-<name>` suffix selects a bridge or runtime capability.
///
/// The monitoring bridges are excluded: they are how `--with-monitoring` is
/// built, not something to ask for by mechanism.
fn with_flag_is_known(name: &str) -> bool {
    if MONITORING_BRIDGES.contains(&name) {
        return false;
    }
    crate::linker::bridge_lib_for_flag(name).is_some()
        || RUNTIME_CAPABILITY_FLAGS.contains(&name)
}

/// Returns accepted `--with-<name>` suffixes in stable help/error order.
fn with_flag_names() -> Vec<&'static str> {
    crate::linker::crate_flag_names()
        .into_iter()
        .filter(|name| !MONITORING_BRIDGES.contains(name))
        .chain(RUNTIME_CAPABILITY_FLAGS.iter().copied())
        .chain(std::iter::once("monitoring"))
        .collect()
}

/// Renders `HELP` with the accepted `--with-<name>` list substituted in.
pub(crate) fn help_text() -> String {
    HELP.replace("{CAPABILITIES}", &with_flag_names().join(", "))
}

/// Reports every optional capability this binary accepts, and the bridge
/// archives each one needs, as tab-separated lines on stdout.
///
/// This exists so an INSTALLED compiler can be asked what it can do, rather
/// than the question being answered by a list kept somewhere else. A bridge is
/// resolved from the directory the binary lives in (or its sibling `lib/`), so
/// a compiler can advertise a capability whose archive was never packed beside
/// it — which is how released tarballs shipped without `libelephc_magician.a`
/// from 0.26.3 to 0.26.5, refusing `--with-eval` and any `eval()` the compiler
/// could not fold. Nothing inside a checkout can see that, because `target/`
/// always holds every archive; only the shipped artifact is short.
///
/// The release probe unpacks a tarball, asks the binary inside it this
/// question, and holds it to the answer. Every field is a projection of
/// `BRIDGES`, `RUNTIME_CAPABILITY_FLAGS` and `MONITORING_BRIDGES`, so a
/// thirteenth bridge is carried into that check by the edit that declares it
/// and there is no second list for anyone to forget.
///
/// Each line is `<kind>\t<name>[\t<archive>...]`: `bridge` for a capability
/// backed by one archive of its own, `capability` for one that is not — either
/// because it needs no archive (`regex`, whose provider is a managed native
/// package) or because it is built out of several (`monitoring`). A capability
/// listed with no archive is still a capability the binary must be able to
/// link; the archives are what can be checked without compiling anything.
pub(crate) fn capability_report() -> String {
    let mut report = String::new();
    for name in with_flag_names() {
        if let Some(archive) = crate::linker::archive_filename_for_flag(name) {
            report.push_str(&format!("bridge\t{name}\t{archive}\n"));
            continue;
        }
        report.push_str(&format!("capability\t{name}"));
        if name == "monitoring" {
            for mechanism in MONITORING_BRIDGES {
                if let Some(archive) = crate::linker::archive_filename_for_flag(mechanism) {
                    report.push_str(&format!("\t{archive}"));
                }
            }
        }
        report.push('\n');
    }
    report
}

/// Parses a single `--ini KEY=VALUE` assignment into a `(key, value)` pair, splitting on the
/// FIRST `=` so an INI value that itself contains `=` is preserved intact. The key is trimmed
/// (a stray CLI space is not part of a directive name) and must be non-empty; the value is taken
/// verbatim (INI values are used exactly as supplied — e.g. `opcache.memory_consumption=256`
/// must report `"256"`). Kept pure (no IO/exit) so the split rule is unit-testable.
fn parse_ini_assignment(assignment: &str) -> Result<(String, String), String> {
    let (key, value) = assignment
        .split_once('=')
        .ok_or_else(|| format!("Invalid --ini '{assignment}': expected KEY=VALUE"))?;
    let key = key.trim();
    if key.is_empty() {
        return Err(format!("Invalid --ini '{assignment}': empty key"));
    }
    Ok((key.to_string(), value.to_string()))
}

/// Parses the required PHP compatibility version after `--php-version`.
fn parse_required_php_version(
    args: &[String],
    index: usize,
) -> crate::web_prelude::PhpVersion {
    if index < args.len() {
        parse_php_version(&args[index])
    } else {
        fail("Missing version after --php-version (expected 8.0 through 8.6)")
    }
}

/// Parses a supported PHP compatibility version or exits with a focused diagnostic.
fn parse_php_version(value: &str) -> crate::web_prelude::PhpVersion {
    crate::web_prelude::PhpVersion::parse(value).unwrap_or_else(|| {
        fail(&format!(
            "Unsupported PHP version '{}': expected one of: {}",
            value,
            crate::web_prelude::PhpVersion::accepted_values()
        ))
    })
}

/// Parse the required emit-kind argument at the given index, or fail if missing.
fn parse_required_emit(args: &[String], index: usize) -> Emit {
    if index < args.len() {
        parse_emit(&args[index])
    } else {
        fail("Missing emit kind after --emit (expected: executable, cdylib, staticlib)")
    }
}

/// Parse an emit-kind string into an `Emit` value, or fail with an error message.
fn parse_emit(value: &str) -> Emit {
    match value {
        "executable" | "exe" | "bin" => Emit::Executable,
        "cdylib" | "dylib" | "shared" => Emit::Cdylib,
        "staticlib" | "static" | "lib" => Emit::Staticlib,
        other => fail(&format!(
            "Invalid --emit kind '{}': expected one of: executable, cdylib, staticlib",
            other
        )),
    }
}

/// Parse a heap size value, returning a value >= 65536 or exit with an error.
fn parse_heap_size(value: &str) -> usize {
    match value.parse::<usize>() {
        Ok(n) if n >= 65536 => n,
        _ => fail("Invalid --heap-size: must be a number >= 65536"),
    }
}

/// Parse the required target argument at the given index, or fail if missing.
fn parse_required_target(args: &[String], index: usize) -> Target {
    if index < args.len() {
        parse_target(&args[index])
    } else {
        fail("Missing target after --target")
    }
}

/// Parse a `--null-repr=` value into a NullRepr, or fail with an error message.
fn parse_null_repr(value: &str) -> crate::codegen::NullRepr {
    match value {
        "sentinel" => crate::codegen::NullRepr::Sentinel,
        "tagged" => crate::codegen::NullRepr::Tagged,
        other => fail(&format!("Unknown null representation: {}", other)),
    }
}

/// Parse a `--regalloc=` value into the linear-scan toggle, or fail.
fn parse_regalloc(value: &str) -> bool {
    match value {
        "linear" => true,
        "stack" => false,
        other => fail(&format!("Unknown register allocator: {}", other)),
    }
}

/// Parse an `--ir-opt=` value into the EIR optimization-pass toggle, or fail.
fn parse_ir_opt(value: &str) -> bool {
    match value {
        "on" => true,
        "off" => false,
        other => fail(&format!("Unknown --ir-opt value: {} (expected on|off)", other)),
    }
}

/// Parses the compile-time web process-isolation model.
fn parse_web_isolation(value: &str) -> WebIsolation {
    match value {
        "worker" => WebIsolation::Worker,
        "pool" => WebIsolation::Pool,
        "request" => WebIsolation::Request,
        other => fail(&format!(
            "Unknown --web-isolation value: {} (expected worker|pool|request)",
            other
        )),
    }
}

/// Parse a target string to a Target enum, or fail with an error message.
fn parse_target(value: &str) -> Target {
    match Target::parse(value) {
        Ok(target) => target,
        Err(err) => fail(&err),
    }
}

/// Validates artifact kinds that depend on the selected target.
///
/// iOS source can still be checked or lowered to EIR with the default emit kind,
/// but actual code generation must use a library boundary: an Elephc executable
/// is a CLI process, not a complete signed iOS application bundle.
fn validate_target_output(
    target: Target,
    emit: Emit,
    check_only: bool,
    emit_ir: bool,
) -> Result<(), &'static str> {
    if target.is_ios() && emit == Emit::Executable && !check_only && !emit_ir {
        return Err(
            "iOS targets do not emit standalone executables; use --emit staticlib (or --emit \
             cdylib) and link the library into an iOS app host",
        );
    }
    Ok(())
}

/// Retrieve a required argument at index, or fail with the given message.
fn required_value(args: &[String], index: usize, message: &str) -> String {
    if index < args.len() {
        args[index].clone()
    } else {
        fail(message)
    }
}

/// Validates an ifdef symbol supplied via `--define`, rejecting an empty symbol.
///
/// Kept pure (no IO/exit) so both the `--define SYMBOL` and `--define=SYMBOL` forms
/// can share one consistent rule and the rejection can be unit-tested.
fn validate_define_symbol(symbol: &str) -> Result<(), &'static str> {
    if symbol.is_empty() {
        return Err("Invalid --define: symbol cannot be empty");
    }
    Ok(())
}

/// Builds the full parameter-error text: an `error: ` prefix, the short usage
/// line, and a hint to run `--help` for the full reference. Kept pure (no
/// IO/exit) so the format can be unit-tested without spawning a process.
fn format_fail_message(message: &str) -> String {
    format!(
        "error: {}\n\n{}\n\nRun 'elephc --help' for more information.",
        message, USAGE
    )
}

/// Prints a formatted parameter-error message to stderr and exits the
/// process with code 1. Never returns.
fn fail(message: &str) -> ! {
    eprintln!("{}", format_fail_message(message));
    process::exit(1);
}

#[cfg(test)]
mod tests {
    /// `--with-<name>` must not offer the two mechanism names by another door.
    ///
    /// `instrument` and `probe` are ordinary bridges, and the accepted set is
    /// derived from the bridge table — so without an explicit exclusion the CLI
    /// keeps accepting `--with-instrument` and, worse, *advertises* both names in
    /// the error text every user sees after a typo. That is the surface the
    /// single `--with-monitoring` flag exists to replace, reachable by a route
    /// nobody thought to check.
    #[test]
    fn the_with_surface_offers_the_capability_not_its_mechanisms() {
        let names = super::with_flag_names();
        for hidden in super::MONITORING_BRIDGES {
            assert!(
                !names.contains(&hidden),
                "--with-{hidden} is still offered; the accepted set is what the \
                 error text advertises"
            );
            assert!(
                !super::with_flag_is_known(hidden),
                "--with-{hidden} is still accepted"
            );
        }
        assert!(
            names.contains(&"monitoring"),
            "the capability that replaced them must be listed"
        );
        // The exclusion must be surgical: every other bridge stays offered.
        assert!(names.contains(&"pdo") && names.contains(&"tls"));
    }

    use super::*;
    use crate::codegen::platform::{AppleVariant, Arch, Platform};

    /// Extracts the compile configuration returned for a legacy invocation.
    fn compile_config(args: &[String]) -> CliConfig {
        let Command::Compile(config) = parse_args(args) else {
            panic!("expected compile command");
        };
        config
    }

    /// Verifies the symbol table is stripped unless the invocation asks to keep it.
    ///
    /// The default is the load-bearing part: stripping removes about a quarter of every linked
    /// executable, so a regression that silently flipped this back would cost that on every build
    /// while breaking nothing a test would otherwise notice.
    #[test]
    fn symbols_are_stripped_unless_kept() {
        let default = compile_config(&["elephc".to_string(), "app.php".to_string()]);
        assert!(!default.keep_symbols, "stripping is the default");

        let kept = compile_config(&[
            "elephc".to_string(),
            "--keep-symbols".to_string(),
            "app.php".to_string(),
        ]);
        assert!(kept.keep_symbols, "--keep-symbols must keep the symbol table");
    }

    /// Verifies `--debug-info` and `--keep-symbols` are independent flags.
    ///
    /// They are consumed together at link time — either one keeps the names — but each must parse
    /// on its own, so that reading one out of the config cannot be mistaken for the other.
    #[test]
    fn debug_info_and_keep_symbols_parse_independently() {
        let debug = compile_config(&[
            "elephc".to_string(),
            "--debug-info".to_string(),
            "app.php".to_string(),
        ]);
        assert!(debug.emit_debug_info);
        assert!(!debug.keep_symbols, "--debug-info is not --keep-symbols");

        let both = compile_config(&[
            "elephc".to_string(),
            "--debug-info".to_string(),
            "--keep-symbols".to_string(),
            "app.php".to_string(),
        ]);
        assert!(both.emit_debug_info && both.keep_symbols);
    }

    /// Verifies `--keep-symbols` appears in the help text.
    ///
    /// `docs/compiling/cli-reference.md` is authoritative and must stay in sync with this file; a
    /// flag missing from `--help` is the first way those two drift apart.
    #[test]
    fn keep_symbols_is_documented_in_help() {
        assert!(HELP.contains("--keep-symbols"));
    }

    /// Verifies an empty `--define` symbol is rejected, matching the `--define=` form,
    /// so the two spellings no longer behave inconsistently.
    #[test]
    fn empty_define_symbol_is_rejected() {
        assert!(validate_define_symbol("").is_err());
    }

    /// Verifies a normal `--define` symbol is accepted.
    #[test]
    fn non_empty_define_symbol_is_accepted() {
        assert!(validate_define_symbol("FEATURE").is_ok());
    }

    /// Verifies the canonical `--emit` spellings parse to the expected `Emit` variants.
    #[test]
    fn emit_kind_parses_canonical_spellings() {
        assert_eq!(parse_emit("executable"), Emit::Executable);
        assert_eq!(parse_emit("cdylib"), Emit::Cdylib);
        assert_eq!(parse_emit("staticlib"), Emit::Staticlib);
    }

    /// Verifies the accepted aliases map to their canonical variants so users coming
    /// from cargo (`cdylib`/`dylib`) and unix toolchains (`shared`, `bin`) all work.
    #[test]
    fn emit_kind_accepts_aliases() {
        assert_eq!(parse_emit("exe"), Emit::Executable);
        assert_eq!(parse_emit("bin"), Emit::Executable);
        assert_eq!(parse_emit("dylib"), Emit::Cdylib);
        assert_eq!(parse_emit("shared"), Emit::Cdylib);
        assert_eq!(parse_emit("static"), Emit::Staticlib);
        assert_eq!(parse_emit("lib"), Emit::Staticlib);
    }

    /// Verifies `lib` is advertised as a static-library alias rather than being
    /// mistaken for the dynamic-library family.
    #[test]
    fn help_identifies_lib_as_a_staticlib_alias() {
        assert!(HELP.contains("static/lib (`lib` is staticlib)"));
    }

    /// Verifies iOS code generation produces host-consumable libraries rather
    /// than a CLI executable that cannot be installed as an iOS application.
    #[test]
    fn ios_rejects_executable_artifacts_but_allows_analysis_and_libraries() {
        for target in [
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
        ] {
            assert!(validate_target_output(target, Emit::Executable, false, false).is_err());
            assert!(validate_target_output(target, Emit::Staticlib, false, false).is_ok());
            assert!(validate_target_output(target, Emit::Cdylib, false, false).is_ok());
            assert!(validate_target_output(target, Emit::Executable, true, false).is_ok());
            assert!(validate_target_output(target, Emit::Executable, false, true).is_ok());
        }
        assert!(
            validate_target_output(
                Target::new(Platform::MacOS, Arch::AArch64),
                Emit::Executable,
                false,
                false,
            )
            .is_ok()
        );
    }

    /// Verifies the canonical `--ir-opt=` spellings toggle the EIR optimization
    /// pass driver, with `on` enabling it and `off` disabling it.
    #[test]
    fn ir_opt_parses_on_and_off() {
        assert!(parse_ir_opt("on"));
        assert!(!parse_ir_opt("off"));
    }

    /// Verifies the register-allocator toggle parses its canonical spellings.
    #[test]
    fn regalloc_parses_linear_and_stack() {
        assert!(parse_regalloc("linear"));
        assert!(!parse_regalloc("stack"));
    }

    /// Verifies `--web` sets the web flag on the parsed config.
    #[test]
    fn web_flag_sets_web() {
        let args = vec!["elephc".into(), "--web".into(), "app.php".into()];
        let config = compile_config(&args);
        assert!(config.web);
        assert_eq!(config.web_isolation, WebIsolation::Worker);
    }

    /// Verifies all explicit web-isolation spellings select their compile-time model.
    #[test]
    fn web_isolation_parses_all_modes() {
        for (value, expected) in [
            ("worker", WebIsolation::Worker),
            ("pool", WebIsolation::Pool),
            ("request", WebIsolation::Request),
        ] {
            let args = vec![
                "elephc".into(),
                "--web".into(),
                format!("--web-isolation={value}"),
                "app.php".into(),
            ];
            assert_eq!(compile_config(&args).web_isolation, expected);
        }
    }

    /// Verifies the split spelling selects the same mode as the equals spelling.
    #[test]
    fn web_isolation_accepts_split_form() {
        let args = vec![
            "elephc".into(),
            "--web".into(),
            "--web-isolation".into(),
            "pool".into(),
            "app.php".into(),
        ];
        assert_eq!(compile_config(&args).web_isolation, WebIsolation::Pool);
    }

    /// Verifies the absence of `--web` leaves the web flag off.
    #[test]
    fn no_web_flag_defaults_off() {
        let args = vec!["elephc".into(), "app.php".into()];
        let config = compile_config(&args);
        assert!(!config.web);
        assert_eq!(config.web_isolation, WebIsolation::Worker);
    }

    /// Verifies every maintained PHP minor maps to its exact compatibility profile.
    #[test]
    fn maintained_php_versions_parse() {
        assert_eq!(parse_php_version("8.0").version_id(), 80000);
        assert_eq!(parse_php_version("8.1").version_id(), 80100);
        assert_eq!(parse_php_version("8.2").version_id(), 80200);
        assert_eq!(parse_php_version("8.3").version_id(), 80300);
        assert_eq!(parse_php_version("8.4").version_id(), 80400);
        assert_eq!(parse_php_version("8.5").version_id(), 80500);
        assert_eq!(parse_php_version("8.6").version_id(), 80600);
    }

    /// Verifies both CLI spellings store the selected PHP compatibility profile.
    #[test]
    fn php_version_flag_accepts_split_and_equals_forms() {
        let split = vec![
            "elephc".into(),
            "--php-version".into(),
            "8.3".into(),
            "app.php".into(),
        ];
        let equals = vec![
            "elephc".into(),
            "--php-version=8.4".into(),
            "app.php".into(),
        ];
        assert_eq!(compile_config(&split).php_version, crate::web_prelude::PhpVersion::Php83);
        assert_eq!(compile_config(&equals).php_version, crate::web_prelude::PhpVersion::Php84);
    }

    /// Verifies the compatibility profile defaults to the newest maintained PHP minor.
    #[test]
    fn php_version_defaults_to_85() {
        let args = vec!["elephc".into(), "app.php".into()];
        let config = compile_config(&args);
        assert_eq!(config.php_version, crate::web_prelude::PhpVersion::Php85);
    }

    /// Verifies `--with-pdo` records the crate for force-link/prelude forcing
    /// without touching the web mode.
    #[test]
    fn with_pdo_records_forced_crate() {
        let args = vec!["elephc".into(), "--with-pdo".into(), "app.php".into()];
        let config = compile_config(&args);
        assert!(config.with_crates.contains("pdo"));
        assert!(!config.web);
    }

    /// Verifies `--with-iconv` records the charset bridge for force-linking.
    #[test]
    fn with_iconv_records_forced_crate() {
        let args = vec!["elephc".into(), "--with-iconv".into(), "app.php".into()];
        let config = compile_config(&args);
        assert!(config.with_crates.contains("iconv"));
        assert!(!config.web);
    }

    /// Verifies `--with-regex` records the dynamic-code regex capability without web mode.
    #[test]
    fn with_regex_records_runtime_capability() {
        let args = vec![
            "elephc".into(),
            "--with-regex".into(),
            "app.php".into(),
        ];
        let config = compile_config(&args);
        assert!(config.with_crates.contains("regex"));
        assert!(!config.web);
    }

    /// Verifies `--with-mysqli` records the runtime capability that force-injects
    /// the mysqli prelude (which links the shared `elephc_pdo` archive), without
    /// touching web mode.
    #[test]
    fn with_mysqli_records_runtime_capability() {
        let args = vec![
            "elephc".into(),
            "--with-mysqli".into(),
            "app.php".into(),
        ];
        let config = compile_config(&args);
        assert!(config.with_crates.contains("mysqli"));
        assert!(!config.web);
    }

    /// Verifies multiple `--with-<name>` flags accumulate into the forced set.
    #[test]
    fn multiple_with_crates_accumulate() {
        let args = vec![
            "elephc".into(),
            "--with-pdo".into(),
            "--with-tls".into(),
            "app.php".into(),
        ];
        let config = compile_config(&args);
        assert!(config.with_crates.contains("pdo"));
        assert!(config.with_crates.contains("tls"));
    }

    /// Verifies `--with-web` aliases `--web` (full web mode) instead of being
    /// recorded as a plain force-link crate, since elephc_web owns the entry point.
    #[test]
    fn with_web_aliases_web_mode() {
        let args = vec!["elephc".into(), "--with-web".into(), "app.php".into()];
        let config = compile_config(&args);
        assert!(config.web);
        assert!(config.with_crates.is_empty());
    }

    /// Verifies the default has no forced crates so non-`--with` builds are unaffected.
    #[test]
    fn no_with_flag_defaults_empty() {
        let args = vec!["elephc".into(), "app.php".into()];
        let config = compile_config(&args);
        assert!(config.with_crates.is_empty());
    }

    /// Verifies `--ini KEY=VALUE` splits on the first `=`, trims the key, and preserves a value
    /// that itself contains `=`; an empty or `=`-less assignment is rejected.
    #[test]
    fn ini_assignment_splits_on_first_equals() {
        assert_eq!(
            parse_ini_assignment("opcache.enable_cli=1").unwrap(),
            ("opcache.enable_cli".to_string(), "1".to_string())
        );
        // Value keeps later `=` intact (split on FIRST `=`).
        assert_eq!(
            parse_ini_assignment("opcache.blacklist_filename=a=b").unwrap(),
            ("opcache.blacklist_filename".to_string(), "a=b".to_string())
        );
        // A stray space around the key is trimmed.
        assert_eq!(
            parse_ini_assignment(" opcache.jit =tracing").unwrap(),
            ("opcache.jit".to_string(), "tracing".to_string())
        );
        assert!(parse_ini_assignment("no_equals_here").is_err());
        assert!(parse_ini_assignment("=value").is_err());
    }

    /// Verifies repeated `--ini` flags (both split and `=` spellings) accumulate in order, and
    /// the default is an empty override list.
    #[test]
    fn ini_flags_accumulate_in_order() {
        let args = vec![
            "elephc".into(),
            "--ini".into(),
            "opcache.enable_cli=1".into(),
            "--ini=opcache.jit=tracing".into(),
            "app.php".into(),
        ];
        let config = compile_config(&args);
        assert_eq!(
            config.ini_overrides,
            vec![
                ("opcache.enable_cli".to_string(), "1".to_string()),
                ("opcache.jit".to_string(), "tracing".to_string()),
            ]
        );

        let no_ini = compile_config(&["elephc".into(), "app.php".into()]);
        assert!(no_ini.ini_overrides.is_empty());
    }

    /// Verifies `--strict-php` sets the strict-PHP flag on the parsed config.
    #[test]
    fn strict_php_flag_sets_strict() {
        let args = vec!["elephc".into(), "--strict-php".into(), "app.php".into()];
        let config = compile_config(&args);
        assert!(config.strict_php);
    }

    /// Verifies the absence of `--strict-php` leaves strict mode off.
    #[test]
    fn no_strict_php_flag_defaults_off() {
        let args = vec!["elephc".into(), "app.php".into()];
        let config = compile_config(&args);
        assert!(!config.strict_php);
    }

    /// Verifies strict mode and conditional symbols may coexist for mixed PHP/LFC projects.
    #[test]
    fn strict_php_with_define_is_accepted() {
        let args = vec![
            "elephc".into(),
            "--strict-php".into(),
            "--define".into(),
            "FEATURE".into(),
            "app.lfc".into(),
        ];
        let config = compile_config(&args);
        assert!(config.strict_php);
        assert!(config.defines.contains("FEATURE"));
    }

    /// Verifies `--strict-locals` sets the strict_locals flag on the parsed config.
    #[test]
    fn strict_locals_flag_sets_strict_locals() {
        let args = vec!["elephc".into(), "--strict-locals".into(), "app.php".into()];
        let config = compile_config(&args);
        assert!(config.strict_locals);
    }

    /// Verifies the absence of `--strict-locals` defaults to permissive local retyping.
    #[test]
    fn no_strict_locals_flag_defaults_off() {
        let args = vec!["elephc".into(), "app.php".into()];
        let config = compile_config(&args);
        assert!(!config.strict_locals);
    }

    /// Verifies `--quiet` sets the quiet flag.
    #[test]
    fn quiet_flag_sets_quiet() {
        let args = vec!["elephc".into(), "--quiet".into(), "app.php".into()];
        let config = compile_config(&args);
        assert!(config.quiet);
    }

    /// Verifies `-q` is accepted as a short alias for `--quiet`.
    #[test]
    fn short_quiet_flag_sets_quiet() {
        let args = vec!["elephc".into(), "-q".into(), "app.php".into()];
        let config = compile_config(&args);
        assert!(config.quiet);
    }

    /// Verifies quiet defaults to false when not passed.
    #[test]
    fn quiet_defaults_to_false() {
        let args = vec!["elephc".into(), "app.php".into()];
        let config = compile_config(&args);
        assert!(!config.quiet);
    }

    /// Verifies `--help` is detected anywhere in the argument list.
    #[test]
    fn wants_help_detects_long_flag_anywhere() {
        let args = vec![
            "elephc".into(),
            "--check".into(),
            "--help".into(),
            "app.php".into(),
        ];
        assert!(wants_help(&args));
    }

    /// Verifies `-h` is detected as the short alias for `--help`.
    #[test]
    fn wants_help_detects_short_flag() {
        let args = vec!["elephc".into(), "-h".into()];
        assert!(wants_help(&args));
    }

    /// Verifies a normal argument list without `--help`/`-h` is not mistaken for a help request.
    #[test]
    fn wants_help_false_without_help_flag() {
        let args = vec!["elephc".into(), "app.php".into()];
        assert!(!wants_help(&args));
    }

    /// Verifies `--version` is detected anywhere in the argument list.
    #[test]
    fn wants_version_detects_long_flag_anywhere() {
        let args = vec![
            "elephc".into(),
            "--check".into(),
            "--version".into(),
            "app.php".into(),
        ];
        assert!(wants_version(&args));
    }

    /// Verifies `-V` is detected as the short alias for `--version`.
    #[test]
    fn wants_version_detects_short_flag() {
        let args = vec!["elephc".into(), "-V".into()];
        assert!(wants_version(&args));
    }

    /// Verifies normal arguments are not mistaken for a version request.
    #[test]
    fn wants_version_false_without_version_flag() {
        let args = vec!["elephc".into(), "app.php".into()];
        assert!(!wants_version(&args));
    }

    /// The report a release probe reads must name every bridge in the table.
    ///
    /// This is the whole mechanism: the probe unpacks a tarball and asks the
    /// binary inside it which archives it needs, so a bridge the report omits
    /// is a bridge the probe cannot check was packed — silently, and only in
    /// the shipped artifact, which is the one place no in-repo test can look.
    /// Derived from `crate_flag_names()` so the next bridge is covered by the
    /// edit that declares it rather than by anyone remembering this test.
    #[test]
    fn the_capability_report_names_every_bridge_archive() {
        let report = super::capability_report();
        for flag in crate::linker::crate_flag_names() {
            let archive = crate::linker::archive_filename_for_flag(flag)
                .expect("every bridge flag resolves to an archive");
            assert!(
                report.contains(&archive),
                "--print-capabilities never names `{archive}`, so the release \
                 probe cannot check that it was packed beside the binary"
            );
        }
    }

    /// Every accepted `--with-<name>` must be advertised on the `--help` line.
    ///
    /// The list was literal once and drifted: `iconv` was accepted by the
    /// parser and absent from `--help`, so the reference text described a
    /// smaller compiler than the binary printing it.
    #[test]
    fn help_advertises_every_accepted_capability() {
        let help = super::help_text();
        assert!(
            !help.contains("{CAPABILITIES}"),
            "the capability placeholder reached the user unsubstituted"
        );
        let line = help
            .lines()
            .find(|line| line.contains("--with-NAME"))
            .expect("--help documents --with-NAME");
        for name in super::with_flag_names() {
            assert!(
                line.contains(name),
                "--help never advertises --with-{name}, which the parser accepts"
            );
        }
    }

    /// Verifies `--print-capabilities` is detected anywhere in the argument list.
    #[test]
    fn wants_capabilities_detects_the_flag_anywhere() {
        let args = vec![
            "elephc".into(),
            "app.php".into(),
            "--print-capabilities".into(),
        ];
        assert!(super::wants_capabilities(&args));
        assert!(!super::wants_capabilities(&["elephc".into(), "app.php".into()]));
    }

    /// Verifies help exposes both the current compiler version and its version flags.
    #[test]
    fn help_includes_version_and_version_flags() {
        assert!(HELP.contains(&format!("Version: {VERSION}")));
        assert!(HELP.contains("-V, --version"));
    }

    /// Verifies `--mascotte` is detected anywhere in the argument list.
    #[test]
    fn wants_mascotte_detects_flag_anywhere() {
        let args = vec![
            "elephc".into(),
            "--check".into(),
            "--mascotte".into(),
            "app.php".into(),
        ];
        assert!(wants_mascotte(&args));
    }

    /// Verifies a normal argument list without `--mascotte` is not mistaken for one.
    #[test]
    fn wants_mascotte_false_without_flag() {
        let args = vec!["elephc".into(), "app.php".into()];
        assert!(!wants_mascotte(&args));
    }

    /// Verifies a parameter-error message is formatted as `error: <message>`
    /// followed by the short usage line and the `--help` hint, so every
    /// parsing failure (unknown flag, bad value, missing file) reads the same way.
    #[test]
    fn format_fail_message_has_error_prefix_usage_and_hint() {
        let msg = format_fail_message("Unknown flag: --bogus");
        assert!(msg.starts_with("error: Unknown flag: --bogus"));
        assert!(msg.contains(USAGE));
        assert!(msg.contains("Run 'elephc --help' for more information."));
    }

    /// Verifies only an exact first positional `native` selects the package command family.
    #[test]
    fn exact_first_native_token_selects_native_command() {
        let args = vec!["elephc".into(), "native".into(), "list".into()];
        assert!(matches!(
            parse_args(&args),
            Command::Native(NativeCommand::List { .. })
        ));

        let explicit_source = vec!["elephc".into(), "./native".into()];
        let Command::Compile(config) = parse_args(&explicit_source) else {
            panic!("explicit source path must remain a compile command");
        };
        assert_eq!(config.filename, "./native");
    }
}
