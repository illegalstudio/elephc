//! Purpose:
//! Implements fail-closed strict PHP equality for exact WASM value shapes and
//! the length-delimited byte-string helper used by `===` and `!==`.
//!
//! Called from:
//! - `crate::codegen_wasm::inst::lower_instruction()` for strict comparisons.
//! - `crate::codegen_wasm::plan::plan_module()` to register the string runtime.
//! - `crate::codegen_wasm::capability` to share the admitted value families.
//!
//! Key details:
//! - Type identity is based on exact PHP/EIR metadata, never `codegen_repr()`;
//!   resources must not collapse into integers for strict comparison.
//! - Strings are compared by length and raw bytes, including embedded NUL and
//!   invalid UTF-8; operands remain borrowed.
//! - A runtime-tagged `Mixed` cell is comparable against a CONCRETE value: the
//!   cell's tag decides the type, and the concrete side is never an array, so
//!   this never needs PHP's deep element-wise array identity. Two Mixed cells
//!   are refused for exactly that reason.
//! - Unions, tagged scalars, containers, callables, resources, pointers, and
//!   packed values remain outside this deliberately narrow batch.

use super::context::{FnCtx, Result};
use super::inst::{operand, store_result};
use super::wat::{ValType, WatModule};
use super::WasmError;
use crate::ir::{Instruction, IrHeapKind, IrType, Op, Ownership};
use crate::types::PhpType;

/// Exact value families whose PHP strict identity is implemented by WASM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StrictValueKind {
    Int,
    Bool,
    Null,
    Float,
    Str,
    Object,
    /// A runtime-tagged `Mixed` cell, comparable against a CONCRETE side only.
    ///
    /// A builtin whose PHP result is `int|false` — `strpos` most visibly — hands back one of
    /// these, and the whole point of the idiom `strpos($h, $n) === false` is that it separates a
    /// match at offset 0 from a miss. Answering it needs the cell's runtime tag, not its storage.
    MixedCell,
    /// An inline `{payload, tag}` pair — this target's `?int`, the ONLY union `codegen_repr`
    /// folds to `TaggedScalar` (`nullable_int_union_members` admits `int` and `null` and nothing
    /// else). Its tag is therefore 0 or 8, which is what makes every pair below decidable: it can
    /// never hold a string, an object, a float or a bool, so those comparisons are constant
    /// FALSE rather than unknown.
    Tagged,
}

/// The runtime tag an inline `{payload, tag}` pair carries when it holds null.
///
/// Shared with `TransferKind::TaggedNull` and with the Mixed cell tags — a divergence would make
/// `$x === null` answer from a tag nothing writes.
const TAGGED_NULL: i32 = 8;

/// The runtime tag that pair carries when it holds an int.
const TAGGED_INT: i32 = 0;

/// Returns the runtime Mixed tag a concrete strict kind boxes under.
///
/// These mirror `inst::lower_mixed_box` exactly; a divergence would make `===` compare a value
/// against the wrong tag and answer false where PHP answers true.
fn mixed_tag_for(kind: StrictValueKind) -> Option<i64> {
    Some(match kind {
        StrictValueKind::Int => 0,
        StrictValueKind::Str => 1,
        StrictValueKind::Float => 2,
        StrictValueKind::Bool => 3,
        StrictValueKind::Object => 6,
        StrictValueKind::Null => 8,
        StrictValueKind::MixedCell | StrictValueKind::Tagged => return None,
    })
}

/// Returns whether a strict comparison of these two kinds is lowered.
///
/// A `Mixed` cell is comparable against any concrete kind, because a tag mismatch settles the
/// answer and the concrete side is never an array — so this never needs PHP's deep element-wise
/// array identity. Two Mixed cells are NOT admitted for exactly that reason: both could be
/// arrays, and answering that correctly is a different problem.
/// A `?int` pair is comparable against anything EXCEPT a runtime-tagged cell: the cell could
/// hold an array, and while a `?int` never can — which settles the answer to false — reaching
/// that conclusion needs the cell's tag at runtime, which is the mixed/concrete path rather than
/// this one. Refused rather than routed there, since the tagged side's own tag is dynamic too.
pub(super) fn strict_pair_is_supported(lhs: StrictValueKind, rhs: StrictValueKind) -> bool {
    !((lhs == StrictValueKind::Tagged && rhs == StrictValueKind::MixedCell)
        || (lhs == StrictValueKind::MixedCell && rhs == StrictValueKind::Tagged))
}

/// Whether a PHP type is exactly the `int|null` an inline tagged pair carries.
///
/// `codegen_repr` folds only that union to `TaggedScalar`, but reading the repr alone would also
/// admit whatever else ever folds there. Checking the members keeps the identity exact — the same
/// reason this module never classifies by `codegen_repr()` elsewhere.
fn is_nullable_int(php_type: &PhpType) -> bool {
    match php_type {
        PhpType::TaggedScalar => true,
        PhpType::Union(members) => members
            .iter()
            .all(|member| matches!(member, PhpType::Int | PhpType::Void | PhpType::Never)),
        _ => false,
    }
}

/// Classifies one exact EIR/PHP/ownership shape for strict comparison.
pub(super) fn classify_strict_value(
    ir_type: IrType,
    php_type: &PhpType,
    ownership: Ownership,
) -> Option<StrictValueKind> {
    match (ir_type, php_type, ownership) {
        (IrType::I64, PhpType::Int, Ownership::NonHeap) => Some(StrictValueKind::Int),
        (
            IrType::I64,
            PhpType::Bool | PhpType::False,
            Ownership::NonHeap,
        ) => Some(StrictValueKind::Bool),
        (IrType::I64, PhpType::Void, Ownership::NonHeap) => Some(StrictValueKind::Null),
        (IrType::F64, PhpType::Float, Ownership::NonHeap) => Some(StrictValueKind::Float),
        (
            IrType::Str,
            PhpType::Str,
            Ownership::Owned
            | Ownership::Borrowed
            | Ownership::MaybeOwned
            | Ownership::Persistent,
        ) => Some(StrictValueKind::Str),
        (
            IrType::Heap(IrHeapKind::Object),
            PhpType::Object(_),
            Ownership::Owned
            | Ownership::Borrowed
            | Ownership::MaybeOwned
            | Ownership::Persistent,
        ) => Some(StrictValueKind::Object),
        (
            IrType::Heap(IrHeapKind::Mixed),
            _,
            Ownership::Owned
            | Ownership::Borrowed
            | Ownership::MaybeOwned
            | Ownership::Persistent,
        ) => Some(StrictValueKind::MixedCell),
        (IrType::TaggedScalar, php_type, Ownership::NonHeap) if is_nullable_int(php_type) => {
            Some(StrictValueKind::Tagged)
        }
        _ => None,
    }
}

