//! Purpose:
//! Lowers a single EIR `Function` to a WebAssembly `FuncBuilder` for the wasm32-wasi
//! backend. Implements the control-flow backbone: a br_table dispatch loop that
//! structures an arbitrary EIR control-flow graph into WebAssembly's structured
//! control flow, plus terminator lowering.
//!
//! Called from:
//! - `crate::codegen_wasm::generate()` which iterates the module's functions and
//!   calls this for each, adding the result to the `WatModule`.
//!
//! Key details:
//! - EIR blocks are indexed by BlockId.as_raw(); the dispatch loop uses br_table to
//!   jump to the appropriate block body based on a `$__state` local.
//! - Params and their corresponding local slots (indices 0..params.len()) share the
//!   same WASM locals; no prologue copies are needed.
//! - Returns lower to the WASM `return` instruction (the function result types are
//!   declared on the builder), so there is no result-carrying outer block. The
//!   br_table default target and post-loop tail are `unreachable`, keeping a
//!   value-returning function's implicit end well-typed.
//! - Instruction bodies are lowered by `crate::codegen_wasm::inst`.

use std::collections::HashMap;

use super::context::{wasm_fn_symbol, FnCtx, Result};
use super::inst::lower_instruction;
use super::values::{declare_local, declare_param, WasmRepr};
use super::wat::{FuncBuilder, ValType};
use super::WasmError;
use crate::ir::{Function, InstId, LocalSlotId, Module, Terminator};

/// Lowers one EIR function to a WAT `FuncBuilder`.
///
/// `is_main` functions become the WASI `_start` command entry; others become
/// `$fn_<sanitized-name>`, exported under that name so hosts (and tests) can invoke
/// them, with the parameter and result signature derived from the EIR function.
///
/// Steps:
/// 1. Choose internal name and export.
/// 2. Create `FuncBuilder`.
/// 3. Declare params (non-main only) and result types.
/// 4. Declare state local `$__state`.
/// 5. Declare local slots (params share locals with slots 0..params.len()).
/// 6. Declare SSA value locals.
/// 7. Build `FnCtx`, emit the entry-state prologue, emit the dispatch loop.
///
/// `str_literals` is the module-wide string-literal layout (indexed by `DataId`),
/// used by `ConstStr` lowering to address the data segments.
pub fn lower_function(
    module: &Module,
    function: &Function,
    str_literals: &[(u32, u32)],
) -> Result<FuncBuilder> {
    let is_main = function.flags.is_main;

    // Step a: Choose internal name and export.
    let (internal_name, export_name) = if is_main {
        ("_entry".to_string(), "_start".to_string())
    } else {
        let name = wasm_fn_symbol(&function.name);
        (name.clone(), name)
    };

    // Step b: Create FuncBuilder and export it.
    let mut fb = FuncBuilder::new(&internal_name);
    fb.export(&export_name);

    // Step c: Declare params (non-main only) and result types.
    let mut param_reprs: Vec<WasmRepr> = Vec::new();
    if !is_main {
        for (i, p) in function.params.iter().enumerate() {
            let repr = declare_param(&mut fb, &format!("p{}", i), p.ir_type);
            param_reprs.push(repr);
        }
        for ty in WasmRepr::val_types(function.return_type) {
            fb.result(ty);
        }
    }

    // Step d: Declare the dispatch state local.
    let state_local = fb.local("__state", ValType::I32);

    // Step e: Declare local slots (slots 0..params.len() share the param locals).
    let mut slot_locals: HashMap<u32, WasmRepr> = HashMap::new();
    for (idx, slot) in function.locals.iter().enumerate() {
        let slot_id_raw = LocalSlotId::from_raw(idx as u32).as_raw();
        if idx < function.params.len() && !is_main {
            slot_locals.insert(slot_id_raw, param_reprs[idx].clone());
        } else {
            let repr = declare_local(&mut fb, &format!("s{}", idx), slot.ir_type);
            slot_locals.insert(slot_id_raw, repr);
        }
    }

    // Step f: Declare SSA value locals.
    let mut value_locals: HashMap<u32, WasmRepr> = HashMap::new();
    for (idx, value) in function.values.iter().enumerate() {
        let repr = declare_local(&mut fb, &format!("v{}", idx), value.ir_type);
        value_locals.insert(idx as u32, repr);
    }

    // Step g: Build FnCtx.
    let mut ctx = FnCtx {
        module,
        function,
        fb,
        value_locals,
        slot_locals,
        state_local,
        temp_counter: 0,
        str_literals,
    };

    // Prologue: set initial state to the entry block index.
    // (For non-main, params and their slots share the same locals, so no copy is needed.)
    let entry_index = function.entry.as_raw();
    ctx.fb.ins(
        &format!("i32.const {}", entry_index),
        "initial dispatch state = entry block",
    );
    ctx.fb.ins(
        &format!("local.set {}", ctx.state_local),
        "enter the dispatch loop at the entry block",
    );

    emit_dispatch_loop(&mut ctx)?;

    Ok(ctx.fb)
}

