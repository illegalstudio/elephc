//! Purpose:
//! Type-aware value-transfer layer for the wasm32-wasi backend.
//!
//! Moves EIR values and raw operand-stack values between WebAssembly
//! representations, applying PHP/EIR conversions such as concrete-to-Mixed boxing.
//! Compares concrete WASM types and payload shapes, not only component counts.
//!
//! Called from:
//! - `crate::codegen_wasm::function` for block-argument materialization and the
//!   main-function `$argc`/`$argv` prologue.
//! - `crate::codegen_wasm::inst` for call-result storage, local loads/stores, and
//!   value forwarding.
//!
//! Key details:
//! - Identical storage representations copy component-wise; concrete-to-Mixed
//!   destinations box via `__rt_mixed_from_value`.
//! - Mixed-to-concrete transfers use the runtime cast helpers, and heap-pointer
//!   unboxing validates the runtime tag before exposing the payload.
//! - Unsupported shape-mismatched transfers return a precise `WasmError`
//!   instead of emitting invalid WAT.
//! - All multi-source transfers (block arguments) load every source into temps
//!   before storing any destination, preserving parallel-move safety.

use super::context::{FnCtx, Result};
use super::values::WasmRepr;
use super::wat::ValType;
use super::WasmError;
use crate::ir::{IrHeapKind, IrType, LocalSlotId, ValueId};
use crate::types::PhpType;

/// Elephc's null sentinel value, loaded for the result of a void callee.
const VOID_SENTINEL: i64 = 0x7fff_ffff_ffff_fffe;

/// The exact conversion family shared by capability validation and emission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TransferKind {
    /// Source and destination have the same concrete WASM representation.
    Copy,
    /// A concrete source is boxed into a runtime Mixed cell.
    BoxMixed,
    /// A runtime Mixed cell is converted to a concrete destination.
    UnboxMixed,
    /// A concrete-element array is copied into a fresh Mixed-cell array.
    ///
    /// `tag` is the cell tag every element gets and `elem_size` the source slot stride.
    WidenArrayToMixed { tag: i64, elem_size: i64 },
    /// PHP `null` is stored into a heap-pointer destination.
    ///
    /// Null IS the zero pointer in every heap slot this backend emits — it is what the
    /// refcount helpers guard on and what a missed object read already answers — so the
    /// transfer is the i64 source dropped and a zero pointer pushed in its place.
    NullPointer,
    /// PHP `null` is stored into a TAGGED scalar destination.
    ///
    /// The nullable-scalar twin of `NullPointer`: a `?int` parameter is a two-word
    /// `{payload, tag}` slot, and null there is tag 8 with an unread payload — the same pair
    /// `__rt_array_get_tagged_int` answers for a missing element. Passing the `null` LITERAL to
    /// one arrives as `Void`/`I64`, which is a different width, so it is a conversion rather
    /// than a copy.
    TaggedNull,
    /// A concrete scalar is widened into a TAGGED scalar destination.
    ///
    /// The non-null half of `TaggedNull`: `d(42)` on `d(?int $v)` carries the value in the
    /// payload and names its kind in the tag. `tag` is the Mixed tag for the source's own type,
    /// and `reinterpret` says whether the payload has to be read out of an f64.
    TaggedScalarFromConcrete { tag: i32, reinterpret: bool },
    /// A TAGGED scalar is narrowed to a concrete one: the payload is kept and the tag dropped.
    ///
    /// The other direction of `TaggedScalarFromConcrete`, and what `f(...$args)` needs: the
    /// spread reads each element as `int|null`, because an element read MIGHT miss, and hands
    /// it to a declared `int` parameter. A spread of `[1, 2]` never produces the null, so the
    /// tag carries no information the callee can use.
    ///
    /// Dropping it rather than checking it is the NATIVE contract, not a shortcut: native keeps
    /// a tagged value's payload in the integer result register and its tag in a separate one,
    /// so passing it to an `int` parameter reads the payload register and never looks at the
    /// tag. The two backends therefore answer alike, and php never reaches the case — a
    /// genuinely null element at a non-nullable parameter is a TypeError the checker raises
    /// before either backend sees it.
    ///
    /// `reinterpret` says the destination is an f64 and the payload carries its bits.
    TaggedScalarToConcrete { reinterpret: bool },
    /// An INT-typed source is stored into a `bool` destination, normalized to 0 or 1.
    ///
    /// `isset(...)` is what needs it: the construct's result is typed `int` by the EIR and the
    /// slot the ternary around it writes is typed `bool`, so `isset($c->p) ? "y" : "n"` was
    /// refused for a transfer, two layers below the opcode anyone would have looked at.
    ///
    /// The conversion is php's truthiness rather than a bit copy, and it is deliberately the
    /// wider rule: `isset` only ever yields 0 or 1, for which normalizing changes nothing, and
    /// anything else reaching a bool slot is a value php would read as `true`. A one-word slot
    /// holding the NULL SENTINEL is the one collision, and it is the encoding's own — the same
    /// one the `Void | Int` copy arm above documents, and the native backend answers alike.
    BoolFromInt,
}

/// Returns the cell tag and source slot stride for widening an `array<T>` to `array<mixed>`.
///
/// Only the element types this target has a concrete array layout for can be widened. `Void` is
/// what `Never` — the element type of a literal `[]` — normalizes to: there is nothing to convert,
/// so the tag and stride are never read.
pub(super) fn array_widen_shape(element: &PhpType) -> Option<(i64, i64)> {
    Some(match element.codegen_repr() {
        PhpType::Int => (0, 8),
        PhpType::Str => (1, 16),
        PhpType::Bool => (3, 8),
        PhpType::Float => (2, 8),
        // An object slot holds its pointer; `__rt_mixed_from_value` increfs a refcounted child,
        // so the copy owns its own share.
        PhpType::Object(_) => (6, 8),
        PhpType::Void => (0, 8),
        _ => return None,
    })
}

/// Returns true when this is an indexed array PROVEN to have no elements.
///
/// `Never` is the element type the checker gives a literal `[]`; `codegen_repr` normalizes it to
/// `Void`. Such an array has no decided slot layout yet, so its pointer is interchangeable with
/// any other element type's.
fn array_is_empty(php: &PhpType) -> bool {
    matches!(php, PhpType::Array(element) if matches!(element.codegen_repr(), PhpType::Void))
}

/// Returns true when an EIR/PHP pair is realized as a runtime Mixed cell pointer.
///
/// Ordinary unions are normalized to `Mixed` before this boundary; nullable-int
/// unions use `TaggedScalar` and must not be treated as Mixed cells.
fn is_mixed_cell_storage(ir: IrType, php: &PhpType) -> bool {
    ir == IrType::Heap(IrHeapKind::Mixed) && php.codegen_repr() == PhpType::Mixed
}

/// Validates that one PHP type uses its canonical EIR storage representation.
///
/// Null values are the sole exception: EIR materializes their sentinel as
/// `I64/Void`, while function-level void results use `Void/Void`.
pub(super) fn validate_storage_pair(ir: IrType, php: &PhpType) -> Result<()> {
    let php = php.codegen_repr();
    let canonical_ir = IrType::from_php(&php);
    if ir == canonical_ir || (ir == IrType::I64 && php == PhpType::Void) {
        return Ok(());
    }
    Err(WasmError::Unsupported(format!(
        "invalid wasm storage pair {:?}/{:?}; canonical EIR storage is {:?}",
        ir, php, canonical_ir
    )))
}

