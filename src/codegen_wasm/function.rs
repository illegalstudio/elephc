//! Purpose:
//! Lowers a single EIR `Function` to a WebAssembly `FuncBuilder` for the wasm32-wasi
//! backend. Implements the control-flow backbone using a br_table dispatch loop to
//! structure arbitrary CFGs into WebAssembly's structured control flow.
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
//! - SSA values are assigned to dedicated WASM locals; instruction lowering (P2)
//!   will populate them.
//! - Returns lower to the WASM `return` instruction (the function result types are
//!   declared on the builder), so there is no result-carrying outer block.
//! - All terminators transfer control away (br/return/proc_exit/unreachable); control
//!   never falls through to the next block body. br_table's default target and the
//!   post-loop tail are `unreachable`, which keeps a value-returning function's
//!   implicit end well-typed.

use std::collections::HashMap;

use super::WasmError;
use crate::codegen_wasm::values::{declare_local, declare_param, WasmRepr};
use crate::codegen_wasm::wat::{FuncBuilder, ValType};
use crate::ir::{BlockId, Function, InstId, LocalSlotId, Module, Terminator, ValueId};

/// Result type for this module, using the parent module's `WasmError`.
type Result<T> = std::result::Result<T, WasmError>;

/// Context for lowering a single EIR function to WebAssembly.
///
/// Holds references to the module and function being lowered, the `FuncBuilder`
/// for emitting WAT, and maps from EIR IDs to their WebAssembly representations.
struct FnCtx<'a> {
    /// The parent module (for cross-function references during instruction lowering).
    // Read by instruction lowering, which is introduced in the next phase; allow it
    // to be write-only for now.
    #[allow(dead_code)]
    module: &'a Module,
    /// The function being lowered.
    function: &'a Function,
    /// The WAT function builder.
    fb: FuncBuilder,
    /// Maps `ValueId::as_raw()` to the `WasmRepr` of the SSA value's local(s).
    value_locals: HashMap<u32, WasmRepr>,
    /// Maps `LocalSlotId::as_raw()` to the `WasmRepr` of the local slot's local(s).
    slot_locals: HashMap<u32, WasmRepr>,
    /// The `$__state` local holding the current block index for dispatch.
    state_local: String,
    /// Counter for generating unique temp local names (`$__tmp0`, `$__tmp1`, ...).
    temp_counter: u32,
}

impl<'a> FnCtx<'a> {
    /// Looks up the `WasmRepr` for an SSA value.
    ///
    /// Returns `Ok(&WasmRepr)` if found, or `Err(WasmError::Unsupported)` if the
    /// value has no corresponding local (should not happen for valid EIR).
    fn value_repr(&self, v: ValueId) -> Result<&WasmRepr> {
        self.value_locals
            .get(&v.as_raw())
            .ok_or_else(|| WasmError::Unsupported(format!("value {:?} has no repr", v)))
    }

    /// Looks up the `WasmRepr` for a local slot.
    ///
    /// Returns `Ok(&WasmRepr)` if found, or `Err(WasmError::Unsupported)` if the
    /// slot has no corresponding local (should not happen for valid EIR).
    #[allow(dead_code)]
    fn slot_repr(&self, s: LocalSlotId) -> Result<&WasmRepr> {
        self.slot_locals
            .get(&s.as_raw())
            .ok_or_else(|| WasmError::Unsupported(format!("slot {:?} has no repr", s)))
    }

    /// Returns the block index for a `BlockId`.
    ///
    /// Block indices are exactly their raw IDs; this is a convention of the
    /// dispatch loop encoding.
    fn block_index(&self, b: BlockId) -> u32 {
        b.as_raw()
    }

    /// Declares a fresh temp local of the given type and returns its `$name` reference.
    ///
    /// Temp locals are named `$__tmp{N}` where N is `temp_counter` before increment.
    fn fresh_temp(&mut self, ty: ValType) -> String {
        let name = format!("__tmp{}", self.temp_counter);
        self.temp_counter += 1;
        self.fb.local(&name, ty)
    }

    /// Emits `local.get` for each local in the value's `WasmRepr`, in canonical order.
    ///
    /// For `I64`/`F64`/`Ptr`: pushes one value.
    /// For `Str`: pushes ptr then len.
    /// For `Tagged`: pushes payload then tag.
    /// For `Void`: pushes nothing.
    fn emit_load_value(&mut self, v: ValueId) -> Result<()> {
        let repr = self.value_repr(v)?.clone();
        for local_ref in repr.local_refs() {
            self.fb
                .ins(&format!("local.get {}", local_ref), "load value component");
        }
        Ok(())
    }

