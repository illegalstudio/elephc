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
    Function, Immediate, InstId, Instruction, IrHeapKind, IrType, LocalSlotId, Module, Op,
    RuntimeCallTarget, RuntimeFnId, Terminator, ValueDef, ValueId,
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
    regex_programs: &std::collections::HashMap<String, (u32, u32)>,
    unset_properties: &[(String, String)],
) -> Result<FuncBuilder> {
    let is_main = function.flags.is_main;
    // A generator's EIR function is lowered as its RESUME body: the thing that can be
    // re-entered after a yield. `generators::emit_wrapper` emits what `g(...)` itself
    // becomes, under the ordinary symbol, so this takes the `__resume` one.
    let is_generator = function.flags.is_generator;
    let generator_frame = is_generator.then(|| super::generators::frame_layout(function));

    // Step a: Choose internal name and export.
    let (internal_name, export_name) = if is_main {
        ("_entry".to_string(), "_start".to_string())
    } else if is_generator {
        let name = super::generators::resume_symbol(function);
        (name.clone(), name)
    } else {
        let name = function_symbol(function);
        (name.clone(), name)
    };

    // Step b: Create FuncBuilder and export it.
    let mut fb = FuncBuilder::new(&internal_name);
    fb.export(&export_name);

    // Step c: Declare params (non-main only) and result types.
    let mut param_reprs: Vec<WasmRepr> = Vec::new();
    // A resume body takes the FRAME and nothing else — the php parameters live in it,
    // because they have to survive between two calls — and answers `(has_value, value)`.
    let frame_local = if is_generator {
        let frame = fb.param("frame", ValType::I32);
        fb.result(ValType::I32);
        fb.result(ValType::I32);
        Some(frame)
    } else {
        None
    };
    if !is_main && !is_generator {
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
        if idx < function.params.len() && !is_main && !is_generator {
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
        regex_programs,
        closure_tag_ptrs,
        fcc_entries,
        static_slots,
        unset_properties,
        generator: generator_frame
            .clone()
            .zip(frame_local.clone())
            .map(|(frame, local)| (frame, local)),
        segments: Vec::new(),
        block_entry: Vec::new(),
        segment: 0,
        iter_state: std::collections::HashMap::new(),
        ref_cell_ptrs: std::collections::HashMap::new(),
        owned_ref_cell_slots: std::collections::HashSet::new(),
        ref_cell_owners: Vec::new(),
        capture_owned_flags: std::collections::HashMap::new(),
        usage_index: std::cell::OnceCell::new(),
    };

    // One "this slot owns its value" flag per reassigned capture that starts out borrowing the
    // descriptor's reference (see `FnCtx::capture_owned_flags`). WASM zeroes locals on entry, so
    // every call starts borrowed. A generator is left out: its frame spills only EIR locals, so a
    // flag would not survive a yield, and that body keeps the unconditional epilogue.
    if ctx.generator.is_none() {
        let mut reassigned: Vec<u32> = function
            .reassigned_capture_slots
            .iter()
            .map(|slot| slot.as_raw())
            .collect();
        reassigned.sort_unstable();
        for raw in reassigned {
            let slot = LocalSlotId::from_raw(raw);
            let Some(local) = function.locals.get(raw as usize) else {
                continue;
            };
            let local_ty = local.php_type.codegen_repr();
            if super::inst::capture_slot_still_borrows_from_descriptor(&ctx, slot, &local_ty) {
                let flag = ctx.fresh_temp(super::wat::ValType::I32);
                ctx.capture_owned_flags.insert(raw, flag);
            }
        }
    }

    // Register by-ref parameter slots in `ref_cell_ptrs` so the callee body's
    // `LoadRefCell`/`StoreRefCell` recover the caller-supplied cell pointer. The callee
    // borrows the caller's cell (no owner recorded): only the caller releases it — via
    // the temp-cell writeback for fresh locals, or the caller's existing owner for an
    // already-ref-bound local. Mirrors the native frame-address borrow.
    if !is_main {
        for (i, p) in function.params.iter().enumerate() {
            if p.by_ref {
                let slot_raw = LocalSlotId::from_raw(i as u32).as_raw();
                match param_reprs.get(i) {
                    Some(WasmRepr::Ptr(ptr_local)) => {
                        ctx.register_ref_cell_ptr(slot_raw, ptr_local.clone(), false);
                    }
                    _ => {
                        return Err(WasmError::Unsupported(format!(
                            "by-reference parameter ${} of {} has no pointer parameter local on wasm32-wasi",
                            p.name, function.name
                        )));
                    }
                }
            }
        }
    }

    let (segments, block_entry) = plan_segments(function);
    ctx.segments = segments;
    ctx.block_entry = block_entry;

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
    if let Some((frame, frame_local)) = ctx.generator.clone() {
        // A resume body reloads EVERY local before it runs a line: the spill at the yield
        // wrote every one, and reloading the same set is what makes the two halves agree
        // without a liveness analysis. Then the state says which block to re-enter.
        super::generators::emit_reload(&mut ctx, &frame, &frame_local);
        ctx.fb.ins(
            &format!(
                "(i32.load offset={} (local.get {frame_local}))",
                crate::codegen_support::runtime::generators::frame::OFF_STATE_IDX
            ),
            "the block this generator suspended at",
        );
        ctx.fb.ins(
            &format!("local.set {}", ctx.state_local),
            "re-enter the dispatch loop there",
        );
    } else {
        let entry_index = ctx.block_index(function.entry);
        ctx.fb.ins(
            &format!("i32.const {}", entry_index),
            "initial dispatch state = the entry block's first segment",
        );
        ctx.fb.ins(
            &format!("local.set {}", ctx.state_local),
            "enter the dispatch loop at the entry block",
        );
    }

    if is_main {
        emit_container_static_defaults(&mut ctx)?;
        emit_main_argc_argv_init(&mut ctx)?;
    }

    emit_dispatch_loop(&mut ctx)?;

    Ok(ctx.fb)
}

/// Builds the container-typed static properties' defaults, before any PHP code runs.
///
/// A scalar or string default is BYTES, and the slot carries them from static data with nothing
/// to run. An array is not bytes: it needs a heap block, so its slot starts as a null pointer and
/// this fills it in `main`'s prologue — the earliest point at which any PHP code could observe
/// it, which is what makes the two indistinguishable from the outside.
///
/// The elements of a non-empty default (`public static $values = [3, 5];`) go in here too, in
/// source order, through the same append the compiled `$a[] = ...` uses.
fn emit_container_static_defaults(ctx: &mut FnCtx) -> Result<()> {
    for (address, php_type, literal) in
        super::statics::container_static_defaults(ctx.module, ctx.static_slots)
    {
        let container = ctx.fresh_temp(ValType::I32);
        match (&php_type, &literal) {
            // An indexed default, empty or not. The element list decides the slot width the
            // array is allocated at, the same choice `array_new` makes at a literal.
            (PhpType::Array(element_type), _) => {
                let element_type = element_type.codegen_repr();
                let elements: &[crate::codegen::LiteralArrayElement] = match &literal {
                    Some(crate::codegen::LiteralDefaultValue::Array { elements, .. }) => elements,
                    _ => &[],
                };
                let boxed = element_type == PhpType::Mixed;
                let stride = if boxed || element_type == PhpType::Str { 16 } else { 8 };
                ctx.fb.ins(
                    &format!(
                        "(local.set {container} (call $__rt_array_new (i64.const {}) (i64.const {stride})))",
                        elements.len()
                    ),
                    "the static array default's storage",
                );
                for element in elements {
                    emit_static_default_element(ctx, &container, element, boxed)?;
                }
            }
            // An associative default. Only the empty one is rendered here: a keyed literal has
            // its own key normalization, and starting it wrong is worse than refusing it.
            (PhpType::AssocArray { value, .. }, _) => {
                let value_tag = crate::codegen::runtime_value_tag(&value.codegen_repr());
                ctx.fb.ins(
                    &format!(
                        "(local.set {container} (call $__rt_hash_new (i64.const 4) (i64.const {value_tag})))"
                    ),
                    "the static hash default's storage",
                );
                if matches!(
                    literal,
                    Some(crate::codegen::LiteralDefaultValue::AssocArray { .. })
                ) {
                    return Err(WasmError::Unsupported(
                        "a non-empty associative static default on wasm32-wasi".to_string(),
                    ));
                }
            }
            // A declared `array`: the list or empty hash is built as cells and boxed, and the
            // slot holds the cell. `__rt_mixed_from_value` takes its own reference, so the
            // construction's is dropped once the cell holds one.
            (PhpType::Mixed, Some(crate::codegen::LiteralDefaultValue::BoxedArray { elements, .. })) => {
                ctx.fb.ins(
                    &format!(
                        "(local.set {container} (call $__rt_array_new (i64.const {}) (i64.const 16)))",
                        elements.len()
                    ),
                    "the static array default's list",
                );
                for element in elements {
                    emit_static_default_element(ctx, &container, element, true)?;
                }
                emit_box_static_container(ctx, &container, 4);
            }
            (PhpType::Mixed, Some(crate::codegen::LiteralDefaultValue::BoxedAssocArray { entries, .. }))
                if entries.is_empty() =>
            {
                ctx.fb.ins(
                    &format!("(local.set {container} (call $__rt_hash_new (i64.const 4) (i64.const 7)))"),
                    "the static array default's hash",
                );
                emit_box_static_container(ctx, &container, 5);
            }
            (PhpType::Mixed, _) => {
                return Err(WasmError::Unsupported(
                    "a boxed static array default with keys on wasm32-wasi".to_string(),
                ))
            }
            _ => continue,
        }
        ctx.fb.ins(
            &format!(
                "(i64.store (i32.const {address}) (i64.extend_i32_u (local.get {container})))"
            ),
            "publish the built default into its slot",
        );
    }
    Ok(())
}

/// Replaces the container in `container` by a fresh cell holding it under `tag`.
fn emit_box_static_container(ctx: &mut FnCtx, container: &str, tag: i64) {
    let cell = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(
        &format!(
            "(local.set {cell} (call $__rt_mixed_from_value (i64.const {tag}) (i64.extend_i32_u (local.get {container})) (i64.const 0)))"
        ),
        "box the default (the cell takes its own reference)",
    );
    ctx.fb.ins(
        &format!("(call $__rt_decref_any (local.get {container}))"),
        "the cell holds the container now",
    );
    ctx.fb.ins(&format!("(local.set {container} (local.get {cell}))"), "the slot holds the cell");
}

/// Appends one literal element to a static array default under construction.
///
/// A `mixed`-element array holds CELLS, so the element is boxed and pushed by
/// `__rt_array_push_mixed` — which records the pointer without retaining it, and the fresh cell's
/// own reference is exactly the one the array should end up holding.
fn emit_static_default_element(
    ctx: &mut FnCtx,
    container: &str,
    element: &crate::codegen::LiteralArrayElement,
    boxed: bool,
) -> Result<()> {
    use crate::codegen::LiteralArrayElement as Element;
    if boxed {
        let (tag, lo) = match element {
            Element::Int(value) => (0, format!("(i64.const {value})")),
            Element::Bool(value) => (3, format!("(i64.const {})", i64::from(*value))),
            Element::Float(value) => (2, format!("(i64.const {})", value.to_bits() as i64)),
            Element::Null => (8, "(i64.const 0)".to_string()),
            Element::Str(value) => {
                // `__rt_mixed_from_value` persists its own copy of the literal's bytes.
                let (ptr, len) = ctx.default_str_literal(value)?;
                (1, format!("(i64.const {ptr}) (i64.const {len})"))
            }
            Element::Array { .. } | Element::AssocArray { .. } => {
                return Err(WasmError::Unsupported(
                    "a nested array element in a static array default on wasm32-wasi".to_string(),
                ))
            }
        };
        // A string's `lo` already carries its length as the high word.
        let payload = if tag == 1 { lo } else { format!("{lo} (i64.const 0)") };
        ctx.fb.ins(
            &format!(
                "(local.set {container} (call $__rt_array_push_mixed (local.get {container}) (call $__rt_mixed_from_value (i64.const {tag}) {payload})))"
            ),
            "one boxed element of the static array default",
        );
        return Ok(());
    }
    match element {
        Element::Int(value) => ctx.fb.ins(
            &format!(
                "(local.set {container} (call $__rt_array_push_int (local.get {container}) (i64.const {value})))"
            ),
            "one element of the static array default",
        ),
        Element::Bool(value) => ctx.fb.ins(
            &format!(
                "(local.set {container} (call $__rt_array_push_int (local.get {container}) (i64.const {})))",
                i64::from(*value)
            ),
            "one element of the static array default",
        ),
        Element::Float(value) => ctx.fb.ins(
            &format!(
                "(local.set {container} (call $__rt_array_push_int (local.get {container}) (i64.const {})))",
                value.to_bits() as i64
            ),
            "one element of the static array default (float bits)",
        ),
        _ => {
            return Err(WasmError::Unsupported(
                "a string or null element in a concrete static array default on wasm32-wasi"
                    .to_string(),
            ))
        }
    }
    Ok(())
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

/// A BUILTIN THAT RAISES MUST BE LISTED HERE. This is what makes the planner lay out the
/// catchable message text, and `emit_runtime_failure` only raises a catchable Error when the
/// message it needs is already addressable. A raising builtin left out of this list still
/// reports the right class and message — it just reports them as an UNCAUGHT FATAL, so a
/// `try`/`catch` around it silently stops working. Nothing derives this list from
/// `objects::CATCHABLE_RUNTIME_ERRORS`, because the two are keyed differently (an identity
/// versus a failure code), so `every_catchable_builtin_is_listed_as_raising` pins the pairs
/// instead.
///
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
                | Op::ArrayUnion | Op::HashUnion | Op::ArrayHashUnion | Op::HashArrayUnion
        ) || matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(
                RuntimeCallTarget::Function(
                    RuntimeFnId::Intdiv
                        | RuntimeFnId::StrRepeat
                        | RuntimeFnId::StrPad
                        | RuntimeFnId::Explode
                        | RuntimeFnId::StrSplit
                        | RuntimeFnId::StreamSetChunkSize
                        | RuntimeFnId::Fclose
                        | RuntimeFnId::BcAdd
                        | RuntimeFnId::BcScale
                        | RuntimeFnId::BcMul
                        | RuntimeFnId::BcDivmod
                        | RuntimeFnId::ArrayPad
                        | RuntimeFnId::ArrayMerge
                        | RuntimeFnId::ArrayReplace
                ) | RuntimeCallTarget::ProfiledFunction {
                    target:
                        RuntimeFnId::Intdiv
                        | RuntimeFnId::StrRepeat
                        | RuntimeFnId::StrPad
                        | RuntimeFnId::Explode
                        | RuntimeFnId::StrSplit
                        | RuntimeFnId::StreamSetChunkSize
                        | RuntimeFnId::Fclose
                        | RuntimeFnId::BcAdd
                        | RuntimeFnId::BcScale
                        | RuntimeFnId::BcMul
                        | RuntimeFnId::BcDivmod
                        | RuntimeFnId::ArrayPad
                        | RuntimeFnId::ArrayMerge
                        | RuntimeFnId::ArrayReplace,
                    ..
                }
            ))
        )
    })
}

