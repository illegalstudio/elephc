//! Purpose:
//! Generator support for `wasm32-wasi`: the heap frame that carries a suspended
//! generator's state, the dense generator id that selects its resume function, and
//! the WAT the wrapper and the resume dispatcher are emitted from.
//!
//! Called from:
//!  - `crate::codegen_wasm::plan` (emits the dispatcher and one wrapper per generator).
//!  - `crate::codegen_wasm::function` (emits the resume function's prologue and reads
//!    the frame layout for spill/reload).
//!  - `crate::codegen_wasm::inst` (`Op::GeneratorYield`).
//!
//! Key details:
//!  - **This target cannot switch stacks.** The native backend runs a generator body
//!    on a fiber and suspends by switching stacks (`codegen/block_emit.rs`,
//!    `codegen_support/runtime/generators/coro.rs`); wasm has no such primitive. What
//!    it has instead is that EVERY function is already emitted as a state machine —
//!    `(local $__state i32)` plus a `br_table` dispatch loop — so suspending is
//!    "write `$__state` and the locals to a heap frame and return", and resuming is
//!    "read them back and re-enter the loop".
//!  - The frame's layout follows `codegen_support::runtime::generators::frame`, whose
//!    80-byte header already reserves `state_idx` and `resume_fn_ptr` for exactly this
//!    design. That file is otherwise unused: the fiber path keeps its own fields in
//!    the fiber's reserved region, so the header is a written-down plan rather than a
//!    live contract, and this is its first implementation.
//!  - `resume_fn_ptr` holds a dense GENERATOR ID, not an address. wasm has no function
//!    pointers, and this backend already resolves every such dispatch with an if-ladder
//!    over an index rather than a `call_indirect` (see `includes.rs`) — a closed world
//!    knows every generator at compile time.
//!  - One frame slot per (local, component), 8 bytes each regardless of the
//!    component's width, so the offset table is the same arithmetic everywhere and
//!    nothing needs alignment reasoning.

use std::collections::HashMap;

use crate::codegen_support::runtime::generators::frame as gen_frame;
use crate::ir::{Function, IrHeapKind, IrType, Module};
use crate::types::PhpType;

use super::values::WasmRepr;
use super::wat::ValType;

/// The heap kind a generator frame's header carries.
///
/// Its own kind rather than the object kind, because releasing one is not releasing an
/// object: the values a frame owns live in its 80-byte header and in the local area, and
/// the object walk only knows about declared property slots at `8 + i*16`. Kinds 1..6 are
/// taken (string, array, hash, object, mixed cell, callable descriptor).
pub(super) const HEAP_KIND_GEN_FRAME: i64 = 7;

/// The frame flag `throw()` sets to say "raise this where the body is waiting".
///
/// The shared header (`codegen_support::runtime::generators::frame`) names only `FLAG_REWOUND`
/// and `FLAG_TERMINATED`, because the fiber path needs no more: native's `__rt_gen_throw` can
/// raise ON the suspended stack, which this target has no way to do. Here the exception is parked
/// in `sent_value` — nothing can be sent and thrown at once — and the resume raises it at the top
/// of the segment the yield came back to, which is inside the dispatch loop's landing pad, so the
/// body's own `try` catches it exactly as php's does.
pub(super) const FLAG_PENDING_THROW: i32 = 4;

/// The frame flag that says this generator has been ADVANCED past its first yield.
///
/// Only `rewind()` reads it, and only to refuse: php answers
/// `Cannot rewind a generator that was already run` once a consumer has moved on, while a rewind
/// at the first yield is a no-op. `FLAG_REWOUND` cannot answer that question — priming sets it,
/// and priming is what a rewind of an untouched generator does.
pub(super) const FLAG_ADVANCED: i32 = 8;

/// The frame flag that says the body ran to a `return` (or fell off its end).
///
/// `getReturn()` reads it: php raises `Cannot get return value of a generator that hasn't
/// returned` for a generator that has not started, is suspended, or died from an exception.
/// `FLAG_TERMINATED` alone cannot say so, because an escaping exception terminates too.
pub(super) const FLAG_RETURNED: i32 = 16;

/// Where one spilled component of one local lives in the frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FrameSlot {
    /// Byte offset from the frame pointer.
    pub(super) offset: u32,
    /// The component's own width, which decides the load/store instruction.
    pub(super) val_type: ValType,
    /// Whether this component is a heap POINTER the frame owns while suspended.
    ///
    /// Only the pointer word of a `Heap(_)` or a `Str` qualifies. A `TaggedScalar`'s payload
    /// can hold an integer that looks like an address, and a tagged slot owns nothing — the
    /// same rule the object release descriptor follows for its tag-8 byte.
    pub(super) owns_heap: bool,
}

/// The frame layout for one generator function.
#[derive(Clone, Debug, Default)]
pub(super) struct GeneratorFrame {
    /// Byte size of the whole frame, 16-byte aligned for `__rt_heap_alloc`.
    pub(super) size: u32,
    /// Slot components, keyed by the local slot's raw index.
    pub(super) slots: HashMap<u32, Vec<FrameSlot>>,
    /// Value components, keyed by the SSA value's raw index.
    pub(super) values: HashMap<u32, Vec<FrameSlot>>,
    /// The armed handler, and one save word per distinct `try` token.
    ///
    /// The same shape of state as an iterator's: `$__handler` and `$__handler_saveN` are locals
    /// `lower_function` makes up, so no EIR table mentions them and a suspension inside a `try`
    /// would come back with nothing armed — or, worse, with whatever the previous resume left.
    /// The saves are ordered by token so the layout is the same on every build.
    pub(super) handlers: Vec<FrameSlot>,
    /// ITERATOR components, keyed by the `IterStart` result's raw value index.
    ///
    /// A `foreach` inside a generator body keeps state that is not a local and not an SSA
    /// value: the source pointer and the cursor live in WAT locals `iter_reserve` makes up, so
    /// `function.locals` and `function.values` do not mention them and the spill walked straight
    /// past. The cursor then reloaded as whatever the previous resume left, which is how
    /// `foreach ([1,2,3] as $v) { yield $v; }` became an endless loop yielding one real value and
    /// then garbage.
    ///
    /// Always three words — source, cursor, runtime kind — whether or not the third is used, so
    /// the layout stays derivable from the function alone: whether a source needs the kind word
    /// is a question about the MODULE, and a layout that asked it could not be computed by the
    /// wrapper.
    pub(super) iters: HashMap<u32, Vec<FrameSlot>>,
}

/// Whether a generator's by-value parameter is a reference the FRAME owns for its whole life.
///
/// A heap value, a string and a callable descriptor all qualify: the caller (or the closure
/// invoker, for a capture) only BORROWS them, and the frame outlives the call, so the wrapper
/// takes a reference of its own that the body's epilogue, or the abandoned-frame release, gives
/// back. A callable travels in an `I64` carrier, which is why it needs naming apart from the
/// `Heap(_)` kinds: the descriptor is a refcounted heap block, and a frame that did not retain it
/// held a dangling word once the creator released the closure.
pub(super) fn param_is_frame_owned(param: &crate::ir::FunctionParam) -> bool {
    !param.by_ref
        && (matches!(param.ir_type, IrType::Heap(_) | IrType::Str)
            || (param.ir_type == IrType::I64 && param.php_type.codegen_repr() == PhpType::Callable))
}