/// Returns true when a source representation can be copied bit-wise into the
/// destination representation without any PHP-level conversion.
fn reprs_match_for_copy(
    source_repr: &WasmRepr,
    source_ir: IrType,
    source_php: &PhpType,
    dest_repr: &WasmRepr,
    dest_ir: IrType,
    dest_php: &PhpType,
) -> bool {
    match (source_repr, dest_repr) {
        (WasmRepr::I64(_), WasmRepr::I64(_))
        | (WasmRepr::F64(_), WasmRepr::F64(_))
        | (WasmRepr::Void, WasmRepr::Void)
        | (WasmRepr::Str { .. }, WasmRepr::Str { .. })
        | (WasmRepr::Tagged { .. }, WasmRepr::Tagged { .. }) => source_php == dest_php,
        (WasmRepr::Ptr(_), WasmRepr::Ptr(_)) => {
            // If either side is a Mixed cell, the transfer is not a plain copy:
            // concrete-to-Mixed must box, and Mixed-to-concrete must unbox.
            if is_mixed_cell_storage(source_ir, source_php)
                || is_mixed_cell_storage(dest_ir, dest_php)
            {
                return false;
            }
            // Concrete heap pointers are bit-compatible only when their EIR
            // storage kinds agree. Cross-kind pointer reinterpretation requires
            // an explicit lowering path instead of a count-only copy.
            //
            // Two indexed arrays whose ELEMENTS differ are not bit-compatible either: this
            // target specializes slot width and value_type per element type, so that pair is
            // a widening conversion (handled below), never a copy.
            //
            // Unless one of them is EMPTY. An array whose element type is `Never` has no
            // elements and no decided layout — `__rt_array_push_*` shapes slot width and
            // value_type on the FIRST push — so its pointer is interchangeable with any
            // element type's. That is the `$out = []; $out[] = ...;` accumulator, where the
            // slot is typed from the empty literal and the value from what gets pushed, and
            // the two meet at the loop's phi in both directions.
            if source_ir == dest_ir
                && source_ir == IrType::Heap(IrHeapKind::Array)
                && (array_is_empty(source_php) || array_is_empty(dest_php))
            {
                return true;
            }
            // Two OBJECT pointers are bit-compatible whatever their classes: an object is one
            // pointer to a header that names its own runtime class, which is why `instanceof`
            // and virtual dispatch both work off the value rather than the static type. Whether
            // a given class may stand in for another is a SEMANTIC question, and it is answered
            // where the class hierarchy is in scope — `argument_is_a_descendant_of_the_parameter`
            // in the capability audit, which admits a subclass and nothing else. Deciding it here
            // instead would mean refusing on the class NAME, which is not a storage fact.
            if source_ir == dest_ir
                && source_ir == IrType::Heap(IrHeapKind::Object)
                && matches!(source_php, PhpType::Object(_))
                && matches!(dest_php, PhpType::Object(_))
            {
                return true;
            }
            // An array or a hash handed to an `iterable` parameter is the SAME pointer: the
            // heap header already names which container it is, and that is exactly what the
            // callee's `foreach` reads. Widening the elements first would be both wasteful and
            // wrong — `foreach` over an `iterable` boxes each value as it goes, from the
            // container's own `value_type`, so `array<int>` walks without being rebuilt.
            if dest_ir == IrType::Heap(IrHeapKind::Iterable)
                && *dest_php == PhpType::Iterable
                && matches!(
                    source_ir,
                    IrType::Heap(IrHeapKind::Array | IrHeapKind::Hash | IrHeapKind::Iterable)
                )
            {
                return true;
            }
            source_ir == dest_ir && source_php == dest_php
        }
        _ => false,
    }
}