/// The failure codes php's `TypeError`s for a null array operand report under.
///
/// `+` names both operand types, so it has one code per side; `array_merge`/`array_replace`
/// name the argument position. `NULL_BOTH` is the odd one out: php evaluates `null + null` to
/// the integer 0, which a result statically typed as an array cannot hold, so that shape stops
/// with a named fatal rather than raising an error php would not.
pub(super) const UNION_NULL_LEFT_FAILURE_CODE: i32 = 33;
/// See [`UNION_NULL_LEFT_FAILURE_CODE`].
pub(super) const UNION_NULL_RIGHT_FAILURE_CODE: i32 = 34;
/// See [`UNION_NULL_LEFT_FAILURE_CODE`].
pub(super) const MERGE_NULL_FIRST_FAILURE_CODE: i32 = 35;
/// See [`UNION_NULL_LEFT_FAILURE_CODE`].
pub(super) const MERGE_NULL_SECOND_FAILURE_CODE: i32 = 36;
/// See [`UNION_NULL_LEFT_FAILURE_CODE`].
pub(super) const REPLACE_NULL_FIRST_FAILURE_CODE: i32 = 37;
/// See [`UNION_NULL_LEFT_FAILURE_CODE`].
pub(super) const REPLACE_NULL_SECOND_FAILURE_CODE: i32 = 38;
/// See [`UNION_NULL_LEFT_FAILURE_CODE`].
pub(super) const UNION_NULL_BOTH_FAILURE_CODE: i32 = 39;