/// Lowers exact strict equality or inequality without PHP coercion.
pub(super) fn lower_strict_compare(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let lhs = operand(inst, 0)?;
    let rhs = operand(inst, 1)?;
    let lhs_value = ctx
        .function
        .value(lhs)
        .ok_or_else(|| WasmError::Unsupported(format!("missing strict lhs {:?}", lhs)))?;
    let rhs_value = ctx
        .function
        .value(rhs)
        .ok_or_else(|| WasmError::Unsupported(format!("missing strict rhs {:?}", rhs)))?;
    let lhs_kind = classify_strict_value(
        lhs_value.ir_type,
        &lhs_value.php_type,
        lhs_value.ownership,
    )
    .ok_or_else(|| {
        WasmError::Unsupported(format!(
            "strict lhs shape {:?}/{:?}/{:?}",
            lhs_value.ir_type, lhs_value.php_type, lhs_value.ownership
        ))
    })?;
    let rhs_kind = classify_strict_value(
        rhs_value.ir_type,
        &rhs_value.php_type,
        rhs_value.ownership,
    )
    .ok_or_else(|| {
        WasmError::Unsupported(format!(
            "strict rhs shape {:?}/{:?}/{:?}",
            rhs_value.ir_type, rhs_value.php_type, rhs_value.ownership
        ))
    })?;
    let negated = inst.op == Op::StrictNotEq;

    // A runtime-tagged cell against a concrete value: the cell's TAG decides the type, so this
    // cannot take the "different kinds are unequal" shortcut below — a Mixed holding an int is
    // strictly equal to an int.
    if lhs_kind == StrictValueKind::MixedCell || rhs_kind == StrictValueKind::MixedCell {
        // Two cells: either could hold an array, so the answer needs PHP's deep, ORDER-sensitive
        // array identity rather than the tag-plus-payload test the concrete path uses.
        if lhs_kind == rhs_kind {
            ctx.emit_load_value(lhs)?;
            ctx.emit_load_value(rhs)?;
            ctx.fb.ins(
                "call $__rt_strict_mixed_mixed",
                "compare two tagged cells, deeply for arrays",
            );
            return finish_i32_boolean(ctx, inst, negated);
        }
        let (cell, concrete, concrete_kind) = if lhs_kind == StrictValueKind::MixedCell {
            (lhs, rhs, rhs_kind)
        } else {
            (rhs, lhs, lhs_kind)
        };
        let tag = mixed_tag_for(concrete_kind).ok_or_else(|| {
            WasmError::Unsupported("strict comparison against an untagged shape".to_string())
        })?;
        ctx.emit_load_value(cell)?;
        ctx.fb
            .ins(&format!("i64.const {tag}"), "the concrete side's runtime tag");
        match concrete_kind {
            StrictValueKind::Null => {
                ctx.fb.ins("i64.const 0", "null carries no payload");
                ctx.fb.ins("i64.const 0", "hi unused");
            }
            StrictValueKind::Float => {
                ctx.emit_load_value(concrete)?;
                ctx.fb
                    .ins("i64.reinterpret_f64", "float bits -> lo");
                ctx.fb.ins("i64.const 0", "hi unused");
            }
            StrictValueKind::Str => {
                ctx.emit_load_value(concrete)?;
                // The loaded string is (ptr, len); the pointer has to widen under the length.
                ctx.fb.ins("call $__rt_str_pair_to_mixed_args", "(ptr,len) -> (lo,hi)");
            }
            StrictValueKind::Object => {
                ctx.emit_load_value(concrete)?;
                ctx.fb.ins("i64.extend_i32_u", "object pointer -> lo");
                ctx.fb.ins("i64.const 0", "hi unused");
            }
            _ => {
                ctx.emit_load_value(concrete)?;
                ctx.fb.ins("i64.const 0", "hi unused");
            }
        }
        ctx.fb.ins(
            "call $__rt_strict_mixed_scalar",
            "compare a tagged cell against a concrete value",
        );
        return finish_i32_boolean(ctx, inst, negated);
    }

    // An inline `?int` pair. Its tag is 0 or 8 and nothing else, so every comparison here is
    // decided by that tag plus at most one payload test — and a side this pair can never hold
    // (a string, an object, a float, a bool) makes the answer a compile-time false.
    if lhs_kind == StrictValueKind::Tagged || rhs_kind == StrictValueKind::Tagged {
        return lower_tagged_strict_compare(
            ctx, inst, lhs, lhs_kind, rhs, rhs_kind, negated,
        );
    }

    if lhs_kind != rhs_kind {
        ctx.fb.ins(
            if negated {
                "i64.const 1"
            } else {
                "i64.const 0"
            },
            "different exact PHP types compare strictly unequal",
        );
        return store_result(ctx, inst);
    }

    match lhs_kind {
        StrictValueKind::Int | StrictValueKind::Bool | StrictValueKind::Null => {
            ctx.emit_load_value(lhs)?;
            ctx.emit_load_value(rhs)?;
            ctx.fb.ins(
                if negated { "i64.ne" } else { "i64.eq" },
                "strict integer-backed equality",
            );
            finish_i32_boolean(ctx, inst, false)
        }
        StrictValueKind::Float => {
            ctx.emit_load_value(lhs)?;
            ctx.emit_load_value(rhs)?;
            ctx.fb.ins(
                if negated { "f64.ne" } else { "f64.eq" },
                "strict PHP float equality",
            );
            finish_i32_boolean(ctx, inst, false)
        }
        StrictValueKind::Str => {
            ctx.emit_load_value(lhs)?;
            ctx.emit_load_value(rhs)?;
            ctx.fb.ins(
                "call $__rt_strict_str_eq",
                "compare strict strings by length and bytes",
            );
            finish_i32_boolean(ctx, inst, negated)
        }
        StrictValueKind::Object => {
            ctx.emit_load_value(lhs)?;
            ctx.emit_load_value(rhs)?;
            ctx.fb.ins(
                if negated { "i32.ne" } else { "i32.eq" },
                "compare strict object identity",
            );
            finish_i32_boolean(ctx, inst, false)
        }
        // Handled above: a Mixed cell never reaches the same-kind path, because a pair of them
        // is refused and a mixed/concrete pair returns before this match.
        // Handled above: a pair of cells returns before this match.
        StrictValueKind::MixedCell => Err(WasmError::Unsupported(
            "strict comparison of two Mixed cells".to_string(),
        )),
        // Handled above too: a tagged pair on EITHER side returns before this match.
        StrictValueKind::Tagged => Err(WasmError::Unsupported(
            "strict comparison of two nullable ints".to_string(),
        )),
    }
}