/// Classifies one source-to-destination transfer without emitting WAT.
///
/// This is the capability contract for call arguments and results. Emission
/// consumes the same classification, preventing the static gate from drifting
/// away from the conversion branches implemented below.
pub(super) fn classify_transfer(
    source_ir: IrType,
    source_php: PhpType,
    dest_ir: IrType,
    dest_php: PhpType,
) -> Result<TransferKind> {
    validate_storage_pair(source_ir, &source_php).map_err(|error| {
        WasmError::Unsupported(format!("invalid source for wasm value transfer: {error}"))
    })?;
    validate_storage_pair(dest_ir, &dest_php).map_err(|error| {
        WasmError::Unsupported(format!(
            "invalid destination for wasm value transfer: {error}"
        ))
    })?;
    let source_php = source_php.codegen_repr();
    let dest_php = dest_php.codegen_repr();
    let source_repr = repr_for_ir(source_ir);
    let dest_repr = repr_for_ir(dest_ir);
    if is_mixed_cell_storage(dest_ir, &dest_php) {
        if is_mixed_cell_storage(source_ir, &source_php) {
            return Ok(TransferKind::Copy);
        }
        mixed_box_tag(&source_php, source_ir)?;
        return Ok(TransferKind::BoxMixed);
    }
    if is_mixed_cell_storage(source_ir, &source_php) {
        let supported = match dest_repr {
            WasmRepr::I64(_) => matches!(dest_php, PhpType::Int | PhpType::Bool),
            WasmRepr::F64(_) => dest_php == PhpType::Float,
            WasmRepr::Str { .. } => dest_php == PhpType::Str,
            WasmRepr::Ptr(_) => matches!(
                dest_ir,
                IrType::Heap(
                    IrHeapKind::Array
                        | IrHeapKind::Hash
                        | IrHeapKind::Object
                        | IrHeapKind::Iterable
                )
            ),
            // A cell and a tagged pair carry the same two facts, so the move exists — what
            // has no tagged spelling is a cell holding a heap POINTER, and the unboxer
            // refuses those tags there rather than flattening them here.
            WasmRepr::Tagged { .. } => dest_php == PhpType::TaggedScalar,
            WasmRepr::Void => false,
        };
        if supported {
            return Ok(TransferKind::UnboxMixed);
        }
        return Err(WasmError::Unsupported(format!(
            "unboxing a Mixed cell to {:?} ({:?}) is not supported on wasm32-wasi",
            dest_ir, dest_php
        )));
    }
    // An `array<T>` handed to an `array<mixed>` destination is a real element-wise conversion.
    if source_ir == IrType::Heap(IrHeapKind::Array)
        && dest_ir == IrType::Heap(IrHeapKind::Array)
        && dest_php == PhpType::Array(Box::new(PhpType::Mixed))
        && source_php != dest_php
    {
        let PhpType::Array(element) = &source_php else {
            return Err(WasmError::Unsupported(format!(
                "widening {:?} to array<mixed> is not supported on wasm32-wasi",
                source_php
            )));
        };
        let (tag, elem_size) = array_widen_shape(element).ok_or_else(|| {
            WasmError::Unsupported(format!(
                "widening an array of {:?} to array<mixed> is not supported on wasm32-wasi",
                element
            ))
        })?;
        return Ok(TransferKind::WidenArrayToMixed { tag, elem_size });
    }
    // `unset($x)` on a heap-typed local stores PHP null over the released value.
    if source_php == PhpType::Void
        && matches!(source_ir, IrType::I64 | IrType::Void)
        && matches!(dest_ir, IrType::Heap(_))
        && matches!(dest_repr, WasmRepr::Ptr(_))
    {
        return Ok(TransferKind::NullPointer);
    }
    // The same `null` reaching a NULLABLE SCALAR — `?int`, `?float`, `?bool` — which this target
    // stores as an inline `{payload, tag}` pair rather than a pointer.
    if source_php == PhpType::Void
        && matches!(source_ir, IrType::I64 | IrType::Void)
        && dest_ir == IrType::TaggedScalar
        && matches!(dest_repr, WasmRepr::Tagged { .. })
    {
        return Ok(TransferKind::TaggedNull);
    }
    // A local that holds `null` at one point and an `int` at another — `$x = null; is_null($x);
    // $x = 99;` — gets ONE one-word slot, and PHP null in a one-word scalar slot IS the
    // `NULL_SENTINEL` bit pattern. Both directions are therefore the same bits, and the EIR's
    // flow-sensitive typing is what says which meaning is live: it types the load after the null
    // store `php=null`, so `is_null` there answers from the type rather than from a comparison.
    //
    // The collision this inherits is the sentinel's own and is shared with the native backend,
    // which answers this example identically: an integer that really equals 9223372036854775806
    // reads as null. That is the documented cost of the one-word encoding, not a new gap.
    if source_ir == IrType::I64
        && dest_ir == IrType::I64
        && matches!(source_php, PhpType::Void | PhpType::Int)
        && matches!(dest_php, PhpType::Void | PhpType::Int)
    {
        return Ok(TransferKind::Copy);
    }
    // An int-typed source reaching a `bool` slot, which `isset(...)` is typed for. Restricted to
    // a genuine `Int` source: `Void` in a one-word slot is the NULL SENTINEL, a nonzero pattern
    // that would normalize to `true` where php's `(bool) null` is false.
    if source_ir == IrType::I64
        && dest_ir == IrType::I64
        && source_php == PhpType::Int
        && matches!(dest_php, PhpType::Bool | PhpType::False)
    {
        return Ok(TransferKind::BoolFromInt);
    }
    // And a concrete scalar into the same nullable slot, which is the other thing a `?int`
    // parameter is ever handed.
    if dest_ir == IrType::TaggedScalar && matches!(dest_repr, WasmRepr::Tagged { .. }) {
        let concrete = match (source_ir, source_php.clone()) {
            (IrType::I64, PhpType::Int) => Some((0, false)),
            (IrType::I64, PhpType::Bool | PhpType::False) => Some((3, false)),
            (IrType::F64, PhpType::Float) => Some((2, true)),
            _ => None,
        };
        if let Some((tag, reinterpret)) = concrete {
            return Ok(TransferKind::TaggedScalarFromConcrete { tag, reinterpret });
        }
    }
    // And the narrowing back out of one, which `f(...$args)` at `f(int $a)` needs. A pair holds
    // ONE payload word, so a string or a container destination has nothing to read and stays
    // refused.
    if source_ir == IrType::TaggedScalar && matches!(source_repr, WasmRepr::Tagged { .. }) {
        let concrete = match (dest_ir, dest_php.clone()) {
            (IrType::I64, PhpType::Int | PhpType::Bool | PhpType::False) => Some(false),
            (IrType::F64, PhpType::Float) => Some(true),
            _ => None,
        };
        if let Some(reinterpret) = concrete {
            return Ok(TransferKind::TaggedScalarToConcrete { reinterpret });
        }
    }
    if reprs_match_for_copy(
        &source_repr,
        source_ir,
        &source_php,
        &dest_repr,
        dest_ir,
        &dest_php,
    ) {
        return Ok(TransferKind::Copy);
    }
    Err(WasmError::Unsupported(format!(
        "unsupported wasm value transfer from {:?} ({:?}/{:?}) to {:?} ({:?}/{:?})",
        source_repr, source_php, source_ir, dest_repr, dest_php, dest_ir
    )))
}

/// Returns the Mixed-cell boxing tag for a concrete source value.
fn mixed_box_tag(source_php: &PhpType, source_ir: IrType) -> Result<i64> {
    let source_php = source_php.codegen_repr();
    match (source_ir, &source_php) {
        (IrType::I64, PhpType::Int) => Ok(0),
        (IrType::I64, PhpType::Bool) => Ok(3),
        (IrType::I64, PhpType::Callable) => Ok(10),
        (IrType::I64 | IrType::Void, PhpType::Void) => Ok(8),
        (IrType::F64, PhpType::Float) => Ok(2),
        (IrType::Str, PhpType::Str) => Ok(1),
        (IrType::Heap(IrHeapKind::Array), PhpType::Array(_)) => Ok(4),
        (IrType::Heap(IrHeapKind::Hash), PhpType::AssocArray { .. }) => Ok(5),
        (IrType::Heap(IrHeapKind::Object), PhpType::Object(_)) => Ok(6),
        (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed) => Err(WasmError::Unsupported(
            "mixed_box_tag called on an already-Mixed value".to_string(),
        )),
        _ => Err(WasmError::Unsupported(format!(
            "boxing {:?}/{:?} into a Mixed cell is not supported on wasm32-wasi",
            source_ir, source_php
        ))),
    }
}

/// Builds a `WasmRepr` from a stack-captured temp name list and the EIR type
/// those temps represent.
fn repr_from_temps(ir: IrType, temps: &[String]) -> WasmRepr {
    match ir {
        IrType::I64 => WasmRepr::I64(temps[0].clone()),
        IrType::F64 => WasmRepr::F64(temps[0].clone()),
        IrType::Heap(_) => WasmRepr::Ptr(temps[0].clone()),
        IrType::Str => WasmRepr::Str {
            ptr: temps[0].clone(),
            len: temps[1].clone(),
        },
        IrType::TaggedScalar => WasmRepr::Tagged {
            payload: temps[0].clone(),
            tag: temps[1].clone(),
        },
        IrType::Void => WasmRepr::Void,
    }
}

/// Pops a value of the given WebAssembly component types from the operand stack
/// into fresh temp locals, returning the temp names in canonical order.
fn pop_stack_to_temps(ctx: &mut FnCtx, val_types: &[ValType]) -> Result<Vec<String>> {
    let mut temps = Vec::with_capacity(val_types.len());
    for ty in val_types {
        temps.push(ctx.fresh_temp(*ty));
    }
    for name in temps.iter().rev() {
        ctx.fb
            .ins(&format!("local.set {}", name), "capture stack component into temp");
    }
    Ok(temps)
}

/// Loads an EIR value into fresh temp locals and returns the temp names in
/// canonical order.
fn load_value_to_temps(ctx: &mut FnCtx, value: ValueId) -> Result<Vec<String>> {
    let repr = ctx.value_repr(value)?.clone();
    let val_types = repr.component_val_types();
    ctx.emit_load_value(value)?;
    pop_stack_to_temps(ctx, &val_types)
}