/// The failure code for php's refusal to rewind a generator that has already advanced.
///
/// `Generator::rewind()` is the only `Generator` method that raises, and it is a METHOD, so
/// `raises_runtime_error` has no `RuntimeFnId` to match — `module_rewinds_a_generator` is what
/// makes the planner lay these bytes out, and without that the raise degrades silently to an
/// uncaught fatal that no `catch` intercepts.
pub(super) const REWIND_ADVANCED_FAILURE_CODE: i32 = 20;

/// The failure code for `getReturn()` on a generator that has not returned.
pub(super) const GENERATOR_NO_RETURN_FAILURE_CODE: i32 = 25;

/// The failure code for a `foreach` over a generator that has already finished.
///
/// php raises `Cannot traverse an already closed generator` from the iterator constructor, so
/// it is the loop that raises, not a method call.
pub(super) const GENERATOR_CLOSED_FAILURE_CODE: i32 = 27;

/// Every function body in the module: plain functions, class methods AND closures.
///
/// The one iterator every module-wide feature scan goes through. A scan that chains only the
/// first two skips closures, so a feature used solely inside a closure (`ob_start`, `define`,
/// `json_encode`, ...) never lays out its runtime and the module fails to assemble.
pub(super) fn all_functions(module: &Module) -> impl Iterator<Item = &Function> {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
}