/// Lowers a strict comparison where at least one side is an inline `?int` pair.
///
/// The pair is `(payload i64, tag i32)` with tag 0 for an int and 8 for null, so:
///
/// - against `null`, only the TAG matters;
/// - against an `int`, the tag must be 0 AND the payloads must match — `$x === 0` must not
///   answer true for a null whose payload word also happens to be zero, which is exactly what
///   testing the payload alone would do;
/// - against a string, object, float or bool, the answer is a compile-time FALSE, because a
///   `?int` holds neither of those and PHP's `===` compares type first;
/// - against another pair, equal tags plus — when they are ints — equal payloads.
fn lower_tagged_strict_compare(
    ctx: &mut FnCtx,
    inst: &Instruction,
    lhs: crate::ir::ValueId,
    lhs_kind: StrictValueKind,
    rhs: crate::ir::ValueId,
    rhs_kind: StrictValueKind,
    negated: bool,
) -> Result<()> {
    // Both sides tagged: compare the tags, then the payloads only when both are ints.
    if lhs_kind == StrictValueKind::Tagged && rhs_kind == StrictValueKind::Tagged {
        let lhs_tag = ctx.fresh_temp(ValType::I32);
        let lhs_payload = ctx.fresh_temp(ValType::I64);
        let rhs_tag = ctx.fresh_temp(ValType::I32);
        let rhs_payload = ctx.fresh_temp(ValType::I64);
        ctx.emit_load_value(lhs)?;
        ctx.fb
            .ins(&format!("local.set {}", lhs_tag), "left runtime tag");
        ctx.fb
            .ins(&format!("local.set {}", lhs_payload), "left payload word");
        ctx.emit_load_value(rhs)?;
        ctx.fb
            .ins(&format!("local.set {}", rhs_tag), "right runtime tag");
        ctx.fb
            .ins(&format!("local.set {}", rhs_payload), "right payload word");
        ctx.fb.ins(&format!("local.get {}", lhs_tag), "left tag");
        ctx.fb.ins(&format!("local.get {}", rhs_tag), "right tag");
        ctx.fb.ins("i32.eq", "same PHP type?");
        ctx.fb.ins("if (result i32)", "tags agree");
        ctx.fb
            .ins(&format!("local.get {}", lhs_tag), "left tag");
        ctx.fb
            .ins(&format!("i32.const {}", TAGGED_NULL), "the null tag");
        ctx.fb.ins("i32.eq", "both null?");
        ctx.fb.ins("if (result i32)", "both null");
        ctx.fb
            .ins("i32.const 1", "null is identical to null whatever the payload word holds");
        ctx.fb.ins("else", "both int");
        ctx.fb
            .ins(&format!("local.get {}", lhs_payload), "left payload");
        ctx.fb
            .ins(&format!("local.get {}", rhs_payload), "right payload");
        ctx.fb.ins("i64.eq", "same integer");
        ctx.fb.ins("end", "end null test");
        ctx.fb.ins("else", "different tags");
        ctx.fb.ins("i32.const 0", "an int is never identical to null");
        ctx.fb.ins("end", "end tag test");
        return finish_i32_boolean(ctx, inst, negated);
    }

    let (tagged, concrete, concrete_kind) = if lhs_kind == StrictValueKind::Tagged {
        (lhs, rhs, rhs_kind)
    } else {
        (rhs, lhs, lhs_kind)
    };
    // A `?int` is never a string, an object, a float or a bool, and `===` compares the type
    // first — so the answer is settled here without reading either side.
    if !matches!(concrete_kind, StrictValueKind::Int | StrictValueKind::Null) {
        ctx.fb.ins(
            if negated {
                "i64.const 1"
            } else {
                "i64.const 0"
            },
            "a nullable int is never identical to this type",
        );
        return store_result(ctx, inst);
    }

    let tag = ctx.fresh_temp(ValType::I32);
    let payload = ctx.fresh_temp(ValType::I64);
    ctx.emit_load_value(tagged)?;
    ctx.fb.ins(&format!("local.set {}", tag), "runtime tag");
    ctx.fb
        .ins(&format!("local.set {}", payload), "payload word");
    ctx.fb.ins(&format!("local.get {}", tag), "runtime tag");
    match concrete_kind {
        StrictValueKind::Null => {
            ctx.fb
                .ins(&format!("i32.const {}", TAGGED_NULL), "the null tag");
            ctx.fb.ins("i32.eq", "holding null is the whole question");
        }
        _ => {
            ctx.fb
                .ins(&format!("i32.const {}", TAGGED_INT), "the int tag");
            ctx.fb.ins("i32.eq", "does it hold an int at all?");
            ctx.fb.ins("if (result i32)", "it holds an int");
            ctx.fb.ins(&format!("local.get {}", payload), "payload word");
            ctx.emit_load_value(concrete)?;
            ctx.fb.ins("i64.eq", "same integer");
            ctx.fb.ins("else", "it holds null");
            ctx.fb.ins("i32.const 0", "null is never identical to an int");
            ctx.fb.ins("end", "end int test");
        }
    }
    finish_i32_boolean(ctx, inst, negated)
}