/// Loads a local slot into fresh temp locals and returns the temp names in
/// canonical order.
fn load_slot_to_temps(ctx: &mut FnCtx, slot: LocalSlotId) -> Result<Vec<String>> {
    let repr = ctx.slot_repr(slot)?.clone();
    let val_types = repr.component_val_types();
    ctx.emit_load_slot(slot)?;
    pop_stack_to_temps(ctx, &val_types)
}

/// Emits the unboxing of a Mixed cell pointer into a concrete destination.
///
/// Uses the existing `__rt_mixed_cast_*` helpers so the unboxed result is owned
/// when needed (strings are persisted; heap pointers are borrowed from the cell).
fn emit_unbox_mixed_to_concrete(
    ctx: &mut FnCtx,
    source_temps: &[String],
    dest_repr: &WasmRepr,
    dest_ir: IrType,
    dest_php: &PhpType,
) -> Result<()> {
    let ptr = &source_temps[0];
    ctx.fb
        .ins(&format!("local.get {}", ptr), "mixed cell to unbox");

    match dest_repr {
        WasmRepr::I64(_) => {
            // The IMPLICIT narrowing, not the explicit `(int)` cast: PHP performs no cast
            // here, so the cast's out-of-range warning would be noise on a silent program.
            let call = if *dest_php == PhpType::Bool {
                "__rt_mixed_cast_bool"
            } else {
                "__rt_mixed_narrow_int"
            };
            ctx.fb.ins(
                &format!("call ${}", call),
                "unbox i64 payload from mixed cell",
            );
        }
        WasmRepr::F64(_) => {
            ctx.fb.ins(
                "call $__rt_mixed_cast_float",
                "unbox f64 payload from mixed cell",
            );
            // The helper answers RAW f64 BITS in an i64 — every caller reinterprets;
            // without this the f64 destination fails module validation.
            ctx.fb
                .ins("f64.reinterpret_i64", "reinterpret cast bits as f64");
        }
        WasmRepr::Str { .. } => {
            ctx.fb.ins(
                "call $__rt_mixed_cast_string",
                "unbox string payload from mixed cell",
            );
            ctx.fb
                .ins("i64.extend_i32_u", "widen string length to i64");
        }
        WasmRepr::Ptr(_) => {
            let accepted_tag_test = match dest_ir {
                IrType::Heap(IrHeapKind::Array) => {
                    "(i64.eq (local.get {tag}) (i64.const 4))"
                }
                IrType::Heap(IrHeapKind::Hash) => {
                    "(i64.eq (local.get {tag}) (i64.const 5))"
                }
                IrType::Heap(IrHeapKind::Object) => {
                    "(i64.eq (local.get {tag}) (i64.const 6))"
                }
                IrType::Heap(IrHeapKind::Iterable) => concat!(
                    "(i32.or ",
                    "(i64.eq (local.get {tag}) (i64.const 4)) ",
                    "(i32.or ",
                    "(i64.eq (local.get {tag}) (i64.const 5)) ",
                    "(i64.eq (local.get {tag}) (i64.const 6))))"
                ),
                other => {
                    return Err(WasmError::Unsupported(format!(
                        "unboxing a Mixed cell to heap storage {:?} is not supported on wasm32-wasi",
                        other
                    )));
                }
            };
            let tag_tmp = ctx.fresh_temp(ValType::I64);
            let lo_tmp = ctx.fresh_temp(ValType::I64);
            let hi_tmp = ctx.fresh_temp(ValType::I64);
            // A ZERO cell pointer means the slot was never written, not that it holds PHP
            // `null` — a stored null is a real cell carrying tag 8. Slots start zeroed, and EIR
            // releases a local's previous value before overwriting it, so the very first write
            // to a Mixed-repr slot unboxes a slot that has never held a cell. Unboxing address
            // zero would read a tag out of the null page and fail the check below, turning
            // ordinary first assignment into a TypeError. Yielding the null pointer instead is
            // exactly what the consumer expects: `__rt_decref_any` treats it as a no-op.
            let cell_tmp = ctx.fresh_temp(ValType::I32);
            ctx.fb
                .ins(&format!("local.set {}", cell_tmp), "save mixed cell pointer");
            ctx.fb
                .ins(&format!("local.get {}", cell_tmp), "mixed cell pointer");
            ctx.fb.ins("i32.eqz", "slot never written?");
            ctx.fb.ins("if (result i32)", "unwritten slot yields the null pointer");
            ctx.fb.ins("i32.const 0", "null heap pointer for an unwritten slot");
            ctx.fb.ins("else", "the slot holds a real mixed cell");
            ctx.fb
                .ins(&format!("local.get {}", cell_tmp), "mixed cell to unbox");
            ctx.fb
                .ins("call $__rt_mixed_unbox", "unbox mixed cell -> (tag, lo, hi)");
            ctx.fb
                .ins(&format!("local.set {}", hi_tmp), "discard mixed high word");
            ctx.fb
                .ins(&format!("local.set {}", lo_tmp), "capture mixed low word");
            ctx.fb
                .ins(&format!("local.set {}", tag_tmp), "capture mixed tag");
            let accepted_tag_test = accepted_tag_test.replace("{tag}", &tag_tmp);
            ctx.fb
                .ins(&accepted_tag_test, "mixed tag belongs to destination kind");
            ctx.fb.ins("i32.eqz", "invert accepted-tag predicate");
            ctx.fb.ins("if", "mixed heap-kind mismatch?");
            if ctx
                .module
                .functions
                .iter()
                .any(|function| function.flags.is_main)
            {
                ctx.fb
                    .ins("i32.const 9", "Mixed heap-kind TypeError diagnostic");
                ctx.fb.ins(
                    "call $__rt_fail",
                    "raise deterministic PHP TypeError for the mismatch",
                );
                ctx.fb.ins(
                    "unreachable",
                    "elephc-trap:post-noreturn:mixed-heap-kind-mismatch runtime TypeError helper does not return",
                );
            } else {
                ctx.fb.ins(
                    "unreachable",
                    "elephc-trap:non-public:reactor-mixed-heap-mismatch import-free reactors are outside the public command surface",
                );
            }
            ctx.fb.ins("end", "end mixed heap-kind validation");
            ctx.fb
                .ins(&format!("local.get {}", lo_tmp), "load mixed heap pointer");
            ctx.fb
                .ins("i32.wrap_i64", "narrow payload pointer to i32");
            ctx.fb.ins("end", "end unwritten-slot guard");
        }
        // A `?int` slot (the only thing a tagged pair is, today) takes a boxed cell by PHP's own
        // rule for a typed destination, not by copying the cell's words. `null` stays null and an
        // int stays an int. Under the weak mode every other scalar coerces exactly as it does at
        // a declared `int` return (a bool becomes 0/1, an integral float or a wholly numeric
        // string its int value, a fractional float truncates after php's deprecation) and
        // anything with no int value -- an array, an object, a non-numeric string, a float out of
        // range -- raises a `TypeError` that a `catch` can receive. Under `strict_types` only the
        // int and the null pass.
        //
        // The pair is what `$this->v = $this->v + 1;` needs too: checked arithmetic widens to a
        // cell on the chance of an overflow, and the slot takes the pair back. An int cell passes
        // through unchanged, so that path is exactly what it was.
        //
        // A module with no `main` is an import-free reactor, which has no runtime to carry the
        // coercion or the diagnostic; it keeps the verbatim move, as every other reactor-only
        // path here does.
        WasmRepr::Tagged { .. } => {
            let has_main = ctx.module.functions.iter().any(|function| function.flags.is_main);
            let tag = ctx.fresh_temp(ValType::I64);
            let lo = ctx.fresh_temp(ValType::I64);
            if !has_main {
                let hi = ctx.fresh_temp(ValType::I64);
                ctx.fb
                    .ins("call $__rt_mixed_unbox", "read (tag, lo, hi) out of the cell");
                ctx.fb.ins(&format!("local.set {}", hi), "a tagged pair has no high word");
                ctx.fb.ins(&format!("local.set {}", lo), "the payload");
                ctx.fb.ins(&format!("local.set {}", tag), "the tag");
                ctx.fb.ins(&format!("local.get {}", lo), "payload first, as every tagged producer pushes it");
                ctx.fb
                    .ins(&format!("(i32.wrap_i64 (local.get {}))", tag), "then the tag");
            } else {
                let ok = ctx.fresh_temp(ValType::I32);
                let out_tag = ctx.fresh_temp(ValType::I32);
                ctx.fb.ins(
                    &format!("i32.const {}", i32::from(ctx.module.strict_types)),
                    "strict_types: only an int or a null is accepted",
                );
                ctx.fb.ins(
                    "call $__rt_mixed_to_tagged_int",
                    "PHP's coercion of a cell into a ?int destination",
                );
                ctx.fb.ins(&format!("local.set {}", out_tag), "the destination's tag");
                ctx.fb.ins(&format!("local.set {}", lo), "the destination's payload");
                ctx.fb.ins(&format!("local.set {}", ok), "did the value convert?");
                ctx.fb.ins(&format!("local.get {}", ok), "did the value convert?");
                ctx.fb.ins("i32.eqz", "no int value exists for this cell");
                ctx.fb.ins("if", "a cell that cannot become an int or null");
                super::inst::emit_runtime_failure(
                    ctx,
                    9,
                    "a value with no int or null form reached a ?int destination",
                );
                ctx.fb.ins("end", "end of the TypeError arm");
                ctx.fb.ins(&format!("local.get {}", lo), "payload first, as every tagged producer pushes it");
                ctx.fb.ins(&format!("local.get {}", out_tag), "then the tag");
            }
        }
        WasmRepr::Void => {
            return Err(WasmError::Unsupported(format!(
                "unboxing a Mixed cell to {:?} is not supported on wasm32-wasi",
                dest_repr
            )));
        }
    }

    Ok(())
}