/// Computes the frame layout from the EIR function alone.
///
/// Derived from `function.locals` and `function.values` rather than from a lowering
/// context, because the WRAPPER needs the same offsets before any body is lowered —
/// and two derivations of one layout is how a spill and a reload come to disagree.
///
/// Parameters share storage with slots `0..params.len()`, so they are laid out once,
/// as slots. Every component takes 8 bytes whatever its width.
pub(super) fn frame_layout(function: &Function) -> GeneratorFrame {
    let mut frame = GeneratorFrame {
        size: 0,
        slots: HashMap::new(),
        values: HashMap::new(),
        handlers: Vec::new(),
        iters: HashMap::new(),
    };
    let mut cursor = gen_frame::FIXED_HEADER_BYTES as u32;
    for (index, slot) in function.locals.iter().enumerate() {
        let mut components = place(&mut cursor, slot.ir_type);
        // A callable PARAMETER's descriptor rides in an `I64` carrier that `place` does not
        // treat as a pointer, but the wrapper retained it for the frame (see
        // `param_is_frame_owned`), so a frame dropped while suspended must release it.
        if function.params.get(index).is_some_and(|param| {
            param.ir_type == IrType::I64 && param_is_frame_owned(param)
        }) {
            if let Some(first) = components.first_mut() {
                first.owns_heap = true;
            }
        }
        frame.slots.insert(index as u32, components);
    }
    // A value word is the frame's to release only when the value OWNS its reference and still
    // holds it: a borrowed or maybe-owned `load_local` reads a slot the frame already releases,
    // and an owned value stored into a slot or returned has handed its reference over. Releasing
    // either again freed the caller's argument when a suspended generator was dropped. An owned
    // temporary that is still live at a yield stays the frame's, and one the body has released
    // reads back as zero, which `__rt_decref_any` ignores.
    let mut handed_off: std::collections::HashSet<u32> = function
        .instructions
        .iter()
        .filter(|inst| {
            matches!(
                inst.op,
                crate::ir::Op::StoreLocal
                    | crate::ir::Op::StoreStaticLocal
                    | crate::ir::Op::InitStaticLocal
                    | crate::ir::Op::Move
            )
        })
        .filter_map(|inst| inst.operands.first().map(|value| value.as_raw()))
        .collect();
    handed_off.extend(function.blocks.iter().filter_map(|block| match block.terminator {
        Some(crate::ir::Terminator::Return { value: Some(value) }) => Some(value.as_raw()),
        _ => None,
    }));
    for (index, value) in function.values.iter().enumerate() {
        let mut components = place(&mut cursor, value.ir_type);
        if value.ownership != crate::ir::Ownership::Owned || handed_off.contains(&(index as u32)) {
            for component in &mut components {
                component.owns_heap = false;
            }
        }
        frame.values.insert(index as u32, components);
    }
    // The armed handler first — always, because `$__handler` is declared whether or not the body
    // catches anything, and a word that is never read costs nothing to carry.
    let mut tokens: Vec<i64> = function
        .instructions
        .iter()
        .filter(|inst| inst.op == crate::ir::Op::TryPushHandler)
        .filter_map(|inst| match inst.immediate {
            Some(crate::ir::Immediate::I64(token)) => Some(token),
            _ => None,
        })
        .collect();
    tokens.sort_unstable();
    tokens.dedup();
    for _ in 0..(1 + tokens.len()) {
        let offset = cursor;
        cursor += 8;
        frame.handlers.push(FrameSlot { offset, val_type: ValType::I32, owns_heap: false });
    }
    // Then one three-word block per iterator, in the order `reserve_iterators` walks them —
    // blocks in storage order, instructions in theirs — because both derivations have to name
    // the same offsets and neither can see the other.
    //
    // None of the three is owned. The iterator BORROWS its source: the EIR keeps the container in
    // a value of its own and releases it when the loop is done, and that value is spilled above.
    for inst in function
        .blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .filter_map(|inst_id| function.instruction(*inst_id))
        .filter(|inst| inst.op == crate::ir::Op::IterStart)
    {
        let Some(result) = inst.result else { continue };
        let components = [ValType::I32, ValType::I64, ValType::I32]
            .into_iter()
            .map(|val_type| {
                let offset = cursor;
                cursor += 8;
                FrameSlot { offset, val_type, owns_heap: false }
            })
            .collect();
        frame.iters.insert(result.as_raw(), components);
    }
    let local_area = cursor - gen_frame::FIXED_HEADER_BYTES as u32;
    frame.size = gen_frame::aligned_frame_size(local_area as usize) as u32;
    frame
}

/// Reserves one 8-byte frame word per component and returns their placements.
///
/// The FIRST component of a `Heap(_)` or a `Str` is its pointer, and that is the one the
/// frame owns while suspended; every other component is a width, a length or a tag.
fn place(cursor: &mut u32, ir: crate::ir::IrType) -> Vec<FrameSlot> {
    let owns_first = matches!(
        ir,
        crate::ir::IrType::Heap(_) | crate::ir::IrType::Str
    );
    WasmRepr::val_types(ir)
        .into_iter()
        .enumerate()
        .map(|(index, val_type)| {
            let offset = *cursor;
            *cursor += 8;
            FrameSlot {
                offset,
                val_type,
                owns_heap: owns_first && index == 0,
            }
        })
        .collect()
}

/// The module's generator functions, in the order their ids are assigned.
///
/// Sorted by symbol so the id a wrapper stamps and the arm the dispatcher runs agree
/// across builds — the same determinism rule the rest of the planner follows.
pub(super) fn generator_functions(module: &Module) -> Vec<&Function> {
    let mut generators: Vec<&Function> = module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .filter(|function| function.flags.is_generator)
        .collect();
    generators.sort_by_key(|function| super::symbols::function_symbol(function));
    generators
}

/// The dense id that selects a generator's resume function, or `None` for a
/// non-generator.
pub(super) fn generator_id(module: &Module, function: &Function) -> Option<u32> {
    let symbol = super::symbols::function_symbol(function);
    generator_functions(module)
        .iter()
        .position(|candidate| super::symbols::function_symbol(candidate) == symbol)
        .map(|index| index as u32)
}

/// The WAT name of a generator's resume function.
pub(super) fn resume_symbol(function: &Function) -> String {
    format!("{}__resume", super::symbols::function_symbol(function))
}

/// Reloads every spilled component from the frame into its local.
///
/// Paired with `emit_spill`, and deliberately over the SAME set: a yield writes every
/// component and a resume reads every component back, which is correct without knowing
/// which were live. The wrapper zeroes the area, so a component no path has written
/// reloads as zero rather than as whatever the allocator left.
pub(super) fn emit_reload(ctx: &mut super::context::FnCtx, frame: &GeneratorFrame, frame_local: &str) {
    for (local, slot) in ordered_components(ctx, frame) {
        let load = match slot.val_type {
            ValType::I32 => format!(
                "(i32.wrap_i64 (i64.load offset={} (local.get {frame_local})))",
                slot.offset
            ),
            ValType::F64 => format!(
                "(f64.reinterpret_i64 (i64.load offset={} (local.get {frame_local})))",
                slot.offset
            ),
            ValType::I64 => format!("(i64.load offset={} (local.get {frame_local}))", slot.offset),
        };
        ctx.fb.ins(&load, "reload a spilled component");
        ctx.fb.ins(&format!("local.set {local}"), "");
    }
}