/// Converts an i32 comparison result into the EIR i64 boolean representation.
fn finish_i32_boolean(ctx: &mut FnCtx, inst: &Instruction, negate: bool) -> Result<()> {
    if negate {
        ctx.fb.ins("i32.eqz", "invert strict string equality");
    }
    ctx.fb.ins("i64.extend_i32_u", "strict bool i32 -> i64");
    store_result(ctx, inst)
}

const RT_STRICT_STR_EQ: &str = r#"(func $__rt_strict_str_eq
  (param $ap i32) (param $al i64) (param $bp i32) (param $bl i64) (result i32)
  (local $i i64)
  (if (i64.ne (local.get $al) (local.get $bl))
    (then (return (i32.const 0))))
  (loop $scan
    (if (i64.ge_u (local.get $i) (local.get $al))
      (then (return (i32.const 1))))
    (if
      (i32.ne
        (i32.load8_u
          (i32.add (local.get $ap) (i32.wrap_i64 (local.get $i))))
        (i32.load8_u
          (i32.add (local.get $bp) (i32.wrap_i64 (local.get $i)))))
      (then (return (i32.const 0))))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $scan))
  (i32.const 0))
"#;

/// Registers the borrowed, length-delimited strict string comparison runtime.
/// `__rt_str_pair_to_mixed_args`: reshapes a loaded `(ptr, len)` string into `(lo, hi)`.
///
/// A string reaches the stack as an i32 pointer under an i64 length, which is the wrong order
/// and the wrong width for the `(lo, hi)` pair the Mixed comparison takes. Doing the swap in a
/// helper keeps the caller from needing two scratch locals at every comparison site.
const RT_STR_PAIR_TO_MIXED_ARGS: &str = r#"(func $__rt_str_pair_to_mixed_args (param $ptr i32) (param $len i64) (result i64) (result i64)
  (i64.extend_i32_u (local.get $ptr))                             ;; lo: the pointer, widened
  (local.get $len))                                               ;; hi: the length
"#;

/// `__rt_strict_mixed_scalar`: PHP's `===` between a tagged Mixed cell and a concrete value.
///
/// The cell's runtime TAG decides the type, so a tag mismatch is the answer — which is also what
/// makes this correct without implementing PHP's deep array identity: the concrete side is never
/// an array, so a cell holding one simply mismatches. Floats compare as FLOATS rather than as
/// bits, because `NAN === NAN` is false and `0.0 === -0.0` is true, and neither survives a bit
/// comparison. Objects compare by pointer identity, which is what PHP's `===` means for them.
///
/// Null compares on its TAG alone. PHP has exactly one null, and this backend does not agree with
/// itself on a payload for it: an unboxed null literal carries the `0x7fff_ffff_ffff_fffe`
/// sentinel while an absent cell unboxes to zero. Comparing payloads there answered false for
/// `$mixed === null` depending only on how the null had arrived.
const RT_STRICT_MIXED_SCALAR: &str = r#"(func $__rt_strict_mixed_scalar (param $cell i32) (param $want i64) (param $lo i64) (param $hi i64) (result i32)
  (local $tag i64)                                                ;; the cell's runtime tag
  (local $clo i64)                                                ;; the cell's payload
  (local $chi i64)                                                ;; the cell's second word
  ;; Walk nested (tag 7) cells inline rather than calling `__rt_mixed_unbox`: this helper ships
  ;; with the strict runtime, which a module carries without necessarily carrying the Mixed one.
  (if (i32.eqz (local.get $cell))
    (then (local.set $tag (i64.const 8)))                         ;; an absent cell is PHP's null
    (else
      (block $found (loop $walk
        (local.set $tag (i64.load (local.get $cell)))
        (br_if $found (i64.ne (local.get $tag) (i64.const 7)))    ;; not a forwarding cell
        (local.set $cell (i32.wrap_i64 (i64.load (i32.add (local.get $cell) (i32.const 8)))))
        (if (i32.eqz (local.get $cell))
          (then (local.set $tag (i64.const 8)) (br $found)))      ;; a nested null is still null
        (br $walk)))
      (if (i32.eqz (local.get $cell))
        (then (local.set $tag (i64.const 8)))
        (else
          (local.set $clo (i64.load (i32.add (local.get $cell) (i32.const 8))))
          (local.set $chi (i64.load (i32.add (local.get $cell) (i32.const 16))))))))
  (if (i64.ne (local.get $tag) (local.get $want))
    (then (return (i32.const 0))))                                ;; a different type is never identical
  (if (i64.eq (local.get $tag) (i64.const 8))                     ;; null: the tag IS the whole value
    (then (return (i32.const 1))))                                ;; PHP has exactly one null
  (if (i64.eq (local.get $tag) (i64.const 1))                     ;; string: compare the bytes
    (then (return (call $__rt_strict_str_eq
      (i32.wrap_i64 (local.get $clo)) (local.get $chi)
      (i32.wrap_i64 (local.get $lo)) (local.get $hi)))))
  (if (i64.eq (local.get $tag) (i64.const 2))                     ;; float: compare as a float
    (then (return (f64.eq (f64.reinterpret_i64 (local.get $clo))
                          (f64.reinterpret_i64 (local.get $lo))))))
  (i64.eq (local.get $clo) (local.get $lo)))                      ;; int, bool, object identity
"#;

