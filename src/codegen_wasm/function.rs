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

use super::context::{FnCtx, Result};
use super::inst::{lower_instruction, reserve_iterators};
use super::symbols::function_symbol;
use super::transfer;
use super::values::{declare_local, declare_param, WasmRepr};
use super::wat::{FuncBuilder, ValType};
use super::WasmError;
use crate::ir::{
    Function, Immediate, InstId, IrHeapKind, IrType, LocalSlotId, Module, Op, RuntimeCallTarget,
    RuntimeFnId, Terminator, ValueDef, ValueId,
};
use crate::types::PhpType;

/// Lowers one EIR function to a WAT `FuncBuilder`.
///
/// `is_main` functions become the WASI `_start` command entry; others become
/// `$fn_<sanitized-name>`, exported under that name so hosts (and tests) can invoke
/// them, with the parameter and result signature derived from the EIR function.
///
/// Steps:
/// 1. Choose internal name and export.
/// 2. Create `FuncBuilder`.
/// 3. Declare params (non-main only) and result types. By-ref params (`p.by_ref`)
///    are declared as a single i32 cell pointer (`WasmRepr::Ptr`) rather than their
///    value type: pointer-ness is a caller/callee ABI agreement, and the callee body
///    reads/writes the value through it via `LoadRefCell`/`StoreRefCell`.
/// 4. Declare state local `$__state`.
/// 5. Declare local slots (params share locals with slots 0..params.len()).
/// 6. Declare SSA value locals.
/// 7. Build `FnCtx`, register by-ref param slots in `ref_cell_ptrs` (the callee
///    borrows the caller's cell — no owner is recorded), emit the entry-state
///    prologue, emit the dispatch loop.
///
/// `str_literals` is the module-wide string-literal layout (indexed by `DataId`),
/// used by `ConstStr` lowering to address the data segments. `default_strings` is
/// the content-keyed layout of the class property string defaults that object
/// construction writes inline, which carry no `DataId` at the construction site
/// (see `objects::literal_default_strings`). `closure_tag_ptrs`
/// is the per-closure capture-tag-array base address layout (indexed by
/// `module.closures` position), used by `ClosureNew` lowering to stamp the
/// descriptor's `capture_tags_ptr`. `fcc_entries` is the distinct first-class
/// callable free-function target list (P7d2a), used by
/// `FirstClassCallableNew` lowering to resolve a target's unified ladder
/// `entry_index`.
pub fn lower_function(
    module: &Module,
    function: &Function,
    str_literals: &[(u32, u32)],
    default_strings: &std::collections::HashMap<String, (u32, u32)>,
    closure_tag_ptrs: &[u32],
    fcc_entries: &[String],
    static_slots: &super::statics::StaticSlots,
) -> Result<FuncBuilder> {
    let is_main = function.flags.is_main;

    // Step a: Choose internal name and export.
    let (internal_name, export_name) = if is_main {
        ("_entry".to_string(), "_start".to_string())
    } else {
        let name = function_symbol(function);
        (name.clone(), name)
    };

    // Step b: Create FuncBuilder and export it.
    let mut fb = FuncBuilder::new(&internal_name);
    fb.export(&export_name);

    // Step c: Declare params (non-main only) and result types.
    let mut param_reprs: Vec<WasmRepr> = Vec::new();
    if !is_main {
        for (i, p) in function.params.iter().enumerate() {
            let repr = if p.by_ref {
                // A by-ref free-function parameter is a single i32 carrying the caller's
                // ref-cell pointer (P7c0b). The callee body reads/writes the value through
                // it via `LoadRefCell`/`StoreRefCell`, which look the pointer up in
                // `ref_cell_ptrs` (registered in the prologue below). The value type is
                // NOT declared as a local here: pointer-ness is a caller/callee ABI
                // agreement, not a value slot, so the slot's `WasmRepr` is `Ptr`.
                let ptr = fb.param(&format!("p{}", i), ValType::I32);
                WasmRepr::Ptr(ptr)
            } else {
                declare_param(&mut fb, &format!("p{}", i), p.ir_type)
            };
            param_reprs.push(repr);
        }
        for ty in WasmRepr::val_types(function.return_type) {
            fb.result(ty);
        }
    }

    // Step d: Declare the dispatch state local and the concat-base local.
    let state_local = fb.local("__state", ValType::I32);
    let handler_local = fb.local("__handler", ValType::I32);
    // One save slot per distinct try token, mirroring the native backend's per-token
    // frame slot. `TryPushHandler` stows the enclosing handler here and `TryPopHandler`
    // restores it, which is what makes nested `try` work without a runtime stack.
    let mut handler_saves: HashMap<i64, String> = HashMap::new();
    for inst in &function.instructions {
        if inst.op != Op::TryPushHandler {
            continue;
        }
        if let Some(Immediate::I64(token)) = inst.immediate {
            handler_saves.entry(token).or_insert_with(|| {
                fb.local(&format!("__handler_save{}", token), ValType::I32)
            });
        }
    }
    let concat_base_local = fb.local("__concat_base", ValType::I32);

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
        handler_local,
        handler_saves,
        concat_base_local,
        temp_counter: 0,
        str_literals,
        default_strings,
        closure_tag_ptrs,
        fcc_entries,
        static_slots,
        iter_state: std::collections::HashMap::new(),
        ref_cell_ptrs: std::collections::HashMap::new(),
        owned_ref_cell_slots: std::collections::HashSet::new(),
        ref_cell_owners: Vec::new(),
    };

    // Register by-ref parameter slots in `ref_cell_ptrs` so the callee body's
    // `LoadRefCell`/`StoreRefCell` recover the caller-supplied cell pointer. The callee
    // borrows the caller's cell (no owner recorded): only the caller releases it — via
    // the temp-cell writeback for fresh locals, or the caller's existing owner for an
    // already-ref-bound local. Mirrors the native frame-address borrow.
    if !is_main {
        for (i, p) in function.params.iter().enumerate() {
            if p.by_ref {
                let slot_raw = LocalSlotId::from_raw(i as u32).as_raw();
                match &param_reprs[i] {
                    WasmRepr::Ptr(ptr_local) => {
                        ctx.register_ref_cell_ptr(slot_raw, ptr_local.clone(), false);
                    }
                    _ => unreachable!("by-ref param {} must be declared as Ptr", i),
                }
            }
        }
    }

    // EIR blocks can be stored in an order where a foreach loop header precedes
    // the block containing IterStart. Reserve iterator locals before lowering any
    // block so later instruction dispatch is independent of storage order.
    reserve_iterators(&mut ctx)?;

    // Prologue: capture this frame's concat-buffer baseline, then set the initial
    // dispatch state. (For non-main, params and their slots share locals, so no
    // parameter copy is needed.)
    ctx.fb
        .ins("global.get $__concat_off", "capture this frame's concat baseline");
    ctx.fb
        .ins(&format!("local.set {}", ctx.concat_base_local), "");
    let entry_index = function.entry.as_raw();
    ctx.fb.ins(
        &format!("i32.const {}", entry_index),
        "initial dispatch state = entry block",
    );
    ctx.fb.ins(
        &format!("local.set {}", ctx.state_local),
        "enter the dispatch loop at the entry block",
    );

    if is_main {
        emit_main_argc_argv_init(&mut ctx)?;
    }

    emit_dispatch_loop(&mut ctx)?;

    Ok(ctx.fb)
}