    /// Emits code to push an `i32` truthiness value (1 or 0) for the given value.
    ///
    /// The value must have `WasmRepr::I64`; emits `local.get`, `i64.const 0`, `i64.ne`.
    /// Returns `Unsupported` for any other representation.
    fn emit_truthy_i32(&mut self, v: ValueId) -> Result<()> {
        let repr = self.value_repr(v)?;
        match repr {
            WasmRepr::I64(local_ref) => {
                self.fb
                    .ins(&format!("local.get {}", local_ref), "load cond value");
                self.fb.ins("i64.const 0", "zero for comparison");
                self.fb.ins("i64.ne", "cond != 0 -> i32 truthy");
                Ok(())
            }
            _ => Err(WasmError::Unsupported(format!(
                "cond of non-i64 type: {:?}",
                repr
            ))),
        }
    }

    /// Copies branch arguments into the target block's parameter locals using
    /// parallel-move-safe ordering.
    ///
    /// Algorithm:
    /// 1. Look up the target block; its `params` are the destination SSA values.
    /// 2. Build SOURCE: for each arg value, append all `$ref`s from its repr.
    /// 3. Build DEST: for each target param, append all `$ref`s from its repr.
    /// 4. Emit `local.get <src>` for each source ref (forward order).
    /// 5. Emit `local.set <dest>` for each dest ref (reverse order).
    ///
    /// This is safe because all gets precede all sets, avoiding clobbering even when
    /// a destination param is also a source arg (e.g. a loop block branching to itself).
    fn materialize_block_args(&mut self, target: BlockId, args: &[ValueId]) -> Result<()> {
        let target_block = self
            .function
            .block(target)
            .ok_or_else(|| WasmError::Unsupported(format!("target block {:?} not found", target)))?;

        let params = &target_block.params;
        if args.len() != params.len() {
            return Err(WasmError::Unsupported(format!(
                "branch arg count {} != param count {}",
                args.len(),
                params.len()
            )));
        }

        // Build flat source and dest local lists.
        let mut src_refs: Vec<String> = Vec::new();
        for arg in args {
            let repr = self.value_repr(*arg)?.clone();
            src_refs.extend(repr.local_refs());
        }

        let mut dest_refs: Vec<String> = Vec::new();
        for param in params {
            let repr = self.value_repr(*param)?.clone();
            dest_refs.extend(repr.local_refs());
        }

        if src_refs.len() != dest_refs.len() {
            return Err(WasmError::Unsupported(format!(
                "source refs {} != dest refs {}",
                src_refs.len(),
                dest_refs.len()
            )));
        }

        if src_refs.is_empty() {
            return Ok(());
        }

        // Emit all gets in forward order.
        for src in &src_refs {
            self.fb
                .ins(&format!("local.get {}", src), "branch arg component");
        }

        // Emit all sets in reverse order (parallel-move-safe).
        for dest in dest_refs.iter().rev() {
            self.fb
                .ins(&format!("local.set {}", dest), "store param component");
        }

        Ok(())
    }
}