/// `__rt_strict_unbox`: reads a Mixed cell's `(tag, lo, hi)`, walking forwarding cells.
///
/// The same inline walk `__rt_strict_mixed_scalar` performs, factored out now that three helpers
/// need it. It stays in this module rather than calling `__rt_mixed_unbox` for the reason that
/// one documents: the strict runtime ships with modules that do not carry the Mixed runtime.
const RT_STRICT_UNBOX: &str = r#"(func $__rt_strict_unbox (param $cell i32) (result i64 i64 i64)
  (local $tag i64) (local $lo i64) (local $hi i64)
  (if (i32.eqz (local.get $cell))
    (then (return (i64.const 8) (i64.const 0) (i64.const 0))))    ;; an absent cell is PHP's null
  (block $found (loop $walk
    (local.set $tag (i64.load (local.get $cell)))
    (br_if $found (i64.ne (local.get $tag) (i64.const 7)))        ;; not a forwarding cell
    (local.set $cell (i32.wrap_i64 (i64.load (i32.add (local.get $cell) (i32.const 8)))))
    (if (i32.eqz (local.get $cell))
      (then (return (i64.const 8) (i64.const 0) (i64.const 0))))  ;; a nested null is still null
    (br $walk)))
  (local.set $lo (i64.load (i32.add (local.get $cell) (i32.const 8))))
  (local.set $hi (i64.load (i32.add (local.get $cell) (i32.const 16))))
  (local.get $tag) (local.get $lo) (local.get $hi))
"#;

/// `__rt_strict_packed_parts`: reads one INDEXED array element as `(tag, lo, hi)`, borrowed.
///
/// The element's type is the array's own `value_type` — an indexed array is homogeneous — read
/// from the kind word exactly as `__rt_array_elem_to_mixed` reads it. That helper BOXES what it
/// finds; a comparison only needs to look, so this one allocates nothing and the mapping from
/// storage `value_type` to runtime tag is the only thing they share.
const RT_STRICT_PACKED_PARTS: &str = r#"(func $__rt_strict_packed_parts (param $array i32) (param $index i64) (result i64 i64 i64)
  (local $vt i64) (local $slot i32)
  (local.set $vt (i64.and
    (i64.shr_u (i64.load (i32.sub (local.get $array) (i32.const 8))) (i64.const 8))
    (i64.const 127)))                                             ;; value_type = kind word bits 8..14
  (local.set $slot (i32.add
    (i32.add (local.get $array) (i32.const 24))                   ;; slots follow the 24-byte header
    (i32.wrap_i64 (i64.mul (local.get $index) (select
      (i64.const 16) (i64.const 8)
      (i32.or (i64.eq (local.get $vt) (i64.const 1))              ;; string slots are 16 bytes,
              (i64.eq (local.get $vt) (i64.const 7))))))))        ;; and so are Mixed-cell slots
  (if (i64.eq (local.get $vt) (i64.const 7))                      ;; a stored cell: read through it
    (then (return (call $__rt_strict_unbox (i32.wrap_i64 (i64.load (local.get $slot)))))))
  (if (i64.eq (local.get $vt) (i64.const 1))                      ;; string: (pointer, length)
    (then (return (i64.const 1) (i64.load (local.get $slot))
                  (i64.load (i32.add (local.get $slot) (i32.const 8))))))
  (if (i64.eq (local.get $vt) (i64.const 2))
    (then (return (i64.const 2) (i64.load (local.get $slot)) (i64.const 0))))   ;; float bits
  (if (i64.eq (local.get $vt) (i64.const 3))
    (then (return (i64.const 3) (i64.load (local.get $slot)) (i64.const 0))))   ;; bool
  (if (i64.eq (local.get $vt) (i64.const 4))
    (then (return (i64.const 6) (i64.load (local.get $slot)) (i64.const 0))))   ;; object
  (if (i64.eq (local.get $vt) (i64.const 5))
    (then (return (i64.const 4) (i64.load (local.get $slot)) (i64.const 0))))   ;; nested indexed
  (if (i64.eq (local.get $vt) (i64.const 6))
    (then (return (i64.const 5) (i64.load (local.get $slot)) (i64.const 0))))   ;; nested hash
  (i64.const 0) (i64.load (local.get $slot)) (i64.const 0))                     ;; int
"#;