/// Initializes source `$argc` and `$argv` locals in a `main` function prologue.
///
/// Locates the EIR local slots by name (the same metadata the native backend
/// uses) and calls the WASI `__rt_argc`/`__rt_argv` helpers. Mixed-cell slots
/// receive the value through the typed transfer layer, which boxes the concrete
/// integer or array pointer.
fn emit_main_argc_argv_init(ctx: &mut FnCtx) -> Result<()> {
    if let Some(argc_slot) = ctx
        .function
        .locals
        .iter()
        .enumerate()
        .find(|(_, local)| local.name.as_deref() == Some("argc"))
        .map(|(idx, _)| LocalSlotId::from_raw(idx as u32))
    {
        ctx.fb.ins("call $__rt_argc", "load $argc from WASI");
        transfer::emit_store_stack_value_into_slot(
            ctx,
            IrType::I64,
            PhpType::Int,
            argc_slot,
        )?;
    }

    if let Some(argv_slot) = ctx
        .function
        .locals
        .iter()
        .enumerate()
        .find(|(_, local)| local.name.as_deref() == Some("argv"))
        .map(|(idx, _)| LocalSlotId::from_raw(idx as u32))
    {
        ctx.fb.ins("call $__rt_argv", "build $argv from WASI");
        transfer::emit_store_stack_value_into_slot(
            ctx,
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Str)),
            argv_slot,
        )?;
    }

    Ok(())
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
/// Name of the single PHP exception tag; its payload is the exception object pointer.
pub(super) const EXCEPTION_TAG: &str = "__php_exc";