/// Writes every local into the frame, so the body can return and be re-entered.
pub(super) fn emit_spill(ctx: &mut super::context::FnCtx, frame: &GeneratorFrame, frame_local: &str) {
    for (local, slot) in ordered_components(ctx, frame) {
        let value = match slot.val_type {
            ValType::I32 => format!("(i64.extend_i32_u (local.get {local}))"),
            ValType::F64 => format!("(i64.reinterpret_f64 (local.get {local}))"),
            ValType::I64 => format!("(local.get {local})"),
        };
        ctx.fb.ins(
            &format!(
                "(i64.store offset={} (local.get {frame_local}) {value})",
                slot.offset
            ),
            "spill a live component",
        );
    }
}

/// Pairs each frame slot with the WAT local it belongs to, in a deterministic order.
///
/// The order is the frame's own byte order, so a spill and a reload walk the same
/// sequence and a reader can line the two up in the emitted WAT.
fn ordered_components(
    ctx: &super::context::FnCtx,
    frame: &GeneratorFrame,
) -> Vec<(String, FrameSlot)> {
    let mut components: Vec<(String, FrameSlot)> = Vec::new();
    for (index, placements) in &frame.slots {
        if let Some(repr) = ctx.slot_locals.get(index) {
            for (local, placement) in repr.local_refs().into_iter().zip(placements.iter()) {
                components.push((local, placement.clone()));
            }
        }
    }
    for (index, placements) in &frame.values {
        if let Some(repr) = ctx.value_locals.get(index) {
            for (local, placement) in repr.local_refs().into_iter().zip(placements.iter()) {
                components.push((local, placement.clone()));
            }
        }
    }
    // The handler and its saves, in the frame's own order: `$__handler` first, then one save per
    // token sorted the way the layout reserved them.
    let mut handler_locals: Vec<String> = vec![ctx.handler_local.clone()];
    let mut tokens: Vec<i64> = ctx.handler_saves.keys().copied().collect();
    tokens.sort_unstable();
    handler_locals.extend(tokens.iter().filter_map(|token| ctx.handler_saves.get(token).cloned()));
    for (local, placement) in handler_locals.into_iter().zip(frame.handlers.iter()) {
        components.push((local, placement.clone()));
    }
    // The iterator's locals are paired by POSITION rather than by a repr, because they are three
    // separate locals rather than one value's components. A source whose EIR type already settles
    // its storage has no kind local, and that word simply stays zero.
    for (index, placements) in &frame.iters {
        let Some(slots) = ctx.iter_state.get(index) else { continue };
        let locals = [
            Some(slots.source.clone()),
            Some(slots.cursor.clone()),
            slots.dynamic_kind.clone(),
        ];
        for (local, placement) in locals.into_iter().zip(placements.iter()) {
            if let Some(local) = local {
                components.push((local, placement.clone()));
            }
        }
    }
    components.sort_by_key(|(_, placement)| placement.offset);
    components
}

/// Emits the hand-called iterator protocol: `current`, `key`, `valid`, `next`, `send`.
///
/// `foreach` drives a generator through `iter_next`/`iter_current_*`, which need none of this.
/// Calling the protocol BY HAND is different in one way that matters: nothing has primed the
/// generator, and php primes on the first call of any of them — a generator evaluates nothing
/// until something asks. `FLAG_REWOUND` is the record of whether that has happened, which is
/// what `__rt_gen_prime` reads.
///
/// `send($v)` is the reason the rest are here: the value it passes is what the suspended `yield`
/// EVALUATES TO, which means the yield has to read a frame word rather than answer a constant,
/// and the word has to be cleared after the read or the next resume would see a stale send.
fn emit_protocol_helpers(wm: &mut super::wat::WatModule, raises_into_caller: bool) {
    let flags = gen_frame::OFF_FLAGS;
    let rewound = gen_frame::FLAG_REWOUND;
    let terminated = gen_frame::FLAG_TERMINATED;
    let last_value = gen_frame::OFF_LAST_VALUE;
    let last_key = gen_frame::OFF_LAST_KEY;
    let sent = gen_frame::OFF_SENT_VALUE;

    wm.add_raw_func(&format!(
        r#"(func $__rt_gen_prime (param $frame i32)
  (if (i32.eqz (i32.and (i32.load offset={flags} (local.get $frame)) (i32.const {rewound})))
    (then
      (i32.store offset={flags} (local.get $frame)
        (i32.or (i32.load offset={flags} (local.get $frame)) (i32.const {rewound})))
      ;; A generator a `foreach` already ran to its end (or that an exception ended) must not be
      ;; re-entered at its stale resume point.
      (if (i32.eqz (i32.and (i32.load offset={flags} (local.get $frame)) (i32.const {terminated})))
        (then
          (drop (call $__rt_gen_resume (local.get $frame)))              ;; run to the first yield
          (drop))))))
"#
    ));
    // One `foreach` ADVANCE: every step after the first. The first one is not a step at all, it
    // is the rewind the loop begins with (`lower_iter_start` primes the generator, and the first
    // `iter_next` only reports whether it is still alive), so a generator a caller already moved
    // to its first yield shows that value instead of skipping it.
    //
    // It marks the generator as started, as priming does, and as ADVANCED, so a later `foreach`
    // or `rewind()` refuses exactly as php's does. A finished generator answers 0 without being
    // resumed.
    wm.add_raw_func(&format!(
        r#"(func $__rt_gen_foreach_step (param $frame i32) (result i32)
  (local $has i32)
  (if (i32.and (i32.load offset={flags} (local.get $frame)) (i32.const {terminated}))
    (then (return (i32.const 0))))
  (i32.store offset={flags} (local.get $frame)
    (i32.or (i32.load offset={flags} (local.get $frame)) (i32.const {rewound_or_advanced})))
  (call $__rt_gen_resume (local.get $frame))
  (drop))
"#,
        rewound_or_advanced = rewound | FLAG_ADVANCED as u32,
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_gen_current (param $frame i32) (result i32)
  (local $cell i32)
  (call $__rt_gen_prime (local.get $frame))
  (local.set $cell (i32.wrap_i64 (i64.load offset={last_value} (local.get $frame))))
  (if (i32.eqz (local.get $cell))                                        ;; a finished generator answers null
    (then (return (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))))
  (call $__rt_incref (local.get $cell))
  (local.get $cell))
"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_gen_key (param $frame i32) (result i32)
  (local $cell i32)
  (call $__rt_gen_prime (local.get $frame))
  (local.set $cell (i32.wrap_i64 (i64.load offset={last_key} (local.get $frame))))
  (if (i32.eqz (local.get $cell))
    (then (return (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))))
  (call $__rt_incref (local.get $cell))
  (local.get $cell))
