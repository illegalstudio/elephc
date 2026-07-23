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
mod optimize;
mod parser;
mod pdo_prelude;
mod pipeline;
mod resolver;
mod runtime_cache;
mod debug_info;
mod source_map;
mod span;
mod strict_php;
mod string_bytes;
mod superglobals;
mod termination;
mod timings;
mod types;
mod tz_prelude;
mod var_export_prelude;
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
/// - Emits warnings/errors to stderr.
/// - May create temporary files during assembly and linking.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    match cli::parse_args(&args) {
        cli::Command::Compile(config) => pipeline::compile(config),
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
