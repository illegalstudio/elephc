//! Purpose:
//! Provides the binary entry point for the elephc compiler.
//! Wires CLI parsing to the ordered compile pipeline without owning compiler logic.
//!
//! Called from:
//! - The operating system when running the `elephc` executable.
//!
//! Key details:
//! - Keep startup thin so CLI validation and pipeline behavior stay in dedicated modules.

mod autoload;
mod cli;
mod codegen;
mod codegen_ir;
mod conditional;
mod errors;
mod exports;
mod intrinsics;
#[allow(dead_code, unused_imports)]
mod ir;
#[allow(dead_code, unused_imports)]
mod ir_lower;
#[allow(dead_code, unused_imports)]
mod ir_passes;
mod linker;
mod lexer;
mod list_id_prelude;
mod magic_constants;
mod name_resolver;
mod names;
mod optimize;
mod parser;
mod pdo_prelude;
mod pipeline;
mod resolver;
mod runtime_cache;
mod source_map;
mod span;
mod string_bytes;
mod termination;
mod timings;
mod types;
mod tz_prelude;
mod var_export_prelude;

/// Entry point for the `elephc` binary.
///
/// Collects command-line arguments, parses them into a `Config`, and delegates
/// to the compile pipeline. Exits via `std::process::exit` if compilation fails
/// (the pipeline handles fatal error reporting internally).
///
/// # Inputs
/// - `std::env::args()`: OS-provided arguments, where `args[0]` is the program name.
///
/// # Outputs
/// - Returns `()` on successful compilation (pipeline handles output binary creation).
/// - Never returns on fatal error (calls `std::process::exit` internally).
///
/// # Side effects
/// - Reads source files and writes the compiled binary alongside the source.
/// - Emits warnings/errors to stderr.
/// - May create temporary files during assembly and linking.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let config = cli::parse_args(&args);
    pipeline::compile(config);
}