/// Name of the global holding the exception currently being handled.
///
/// Mirrors the native backend's `_exc_value` symbol, which `CatchCurrent` reads.
pub(super) const EXCEPTION_VALUE_GLOBAL: &str = "__exc_value";

/// Name of the global carrying the diagnostic `main` prints if an exception is never caught.
///
/// Every raise site sets it immediately before `throw`, so the value in flight always belongs
/// to the exception in flight. A runtime error names its own PHP class and message; an ordinary
/// `throw` restores the class-agnostic default, which is what keeps a user exception raised
/// AFTER a caught `DivisionByZeroError` from inheriting that error's diagnostic.
pub(super) const EXCEPTION_FATAL_CODE_GLOBAL: &str = "__exc_fatal_code";

/// `__rt_fail` code for an exception that reaches the top of `main` with no `catch`.
pub(super) const UNCAUGHT_EXCEPTION_FAILURE_CODE: i32 = 10;

/// Returns whether a function participates in exception handling.
///
/// Only such functions pay for the `try_table` wrapper and the handler slot, so a
/// module without `try`/`throw` emits exactly the code it did before.
pub(super) fn uses_exceptions(function: &Function) -> bool {
    function.instructions.iter().any(|inst| {
        matches!(
            inst.op,
            Op::TryPushHandler | Op::TryPopHandler | Op::ThrowException | Op::ThrowErrorValue
        )
    }) || function
        .blocks
        .iter()
        .any(|block| matches!(block.terminator, Some(Terminator::Throw { .. })))
}

/// Returns whether a function holds an operation whose lowering can raise a PHP runtime Error.
///
/// PHP does not treat a division by zero or a negative shift count as an immediate fatal: it
/// raises `DivisionByZeroError` / `ArithmeticError`, which a `catch` can receive like any other
/// `Throwable`. This is what tells `plan_module` to lay out those errors' message text.
///
/// It deliberately does NOT make a module declare the exception tag. A module with no `try`
/// anywhere has no clause that could ever receive such an error, so raising it and reporting it
/// directly are indistinguishable — same message, same 255 exit — and staying on the direct path
/// keeps a program that merely divides runnable on a host without the exceptions proposal.
///
/// This list mirrors `inst::lower_instruction`'s dispatch to the lowerings that call
/// `emit_runtime_failure`. Missing an entry costs catchability at that site — the failure path
/// falls back to the deterministic fatal — but never emits a `throw` without a declared tag,
/// because `emit_runtime_failure` re-checks `module_uses_exceptions` before raising.
pub(super) fn raises_runtime_error(function: &Function) -> bool {
    function.instructions.iter().any(|inst| {
        matches!(
            inst.op,
            Op::IShl | Op::IShrA | Op::IDiv | Op::ISDiv | Op::ISMod | Op::FDiv
        ) || matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(
                RuntimeCallTarget::Function(
                    RuntimeFnId::Intdiv
                        | RuntimeFnId::StrRepeat
                        | RuntimeFnId::StrPad
                        | RuntimeFnId::Explode
                        | RuntimeFnId::StrSplit
                ) | RuntimeCallTarget::ProfiledFunction {
                    target:
                        RuntimeFnId::Intdiv
                        | RuntimeFnId::StrRepeat
                        | RuntimeFnId::StrPad
                        | RuntimeFnId::Explode
                        | RuntimeFnId::StrSplit,
                    ..
                }
            ))
        )
    })
}

/// Returns whether ANY function, class method or closure in the module can raise a runtime Error.
pub(super) fn module_raises_runtime_errors(module: &Module) -> bool {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .any(raises_runtime_error)
}

/// Returns whether ANY function, class method or closure in the module uses exceptions.
///
/// This is what decides whether the module declares the exception tag and whether `main` is
/// guarded. It deliberately spans every function list rather than `module.functions` alone: a
/// `throw` living only inside a method or a closure body still needs the tag declared and still
/// unwinds into `main`. It is also what lets a runtime Error be RAISED rather than reported
/// directly, because the two are only distinguishable once some frame can catch.
pub(super) fn module_uses_exceptions(module: &Module) -> bool {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .any(uses_exceptions)
}

/// Returns whether a function can catch, and so needs the `try_table` wrapper.
///
/// A function that only throws propagates to its caller and needs no landing pad.
fn catches_exceptions(function: &Function) -> bool {
    function
        .instructions
        .iter()
        .any(|inst| inst.op == Op::TryPushHandler)
}