/// Emits the br_table dispatch loop containing every block body.
///
/// Structure for `n` blocks (block k's body is reached by `br_table` selecting
/// `$__b{k}`, which lands just after that wrapper closes):
/// ```wat
/// (loop $__dispatch
///   (block $__default
///     (block $__b{n-1}
///       ...
///         (block $__b0
///           local.get $__state
///           br_table $__b0 $__b1 ... $__b{n-1} $__default)
///         ;; block 0 body (instructions + terminator)
///       )
///       ;; block 1 body
///     )
///     ;; block {n-1} body
///   )
///   unreachable        ;; $__default target: out-of-range dispatch state traps
/// )
/// unreachable          ;; the loop is left only via `return`/`proc_exit`; keeps a
///                      ;; value-returning function's implicit end well-typed
/// ```
/// Terminators set `$__state` and `br $__dispatch` to jump between blocks, or use
/// `return`/`proc_exit` to leave the function. Because every block body branches
/// away, control never falls through to the next body.
fn emit_dispatch_loop(ctx: &mut FnCtx) -> Result<()> {
    let n = ctx.function.blocks.len();

    ctx.fb.raw("(loop $__dispatch");
    ctx.fb.comment("$__dispatch: br_table dispatch loop");
    ctx.fb.raw("(block $__default");
    ctx.fb.comment("$__default: out-of-range dispatch state");

    for k in (0..n).rev() {
        ctx.fb.raw(&format!("(block $__b{}", k));
    }

    ctx.fb
        .ins(&format!("local.get {}", ctx.state_local), "load dispatch state");
    let mut targets: Vec<String> = (0..n).map(|k| format!("$__b{}", k)).collect();
    targets.push("$__default".to_string());
    ctx.fb
        .ins(&format!("br_table {}", targets.join(" ")), "dispatch on state");

    // Close $__b0; its body follows immediately.
    if n > 0 {
        ctx.fb.raw(")");
    }

    for k in 0..n {
        ctx.fb.comment(&format!("---- block {} ----", k));
        let inst_ids: Vec<InstId> = ctx.function.blocks[k].instructions.clone();
        for inst_id in inst_ids {
            lower_instruction(ctx, inst_id)?;
        }
        let terminator = ctx.function.blocks[k]
            .terminator
            .clone()
            .ok_or_else(|| WasmError::Unsupported(format!("block {} has no terminator", k)))?;
        lower_terminator(ctx, &terminator)?;

        // Close the wrapper for block k+1; the last block's body sits inside $__default.
        if k + 1 < n {
            ctx.fb.raw(")");
        }
    }

    ctx.fb.raw(")");
    ctx.fb
        .ins("unreachable", "$__default: out-of-range dispatch state traps");
    ctx.fb.raw(")");
    ctx.fb
        .ins("unreachable", "dispatch loop is left only via return/proc_exit");

    Ok(())
}