"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_gen_valid (param $frame i32) (result i64)
  (call $__rt_gen_prime (local.get $frame))
  (i64.extend_i32_u
    (i32.eqz (i32.and (i32.load offset={flags} (local.get $frame)) (i32.const {terminated})))))
"#
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_gen_next (param $frame i32)
  (call $__rt_gen_prime (local.get $frame))
  (if (i32.eqz (i32.and (i32.load offset={flags} (local.get $frame)) (i32.const {terminated})))
    (then
      (call $__rt_gen_note_advance (local.get $frame))
      (drop (call $__rt_gen_resume (local.get $frame)))
      (drop))))
"#
    ));
    // `send` takes the value BORROWED: the caller's argument keeps its own reference, and the
    // frame takes one of its own for the word it parks it in.
    wm.add_raw_func(&format!(
        r#"(func $__rt_gen_send (param $frame i32) (param $value i32) (result i32)
  (call $__rt_gen_prime (local.get $frame))                              ;; php primes before sending
  (call $__rt_decref_any (i32.wrap_i64 (i64.load offset={sent} (local.get $frame))))
  (call $__rt_incref (local.get $value))
  (i64.store offset={sent} (local.get $frame) (i64.extend_i32_u (local.get $value)))
  (if (i32.eqz (i32.and (i32.load offset={flags} (local.get $frame)) (i32.const {terminated})))
    (then
      (call $__rt_gen_note_advance (local.get $frame))
      (drop (call $__rt_gen_resume (local.get $frame)))
      (drop)))
  (call $__rt_gen_current (local.get $frame)))
"#
    ));
    // Advancing is what `rewind()` refuses after, and the three helpers that do it say so here
    // rather than each open-coding the bit.
    wm.add_raw_func(&format!(
        r#"(func $__rt_gen_note_advance (param $frame i32)
  (i32.store offset={flags} (local.get $frame)
    (i32.or (i32.load offset={flags} (local.get $frame)) (i32.const {advanced}))))
"#,
        flags = gen_frame::OFF_FLAGS,
        advanced = FLAG_ADVANCED,
    ));
    // `throw($e)` raises INSIDE the suspended body, at the point it is waiting — so the work here
    // is to park the exception and resume; the raise itself happens at the top of the segment the
    // yield resumes into, which is the only place inside this frame's landing pad.
    //
    // A generator that has already finished has no waiting point, and php raises the exception at
    // the CALLER instead: the helper throws it itself, on the module's one exception tag. The
    // raise exists only in a module that declares the tag, which `module_throws_into_a_generator`
    // guarantees for any module that calls `throw()` at all.
    let finished_raise = if raises_into_caller {
        format!(
            r#"  (if (i32.and (i32.load offset={flags} (local.get $frame)) (i32.const {terminated}))
    (then
      (call $__rt_incref (local.get $exception))                         ;; the raise owns a reference
      (global.set ${fatal_code} (i32.const {uncaught}))
      (throw ${tag} (local.get $exception))))
"#,
            flags = gen_frame::OFF_FLAGS,
            terminated = gen_frame::FLAG_TERMINATED,
            fatal_code = super::function::EXCEPTION_FATAL_CODE_GLOBAL,
            uncaught = super::function::UNCAUGHT_EXCEPTION_FAILURE_CODE,
            tag = super::function::EXCEPTION_TAG,
        )
    } else {
        String::new()
    };
    wm.add_raw_func(&format!(
        r#"(func $__rt_gen_throw (param $frame i32) (param $exception i32) (result i32)
  (call $__rt_gen_prime (local.get $frame))                              ;; php primes before throwing
{finished_raise}  (call $__rt_decref_any (i32.wrap_i64 (i64.load offset={sent} (local.get $frame))))
  (call $__rt_incref (local.get $exception))
  (i64.store offset={sent} (local.get $frame) (i64.extend_i32_u (local.get $exception)))
  (i32.store offset={flags} (local.get $frame)
    (i32.or (i32.load offset={flags} (local.get $frame)) (i32.const {pending})))
  (if (i32.eqz (i32.and (i32.load offset={flags} (local.get $frame)) (i32.const {terminated})))
    (then
      (call $__rt_gen_note_advance (local.get $frame))
      (drop (call $__rt_gen_resume (local.get $frame)))
      (drop)))
  (call $__rt_gen_current (local.get $frame)))
"#,
        sent = gen_frame::OFF_SENT_VALUE,
        flags = gen_frame::OFF_FLAGS,
        pending = FLAG_PENDING_THROW,
        terminated = gen_frame::FLAG_TERMINATED,
        finished_raise = finished_raise,
    ));
    // What the suspended `yield` evaluates to. Cleared on the read, so a resume that nobody sent
    // to sees null — which is what php gives.
    wm.add_raw_func(&format!(
        r#"(func $__rt_gen_take_sent (param $frame i32) (result i32)
  (local $cell i32)
  (local.set $cell (i32.wrap_i64 (i64.load offset={sent} (local.get $frame))))
  (i64.store offset={sent} (local.get $frame) (i64.const 0))
  (if (i32.eqz (local.get $cell))
    (then (return (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))))
  (local.get $cell))
"#
    ));
}