fn emit_dispatch_loop(ctx: &mut FnCtx) -> Result<()> {
    let n = ctx.function.blocks.len();
    // `main` guards the whole program even when it contains no `catch` of its own: an exception
    // that unwinds past it is PHP's uncaught fatal, and without a landing pad here the WASM
    // exception would escape the module and surface as a HOST-level crash instead.
    let catches = catches_exceptions(ctx.function)
        || (ctx.function.flags.is_main && module_uses_exceptions(ctx.module));
    if catches {
        ctx.fb.ins("i32.const -1", "no handler is armed yet");
        ctx.fb.ins(
            &format!("local.set {}", ctx.handler_local),
            "arm nothing until a try is entered",
        );
    }

    ctx.fb.raw("(loop $__dispatch");
    ctx.fb.comment("$__dispatch: br_table dispatch loop");
    if catches {
        // An EIR block is a flat `br_table` case, not a lexical region, so a `try`
        // cannot be a nested scope. Catching instead becomes an ordinary state
        // transition: the landing pad below reads the armed handler's catch-block
        // index and re-dispatches, exactly like every other control transfer here.
        ctx.fb.raw(&format!("(block $__caught (result i32)"));
        ctx.fb.comment("$__caught: receives the thrown exception pointer");
        ctx.fb
            .raw(&format!("(try_table (catch ${} $__caught)", EXCEPTION_TAG));
    }
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
        let last_op = inst_ids
            .last()
            .and_then(|inst_id| ctx.function.instruction(*inst_id))
            .map(|instruction| instruction.op);
        for inst_id in inst_ids {
            lower_instruction(ctx, inst_id)?;
        }
        let terminator = ctx.function.blocks[k]
            .terminator
            .clone()
            .ok_or_else(|| WasmError::Unsupported(format!("block {} has no terminator", k)))?;
        lower_terminator(ctx, &terminator, last_op)?;

        // Close the wrapper for block k+1; the last block's body sits inside $__default.
        if k + 1 < n {
            ctx.fb.raw(")");
        }
    }

    ctx.fb.raw(")");
    ctx.fb.ins(
        "unreachable",
        "elephc-trap:proven-invariant:dispatch-state-range $__default rejects an out-of-range dispatch state",
    );
    if catches {
        ctx.fb.raw(")");
        ctx.fb.ins(
            "unreachable",
            "elephc-trap:proven-invariant:try-table-tail the guarded region is left only by a branch",
        );
        ctx.fb.raw(")");
        // Landing pad. The tag payload is on the stack; publish it where
        // `CatchCurrent` reads it, then resume at the armed handler's block.
        ctx.fb.ins(
            &format!("global.set ${}", EXCEPTION_VALUE_GLOBAL),
            "publish the exception being handled",
        );
        // A frame can catch SOMEWHERE without a try being active at the throw point — the
        // handler is `-1` outside every try, and outside `main` that means the exception
        // belongs to the caller. Re-raising is what makes the sentinel necessary: block 0 is a
        // legitimate handler index, so "no handler" cannot be spelled as zero.
        ctx.fb
            .ins(&format!("local.get {}", ctx.handler_local), "armed handler");
        ctx.fb.ins("i32.const 0", "handler sentinel bound");
        ctx.fb.ins("i32.lt_s", "no try is active in this frame?");
        ctx.fb.ins("if", "unhandled here");
        if ctx.function.flags.is_main {
            ctx.fb.ins(
                &format!("global.get ${}", EXCEPTION_FATAL_CODE_GLOBAL),
                "diagnostic the raise site chose for this exception",
            );
            ctx.fb.ins(
                "call $__rt_fail",
                "report PHP's uncaught fatal and exit with status 255",
            );
            ctx.fb.ins(
                "unreachable",
                "elephc-trap:post-noreturn:uncaught-exception-exit runtime fatal helper does not return",
            );
        } else {
            ctx.fb.ins(
                &format!("global.get ${}", EXCEPTION_VALUE_GLOBAL),
                "the exception still in flight",
            );
            ctx.fb.ins(
                &format!("throw ${}", EXCEPTION_TAG),
                "propagate it to the caller's landing pad",
            );
        }
        ctx.fb.ins("end", "end unhandled check");
        ctx.fb.ins(
            &format!("local.get {}", ctx.handler_local),
            "armed handler's catch block",
        );
        ctx.fb.ins(
            &format!("local.set {}", ctx.state_local),
            "resume dispatch at the catch block",
        );
        ctx.fb.raw("(br $__dispatch)");
    }
    ctx.fb.raw(")");
    ctx.fb.ins(
        "unreachable",
        "elephc-trap:proven-invariant:dispatch-loop-tail dispatch loop is left only via return/proc_exit",
    );

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
/// - `Throw`, `Fatal`, `GeneratorSuspend`: defensive `Unsupported` fallbacks;
///   the capability audit rejects them before this lowerer is entered.
fn lower_terminator(ctx: &mut FnCtx, term: &Terminator, preceding_op: Option<Op>) -> Result<()> {
    match term {
        Terminator::Unreachable => {
            let marker = if preceding_op == Some(Op::ThrowError) {
                "elephc-trap:post-noreturn:method-null-error method fatal helper does not return"
            } else {
                "elephc-trap:proven-invariant:eir-unreachable EIR unreachable terminator"
            };
            ctx.fb.ins("unreachable", marker);
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
            let returned_slot = value.and_then(|v| returned_local_slot(ctx.function, v));
            // Owner-slot release epilogue: release every ref-cell owner before
            // leaving the function. Runs first so the return value (pushed next)
            // is not stranded across the epilogue's local.get/local.set. Idempotent
            // via the null-guard — explicit ReleaseLocalRefCell ops already zeroed
            // their owners, so the epilogue skips them. Mirrors the native
            // emit_ref_cell_owner_epilogue_cleanup at every exit.
            ctx.emit_ref_cell_owner_epilogue()?;
            // Release by-value closure captures the body reassigned (now slot-owned),
            // skipping the one this return moves out. Mirrors the native
            // reassigned_capture_epilogue_locals. Runs before the return value is
            // pushed so its local.get/local.set never strand the result.
            ctx.emit_reassigned_capture_epilogue(returned_slot)?;
            // Release ordinary owned locals that do not move out through this
            // return. This is what runs object destructors at function end and
            // during main shutdown, and balances owned strings/containers.
            ctx.emit_local_epilogue_cleanup(returned_slot)?;
            if ctx.function.flags.is_main {
                ctx.fb.ins("i32.const 0", "exit status 0");
                ctx.fb.ins("call $wasi_proc_exit", "WASI proc_exit(0)");
                ctx.fb.ins(
                    "unreachable",
                    "elephc-trap:post-noreturn:main-proc-exit WASI proc_exit is non-returning",
                );
            } else {
                if let Some(v) = value {
                    ctx.emit_load_value(*v)?;
                }
                ctx.fb.ins("return", "return from function");
            }
            Ok(())
        }

        Terminator::Throw { value } => {
            // Raising leaves the frame, so no state transition follows: either an
            // enclosing `try_table` in this function catches it, or it propagates to
            // the caller's landing pad.
            ctx.emit_load_value(*value)?;
            ctx.fb.ins(
                &format!("i32.const {}", UNCAUGHT_EXCEPTION_FAILURE_CODE),
                "class-agnostic diagnostic for a user-raised exception",
            );
            ctx.fb.ins(
                &format!("global.set ${}", EXCEPTION_FATAL_CODE_GLOBAL),
                "claim the uncaught diagnostic for this exception",
            );
            ctx.fb.ins(
                &format!("throw ${}", EXCEPTION_TAG),
                "raise the PHP exception",
            );
            Ok(())
        }

        // A terminating PHP fatal whose message the EIR already interned — `match` with no
        // arm taken is the one that reaches here. Writing that exact text and exiting 255 is
        // what the native does, so the two targets report identically.
        Terminator::Fatal { message } => {
            let (pointer, length) = ctx.str_literal(*message)?;
            ctx.fb.ins(
                &format!(
                    "(call $__rt_wasi_write_or_fail (i32.const 2) (i32.const {pointer}) (i32.const {length}))"
                ),
                "the interned fatal message, on stderr",
            );
            ctx.fb.ins(
                "(call $wasi_proc_exit (i32.const 255))",
                "PHP's fatal exit status",
            );
            ctx.fb.ins(
                "unreachable",
                "elephc-trap:post-noreturn:fatal-terminator",
            );
            Ok(())
        }

        Terminator::GeneratorSuspend { .. } => Err(WasmError::Unsupported(
            "generator-suspend terminator".to_string(),
        )),
    }
}