/// Returns whether ANY function, class method or closure in the module can raise a runtime Error.
pub(super) fn module_raises_runtime_errors(module: &Module) -> bool {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .any(raises_runtime_error)
        // `Generator::rewind()` raises php's `Exception` for a generator that has already been
        // advanced, and it is a METHOD rather than a builtin call, so there is no `RuntimeFnId`
        // for `raises_runtime_error` to match on.
        || module_rewinds_a_generator(module)
        // And `getReturn()` raises one when asked too early.
        || module_gets_a_generators_return(module)
        // A `foreach` over a generator is a rewind: it raises for a finished or advanced one.
        || module_traverses_a_generator(module)
}

/// Whether any function in the module runs a `foreach` over a value typed as a `Generator`.
///
/// Starting such a loop is php's rewind, which raises `Cannot traverse an already closed
/// generator` and `Cannot rewind a generator that was already run`, so the planner has to lay
/// those messages out for a module whose only generator contact is the loop.
pub(super) fn module_traverses_a_generator(module: &Module) -> bool {
    all_functions(module).any(|function| {
        function.instructions.iter().any(|inst| {
            inst.op == Op::IterStart
                && inst
                    .operands
                    .first()
                    .and_then(|id| function.value(*id))
                    .map(|value| value.php_type.codegen_repr())
                    .is_some_and(|php_type| {
                        matches!(php_type, PhpType::Object(class) if class.trim_start_matches('\\') == "Generator")
                    })
        })
    })
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
        // `$gen->throw($e)` is a raise with no `throw` instruction to find: the resume body
        // open-codes it at the point the yield came back to. Without this the module declares
        // neither the exception tag nor the fatal-code global.
        || module_throws_into_a_generator(module)
        // A SYNTHESIZED raise counts too, and it carries no `throw` instruction to find: the
        // emitter open-codes `Enum::from()`'s `ValueError` at its no-match arm. Without this the
        // module declares neither the exception tag nor the fatal-code global, and the WAT fails
        // to assemble with "unknown global" — for a program that never writes `try` or `throw`.
        //
        // The question needs the MODULE rather than one function, because what identifies the
        // call is that its receiver is an enum, and `plan` already reads exactly that to lay the
        // message out. One reader, so the tag and the message cannot disagree about which
        // modules need them.
        || !super::plan::enum_names_reached_by_from(module).is_empty()
        // And a read of a slot some `unset` can have cleared, which raises php's `Error` the same
        // synthesized way. Same reader as the message layout, for the same reason.
        || !super::plan::properties_reached_by_unset(module).is_empty()
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

/// Splits every block into the dispatch SEGMENTS the loop emits one arm each for.
///
/// One segment per block is the answer for every function that does not yield, and that
/// is what keeps this change invisible to them: `block_entry[i] == i`, the `br_table` has
/// the same arms it always had, and the emitted WAT is byte-identical.
///
/// A `yield` is why segments exist at all. It RETURNS — this target cannot switch stacks
/// — so the resume has to re-enter the body just past it, and every yield this backend
/// sees sits MID-block with code after it. An EIR block index cannot name that point, so
/// the block splits: the instructions up to and including the yield are one segment, the
/// rest of the block is the next. The yield stores its successor's index as the state.
///
/// Returns the segments in emission order, and the segment each block starts at. Blocks
/// keep segments `0..blocks.len()` in order, so a branch target's index is unchanged for
/// every block that does not follow a split one.
#[cfg(test)]
pub(super) fn plan_segments_for_test(
    function: &crate::ir::Function,
) -> (Vec<(usize, Vec<InstId>)>, Vec<u32>) {
    plan_segments(function)
}

/// Raises what `throw()` parked, at the top of the segment the yield resumes into.
///
/// `$gen->throw($e)` has to raise INSIDE the suspended body, at the point it is waiting, so that
/// the body's own `try` sees it — and on this target there is exactly one place that is: the top
/// of the resumed segment, which sits inside the dispatch loop's landing pad. `__rt_gen_throw`
/// parks the exception in `sent_value` (nothing can be sent and thrown at once) and sets
/// `FLAG_PENDING_THROW`; this reads both, clears both, and throws.
///
/// Emitted only for a module that CALLS `Generator::throw`, because the raise names the exception
/// tag and the fatal-code global, and a module with no exceptions declares neither.
fn emit_resumed_pending_throw(ctx: &mut FnCtx, segment: usize) -> Result<()> {
    let Some((_, frame_local)) = ctx.generator.clone() else {
        return Ok(());
    };
    if !module_throws_into_a_generator(ctx.module) {
        return Ok(());
    }
    let Some(previous) = segment.checked_sub(1) else {
        return Ok(());
    };
    let resumed = ctx
        .segments
        .get(previous)
        .and_then(|(_, instructions)| instructions.last().copied())
        .and_then(|inst_id| ctx.function.instruction(inst_id))
        .is_some_and(|inst| matches!(inst.op, Op::GeneratorYield | Op::GeneratorYieldFrom));
    if !resumed {
        return Ok(());
    }
    let flags = crate::codegen_support::runtime::generators::frame::OFF_FLAGS;
    let sent = crate::codegen_support::runtime::generators::frame::OFF_SENT_VALUE;
    let pending = super::generators::FLAG_PENDING_THROW;
    let exception = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(
        &format!(
            "(i32.and (i32.load offset={flags} (local.get {frame_local})) (i32.const {pending}))"
        ),
        "did throw() park an exception for this resume?",
    );
    ctx.fb.ins("if", "raise it where the body is waiting");
    ctx.fb.ins(
        &format!(
            "(i32.store offset={flags} (local.get {frame_local}) (i32.and (i32.load offset={flags} (local.get {frame_local})) (i32.const {})))",
            !pending
        ),
        "one raise per throw()",
    );
    ctx.fb.ins(
        &format!("(i32.wrap_i64 (i64.load offset={sent} (local.get {frame_local})))"),
        "the exception object throw() handed over",
    );
    ctx.fb.ins(&format!("local.set {exception}"), "");
    ctx.fb.ins(
        &format!("(i64.store offset={sent} (local.get {frame_local}) (i64.const 0))"),
        "the frame's reference becomes the raise's",
    );
    ctx.fb.ins(&format!("local.get {exception}"), "the exception in flight");
    ctx.fb.ins(
        &format!("i32.const {UNCAUGHT_EXCEPTION_FAILURE_CODE}"),
        "class-agnostic diagnostic if nothing catches it",
    );
    ctx.fb.ins(
        &format!("global.set ${EXCEPTION_FATAL_CODE_GLOBAL}"),
        "claim the uncaught diagnostic",
    );
    ctx.fb.ins(
        &format!("throw ${EXCEPTION_TAG}"),
        "raise it inside the generator body",
    );
    ctx.fb.ins("end", "end pending-throw check");
    Ok(())
}

/// Whether any function in the module calls `Generator::throw`.
///
/// Two things read this: the resume bodies, which only carry the raise where some caller can
/// reach it, and `module_uses_exceptions`, because that raise needs the tag declared in a program
/// that may contain no `throw` statement of its own.
pub(super) fn module_throws_into_a_generator(module: &Module) -> bool {
    generator_method_is_called(module, "throw")
}

/// Whether any function in the module calls `Generator::rewind`.
///
/// It is the one `Generator` method that RAISES, so the planner has to lay its message out.
pub(super) fn module_rewinds_a_generator(module: &Module) -> bool {
    generator_method_is_called(module, "rewind")
}

/// Whether any function in the module calls `Generator::getReturn`.
///
/// It raises php's `Exception` before the generator has returned, so the planner has to lay the
/// message out.
pub(super) fn module_gets_a_generators_return(module: &Module) -> bool {
    generator_method_is_called(module, "getReturn")
}

/// Whether any function calls `name` on a receiver the EIR types as a `Generator`.
fn generator_method_is_called(module: &Module, name: &str) -> bool {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .any(|function| {
            function.instructions.iter().any(|inst| {
                inst.op == Op::MethodCall
                    && method_name_of(module, inst).is_some_and(|called| called.eq_ignore_ascii_case(name))
                    && inst
                        .operands
                        .first()
                        .and_then(|id| function.value(*id))
                        .map(|value| value.php_type.codegen_repr())
                        .is_some_and(|php_type| {
                            matches!(php_type, PhpType::Object(class) if class.trim_start_matches('\\') == "Generator")
                        })
            })
        })
}

/// The method name a `MethodCall` carries, as the module's string table spells it.
fn method_name_of<'a>(module: &'a Module, inst: &Instruction) -> Option<&'a str> {
    let data = match inst.immediate {
        Some(Immediate::Data(data)) | Some(Immediate::ProfiledData { data, .. }) => data,
        _ => return None,
    };
    module.data.strings.get(data.as_raw() as usize).map(String::as_str)
}