/// Emits the local.get sequence that pushes the destination representation of
/// `source_temps` onto the operand stack.
///
/// For identical representations this is a plain component-wise copy. For a
/// Mixed-cell destination this boxes (or moves) the value via
/// `__rt_mixed_from_value`.  Unsupported conversions return `WasmError`.
/// Returns the local holding a heap value this conversion ALLOCATED, with its EIR kind, when it
/// allocated one.
/// Callers that hand the value to an owner (a value, a slot) ignore it; a call argument, which
/// the callee only borrows, must release it after the call.
fn convert_temps_to_dest(
    ctx: &mut FnCtx,
    source_temps: &[String],
    source_repr: &WasmRepr,
    source_php: PhpType,
    source_ir: IrType,
    dest_repr: &WasmRepr,
    dest_ir: IrType,
    dest_php: PhpType,
) -> Result<Option<(String, IrType)>> {
    match classify_transfer(source_ir, source_php.clone(), dest_ir, dest_php.clone())? {
        TransferKind::Copy => {
            for name in source_temps {
                ctx.fb
                    .ins(&format!("local.get {}", name), "copy value component");
            }
            Ok(None)
        }
        // handled by the caller, which knows whether the destination owns the value
        TransferKind::BoxMixed => emit_box_temps_into_mixed(
            ctx,
            source_temps,
            source_repr,
            source_php,
            source_ir,
        ),
        TransferKind::WidenArrayToMixed { tag, elem_size } => {
            ctx.fb.ins(
                &format!("local.get {}", source_temps[0]),
                "source array pointer",
            );
            ctx.fb.ins(&format!("i64.const {tag}"), "cell tag for every element");
            ctx.fb
                .ins(&format!("i64.const {elem_size}"), "source slot stride");
            ctx.fb.ins(
                "call $__rt_array_widen_to_mixed",
                "copy into a fresh owned mixed-cell array",
            );
            let widened = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(
                &format!("local.tee {}", widened),
                "remember the widened array: FRESH, so it is already the one reference the destination takes",
            );
            let _ = widened;
            Ok(None)
        }
        TransferKind::NullPointer => {
            ctx.fb
                .ins("i32.const 0", "PHP null is the zero pointer in a heap slot");
            Ok(None)
        }
        TransferKind::BoolFromInt => {
            // Read from the temps like every other arm rather than assuming the stack.
            ctx.fb
                .ins(&format!("local.get {}", source_temps[0]), "the int source");
            ctx.fb.ins("i64.const 0", "php reads any nonzero int as true");
            ctx.fb.ins("i64.ne", "normalize to 0 or 1");
            ctx.fb.ins("i64.extend_i32_u", "bool i32 -> i64");
            Ok(None)
        }
        TransferKind::TaggedNull => {
            // The pair is pushed payload-first, matching every other tagged producer; tag 8 is
            // what `is_null` and the coercion sites already read as null.
            ctx.fb
                .ins("i64.const 0", "tagged null carries no payload");
            ctx.fb.ins("i32.const 8", "Mixed null tag");
            Ok(None)
        }
        TransferKind::TaggedScalarToConcrete { reinterpret } => {
            // The payload is the value; the tag is the word the callee's shape has no place
            // for. Read from the temps like every other arm rather than assuming the stack.
            ctx.fb.ins(
                &format!("local.get {}", source_temps[0]),
                "the tagged payload is the value",
            );
            if reinterpret {
                ctx.fb
                    .ins("f64.reinterpret_i64", "a float's payload carries its bits");
            }
            Ok(None)
        }
        TransferKind::TaggedScalarFromConcrete { tag, reinterpret } => {
            // Like every arm here, the source is READ from its temps rather than assumed to be
            // on the stack. It becomes the payload, then the tag is appended so the pair reads
            // payload-first like every other tagged producer.
            ctx.fb.ins(
                &format!("local.get {}", source_temps[0]),
                "the value becomes the tagged payload",
            );
            if reinterpret {
                ctx.fb
                    .ins("i64.reinterpret_f64", "a tagged payload carries the float's bits");
            }
            ctx.fb.ins(&format!("i32.const {tag}"), "the value's own Mixed tag");
            Ok(None)
        }
        TransferKind::UnboxMixed => {
            emit_unbox_mixed_to_concrete(ctx, source_temps, dest_repr, dest_ir, &dest_php)?;
            Ok(None)
        }
    }
}