/// Lowers a terminator to WebAssembly control flow.
///
/// Handles:
/// - `Unreachable`: emits `unreachable`.
/// - `Br`: materializes args, sets state, `br $__dispatch`.
/// - `CondBr`: emits if/else, each branch materializing args and re-dispatching.
/// - `Switch`: emits cascaded i64 comparisons; falls through to the default edge.
/// - `Return`: for main, calls `proc_exit(0)`; for others, loads the value and `return`s.
/// - `Throw`, `Fatal`, `GeneratorSuspend`: returns `Unsupported` (later phases).
fn lower_terminator(ctx: &mut FnCtx, term: &Terminator) -> Result<()> {
    match term {
        Terminator::Unreachable => {
            ctx.fb.ins("unreachable", "EIR unreachable");
            Ok(())
        }

        Terminator::Br { target, args } => {
            ctx.materialize_block_args(*target, args)?;
            let idx = ctx.block_index(*target);
            ctx.fb
                .ins(&format!("i32.const {}", idx), &format!("goto block {}", idx));
            ctx.fb
                .ins(&format!("local.set {}", ctx.state_local), "set next dispatch state");
            ctx.fb.ins("br $__dispatch", "continue dispatch loop");
            Ok(())
        }

        Terminator::CondBr {
            cond,
            then_target,
            then_args,
            else_target,
            else_args,
        } => {
            ctx.emit_truthy_i32(*cond)?;
            ctx.fb.raw("(if");
            ctx.fb.raw("(then");
            ctx.materialize_block_args(*then_target, then_args)?;
            let then_idx = ctx.block_index(*then_target);
            ctx.fb.ins(
                &format!("i32.const {}", then_idx),
                &format!("then: goto block {}", then_idx),
            );
            ctx.fb
                .ins(&format!("local.set {}", ctx.state_local), "set next dispatch state");
            ctx.fb.ins("br $__dispatch", "continue dispatch loop");
            ctx.fb.raw(")");
            ctx.fb.raw("(else");
            ctx.materialize_block_args(*else_target, else_args)?;
            let else_idx = ctx.block_index(*else_target);
            ctx.fb.ins(
                &format!("i32.const {}", else_idx),
                &format!("else: goto block {}", else_idx),
            );
            ctx.fb
                .ins(&format!("local.set {}", ctx.state_local), "set next dispatch state");
            ctx.fb.ins("br $__dispatch", "continue dispatch loop");
            ctx.fb.raw(")");
            ctx.fb.raw(")");
            Ok(())
        }

        Terminator::Switch {
            scrutinee,
            cases,
            default,
            default_args,
        } => {
            let scrut_temp = ctx.fresh_temp(ValType::I64);
            ctx.emit_load_value(*scrutinee)?;
            ctx.fb
                .ins(&format!("local.set {}", scrut_temp), "save scrutinee for switch");

            for case in cases {
                ctx.fb
                    .ins(&format!("local.get {}", scrut_temp), "reload scrutinee");
                ctx.fb.ins(&format!("i64.const {}", case.value), "case value");
                ctx.fb.ins("i64.eq", "compare scrutinee to case value");
                ctx.fb.raw("(if");
                ctx.fb.raw("(then");
                ctx.materialize_block_args(case.target, &case.args)?;
                let case_idx = ctx.block_index(case.target);
                ctx.fb.ins(
                    &format!("i32.const {}", case_idx),
                    &format!("case: goto block {}", case_idx),
                );
                ctx.fb
                    .ins(&format!("local.set {}", ctx.state_local), "set next dispatch state");
                ctx.fb.ins("br $__dispatch", "continue dispatch loop");
                ctx.fb.raw(")");
                ctx.fb.raw(")");
            }

            ctx.materialize_block_args(*default, default_args)?;
            let default_idx = ctx.block_index(*default);
            ctx.fb.ins(
                &format!("i32.const {}", default_idx),
                &format!("default: goto block {}", default_idx),
            );
            ctx.fb
                .ins(&format!("local.set {}", ctx.state_local), "set next dispatch state");
            ctx.fb.ins("br $__dispatch", "continue dispatch loop");
            Ok(())
        }

        Terminator::Return { value } => {
            if ctx.function.flags.is_main {
                ctx.fb.ins("i32.const 0", "exit status 0");
                ctx.fb.ins("call $wasi_proc_exit", "WASI proc_exit(0)");
            } else {
                if let Some(v) = value {
                    ctx.emit_load_value(*v)?;
                }
                ctx.fb.ins("return", "return from function");
            }
            Ok(())
        }

        Terminator::Throw { .. } => Err(WasmError::Unsupported("throw terminator".to_string())),

        Terminator::Fatal { .. } => Err(WasmError::Unsupported("fatal terminator".to_string())),

        Terminator::GeneratorSuspend { .. } => Err(WasmError::Unsupported(
            "generator-suspend terminator".to_string(),
        )),
    }
}