/// Stores what `send()` passed in into the yield's result, at the top of the segment the yield
/// resumes into.
///
/// The `yield` expression's value is decided by the RESUME, not by the suspension — `$got = yield`
/// answers whatever the next `send($v)` carried, and null for a plain `next()` or a `foreach`.
/// So the read sits here, at the point control comes back, and `__rt_gen_take_sent` clears the
/// word as it reads it so the resume after an unsent one sees null.
fn emit_resumed_sent_value(ctx: &mut FnCtx, segment: usize) -> Result<()> {
    let Some((frame, frame_local)) = ctx.generator.clone() else {
        return Ok(());
    };
    let _ = frame;
    let Some(previous) = segment.checked_sub(1) else {
        return Ok(());
    };
    let Some((_, instructions)) = ctx.segments.get(previous).cloned() else {
        return Ok(());
    };
    let Some(last) = instructions
        .last()
        .and_then(|inst_id| ctx.function.instruction(*inst_id))
        .cloned()
    else {
        return Ok(());
    };
    if last.op != Op::GeneratorYield {
        // A delegation is the other suspension, and what IT evaluates to is the inner's return
        // value — already stored by the step that discovered the inner had finished.
        return Ok(());
    }
    let Some(result) = last.result else {
        return Ok(());
    };
    // A DEAD result still has to take the value out. `yield from <array>` desugars into a loop
    // of delegated yields whose result nothing reads and nothing releases — the native suspend
    // helper owns what it is handed — so storing the cell into a local no one looks at leaked one
    // 24-byte cell per forwarded element: 3 pages became 26 over 20000 delegations. Taking it and
    // releasing it keeps the two things the word is for, which are that a resume nobody sent to
    // sees null and that the next one does not see this one's value.
    //
    // "Nothing reads it" has to count TERMINATORS too: `return yield 1;` hands the yield's result
    // straight to the `return`, and treating that as dead dropped the value `send()` parked, so
    // `getReturn()` answered null.
    let dead = !ctx
        .function
        .instructions
        .iter()
        .any(|candidate| candidate.operands.contains(&result))
        && !ctx
            .function
            .blocks
            .iter()
            .any(|block| block.terminator.as_ref().is_some_and(|t| terminator_uses_value(t, result)));
    if dead {
        ctx.fb.ins(
            &format!(
                "(call $__rt_decref_any (call $__rt_gen_take_sent (local.get {frame_local})))"
            ),
            "clear what send() parked; this yield's value is never read",
        );
        return Ok(());
    }
    ctx.fb.ins(
        &format!("(call $__rt_gen_take_sent (local.get {frame_local}))"),
        "what send() passed in, or null",
    );
    ctx.emit_store_value(result)
}

/// Whether a block terminator reads `value` (as a condition, a branch argument or a result).
fn terminator_uses_value(terminator: &Terminator, value: ValueId) -> bool {
    match terminator {
        Terminator::Br { args, .. } => args.contains(&value),
        Terminator::CondBr { cond, then_args, else_args, .. } => {
            *cond == value || then_args.contains(&value) || else_args.contains(&value)
        }
        Terminator::Switch { scrutinee, cases, default_args, .. } => {
            *scrutinee == value
                || default_args.contains(&value)
                || cases.iter().any(|case| case.args.contains(&value))
        }
        Terminator::Return { value: returned } => *returned == Some(value),
        Terminator::Throw { value: thrown } => *thrown == value,
        Terminator::GeneratorSuspend { key, value: yielded, resume_args, .. } => {
            *key == Some(value) || *yielded == Some(value) || resume_args.contains(&value)
        }
        Terminator::Fatal { .. } | Terminator::Unreachable => false,
    }
}