/// Lowers one EIR function to a WAT `FuncBuilder`.
///
/// `is_main` functions become the WASI `_start` command entry; others become
/// `$fn_<sanitized-name>` with the parameter and result signature derived from the
/// EIR function.
///
/// Steps:
/// 1. Choose internal name and export.
/// 2. Create `FuncBuilder`.
/// 3. Declare params (non-main only) and result types.
/// 4. Declare state local `$__state`.
/// 5. Declare local slots (params share locals with slots 0..params.len()).
/// 6. Declare SSA value locals.
/// 7. Build `FnCtx`, emit the entry-state prologue, emit the dispatch loop.
pub fn lower_function(module: &Module, function: &Function) -> Result<FuncBuilder> {
    let is_main = function.flags.is_main;

    // Step a: Choose internal name and export.
    let (internal_name, maybe_export) = if is_main {
        ("_entry".to_string(), Some("_start".to_string()))
    } else {
        let sanitized: String = function
            .name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
            .collect();
        (format!("fn_{}", sanitized), None)
    };

    // Step b: Create FuncBuilder.
    let mut fb = FuncBuilder::new(&internal_name);
    if let Some(export_name) = &maybe_export {
        fb.export(export_name);
    }

    // Step c: Declare params (non-main only) and result types.
    let mut param_reprs: Vec<WasmRepr> = Vec::new();
    if !is_main {
        for (i, p) in function.params.iter().enumerate() {
            let repr = declare_param(&mut fb, &format!("p{}", i), p.ir_type);
            param_reprs.push(repr);
        }

        // Declare result types.
        for ty in WasmRepr::val_types(function.return_type) {
            fb.result(ty);
        }
    }

    // Step d: Declare state local.
    let state_local = fb.local("__state", ValType::I32);

    // Step e: Declare local slots.
    let mut slot_locals: HashMap<u32, WasmRepr> = HashMap::new();
    for (idx, slot) in function.locals.iter().enumerate() {
        let slot_id_raw = LocalSlotId::from_raw(idx as u32).as_raw();
        if idx < function.params.len() && !is_main {
            // Param-backed slot: reuse the param's repr (they share the same locals).
            slot_locals.insert(slot_id_raw, param_reprs[idx].clone());
        } else {
            // Fresh slot.
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

    // Emit dispatch loop.
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
/// unreachable          ;; the loop is left only via `return`/`proc_exit`; this keeps
///                      ;; a value-returning function's implicit end well-typed
/// ```
/// Terminators set `$__state` and `br $__dispatch` to jump between blocks, or use
/// `return`/`proc_exit` to leave the function. Because every block body branches
/// away, control never falls through to the next body.
fn emit_dispatch_loop(ctx: &mut FnCtx) -> Result<()> {
    let n = ctx.function.blocks.len();

    // Open the dispatch loop and the default-trap wrapper.
    ctx.fb.raw("(loop $__dispatch");
    ctx.fb.comment("$__dispatch: br_table dispatch loop");
    ctx.fb.raw("(block $__default");
    ctx.fb.comment("$__default: out-of-range dispatch state");

    // Open one wrapper per block, innermost ($__b0) last.
    for k in (0..n).rev() {
        ctx.fb.raw(&format!("(block $__b{}", k));
    }

    // Load the state and dispatch.
    ctx.fb
        .ins(&format!("local.get {}", ctx.state_local), "load dispatch state");
    let mut targets: Vec<String> = (0..n).map(|k| format!("$__b{}", k)).collect();
    targets.push("$__default".to_string());
    ctx.fb
        .ins(&format!("br_table {}", targets.join(" ")), "dispatch on state");

    // Close $__b0 (its body follows immediately).
    if n > 0 {
        ctx.fb.raw(")");
    }

    // Emit each block body. Block k's body sits between the close of $__b{k} and
    // the close of $__b{k+1}; the last block's body sits inside $__default.
    for k in 0..n {
        ctx.fb.comment(&format!("---- block {} ----", k));
        let block = &ctx.function.blocks[k];
        let inst_ids: Vec<InstId> = block.instructions.clone();
        for inst_id in inst_ids {
            lower_instruction(ctx, inst_id)?;
        }
        let terminator = ctx.function.blocks[k]
            .terminator
            .clone()
            .ok_or_else(|| WasmError::Unsupported(format!("block {} has no terminator", k)))?;
        lower_terminator(ctx, &terminator)?;

        // Close the wrapper for block k+1 (so the next body lands after it). The
        // last block needs no close: its body already sits inside $__default.
        if k + 1 < n {
            ctx.fb.raw(")");
        }
    }

    // Close $__default; its target body is the trap.
    ctx.fb.raw(")");
    ctx.fb
        .ins("unreachable", "$__default: out-of-range dispatch state traps");
    // Close the loop, then trap on the (dead) structural fall-through.
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
            // Save the scrutinee into a temp so each case can re-read it.
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

            // Fall-through is the default edge.
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
                // Main is the WASI `_start` command entry; returning exits the process.
                ctx.fb.ins("i32.const 0", "exit status 0");
                ctx.fb.ins("call $wasi_proc_exit", "WASI proc_exit(0)");
            } else {
                // Load the result (if any) onto the stack and return it directly.
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

/// P1 stub: instruction (op) lowering is implemented in a later phase.
///
/// Any block that actually contains instructions is rejected here so no
/// silently-wrong code is emitted.
fn lower_instruction(_ctx: &mut FnCtx, _inst: InstId) -> Result<()> {
    Err(WasmError::Unsupported(
        "instruction lowering (phase P2)".to_string(),
    ))
}