/// Returns the local slot whose `LoadLocal` directly provides a returned value.
///
/// Used by the `Return` epilogue to skip a reassigned closure-capture slot that is
/// returned: the WASM return moves the slot's value out (no incref), so the capture
/// release epilogue must not also release it. Recurses through the `ArrayToMixed` /
/// `HashToMixed` in-place conversions and ownership-neutral `Move`/`Borrow`
/// forwarding, mirroring the native `direct_return_local_slot`. `Acquire` is
/// intentionally not followed because it creates an independent owned value.
fn returned_local_slot(function: &Function, value: ValueId) -> Option<LocalSlotId> {
    let value = function.value(value)?;
    let ValueDef::Instruction { inst, .. } = value.def else {
        return None;
    };
    let inst = function.instruction(inst)?;
    match inst.op {
        Op::LoadLocal => match inst.immediate {
            Some(Immediate::LocalSlot(slot)) => Some(slot),
            _ => None,
        },
        Op::ArrayToMixed | Op::HashToMixed | Op::Move | Op::Borrow => {
            let source = *inst.operands.first()?;
            returned_local_slot(function, source)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;
    use crate::ir::{Builder, LocalKind, Ownership};

    /// Verifies the exception opcodes lower to the Core WebAssembly exception forms.
    ///
    /// One `try_table` guards the dispatch loop, `throw` carries the exception object's pointer
    /// on the module's single tag, and the landing pad publishes the exception where
    /// `CatchCurrent`/`CatchBind` read it before resuming dispatch at the armed catch block.
    /// The sentinel check is what distinguishes "no `try` is active" from "the handler is block
    /// zero", which a plain zero-initialized handler local could not express.
    #[test]
    fn exception_ops_lower_to_core_wasm_forms() {
        let mut module = Module::new(Target::wasm());
        let mut function = Function::new("thrower".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let exception_slot = builder.add_local(
                Some("e".to_string()),
                IrType::Heap(IrHeapKind::Object),
                PhpType::Object("Exception".to_string()),
                LocalKind::PhpLocal,
            );
            let entry = builder.create_named_block("entry", Vec::new());
            let catch = builder.create_named_block("catch", Vec::new());
            builder.set_entry(entry);

            builder.position_at_end(entry);
            builder
                .emit(
                    Op::TryPushHandler,
                    Vec::new(),
                    Some(Immediate::I64(catch.as_raw() as i64)),
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            // Any object-typed value will do as the payload: what is under test is the tag
            // carrying it, not where it came from.
            let raised = builder.emit_load_local(
                exception_slot,
                IrType::Heap(IrHeapKind::Object),
                PhpType::Object("Exception".to_string()),
            );
            builder.terminate(Terminator::Throw { value: raised });

            builder.position_at_end(catch);
            builder
                .emit(
                    Op::TryPopHandler,
                    Vec::new(),
                    Some(Immediate::I64(catch.as_raw() as i64)),
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            builder
                .emit(
                    Op::CatchCurrent,
                    Vec::new(),
                    None,
                    IrType::Heap(IrHeapKind::Object),
                    PhpType::Object("Throwable".to_string()),
                    Ownership::MaybeOwned,
                )
                .expect("read the exception in flight");
            builder
                .emit(
                    Op::CatchBind,
                    Vec::new(),
                    None,
                    IrType::Heap(IrHeapKind::Object),
                    PhpType::Object("Exception".to_string()),
                    Ownership::Owned,
                )
                .expect("take the exception");
            builder.terminate(Terminator::Return { value: None });
        }

        assert!(
            uses_exceptions(&function),
            "a function that throws participates in exception handling"
        );
        assert!(
            catches_exceptions(&function),
            "a function arming a handler needs the try_table wrapper"
        );

        module.functions.push(function.clone());
        let lowered = lower_function(
            &module,
            &function,
            &[],
            &Default::default(),
            &[],
            &[],
            &Default::default(),
        )
        .expect("exception lowering");
        let wat = lowered.render("  ");

        assert!(
            wat.contains(&format!("(try_table (catch ${} $__caught)", EXCEPTION_TAG)),
            "the dispatch loop must be guarded: {wat}"
        );
        assert!(
            wat.contains(&format!("throw ${}", EXCEPTION_TAG)),
            "the raise site must throw the tag: {wat}"
        );
        assert!(
            wat.contains(&format!("global.set ${}", EXCEPTION_VALUE_GLOBAL)),
            "the landing pad must publish the exception: {wat}"
        );
        assert!(
            wat.contains("i32.const -1"),
            "the handler must start disarmed with a sentinel: {wat}"
        );
        assert!(
            wat.contains("i32.lt_s"),
            "the landing pad must distinguish an unarmed handler: {wat}"
        );
    }
}