/// Boxes the value described by `source_temps` into a fresh owned Mixed cell via
/// `__rt_mixed_from_value`, leaving the i32 cell pointer on the operand stack.
///
/// Also parks the pointer in a local and returns its name. The cell is an artefact of THIS
/// backend — EIR emits no boxing instruction and therefore no matching release — so whoever
/// creates it owns it. A call argument is borrowed by the callee, so the call site is the
/// only place that can free it.
fn emit_box_temps_into_mixed(
    ctx: &mut FnCtx,
    source_temps: &[String],
    source_repr: &WasmRepr,
    source_php: PhpType,
    source_ir: IrType,
) -> Result<Option<(String, IrType)>> {
    let tag = mixed_box_tag(&source_php, source_ir)?;
    ctx.fb
        .ins(&format!("i64.const {}", tag), "mixed boxing tag");

    match source_repr {
        WasmRepr::I64(_) => {
            ctx.fb.ins(
                &format!("local.get {}", source_temps[0]),
                "scalar payload lo",
            );
            ctx.fb.ins("i64.const 0", "scalar payload hi unused");
        }
        WasmRepr::F64(_) => {
            ctx.fb.ins(
                &format!("local.get {}", source_temps[0]),
                "float payload",
            );
            ctx.fb.ins("i64.reinterpret_f64", "float bits -> i64 lo");
            ctx.fb.ins("i64.const 0", "float payload hi unused");
        }
        WasmRepr::Str { .. } => {
            let ptr = &source_temps[0];
            let len = &source_temps[1];
            ctx.fb
                .ins(&format!("local.get {}", ptr), "string pointer lo");
            ctx.fb.ins("i64.extend_i32_u", "ptr -> i64 lo");
            ctx.fb
                .ins(&format!("local.get {}", len), "string length hi");
        }
        WasmRepr::Ptr(_) => {
            ctx.fb
                .ins(&format!("local.get {}", source_temps[0]), "heap pointer lo");
            ctx.fb.ins("i64.extend_i32_u", "ptr -> i64 lo");
            ctx.fb.ins("i64.const 0", "heap payload hi unused");
        }
        WasmRepr::Tagged { .. } => {
            return Err(WasmError::Unsupported(
                "boxing a tagged scalar into a Mixed cell is not supported on wasm32-wasi"
                    .to_string(),
            ));
        }
        WasmRepr::Void => {
            ctx.fb.ins(
                &format!("i64.const {}", VOID_SENTINEL),
                "void/null sentinel",
            );
            ctx.fb.ins("i64.const 0", "null hi unused");
        }
    }

    ctx.fb
        .ins("call $__rt_mixed_from_value", "box into owned mixed cell");
    let cell = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(
        &format!("local.tee {}", cell),
        "remember the boxed cell so its creator can free it",
    );
    Ok(Some((cell, IrType::Heap(IrHeapKind::Mixed))))
}

/// Stores a stack-captured value into a destination EIR value, applying any
/// required representation conversion.
pub(super) fn emit_store_temps_into_value(
    ctx: &mut FnCtx,
    source_temps: &[String],
    source_repr: &WasmRepr,
    source_php: PhpType,
    source_ir: IrType,
    dest_value: ValueId,
) -> Result<()> {
    let dest_repr = ctx.value_repr(dest_value)?.clone();
    let (dest_php, dest_ir) = ctx
        .function
        .value(dest_value)
        .map(|v| (v.php_type.codegen_repr(), v.ir_type))
        .ok_or_else(|| {
            WasmError::Unsupported(format!("destination value {:?} missing", dest_value))
        })?;

    convert_temps_to_dest(
        ctx,
        source_temps,
        source_repr,
        source_php,
        source_ir,
        &dest_repr,
        dest_ir,
        dest_php,
    )?;

    for local_ref in dest_repr.local_refs().iter().rev() {
        ctx.fb
            .ins(&format!("local.set {}", local_ref), "store destination value component");
    }
    Ok(())
}

/// Stores a stack-captured value into a local slot, applying any required
/// representation conversion.
pub(super) fn emit_store_temps_into_slot(
    ctx: &mut FnCtx,
    source_temps: &[String],
    source_repr: &WasmRepr,
    source_php: PhpType,
    source_ir: IrType,
    dest_slot: LocalSlotId,
) -> Result<()> {
    let dest_repr = ctx.slot_repr(dest_slot)?.clone();
    let (dest_php, dest_ir) = ctx
        .function
        .locals
        .get(dest_slot.as_raw() as usize)
        .map(|local| (local.php_type.codegen_repr(), local.ir_type))
        .ok_or_else(|| {
            WasmError::Unsupported(format!("destination slot {:?} missing", dest_slot))
        })?;

    convert_temps_to_dest(
        ctx,
        source_temps,
        source_repr,
        source_php,
        source_ir,
        &dest_repr,
        dest_ir,
        dest_php,
    )?;

    for local_ref in dest_repr.local_refs().iter().rev() {
        ctx.fb
            .ins(&format!("local.set {}", local_ref), "store destination slot component");
    }
    Ok(())
}

/// Builds a placeholder `WasmRepr` for an EIR type.
///
/// Only the variant shape matters for conversion decisions; the local names are
/// never emitted.
fn repr_for_ir(ir: IrType) -> WasmRepr {
    match ir {
        IrType::I64 => WasmRepr::I64(String::new()),
        IrType::F64 => WasmRepr::F64(String::new()),
        IrType::Heap(_) => WasmRepr::Ptr(String::new()),
        IrType::Str => WasmRepr::Str {
            ptr: String::new(),
            len: String::new(),
        },
        IrType::TaggedScalar => WasmRepr::Tagged {
            payload: String::new(),
            tag: String::new(),
        },
        IrType::Void => WasmRepr::Void,
    }
}

/// Unboxes a Mixed cell onto the stack as `dest_php`'s concrete storage.
///
/// The property store needs the same narrowing a local load performs, so both go through
/// `emit_unbox_mixed_to_concrete` rather than growing a second copy of the tag rules.
pub(super) fn emit_narrow_mixed_into(
    ctx: &mut FnCtx,
    value: ValueId,
    dest_php: &PhpType,
) -> Result<()> {
    let temps = load_value_to_temps(ctx, value)?;
    let dest_ir = IrType::from_php(&dest_php.codegen_repr());
    if emit_typed_scalar_coercion(ctx, &temps[0], dest_php) {
        return Ok(());
    }
    emit_unbox_mixed_to_concrete(ctx, &temps, &repr_for_ir(dest_ir), dest_ir, dest_php)
}

