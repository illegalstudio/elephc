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
mod lexer;
mod linker;
mod list_id_prelude;
mod magic_constants;
mod name_resolver;
mod names;
mod optimize;
mod parser;
mod pdo_prelude;
mod pipeline;
mod progress;
mod resolver;
mod runtime_cache;
mod debug_info;
mod source_map;
mod source_path;
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
mod windows_toolchain;

/// Runs the compiler entry point on the platform's main or a dedicated worker thread.
///
/// Windows gives the executable's initial thread a comparatively small fixed
/// stack. The compiler still contains recursive frontend and lowering paths,
/// so Windows moves that work to an explicitly sized Rust thread. This leaves
/// the process entry thread shallow while preserving the normal process-wide
/// exit behavior used by fatal compilation diagnostics.
///
/// # Inputs
/// - The operating-system process entry point.
///
/// # Outputs
/// - Returns once the compiler succeeds, or propagates a worker panic.
///
/// # Side effects
/// - Allocates a Windows compiler worker stack before invoking the pipeline.
fn main() {
    #[cfg(windows)]
    {
        const WINDOWS_COMPILER_STACK_BYTES: usize = 64 * 1024 * 1024;
        std::thread::Builder::new()
            .name("elephc-compiler".into())
            .stack_size(WINDOWS_COMPILER_STACK_BYTES)
            .spawn(run_compiler)
            .expect("create Windows compiler worker thread")
            .join()
            .expect("Windows compiler worker thread panicked");
    }

    #[cfg(not(windows))]
    run_compiler();
}

/// Parses CLI arguments and runs the compiler's ordered pipeline.
///
/// A Windows caller invokes this from a dedicated large-stack worker because
/// `RUST_MIN_STACK` does not resize the executable's original main thread.
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
fn run_compiler() {
    let args: Vec<String> = std::env::args().collect();
    if cli::wants_mascotte(&args) {
        cli::print_mascotte();
    }
    let config = cli::parse_args(&args);
    pipeline::compile(config);
}