/// `__rt_strict_container_eq`: PHP's `===` between two arrays, whatever their storage.
///
/// Measured on php-src 8.5.6, array identity is DEEP and ORDER-SENSITIVE: the counts must match,
/// and walking both in INSERTION order the keys must be identical pairwise and the values
/// strictly equal. `[1 => "a", 2 => "b"] === [2 => "b", 1 => "a"]` is false, so comparing key
/// SETS would answer the wrong thing.
///
/// One logical cursor serves both representations, which is what makes a packed array comparable
/// against a hash for free: an indexed array's keys are `0..count-1` and its cursor is the index,
/// while a hash walks its insertion-order list from `head` through each entry's `next`. `kind` is
/// 0 for indexed and 1 for a hash — the caller derives it from the runtime tag, 4 or 5.
///
/// Nothing here allocates or takes a reference: both containers are borrowed for the duration.
const RT_STRICT_CONTAINER_EQ: &str = r#"(func $__rt_strict_container_eq (param $a i32) (param $ak i32) (param $b i32) (param $bk i32) (result i32)
  (local $ac i64) (local $bc i64) (local $ai i64) (local $bi i64) (local $ae i32) (local $be i32)
  (local $akl i64) (local $akh i64) (local $bkl i64) (local $bkh i64)
  (local $atag i64) (local $alo i64) (local $ahi i64)
  (local $btag i64) (local $blo i64) (local $bhi i64)
  ;; A null container pointer is an empty array; two of them are identical.
  (if (i32.or (i32.eqz (local.get $a)) (i32.eqz (local.get $b)))
    (then (return (i32.and (i32.eqz (local.get $a)) (i32.eqz (local.get $b))))))
  (local.set $ac (i64.load (local.get $a)))                       ;; count sits at +0 in both layouts
  (local.set $bc (i64.load (local.get $b)))
  (if (i64.ne (local.get $ac) (local.get $bc))
    (then (return (i32.const 0))))                                ;; different lengths, different arrays
  ;; The starting cursor: a hash begins at its list head, an indexed array at 0.
  (local.set $ai (select (i64.load (i32.add (local.get $a) (i32.const 24))) (i64.const 0) (local.get $ak)))
  (local.set $bi (select (i64.load (i32.add (local.get $b) (i32.const 24))) (i64.const 0) (local.get $bk)))
  (block $done (loop $step
    ;; End of the walk. The counts already agree, so one ending ends both.
    (br_if $done (select
      (i64.eq (local.get $ai) (i64.const -1))
      (i64.ge_u (local.get $ai) (local.get $ac))
      (local.get $ak)))
    ;; Read this position's key and value from each side.
    (if (local.get $ak)
      (then
        (local.set $ae (i32.add (i32.add (local.get $a) (i32.const 40))
                                (i32.wrap_i64 (i64.mul (local.get $ai) (i64.const 72)))))
        (local.set $akl (i64.load (i32.add (local.get $ae) (i32.const 8))))
        (local.set $akh (i64.load (i32.add (local.get $ae) (i32.const 16))))
        (local.set $atag (i64.load (i32.add (local.get $ae) (i32.const 40))))
        (local.set $alo (i64.load (i32.add (local.get $ae) (i32.const 24))))
        (local.set $ahi (i64.load (i32.add (local.get $ae) (i32.const 32)))))
      (else
        (local.set $akl (local.get $ai))
        (local.set $akh (i64.const -1))                           ;; -1 marks an integer key
        (call $__rt_strict_packed_parts (local.get $a) (local.get $ai))
        (local.set $ahi) (local.set $alo) (local.set $atag)))
    (if (local.get $bk)
      (then
        (local.set $be (i32.add (i32.add (local.get $b) (i32.const 40))
                                (i32.wrap_i64 (i64.mul (local.get $bi) (i64.const 72)))))
        (local.set $bkl (i64.load (i32.add (local.get $be) (i32.const 8))))
        (local.set $bkh (i64.load (i32.add (local.get $be) (i32.const 16))))
        (local.set $btag (i64.load (i32.add (local.get $be) (i32.const 40))))
        (local.set $blo (i64.load (i32.add (local.get $be) (i32.const 24))))
        (local.set $bhi (i64.load (i32.add (local.get $be) (i32.const 32)))))
      (else
        (local.set $bkl (local.get $bi))
        (local.set $bkh (i64.const -1))
        (call $__rt_strict_packed_parts (local.get $b) (local.get $bi))
        (local.set $bhi) (local.set $blo) (local.set $btag)))
    ;; An integer key (hi = -1) is never identical to a string key.
    (if (i32.ne (i64.eq (local.get $akh) (i64.const -1)) (i64.eq (local.get $bkh) (i64.const -1)))
      (then (return (i32.const 0))))
    (if (i64.eq (local.get $akh) (i64.const -1))
      (then
        (if (i64.ne (local.get $akl) (local.get $bkl))
          (then (return (i32.const 0)))))
      (else
        (if (i32.eqz (call $__rt_strict_str_eq
              (i32.wrap_i64 (local.get $akl)) (local.get $akh)
              (i32.wrap_i64 (local.get $bkl)) (local.get $bkh)))
          (then (return (i32.const 0))))))
    (if (i32.eqz (call $__rt_strict_parts
          (local.get $atag) (local.get $alo) (local.get $ahi)
          (local.get $btag) (local.get $blo) (local.get $bhi)))
      (then (return (i32.const 0))))
    ;; Advance both cursors.
    (local.set $ai (select
      (i64.load (i32.add (local.get $ae) (i32.const 56)))
      (i64.add (local.get $ai) (i64.const 1))
      (local.get $ak)))
    (local.set $bi (select
      (i64.load (i32.add (local.get $be) (i32.const 56)))
      (i64.add (local.get $bi) (i64.const 1))
      (local.get $bk)))
    (br $step)))
  (i32.const 1))
"#;

/// `__rt_strict_parts`: PHP's `===` between two already-unboxed `(tag, lo, hi)` triples.
///
/// The type-directed half of strict identity, shared by the cell-to-cell entry point and by the
/// element comparison inside a container walk — which is why it takes triples rather than cells:
/// a hash entry stores its value that way, so boxing one just to compare it would allocate for
/// nothing.
///
/// The tags an ARRAY can carry are two (4 indexed, 5 hash) for one PHP type, so they are folded
/// before the mismatch test; everything else identifies its type exactly. Floats compare as
/// floats (`NAN === NAN` is false, `0.0 === -0.0` is true), objects by pointer identity, and null
/// on its tag alone.
const RT_STRICT_PARTS: &str = r#"(func $__rt_strict_parts (param $atag i64) (param $alo i64) (param $ahi i64) (param $btag i64) (param $blo i64) (param $bhi i64) (result i32)
  (local $acont i32) (local $bcont i32)
  (local.set $acont (i32.or (i64.eq (local.get $atag) (i64.const 4)) (i64.eq (local.get $atag) (i64.const 5))))
  (local.set $bcont (i32.or (i64.eq (local.get $btag) (i64.const 4)) (i64.eq (local.get $btag) (i64.const 5))))
  (if (i32.and (local.get $acont) (local.get $bcont))             ;; both arrays: deep and ordered
    (then (return (call $__rt_strict_container_eq
      (i32.wrap_i64 (local.get $alo)) (i64.eq (local.get $atag) (i64.const 5))
      (i32.wrap_i64 (local.get $blo)) (i64.eq (local.get $btag) (i64.const 5))))))
  (if (i32.or (local.get $acont) (local.get $bcont))
    (then (return (i32.const 0))))                                ;; an array is never anything else
  (if (i64.ne (local.get $atag) (local.get $btag))
    (then (return (i32.const 0))))                                ;; a different type is never identical
  (if (i64.eq (local.get $atag) (i64.const 8))
    (then (return (i32.const 1))))                                ;; PHP has exactly one null
  (if (i64.eq (local.get $atag) (i64.const 1))                    ;; string: compare the bytes
    (then (return (call $__rt_strict_str_eq
      (i32.wrap_i64 (local.get $alo)) (local.get $ahi)
      (i32.wrap_i64 (local.get $blo)) (local.get $bhi)))))
  (if (i64.eq (local.get $atag) (i64.const 2))                    ;; float: compare as a float
    (then (return (f64.eq (f64.reinterpret_i64 (local.get $alo))
                          (f64.reinterpret_i64 (local.get $blo))))))
  (i64.eq (local.get $alo) (local.get $blo)))                     ;; int, bool, object identity
"#;

