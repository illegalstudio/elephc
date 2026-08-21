//! Purpose:
//! Provides the binary entry point for the compiler and native dependency commands.
//! Wires top-level dispatch to the appropriate orchestration layer.
//!
//! Called from:
//! - The operating system when running the `elephc` executable.
//!
//! Key details:
//! - Keep startup thin so CLI validation and pipeline behavior stay in dedicated modules.

mod autoload;
mod builtins;
mod cli;
mod codegen;
mod codegen_support;
mod conditional;
mod errors;
mod eval_aot;
mod exports;
mod func_args;
mod dir_prelude;
mod hash_prelude;
mod image_prelude;
mod intrinsics;
#[allow(dead_code, unused_imports)]
mod ir;
#[allow(dead_code, unused_imports)]
mod ir_lower;
#[allow(dead_code, unused_imports)]
mod ir_passes;
#[allow(dead_code)]
mod link_plan;
mod link_planning;
mod linker;
mod lexer;
mod list_id_prelude;
mod magic_constants;
mod name_resolver;
#[allow(dead_code, unused_imports)]
mod native_deps;
mod names;
mod opcache;
mod opcache_prelude;
mod optimize;
mod parser;
mod php_version;
mod pdo_prelude;
mod php_profile;
mod prelude_prune;
mod pipeline;
mod progress;
mod resolver;
mod runtime_cache;
mod scanf_prelude;
mod debug_info;
mod source;
mod source_map;
mod span;
mod strict_php;
mod string_bytes;
mod superglobals;
#[allow(dead_code)]
mod synthetic_class;
mod termination;
mod timings;
mod types;
mod tz_prelude;
mod var_export_prelude;
mod version_prelude;
mod web_prelude;

/// Entry point for the `elephc` binary.
///
/// Collects command-line arguments, parses the top-level command, and delegates
/// to either compilation or explicit native-dependency orchestration.
///
/// # Inputs
/// - `std::env::args()`: OS-provided arguments, where `args[0]` is the program name.
///
/// # Outputs
/// - Returns `()` when the selected command succeeds without an explicit exit.
/// - Never returns on fatal errors or unhealthy native diagnostics.
///
/// # Side effects
/// - Compile commands read source files and write outputs alongside the source.
/// - Mutating native commands may update project files and the durable native cache.
/// - Emits warnings/errors to stderr, including OPcache `--ini` quantity diagnostics
///   ([`emit_ini_override_warnings`]) for compile commands.
/// - May create temporary files during assembly and linking.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if cli::wants_mascotte(&args) {
        cli::print_mascotte();
    }
    match cli::parse_args(&args) {
        cli::Command::Compile(config) => {
            emit_ini_override_warnings(&config);
            pipeline::compile(config);
        }
        cli::Command::Native(command) => run_native(command),
    }
}

/// Executes a parsed native command and maps its captured output to process streams/status.
fn run_native(command: native_deps::NativeCommand) {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            eprintln!("failed to read current directory: {error}");
            std::process::exit(1);
        }
    };
    match native_deps::run_native_command(&command, &cwd) {
        Ok(output) => {
            print!("{}", output.stdout);
            if output.exit_code != 0 {
                std::process::exit(output.exit_code);
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

/// Prints the startup diagnostics reference PHP would emit for the `--ini` overrides this
/// compile carries, to stderr, before the pipeline runs.
///
/// Reference PHP emits these while REGISTERING the INI entries at startup — a
/// `Warning: Invalid "opcache.max_file_size" setting. Invalid quantity "12abc": unknown
/// multiplier "c", interpreting as "12" for backwards compatibility in Unknown on line 0` for
/// `php -d opcache.max_file_size=12abc`. For elephc the compile IS the registration (the
/// directive values are baked into the binary), so this is where the faithful analogue belongs
/// and the only point at which it is actionable. The value is still STORED either way — see
/// `crate::opcache::directives::parse_ini_quantity` — so without this the misread is silent.
///
/// The `in Unknown on line 0` tail is dropped: it names reference PHP's INI-file position, and
/// elephc's source of the value is a command-line flag, which the compiler's own stderr voice
/// already implies. Nothing is emitted when there are no `--ini` overrides, so the default
/// compile path is byte-identical on stderr.
fn emit_ini_override_warnings(config: &cli::CliConfig) {
    for warning in opcache::directives::ini_override_warnings(
        config.php_version.version_id(),
        &config.ini_overrides,
    ) {
        eprintln!("Warning: {warning}");
    }
}
