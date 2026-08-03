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
use crate::codegen::platform::Target;
use crate::native_deps::{native_help, parse_native_args, NativeCommand, NativeParseOutcome};

/// Short usage line shown after every parameter error, alongside the `--help` hint.
/// The full categorized reference lives in `HELP`.
pub(crate) const USAGE: &str = "Usage: elephc [OPTIONS] <source-file>";

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

/// Full `--help` reference text, categorized by section. Printed to stdout
/// with exit code 0 — this is a successful, requested action, not an error.
pub(crate) const HELP: &str = "Usage: elephc [OPTIONS] <source-file>

A PHP-to-native AOT compiler

Arguments:
  <source-file>           Tagged .php or tagless .lfc source file to compile

Modes:
  --web                   Compile as a prefork HTTP server (native only)
  --strict-php            Reject elephc extensions in tagged PHP source; .lfc remains extension-enabled

Output modes:
  --check                 Type-check only, no codegen (mutually exclusive with --emit-ir/--emit-asm)
  --emit-ir               Emit EIR text instead of compiling
  --emit-asm              Emit readable assembly (.s; .wat for WASM) instead of linking
  --emit KIND             Output kind: executable (default) | cdylib (native) | npm (WASM only)

Target:
  --target TARGET         macos-aarch64 | linux-aarch64 | linux-x86_64 | wasm32-wasi (windows-x86_64 recognized, backend pending)
  --php-version VERSION   8.2 | 8.3 | 8.4 | 8.5 (default: 8.5)

Codegen:
  --heap-size=BYTES       Fixed native heap size in bytes (default: 8388608)
  --null-repr=MODE        Native null representation: sentinel (default) | tagged
  --regalloc=MODE         Native register allocator: linear (default) | stack
  --ir-opt=on|off         EIR optimization passes (default: on; --no-ir-opt is an alias for --ir-opt=off)
  --gc-stats              Print native GC statistics at exit
  --heap-debug            Enable native heap debug instrumentation
  --define SYMBOL         Define a symbol for `ifdef` conditional compilation
  --ini KEY=VALUE         Bake an INI directive override (repeatable; opcache.* honored)

Linking:
  --link LIB, -l LIB      Extra native library to link
  --link-path DIR, -L DIR Extra native library search path
  --framework NAME        Native macOS framework to link
  --with-CRATE            Force-link a native bridge crate (pdo, tls, crypto, phar, tz, image, web, eval)

Diagnostics:
  --timings               Show a per-phase timing table on stderr
  --quiet, -q             Disable progress lines and colorized output
  --source-map            Emit a native .map source map alongside the assembly
  --debug-info            Embed native DWARF line info for debuggers

Other:
  -h, --help              Print this help and exit
  -V, --version           Print the compiler version and exit
  --mascotte              Print an ASCII mascot and a random quote before output
";

/// Configuration derived from command-line arguments, passed to the compile pipeline.
/// Controls heap allocation size, debug output, code generation options, and linking behavior.
pub(crate) struct CliConfig {
    pub(crate) filename: String,
    pub(crate) heap_size: usize,
    pub(crate) gc_stats: bool,
    pub(crate) heap_debug: bool,
    pub(crate) emit_ir: bool,
    pub(crate) null_repr: crate::codegen::NullRepr,
    pub(crate) emit_asm: bool,
    pub(crate) emit: Emit,
    pub(crate) check_only: bool,
    pub(crate) emit_timings: bool,
    pub(crate) emit_source_map: bool,
    pub(crate) emit_debug_info: bool,
    pub(crate) regalloc_linear: bool,
    pub(crate) ir_opt: bool,
    pub(crate) target: Target,
    /// PHP compatibility profile used by version-dependent language/runtime
    /// surfaces. Session behavior under `--web` currently consumes it.
    pub(crate) php_version: crate::web_prelude::PhpVersion,
    pub(crate) extra_link_libs: Vec<String>,
    pub(crate) extra_link_paths: Vec<String>,
    pub(crate) extra_frameworks: Vec<String>,
    pub(crate) defines: HashSet<String>,
    /// Accept only PHP-compatible constructs: elephc extensions (`ptr`, `buffer<T>`,
    /// `packed class`, `extern`, `ifdef`, extension builtins) become compile errors.
    pub(crate) strict_php: bool,
    pub(crate) web: bool,
    /// Bridge crates the user force-enabled with `--with-<crate>` (short flag
    /// names such as `"pdo"`). Each one force-links the matching staticlib and,
    /// for crates with a PHP-surface prelude, forces that prelude's injection so
    /// the API is available even when feature auto-detection would not trigger.
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
}