/// Emits `__rt_gen_delegate_step`, one advance of a `yield from`.
///
/// Delegation is a yield whose resume point is ITSELF: while the inner generator has values the
/// outer suspends again at the same segment, forwarding the inner's key and value UNCHANGED —
/// php does not renumber them, and it leaves the outer's own `auto_key_counter` alone, so a plain
/// `yield` after the delegation continues where the outer left off rather than after the inner.
///
/// Answers `(has_value, payload)`. With a value, the payload is what the outer reports as yielded
/// and the outer's `last_key`/`last_value` now hold the inner's. Without one, the inner has
/// finished and the payload is its RETURN value — which is what the `yield from` expression
/// evaluates to.
///
/// Two things make this more than a resume. The step has to tell its FIRST call from the rest,
/// because the first must run the inner to its first yield and every later one must advance past
/// the current one — `FLAG_REWOUND` on the INNER is that distinction, the same question
/// `__rt_gen_prime` asks. And a value sent to the OUTER while it delegates belongs to the inner's
/// suspended yield, so the word is MOVED down before the resume; php forwards it the same way.
fn emit_delegate_step(wm: &mut super::wat::WatModule) {
    wm.add_raw_func(&format!(
        r#"(func $__rt_gen_delegate_step (param $frame i32) (result i32) (result i32)
  (local $inner i32) (local $cell i32)
  (local.set $inner (i32.wrap_i64 (i64.load offset={delegated} (local.get $frame))))
  (if (i32.eqz (local.get $inner))                            ;; nothing installed: already finished
    (then
      (return (i32.const 0)
        (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0)))))
  ;; A throw() to the outer is a throw() into the inner's suspended yield: php raises it where
  ;; the INNER is waiting, so the inner's own `try` sees it first. The pending flag travels with
  ;; the parked exception (moved just below with the sent word). An inner that has already
  ;; finished has nowhere to raise it, so the flag stays on the outer and the segment after the
  ;; delegation raises it there.
  (if (i32.and (i32.load offset={flags} (local.get $frame)) (i32.const {pending}))
    (then
      (if (i32.eqz (i32.and (i32.load offset={flags} (local.get $inner)) (i32.const {terminated})))
        (then
          (i32.store offset={flags} (local.get $frame)
            (i32.and (i32.load offset={flags} (local.get $frame)) (i32.const {not_pending})))
          (i32.store offset={flags} (local.get $inner)
            (i32.or (i32.load offset={flags} (local.get $inner)) (i32.const {pending})))))))
  ;; A send() to the outer is a send() to the inner's suspended yield. Moved, not copied: the
  ;; outer's word gives up its reference to the inner's.
  (local.set $cell (i32.wrap_i64 (i64.load offset={sent} (local.get $frame))))
  (if (i32.and (i32.ne (local.get $cell) (i32.const 0))
        (i32.eqz (i32.and (i32.load offset={flags} (local.get $frame)) (i32.const {pending}))))
    (then
      (i64.store offset={sent} (local.get $frame) (i64.const 0))
      (call $__rt_decref_any (i32.wrap_i64 (i64.load offset={sent} (local.get $inner))))
      (i64.store offset={sent} (local.get $inner) (i64.extend_i32_u (local.get $cell)))))
  (if (i32.eqz (i32.and (i32.load offset={flags} (local.get $inner)) (i32.const {rewound})))
    (then (call $__rt_gen_prime (local.get $inner)))           ;; first step: run to its first yield
    (else
      (if (i32.eqz (i32.and (i32.load offset={flags} (local.get $inner)) (i32.const {terminated})))
        (then
          (drop (call $__rt_gen_resume (local.get $inner)))     ;; every later step advances
          (drop)))))
  (if (i32.eqz (i32.and (i32.load offset={flags} (local.get $inner)) (i32.const {terminated})))
    (then
      ;; Forward the inner's key and value as the outer's own, each taking its own reference.
      (local.set $cell (i32.wrap_i64 (i64.load offset={last_key} (local.get $inner))))
      (call $__rt_incref (local.get $cell))
      (call $__rt_decref_any (i32.wrap_i64 (i64.load offset={last_key} (local.get $frame))))
      (i64.store offset={last_key} (local.get $frame) (i64.extend_i32_u (local.get $cell)))
      (local.set $cell (i32.wrap_i64 (i64.load offset={last_value} (local.get $inner))))
      (call $__rt_incref (local.get $cell))
      (call $__rt_decref_any (i32.wrap_i64 (i64.load offset={last_value} (local.get $frame))))
      (i64.store offset={last_value} (local.get $frame) (i64.extend_i32_u (local.get $cell)))
      (return (i32.const 1) (local.get $cell))))
  ;; Exhausted: the delegation's value is the inner's return, and the outer stops delegating.
  (local.set $cell (i32.wrap_i64 (i64.load offset={return_value} (local.get $inner))))
  (if (i32.eqz (local.get $cell))
    (then
      (local.set $cell
        (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0))))
    (else (call $__rt_incref (local.get $cell))))
  (call $__rt_decref_any (local.get $inner))                   ;; the frame gives up the inner
  (i64.store offset={delegated} (local.get $frame) (i64.const 0))
  (i32.const 0)
  (local.get $cell))
"#,
        delegated = gen_frame::OFF_DELEGATED_ITER,
        sent = gen_frame::OFF_SENT_VALUE,
        flags = gen_frame::OFF_FLAGS,
        rewound = gen_frame::FLAG_REWOUND,
        terminated = gen_frame::FLAG_TERMINATED,
        pending = FLAG_PENDING_THROW,
        not_pending = !FLAG_PENDING_THROW,
        last_key = gen_frame::OFF_LAST_KEY,
        last_value = gen_frame::OFF_LAST_VALUE,
        return_value = gen_frame::OFF_RETURN_VALUE,
    ));
}

/// Emits the two helpers that maintain a generator's KEY, which php numbers for you.
///
/// `yield $v` carries no key, and php answers 0, 1, 2 … per generator — but the counter is
/// `largest_used_integer_key + 1`, not a yield count: an explicit `yield 5 => $v` moves it, so
/// the next plain yield answers 6. A STRING key leaves it alone. `__rt_gen_note_key` applies
/// exactly that rule to a key the body supplied, and `__rt_gen_auto_key` applies it to one the
/// body did not.
///
/// Both own the cell they store: the previous key is released first, the way every other slot
/// that holds a boxed value here is written.
fn emit_key_helpers(wm: &mut super::wat::WatModule) {
    wm.add_raw_func(&format!(
        r#"(func $__rt_gen_note_key (param $frame i32) (param $key i32)
  (local $tag i64) (local $next i64)
  (call $__rt_decref_any (i32.wrap_i64 (i64.load offset={last_key} (local.get $frame))))
  (i64.store offset={last_key} (local.get $frame) (i64.extend_i32_u (local.get $key)))
  (local.set $tag (call $__rt_mixed_tag_of (local.get $key)))
  (if (i64.eq (local.get $tag) (i64.const 0))                    ;; an INTEGER key moves the counter
    (then
      (local.set $next (i64.add (i64.load (i32.add (local.get $key) (i32.const 8))) (i64.const 1)))
      (if (i64.gt_s (local.get $next) (i64.load offset={counter} (local.get $frame)))
        (then (i64.store offset={counter} (local.get $frame) (local.get $next))))))
  )
"#,
        last_key = gen_frame::OFF_LAST_KEY,
        counter = gen_frame::OFF_AUTO_KEY_COUNTER,
    ));
    // A DELEGATED yield forwards a key that is not this generator's to number. `yield from
    // [10, 20]` desugars to a loop of these, and php leaves the outer counter alone through it:
    // a plain `yield 30` after that delegation answers 0, not 2. So the key is stored and the
    // counter is not consulted — the one difference between this and `__rt_gen_note_key`.
    wm.add_raw_func(&format!(
        r#"(func $__rt_gen_forward_key (param $frame i32) (param $key i32)
  (call $__rt_decref_any (i32.wrap_i64 (i64.load offset={last_key} (local.get $frame))))
  (i64.store offset={last_key} (local.get $frame) (i64.extend_i32_u (local.get $key))))
"#,
        last_key = gen_frame::OFF_LAST_KEY,
    ));
    wm.add_raw_func(&format!(
        r#"(func $__rt_gen_auto_key (param $frame i32)
  (local $n i64)
  (local.set $n (i64.load offset={counter} (local.get $frame)))
  (call $__rt_decref_any (i32.wrap_i64 (i64.load offset={last_key} (local.get $frame))))
  (i64.store offset={last_key} (local.get $frame)
    (i64.extend_i32_u (call $__rt_mixed_from_value (i64.const 0) (local.get $n) (i64.const 0))))
  (i64.store offset={counter} (local.get $frame) (i64.add (local.get $n) (i64.const 1)))
  )
"#,
        last_key = gen_frame::OFF_LAST_KEY,
        counter = gen_frame::OFF_AUTO_KEY_COUNTER,
    ));
}

