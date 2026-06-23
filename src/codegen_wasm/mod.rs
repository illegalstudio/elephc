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
//!   WASM locals (`values`), and the arbitrary EIR control-flow graph is lowered
//!   to structured control flow via a br_table dispatch loop (`function`).
//!   Runtime helpers are emitted as WAT functions preserving the native memory
//!   layouts so semantics match the native targets.
//! - Instruction (op) bodies are lowered in a later phase; until then a function
//!   containing instructions returns `WasmError::Unsupported` rather than emitting
//!   silently-wrong code. Empty `main` (the P1 gate) lowers and runs end to end.

mod function;
mod values;
mod wat;

use crate::codegen::Emit;
use crate::ir::Module;

/// An error raised while lowering EIR to WebAssembly.
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
/// Sets up the module's WASI imports and memory, lowers every EIR function
/// through `function::lower_function`, and renders the result. The `is_main`
/// function becomes the `_start` command entry. Returns `WasmError::Unsupported`
/// if any function uses an EIR construct the backend does not yet handle.
///
/// `emit` will select command vs. reactor packaging once non-command output is
/// implemented; the WASI command shape is the only one wired today.
pub fn generate(module: &Module, emit: Emit) -> Result<String, WasmError> {
    let _ = emit;
    let mut wm = wat::WatModule::new();
    // WASI imports consumed by the runtime and terminator lowering.
    wm.import_func(wat::FuncImport {
        module: "wasi_snapshot_preview1".to_string(),
        field: "proc_exit".to_string(),
        internal: "wasi_proc_exit".to_string(),
        params: vec![wat::ValType::I32],
        results: vec![],
    });
    wm.set_memory(1, Some("memory"));

    // Lower every user function; `main` becomes the WASI `_start` command entry.
    for func in &module.functions {
        let fb = function::lower_function(module, func)?;
        wm.add_func(fb);
    }

    Ok(wm.render())
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! End-to-end tests for the wasm32-wasi control-flow backbone (P1): the
    //! br_table dispatch loop, block-argument materialization, and terminators.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - EIR is hand-built with `crate::ir::Builder` using ONLY block parameters
    //!   and terminators (no instructions), because instruction lowering lands in a
    //!   later phase. The generated WAT is fully type-validated with `wasmparser`,
    //!   which catches structural and typing defects (e.g. a result-type mismatch
    //!   on a value-returning function), and the `main` module is run under
    //!   `wasmer` when it is available.

    use super::generate;
    use crate::codegen::platform::Target;
    use crate::codegen::Emit;
    use crate::ir::{Builder, Function, IrType, Module, Terminator};
    use crate::types::PhpType;

    /// Assembles WAT to a wasm binary and fully validates it, returning the bytes.
    ///
    /// Panics with the WAT text if assembly or validation fails, so a structural or
    /// typing defect in the dispatch-loop lowering is reported legibly.
    fn assemble_and_validate(wat: &str) -> Vec<u8> {
        let bytes = ::wat::parse_str(wat).unwrap_or_else(|e| panic!("WAT did not assemble: {e}\n{wat}"));
        wasmparser::validate(&bytes)
            .unwrap_or_else(|e| panic!("wasm did not validate: {e}\n{wat}"));
        bytes
    }

    /// Builds a `main` (is_main) function whose entry `CondBr`s on a block parameter
    /// into two empty blocks, each returning void. Exercises CondBr, the 3-block
    /// dispatch loop, and the main `proc_exit(0)` return.
    fn main_condbr_module() -> Module {
        let mut module = Module::new(Target::wasm());
        let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut b = Builder::new(&mut function);
            let entry = b.create_named_block("entry", vec![(IrType::I64, PhpType::Int)]);
            let then_b = b.create_named_block("then", Vec::new());
            let else_b = b.create_named_block("else", Vec::new());
            b.set_entry(entry);
            let cond = b.block_param(entry, 0);
            b.position_at_end(entry);
            b.terminate(Terminator::CondBr {
                cond,
                then_target: then_b,
                then_args: Vec::new(),
                else_target: else_b,
                else_args: Vec::new(),
            });
            b.position_at_end(then_b);
            b.terminate(Terminator::Return { value: None });
            b.position_at_end(else_b);
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);
        module
    }

    /// Builds a non-main `int` function: `entry(x) -> br body(x); body(y) -> return y`.
    /// Exercises an unconditional `Br` with one argument (parallel-move
    /// materialization across two blocks) and a value `return`.
    fn br_with_args_module() -> Module {
        let mut module = Module::new(Target::wasm());
        let mut function = Function::new("thread".to_string(), IrType::I64, PhpType::Int);
        {
            let mut b = Builder::new(&mut function);
            let entry = b.create_named_block("entry", vec![(IrType::I64, PhpType::Int)]);
            let body = b.create_named_block("body", vec![(IrType::I64, PhpType::Int)]);
            b.set_entry(entry);
            let x = b.block_param(entry, 0);
            let y = b.block_param(body, 0);
            b.position_at_end(entry);
            b.terminate(Terminator::Br {
                target: body,
                args: vec![x],
            });
            b.position_at_end(body);
            b.terminate(Terminator::Return { value: Some(y) });
        }
        module.add_function(function);
        module
    }

    /// Builds a non-main `int` function whose entry `Switch`es a block-parameter
    /// scrutinee with one case and a default, both carrying a block argument.
    /// Exercises the Switch cascade and case/default argument materialization.
    fn switch_module() -> Module {
        use crate::ir::SwitchCase;
        let mut module = Module::new(Target::wasm());
        let mut function = Function::new("pick".to_string(), IrType::I64, PhpType::Int);
        {
            let mut b = Builder::new(&mut function);
            let entry = b.create_named_block("entry", vec![(IrType::I64, PhpType::Int)]);
            let case_b = b.create_named_block("case", vec![(IrType::I64, PhpType::Int)]);
            let default_b = b.create_named_block("default", vec![(IrType::I64, PhpType::Int)]);
            b.set_entry(entry);
            let s = b.block_param(entry, 0);
            let cv = b.block_param(case_b, 0);
            let dv = b.block_param(default_b, 0);
            b.position_at_end(entry);
            b.terminate(Terminator::Switch {
                scrutinee: s,
                cases: vec![SwitchCase {
                    value: 1,
                    target: case_b,
                    args: vec![s],
                }],
                default: default_b,
                default_args: vec![s],
            });
            b.position_at_end(case_b);
            b.terminate(Terminator::Return { value: Some(cv) });
            b.position_at_end(default_b);
            b.terminate(Terminator::Return { value: Some(dv) });
        }
        module.add_function(function);
        module
    }

    /// Verifies a `main` with a CondBr lowers to a valid `_start` command module
    /// containing the dispatch loop and the WASI proc_exit return.
    #[test]
    fn main_condbr_lowers_to_valid_wasm() {
        let wat = generate(&main_condbr_module(), Emit::Executable).expect("main should lower");
        assert!(wat.contains("(func $_entry (export \"_start\")"), "{wat}");
        assert!(wat.contains("br_table"), "{wat}");
        assert!(wat.contains("call $wasi_proc_exit"), "{wat}");
        assemble_and_validate(&wat);
    }

    /// Verifies an unconditional branch with a block argument validates (the
    /// parallel-move materialization and the i64 value `return` are well-typed).
    #[test]
    fn br_with_args_lowers_to_valid_wasm() {
        let wat = generate(&br_with_args_module(), Emit::Executable).expect("br fn should lower");
        assert!(wat.contains("(func $fn_thread"), "{wat}");
        assert!(wat.contains("(result i64)"), "{wat}");
        assert!(wat.contains("return"), "{wat}");
        assemble_and_validate(&wat);
    }

    /// Verifies a Switch terminator with case/default arguments validates.
    #[test]
    fn switch_lowers_to_valid_wasm() {
        let wat = generate(&switch_module(), Emit::Executable).expect("switch fn should lower");
        assert!(wat.contains("(func $fn_pick"), "{wat}");
        assert!(wat.contains("i64.eq"), "{wat}");
        assemble_and_validate(&wat);
    }

    /// Verifies a block containing an instruction is rejected (instruction lowering
    /// is a later phase) instead of emitting silently-wrong code.
    #[test]
    fn instruction_bearing_block_is_rejected() {
        let mut module = Module::new(Target::wasm());
        let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut b = Builder::new(&mut function);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let _ = b.emit_const_i64(7);
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);
        assert!(generate(&module, Emit::Executable).is_err());
    }

    /// Runs the generated `main` command module under `wasmer` (when present) and
    /// asserts a clean exit. Skips silently if `wasmer` is not installed, mirroring
    /// how external-tool-dependent tests degrade gracefully.
    #[test]
    fn main_module_runs_under_wasmer() {
        use std::process::Command;
        if Command::new("wasmer").arg("--version").output().is_err() {
            return; // wasmer not available; skip.
        }
        let wat = generate(&main_condbr_module(), Emit::Executable).expect("main should lower");
        let bytes = assemble_and_validate(&wat);
        let dir = std::env::temp_dir().join(format!("elephc_wasm_p1_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("main.wasm");
        std::fs::write(&path, &bytes).expect("write wasm");
        let status = Command::new("wasmer")
            .arg("run")
            .arg(&path)
            .status()
            .expect("run wasmer");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(status.success(), "wasmer run failed: {status}");
    }
}