/// Parses the exact top-level `native` selector before falling back to legacy compilation.
pub(crate) fn parse_args(args: &[String]) -> Command {
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
        println!("{HELP}");
        process::exit(0);
    }
    if wants_version(args) {
        println!("elephc {}", env!("CARGO_PKG_VERSION"));
        process::exit(0);
    }

    let mut heap_size: usize = 8_388_608; // 8MB default
    let mut heap_size_overridden = false;
    let mut gc_stats = false;
    let mut heap_debug = false;
    let mut emit_ir = false;
    let mut emit_asm = false;
    let mut emit = Emit::Executable;
    let mut check_only = false;
    let mut emit_timings = false;
    let mut emit_source_map = false;
    let mut emit_debug_info = false;
    let mut filename_arg = None;
    let mut target = Target::detect_host();
    let mut php_version = crate::web_prelude::PhpVersion::default();
    let mut extra_link_libs: Vec<String> = Vec::new();
    let mut extra_link_paths: Vec<String> = Vec::new();
    let mut extra_frameworks: Vec<String> = Vec::new();
    let mut defines: HashSet<String> = HashSet::new();
    let mut strict_php = false;
    let mut web = false;
    let mut quiet = false;
    let mut with_crates: HashSet<String> = HashSet::new();
    let mut ini_overrides: Vec<(String, String)> = Vec::new();
    let null_repr_env = std::env::var("ELEPHC_NULL_REPR").ok();
    // Environment defaults are native test-harness policy, not explicit user
    // requests. Only a CLI flag should make WASM reject the native-only knob.
    let mut null_repr_overridden = false;
    let mut null_repr = match null_repr_env.as_deref() {
        Some("tagged") => crate::codegen::NullRepr::Tagged,
        Some("sentinel") => crate::codegen::NullRepr::Sentinel,
        _ => crate::codegen::NullRepr::default(),
    };
    // The register allocator is on by default; an env override lets the test
    // harness compile the whole suite under the stack fallback for comparison.
    let regalloc_env = std::env::var("ELEPHC_REGALLOC").ok();
    // As above, the environment selects the native default without turning it
    // into an explicit target-specific option.
    let mut regalloc_overridden = false;
    let mut regalloc_linear = match regalloc_env.as_deref() {
        Some("stack") => false,
        Some("linear") => true,
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
            heap_size_overridden = true;
        } else if arg == "--target" {
            i += 1;
            target = parse_required_target(args, i);
        } else if let Some(value) = arg.strip_prefix("--target=") {
            target = parse_target(value);
        } else if arg == "--php-version" {
            i += 1;
            php_version = parse_required_php_version(args, i);
        } else if let Some(value) = arg.strip_prefix("--php-version=") {
            php_version = parse_php_version(value);
        } else if arg == "--gc-stats" {
            gc_stats = true;
        } else if arg == "--heap-debug" {
            heap_debug = true;
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
        } else if arg == "--quiet" || arg == "-q" {
            quiet = true;
        } else if arg == "--mascotte" {
            // Already handled in main() before parse_args ran (so the banner
            // prints before --help/errors/compilation); recognized here only
            // so it isn't mistaken for an unknown flag.
        } else if let Some(value) = arg.strip_prefix("--null-repr=") {
            null_repr = parse_null_repr(value);
            null_repr_overridden = true;
        } else if let Some(value) = arg.strip_prefix("--regalloc=") {
            regalloc_linear = parse_regalloc(value);
            regalloc_overridden = true;
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
        } else if arg == "--web" {
            web = true;
        } else if let Some(name) = arg.strip_prefix("--with-") {
            // `--with-web` aliases the full `--web` mode (it owns the program
            // entry point); every other known crate is recorded for force-link
            // and prelude forcing. An unknown crate name is a hard error so a
            // typo never silently no-ops.
            if name == "web" {
                web = true;
            } else if crate::linker::bridge_lib_for_flag(name).is_some() {
                with_crates.insert(name.to_string());
            } else {
                fail(&format!(
                    "Unknown crate for --with-{}: expected one of: {}",
                    name,
                    crate::linker::crate_flag_names().join(", ")
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
    if let Err(message) = validate_emit_target(emit, target, emit_asm) {
        fail(message);
    }
    let wasm_options = WasmCliOptions {
        web,
        source_map: emit_source_map,
        debug_info: emit_debug_info,
        gc_stats,
        heap_debug,
        heap_size: heap_size_overridden,
        null_repr: null_repr_overridden,
        regalloc: regalloc_overridden,
        native_linking: !extra_link_libs.is_empty()
            || !extra_link_paths.is_empty()
            || !extra_frameworks.is_empty(),
        bridge_crates: !with_crates.is_empty(),
    };
    if let Err(message) = validate_wasm_options(target, &wasm_options) {
        fail(message);
    }
    if web && check_only {
        fail("--web cannot be combined with --check");
    }
    if web && matches!(emit, Emit::Cdylib) {
        fail("--web cannot be combined with --emit cdylib");
    }
    if web && emit_asm {
        fail("--web cannot be combined with --emit-asm");
    }
    if web && emit_ir {
        fail("--web cannot be combined with --emit-ir");
    }
    CliConfig {
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
    }
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
        fail("Missing version after --php-version (expected 8.2, 8.3, 8.4, or 8.5)")
    }
}

/// Parses a supported PHP compatibility version or exits with a focused diagnostic.
fn parse_php_version(value: &str) -> crate::web_prelude::PhpVersion {
    crate::web_prelude::PhpVersion::parse(value).unwrap_or_else(|| {
        fail(&format!(
            "Unsupported PHP version '{}': expected 8.2, 8.3, 8.4, or 8.5",
            value
        ))
    })
}

/// Validates target-specific output kinds and combinations before compilation.
fn validate_emit_target(emit: Emit, target: Target, emit_asm: bool) -> Result<(), &'static str> {
    if matches!(emit, Emit::NpmPackage) && !target.is_wasm() {
        return Err("--emit npm requires --target wasm32-wasi");
    }
    if matches!(emit, Emit::NpmPackage) && emit_asm {
        return Err("--emit npm cannot be combined with --emit-asm");
    }
    if matches!(emit, Emit::Cdylib) && target.is_wasm() {
        return Err("--emit cdylib is not yet supported for --target wasm32-wasi");
    }
    Ok(())
}

/// Records CLI surfaces whose implementation is currently native-backend-only.
#[derive(Default)]
struct WasmCliOptions {
    web: bool,
    source_map: bool,
    debug_info: bool,
    gc_stats: bool,
    heap_debug: bool,
    heap_size: bool,
    null_repr: bool,
    regalloc: bool,
    native_linking: bool,
    bridge_crates: bool,
}

/// Rejects native-only options for the WASM target instead of silently ignoring
/// them after the pipeline branches into `codegen_wasm`.
fn validate_wasm_options(
    target: Target,
    options: &WasmCliOptions,
) -> Result<(), &'static str> {
    if !target.is_wasm() {
        return Ok(());
    }
    if options.web {
        return Err("--web is not yet supported for --target wasm32-wasi");
    }
    if options.source_map {
        return Err("--source-map is not yet supported for --target wasm32-wasi");
    }
    if options.debug_info {
        return Err("--debug-info is not yet supported for --target wasm32-wasi");
    }
    if options.gc_stats {
        return Err("--gc-stats is not yet supported for --target wasm32-wasi");
    }
    if options.heap_debug {
        return Err("--heap-debug is not yet supported for --target wasm32-wasi");
    }
    if options.heap_size {
        return Err("--heap-size is not yet configurable for --target wasm32-wasi");
    }
    if options.null_repr {
        return Err("--null-repr is not configurable for --target wasm32-wasi");
    }
    if options.regalloc {
        return Err("--regalloc does not apply to --target wasm32-wasi");
    }
    if options.native_linking {
        return Err("--link, --link-path, and --framework are not supported for --target wasm32-wasi");
    }
    if options.bridge_crates {
        return Err("--with-CRATE bridge linking is not supported for --target wasm32-wasi");
    }
    Ok(())
}