/// Coerces the Mixed cell in `cell` into a plain `int`, `float` or `bool` DECLARED destination.
///
/// A typed property, static property or user-function parameter is not a local load: php coerces
/// a scalar into it by the weak-mode rules (or, under `strict_types`, accepts only the exact
/// type) and raises a `TypeError` for everything else -- an array, an object, a null, a
/// non-numeric string. Narrowing silently, as a local load does, made `$o->n = [1, 2]` store 2
/// and a `null` argument arrive as 0. Leaves the destination word on the stack (an f64 for a
/// float) and returns true; returns false, emitting nothing, for a destination or module this
/// does not cover -- a reactor module has no runtime to carry the check.
fn emit_typed_scalar_coercion(ctx: &mut FnCtx, cell: &str, dest_php: &PhpType) -> bool {
    let (helper, kind) = match dest_php {
        PhpType::Int => ("__rt_mixed_to_typed_int", "int"),
        PhpType::Float => ("__rt_mixed_to_typed_float", "float"),
        PhpType::Bool => ("__rt_mixed_to_typed_bool", "bool"),
        _ => return false,
    };
    if !ctx.module.functions.iter().any(|function| function.flags.is_main) {
        return false;
    }
    let ok = ctx.fresh_temp(ValType::I32);
    let word = ctx.fresh_temp(ValType::I64);
    ctx.fb.ins(&format!("local.get {}", cell), "mixed cell to coerce");
    ctx.fb.ins(
        &format!("i32.const {}", i32::from(ctx.module.strict_types)),
        "strict_types: no coercion, only the exact type",
    );
    ctx.fb.ins(
        &format!("call ${helper}"),
        &format!("PHP's coercion of a cell into a typed {kind} destination"),
    );
    ctx.fb.ins(&format!("local.set {}", word), "the coerced word");
    ctx.fb.ins(&format!("local.set {}", ok), "did the value convert?");
    ctx.fb.ins(&format!("local.get {}", ok), "did the value convert?");
    ctx.fb.ins("i32.eqz", "no form of this type exists for the cell");
    ctx.fb.ins("if", "a cell that cannot become the declared type");
    super::inst::emit_runtime_failure(
        ctx,
        9,
        "a value of the wrong type reached a typed scalar destination",
    );
    ctx.fb.ins("end", "end of the TypeError arm");
    ctx.fb.ins(&format!("local.get {}", word), "the coerced value");
    if dest_php == &PhpType::Float {
        ctx.fb
            .ins("f64.reinterpret_i64", "the helper answers raw f64 bits");
    }
    true
}

/// Reports whether a value stored as `source` already IS a value stored as `dest`.
///
/// Asks the same classifier the transfer paths use rather than comparing types, so "an Int and
/// a Bool share one i64" and "a Union collapses to Mixed" stay decided in one place.
pub(super) fn storage_shapes_agree(
    source_ir: IrType,
    source_php: PhpType,
    dest_ir: IrType,
    dest_php: PhpType,
) -> Result<bool> {
    Ok(matches!(
        classify_transfer(source_ir, source_php, dest_ir, dest_php)?,
        TransferKind::Copy
    ))
}

/// Converts a value ALREADY on the operand stack from one storage shape into another.
///
/// The transfer paths all start from a value or a slot they can re-read. A static slot is
/// neither — it is raw memory the caller has just loaded at the slot's declared width — so
/// this takes the components back off the stack and runs them through the same conversion
/// table.
///
/// Returns the local holding a heap value the conversion ALLOCATED, when it allocated one; a
/// caller storing into an owner ignores it, and a caller that only borrows must release it.
pub(super) fn emit_convert_stack_value(
    ctx: &mut FnCtx,
    source_ir: IrType,
    source_php: PhpType,
    dest_ir: IrType,
    dest_php: PhpType,
) -> Result<Option<(String, IrType)>> {
    if storage_shapes_agree(source_ir, source_php.clone(), dest_ir, dest_php.clone())? {
        return Ok(None);
    }
    let source_repr = repr_for_ir(source_ir);
    let dest_repr = repr_for_ir(dest_ir);
    let temps = pop_stack_to_temps(ctx, &source_repr.component_val_types())?;
    convert_temps_to_dest(
        ctx,
        &temps,
        &source_repr,
        source_php,
        source_ir,
        &dest_repr,
        dest_ir,
        dest_php,
    )
}

/// Loads a call argument and pushes it in the callee parameter's representation.
///
/// Applies concrete-to-Mixed boxing or Mixed-to-concrete unboxing as needed so
/// the operand stack matches the callee's declared parameter signature.
///
/// Returns the local holding a heap value the conversion allocated, with its EIR kind, if any.
/// The callee BORROWS its
/// parameter (`own=maybe_owned`, and the EIR release stays with the caller's original value), so
/// that cell has no other owner and the call site must free it once the call returns.
pub(super) fn emit_push_call_argument(
    ctx: &mut FnCtx,
    arg: ValueId,
    param_ir: IrType,
    param_php: PhpType,
) -> Result<Option<(String, IrType)>> {
    push_call_argument(ctx, arg, param_ir, param_php, true)
}

/// `emit_push_call_argument` for a BUILTIN's parameter, which keeps the silent narrowing: php's
/// rules for an internal function differ (a `null` is deprecated, not a `TypeError`), and the
/// builtin lowerings that need the exact text carry their own coercion.
pub(super) fn emit_push_builtin_argument(
    ctx: &mut FnCtx,
    arg: ValueId,
    param_ir: IrType,
    param_php: PhpType,
) -> Result<Option<(String, IrType)>> {
    push_call_argument(ctx, arg, param_ir, param_php, false)
}

/// Pushes one call argument onto the stack converted to the parameter's storage `param_ir` and
/// PHP type, answering the temporary the conversion allocated (to be released after the call)
/// when there is one. `typed_destination` says the parameter has a declared type.
fn push_call_argument(
    ctx: &mut FnCtx,
    arg: ValueId,
    param_ir: IrType,
    param_php: PhpType,
    typed_destination: bool,
) -> Result<Option<(String, IrType)>> {
    let source_repr = ctx.value_repr(arg)?.clone();
    let (source_php, source_ir) = ctx
        .function
        .value(arg)
        .map(|v| (v.php_type.codegen_repr(), v.ir_type))
        .ok_or_else(|| WasmError::Unsupported(format!("call arg {:?} missing", arg)))?;
    let temps = load_value_to_temps(ctx, arg)?;
    if typed_destination
        && source_ir == IrType::Heap(IrHeapKind::Mixed)
        && source_php == PhpType::Mixed
        && emit_typed_scalar_coercion(ctx, &temps[0], &param_php)
    {
        return Ok(None);
    }
    let dest_repr = repr_for_ir(param_ir);
    convert_temps_to_dest(
        ctx,
        &temps,
        &source_repr,
        source_php,
        source_ir,
        &dest_repr,
        param_ir,
        param_php,
    )
}

/// Pops a value of the given EIR representation from the operand stack and
/// stores it into `dest_value`, applying any required conversion.
pub(super) fn emit_store_stack_value_into_value(
    ctx: &mut FnCtx,
    source_ir: IrType,
    source_php: PhpType,
    dest_value: ValueId,
) -> Result<()> {
    let val_types = WasmRepr::val_types(source_ir);
    let temps = pop_stack_to_temps(ctx, &val_types)?;
    let source_repr = repr_from_temps(source_ir, &temps);
    emit_store_temps_into_value(ctx, &temps, &source_repr, source_php, source_ir, dest_value)
}

/// Pops a value of the given EIR representation from the operand stack and
/// stores it into `dest_slot`, applying any required conversion.
pub(super) fn emit_store_stack_value_into_slot(
    ctx: &mut FnCtx,
    source_ir: IrType,
    source_php: PhpType,
    dest_slot: LocalSlotId,
) -> Result<()> {
    let val_types = WasmRepr::val_types(source_ir);
    let temps = pop_stack_to_temps(ctx, &val_types)?;
    let source_repr = repr_from_temps(source_ir, &temps);
    emit_store_temps_into_slot(ctx, &temps, &source_repr, source_php, source_ir, dest_slot)
}

