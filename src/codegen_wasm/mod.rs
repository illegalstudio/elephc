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

mod context;
mod function;
mod inst;
mod runtime;
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
    // The WASI imports + `__rt_*` runtime are only added for command (main-bearing)
    // modules. Importing WASI makes a runtime treat the module as a command
    // (requiring `_start`), so a reactor/library module with no main must not.
    let has_main = module.functions.iter().any(|f| f.flags.is_main);
    if has_main {
        runtime::emit_command_runtime(&mut wm);
    }

    // Lay out every interned string literal as a data segment above the runtime
    // scratch region, recording (offset, byte_len) per DataId for ConstStr.
    let mut str_literals: Vec<(u32, u32)> = Vec::with_capacity(module.data.strings.len());
    let mut cursor = runtime::RT_SCRATCH_END;
    for s in &module.data.strings {
        let bytes = s.as_bytes();
        wm.add_data(wat::DataSegment {
            offset: cursor,
            bytes: bytes.to_vec(),
        });
        str_literals.push((cursor, bytes.len() as u32));
        // 4-align the next literal.
        cursor = (cursor + bytes.len() as u32 + 3) & !3;
    }

    // Size memory to hold the scratch + data, plus headroom for the heap.
    const PAGE: u32 = 65536;
    let pages = (cursor / PAGE) + 2;
    wm.set_memory(pages, Some("memory"));

    // Lower every user function; `main` becomes the WASI `_start` command entry.
    for func in &module.functions {
        let fb = function::lower_function(module, func, &str_literals)?;
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
    use crate::ir::{
        Builder, CmpPredicate, Function, FunctionParam, Immediate, IrType, LocalKind, Module, Op,
        Ownership, Terminator, ValueId,
    };
    use crate::types::PhpType;

    use std::sync::atomic::{AtomicU32, Ordering};

    /// Per-process sequence for unique temp directories (tests run in parallel).
    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    /// Returns a unique temp directory path so concurrent wasmer runs never collide.
    fn unique_tmp_dir(tag: &str) -> std::path::PathBuf {
        let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("elephc_wasm_{}_{}_{}", tag, std::process::id(), n))
    }

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

    /// Verifies an op that is not yet lowered (`VarDump`, a much later phase) is
    /// rejected with a clean error instead of emitting silently-wrong code.
    #[test]
    fn unsupported_op_is_rejected() {
        let mut module = Module::new(Target::wasm());
        let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut b = Builder::new(&mut function);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let v = b.emit_const_i64(7);
            // VarDump needs the full runtime and is not lowered yet.
            let _ = b.emit(
                Op::VarDump,
                vec![v],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
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
        let dir = unique_tmp_dir("cmd");
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

    /// Generates and validates a command (main-bearing) module, runs it under
    /// `wasmer`, and returns its trimmed stdout. Validation always runs; the run
    /// is skipped (returns `None`) when `wasmer` is absent.
    fn run_main(module: &Module) -> Option<String> {
        let wat = generate(module, Emit::Executable).expect("module should lower");
        let bytes = assemble_and_validate(&wat);
        if !wasmer_available() {
            return None;
        }
        let dir = unique_tmp_dir("run");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("m.wasm");
        std::fs::write(&path, &bytes).expect("write wasm");
        let out = std::process::Command::new("wasmer")
            .arg("run")
            .arg(&path)
            .output()
            .expect("run wasmer");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            out.status.success(),
            "wasmer run failed: {}\n{}",
            String::from_utf8_lossy(&out.stderr),
            wat
        );
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Verifies `echo` of integers writes correct decimal text to stdout, covering
    /// positive, negative, and zero values via the `__rt_echo_i64` runtime helper.
    #[test]
    fn echo_integers_writes_to_stdout() {
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            for v in [42_i64, -7, 0, 1000000] {
                let c = b.emit_const_i64(v);
                let _ = b.emit(
                    Op::EchoValue,
                    vec![c],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            }
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        if let Some(out) = run_main(&module) {
            assert_eq!(out, "42-701000000");
        }
    }

    /// Verifies `echo` of booleans: true writes "1", false writes nothing.
    #[test]
    fn echo_booleans_writes_to_stdout() {
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            for v in [true, false, true] {
                let c = b.emit_const_bool(v);
                let _ = b.emit(
                    Op::EchoValue,
                    vec![c],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            }
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        if let Some(out) = run_main(&module) {
            assert_eq!(out, "11");
        }
    }

    /// Verifies `echo "Hello, WASM!"` of a string literal writes the exact bytes to
    /// stdout via a data segment + `__rt_echo_str`.
    #[test]
    fn echo_string_literal_writes_to_stdout() {
        let mut module = Module::new(Target::wasm());
        let hello = module.data.intern_string("Hello, WASM!");
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let s = b.emit_const_str(hello);
            let _ = b.emit(
                Op::EchoValue,
                vec![s],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        if let Some(out) = run_main(&module) {
            assert_eq!(out, "Hello, WASM!");
        }
    }

    /// Verifies `strlen` of a string literal returns the byte length (the literal's
    /// data-segment length), checked via `wasmer --invoke`.
    #[test]
    fn strlen_of_literal_invokes_correctly() {
        let mut module = Module::new(Target::wasm());
        let s_id = module.data.intern_string("héllo"); // 6 bytes (é is 2 bytes UTF-8)
        let mut f = Function::new("slen".to_string(), IrType::I64, PhpType::Int);
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let s = b.emit_const_str(s_id);
            let len = b
                .emit(Op::StrLen, vec![s], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::Return { value: Some(len) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_slen", &[]) {
            assert_eq!(o, "6");
        }
    }

    // ----- P2: scalar instruction lowering, observed via wasmer --invoke -----

    /// Returns whether the `wasmer` CLI is available.
    fn wasmer_available() -> bool {
        std::process::Command::new("wasmer")
            .arg("--version")
            .output()
            .is_ok()
    }

    /// Builds a single non-main function taking `nparams` i64 parameters. The
    /// `body` closure receives the loaded parameter values and returns the value
    /// the function returns. This mirrors real EIR: parameters are local slots
    /// accessed via `LoadLocal`.
    fn make_fn(
        name: &str,
        nparams: usize,
        ret_ir: IrType,
        ret_php: PhpType,
        body: impl FnOnce(&mut Builder, &[ValueId]) -> ValueId,
    ) -> Module {
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new(name.to_string(), ret_ir, ret_php);
        let mut slots = Vec::new();
        for i in 0..nparams {
            f.params.push(FunctionParam {
                name: format!("p{i}"),
                ir_type: IrType::I64,
                php_type: PhpType::Int,
                by_ref: false,
                variadic: false,
            });
            slots.push(f.add_local(
                Some(format!("p{i}")),
                IrType::I64,
                PhpType::Int,
                LocalKind::PhpLocal,
            ));
        }
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let loaded: Vec<ValueId> = slots
                .iter()
                .map(|s| b.emit_load_local(*s, IrType::I64, PhpType::Int))
                .collect();
            let result = body(&mut b, &loaded);
            b.terminate(Terminator::Return { value: Some(result) });
        }
        module.add_function(f);
        module
    }

    /// Generates and validates the module's wasm, then invokes `export` under
    /// `wasmer` with `args`, returning the trimmed stdout. Validation always runs;
    /// the run is skipped (returns `None`) when `wasmer` is absent.
    fn invoke(module: &Module, export: &str, args: &[&str]) -> Option<String> {
        let wat = generate(module, Emit::Executable).expect("module should lower");
        let bytes = assemble_and_validate(&wat);
        if !wasmer_available() {
            return None;
        }
        let dir = unique_tmp_dir("inv");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("m.wasm");
        std::fs::write(&path, &bytes).expect("write wasm");
        let mut cmd = std::process::Command::new("wasmer");
        cmd.arg("run").arg("--invoke").arg(export).arg(&path);
        for a in args {
            cmd.arg(a);
        }
        let out = cmd.output().expect("run wasmer");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            out.status.success(),
            "wasmer --invoke {export} failed: {}\n{}",
            String::from_utf8_lossy(&out.stderr),
            wat
        );
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Builds a two-i64-parameter function applying one EIR binary op to the args.
    fn int_binop_fn(name: &str, op: Op) -> Module {
        make_fn(name, 2, IrType::I64, PhpType::Int, |b, p| {
            b.emit(op, vec![p[0], p[1]], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .expect("binop produces a value")
        })
    }

    /// Verifies integer add/sub/mul/and/intdiv/mod compute correct values.
    #[test]
    fn int_arithmetic_invokes_correctly() {
        // Validation always runs inside invoke(); value checks run under wasmer.
        if let Some(o) = invoke(&int_binop_fn("add", Op::IAdd), "fn_add", &["10", "7"]) {
            assert_eq!(o, "17");
        }
        if let Some(o) = invoke(&int_binop_fn("sub", Op::ISub), "fn_sub", &["10", "7"]) {
            assert_eq!(o, "3");
        }
        if let Some(o) = invoke(&int_binop_fn("mul", Op::IMul), "fn_mul", &["6", "7"]) {
            assert_eq!(o, "42");
        }
        if let Some(o) = invoke(&int_binop_fn("band", Op::IBitAnd), "fn_band", &["6", "3"]) {
            assert_eq!(o, "2");
        }
        if let Some(o) = invoke(&int_binop_fn("idiv", Op::ISDiv), "fn_idiv", &["17", "5"]) {
            assert_eq!(o, "3");
        }
        if let Some(o) = invoke(&int_binop_fn("imod", Op::ISMod), "fn_imod", &["17", "5"]) {
            assert_eq!(o, "2");
        }
    }

    /// Verifies unary integer negation.
    #[test]
    fn int_neg_invokes_correctly() {
        let m = make_fn("neg", 1, IrType::I64, PhpType::Int, |b, p| {
            b.emit(Op::INeg, vec![p[0]], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap()
        });
        if let Some(o) = invoke(&m, "fn_neg", &["5"]) {
            assert_eq!(o, "-5");
        }
    }

    /// Verifies a signed less-than comparison yields an i64 boolean (0/1).
    #[test]
    fn int_compare_invokes_correctly() {
        let lt = || {
            make_fn("lt", 2, IrType::I64, PhpType::Bool, |b, p| {
                b.emit(
                    Op::ICmp,
                    vec![p[0], p[1]],
                    Some(Immediate::CmpPredicate(CmpPredicate::Slt)),
                    IrType::I64,
                    PhpType::Bool,
                    Ownership::NonHeap,
                )
                .unwrap()
            })
        };
        if let Some(o) = invoke(&lt(), "fn_lt", &["3", "5"]) {
            assert_eq!(o, "1");
        }
        if let Some(o) = invoke(&lt(), "fn_lt", &["5", "3"]) {
            assert_eq!(o, "0");
        }
    }

    /// Verifies PHP `/` lowers to floating-point division (returns a float).
    #[test]
    fn php_division_returns_float() {
        let m = make_fn("div", 2, IrType::F64, PhpType::Float, |b, p| {
            b.emit(Op::IDiv, vec![p[0], p[1]], None, IrType::F64, PhpType::Float, Ownership::NonHeap)
                .unwrap()
        });
        if let Some(o) = invoke(&m, "fn_div", &["7", "2"]) {
            assert_eq!(o, "3.5");
        }
    }

    /// Verifies recursion: `fib(n) = n<2 ? n : fib(n-1)+fib(n-2)` lowers across
    /// multiple blocks with self-calls and computes fib(10) = 55 under wasmer.
    #[test]
    fn recursive_fib_invokes_correctly() {
        let mut module = Module::new(Target::wasm());
        // Intern the callee name into the module data pool so Op::Call can reference it.
        let fib_name = module.data.intern_function_name("fib");
        let mut f = Function::new("fib".to_string(), IrType::I64, PhpType::Int);
        f.params.push(FunctionParam {
            name: "n".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: false,
            variadic: false,
        });
        let slot_n = f.add_local(Some("n".to_string()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            let base = b.create_named_block("base", Vec::new());
            let recurse = b.create_named_block("recurse", Vec::new());
            b.set_entry(entry);

            b.position_at_end(entry);
            let n = b.emit_load_local(slot_n, IrType::I64, PhpType::Int);
            let two = b.emit_const_i64(2);
            let cond = b
                .emit(
                    Op::ICmp,
                    vec![n, two],
                    Some(Immediate::CmpPredicate(CmpPredicate::Slt)),
                    IrType::I64,
                    PhpType::Bool,
                    Ownership::NonHeap,
                )
                .unwrap();
            b.terminate(Terminator::CondBr {
                cond,
                then_target: base,
                then_args: Vec::new(),
                else_target: recurse,
                else_args: Vec::new(),
            });

            b.position_at_end(base);
            b.terminate(Terminator::Return { value: Some(n) });

            b.position_at_end(recurse);
            let one = b.emit_const_i64(1);
            let nm1 = b
                .emit(Op::ISub, vec![n, one], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            let r1 = b
                .emit(
                    Op::Call,
                    vec![nm1],
                    Some(Immediate::Data(fib_name)),
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                )
                .unwrap();
            let two2 = b.emit_const_i64(2);
            let nm2 = b
                .emit(Op::ISub, vec![n, two2], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            let r2 = b
                .emit(
                    Op::Call,
                    vec![nm2],
                    Some(Immediate::Data(fib_name)),
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                )
                .unwrap();
            let sum = b
                .emit(Op::IAdd, vec![r1, r2], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::Return { value: Some(sum) });
        }
        module.add_function(f);

        if let Some(o) = invoke(&module, "fn_fib", &["10"]) {
            assert_eq!(o, "55");
        }
    }

    /// Verifies an instruction-bearing block now lowers (it previously errored as a
    /// stub): `IAdd` of two constants validates as real wasm.
    #[test]
    fn const_add_lowers_and_validates() {
        let m = make_fn("c", 0, IrType::I64, PhpType::Int, |b, _| {
            let a = b.emit_const_i64(40);
            let c = b.emit_const_i64(2);
            b.emit(Op::IAdd, vec![a, c], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap()
        });
        if let Some(o) = invoke(&m, "fn_c", &[]) {
            assert_eq!(o, "42");
        }
    }
}