/// Splits a function into dispatch segments, answering the segments and each block's first one.
///
/// A segment is a run of instructions with no suspension point inside it: a `yield` or a
/// `yield from` delegation ends one (a delegation also starts the next), so a generator can
/// resume at any boundary. Each segment carries the index of its block.
fn plan_segments(function: &crate::ir::Function) -> (Vec<(usize, Vec<InstId>)>, Vec<u32>) {
    let mut segments: Vec<(usize, Vec<InstId>)> = Vec::new();
    let mut block_entry: Vec<u32> = Vec::with_capacity(function.blocks.len());
    for (index, block) in function.blocks.iter().enumerate() {
        block_entry.push(segments.len() as u32);
        let mut current: Vec<InstId> = Vec::new();
        for inst_id in &block.instructions {
            let op = function.instruction(*inst_id).map(|inst| inst.op);
            // A DELEGATION ends a segment the way a yield does, and also starts one: its resume
            // point is ITSELF, so control comes back to it while the inner generator still has
            // values — and re-entering a segment that also held the `inner()` CALL would build a
            // fresh inner generator on every resume. Alone in its own segment, the only thing it
            // re-reads is the frame.
            if op == Some(Op::GeneratorYieldFrom) && !current.is_empty() {
                segments.push((index, std::mem::take(&mut current)));
            }
            current.push(*inst_id);
            if matches!(op, Some(Op::GeneratorYield) | Some(Op::GeneratorYieldFrom)) {
                segments.push((index, std::mem::take(&mut current)));
            }
        }
        // The tail carries the block's terminator, so it exists even when empty.
        segments.push((index, current));
    }
    (segments, block_entry)
}