/// Emits `__rt_gen_resume`, the if-ladder that runs the right generator's body.
///
/// An if-ladder over a dense id rather than a `call_indirect`, because this backend
/// resolves every such dispatch that way (`includes.rs` says so for the include
/// variant ladder) and a closed world knows every generator at compile time. wasm has
/// no function pointers, so `resume_fn_ptr` carries the id instead of an address.
///
/// Answers `(has_value, value)`: a suspended generator reports 1 and the boxed value
/// it yielded, a finished one reports 0 and its boxed return value.
pub(super) fn emit_resume_dispatcher(wm: &mut super::wat::WatModule, module: &Module) {
    let generators = generator_functions(module);
    if generators.is_empty() {
        return;
    }
    emit_key_helpers(wm);
    emit_protocol_helpers(wm, super::function::module_throws_into_a_generator(module));
    emit_delegate_step(wm);
    let mut wat = String::from(
        "(func $__rt_gen_resume (param $frame i32) (result i32) (result i32)\n  (local $id i32)\n",
    );
    wat.push_str(&format!(
        "  (local.set $id (i32.load offset={} (local.get $frame)))   ;; the generator id stamped by the wrapper\n",
        gen_frame::OFF_RESUME_FN
    ));
    for (id, function) in generators.iter().enumerate() {
        wat.push_str(&format!(
            "  (if (i32.eq (local.get $id) (i32.const {id}))\n    (then (return (call ${} (local.get $frame)))))\n",
            resume_symbol(function)
        ));
    }
    // An id no arm claims cannot come from a wrapper this planner emitted, so it is a
    // corrupted frame rather than an unsupported generator: report finished and null
    // rather than running an arbitrary body.
    wat.push_str("  (i32.const 0)\n  (i32.const 0))\n");
    wm.add_raw_func(&wat);
}

/// Emits `__rt_gen_frame_release` and one `<f>__release_locals` per generator.
///
/// A frame is not an object, so the object walk cannot release it: the values it owns are in
/// the 80-byte header (`last_value`, `return_value`, and the words a later slice will use) and
/// in the local area, while that walk only knows declared property slots at `8 + i*16`. Hence
/// its own heap kind and its own release.
///
/// The local area is released **only while the generator is still suspended**. A body that ran
/// to its end already released its own locals in the function epilogue, and `FLAG_TERMINATED`
/// is exactly the record of which of the two happened — releasing them again would be a double
/// free, and not releasing an abandoned generator's would strand every string and container it
/// was holding.
pub(super) fn emit_frame_release(wm: &mut super::wat::WatModule, module: &Module) {
    let generators = generator_functions(module);
    if generators.is_empty() {
        // `__rt_decref_any`'s kind-7 arm names this unconditionally, so a module without a
        // generator still has to define it or the WAT fails to assemble with "unknown func".
        // Nothing can reach it — no frame exists to carry kind 7 — which is what makes an
        // empty body the honest one. Mirrors `emit_gc_desc_stub` for the object walk.
        wm.add_raw_func("(func $__rt_gen_frame_release (param $frame i32))\n");
        return;
    }
    for function in &generators {
        let frame = frame_layout(function);
        let mut owned: Vec<&FrameSlot> = frame
            .slots
            .values()
            .flatten()
            .chain(frame.values.values().flatten())
            .filter(|slot| slot.owns_heap)
            .collect();
        owned.sort_by_key(|slot| slot.offset);
        let mut wat = format!(
            "(func ${}__release_locals (param $frame i32)\n",
            super::symbols::function_symbol(function)
        );
        for slot in owned {
            wat.push_str(&format!(
                "  (call $__rt_decref_any (i32.wrap_i64 (i64.load offset={} (local.get $frame))))\n",
                slot.offset
            ));
        }
        wat.push_str("  )\n");
        wm.add_raw_func(&wat);
    }

    let mut wat = String::from(
        "(func $__rt_gen_frame_release (param $frame i32)\n  (local $rc i32)\n  (local $id i32)\n",
    );
    wat.push_str("  (if (i32.eqz (local.get $frame)) (then (return)))\n");
    wat.push_str(
        "  (local.set $rc (i32.sub (i32.load (i32.sub (local.get $frame) (i32.const 12))) (i32.const 1)))\n",
    );
    wat.push_str("  (i32.store (i32.sub (local.get $frame) (i32.const 12)) (local.get $rc))\n");
    wat.push_str("  (if (i32.gt_s (local.get $rc) (i32.const 0)) (then (return)))   ;; still referenced\n");
    for (offset, what) in [
        (gen_frame::OFF_LAST_KEY, "the key the last yield recorded"),
        (gen_frame::OFF_LAST_VALUE, "the value the last yield recorded"),
        (gen_frame::OFF_RETURN_VALUE, "what getReturn() would have answered"),
        (gen_frame::OFF_SENT_VALUE, "whatever send() left"),
        (gen_frame::OFF_DELEGATED_ITER, "an inner iterator, once yield from exists"),
    ] {
        wat.push_str(&format!(
            "  (call $__rt_decref_any (i32.wrap_i64 (i64.load offset={offset} (local.get $frame))))   ;; {what}\n"
        ));
    }
    wat.push_str(&format!(
        "  (if (i32.eqz (i32.and (i32.load offset={} (local.get $frame)) (i32.const {})))\n",
        gen_frame::OFF_FLAGS,
        gen_frame::FLAG_TERMINATED
    ));
    wat.push_str("    (then   ;; still suspended: the frame owns the locals the spill wrote\n");
    wat.push_str(&format!(
        "      (local.set $id (i32.load offset={} (local.get $frame)))\n",
        gen_frame::OFF_RESUME_FN
    ));
    for (id, function) in generators.iter().enumerate() {
        wat.push_str(&format!(
            "      (if (i32.eq (local.get $id) (i32.const {id}))\n        (then (call ${}__release_locals (local.get $frame))))\n",
            super::symbols::function_symbol(function)
        ));
    }
    wat.push_str("    ))\n");
    if super::handles::module_uses_object_handles(module) {
        wat.push_str(
            "  (call $__rt_handle_forget (local.get $frame))   ;; the php handle is reusable once the frame is gone\n",
        );
    }
    wat.push_str("  (call $__rt_heap_free (local.get $frame)))\n");
    wm.add_raw_func(&wat);
}

/// Whether a generator's by-value parameter can be boxed into the Mixed slot its body widens it
/// to, and the `(tag, lo, hi)` operands that box it.
///
/// Answers `None` for a shape the wrapper cannot convert, which `capability` refuses by name
/// rather than letting the argument land in a word of the wrong width.
pub(super) fn widened_argument_cell(
    param: &crate::ir::FunctionParam,
    components: &[(String, ValType)],
) -> Option<(String, String, String)> {
    let get = |index: usize| format!("(local.get {})", components[index].0);
    let zero = "(i64.const 0)".to_string();
    match (param.ir_type, components.len()) {
        (IrType::I64, 1) => {
            let tag = match param.php_type.codegen_repr() {
                PhpType::Int => 0,
                PhpType::Bool => 3,
                _ => return None,
            };
            Some((format!("(i64.const {tag})"), get(0), zero))
        }
        (IrType::F64, 1) => Some((
            "(i64.const 2)".to_string(),
            format!("(i64.reinterpret_f64 {})", get(0)),
            zero,
        )),
        (IrType::Str, 2) => Some((
            "(i64.const 1)".to_string(),
            format!("(i64.extend_i32_u {})", get(0)),
            get(1),
        )),
        (IrType::Heap(kind), 1) => {
            let tag = match kind {
                IrHeapKind::Array => 4,
                IrHeapKind::Hash => 5,
                IrHeapKind::Object => 6,
                _ => return None,
            };
            Some((format!("(i64.const {tag})"), format!("(i64.extend_i32_u {})", get(0)), zero))
        }
        _ => None,
    }
}