/// `__rt_strict_mixed_mixed`: PHP's `===` between two runtime-tagged cells.
///
/// Both sides are unboxed and handed to the shared type-directed comparison. This pair used to be
/// refused outright because either cell could hold an array, whose identity is PHP's deep
/// element-wise comparison; that comparison now exists, so the pair is answered rather than
/// turned away.
const RT_STRICT_MIXED_MIXED: &str = r#"(func $__rt_strict_mixed_mixed (param $a i32) (param $b i32) (result i32)
  (local $atag i64) (local $alo i64) (local $ahi i64)
  (local $btag i64) (local $blo i64) (local $bhi i64)
  (call $__rt_strict_unbox (local.get $a))
  (local.set $ahi) (local.set $alo) (local.set $atag)
  (call $__rt_strict_unbox (local.get $b))
  (local.set $bhi) (local.set $blo) (local.set $btag)
  (call $__rt_strict_parts
    (local.get $atag) (local.get $alo) (local.get $ahi)
    (local.get $btag) (local.get $blo) (local.get $bhi)))
"#;

/// Adds the `===` runtime: string, scalar, container and boxed-pair comparison helpers.
pub(super) fn emit_strict_runtime(module: &mut WatModule) {
    module.add_raw_func(RT_STRICT_STR_EQ);
    module.add_raw_func(RT_STR_PAIR_TO_MIXED_ARGS);
    module.add_raw_func(RT_STRICT_MIXED_SCALAR);
    module.add_raw_func(RT_STRICT_UNBOX);
    module.add_raw_func(RT_STRICT_PACKED_PARTS);
    module.add_raw_func(RT_STRICT_CONTAINER_EQ);
    module.add_raw_func(RT_STRICT_PARTS);
    module.add_raw_func(RT_STRICT_MIXED_MIXED);
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! WAT validation and Wasmer regressions for strict binary string equality.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Raw data segments exercise empty, prefix, embedded-NUL, and invalid
    //!   UTF-8 bytes without passing through Rust or PHP string decoding.

    use super::*;
    use crate::codegen::Emit;
    use crate::codegen_wasm::wat::DataSegment;
    use crate::ir::{Builder, Function, Module, Terminator};
    use crate::codegen_support::platform::Target;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

    /// Returns whether the Wasmer CLI is available; a missing host skips loudly or, under
/// `ELEPHC_TEST_REQUIRE_WASM_HOSTS`, fails (see `test_support`).
    fn wasmer_available() -> bool {
        crate::codegen_wasm::test_support::wasmer_available()
    }

    /// Verifies the Mixed tags this module compares against match what boxing writes.
    ///
    /// `===` against a runtime-tagged cell decides the TYPE from the cell's tag, so these
    /// constants have to be the same ones `inst::lower_mixed_box` stamps. A drift would make a
    /// comparison answer false for a value PHP calls identical, silently and only at runtime.
    #[test]
    fn concrete_strict_kinds_map_to_the_tags_boxing_writes() {
        for (kind, tag) in [
            (StrictValueKind::Int, 0),
            (StrictValueKind::Str, 1),
            (StrictValueKind::Float, 2),
            (StrictValueKind::Bool, 3),
            (StrictValueKind::Object, 6),
            (StrictValueKind::Null, 8),
        ] {
            assert_eq!(mixed_tag_for(kind), Some(tag), "{kind:?} boxes under tag {tag}");
        }
        // A cell has no tag of its own to compare against: it IS the tagged side.
        assert_eq!(mixed_tag_for(StrictValueKind::MixedCell), None);

        let boxing = include_str!("inst.rs");
        for (tag, comment) in [
            (2, "mixed tag (float)"),
            (1, "mixed tag (string)"),
        ] {
            assert!(
                boxing.contains(&format!("i64.const {tag}\", \"{comment}")),
                "boxing must still stamp tag {tag} for {comment}"
            );
        }
    }

    /// Verifies a pair of Mixed cells is admitted now that deep array identity exists.
    ///
    /// Two cells could both hold arrays, whose PHP identity is a deep, ORDER-sensitive
    /// element-wise comparison — which is why the pair used to be refused. That walk exists, so
    /// the pair is answered. What stays refused is a cell against an inline `?int` PAIR: that
    /// side's tag is dynamic too, and the mixed/concrete path assumes exactly one side is.
    #[test]
    fn two_mixed_cells_are_admitted_and_a_tagged_pair_is_not() {
        assert!(strict_pair_is_supported(
            StrictValueKind::MixedCell,
            StrictValueKind::MixedCell
        ));
        for concrete in [
            StrictValueKind::Int,
            StrictValueKind::Bool,
            StrictValueKind::Null,
            StrictValueKind::Float,
            StrictValueKind::Str,
            StrictValueKind::Object,
        ] {
            assert!(strict_pair_is_supported(StrictValueKind::MixedCell, concrete));
            assert!(strict_pair_is_supported(concrete, StrictValueKind::MixedCell));
        }
        assert!(!strict_pair_is_supported(
            StrictValueKind::MixedCell,
            StrictValueKind::Tagged
        ));
        assert!(!strict_pair_is_supported(
            StrictValueKind::Tagged,
            StrictValueKind::MixedCell
        ));
    }

    /// Verifies the deep array walk is ORDER-sensitive and allocates nothing.
    ///
    /// Measured on php-src 8.5.6: `["a" => 1, "b" => 2] === ["b" => 2, "a" => 1]` is FALSE, so
    /// comparing key SETS would answer the wrong thing — the walk has to advance both cursors in
    /// insertion order and compare pairwise. And a comparison that boxed each element to compare
    /// it would allocate on a read-only operation, so the helpers read `(tag, lo, hi)` in place.
    #[test]
    fn the_deep_array_walk_advances_both_cursors_and_never_boxes() {
        assert!(
            RT_STRICT_CONTAINER_EQ.contains("(i64.load (i32.add (local.get $ae) (i32.const 56)))")
                && RT_STRICT_CONTAINER_EQ
                    .contains("(i64.load (i32.add (local.get $be) (i32.const 56)))"),
            "both sides must advance through their own insertion-order list"
        );
        for boxing in ["__rt_mixed_from_value", "__rt_heap_alloc", "__rt_incref"] {
            assert!(
                !RT_STRICT_CONTAINER_EQ.contains(boxing)
                    && !RT_STRICT_PACKED_PARTS.contains(boxing),
                "a read-only comparison must not call {boxing}"
            );
        }
    }

    /// Verifies the tagged comparison treats floats and null the way PHP does, not the way bits do.
    ///
    /// `NAN === NAN` is false and `0.0 === -0.0` is true, so a float has to be compared as a
    /// float; comparing the payload words would get both backwards. Null compares on its tag
    /// alone because this backend represents an unboxed null literal with a sentinel and an
    /// absent cell with zero, so a payload comparison would depend on how the null arrived.
    #[test]
    fn tagged_comparison_uses_float_semantics_and_a_tag_only_null() {
        assert!(
            RT_STRICT_MIXED_SCALAR.contains("f64.eq (f64.reinterpret_i64 (local.get $clo))"),
            "a float must compare as a float, not as its bits"
        );
        assert!(
            RT_STRICT_MIXED_SCALAR
                .contains("(if (i64.eq (local.get $tag) (i64.const 8))                     ;; null: the tag IS the whole value"),
            "null compares on its tag alone"
        );
        // Self-contained: this ships with the strict runtime, which a module carries without
        // necessarily carrying the Mixed one. The name may appear in a comment explaining why
        // the walk is inline; what must not appear is a CALL to it.
        assert!(!RT_STRICT_MIXED_SCALAR.contains("call $__rt_mixed_unbox"));
    }

    /// Builds, validates, and invokes the strict-string runtime driver.
    fn run_strict_string_driver() -> Option<String> {
        let mut module = WatModule::new();
        module.set_memory(1, Some("memory"));
        emit_strict_runtime(&mut module);
        for (offset, bytes) in [
            (32, vec![b'a', 0, b'b', 0xff]),
            (64, vec![b'a', 0, b'b', 0xff]),
            (96, vec![b'a', 0, b'b']),
            (128, vec![b'a', 0, b'c', 0xff]),
        ] {
            module.add_data(DataSegment { offset, bytes });
        }
        module.add_raw_func(
            r#"(func $t (export "t") (result i32)
  (call $__rt_strict_str_eq (i32.const 0) (i64.const 0) (i32.const 0) (i64.const 0))
  (i32.const 1000)
  i32.mul
  (call $__rt_strict_str_eq (i32.const 32) (i64.const 4) (i32.const 64) (i64.const 4))
  (i32.const 100)
  i32.mul
  i32.add
  (call $__rt_strict_str_eq (i32.const 32) (i64.const 4) (i32.const 96) (i64.const 3))
  i32.eqz
  (i32.const 10)
  i32.mul
  i32.add
  (call $__rt_strict_str_eq (i32.const 32) (i64.const 4) (i32.const 128) (i64.const 4))
  i32.eqz
  i32.add)
"#,
        );
        let wat = module.render();
        let bytes =
            ::wat::parse_str(&wat).unwrap_or_else(|error| panic!("WAT failed: {error}\n{wat}"));
        wasmparser::validate(&bytes)
            .unwrap_or_else(|error| panic!("WASM validation failed: {error}\n{wat}"));
        if !wasmer_available() {
            return None;
        }

        let sequence = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "elephc_wasm_strict_{}_{}",
            std::process::id(),
            sequence
        ));
        std::fs::create_dir_all(&dir).expect("strict temp directory");
        let path = dir.join("strict.wasm");
        std::fs::write(&path, bytes).expect("strict wasm artifact");
        let output = std::process::Command::new("wasmer")
            .arg("run")
            .arg("--invoke")
            .arg("t")
            .arg(&path)
            .output()
            .expect("invoke strict wasm driver");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            output.status.success(),
            "strict driver failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Verifies both strict opcodes pass capability planning, assemble, validate,
    /// and execute with the EIR i64 boolean result convention.
    #[test]
    fn strict_scalar_equality_opcodes_lower_and_run() {
        let mut module = Module::new(Target::wasm());
        let delimiter = module.data.intern_string(",");
        let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);

            for (op, lhs, rhs) in [
                (Op::StrictEq, 7, 7),
                (Op::StrictNotEq, 7, 8),
                (Op::StrictEq, 7, 8),
                (Op::StrictNotEq, 7, 7),
            ] {
                let lhs = builder.emit_const_i64(lhs);
                let rhs = builder.emit_const_i64(rhs);
                let result = builder
                    .emit(
                        op,
                        vec![lhs, rhs],
                        None,
                        IrType::I64,
                        PhpType::Bool,
                        Ownership::NonHeap,
                    )
                    .expect("strict scalar result");
                let _ = builder.emit(
                    Op::EchoValue,
                    vec![result],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
                let delimiter = builder.emit_const_str(delimiter);
                let _ = builder.emit(
                    Op::EchoValue,
                    vec![delimiter],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            }
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let wat = crate::codegen_wasm::generate(&module, Emit::Executable)
            .expect("strict scalar module");
        let bytes =
            ::wat::parse_str(&wat).unwrap_or_else(|error| panic!("WAT failed: {error}\n{wat}"));
        wasmparser::validate(&bytes)
            .unwrap_or_else(|error| panic!("WASM validation failed: {error}\n{wat}"));
        if !wasmer_available() {
            return;
        }

        let sequence = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "elephc_wasm_strict_eir_{}_{}",
            std::process::id(),
            sequence
        ));
        std::fs::create_dir_all(&dir).expect("strict EIR temp directory");
        let path = dir.join("strict-eir.wasm");
        std::fs::write(&path, bytes).expect("strict EIR wasm artifact");
        let output = std::process::Command::new("wasmer")
            .arg("run")
            .arg(&path)
            .output()
            .expect("run strict EIR module");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            output.status.success(),
            "strict EIR module failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "1,1,,,");
        assert!(output.stderr.is_empty());
    }

    /// Verifies strict strings compare by exact length and raw byte content.
    #[test]
    fn strict_binary_string_equality_is_length_delimited() {
        if let Some(output) = run_strict_string_driver() {
            assert_eq!(output, "1111");
        }
    }
}