/// Emits the state-machine body of a function: the `$state` dispatch loop over its segments.
///
/// `main`, every function with a `catch`, and every generator body is wrapped in a landing pad,
/// so a raised exception resumes at its handler or ends the generator rather than escaping the
/// module.
fn emit_dispatch_loop(ctx: &mut FnCtx) -> Result<()> {
    let n = ctx.segments.len();
    // `main` guards the whole program even when it contains no `catch` of its own: an exception
    // that unwinds past it is PHP's uncaught fatal, and without a landing pad here the WASM
    // exception would escape the module and surface as a HOST-level crash instead.
    // A GENERATOR body is guarded even when it catches nothing of its own, because an exception
    // that leaves it also FINISHES it: php answers false from `valid()` afterwards, and the frame
    // records that in a flag no unwinding path would otherwise reach. The landing pad below is
    // the only place that runs between the raise and the caller.
    let catches = catches_exceptions(ctx.function)
        || (ctx.function.flags.is_main && module_uses_exceptions(ctx.module))
        || (ctx.function.flags.is_generator && module_uses_exceptions(ctx.module));
    // A RESUME must not disarm what it just reloaded. The frame carries the armed handler across
    // the suspension, and the wrapper seeded it with the same -1 this would write — so running it
    // on a resume threw away the `try` the body is still inside, and a `throw` on the far side of
    // a yield escaped to the consumer as an uncaught exception.
    if catches && ctx.generator.is_none() {
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
    // The wrappers are opened in reverse and closed one per body below, so a `br $__bK`
    // lands exactly where body K begins. Segment order is therefore emission order AND
    // state order, which is what lets the `br_table` index be the state.


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
        let (block, inst_ids) = ctx.segments[k].clone();
        ctx.fb
            .comment(&format!("---- segment {} (block {}) ----", k, block));
        let last_op = inst_ids
            .last()
            .and_then(|inst_id| ctx.function.instruction(*inst_id))
            .map(|instruction| instruction.op);
        ctx.segment = k;
        // A segment reached by RESUMING opens with the value `send()` passed in, which is what
        // the suspended `yield` evaluates to. It cannot be read at the yield: nothing has been
        // sent yet at that point.
        emit_resumed_pending_throw(ctx, k)?;
        emit_resumed_sent_value(ctx, k)?;
        for inst_id in inst_ids {
            lower_instruction(ctx, inst_id)?;
        }
        // Only the segment that reaches the END of its block carries the terminator. A
        // segment that stopped at a yield has already returned, so anything after it in
        // this arm would be unreachable — and the next arm continues the block.
        let ends_block = ctx
            .segments
            .get(k + 1)
            .map(|(next, _)| *next != block)
            .unwrap_or(true);
        if ends_block {
            let terminator = ctx.function.blocks[block]
                .terminator
                .clone()
                .ok_or_else(|| {
                    WasmError::Unsupported(format!("block {} has no terminator", block))
                })?;
            lower_terminator(ctx, &terminator, last_op)?;
        }

        // Close the wrapper for segment k+1; the last body sits inside $__default.
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
            // An exception leaving a generator body finishes it. Nothing else can record that:
            // the Return terminator is what normally sets the flag, and unwinding skips it — so
            // `$g->valid()` answered true after an uncaught `throw()` where php answers false.
            if let Some((_, frame_local)) = ctx.generator.clone() {
                // The locals this body owns die with it, exactly as at a `return`: a terminated
                // frame's release skips the spilled copy (it may be stale — the body releases
                // and clears locals as it goes), so nothing else would give them back. The slot
                // locals hold their CURRENT values here and each release clears its local first,
                // so the same pass is idempotent.
                ctx.emit_ref_cell_owner_epilogue()?;
                ctx.emit_reassigned_capture_epilogue(None)?;
                ctx.emit_local_epilogue_cleanup(None)?;
                let flags = crate::codegen_support::runtime::generators::frame::OFF_FLAGS;
                let terminated = crate::codegen_support::runtime::generators::frame::FLAG_TERMINATED;
                let last_value = crate::codegen_support::runtime::generators::frame::OFF_LAST_VALUE;
                ctx.fb.ins(
                    &format!(
                        "(i32.store offset={flags} (local.get {frame_local}) (i32.or (i32.load offset={flags} (local.get {frame_local})) (i32.const {terminated})))"
                    ),
                    "an exception out of the body finishes the generator",
                );
                ctx.fb.ins(
                    &format!(
                        "(call $__rt_decref_any (i32.wrap_i64 (i64.load offset={last_value} (local.get {frame_local}))))"
                    ),
                    "and a finished generator has no current value",
                );
                ctx.fb.ins(
                    &format!("(i64.store offset={last_value} (local.get {frame_local}) (i64.const 0))"),
                    "",
                );
            }
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
                if super::output_buffer::module_uses_output_buffering(ctx.module) {
                    ctx.fb.ins(
                        "call $__rt_ob_flush_all",
                        "php flushes output still buffered when the script ends",
                    );
                }
                ctx.fb.ins("i32.const 0", "exit status 0");
                ctx.fb.ins("call $wasi_proc_exit", "WASI proc_exit(0)");
                ctx.fb.ins(
                    "unreachable",
                    "elephc-trap:post-noreturn:main-proc-exit WASI proc_exit is non-returning",
                );
            } else if let Some((frame, frame_local)) = ctx.generator.clone() {
                // A generator's `return` ENDS it: the value becomes what `getReturn()` reads
                // rather than what the call answers, the frame is marked terminated so a
                // later resume answers "no more", and the resume itself reports `(0, _)`.
                if let Some(v) = value {
                    ctx.emit_load_value(*v)?;
                    let parked = ctx.fresh_temp(super::wat::ValType::I32);
                    ctx.fb.ins(&format!("local.set {parked}"), "the generator's return value");
                    ctx.fb.ins(
                        &format!(
                            "(i64.store offset={} (local.get {frame_local}) (i64.extend_i32_u (local.get {parked})))",
                            crate::codegen_support::runtime::generators::frame::OFF_RETURN_VALUE
                        ),
                        "getReturn() reads this",
                    );
                }
                let _ = frame;
                // OR the bit in rather than store it: overwriting the word cleared FLAG_REWOUND,
                // so the next `current()`/`valid()` thought the generator had never started,
                // primed it again, and re-entered the stale `state_idx` — printing the tail of
                // the body a second time and answering the last value instead of null.
                ctx.fb.ins(
                    &format!(
                        "(i32.store offset={flags} (local.get {frame_local}) (i32.or (i32.load offset={flags} (local.get {frame_local})) (i32.const {terminated})))",
                        flags = crate::codegen_support::runtime::generators::frame::OFF_FLAGS,
                        // RETURNED too: `getReturn()` is only legal once the body ran to a `return`
                        // (or its end), which is not the same as being terminated by an exception.
                        terminated = crate::codegen_support::runtime::generators::frame::FLAG_TERMINATED
                            | super::generators::FLAG_RETURNED as u32
                    ),
                    "a finished generator never resumes again",
                );
                // And it has no current value: php answers null from `current()` once a generator
                // has run out, where the frame would still be holding the last thing it yielded.
                ctx.fb.ins(
                    &format!(
                        "(call $__rt_decref_any (i32.wrap_i64 (i64.load offset={last} (local.get {frame_local}))))",
                        last = crate::codegen_support::runtime::generators::frame::OFF_LAST_VALUE
                    ),
                    "give back the last yielded value",
                );
                ctx.fb.ins(
                    &format!(
                        "(i64.store offset={last} (local.get {frame_local}) (i64.const 0))",
                        last = crate::codegen_support::runtime::generators::frame::OFF_LAST_VALUE
                    ),
                    "a finished generator has no current value",
                );
                // Nor a current key: php answers null from `key()` once the generator is exhausted.
                ctx.fb.ins(
                    &format!(
                        "(call $__rt_decref_any (i32.wrap_i64 (i64.load offset={key} (local.get {frame_local}))))",
                        key = crate::codegen_support::runtime::generators::frame::OFF_LAST_KEY
                    ),
                    "give back the last yielded key",
                );
                ctx.fb.ins(
                    &format!(
                        "(i64.store offset={key} (local.get {frame_local}) (i64.const 0))",
                        key = crate::codegen_support::runtime::generators::frame::OFF_LAST_KEY
                    ),
                    "a finished generator has no current key",
                );
                ctx.fb.ins("i32.const 0", "no value: the body ran out");
                ctx.fb.ins("i32.const 0", "and nothing was yielded");
                ctx.fb.ins("return", "leave the resume body");
            } else {
                if let Some(v) = value {
                    ctx.emit_load_value(*v)?;
                    // A PARAMETER slot is a BORROW: the caller owns the argument and releases
                    // it after the call, while it treats the call's result as OWNED. Returning
                    // such a slot's heap value therefore has to hand over a reference of its
                    // own, or the caller's release of the argument frees what it just received
                    // — `f($n - 1) + f($n - 2)` reads a freed cell and reports an operand-type
                    // fatal. Ordinary locals need nothing: the epilogue above already skips the
                    // slot this return moves out.
                    //
                    // A STATIC's cell is the same situation with different storage: it
                    // outlives the call and keeps its own reference, so `static $a = [];
                    // return $a;` handed the caller the cell's ONLY reference and the caller's
                    // release freed what the next call would read. A static PROPERTY's slot
                    // says the same thing about the same storage.
                    if (returned_from_parameter_slot(ctx.function, returned_slot)
                        || returns_static_slot_storage(ctx.function, *v))
                        && matches!(ctx.function.value(*v).map(|value| value.ir_type),
                            Some(IrType::Heap(_)))
                    {
                        ctx.fb.ins("call $__rt_incref_borrowed_return", "the caller owns the result");
                    }
                    // A STRING read from static storage is the slot's own block, not a copy:
                    // handing it over as the result let the caller's release free the slot's
                    // string (`Box::$last = $a . "!"; return Box::$last;` left the property
                    // reading as blanks). The caller owns what it is given, so give it a copy.
                    if returns_static_slot_storage(ctx.function, *v)
                        && matches!(ctx.function.value(*v).map(|value| value.ir_type),
                            Some(IrType::Str))
                    {
                        ctx.fb.ins("call $__rt_str_persist", "the caller owns a private copy of the slot's string");
                    }
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
/// Whether the slot a return moves out of is a PARAMETER's.
///
/// Parameter slots are excluded from the epilogue cleanup because the caller owns the argument;
/// that same fact is what makes returning one a borrow the caller cannot be handed as owned.
fn returned_from_parameter_slot(function: &Function, slot: Option<LocalSlotId>) -> bool {
    slot.is_some_and(|slot| (slot.as_raw() as usize) < function.params.len())
}

/// Whether this returned value is one a STATIC slot still holds.
///
/// A function's `static $x` and a class's `static $p` are the same storage with different
/// keys, and both outlive the call while keeping their own reference — so a return of either
/// hands the caller something it must be given a reference to.
///
/// Only the moves that forward a pointer unchanged are followed. A conversion such as
/// `ArrayToMixed` ALLOCATES, so its result is already owned and increfing it would leak —
/// which is why this walk is shorter than `returned_local_slot`'s.
fn returns_static_slot_storage(function: &Function, value: ValueId) -> bool {
    let Some(value) = function.value(value) else {
        return false;
    };
    let ValueDef::Instruction { inst, .. } = value.def else {
        return false;
    };
    let Some(inst) = function.instruction(inst) else {
        return false;
    };
    match inst.op {
        Op::LoadStaticLocal | Op::LoadStaticProperty => true,
        Op::Move | Op::Borrow => inst
            .operands
            .first()
            .is_some_and(|source| returns_static_slot_storage(function, *source)),
        _ => false,
    }
}

/// Returns the local slot whose `LoadLocal` directly provides a returned value, following the
/// ownership-neutral forwarding ops, or `None` when the value is computed.
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

    /// Every builtin with a catchable runtime Error is listed as one that raises.
    ///
    /// The two facts live in different modules and are keyed differently — `raises_runtime_error`
    /// matches a `RuntimeFnId`, `objects::CATCHABLE_RUNTIME_ERRORS` a failure code — and nothing
    /// derives one from the other. When they disagree the compile still succeeds and the class
    /// and message are still right; the Error simply stops being CATCHABLE, because the planner
    /// never laid its message out and `emit_runtime_failure` falls back to the uncaught fatal.
    /// That is how `stream_set_chunk_size()`'s `ValueError` shipped uncatchable in a first cut of
    /// this file: `try { stream_set_chunk_size($h, 0); } catch (Throwable) {}` killed the program
    /// instead of running the handler.
    ///
    /// So the pairs are written out here. A builtin added to `CATCHABLE_RUNTIME_ERRORS` without
    /// its identity appearing here fails this test, which is the only place the connection can
    /// be checked.
    #[test]
    fn every_catchable_builtin_is_listed_as_raising() {
        // (the builtin, the php function name its catchable message names)
        let pairs = [
            (RuntimeFnId::Intdiv, "intdiv("),
            (RuntimeFnId::StrRepeat, "str_repeat("),
            (RuntimeFnId::StrPad, "str_pad("),
            (RuntimeFnId::Explode, "explode("),
            (RuntimeFnId::StrSplit, "str_split("),
            (RuntimeFnId::StreamSetChunkSize, "stream_set_chunk_size("),
            (RuntimeFnId::Fclose, "fclose("),
            (RuntimeFnId::BcAdd, "bcmath"),
            (RuntimeFnId::BcMul, "bcmath"),
            (RuntimeFnId::BcDivmod, "bcmath"),
            (RuntimeFnId::BcScale, "bcscale("),
            (RuntimeFnId::ArrayPad, "array_pad("),
            (RuntimeFnId::ArrayMerge, "array_merge("),
            (RuntimeFnId::ArrayReplace, "array_replace("),
        ];

        for (target, _) in pairs {
            let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
            {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                builder.emit(
                    Op::RuntimeCall,
                    Vec::new(),
                    Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))),
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                );
            }
            assert!(
                raises_runtime_error(&function),
                "{target:?} has a catchable runtime Error but is not listed as raising, so its \
                 message is never laid out and the raise degrades to an uncaught fatal"
            );
        }

        // And the other direction: every catchable message belongs to one of those builtins, so
        // a code added to the table without a listed identity is caught here rather than at
        // runtime. `intdiv`'s messages are the arithmetic ones, which the OPCODE arm covers.
        // A raise that is not a BUILTIN call has no `RuntimeFnId` to list, so it is named here
        // instead — with the module predicate that lays its message out, which is the thing the
        // builtin arm above exists to prove.
        let methods = [
            (
                "Cannot rewind a generator that was already run",
                "function::module_rewinds_a_generator",
            ),
            (
                "Cannot get return value of a generator that hasn't returned",
                "function::module_gets_a_generators_return",
            ),
            (
                "Cannot traverse an already closed generator",
                "function::module_traverses_a_generator",
            ),
            // The `+` operator's four array opcodes, matched by `raises_runtime_error`'s opcode arm.
            (
                "Unsupported operand types: null + array",
                "function::raises_runtime_error (array union opcodes)",
            ),
            (
                "Unsupported operand types: array + null",
                "function::raises_runtime_error (array union opcodes)",
            ),
        ];
        for (code, _, message) in crate::codegen_wasm::objects::CATCHABLE_RUNTIME_ERRORS {
            if code < 11 {
                continue; // codes 1..10 are arithmetic and unwinding, not builtin calls
            }
            assert!(
                pairs.iter().any(|(_, prefix)| message.starts_with(prefix))
                    || methods.iter().any(|(text, _)| *text == message),
                "catchable message {message:?} names no builtin listed as raising"
            );
        }

        // And the method arm's own direction: the predicate that lays the message out has to
        // answer true for a program that calls it, or the raise degrades to an uncaught fatal.
        let mut module = Module::new(crate::codegen::platform::Target::wasm());
        let mut caller = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = crate::ir::Builder::new(&mut caller);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let receiver = builder
                .emit(
                    Op::ConstNull,
                    Vec::new(),
                    None,
                    IrType::Heap(IrHeapKind::Object),
                    PhpType::Object("Generator".to_string()),
                    Ownership::Owned,
                )
                .expect("a value-producing op answers its result");
            builder.emit(
                Op::MethodCall,
                vec![receiver],
                Some(Immediate::Data(crate::ir::DataId::from_raw(0))),
                IrType::I64,
                PhpType::Void,
                Ownership::NonHeap,
            );
        }
        module.data.strings.push("rewind".to_string());
        module.functions.push(caller);
        assert!(
            module_rewinds_a_generator(&module),
            "a `Generator::rewind()` call is what lays php's rewind Exception out; without it the \
             raise is not catchable"
        );
    }

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
            &Default::default(),
            &[],
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
