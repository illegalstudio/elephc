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

mod arrays;
mod context;
mod float;
mod function;
mod hashes;
mod heap;
mod inst;
mod inst_hash;
mod mixed;
mod objects;
mod refcount;
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
    // Import-free runtime (concat buffer + cursor) is needed by every module.
    runtime::emit_common_runtime(&mut wm);
    let has_main = module.functions.iter().any(|f| f.flags.is_main);
    if has_main {
        runtime::emit_command_runtime(&mut wm);
    }

    // Lay out every interned string literal as a data segment above the runtime
    // scratch region, recording (offset, byte_len) per DataId for ConstStr. The
    // float<->string scratch region sits between the concat buffer and the string
    // literals so a strtod/ftoa never runs through an in-flight concatenation.
    let mut str_literals: Vec<(u32, u32)> = Vec::with_capacity(module.data.strings.len());
    let mut cursor = runtime::RT_SCRATCH_END + runtime::FLOAT_SCRATCH_SIZE;
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

    // Emit the per-class gc_desc data (one runtime tag byte per property) plus the
    // class-indexed pointer table and the `$__gc_desc_ptrs` / `$__gc_desc_count` globals,
    // advancing the static-data cursor. This must land before `heap_base` is computed so
    // the descriptor data sits in static memory below the heap and is never overwritten by
    // allocation. `__rt_decref_object` walks these descriptors to release refcounted
    // property values before freeing an object at refcount zero.
    cursor = objects::emit_gc_desc_table(&mut wm, &module.class_infos, cursor);

    // The heap begins 16-aligned just above the string/data region; reserve two
    // pages of initial headroom above it. The bump allocator grows beyond
    // `heap_end` with `memory.grow` when this region is exhausted.
    const PAGE: u32 = 65536;
    let heap_base = (cursor + 15) & !15;
    let pages = (heap_base / PAGE) + 2;
    let heap_end = pages * PAGE;
    wm.set_memory(pages, Some("memory"));
    heap::emit_heap_runtime(&mut wm, heap_base, heap_end);
    refcount::emit_refcount_runtime(&mut wm);
    // Object refcount runtime: `__rt_decref_object`, called from `__rt_decref_any`
    // kind-4. P6b performs the full gc_desc-driven property walk + `__rt_heap_free`.
    objects::emit_object_runtime(&mut wm);
    arrays::emit_array_runtime(&mut wm);
    mixed::emit_mixed_runtime(&mut wm);
    hashes::emit_hash_runtime(&mut wm);
    // Float<->string runtime (ftoa + strtod). Published with the `$__float_scratch`
    // global set to `FLOAT_SCRATCH_BASE` so cast/echo/mixed-stdout callers pass
    // `(global.get $__float_scratch)` as the bignum scratch base.
    float::emit_float_runtime(&mut wm, runtime::FLOAT_SCRATCH_BASE as i32);

    // Lower every user function; `main` becomes the WASI `_start` command entry.
    for func in &module.functions {
        let fb = function::lower_function(module, func, &str_literals)?;
        wm.add_func(fb);
    }

    // Lower every class method (instance + static), so `__construct` and other
    // methods become callable WAT functions. Reuses the same lowering as user
    // functions: a non-static method's hidden leading `this` param is just param 0
    // (`IrType::Heap(Object)` -> `WasmRepr::Ptr` / i32), and the body uses the
    // already-supported `PropGet`/`PropSet`/`LoadLocal("this")`/`EchoValue` ops. WAT
    // `call $<name>` resolves a module-local function regardless of definition
    // order, so a `module.functions` entry calling `__construct` (via `ObjectNew`)
    // sees the method defined here even though methods are lowered after it.
    for func in &module.class_methods {
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
        Builder, CmpPredicate, DataId, Function, FunctionParam, Immediate, IrHeapKind, IrType,
        LocalKind, Module, Op, Ownership, Terminator, ValueId,
    };
    use crate::parser::ast::{Expr, ExprKind};
    use crate::span::Span;
    use crate::types::{ClassInfo, FunctionSig, PhpType};
    use std::collections::{HashMap, HashSet};

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

    /// Verifies `echo` of floats writes correct `%.14G` text to stdout via the
    /// `__rt_echo_f64` runtime helper, covering fractional, integer-valued, zero,
    /// negative, INF, and NAN floats. Each value's text was verified against `php -r`
    /// by the ftoa suite (S4) and the mixed cast_string tests (S6d); this test
    /// exercises the scalar-float `EchoValue` lowering + `__rt_echo_f64` glue
    /// (`f64.reinterpret_i64` -> `__rt_ftoa` -> `fd_write`).
    #[test]
    fn echo_float_writes_to_stdout() {
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            // (value, PHP %.14G text) — all confirmed against `php -r`.
            for v in [1.9_f64, 100.0, 0.0, -1.5, f64::INFINITY, f64::NAN] {
                let c = b.emit_const_f64(v);
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
            assert_eq!(out, "1.91000-1.5INFNAN");
        }
    }

    /// Verifies `echo` of a Mixed cell holding a float routes through the tag-2 arm
    /// of `__rt_mixed_write_stdout` (wired to `__rt_echo_f64`) and writes `%.14G`
    /// text. `MixedBox` of a float operand stamps runtime tag 2, so this exercises
    /// the previously-deferred mixed-float stdout path end to end.
    #[test]
    fn echo_mixed_float_writes_to_stdout() {
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let flt = b.emit_const_f64(1.9);
            let m = b
                .emit(
                    Op::MixedBox,
                    vec![flt],
                    None,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                )
                .unwrap();
            let _ = b.emit(
                Op::EchoValue,
                vec![m],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        if let Some(out) = run_main(&module) {
            assert_eq!(out, "1.9");
        }
    }

    /// Verifies `HashSet` of a Mixed cell holding a float into an INT-typed hash casts
    /// via `__rt_mixed_cast_int` (S6f): `(int)9.5` truncates toward zero to 9 and
    /// `(int)7.7` to 7, so `$h[1]+h[2]` echoes "16". A missing cast would mis-store the
    /// f64 bits as an int and echo a huge value, not "16".
    #[test]
    fn hash_set_mixed_int_cast_lowers() {
        let assoc = PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(PhpType::Int),
        };
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        let slot = f.add_local(
            Some("h".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            assoc.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(2)),
                    IrType::Heap(IrHeapKind::Hash),
                    assoc.clone(),
                    Ownership::Owned,
                )
                .unwrap();
            b.emit_store_local(slot, hash);
            for (k, v) in [(1_i64, 9.5_f64), (2, 7.7)] {
                let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
                let key = b.emit_const_i64(k);
                let fv = b.emit_const_f64(v);
                let m = b
                    .emit(
                        Op::MixedBox,
                        vec![fv],
                        None,
                        IrType::Heap(IrHeapKind::Mixed),
                        PhpType::Mixed,
                        Ownership::Owned,
                    )
                    .unwrap();
                let _ = b.emit(
                    Op::HashSet,
                    vec![h, key, m],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            }
            let h1 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let k1 = b.emit_const_i64(1);
            let g1 = b
                .emit(Op::HashGet, vec![h1, k1], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            let h2 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let k2 = b.emit_const_i64(2);
            let g2 = b
                .emit(Op::HashGet, vec![h2, k2], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            let sum = b
                .emit(Op::IAdd, vec![g1, g2], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            let _ = b.emit(
                Op::EchoValue,
                vec![sum],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        if let Some(out) = run_main(&module) {
            assert_eq!(out, "16");
        }
    }

    /// Verifies `HashSet` of a Mixed cell holding an int into a FLOAT-typed hash casts
    /// via `__rt_mixed_cast_float` (S6f): `(float)7` -> 7.0, and `7.0 / 2.0` echoes
    /// "3.5" — a non-integer only a correct f64 widening can produce. Forwarding the
    /// raw int bits as f64 would render a subnormal ("0"/"3e-322"), not "3.5".
    #[test]
    fn hash_set_mixed_float_cast_lowers() {
        let assoc = PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(PhpType::Float),
        };
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        let slot = f.add_local(
            Some("h".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            assoc.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(1)),
                    IrType::Heap(IrHeapKind::Hash),
                    assoc.clone(),
                    Ownership::Owned,
                )
                .unwrap();
            b.emit_store_local(slot, hash);
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key = b.emit_const_i64(1);
            let iv = b.emit_const_i64(7);
            let m = b
                .emit(
                    Op::MixedBox,
                    vec![iv],
                    None,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                )
                .unwrap();
            let _ = b.emit(
                Op::HashSet,
                vec![h, key, m],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let h1 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let k1 = b.emit_const_i64(1);
            let g = b
                .emit(
                    Op::HashGet,
                    vec![h1, k1],
                    None,
                    IrType::F64,
                    PhpType::Float,
                    Ownership::NonHeap,
                )
                .unwrap();
            let two = b.emit_const_f64(2.0);
            let half = b
                .emit(Op::FDiv, vec![g, two], None, IrType::F64, PhpType::Float, Ownership::NonHeap)
                .unwrap();
            let _ = b.emit(
                Op::EchoValue,
                vec![half],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        if let Some(out) = run_main(&module) {
            assert_eq!(out, "3.5");
        }
    }

    /// Verifies `HashSet` of a Mixed cell holding an int into a STRING-typed hash casts
    /// via `__rt_mixed_cast_string_ref` (S6f): `(string)42` -> "42". The borrowed cast
    /// result is persisted once by `__rt_hash_set`, so the cast's no-persist variant
    /// avoids the double-persist leak that the always-persisting `__rt_mixed_cast_string`
    /// would cause. Equivalent to `$h[1]=(string)(mixed)42; echo $h[1];` -> "42".
    #[test]
    fn hash_set_mixed_string_cast_lowers() {
        let assoc = PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(PhpType::Str),
        };
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        let slot = f.add_local(
            Some("h".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            assoc.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(1)),
                    IrType::Heap(IrHeapKind::Hash),
                    assoc.clone(),
                    Ownership::Owned,
                )
                .unwrap();
            b.emit_store_local(slot, hash);
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key = b.emit_const_i64(1);
            let iv = b.emit_const_i64(42);
            let m = b
                .emit(
                    Op::MixedBox,
                    vec![iv],
                    None,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                )
                .unwrap();
            let _ = b.emit(
                Op::HashSet,
                vec![h, key, m],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let h1 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let k1 = b.emit_const_i64(1);
            let g = b
                .emit(
                    Op::HashGet,
                    vec![h1, k1],
                    None,
                    IrType::Str,
                    PhpType::Str,
                    Ownership::MaybeOwned,
                )
                .unwrap();
            let _ = b.emit(
                Op::EchoValue,
                vec![g],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        if let Some(out) = run_main(&module) {
            assert_eq!(out, "42");
        }
    }

    /// Generates and validates a command module, runs it under `wasmer`, and
    /// returns its process exit code (not asserting success). Returns `None` when
    /// `wasmer` is absent.
    fn run_main_exit_code(module: &Module) -> Option<i32> {
        let wat = generate(module, Emit::Executable).expect("module should lower");
        let bytes = assemble_and_validate(&wat);
        if !wasmer_available() {
            return None;
        }
        let dir = unique_tmp_dir("exit");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("m.wasm");
        std::fs::write(&path, &bytes).expect("write wasm");
        let out = std::process::Command::new("wasmer")
            .arg("run")
            .arg(&path)
            .output()
            .expect("run wasmer");
        let _ = std::fs::remove_dir_all(&dir);
        out.status.code()
    }

    /// Like `run_main`, but passes `args` to the program. Returns trimmed stdout,
    /// or `None` when `wasmer` is absent.
    fn run_main_with_args(module: &Module, args: &[&str]) -> Option<String> {
        let wat = generate(module, Emit::Executable).expect("module should lower");
        let bytes = assemble_and_validate(&wat);
        if !wasmer_available() {
            return None;
        }
        let dir = unique_tmp_dir("args");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("m.wasm");
        std::fs::write(&path, &bytes).expect("write wasm");
        let mut cmd = std::process::Command::new("wasmer");
        cmd.arg("run").arg(&path).arg("--");
        for a in args {
            cmd.arg(a);
        }
        let out = cmd.output().expect("run wasmer");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            out.status.success(),
            "wasmer run failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Verifies `echo $argc` reports the process argument count (script + args),
    /// via the `__rt_argc` runtime over WASI `args_sizes_get`.
    #[test]
    fn argc_reports_argument_count() {
        let mut module = Module::new(Target::wasm());
        let argc_name = module.data.intern_global_name("argc");
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let argc = b
                .emit(
                    Op::LoadGlobal,
                    Vec::new(),
                    Some(Immediate::GlobalName(argc_name)),
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                )
                .unwrap();
            let _ = b.emit(
                Op::EchoValue,
                vec![argc],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        // script + two args = 3.
        if let Some(out) = run_main_with_args(&module, &["foo", "bar"]) {
            assert_eq!(out, "3");
        }
    }

    /// Verifies `exit($code)` lowers to WASI `proc_exit` with the integer status.
    #[test]
    fn exit_with_code_sets_process_status() {
        let mut module = Module::new(Target::wasm());
        let exit_name = module.data.intern_function_name("exit");
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let code = b.emit_const_i64(3);
            let _ = b.emit(
                Op::BuiltinCall,
                vec![code],
                Some(Immediate::Data(exit_name)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        if let Some(code) = run_main_exit_code(&module) {
            assert_eq!(code, 3);
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

    /// Verifies chained string concatenation `"Hello, " . "WASM" . "!"` produces the
    /// correct bytes — exercising the concat buffer + `__rt_concat` and proving the
    /// result pointer addresses the freshly-assembled region after two appends.
    #[test]
    fn chained_concat_echoes_correctly() {
        let mut module = Module::new(Target::wasm());
        let s1 = module.data.intern_string("Hello, ");
        let s2 = module.data.intern_string("WASM");
        let s3 = module.data.intern_string("!");
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let v1 = b.emit_const_str(s1);
            let v2 = b.emit_const_str(s2);
            let c12 = b
                .emit(Op::StrConcat, vec![v1, v2], None, IrType::Str, PhpType::Str, Ownership::Borrowed)
                .unwrap();
            let v3 = b.emit_const_str(s3);
            let c123 = b
                .emit(Op::StrConcat, vec![c12, v3], None, IrType::Str, PhpType::Str, Ownership::Borrowed)
                .unwrap();
            let _ = b.emit(
                Op::EchoValue,
                vec![c123],
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

    // ----- P5b: ownership lowering (Acquire / Release / Move) -----

    /// Verifies `Op::Acquire` of a string literal persists it into an owned heap
    /// copy (`__rt_str_persist`) whose bytes echo back correctly.
    #[test]
    fn acquire_string_persists_and_echoes() {
        let mut module = Module::new(Target::wasm());
        let hello = module.data.intern_string("hi there");
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let lit = b.emit_const_str(hello);
            let owned = b
                .emit(Op::Acquire, vec![lit], None, IrType::Str, PhpType::Str, Ownership::Owned)
                .unwrap();
            let _ = b.emit(
                Op::EchoValue,
                vec![owned],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        if let Some(out) = run_main(&module) {
            assert_eq!(out, "hi there");
        }
    }

    /// Verifies the full owned-string lifecycle — `Acquire` (persist to heap),
    /// `EchoValue`, then `Release` (free via `__rt_heap_free_safe`) — echoes the
    /// content and exits cleanly (a corrupt/double free would trap under wasmer).
    #[test]
    fn acquire_echo_release_string_roundtrip() {
        let mut module = Module::new(Target::wasm());
        let s = module.data.intern_string("bye");
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let lit = b.emit_const_str(s);
            let owned = b
                .emit(Op::Acquire, vec![lit], None, IrType::Str, PhpType::Str, Ownership::Owned)
                .unwrap();
            let _ = b.emit(
                Op::EchoValue,
                vec![owned],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let _ = b.emit(
                Op::Release,
                vec![owned],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        if let Some(out) = run_main(&module) {
            assert_eq!(out, "bye");
        }
    }

    /// Verifies `Op::Move` forwards a scalar value unchanged (no refcount work).
    #[test]
    fn move_forwards_int_value() {
        let m = make_fn("mv", 1, IrType::I64, PhpType::Int, |b, p| {
            b.emit(Op::Move, vec![p[0]], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap()
        });
        if let Some(o) = invoke(&m, "fn_mv", &["42"]) {
            assert_eq!(o, "42");
        }
    }

    // ----- P5c: indexed-array lowering (ArrayNew / ArrayPush / ArrayLen / ArrayGet) -----

    /// Builds an indexed array `[10, 20, 30]` (ArrayNew + three ArrayPush) reusing
    /// the same array value, then returns `$a[1]` via ArrayGet — verifying the
    /// push writeback and the bounded getter through the full lowering.
    #[test]
    fn array_new_push_get_lowers() {
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("a".to_string(), IrType::I64, PhpType::Int);
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let arr = b
                .emit(
                    Op::ArrayNew,
                    Vec::new(),
                    Some(Immediate::Capacity(4)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Int)),
                    Ownership::Owned,
                )
                .unwrap();
            for v in [10_i64, 20, 30] {
                let c = b.emit_const_i64(v);
                let _ = b.emit(
                    Op::ArrayPush,
                    vec![arr, c],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            }
            let idx = b.emit_const_i64(1);
            let g = b
                .emit(Op::ArrayGet, vec![arr, idx], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::Return { value: Some(g) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_a", &[]) {
            assert_eq!(o, "20");
        }
    }

    /// Verifies `ArrayLen` reads the element count after three pushes (= 3).
    #[test]
    fn array_len_lowers() {
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("n".to_string(), IrType::I64, PhpType::Int);
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let arr = b
                .emit(
                    Op::ArrayNew,
                    Vec::new(),
                    Some(Immediate::Capacity(4)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Int)),
                    Ownership::Owned,
                )
                .unwrap();
            for v in [1_i64, 2, 3] {
                let c = b.emit_const_i64(v);
                let _ = b.emit(
                    Op::ArrayPush,
                    vec![arr, c],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            }
            let len = b
                .emit(Op::ArrayLen, vec![arr], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::Return { value: Some(len) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_n", &[]) {
            assert_eq!(o, "3");
        }
    }

    /// Verifies the slot writeback: a zero-capacity array stored in a local slot,
    /// loaded, pushed (forcing a reallocation), then re-loaded from the SAME slot
    /// still reads the pushed element — proving `ArrayPush` mirrors the new pointer
    /// back to the variable's slot, not just the loaded SSA value.
    #[test]
    fn array_push_writes_back_to_slot_after_realloc() {
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("s".to_string(), IrType::I64, PhpType::Int);
        let slot = f.add_local(
            Some("a".to_string()),
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let arr0 = b
                .emit(
                    Op::ArrayNew,
                    Vec::new(),
                    Some(Immediate::Capacity(0)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Int)),
                    Ownership::Owned,
                )
                .unwrap();
            b.emit_store_local(slot, arr0);
            let a1 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Int)));
            let c = b.emit_const_i64(77);
            let _ = b.emit(
                Op::ArrayPush,
                vec![a1, c],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let a2 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Int)));
            let idx = b.emit_const_i64(0);
            let g = b
                .emit(Op::ArrayGet, vec![a2, idx], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::Return { value: Some(g) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_s", &[]) {
            assert_eq!(o, "77");
        }
    }

    /// Verifies `$a[1] = 99` through the full `ArraySet` lowering: an array
    /// `[10, 20, 30]` is stored in a local slot, the element is overwritten via
    /// `Op::ArraySet`, then reloaded from the SAME slot and read back — proving the
    /// setter mutates in place and the returned pointer is mirrored to the slot.
    #[test]
    fn array_set_overwrite_lowers() {
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("s".to_string(), IrType::I64, PhpType::Int);
        let slot = f.add_local(
            Some("a".to_string()),
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let arr = b
                .emit(
                    Op::ArrayNew,
                    Vec::new(),
                    Some(Immediate::Capacity(4)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Int)),
                    Ownership::Owned,
                )
                .unwrap();
            for v in [10_i64, 20, 30] {
                let c = b.emit_const_i64(v);
                let _ = b.emit(
                    Op::ArrayPush,
                    vec![arr, c],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            }
            b.emit_store_local(slot, arr);
            let a1 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Int)));
            let idx = b.emit_const_i64(1);
            let val = b.emit_const_i64(99);
            let _ = b.emit(
                Op::ArraySet,
                vec![a1, idx, val],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let a2 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Int)));
            let idx2 = b.emit_const_i64(1);
            let g = b
                .emit(Op::ArrayGet, vec![a2, idx2], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::Return { value: Some(g) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_s", &[]) {
            assert_eq!(o, "99");
        }
    }

    /// Verifies `$a[3] = 77` on a short array extends it via the `ArraySet`
    /// lowering: setting past the end grows + gap-fills, so reloading from the slot
    /// and reading the length yields 4.
    #[test]
    fn array_set_extends_lowers() {
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("e".to_string(), IrType::I64, PhpType::Int);
        let slot = f.add_local(
            Some("a".to_string()),
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let arr = b
                .emit(
                    Op::ArrayNew,
                    Vec::new(),
                    Some(Immediate::Capacity(2)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Int)),
                    Ownership::Owned,
                )
                .unwrap();
            b.emit_store_local(slot, arr);
            let a1 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Int)));
            let idx = b.emit_const_i64(3);
            let val = b.emit_const_i64(77);
            let _ = b.emit(
                Op::ArraySet,
                vec![a1, idx, val],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let a2 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Int)));
            let len = b
                .emit(Op::ArrayLen, vec![a2], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::Return { value: Some(len) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_e", &[]) {
            assert_eq!(o, "4");
        }
    }

    /// Builds the int-keyed associative-array type used by the hash lowering tests.
    fn int_hash_type() -> PhpType {
        PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(PhpType::Int),
        }
    }

    /// Verifies `$h[7] = 100; $h[13] = 200; return $h[7];` through the full
    /// `HashNew`/`HashSet`/`HashGet` lowering: a fresh hash is stored in a slot,
    /// two int-keyed entries are inserted (each via a reload from the SAME slot so the
    /// write-back is exercised), then one is read back — proving the runtime stores
    /// and retrieves ordered-map entries through compiled code.
    #[test]
    fn hash_set_get_int_lowers() {
        let assoc = int_hash_type();
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("s".to_string(), IrType::I64, PhpType::Int);
        let slot = f.add_local(
            Some("h".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            assoc.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(2)),
                    IrType::Heap(IrHeapKind::Hash),
                    assoc.clone(),
                    Ownership::Owned,
                )
                .unwrap();
            b.emit_store_local(slot, hash);
            for (k, v) in [(7_i64, 100_i64), (13, 200)] {
                let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
                let key = b.emit_const_i64(k);
                let val = b.emit_const_i64(v);
                let _ = b.emit(
                    Op::HashSet,
                    vec![h, key, val],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            }
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key = b.emit_const_i64(7);
            let g = b
                .emit(Op::HashGet, vec![h, key], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::Return { value: Some(g) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_s", &[]) {
            assert_eq!(o, "100");
        }
    }

    /// Verifies a `HashGet` on an absent key yields the PHP null sentinel
    /// (`0x7fff_ffff_ffff_fffe`): `$h[7] = 100; return $h[99];`. The runtime miss path
    /// returns `(found=0, ...)` and the lowering `select`s the sentinel.
    #[test]
    fn hash_get_miss_returns_null_sentinel() {
        let assoc = int_hash_type();
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("m".to_string(), IrType::I64, PhpType::Int);
        let slot = f.add_local(
            Some("h".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            assoc.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(2)),
                    IrType::Heap(IrHeapKind::Hash),
                    assoc.clone(),
                    Ownership::Owned,
                )
                .unwrap();
            b.emit_store_local(slot, hash);
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key = b.emit_const_i64(7);
            let val = b.emit_const_i64(100);
            let _ = b.emit(
                Op::HashSet,
                vec![h, key, val],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let h2 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let miss = b.emit_const_i64(99);
            let g = b
                .emit(Op::HashGet, vec![h2, miss], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::Return { value: Some(g) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_m", &[]) {
            assert_eq!(o, "9223372036854775806");
        }
    }

    /// Builds the int-keyed, bool-valued associative-array type used by the
    /// mixed→concrete-storage cast tests (`array<int, bool>`).
    fn bool_hash_type() -> PhpType {
        PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(PhpType::Bool),
        }
    }

    /// Verifies `HashSet` of a boxed Mixed value into a concretely BOOL-typed hash
    /// casts at runtime via `__rt_mixed_cast_bool` (P5d-2c): a Mixed cell holding the
    /// int 5 stores `true`, one holding 0 stores `false`. Equivalent to
    /// `$h[1] = (bool)$m5; $h[2] = (bool)$m0; return $h[1]*10 + $h[2];` -> 10. Without
    /// the cast the lowering would mis-tag the Mixed-cell pointer as an inline scalar.
    #[test]
    fn hash_set_mixed_bool_cast_lowers() {
        let assoc = bool_hash_type();
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("c".to_string(), IrType::I64, PhpType::Int);
        let slot = f.add_local(
            Some("h".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            assoc.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(2)),
                    IrType::Heap(IrHeapKind::Hash),
                    assoc.clone(),
                    Ownership::Owned,
                )
                .unwrap();
            b.emit_store_local(slot, hash);
            for (k, raw) in [(1_i64, 5_i64), (2, 0)] {
                let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
                let key = b.emit_const_i64(k);
                let scalar = b.emit_const_i64(raw);
                let m = b
                    .emit(
                        Op::MixedBox,
                        vec![scalar],
                        None,
                        IrType::Heap(IrHeapKind::Mixed),
                        PhpType::Mixed,
                        Ownership::Owned,
                    )
                    .unwrap();
                let _ = b.emit(
                    Op::HashSet,
                    vec![h, key, m],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            }
            let h1 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let k1 = b.emit_const_i64(1);
            let g1 = b
                .emit(Op::HashGet, vec![h1, k1], None, IrType::I64, PhpType::Bool, Ownership::NonHeap)
                .unwrap();
            let h2 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let k2 = b.emit_const_i64(2);
            let g2 = b
                .emit(Op::HashGet, vec![h2, k2], None, IrType::I64, PhpType::Bool, Ownership::NonHeap)
                .unwrap();
            let ten = b.emit_const_i64(10);
            let scaled = b
                .emit(Op::IMul, vec![g1, ten], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            let sum = b
                .emit(Op::IAdd, vec![scaled, g2], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::Return { value: Some(sum) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_c", &[]) {
            assert_eq!(o, "10");
        }
    }

    /// Verifies `HashAppend` (`$h[] = v`) of a boxed Mixed value into a BOOL-typed hash
    /// also routes through the `__rt_mixed_cast_bool` cast (the same shared
    /// `materialize_hash_value_tagged` path as `HashSet`): appending a Mixed cell
    /// holding the string "x" stores `true` at int key 0. Reads it back -> 1.
    #[test]
    fn hash_append_mixed_bool_cast_lowers() {
        let assoc = bool_hash_type();
        let mut module = Module::new(Target::wasm());
        let x = module.data.intern_string("x");
        let mut f = Function::new("a".to_string(), IrType::I64, PhpType::Int);
        let slot = f.add_local(
            Some("h".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            assoc.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(1)),
                    IrType::Heap(IrHeapKind::Hash),
                    assoc.clone(),
                    Ownership::Owned,
                )
                .unwrap();
            b.emit_store_local(slot, hash);
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let s = b.emit_const_str(x);
            let m = b
                .emit(
                    Op::MixedBox,
                    vec![s],
                    None,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                )
                .unwrap();
            let _ = b.emit(
                Op::HashAppend,
                vec![h, m],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let h2 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let k0 = b.emit_const_i64(0);
            let g = b
                .emit(Op::HashGet, vec![h2, k0], None, IrType::I64, PhpType::Bool, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::Return { value: Some(g) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_a", &[]) {
            assert_eq!(o, "1");
        }
    }

    /// Verifies overwriting an existing key updates in place and does not grow the
    /// table: `$h[7] = 100; $h[7] = 999; return $h[7];` -> 999. Exercises the
    /// `__rt_hash_set` update-on-match branch through the lowering.
    #[test]
    fn hash_overwrite_updates_in_place() {
        let assoc = int_hash_type();
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("o".to_string(), IrType::I64, PhpType::Int);
        let slot = f.add_local(
            Some("h".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            assoc.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(2)),
                    IrType::Heap(IrHeapKind::Hash),
                    assoc.clone(),
                    Ownership::Owned,
                )
                .unwrap();
            b.emit_store_local(slot, hash);
            for v in [100_i64, 999] {
                let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
                let key = b.emit_const_i64(7);
                let val = b.emit_const_i64(v);
                let _ = b.emit(
                    Op::HashSet,
                    vec![h, key, val],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            }
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key = b.emit_const_i64(7);
            let g = b
                .emit(Op::HashGet, vec![h, key], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::Return { value: Some(g) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_o", &[]) {
            assert_eq!(o, "999");
        }
    }

    /// Verifies the load-factor resize+rehash path: inserting eight sparse int-keyed
    /// entries into a capacity-2 hash forces `__rt_hash_resize`, then reading one of the
    /// earlier keys back proves the rehash preserved every entry. `$h[i] = i*10` for
    /// i in 1..=8; `return $h[5];` -> 50.
    #[test]
    fn hash_resize_preserves_entries() {
        let assoc = int_hash_type();
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("r".to_string(), IrType::I64, PhpType::Int);
        let slot = f.add_local(
            Some("h".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            assoc.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(2)),
                    IrType::Heap(IrHeapKind::Hash),
                    assoc.clone(),
                    Ownership::Owned,
                )
                .unwrap();
            b.emit_store_local(slot, hash);
            for i in 1_i64..=8 {
                let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
                let key = b.emit_const_i64(i);
                let val = b.emit_const_i64(i * 10);
                let _ = b.emit(
                    Op::HashSet,
                    vec![h, key, val],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            }
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key = b.emit_const_i64(5);
            let g = b
                .emit(Op::HashGet, vec![h, key], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::Return { value: Some(g) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_r", &[]) {
            assert_eq!(o, "50");
        }
    }

    /// Verifies a string-keyed, Mixed-valued hash round-trips a string value:
    /// `$h["name"] = "Bob"; echo $h["name"];` -> "Bob". Exercises string-key
    /// materialization (`__rt_hash_normalize_key` keeps "name" a string key), a borrowed
    /// string value persisted by `__rt_hash_set`, and a box-on-read Mixed result echoed
    /// through the Mixed writer.
    #[test]
    fn hash_string_key_mixed_value_echoes() {
        let assoc = PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Mixed),
        };
        let mut module = Module::new(Target::wasm());
        let name = module.data.intern_string("name");
        let bob = module.data.intern_string("Bob");
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        let slot = f.add_local(
            Some("h".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            assoc.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(2)),
                    IrType::Heap(IrHeapKind::Hash),
                    assoc.clone(),
                    Ownership::Owned,
                )
                .unwrap();
            b.emit_store_local(slot, hash);
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key = b.emit_const_str(name);
            let val = b.emit_const_str(bob);
            let _ = b.emit(
                Op::HashSet,
                vec![h, key, val],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let h2 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key2 = b.emit_const_str(name);
            let g = b
                .emit(
                    Op::HashGet,
                    vec![h2, key2],
                    None,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::MaybeOwned,
                )
                .unwrap();
            let _ = b.emit(
                Op::EchoValue,
                vec![g],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        if let Some(o) = run_main(&module) {
            assert_eq!(o, "Bob");
        }
    }

    /// Verifies an integer-like string key normalizes to the same int key:
    /// `$h["7"] = 100; return $h[7];` -> 100. The string key "7" and the int key 7 must
    /// hash and compare equal — `__rt_hash_normalize_key` collapses "7" to int 7.
    #[test]
    fn hash_intlike_string_key_normalizes() {
        let assoc = PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Int),
        };
        let mut module = Module::new(Target::wasm());
        let seven = module.data.intern_string("7");
        let mut f = Function::new("n".to_string(), IrType::I64, PhpType::Int);
        let slot = f.add_local(
            Some("h".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            assoc.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(2)),
                    IrType::Heap(IrHeapKind::Hash),
                    assoc.clone(),
                    Ownership::Owned,
                )
                .unwrap();
            b.emit_store_local(slot, hash);
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key = b.emit_const_str(seven); // string key "7"
            let val = b.emit_const_i64(100);
            let _ = b.emit(
                Op::HashSet,
                vec![h, key, val],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let h2 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let ikey = b.emit_const_i64(7); // int key 7 — must collide with "7"
            let g = b
                .emit(Op::HashGet, vec![h2, ikey], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::Return { value: Some(g) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_n", &[]) {
            assert_eq!(o, "100");
        }
    }

    /// Verifies a Mixed-valued hash read of an absent string key boxes to PHP null,
    /// which echoes as the empty string: `$h["a"] = "x"; echo $h["b"];` -> "". The miss
    /// path returns tag 8, so `__rt_mixed_from_value` produces a null cell.
    #[test]
    fn hash_mixed_read_miss_echoes_empty() {
        let assoc = PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Mixed),
        };
        let mut module = Module::new(Target::wasm());
        let a = module.data.intern_string("a");
        let x = module.data.intern_string("x");
        let bkey = module.data.intern_string("b");
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        let slot = f.add_local(
            Some("h".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            assoc.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(2)),
                    IrType::Heap(IrHeapKind::Hash),
                    assoc.clone(),
                    Ownership::Owned,
                )
                .unwrap();
            b.emit_store_local(slot, hash);
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key = b.emit_const_str(a);
            let val = b.emit_const_str(x);
            let _ = b.emit(
                Op::HashSet,
                vec![h, key, val],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let h2 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let miss = b.emit_const_str(bkey);
            let g = b
                .emit(
                    Op::HashGet,
                    vec![h2, miss],
                    None,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::MaybeOwned,
                )
                .unwrap();
            let _ = b.emit(
                Op::EchoValue,
                vec![g],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        if let Some(o) = run_main(&module) {
            assert_eq!(o, "");
        }
    }

    /// Verifies a string-keyed, string-valued hash round-trips through the owned-copy
    /// read path: `$h["k"] = "val"; echo $h["k"];` -> "val". The read returns a value the
    /// EIR marks `MaybeOwned`, so the lowering persists an owned copy via
    /// `__rt_str_persist` rather than aliasing the hash's stored reference.
    #[test]
    fn hash_string_value_owned_read_echoes() {
        let assoc = PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Str),
        };
        let mut module = Module::new(Target::wasm());
        let k = module.data.intern_string("k");
        let v = module.data.intern_string("val");
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        let slot = f.add_local(
            Some("h".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            assoc.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(2)),
                    IrType::Heap(IrHeapKind::Hash),
                    assoc.clone(),
                    Ownership::Owned,
                )
                .unwrap();
            b.emit_store_local(slot, hash);
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key = b.emit_const_str(k);
            let val = b.emit_const_str(v);
            let _ = b.emit(
                Op::HashSet,
                vec![h, key, val],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let h2 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key2 = b.emit_const_str(k);
            let g = b
                .emit(Op::HashGet, vec![h2, key2], None, IrType::Str, PhpType::Str, Ownership::MaybeOwned)
                .unwrap();
            let _ = b.emit(
                Op::EchoValue,
                vec![g],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        if let Some(o) = run_main(&module) {
            assert_eq!(o, "val");
        }
    }

    /// Verifies a hash whose values are indexed arrays round-trips a container through
    /// the increfing read path: `$h["a"] = [10, 20]; return $h["a"][1];` -> 20. The
    /// `HashGet` returns the stored array retained (owned), and `ArrayGet` then reads an
    /// element of it.
    #[test]
    fn hash_array_value_container_read() {
        let inner = PhpType::Array(Box::new(PhpType::Int));
        let assoc = PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(inner.clone()),
        };
        let mut module = Module::new(Target::wasm());
        let a = module.data.intern_string("a");
        let mut f = Function::new("c".to_string(), IrType::I64, PhpType::Int);
        let slot = f.add_local(
            Some("h".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            assoc.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(2)),
                    IrType::Heap(IrHeapKind::Hash),
                    assoc.clone(),
                    Ownership::Owned,
                )
                .unwrap();
            b.emit_store_local(slot, hash);
            let arr = b
                .emit(
                    Op::ArrayNew,
                    Vec::new(),
                    Some(Immediate::Capacity(2)),
                    IrType::Heap(IrHeapKind::Array),
                    inner.clone(),
                    Ownership::Owned,
                )
                .unwrap();
            for v in [10_i64, 20] {
                let c = b.emit_const_i64(v);
                let _ = b.emit(
                    Op::ArrayPush,
                    vec![arr, c],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            }
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key = b.emit_const_str(a);
            let _ = b.emit(
                Op::HashSet,
                vec![h, key, arr],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let h2 = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key2 = b.emit_const_str(a);
            let got = b
                .emit(
                    Op::HashGet,
                    vec![h2, key2],
                    None,
                    IrType::Heap(IrHeapKind::Array),
                    inner.clone(),
                    Ownership::MaybeOwned,
                )
                .unwrap();
            let idx = b.emit_const_i64(1);
            let g = b
                .emit(Op::ArrayGet, vec![got, idx], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::Return { value: Some(g) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_c", &[]) {
            assert_eq!(o, "20");
        }
    }

    /// Pushes a string literal into a string array, reads it back via ArrayGet,
    /// and echoes it — exercising `__rt_array_push_str` (persist) + `get_str`
    /// + `__rt_echo_str` through the full lowering.
    #[test]
    fn string_array_push_get_echo_lowers() {
        let mut module = Module::new(Target::wasm());
        let hello = module.data.intern_string("hello");
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let arr = b
                .emit(
                    Op::ArrayNew,
                    Vec::new(),
                    Some(Immediate::Capacity(2)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Str)),
                    Ownership::Owned,
                )
                .unwrap();
            let lit = b.emit_const_str(hello);
            let _ = b.emit(
                Op::ArrayPush,
                vec![arr, lit],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let idx = b.emit_const_i64(0);
            let g = b
                .emit(Op::ArrayGet, vec![arr, idx], None, IrType::Str, PhpType::Str, Ownership::Borrowed)
                .unwrap();
            let _ = b.emit(
                Op::EchoValue,
                vec![g],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        if let Some(o) = run_main(&module) {
            assert_eq!(o, "hello");
        }
    }

    /// Verifies `echo $argv[1]` reads the first command-line argument: `$argv` is
    /// built from WASI `args_get` into an indexed string array, indexed, and echoed.
    #[test]
    fn argv_index_one_echoes_first_arg() {
        let mut module = Module::new(Target::wasm());
        let argv_name = module.data.intern_global_name("argv");
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let argv = b
                .emit(
                    Op::LoadGlobal,
                    Vec::new(),
                    Some(Immediate::GlobalName(argv_name)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Str)),
                    Ownership::Owned,
                )
                .unwrap();
            let idx = b.emit_const_i64(1);
            let g = b
                .emit(Op::ArrayGet, vec![argv, idx], None, IrType::Str, PhpType::Str, Ownership::Borrowed)
                .unwrap();
            let _ = b.emit(
                Op::EchoValue,
                vec![g],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        // script + ["foo","bar"]; $argv[1] is the first user argument "foo".
        if let Some(o) = run_main_with_args(&module, &["foo", "bar"]) {
            assert_eq!(o, "foo");
        }
    }

    // ----- P5c-4: Mixed boxing (MixedBox + echo of a Mixed value) -----

    /// Helper: builds a `main` that boxes one operand value into a Mixed cell and
    /// echoes it. `build` returns the value to box.
    fn box_and_echo_module(build: impl FnOnce(&mut Builder) -> ValueId) -> Module {
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let v = build(&mut b);
            let m = b
                .emit(
                    Op::MixedBox,
                    vec![v],
                    None,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                )
                .unwrap();
            let _ = b.emit(
                Op::EchoValue,
                vec![m],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        module
    }

    /// Boxing an int and echoing the Mixed value prints the decimal integer.
    #[test]
    fn mixed_box_int_echoes() {
        let m = box_and_echo_module(|b| b.emit_const_i64(42));
        if let Some(o) = run_main(&m) {
            assert_eq!(o, "42");
        }
    }

    /// Boxing a string (persisted into the cell) and echoing prints the bytes.
    #[test]
    fn mixed_box_string_echoes() {
        let mut module = Module::new(Target::wasm());
        let yo = module.data.intern_string("yo");
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let v = b.emit_const_str(yo);
            let m = b
                .emit(Op::MixedBox, vec![v], None, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned)
                .unwrap();
            let _ = b.emit(Op::EchoValue, vec![m], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        if let Some(o) = run_main(&module) {
            assert_eq!(o, "yo");
        }
    }

    /// Boxing a `true` bool and echoing prints "1" (PHP bool echo semantics).
    #[test]
    fn mixed_box_bool_echoes() {
        let m = box_and_echo_module(|b| b.emit_const_bool(true));
        if let Some(o) = run_main(&m) {
            assert_eq!(o, "1");
        }
    }

    // ----- P5c-5: foreach over an indexed array -----

    /// Builds `foreach ([10,20,30] as $v) echo $v;` as the canonical foreach CFG
    /// (entry builds the array + IterStart; a header runs IterNext into a CondBr; a
    /// body reads IterCurrentValue as a Mixed and echoes it; an exit returns). The
    /// concatenated output is "102030".
    #[test]
    fn foreach_echoes_indexed_int_array() {
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            let header = b.create_named_block("header", Vec::new());
            let body = b.create_named_block("body", Vec::new());
            let exit = b.create_named_block("exit", Vec::new());
            b.set_entry(entry);

            b.position_at_end(entry);
            let arr = b
                .emit(
                    Op::ArrayNew,
                    Vec::new(),
                    Some(Immediate::Capacity(3)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Int)),
                    Ownership::Owned,
                )
                .unwrap();
            for v in [10_i64, 20, 30] {
                let c = b.emit_const_i64(v);
                let _ = b.emit(
                    Op::ArrayPush,
                    vec![arr, c],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            }
            let iter = b
                .emit(
                    Op::IterStart,
                    vec![arr],
                    None,
                    IrType::Heap(IrHeapKind::Iterable),
                    PhpType::Iterable,
                    Ownership::Borrowed,
                )
                .unwrap();
            b.terminate(Terminator::Br {
                target: header,
                args: Vec::new(),
            });

            b.position_at_end(header);
            let has_next = b
                .emit(Op::IterNext, vec![iter], None, IrType::I64, PhpType::Bool, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::CondBr {
                cond: has_next,
                then_target: body,
                then_args: Vec::new(),
                else_target: exit,
                else_args: Vec::new(),
            });

            b.position_at_end(body);
            let val = b
                .emit(
                    Op::IterCurrentValue,
                    vec![iter],
                    None,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                )
                .unwrap();
            let _ = b.emit(
                Op::EchoValue,
                vec![val],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Br {
                target: header,
                args: Vec::new(),
            });

            b.position_at_end(exit);
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        if let Some(o) = run_main(&module) {
            assert_eq!(o, "102030");
        }
    }

    // ----- P5d-3: foreach over an associative hash -----

    /// Emits the canonical foreach loop CFG (entry already positioned with `hash` built
    /// and three string keys inserted) that walks `hash`, emits `op` (IterCurrentKey or
    /// IterCurrentValue) as a Mixed in the body, and echoes it. Shared by the hash-foreach
    /// tests so each only differs in what it inserts and which current-op it reads.
    fn emit_hash_foreach_loop(b: &mut Builder, hash: ValueId, op: Op) {
        let header = b.create_named_block("header", Vec::new());
        let body = b.create_named_block("body", Vec::new());
        let exit = b.create_named_block("exit", Vec::new());
        let iter = b
            .emit(
                Op::IterStart,
                vec![hash],
                None,
                IrType::Heap(IrHeapKind::Iterable),
                PhpType::Iterable,
                Ownership::Borrowed,
            )
            .unwrap();
        b.terminate(Terminator::Br { target: header, args: Vec::new() });

        b.position_at_end(header);
        let has_next = b
            .emit(Op::IterNext, vec![iter], None, IrType::I64, PhpType::Bool, Ownership::NonHeap)
            .unwrap();
        b.terminate(Terminator::CondBr {
            cond: has_next,
            then_target: body,
            then_args: Vec::new(),
            else_target: exit,
            else_args: Vec::new(),
        });

        b.position_at_end(body);
        let cur = b
            .emit(op, vec![iter], None, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned)
            .unwrap();
        let _ = b.emit(Op::EchoValue, vec![cur], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
        b.terminate(Terminator::Br { target: header, args: Vec::new() });

        b.position_at_end(exit);
        b.terminate(Terminator::Return { value: None });
    }

    /// `foreach (["a"=>10, "b"=>20, "c"=>30] as $v) echo $v;` -> "102030". Exercises the
    /// hash iterator advancing in insertion order and the scalar-through-Mixed box-on-read
    /// value path (`IterCurrentValue`, tag 0).
    #[test]
    fn foreach_hash_int_values() {
        let assoc = PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Int),
        };
        let mut module = Module::new(Target::wasm());
        let keys: Vec<_> = ["a", "b", "c"].iter().map(|s| module.data.intern_string(s)).collect();
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(Op::HashNew, Vec::new(), Some(Immediate::Capacity(8)), IrType::Heap(IrHeapKind::Hash), assoc.clone(), Ownership::Owned)
                .unwrap();
            for (i, &k) in keys.iter().enumerate() {
                let key = b.emit_const_str(k);
                let val = b.emit_const_i64(((i as i64) + 1) * 10);
                let _ = b.emit(Op::HashSet, vec![hash, key, val], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            emit_hash_foreach_loop(&mut b, hash, Op::IterCurrentValue);
        }
        module.add_function(f);
        if let Some(o) = run_main(&module) {
            assert_eq!(o, "102030");
        }
    }

    /// `foreach (["a"=>"x", "b"=>"y", "c"=>"z"] as $k => $v) echo $k;` -> "abc". Exercises
    /// the string-key box-on-read path (`IterCurrentKey`) over the insertion-order walk.
    #[test]
    fn foreach_hash_string_keys() {
        let assoc = PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Str),
        };
        let mut module = Module::new(Target::wasm());
        let keys: Vec<_> = ["a", "b", "c"].iter().map(|s| module.data.intern_string(s)).collect();
        let vals: Vec<_> = ["x", "y", "z"].iter().map(|s| module.data.intern_string(s)).collect();
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(Op::HashNew, Vec::new(), Some(Immediate::Capacity(8)), IrType::Heap(IrHeapKind::Hash), assoc.clone(), Ownership::Owned)
                .unwrap();
            for (&k, &v) in keys.iter().zip(vals.iter()) {
                let key = b.emit_const_str(k);
                let val = b.emit_const_str(v);
                let _ = b.emit(Op::HashSet, vec![hash, key, val], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            emit_hash_foreach_loop(&mut b, hash, Op::IterCurrentKey);
        }
        module.add_function(f);
        if let Some(o) = run_main(&module) {
            assert_eq!(o, "abc");
        }
    }

    /// `foreach (["a"=>"x", "b"=>"y", "c"=>"z"] as $v) echo $v;` -> "xyz". Exercises the
    /// string-value persist path through box-on-read (`IterCurrentValue`, tag 1).
    #[test]
    fn foreach_hash_string_values() {
        let assoc = PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Str),
        };
        let mut module = Module::new(Target::wasm());
        let keys: Vec<_> = ["a", "b", "c"].iter().map(|s| module.data.intern_string(s)).collect();
        let vals: Vec<_> = ["x", "y", "z"].iter().map(|s| module.data.intern_string(s)).collect();
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(Op::HashNew, Vec::new(), Some(Immediate::Capacity(8)), IrType::Heap(IrHeapKind::Hash), assoc.clone(), Ownership::Owned)
                .unwrap();
            for (&k, &v) in keys.iter().zip(vals.iter()) {
                let key = b.emit_const_str(k);
                let val = b.emit_const_str(v);
                let _ = b.emit(Op::HashSet, vec![hash, key, val], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            emit_hash_foreach_loop(&mut b, hash, Op::IterCurrentValue);
        }
        module.add_function(f);
        if let Some(o) = run_main(&module) {
            assert_eq!(o, "xyz");
        }
    }

    /// `$h[] = 10; $h[] = 20; $h[] = 30; return $h[2];` -> "30" through `Op::HashAppend`.
    /// The hash lives in a PHP slot and is reloaded before each append, exercising the
    /// runtime next-int-key scan AND the append write-back to the source slot. Reading
    /// key 2 proves the three appends landed at sequential integer keys 0, 1, 2.
    #[test]
    fn hash_append_assigns_sequential_int_keys() {
        let assoc = int_hash_type();
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("a".to_string(), IrType::I64, PhpType::Int);
        let slot = f.add_local(
            Some("h".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            assoc.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(Op::HashNew, Vec::new(), Some(Immediate::Capacity(2)), IrType::Heap(IrHeapKind::Hash), assoc.clone(), Ownership::Owned)
                .unwrap();
            b.emit_store_local(slot, hash);
            for v in [10_i64, 20, 30] {
                let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
                let val = b.emit_const_i64(v);
                let _ = b.emit(Op::HashAppend, vec![h, val], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key = b.emit_const_i64(2);
            let g = b
                .emit(Op::HashGet, vec![h, key], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::Return { value: Some(g) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_a", &[]) {
            assert_eq!(o, "30");
        }
    }

    /// `$h[5] = 500; $h[] = 7; return $h[6];` -> "7" through `Op::HashAppend`. The append
    /// key is the largest existing integer key (5) plus one, NOT the entry count, proving
    /// the backend's next-key scan matches PHP/native semantics through compiled code.
    #[test]
    fn hash_append_after_explicit_key_uses_max_plus_one() {
        let assoc = int_hash_type();
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("b".to_string(), IrType::I64, PhpType::Int);
        let slot = f.add_local(
            Some("h".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            assoc.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(Op::HashNew, Vec::new(), Some(Immediate::Capacity(8)), IrType::Heap(IrHeapKind::Hash), assoc.clone(), Ownership::Owned)
                .unwrap();
            b.emit_store_local(slot, hash);
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key5 = b.emit_const_i64(5);
            let val500 = b.emit_const_i64(500);
            let _ = b.emit(Op::HashSet, vec![h, key5, val500], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let val7 = b.emit_const_i64(7);
            let _ = b.emit(Op::HashAppend, vec![h, val7], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key6 = b.emit_const_i64(6);
            let g = b
                .emit(Op::HashGet, vec![h, key6], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::Return { value: Some(g) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_b", &[]) {
            assert_eq!(o, "7");
        }
    }

    /// `$a = [1=>10, 2=>20]; $b = [2=>99, 3=>30]; return ($a + $b)[2];` -> "20" through
    /// `Op::HashUnion`. The left operand wins on the shared key 2, proving the union
    /// lowering produces a working left-wins merge through compiled code.
    #[test]
    fn hash_union_left_wins_lowers() {
        let assoc = int_hash_type();
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("c".to_string(), IrType::I64, PhpType::Int);
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let a = b
                .emit(Op::HashNew, Vec::new(), Some(Immediate::Capacity(8)), IrType::Heap(IrHeapKind::Hash), assoc.clone(), Ownership::Owned)
                .unwrap();
            for (k, v) in [(1_i64, 10_i64), (2, 20)] {
                let key = b.emit_const_i64(k);
                let val = b.emit_const_i64(v);
                let _ = b.emit(Op::HashSet, vec![a, key, val], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            let bb = b
                .emit(Op::HashNew, Vec::new(), Some(Immediate::Capacity(8)), IrType::Heap(IrHeapKind::Hash), assoc.clone(), Ownership::Owned)
                .unwrap();
            for (k, v) in [(2_i64, 99_i64), (3, 30)] {
                let key = b.emit_const_i64(k);
                let val = b.emit_const_i64(v);
                let _ = b.emit(Op::HashSet, vec![bb, key, val], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            let u = b
                .emit(Op::HashUnion, vec![a, bb], None, IrType::Heap(IrHeapKind::Hash), assoc.clone(), Ownership::Owned)
                .unwrap();
            let key = b.emit_const_i64(2);
            let g = b
                .emit(Op::HashGet, vec![u, key], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                .unwrap();
            b.terminate(Terminator::Return { value: Some(g) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_c", &[]) {
            assert_eq!(o, "20");
        }
    }

    /// `foreach (["a"=>"x"] + ["b"=>"y"] as $v) echo $v;` -> "xy" through `Op::HashUnion`.
    /// Exercises a string-keyed/string-valued union whose merged result is iterated in
    /// insertion order (left entries first, then the right operand's new keys), proving the
    /// union result is a well-formed iterable hash with persisted string children.
    #[test]
    fn hash_union_foreach_echoes_merged() {
        let assoc = PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Str),
        };
        let mut module = Module::new(Target::wasm());
        let ak = module.data.intern_string("a");
        let av = module.data.intern_string("x");
        let bk = module.data.intern_string("b");
        let bv = module.data.intern_string("y");
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let a = b
                .emit(Op::HashNew, Vec::new(), Some(Immediate::Capacity(8)), IrType::Heap(IrHeapKind::Hash), assoc.clone(), Ownership::Owned)
                .unwrap();
            let akey = b.emit_const_str(ak);
            let aval = b.emit_const_str(av);
            let _ = b.emit(Op::HashSet, vec![a, akey, aval], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            let bb = b
                .emit(Op::HashNew, Vec::new(), Some(Immediate::Capacity(8)), IrType::Heap(IrHeapKind::Hash), assoc.clone(), Ownership::Owned)
                .unwrap();
            let bkey = b.emit_const_str(bk);
            let bval = b.emit_const_str(bv);
            let _ = b.emit(Op::HashSet, vec![bb, bkey, bval], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            let u = b
                .emit(Op::HashUnion, vec![a, bb], None, IrType::Heap(IrHeapKind::Hash), assoc.clone(), Ownership::Owned)
                .unwrap();
            emit_hash_foreach_loop(&mut b, u, Op::IterCurrentValue);
        }
        module.add_function(f);
        if let Some(o) = run_main(&module) {
            assert_eq!(o, "xy");
        }
    }

    /// `[10,20] + [99,88,77]` through `Op::ArrayUnion`. The left elements are preserved and
    /// only the right tail at index >= 2 is appended, yielding `[10,20,77]`. Returns
    /// `u[0]*100 + u[2]` = 10*100 + 77 = 1077, proving the indexed-union lowering produces a
    /// working left-wins, tail-append result through compiled code.
    #[test]
    fn array_union_lowers() {
        let elem = PhpType::Array(Box::new(PhpType::Int));
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("a".to_string(), IrType::I64, PhpType::Int);
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let a = b
                .emit(Op::ArrayNew, Vec::new(), Some(Immediate::Capacity(4)), IrType::Heap(IrHeapKind::Array), elem.clone(), Ownership::Owned)
                .unwrap();
            for v in [10_i64, 20] {
                let c = b.emit_const_i64(v);
                let _ = b.emit(Op::ArrayPush, vec![a, c], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            let bb = b
                .emit(Op::ArrayNew, Vec::new(), Some(Immediate::Capacity(4)), IrType::Heap(IrHeapKind::Array), elem.clone(), Ownership::Owned)
                .unwrap();
            for v in [99_i64, 88, 77] {
                let c = b.emit_const_i64(v);
                let _ = b.emit(Op::ArrayPush, vec![bb, c], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            let u = b
                .emit(Op::ArrayUnion, vec![a, bb], None, IrType::Heap(IrHeapKind::Array), elem.clone(), Ownership::Owned)
                .unwrap();
            let i0 = b.emit_const_i64(0);
            let g0 = b.emit(Op::ArrayGet, vec![u, i0], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            let i2 = b.emit_const_i64(2);
            let g2 = b.emit(Op::ArrayGet, vec![u, i2], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            let hundred = b.emit_const_i64(100);
            let g0x = b.emit(Op::IMul, vec![g0, hundred], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            let total = b.emit(Op::IAdd, vec![g0x, g2], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            b.terminate(Terminator::Return { value: Some(total) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_a", &[]) {
            assert_eq!(o, "1077");
        }
    }

    /// `[10,20] + [1=>99, 5=>30]` through `Op::ArrayHashUnion`. The left indexed positions
    /// promote to integer keys (0:10, 1:20); key 1 wins over the right's `1=>99`, and the
    /// right's new key `5=>30` is merged. Returns `get(1)*100 + get(5)` = 20*100 + 30 = 2030,
    /// proving the cross-representation lowering yields a usable left-wins hash result.
    #[test]
    fn array_hash_union_lowers() {
        let elem = PhpType::Array(Box::new(PhpType::Int));
        let assoc = int_hash_type();
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("a".to_string(), IrType::I64, PhpType::Int);
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let a = b
                .emit(Op::ArrayNew, Vec::new(), Some(Immediate::Capacity(4)), IrType::Heap(IrHeapKind::Array), elem.clone(), Ownership::Owned)
                .unwrap();
            for v in [10_i64, 20] {
                let c = b.emit_const_i64(v);
                let _ = b.emit(Op::ArrayPush, vec![a, c], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            let bb = b
                .emit(Op::HashNew, Vec::new(), Some(Immediate::Capacity(8)), IrType::Heap(IrHeapKind::Hash), assoc.clone(), Ownership::Owned)
                .unwrap();
            for (k, v) in [(1_i64, 99_i64), (5, 30)] {
                let key = b.emit_const_i64(k);
                let val = b.emit_const_i64(v);
                let _ = b.emit(Op::HashSet, vec![bb, key, val], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            let u = b
                .emit(Op::ArrayHashUnion, vec![a, bb], None, IrType::Heap(IrHeapKind::Hash), assoc.clone(), Ownership::Owned)
                .unwrap();
            let k1 = b.emit_const_i64(1);
            let g1 = b.emit(Op::HashGet, vec![u, k1], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            let k5 = b.emit_const_i64(5);
            let g5 = b.emit(Op::HashGet, vec![u, k5], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            let hundred = b.emit_const_i64(100);
            let g1x = b.emit(Op::IMul, vec![g1, hundred], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            let total = b.emit(Op::IAdd, vec![g1x, g5], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            b.terminate(Terminator::Return { value: Some(total) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_a", &[]) {
            assert_eq!(o, "2030");
        }
    }

    /// `[0=>10, 5=>50] + [99,88,77]` through `Op::HashArrayUnion`. The result clones the left
    /// hash; key 0 wins over the right's index 0 (99), and the right's missing positions
    /// `1=>88` and `2=>77` are appended under their integer keys. Returns `get(0)*100 + get(2)`
    /// = 10*100 + 77 = 1077, proving the cross-representation lowering yields a usable result.
    #[test]
    fn hash_array_union_lowers() {
        let elem = PhpType::Array(Box::new(PhpType::Int));
        let assoc = int_hash_type();
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("a".to_string(), IrType::I64, PhpType::Int);
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let a = b
                .emit(Op::HashNew, Vec::new(), Some(Immediate::Capacity(8)), IrType::Heap(IrHeapKind::Hash), assoc.clone(), Ownership::Owned)
                .unwrap();
            for (k, v) in [(0_i64, 10_i64), (5, 50)] {
                let key = b.emit_const_i64(k);
                let val = b.emit_const_i64(v);
                let _ = b.emit(Op::HashSet, vec![a, key, val], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            let bb = b
                .emit(Op::ArrayNew, Vec::new(), Some(Immediate::Capacity(4)), IrType::Heap(IrHeapKind::Array), elem.clone(), Ownership::Owned)
                .unwrap();
            for v in [99_i64, 88, 77] {
                let c = b.emit_const_i64(v);
                let _ = b.emit(Op::ArrayPush, vec![bb, c], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            let u = b
                .emit(Op::HashArrayUnion, vec![a, bb], None, IrType::Heap(IrHeapKind::Hash), assoc.clone(), Ownership::Owned)
                .unwrap();
            let k0 = b.emit_const_i64(0);
            let g0 = b.emit(Op::HashGet, vec![u, k0], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            let k2 = b.emit_const_i64(2);
            let g2 = b.emit(Op::HashGet, vec![u, k2], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            let hundred = b.emit_const_i64(100);
            let g0x = b.emit(Op::IMul, vec![g0, hundred], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            let total = b.emit(Op::IAdd, vec![g0x, g2], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            b.terminate(Terminator::Return { value: Some(total) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_a", &[]) {
            assert_eq!(o, "1077");
        }
    }

    /// `$h[1]=10; $h[2]=20; $h[3]=30; unset($h[2]);` then
    /// `return is_null($h[2])*10000 + $h[1]*100 + $h[3];` -> "11030" through `Op::HashUnset`.
    /// The hash lives in a PHP slot reloaded before the unset and each read, exercising the
    /// removal write-back to the source slot: key 2 now misses (reads the null sentinel, so
    /// `is_null` is 1) while keys 1 and 3 still resolve. Without the unset the result is 1030.
    #[test]
    fn hash_unset_removes_element_lowers() {
        let assoc = int_hash_type();
        let mut module = Module::new(Target::wasm());
        let mut f = Function::new("a".to_string(), IrType::I64, PhpType::Int);
        let slot = f.add_local(
            Some("h".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            assoc.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(Op::HashNew, Vec::new(), Some(Immediate::Capacity(8)), IrType::Heap(IrHeapKind::Hash), assoc.clone(), Ownership::Owned)
                .unwrap();
            b.emit_store_local(slot, hash);
            for (k, v) in [(1_i64, 10_i64), (2, 20), (3, 30)] {
                let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
                let key = b.emit_const_i64(k);
                let val = b.emit_const_i64(v);
                let _ = b.emit(Op::HashSet, vec![h, key, val], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key2 = b.emit_const_i64(2);
            let _ = b.emit(Op::HashUnset, vec![h, key2], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let k1 = b.emit_const_i64(1);
            let g1 = b.emit(Op::HashGet, vec![h, k1], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let k3 = b.emit_const_i64(3);
            let g3 = b.emit(Op::HashGet, vec![h, k3], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let k2 = b.emit_const_i64(2);
            let g2 = b.emit(Op::HashGet, vec![h, k2], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            let removed = b.emit(Op::IsNull, vec![g2], None, IrType::I64, PhpType::Bool, Ownership::NonHeap).unwrap();
            let ten_k = b.emit_const_i64(10000);
            let removed_x = b.emit(Op::IMul, vec![removed, ten_k], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            let hundred = b.emit_const_i64(100);
            let g1x = b.emit(Op::IMul, vec![g1, hundred], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            let sum = b.emit(Op::IAdd, vec![removed_x, g1x], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            let total = b.emit(Op::IAdd, vec![sum, g3], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
            b.terminate(Terminator::Return { value: Some(total) });
        }
        module.add_function(f);
        if let Some(o) = invoke(&module, "fn_a", &[]) {
            assert_eq!(o, "11030");
        }
    }

    /// `$h = ["a"=>10, "b"=>20, "c"=>30]; unset($h["b"]); foreach ($h as $v) echo $v;` -> "1030"
    /// through `Op::HashUnset`. The removed entry is spliced out of the insertion-order chain,
    /// so the post-unset foreach walks only "a" and "c" in order, proving the linked-list
    /// unlink is correct through compiled iteration.
    #[test]
    fn hash_unset_then_foreach_skips_removed() {
        let assoc = PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Int),
        };
        let mut module = Module::new(Target::wasm());
        let keys: Vec<_> = ["a", "b", "c"].iter().map(|s| module.data.intern_string(s)).collect();
        let bkey = module.data.intern_string("b");
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        let slot = f.add_local(
            Some("h".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            assoc.clone(),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let hash = b
                .emit(Op::HashNew, Vec::new(), Some(Immediate::Capacity(8)), IrType::Heap(IrHeapKind::Hash), assoc.clone(), Ownership::Owned)
                .unwrap();
            b.emit_store_local(slot, hash);
            for (i, &k) in keys.iter().enumerate() {
                let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
                let key = b.emit_const_str(k);
                let val = b.emit_const_i64(((i as i64) + 1) * 10);
                let _ = b.emit(Op::HashSet, vec![h, key, val], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            let key = b.emit_const_str(bkey);
            let _ = b.emit(Op::HashUnset, vec![h, key], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            let h = b.emit_load_local(slot, IrType::Heap(IrHeapKind::Hash), assoc.clone());
            emit_hash_foreach_loop(&mut b, h, Op::IterCurrentValue);
        }
        module.add_function(f);
        if let Some(o) = run_main(&module) {
            assert_eq!(o, "1030");
        }
    }

    // ----- P6a: object allocation + scalar properties + kind-4 decref -----

    /// Builds a `ClassInfo` with only the P6a-relevant fields populated
    /// (`class_id`, `properties`, `property_offsets`, `defaults`,
    /// `allow_dynamic_properties`) and every other field empty, mirroring a
    /// freshly-declared scalar-property class. Property offsets are assigned
    /// parent-first as `8 + i*16`, matching the object payload layout the lowering
    /// emits and reads.
    fn test_class_info(
        class_id: u64,
        properties: Vec<(String, PhpType)>,
        defaults: Vec<Option<Expr>>,
        allow_dynamic_properties: bool,
    ) -> ClassInfo {
        let property_offsets = properties
            .iter()
            .enumerate()
            .map(|(i, (n, _))| (n.clone(), 8 + i * 16))
            .collect::<HashMap<_, _>>();
        ClassInfo {
            class_id,
            parent: None,
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            allow_dynamic_properties,
            constants: HashMap::new(),
            attribute_names: Vec::new(),
            attribute_args: Vec::new(),
            method_attribute_names: HashMap::new(),
            method_attribute_args: HashMap::new(),
            property_attribute_names: HashMap::new(),
            property_attribute_args: HashMap::new(),
            used_traits: Vec::new(),
            properties,
            property_offsets,
            property_declaring_classes: HashMap::new(),
            defaults,
            property_visibilities: HashMap::new(),
            property_set_visibilities: HashMap::new(),
            declared_properties: HashSet::new(),
            final_properties: HashSet::new(),
            readonly_properties: HashSet::new(),
            reference_properties: HashSet::new(),
            abstract_properties: HashSet::new(),
            abstract_property_hooks: HashMap::new(),
            static_properties: Vec::new(),
            static_defaults: Vec::new(),
            static_property_declaring_classes: HashMap::new(),
            static_property_visibilities: HashMap::new(),
            declared_static_properties: HashSet::new(),
            final_static_properties: HashSet::new(),
            method_decls: Vec::new(),
            methods: HashMap::new(),
            static_methods: HashMap::new(),
            callable_method_return_sigs: HashMap::new(),
            callable_array_method_return_sigs: HashMap::new(),
            method_visibilities: HashMap::new(),
            final_methods: HashSet::new(),
            method_declaring_classes: HashMap::new(),
            method_impl_classes: HashMap::new(),
            vtable_methods: Vec::new(),
            vtable_slots: HashMap::new(),
            static_method_visibilities: HashMap::new(),
            final_static_methods: HashSet::new(),
            static_method_declaring_classes: HashMap::new(),
            static_method_impl_classes: HashMap::new(),
            static_vtable_methods: Vec::new(),
            static_vtable_slots: HashMap::new(),
            interfaces: Vec::new(),
            constructor_param_to_prop: Vec::new(),
        }
    }

    /// Builds a module with one declared class and one `fn_obj` function (no params,
    /// returns i64) whose body is `body`. The class is registered in
    /// `module.class_infos` and its name interned into `module.data.class_names`;
    /// each property name is interned into `module.data.strings` in declaration
    /// order. `body` receives the builder, the class data id, and the per-property
    /// string data ids (declaration order) and returns the i64 result. Run with
    /// `invoke(&module, "fn_obj", &[])`.
    fn object_fn_module(
        class_name: &str,
        properties: Vec<(String, PhpType)>,
        defaults: Vec<Option<Expr>>,
        body: impl FnOnce(&mut Builder, DataId, &[DataId]) -> ValueId,
    ) -> Module {
        let mut module = Module::new(Target::wasm());
        let class_data = module.data.intern_class_name(class_name);
        let prop_data: Vec<DataId> = properties
            .iter()
            .map(|(n, _)| module.data.intern_string(n))
            .collect();
        module
            .class_infos
            .insert(class_name.to_string(), test_class_info(1, properties, defaults, false));
        let mut f = Function::new("obj".to_string(), IrType::I64, PhpType::Int);
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let result = body(&mut b, class_data, &prop_data);
            b.terminate(Terminator::Return { value: Some(result) });
        }
        module.add_function(f);
        module
    }

    /// Like `object_fn_module` but builds a `main` function (void, command mode) whose
    /// body is `body` (it performs its own `EchoValue`s and returns nothing). Run with
    /// `run_main(&module)`.
    fn object_main_module(
        class_name: &str,
        properties: Vec<(String, PhpType)>,
        defaults: Vec<Option<Expr>>,
        body: impl FnOnce(&mut Builder, DataId, &[DataId]),
    ) -> Module {
        let mut module = Module::new(Target::wasm());
        let class_data = module.data.intern_class_name(class_name);
        let prop_data: Vec<DataId> = properties
            .iter()
            .map(|(n, _)| module.data.intern_string(n))
            .collect();
        module
            .class_infos
            .insert(class_name.to_string(), test_class_info(1, properties, defaults, false));
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            body(&mut b, class_data, &prop_data);
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        module
    }

    /// Emits `new ClassName()` and returns the object value id (kind-4 heap block).
    fn emit_object_new(b: &mut Builder, class_name: &str, class_data: DataId) -> ValueId {
        b.emit(
            Op::ObjectNew,
            Vec::new(),
            Some(Immediate::Data(class_data)),
            IrType::Heap(IrHeapKind::Object),
            PhpType::Object(class_name.to_string()),
            Ownership::Owned,
        )
        .expect("ObjectNew lowers")
    }

    /// Emits `new ClassName(args...)`: `Op::ObjectNew` carrying the ctor USER args as
    /// operands (the receiver `$this` is NOT included — the backend prepends it).
    fn emit_object_new_with_args(
        b: &mut Builder,
        class_name: &str,
        class_data: DataId,
        args: Vec<ValueId>,
    ) -> ValueId {
        b.emit(
            Op::ObjectNew,
            args,
            Some(Immediate::Data(class_data)),
            IrType::Heap(IrHeapKind::Object),
            PhpType::Object(class_name.to_string()),
            Ownership::Owned,
        )
        .expect("ObjectNew with ctor args lowers")
    }

    /// Emits `$obj->$prop = $value` (PropSet is void).
    fn emit_prop_set(b: &mut Builder, obj: ValueId, prop_data: DataId, value: ValueId) {
        let _ = b.emit(
            Op::PropSet,
            vec![obj, value],
            Some(Immediate::Data(prop_data)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
    }

    /// Emits `$obj->$prop` (PropGet) with the given scalar result type and returns it.
    fn emit_prop_get(
        b: &mut Builder,
        obj: ValueId,
        prop_data: DataId,
        ir: IrType,
        php: PhpType,
    ) -> ValueId {
        b.emit(
            Op::PropGet,
            vec![obj],
            Some(Immediate::Data(prop_data)),
            ir,
            php,
            Ownership::NonHeap,
        )
        .expect("PropGet lowers")
    }

    /// `new P{int x; int y}; x=3; y=4; return x+y` -> "7". Verifies alloc + scalar
    /// PropSet/PropGet round-trip for two int properties at offsets 8 and 24.
    #[test]
    fn object_new_scalar_props_roundtrip() {
        let class = "P";
        let m = object_fn_module(
            class,
            vec![("x".to_string(), PhpType::Int), ("y".to_string(), PhpType::Int)],
            vec![None, None],
            |b, cd, pd| {
                let obj = emit_object_new(b, class, cd);
                let v0 = b.emit_const_i64(3);
                emit_prop_set(b, obj, pd[0], v0);
                let v1 = b.emit_const_i64(4);
                emit_prop_set(b, obj, pd[1], v1);
                let x = emit_prop_get(b, obj, pd[0], IrType::I64, PhpType::Int);
                let y = emit_prop_get(b, obj, pd[1], IrType::I64, PhpType::Int);
                b.emit(Op::IAdd, vec![x, y], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                    .unwrap()
            },
        );
        if let Some(o) = invoke(&m, "fn_obj", &[]) {
            assert_eq!(o, "7");
        }
    }

    /// `new P{int x = 5}; return x` -> "5". Verifies the int property default is
    /// emitted by `ObjectNew` (read straight back, no PropSet).
    #[test]
    fn object_int_default_is_emitted() {
        let class = "P";
        let m = object_fn_module(
            class,
            vec![("x".to_string(), PhpType::Int)],
            vec![Some(Expr { kind: ExprKind::IntLiteral(5), span: Span::dummy() })],
            |b, cd, pd| {
                let obj = emit_object_new(b, class, cd);
                emit_prop_get(b, obj, pd[0], IrType::I64, PhpType::Int)
            },
        );
        if let Some(o) = invoke(&m, "fn_obj", &[]) {
            assert_eq!(o, "5");
        }
    }

    /// `new P{int x}; return x` -> "0". Verifies an unset (no-default) int property
    /// reads as zero: the `ObjectNew` zeroing loop wrote `(0, 0)` and no default follows.
    #[test]
    fn object_unset_int_property_reads_zero() {
        let class = "P";
        let m = object_fn_module(
            class,
            vec![("x".to_string(), PhpType::Int)],
            vec![None],
            |b, cd, pd| {
                let obj = emit_object_new(b, class, cd);
                emit_prop_get(b, obj, pd[0], IrType::I64, PhpType::Int)
            },
        );
        if let Some(o) = invoke(&m, "fn_obj", &[]) {
            assert_eq!(o, "0");
        }
    }

    /// `new P{int x}; x=1; x=2; return x` -> "2". Verifies PropSet overwrites the
    /// previous scalar value in place (same slot, last write wins).
    #[test]
    fn object_prop_set_overwrites() {
        let class = "P";
        let m = object_fn_module(
            class,
            vec![("x".to_string(), PhpType::Int)],
            vec![None],
            |b, cd, pd| {
                let obj = emit_object_new(b, class, cd);
                let one = b.emit_const_i64(1);
                emit_prop_set(b, obj, pd[0], one);
                let two = b.emit_const_i64(2);
                emit_prop_set(b, obj, pd[0], two);
                emit_prop_get(b, obj, pd[0], IrType::I64, PhpType::Int)
            },
        );
        if let Some(o) = invoke(&m, "fn_obj", &[]) {
            assert_eq!(o, "2");
        }
    }

    /// Two instances of `P{int x}` with `a.x=1`, `b.x=2` -> `a.x*10 + b.x` = "12".
    /// Verifies distinct `ObjectNew` allocations do not share property storage.
    #[test]
    fn object_two_instances_are_independent() {
        let class = "P";
        let m = object_fn_module(
            class,
            vec![("x".to_string(), PhpType::Int)],
            vec![None],
            |b, cd, pd| {
                let a = emit_object_new(b, class, cd);
                let bb = emit_object_new(b, class, cd);
                let one = b.emit_const_i64(1);
                emit_prop_set(b, a, pd[0], one);
                let two = b.emit_const_i64(2);
                emit_prop_set(b, bb, pd[0], two);
                let av = emit_prop_get(b, a, pd[0], IrType::I64, PhpType::Int);
                let bv = emit_prop_get(b, bb, pd[0], IrType::I64, PhpType::Int);
                let ten = b.emit_const_i64(10);
                let scaled = b.emit(
                    Op::IMul,
                    vec![av, ten],
                    None,
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                )
                .unwrap();
                b.emit(Op::IAdd, vec![scaled, bv], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                    .unwrap()
            },
        );
        if let Some(o) = invoke(&m, "fn_obj", &[]) {
            assert_eq!(o, "12");
        }
    }

    /// `new Q{int x; int y}` (Q inherits P{x}) with the flattened parent-first
    /// property list `[(x,Int),(y,Int)]`; `x=1; y=2; return x+y` -> "3". Verifies the
    /// parent-first offset layout (`x` at offset 8, `y` at offset 24) the lowering
    /// reads from `ClassInfo.property_offsets`.
    #[test]
    fn object_inherited_property_offsets() {
        let class = "Q";
        let m = object_fn_module(
            class,
            vec![("x".to_string(), PhpType::Int), ("y".to_string(), PhpType::Int)],
            vec![None, None],
            |b, cd, pd| {
                let obj = emit_object_new(b, class, cd);
                let one = b.emit_const_i64(1);
                emit_prop_set(b, obj, pd[0], one);
                let two = b.emit_const_i64(2);
                emit_prop_set(b, obj, pd[1], two);
                let x = emit_prop_get(b, obj, pd[0], IrType::I64, PhpType::Int);
                let y = emit_prop_get(b, obj, pd[1], IrType::I64, PhpType::Int);
                b.emit(Op::IAdd, vec![x, y], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                    .unwrap()
            },
        );
        if let Some(o) = invoke(&m, "fn_obj", &[]) {
            assert_eq!(o, "3");
        }
    }

    /// `new P{int a; int b; int c}; a=1; b=2; c=3; return c*100+b*10+a` -> "321".
    /// Verifies the non-zero-index offset math `8 + i*16` for i = 0, 1, 2 (slots at
    /// 8, 24, 40) so a later property does not clobber an earlier one.
    #[test]
    fn object_multi_property_nonzero_index_offsets() {
        let class = "P";
        let m = object_fn_module(
            class,
            vec![
                ("a".to_string(), PhpType::Int),
                ("b".to_string(), PhpType::Int),
                ("c".to_string(), PhpType::Int),
            ],
            vec![None, None, None],
            |b, cd, pd| {
                let obj = emit_object_new(b, class, cd);
                let one = b.emit_const_i64(1);
                emit_prop_set(b, obj, pd[0], one);
                let two = b.emit_const_i64(2);
                emit_prop_set(b, obj, pd[1], two);
                let three = b.emit_const_i64(3);
                emit_prop_set(b, obj, pd[2], three);
                let a = emit_prop_get(b, obj, pd[0], IrType::I64, PhpType::Int);
                let bb = emit_prop_get(b, obj, pd[1], IrType::I64, PhpType::Int);
                let c = emit_prop_get(b, obj, pd[2], IrType::I64, PhpType::Int);
                let hundred = b.emit_const_i64(100);
                let c100 = b.emit(
                    Op::IMul,
                    vec![c, hundred],
                    None,
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                )
                .unwrap();
                let ten = b.emit_const_i64(10);
                let b10 = b.emit(
                    Op::IMul,
                    vec![bb, ten],
                    None,
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                )
                .unwrap();
                let sum = b.emit(
                    Op::IAdd,
                    vec![c100, b10],
                    None,
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                )
                .unwrap();
                b.emit(Op::IAdd, vec![sum, a], None, IrType::I64, PhpType::Int, Ownership::NonHeap)
                    .unwrap()
            },
        );
        if let Some(o) = invoke(&m, "fn_obj", &[]) {
            assert_eq!(o, "321");
        }
    }

    /// `echo $p->x` for `new P{int x}; x=42` -> "42". Verifies the int property load
    /// feeds `EchoValue` (int -> decimal stdout) through `run_main`.
    #[test]
    fn object_echo_int_property() {
        let class = "P";
        let m = object_main_module(
            class,
            vec![("x".to_string(), PhpType::Int)],
            vec![None],
            |b, cd, pd| {
                let obj = emit_object_new(b, class, cd);
                let v = b.emit_const_i64(42);
                emit_prop_set(b, obj, pd[0], v);
                let x = emit_prop_get(b, obj, pd[0], IrType::I64, PhpType::Int);
                let _ = b.emit(
                    Op::EchoValue,
                    vec![x],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            },
        );
        if let Some(o) = run_main(&m) {
            assert_eq!(o, "42");
        }
    }

    /// `echo $p->f` for `new P{float f = 2.5}` -> "2.5". Verifies the float property
    /// default is emitted as raw f64 bits and read back by `f64.load`, then echoed.
    #[test]
    fn object_float_default_echo() {
        let class = "P";
        let m = object_main_module(
            class,
            vec![("f".to_string(), PhpType::Float)],
            vec![Some(Expr { kind: ExprKind::FloatLiteral(2.5), span: Span::dummy() })],
            |b, cd, pd| {
                let obj = emit_object_new(b, class, cd);
                let f = emit_prop_get(b, obj, pd[0], IrType::F64, PhpType::Float);
                let _ = b.emit(
                    Op::EchoValue,
                    vec![f],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            },
        );
        if let Some(o) = run_main(&m) {
            assert_eq!(o, "2.5");
        }
    }

    /// `echo $p->f` for `new P{float f}; f=3.5` -> "3.5". Verifies a PropSet float
    /// stores via `f64.store` and reads back via `f64.load` then echoes.
    #[test]
    fn object_float_prop_set_then_echo() {
        let class = "P";
        let m = object_main_module(
            class,
            vec![("f".to_string(), PhpType::Float)],
            vec![None],
            |b, cd, pd| {
                let obj = emit_object_new(b, class, cd);
                let v = b.emit_const_f64(3.5);
                emit_prop_set(b, obj, pd[0], v);
                let f = emit_prop_get(b, obj, pd[0], IrType::F64, PhpType::Float);
                let _ = b.emit(
                    Op::EchoValue,
                    vec![f],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            },
        );
        if let Some(o) = run_main(&m) {
            assert_eq!(o, "3.5");
        }
    }

    /// `echo $p->b` for `new P{bool b = true}` -> "1". Verifies the bool property
    /// default is emitted as i64 1 and echoed (true -> "1").
    #[test]
    fn object_bool_default_echo() {
        let class = "P";
        let m = object_main_module(
            class,
            vec![("b".to_string(), PhpType::Bool)],
            vec![Some(Expr { kind: ExprKind::BoolLiteral(true), span: Span::dummy() })],
            |b, cd, pd| {
                let obj = emit_object_new(b, class, cd);
                let bv = emit_prop_get(b, obj, pd[0], IrType::I64, PhpType::Bool);
                let _ = b.emit(
                    Op::EchoValue,
                    vec![bv],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            },
        );
        if let Some(o) = run_main(&m) {
            assert_eq!(o, "1");
        }
    }

    /// `echo $p->b` for `new P{bool b}; b=false` -> "" (false echoes as empty). Verifies
    /// a PropSet bool stores 0 and echoes as nothing (matching the `echo_booleans` test).
    #[test]
    fn object_bool_prop_set_false_echo_empty() {
        let class = "P";
        let m = object_main_module(
            class,
            vec![("b".to_string(), PhpType::Bool)],
            vec![None],
            |b, cd, pd| {
                let obj = emit_object_new(b, class, cd);
                let v = b.emit_const_bool(false);
                emit_prop_set(b, obj, pd[0], v);
                let bv = emit_prop_get(b, obj, pd[0], IrType::I64, PhpType::Bool);
                let _ = b.emit(
                    Op::EchoValue,
                    vec![bv],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            },
        );
        if let Some(o) = run_main(&m) {
            assert_eq!(o, "");
        }
    }

    /// `new P{mixed m}; $o->m = 42; echo $o->m` -> "42". Verifies the P6b Mixed-property
    /// BOX path: PropSet of a scalar into a mixed slot boxes it via `__rt_mixed_from_value`
    /// (tag 0 / int) after releasing the previous (zero) slot value, and PropGet returns an
    /// owned mixed cell whose `EchoValue` dispatches by runtime tag to `__rt_itoa`.
    #[test]
    fn object_mixed_prop_box_int_then_echo() {
        let class = "P";
        let m = object_main_module(
            class,
            vec![("m".to_string(), PhpType::Mixed)],
            vec![None],
            |b, cd, pd| {
                let obj = emit_object_new(b, class, cd);
                let v = b.emit_const_i64(42);
                emit_prop_set(b, obj, pd[0], v);
                let mv = emit_prop_get(b, obj, pd[0], IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
                let _ = b.emit(
                    Op::EchoValue,
                    vec![mv],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            },
        );
        if let Some(o) = run_main(&m) {
            assert_eq!(o, "42");
        }
    }

    /// `new P{string s}; $o->s = "hi"; echo $o->s` -> "hi". Verifies the P6b string-property
    /// path: PropSet persists a copy into the slot (lo = ptr, hi = len) after releasing the
    /// previous (zero) value, and PropGet persists the read copy so `EchoValue` writes the
    /// exact bytes via `__rt_echo_str`. Built inline (not via `object_main_module`) so the
    /// string literal can be interned and its `DataId` moved into the body.
    #[test]
    fn object_string_prop_set_then_echo() {
        let class = "P";
        let mut module = Module::new(Target::wasm());
        let class_data = module.data.intern_class_name(class);
        let prop_data = module.data.intern_string("s");
        let hi = module.data.intern_string("hi");
        module.class_infos.insert(
            class.to_string(),
            test_class_info(1, vec![("s".to_string(), PhpType::Str)], vec![None], false),
        );
        let mut f = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        f.flags.is_main = true;
        {
            let mut b = Builder::new(&mut f);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let obj = emit_object_new(&mut b, class, class_data);
            let s = b.emit_const_str(hi);
            emit_prop_set(&mut b, obj, prop_data, s);
            let sv = emit_prop_get(&mut b, obj, prop_data, IrType::Str, PhpType::Str);
            let _ = b.emit(
                Op::EchoValue,
                vec![sv],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(f);
        if let Some(out) = run_main(&module) {
            assert_eq!(out, "hi");
        }
    }

    // ----- P6c: __construct + $this param convention -----

    /// Builds a `__construct` `FunctionSig` over the given user params (no defaults,
    /// no by-ref, no variadic). `$this` is NOT included: it is a backend convention
    /// (hidden leading param), not a declared param, so `sig.params` lists only the
    /// user params — matching the native `__construct` signature.
    fn ctor_sig(user_params: &[(&str, PhpType)]) -> FunctionSig {
        FunctionSig {
            params: user_params
                .iter()
                .map(|(n, t)| (n.to_string(), t.clone()))
                .collect(),
            defaults: (0..user_params.len()).map(|_| None).collect(),
            return_type: PhpType::Void,
            declared_return: true,
            ref_params: (0..user_params.len()).map(|_| false).collect(),
            declared_params: (0..user_params.len()).map(|_| true).collect(),
            variadic: None,
            deprecation: None,
        }
    }

    /// `new P(42)` where `P::__construct(int $v){ $this->x = $v; }` -> echo `$o->x` = "42".
    /// Verifies the full P6c path: `Op::ObjectNew` carries the ctor user arg, the backend
    /// resolves `__construct`, prepends the fresh object as `$this`, calls `P::__construct`
    /// with `[this, 42]`, and the ctor body's `PropSet` writes the arg into property `x`.
    #[test]
    fn object_ctor_one_arg_sets_prop_then_echo() {
        let class = "P";
        let mut module = Module::new(Target::wasm());
        let class_data = module.data.intern_class_name(class);
        let prop_data = module.data.intern_string("x");
        // Class P: one int property x (no default) + a declared __construct(int $v).
        let mut ci = test_class_info(1, vec![("x".to_string(), PhpType::Int)], vec![None], false);
        let ctor_key = crate::names::php_symbol_key("__construct");
        ci.methods.insert(ctor_key.clone(), ctor_sig(&[("v", PhpType::Int)]));
        ci.method_impl_classes.insert(ctor_key, class.to_string());
        module.class_infos.insert(class.to_string(), ci);

        // P::__construct(this, int $v): $this->x = $v. Params are [this, v]; slots map
        // 0->this (param 0), 1->v (param 1) in lower_function's param<->local mapping.
        let mut ctor = Function::new(format!("{}::__construct", class), IrType::Void, PhpType::Void);
        ctor.flags.is_method = true;
        ctor.params.push(FunctionParam {
            name: "this".to_string(),
            ir_type: IrType::Heap(IrHeapKind::Object),
            php_type: PhpType::Object(class.to_string()),
            by_ref: false,
            variadic: false,
        });
        ctor.params.push(FunctionParam {
            name: "v".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: false,
            variadic: false,
        });
        let this_slot = ctor.add_local(
            Some("this".to_string()),
            IrType::Heap(IrHeapKind::Object),
            PhpType::Object(class.to_string()),
            LocalKind::PhpLocal,
        );
        let v_slot = ctor.add_local(
            Some("v".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut ctor);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            // Materialize $this and $v as ValueIds before the PropSet.
            let this = b.emit_load_local(
                this_slot,
                IrType::Heap(IrHeapKind::Object),
                PhpType::Object(class.to_string()),
            );
            let v = b.emit_load_local(v_slot, IrType::I64, PhpType::Int);
            emit_prop_set(&mut b, this, prop_data, v);
            b.terminate(Terminator::Return { value: None });
        }
        module.class_methods.push(ctor);

        // main: $o = new P(42); echo $o->x;
        let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        main.flags.is_main = true;
        {
            let mut b = Builder::new(&mut main);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let arg = b.emit_const_i64(42);
            let obj = emit_object_new_with_args(&mut b, class, class_data, vec![arg]);
            let x = emit_prop_get(&mut b, obj, prop_data, IrType::I64, PhpType::Int);
            let _ = b.emit(
                Op::EchoValue,
                vec![x],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(main);
        if let Some(out) = run_main(&module) {
            assert_eq!(out, "42");
        }
    }

    /// `new P()` where `P::__construct(){ $this->x = 7; }` (property default 0) -> echo = "7".
    /// Verifies a 0-arg ctor is STILL called (operands empty but ctor present): `$this` is
    /// passed alone, so the ctor body overwrites the default-0 property with 7. Proves the
    /// "ctor present -> call regardless of operand count" gate path.
    #[test]
    fn object_ctor_zero_arg_sets_default_prop_then_echo() {
        let class = "P";
        let mut module = Module::new(Target::wasm());
        let class_data = module.data.intern_class_name(class);
        let prop_data = module.data.intern_string("x");
        // Property x with a scalar default 0 (written before the ctor), then ctor sets 7.
        let default_zero = Expr { kind: ExprKind::IntLiteral(0), span: Span::dummy() };
        let mut ci = test_class_info(
            1,
            vec![("x".to_string(), PhpType::Int)],
            vec![Some(default_zero)],
            false,
        );
        let ctor_key = crate::names::php_symbol_key("__construct");
        ci.methods.insert(ctor_key.clone(), ctor_sig(&[]));
        ci.method_impl_classes.insert(ctor_key, class.to_string());
        module.class_infos.insert(class.to_string(), ci);

        // P::__construct(this): $this->x = 7. One param (this), one slot.
        let mut ctor = Function::new(format!("{}::__construct", class), IrType::Void, PhpType::Void);
        ctor.flags.is_method = true;
        ctor.params.push(FunctionParam {
            name: "this".to_string(),
            ir_type: IrType::Heap(IrHeapKind::Object),
            php_type: PhpType::Object(class.to_string()),
            by_ref: false,
            variadic: false,
        });
        let this_slot = ctor.add_local(
            Some("this".to_string()),
            IrType::Heap(IrHeapKind::Object),
            PhpType::Object(class.to_string()),
            LocalKind::PhpLocal,
        );
        {
            let mut b = Builder::new(&mut ctor);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let this = b.emit_load_local(
                this_slot,
                IrType::Heap(IrHeapKind::Object),
                PhpType::Object(class.to_string()),
            );
            let seven = b.emit_const_i64(7);
            emit_prop_set(&mut b, this, prop_data, seven);
            b.terminate(Terminator::Return { value: None });
        }
        module.class_methods.push(ctor);

        // main: $o = new P(); echo $o->x; (ObjectNew with no operands -> 0-arg ctor call)
        let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        main.flags.is_main = true;
        {
            let mut b = Builder::new(&mut main);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let obj = emit_object_new(&mut b, class, class_data);
            let x = emit_prop_get(&mut b, obj, prop_data, IrType::I64, PhpType::Int);
            let _ = b.emit(
                Op::EchoValue,
                vec![x],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(main);
        if let Some(out) = run_main(&module) {
            assert_eq!(out, "7");
        }
    }

    /// `new P(1)` on a class with NO `__construct` must fail lowering (gate: no ctor +
    /// operands -> Unsupported). Confirms the backend rejects args-without-ctor instead of
    /// silently dropping them, mirroring the native `lower_new_object` gate.
    #[test]
    fn object_no_ctor_args_rejected() {
        let class = "P";
        let mut module = Module::new(Target::wasm());
        let class_data = module.data.intern_class_name(class);
        let prop_data = module.data.intern_string("x");
        module.class_infos.insert(
            class.to_string(),
            test_class_info(1, vec![("x".to_string(), PhpType::Int)], vec![None], false),
        );
        let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        main.flags.is_main = true;
        {
            let mut b = Builder::new(&mut main);
            let entry = b.create_named_block("entry", Vec::new());
            b.set_entry(entry);
            b.position_at_end(entry);
            let arg = b.emit_const_i64(1);
            // ObjectNew with 1 arg but no __construct -> generate() must return Err.
            let _ = emit_object_new_with_args(&mut b, class, class_data, vec![arg]);
            // An echo so the block is reachable and well-formed even though we expect a
            // lowering error before this point.
            let x = emit_prop_get(&mut b, arg, prop_data, IrType::I64, PhpType::Int);
            let _ = b.emit(
                Op::EchoValue,
                vec![x],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            b.terminate(Terminator::Return { value: None });
        }
        module.add_function(main);
        let err = generate(&module, Emit::Executable).expect_err("lowering should reject ctor args without __construct");
        assert!(
            err.to_string().contains("no __construct"),
            "unexpected error message: {err}"
        );
    }
}