/// Transfers `src_value` into `dest_value`, loading the source into temps first
/// so the move is safe even when the values share a local.
pub(super) fn emit_transfer_value(
    ctx: &mut FnCtx,
    src_value: ValueId,
    dest_value: ValueId,
) -> Result<()> {
    let source_repr = ctx.value_repr(src_value)?.clone();
    let (source_php, source_ir) = ctx
        .function
        .value(src_value)
        .map(|v| (v.php_type.codegen_repr(), v.ir_type))
        .ok_or_else(|| WasmError::Unsupported(format!("source value {:?} missing", src_value)))?;
    let temps = load_value_to_temps(ctx, src_value)?;
    emit_store_temps_into_value(ctx, &temps, &source_repr, source_php, source_ir, dest_value)
}

/// Transfers `src_value` into `dest_slot`, loading the source into temps first.
pub(super) fn emit_transfer_to_slot(
    ctx: &mut FnCtx,
    src_value: ValueId,
    dest_slot: LocalSlotId,
) -> Result<()> {
    let source_repr = ctx.value_repr(src_value)?.clone();
    let (source_php, source_ir) = ctx
        .function
        .value(src_value)
        .map(|v| (v.php_type.codegen_repr(), v.ir_type))
        .ok_or_else(|| WasmError::Unsupported(format!("source value {:?} missing", src_value)))?;
    let temps = load_value_to_temps(ctx, src_value)?;
    emit_store_temps_into_slot(ctx, &temps, &source_repr, source_php, source_ir, dest_slot)
}

/// Transfers the contents of `src_slot` into `dest_value`.
pub(super) fn emit_transfer_from_slot(
    ctx: &mut FnCtx,
    src_slot: LocalSlotId,
    dest_value: ValueId,
) -> Result<()> {
    let source_repr = ctx.slot_repr(src_slot)?.clone();
    let (source_php, source_ir) = ctx
        .function
        .locals
        .get(src_slot.as_raw() as usize)
        .map(|local| (local.php_type.codegen_repr(), local.ir_type))
        .ok_or_else(|| WasmError::Unsupported(format!("source slot {:?} missing", src_slot)))?;
    let temps = load_slot_to_temps(ctx, src_slot)?;
    emit_store_temps_into_value(ctx, &temps, &source_repr, source_php, source_ir, dest_value)
}

/// Materializes block arguments for a branch, loading every source into temps
/// before storing any destination so parallel moves remain safe.
pub(super) fn emit_transfer_block_args(
    ctx: &mut FnCtx,
    target: crate::ir::BlockId,
    args: &[ValueId],
) -> Result<()> {
    let target_block = ctx
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

    let mut arg_temps: Vec<Vec<String>> = Vec::with_capacity(args.len());
    let mut arg_infos: Vec<(WasmRepr, PhpType, IrType)> = Vec::with_capacity(args.len());
    for &arg in args {
        let repr = ctx.value_repr(arg)?.clone();
        let val = ctx
            .function
            .value(arg)
            .ok_or_else(|| WasmError::Unsupported(format!("branch arg {:?} missing", arg)))?;
        let source_php = val.php_type.codegen_repr();
        let source_ir = val.ir_type;
        let temps = load_value_to_temps(ctx, arg)?;
        arg_temps.push(temps);
        arg_infos.push((repr, source_php, source_ir));
    }

    for (i, &param) in params.iter().enumerate() {
        let temps = &arg_temps[i];
        let (source_repr, source_php, source_ir) = &arg_infos[i];
        emit_store_temps_into_value(
            ctx,
            temps,
            source_repr,
            source_php.clone(),
            *source_ir,
            param,
        )?;
    }

    Ok(())
}

/// Stores a function-call result into the instruction's result value.
///
/// For a void callee this materializes Elephc's null sentinel and boxes it when
/// the destination is a Mixed cell. Concrete return values are popped from the
/// operand stack and converted as needed.
pub(super) fn emit_store_call_result(
    ctx: &mut FnCtx,
    inst: &crate::ir::Instruction,
    return_type: IrType,
    return_php_type: PhpType,
) -> Result<()> {
    if let Some(result) = inst.result {
        if return_type == IrType::Void {
            let sentinel = ctx.fresh_temp(ValType::I64);
            ctx.fb.ins(
                &format!("i64.const {}", VOID_SENTINEL),
                "Elephc null sentinel for void callee result",
            );
            ctx.fb
                .ins(&format!("local.set {}", sentinel), "capture void sentinel");
            let source_repr = WasmRepr::I64(sentinel.clone());
            // The source is described as `I64`, not `Void`: nothing came back from the callee,
            // but what this transfer actually carries is the i64 sentinel just materialized.
            // Describing it as `Void` would classify a `WasmRepr::Void` (zero locals) against
            // the destination and reject the very store this branch exists to perform.
            emit_store_temps_into_value(
                ctx,
                &[sentinel],
                &source_repr,
                PhpType::Void,
                IrType::I64,
                result,
            )?;
        } else {
            emit_store_stack_value_into_value(ctx, return_type, return_php_type, result)?;
        }
    }
    Ok(())
}

/// Whether the module can unbox a Mixed cell into a `?int` destination or store one into a typed
/// `int`/`float`/`bool` property, the places the coercions above can raise.
///
/// The raise is a catchable `TypeError`, whose message text the planner lays out only when the
/// module is known to need it. A tagged destination is a `?int` slot of a function, a parameter,
/// a return or a class property, and a cell is any Mixed value; a module with both could join
/// them, so this is deliberately a coarse "both exist" test -- a false positive costs a few bytes
/// of message text, a false negative would turn the catchable error into an uncaught fatal.
pub(super) fn module_may_unbox_mixed_into_tagged(module: &crate::ir::Module) -> bool {
    let functions = || {
        module
            .functions
            .iter()
            .chain(module.class_methods.iter())
            .chain(module.closures.iter())
    };
    let has_scalar_slot = module.class_infos.values().any(|class| {
        class
            .properties
            .iter()
            .chain(class.static_properties.iter())
            .any(|(_, php_type)| {
                matches!(php_type.codegen_repr(), PhpType::Int | PhpType::Float | PhpType::Bool)
            })
    });
    let has_tagged = has_scalar_slot
        || functions().any(|function| {
            function.params.iter().any(|param| {
                matches!(param.php_type.codegen_repr(), PhpType::Int | PhpType::Float | PhpType::Bool)
            })
        })
        || functions().any(|function| {
        function.return_type == IrType::TaggedScalar
            || function.params.iter().any(|param| param.ir_type == IrType::TaggedScalar)
            || function.locals.iter().any(|local| local.ir_type == IrType::TaggedScalar)
            || function.values.iter().any(|value| value.ir_type == IrType::TaggedScalar)
    }) || module.class_infos.values().any(|class| {
        class
            .properties
            .iter()
            .chain(class.static_properties.iter())
            .any(|(_, php_type)| php_type.codegen_repr() == PhpType::TaggedScalar)
    });
    has_tagged
        && functions().any(|function| {
            function
                .values
                .iter()
                .any(|value| value.ir_type == IrType::Heap(IrHeapKind::Mixed))
        })
}
