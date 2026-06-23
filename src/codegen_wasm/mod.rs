//! Purpose:
//! Active EIR → WebAssembly (`wasm32-wasi`) backend. Consumes the same EIR
//! `Module` the native backend consumes and emits a WebAssembly text module
//! (`.wat`); the pipeline then encodes that to a `.wasm` binary via the `wat`
//! crate, or packages it for NPM.
//!
//! Called from:
//! - `crate::pipeline::compile()` on the `target.is_wasm()` branch, in place of
//!   `crate::codegen_ir` + the native assembler/linker.
//!
//! Key details:
//! - WebAssembly is a structured stack machine with linear memory; there are no
//!   machine registers and no register allocation. EIR SSA values map to typed
//!   WASM locals, and the arbitrary EIR control-flow graph is lowered to
//!   structured control flow via a dispatch loop (`crate::codegen_wasm` later
//!   phases). Runtime helpers are emitted as WAT functions preserving the exact
//!   native memory layouts so semantics match the native targets byte-for-byte.
//! - This module is currently a P0 seam stub: it emits a minimal valid WASI
//!   command module so the target plumbing (CLI, pipeline branch, `.wat`/`.wasm`
//!   emission, packaging) can be wired and tested end to end. The real EIR
//!   lowering lands in subsequent phases.

use crate::codegen::Emit;
use crate::ir::Module;

/// An error raised while lowering EIR to WebAssembly.
// The P0 seam stub never fails; `Unsupported` is constructed by the real EIR
// lowering introduced in the next phase. Allow dead code until then.
#[allow(dead_code)]
#[derive(Debug)]
pub enum WasmError {
    /// An EIR construct (op, terminator, type, or feature) that the WebAssembly
    /// backend does not yet support. The string names the construct so the
    /// pipeline can surface a clean diagnostic instead of emitting a broken
    /// module or panicking.
    Unsupported(String),
}

impl std::fmt::Display for WasmError {
    /// Formats the error for the compiler's stderr diagnostic.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WasmError::Unsupported(what) => {
                write!(f, "{} is not yet supported on the wasm32-wasi target", what)
            }
        }
    }
}

impl std::error::Error for WasmError {}

/// Lowers a checked, optimized EIR `Module` to a WebAssembly text module (`.wat`).
///
/// `emit` selects the artifact shape: `Executable` and `NpmPackage` produce a
/// WASI *command* module (exporting `_start` and `memory`); `Cdylib` produces a
/// *reactor* module (no `_start`). The returned string is valid WebAssembly text
/// that the pipeline encodes to `.wasm` with the `wat` crate.
///
/// P0 seam stub: emits a minimal WASI command module that exits with status 0,
/// ignoring the EIR body. Real EIR lowering is added in later phases.
pub fn generate(module: &Module, emit: Emit) -> Result<String, WasmError> {
    let _ = module;
    let mut wat = String::new();
    wat.push_str("(module\n");
    wat.push_str(
        "  (import \"wasi_snapshot_preview1\" \"proc_exit\" (func $__wasi_proc_exit (param i32)))\n",
    );
    wat.push_str("  (memory (export \"memory\") 1)\n");
    match emit {
        // Command module: WASI runtimes invoke `_start` as the entry point.
        Emit::Executable | Emit::NpmPackage => {
            wat.push_str("  (func (export \"_start\")\n");
            wat.push_str("    (call $__wasi_proc_exit (i32.const 0)))\n");
        }
        // Reactor module: no `_start`; only explicitly exported functions run.
        Emit::Cdylib => {
            wat.push_str("  (func (export \"_initialize\"))\n");
        }
    }
    wat.push_str(")\n");
    Ok(wat)
}