/// Emits the wrapper that `g(...)` itself becomes: it allocates the frame and answers
/// it as the `Generator` object, without running a line of the body.
///
/// php evaluates nothing in a generator until the first `rewind()`/`valid()`, so the
/// wrapper's whole job is the frame. Parameters are stored into their slot words in
/// the frame's local area, at the offsets `frame_layout` gives — the same table the
/// resume prologue reloads them from.
pub(super) fn emit_wrapper(
    wm: &mut super::wat::WatModule,
    module: &Module,
    function: &Function,
    class_id: u64,
) -> Result<(), super::WasmError> {
    let frame = frame_layout(function);
    let id = generator_id(module, function).ok_or_else(|| {
        super::WasmError::Unsupported(format!(
            "generator {} has no id in the module's generator list",
            function.name
        ))
    })?;
    let symbol = super::symbols::function_symbol(function);
    let mut wat = format!("(func ${symbol} (export \"{symbol}\")");
    let mut param_components: Vec<(String, ValType)> = Vec::new();
    for (index, param) in function.params.iter().enumerate() {
        for (component, val_type) in WasmRepr::val_types(param.ir_type).into_iter().enumerate() {
            let name = format!("$p{index}_{component}");
            wat.push_str(&format!(" (param {name} {})", val_type.as_str()));
            param_components.push((name, val_type));
        }
    }
    wat.push_str(" (result i32)\n  (local $frame i32)\n");
    wat.push_str(&format!(
        "  (local.set $frame (call $__rt_heap_alloc (i32.const {})))   ;; the suspended generator's whole state\n",
        frame.size
    ));
    // The frame IS the Generator object, so it carries an object header: the kind word
    // sits below the payload exactly as `emit_object_allocation` writes it.
    wat.push_str(&format!(
        "  (i64.store (i32.sub (local.get $frame) (i32.const 8)) (i64.const {HEAP_KIND_GEN_FRAME}))   ;; a generator frame releases its own way\n"
    ));
    wat.push_str(&format!(
        "  (i64.store offset={} (local.get $frame) (i64.const {class_id}))   ;; the synthetic Generator class id\n",
        gen_frame::OFF_CLASS_ID
    ));
    if super::handles::module_uses_object_handles(module) {
        // A generator IS an object to php, so `spl_object_id()` must answer a handle for it.
        wat.push_str(
            "  (drop (call $__rt_handle_register (local.get $frame)))   ;; its php object handle\n",
        );
    }
    wat.push_str(&format!(
        "  (i32.store offset={} (local.get $frame) (i32.const {id}))   ;; which resume arm runs this body\n",
        gen_frame::OFF_RESUME_FN
    ));
    wat.push_str(&format!(
        "  (i32.store offset={} (local.get $frame) (i32.const 0))   ;; state 0 is the entry block\n",
        gen_frame::OFF_STATE_IDX
    ));
    wat.push_str(&format!(
        "  (i32.store offset={} (local.get $frame) (i32.const 0))   ;; neither rewound nor terminated\n",
        gen_frame::OFF_FLAGS
    ));
    for offset in [
        gen_frame::OFF_AUTO_KEY_COUNTER,
        gen_frame::OFF_LAST_KEY,
        gen_frame::OFF_LAST_VALUE,
        gen_frame::OFF_RETURN_VALUE,
        gen_frame::OFF_SENT_VALUE,
        gen_frame::OFF_DELEGATED_ITER,
        gen_frame::OFF_LAYOUT_ID,
    ] {
        wat.push_str(&format!(
            "  (i64.store offset={offset} (local.get $frame) (i64.const 0))\n"
        ));
    }
    // Zero the local area: the resume prologue reloads EVERY component, so an
    // unwritten one would arrive as whatever the allocator left behind — and the
    // epilogue's release walk would read it as a pointer.
    let mut zeroed: Vec<&FrameSlot> = frame.slots.values().flatten().collect();
    zeroed.extend(frame.values.values().flatten());
    zeroed.extend(frame.iters.values().flatten());
    zeroed.sort_by_key(|slot| slot.offset);
    for slot in zeroed {
        wat.push_str(&format!(
            "  (i64.store offset={} (local.get $frame) (i64.const 0))\n",
            slot.offset
        ));
    }
    // The handler words are the exception: their "nothing armed" value is -1, because block 0 is
    // a legitimate handler and zero cannot mean both. A zeroed word would have every resume enter
    // the entry segment as though it were a catch.
    for slot in &frame.handlers {
        wat.push_str(&format!(
            "  (i64.store offset={} (local.get $frame) (i64.const -1))   ;; no handler is armed yet\n",
            slot.offset
        ));
    }
    // The frame outlives this call, so it takes its own reference on every argument it will
    // release: an ordinary callee borrows its parameters, and the caller frees a boxed or
    // temporary argument as soon as the call returns — before the body has run a line. A string
    // may be a concat-buffer view the next statement overwrites, so it gets a private copy. An
    // indexed-array parameter is the exception: the caller already hands it over (see
    // `emit_local_epilogue_cleanup`), and a second reference would never be given back.
    // A by-value parameter the body widens (`function g(string $s) { $s = 5; yield; }`) has a
    // slot laid out for the WIDENED type, a single Mixed word, while the argument arrives at its
    // declared width. Native boxes it in the prologue; so does this: the frame's word is a fresh
    // cell that owns what it was built from (`__rt_mixed_from_value` persists a string and
    // increfs a container), so such a parameter needs no ownership step of its own.
    let widened: Vec<bool> = function
        .params
        .iter()
        .zip(&function.locals)
        .map(|(param, local)| !param.by_ref && param.ir_type != local.ir_type)
        .collect();
    let mut component = 0usize;
    for (index, param) in function.params.iter().enumerate() {
        let callee_owned = matches!(param.php_type.codegen_repr(), PhpType::Array(_));
        if widened[index] {
            component += WasmRepr::val_types(param.ir_type).len();
            continue;
        }
        match param.ir_type {
            IrType::Heap(_) if !param.by_ref && !callee_owned => {
                let (name, _) = &param_components[component];
                wat.push_str(&format!(
                    "  (call $__rt_incref (local.get {name}))   ;; the frame owns argument {index}
"
                ));
            }
            IrType::I64 if param_is_frame_owned(param) => {
                let (name, _) = &param_components[component];
                wat.push_str(&format!(
                    "  (call $__rt_incref (i32.wrap_i64 (local.get {name})))   ;; the frame owns callable argument {index}
"
                ));
            }
            IrType::Str if !param.by_ref => {
                let (ptr, _) = &param_components[component];
                let (len, _) = &param_components[component + 1];
                wat.push_str(&format!(
                    "  (call $__rt_str_persist (local.get {ptr}) (local.get {len}))   ;; the frame's own copy of argument {index}
  (local.set {len})
  (local.set {ptr})
"
                ));
            }
            _ => {}
        }
        component += WasmRepr::val_types(param.ir_type).len();
    }
    // Then the arguments, into the slots their parameters share. The component cursor walks the
    // PARAMETERS' components, not the slots': the two differ for a widened parameter, and
    // advancing by the slot's width shifted every later argument.
    let mut component = 0usize;
    for (index, param) in function.params.iter().enumerate() {
        let width = WasmRepr::val_types(param.ir_type).len();
        let placements = frame.slots.get(&(index as u32)).cloned().unwrap_or_default();
        if widened[index] {
            let (tag, lo, hi) =
                widened_argument_cell(param, &param_components[component..component + width])
                    .ok_or_else(|| {
                        super::WasmError::Unsupported(format!(
                            "generator {} parameter ${} cannot be boxed into its widened slot",
                            function.name, param.name
                        ))
                    })?;
            if let Some(slot) = placements.first() {
                wat.push_str(&format!(
                    "  (i64.store offset={} (local.get $frame) (i64.extend_i32_u (call $__rt_mixed_from_value {tag} {lo} {hi})))   ;; argument {index}, boxed into its widened slot
",
                    slot.offset
                ));
            }
            component += width;
            continue;
        }
        for placement in placements {
            let (name, val_type) = &param_components[component];
            component += 1;
            let value = match (val_type, placement.val_type) {
                (ValType::I32, _) => format!("(i64.extend_i32_u (local.get {name}))"),
                (ValType::F64, _) => format!("(i64.reinterpret_f64 (local.get {name}))"),
                _ => format!("(local.get {name})"),
            };
            wat.push_str(&format!(
                "  (i64.store offset={} (local.get $frame) {value})   ;; argument {index} into its slot
",
                placement.offset
            ));
        }
    }
    wat.push_str("  (local.get $frame))\n");
    wm.add_raw_func(&wat);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Builder, Function, IrType, Op, Ownership, Terminator};
    use crate::types::PhpType;

    /// A block splits into ONE SEGMENT PER YIELD, which is the whole reason the dispatch loop
    /// grew segments: a yield RETURNS, and every yield this backend sees sits mid-block with
    /// code after it, so an EIR block index cannot name the point to resume at.
    ///
    /// The frame's local area comes from the same function, and the spill and the reload walk it
    /// from the same table — so the packing is checked here too. A gap or a double-booked word
    /// would show up as a value arriving in the wrong local after a suspension, which is the
    /// hardest kind of generator bug to read back from a wrong answer.
    #[test]
    fn a_block_splits_into_one_segment_per_yield() {
        let mut function = Function::new(
            "g".to_string(),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        function.flags.is_generator = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            for value in [1i64, 2, 3] {
                let literal = builder.emit_const_i64(value);
                builder.emit(
                    Op::GeneratorYield,
                    vec![literal],
                    None,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                );
            }
            builder.terminate(Terminator::Return { value: None });
        }

        let frame = frame_layout(&function);
        let mut offsets: Vec<u32> = frame
            .slots
            .values()
            .flatten()
            .chain(frame.values.values().flatten())
            .map(|slot| slot.offset)
            .collect();
        offsets.sort_unstable();
        let expected: Vec<u32> = (0..offsets.len())
            .map(|index| gen_frame::FIXED_HEADER_BYTES as u32 + 8 * index as u32)
            .collect();
        assert_eq!(
            offsets, expected,
            "the local area is packed, one 8-byte word per component, past the header"
        );
        assert!(
            frame.size >= gen_frame::FIXED_HEADER_BYTES as u32 + 8 * offsets.len() as u32,
            "the frame has to cover its own local area"
        );
        assert_eq!(
            frame.size % 16,
            0,
            "__rt_heap_alloc hands back 16-byte-aligned blocks"
        );

        // Three yields in one block: four segments, every one of them that block's.
        let (segments, block_entry) = super::super::function::plan_segments_for_test(&function);
        assert_eq!(
            segments.len(),
            4,
            "one segment per yield, plus the tail that carries the terminator"
        );
        assert!(segments.iter().all(|(block, _)| *block == 0));
        assert_eq!(
            block_entry,
            vec![0],
            "the block still starts where it always did, so every branch target is unchanged"
        );
    }

    /// A DELEGATION is alone in its segment, because its resume point is itself.
    ///
    /// The instruction before it here is the `inner()` call that builds the generator being
    /// delegated to, and that is exactly the shape the split-before exists for: re-entering a
    /// segment holding both would call `inner()` again on every resume and delegate to a fresh
    /// generator each time, so the delegation would never advance past the inner's first value.
    #[test]
    fn a_delegation_gets_a_segment_of_its_own() {
        let mut function = Function::new(
            "outer".to_string(),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        function.flags.is_generator = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let literal = builder.emit_const_i64(1);
            builder.emit(
                Op::GeneratorYield,
                vec![literal],
                None,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            );
            let inner = builder.emit_const_i64(0); // stands in for the `inner()` call
            builder.emit(
                Op::GeneratorYieldFrom,
                vec![inner],
                None,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            );
            let after = builder.emit_const_i64(2);
            builder.emit(
                Op::GeneratorYield,
                vec![after],
                None,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        let (segments, block_entry) = super::super::function::plan_segments_for_test(&function);
        assert_eq!(block_entry, vec![0], "the block still starts where it did");
        let lengths: Vec<usize> = segments.iter().map(|(_, ids)| ids.len()).collect();
        assert_eq!(
            lengths,
            vec![2, 1, 1, 2, 0],
            "const+yield, the const that builds the inner, the delegation ALONE, const+yield, tail"
        );
        let delegation = segments[2]
            .1
            .first()
            .and_then(|id| function.instruction(*id))
            .map(|inst| inst.op);
        assert_eq!(
            delegation,
            Some(Op::GeneratorYieldFrom),
            "the delegation is the only instruction of the segment it resumes into"
        );
    }

    /// A function that does NOT yield gets exactly one segment per block.
    ///
    /// This is what makes the split invisible to every other function: same arms, same
    /// `block_entry`, byte-identical WAT.
    #[test]
    fn a_function_without_yields_keeps_one_segment_per_block() {
        let mut function = Function::new("f".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            let second = builder.create_named_block("second", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let _ = builder.emit_const_i64(7);
            builder.terminate(Terminator::Br {
                target: second,
                args: Vec::new(),
            });
            builder.position_at_end(second);
            builder.terminate(Terminator::Return { value: None });
        }
        let (segments, block_entry) = super::super::function::plan_segments_for_test(&function);
        assert_eq!(segments.len(), 2, "two blocks, two segments");
        assert_eq!(block_entry, vec![0, 1], "and the indices are the block indices");
    }
}