/// Parse the required emit-kind argument at the given index, or fail if missing.
fn parse_required_emit(args: &[String], index: usize) -> Emit {
    if index < args.len() {
        parse_emit(&args[index])
    } else {
        fail("Missing emit kind after --emit (expected: executable, cdylib, npm)")
    }
}

/// Parse an emit-kind string into an `Emit` value, or fail with an error message.
///
/// `npm`/`npm-package` selects WebAssembly NPM-package output and is only valid
/// with `--target wasm32-wasi`; that cross-flag constraint is enforced in
/// `parse_args` once the target is known.
fn parse_emit(value: &str) -> Emit {
    match value {
        "executable" | "exe" | "bin" => Emit::Executable,
        "cdylib" | "dylib" | "shared" => Emit::Cdylib,
        "npm" | "npm-package" => Emit::NpmPackage,
        other => fail(&format!(
            "Invalid --emit kind '{}': expected one of: executable, cdylib, npm",
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

/// Parse a target string to a Target enum, or fail with an error message.
fn parse_target(value: &str) -> Target {
    match Target::parse(value) {
        Ok(target) => target,
        Err(err) => fail(&err),
    }
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
    use super::*;

    /// Extracts the compile configuration returned for a legacy invocation.
    fn compile_config(args: &[String]) -> CliConfig {
        let Command::Compile(config) = parse_args(args) else {
            panic!("expected compile command");
        };
        config
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
        assert_eq!(parse_emit("npm"), Emit::NpmPackage);
    }

    /// Verifies the accepted aliases map to their canonical variants so users coming
    /// from cargo (`cdylib`/`dylib`) and unix toolchains (`shared`, `bin`) all work.
    #[test]
    fn emit_kind_accepts_aliases() {
        assert_eq!(parse_emit("exe"), Emit::Executable);
        assert_eq!(parse_emit("bin"), Emit::Executable);
        assert_eq!(parse_emit("dylib"), Emit::Cdylib);
        assert_eq!(parse_emit("shared"), Emit::Cdylib);
        assert_eq!(parse_emit("npm-package"), Emit::NpmPackage);
    }

    /// Verifies WASM accepts command/NPM output but rejects native cdylibs and
    /// the contradictory NPM-plus-WAT-only combination.
    #[test]
    fn wasm_emit_combinations_are_validated() {
        let wasm = Target::wasm();
        assert_eq!(
            validate_emit_target(Emit::Executable, wasm, false),
            Ok(())
        );
        assert_eq!(
            validate_emit_target(Emit::NpmPackage, wasm, false),
            Ok(())
        );
        assert_eq!(
            validate_emit_target(Emit::Cdylib, wasm, false),
            Err("--emit cdylib is not yet supported for --target wasm32-wasi")
        );
        assert_eq!(
            validate_emit_target(Emit::NpmPackage, wasm, true),
            Err("--emit npm cannot be combined with --emit-asm")
        );
    }

    /// Verifies NPM packaging cannot be selected for an ordinary native target.
    #[test]
    fn native_target_rejects_npm_output() {
        assert_eq!(
            validate_emit_target(Emit::NpmPackage, Target::detect_host(), false),
            Err("--emit npm requires --target wasm32-wasi")
        );
    }

    /// Verifies native-only codegen and linker options are rejected for WASM
    /// rather than being silently ignored by the pipeline's early return.
    #[test]
    fn wasm_rejects_native_only_options() {
        let wasm = Target::wasm();
        assert_eq!(
            validate_wasm_options(
                wasm,
                &WasmCliOptions {
                    source_map: true,
                    ..WasmCliOptions::default()
                },
            ),
            Err("--source-map is not yet supported for --target wasm32-wasi")
        );
        assert_eq!(
            validate_wasm_options(
                wasm,
                &WasmCliOptions {
                    native_linking: true,
                    ..WasmCliOptions::default()
                },
            ),
            Err("--link, --link-path, and --framework are not supported for --target wasm32-wasi")
        );
        assert_eq!(
            validate_wasm_options(
                wasm,
                &WasmCliOptions {
                    bridge_crates: true,
                    ..WasmCliOptions::default()
                },
            ),
            Err("--with-CRATE bridge linking is not supported for --target wasm32-wasi")
        );
        assert_eq!(
            validate_wasm_options(Target::detect_host(), &WasmCliOptions {
                source_map: true,
                native_linking: true,
                bridge_crates: true,
                ..WasmCliOptions::default()
            }),
            Ok(())
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
    }

    /// Verifies the absence of `--web` leaves the web flag off.
    #[test]
    fn no_web_flag_defaults_off() {
        let args = vec!["elephc".into(), "app.php".into()];
        let config = compile_config(&args);
        assert!(!config.web);
    }

    /// Verifies every maintained PHP minor maps to its exact compatibility profile.
    #[test]
    fn maintained_php_versions_parse() {
        assert_eq!(parse_php_version("8.2").version_id(), 80200);
        assert_eq!(parse_php_version("8.3").version_id(), 80300);
        assert_eq!(parse_php_version("8.4").version_id(), 80400);
        assert_eq!(parse_php_version("8.5").version_id(), 80500);
        assert!(crate::web_prelude::PhpVersion::parse("8.1").is_none());
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

    /// Verifies multiple `--with-<crate>` flags accumulate into the forced set.
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

    /// Verifies both compiler-version flag spellings are recognized.
    #[test]
    fn wants_version_detects_short_and_long_flags() {
        let long = vec!["elephc".into(), "--version".into()];
        let short = vec!["elephc".into(), "-V".into()];
        assert!(wants_version(&long));
        assert!(wants_version(&short));
    }

    /// Verifies ordinary compile arguments do not request compiler-version output.
    #[test]
    fn wants_version_false_without_version_flag() {
        let args = vec!["elephc".into(), "--check".into(), "app.php".into()];
        assert!(!wants_version(&args));
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
