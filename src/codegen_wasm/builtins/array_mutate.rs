//! Purpose:
//! Array builtins that change or rebuild an array: push/pop/shift/unshift/splice and pad/rebuild.
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.

use super::*;

/// Validates `array_splice` and `array_unshift`, which REPLACE the caller's array.
///
/// Both answer a relocated pointer that has to reach the caller's storage, which is what
/// `write_back_container_slot` does for a local slot and for a by-reference cell alike. What
/// this proves is the part that mechanism cannot: that the operands have the representation the
/// helpers read.
pub(super) fn array_mutator_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let Some(source) = call.operands.first().and_then(|id| function.value(*id)) else {
        return Some(format!("{target:?} source is missing from the value table"));
    };
    // A declared `array` prepends inside its cell (`__rt_mixed_cell_unshift`), so any element
    // layout serves; each value is boxed into a cell of its own.
    if target == RuntimeFnId::ArrayUnshift
        && source.ir_type == IrType::Heap(IrHeapKind::Mixed)
        && source.php_type.is_php_array()
    {
        for (index, value) in call.operands.iter().enumerate().skip(1) {
            if !crate::codegen_wasm::capability::value_boxes_into_a_container_write(function, *value) {
                return Some(format!(
                    "array_unshift value #{index} cannot be boxed into a cell of its own"
                ));
            }
        }
        return None;
    }
    if source.ir_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "{target:?} source {:?}/{:?} is not an indexed array",
            source.ir_type,
            source.php_type.codegen_repr()
        ));
    }
    let PhpType::Array(source_element) = source.php_type.codegen_repr() else {
        return Some(format!(
            "{target:?} source {:?} is not an indexed array",
            source.php_type.codegen_repr()
        ));
    };
    if target == RuntimeFnId::ArrayUnshift {
        // The prepended values are WRITTEN, so they need the raw i64 representation for the
        // same reason `array_pad`'s fill does.
        if !matches!(*source_element, PhpType::Int | PhpType::Never) {
            return Some(format!(
                "array_unshift into array<{:?}> on wasm32-wasi (a prepended element has to be a \
                 raw i64 slot)",
                source_element
            ));
        }
        for (index, value) in call.operands.iter().enumerate().skip(1) {
            let Some(value) = function.value(*value) else {
                return Some(format!("array_unshift value #{index} is missing"));
            };
            if value.ir_type != IrType::I64 || value.php_type.codegen_repr() != PhpType::Int {
                return Some(format!(
                    "array_unshift value #{index} is {:?}/{:?}, not an int",
                    value.ir_type,
                    value.php_type.codegen_repr()
                ));
            }
        }
        return None;
    }
    if call.operands.len() < 2 || call.operands.len() > 4 {
        return Some(format!(
            "array_splice takes an offset and optionally a length and a replacement, got {} \
             operands",
            call.operands.len()
        ));
    }
    for index in 1..call.operands.len().min(3) {
        let Some(value) = function.value(call.operands[index]) else {
            return Some(format!("array_splice operand #{index} is missing"));
        };
        if value.ir_type != IrType::I64 {
            return Some(format!(
                "array_splice operand #{index} is {:?}, not an int",
                value.ir_type
            ));
        }
    }
    // The replacement's elements are copied slot-for-slot into the result, so the two arrays
    // must agree on their element representation — a wider replacement would be read at the
    // source's stride and answer garbage rather than php's heterogeneous array.
    if let Some(replacement) = call.operands.get(3).and_then(|id| function.value(*id)) {
        let matching = replacement.ir_type == IrType::Heap(IrHeapKind::Array)
            && matches!(
                replacement.php_type.codegen_repr(),
                PhpType::Array(element) if element.codegen_repr() == source_element.codegen_repr()
            );
        if !matching {
            return Some(format!(
                "array_splice replacement {:?}/{:?} does not share the source's element \
                 representation",
                replacement.ir_type,
                replacement.php_type.codegen_repr()
            ));
        }
    }
    None
}

/// Lowers `array_splice` and `array_unshift`, writing the relocated array back.
/// Lowers `array_unshift` through a declared `array`: the values as a list of cells, the
/// prepend inside the receiver's cell, and php's new count.
pub(super) fn lower_boxed_array_unshift(ctx: &mut FnCtx, inst: &Instruction, source: crate::ir::ValueId) -> Result<()> {
    let values = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
    ctx.fb.ins(
        &format!(
            "(local.set {values} (call $__rt_array_new (i64.const {}) (i64.const 16)))",
            inst.operands.len().saturating_sub(1)
        ),
        "the values to prepend, in order",
    );
    for index in 1..inst.operands.len() {
        let value = operand(inst, index)?;
        let cell = crate::codegen_wasm::inst::emit_owned_cell_for_container_write(ctx, value)?;
        ctx.fb.ins(
            &format!("(local.set {values} (call $__rt_array_push_mixed (local.get {values}) (local.get {cell})))"),
            "one prepended value, owned by the list",
        );
    }
    ctx.emit_load_value(source)?;
    ctx.fb.ins(&format!("local.get {values}"), "the value list");
    ctx.fb.ins("call $__rt_mixed_cell_unshift", "prepend inside the array's cell");
    let count = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    ctx.fb.ins(&format!("local.set {count}"), "php answers the new element count");
    ctx.fb.ins(
        &format!("(call $__rt_decref_any (local.get {values}))"),
        "the value list; the array holds its own references",
    );
    ctx.fb.ins(&format!("(i64.lt_s (local.get {count}) (i64.const 0))"), "no array in the cell?");
    ctx.fb.ins("if", "corrupt array cell (non-returning)");
    ctx.fb.ins("i32.const 9", "heap type mismatch failure code");
    ctx.fb.ins("call $__rt_fail", "report the heap type mismatch");
    ctx.fb.ins(
        "unreachable",
        "elephc-trap:post-noreturn:mixed-cell-unshift-not-an-array a cell without an array does not return",
    );
    ctx.fb.ins("end", "end corrupt cell guard");
    if inst.result.is_some() {
        ctx.fb.ins(&format!("local.get {count}"), "the new count");
        return store_result(ctx, inst);
    }
    Ok(())
}

/// Lowers `array_splice` and `array_unshift`, which REPLACE the caller's array.
///
/// The helper answers the relocated array pointer, and this writes it back to the storage the
/// caller holds (a local slot or a by-reference cell) so the old pointer is never reused.
pub(super) fn lower_array_mutator(
    ctx: &mut FnCtx,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Result<()> {
    let source = operand(inst, 0)?;
    if target == RuntimeFnId::ArrayUnshift
        && ctx.function.value(source).map(|value| value.ir_type) == Some(IrType::Heap(IrHeapKind::Mixed))
    {
        return lower_boxed_array_unshift(ctx, inst, source);
    }
    if target == RuntimeFnId::ArrayUnshift {
        // The values arrive as separate operands; the helper takes them as one array so the
        // prepend is a single copy rather than one relocation per value.
        let values = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
        ctx.fb.ins(
            &format!(
                "(local.set {values} (call $__rt_array_new (i64.const {}) (i64.const 8)))",
                inst.operands.len().saturating_sub(1)
            ),
            "the values to prepend, in order",
        );
        for index in 1..inst.operands.len() {
            ctx.fb.ins(&format!("local.get {values}"), "the value list");
            ctx.emit_load_value(operand(inst, index)?)?;
            ctx.fb.ins("call $__rt_array_push_int", "one prepended value");
            ctx.fb.ins(&format!("local.set {values}"), "it may have relocated");
        }
        ctx.emit_load_value(source)?;
        ctx.fb.ins(&format!("local.get {values}"), "the value list");
        ctx.fb.ins("call $__rt_array_unshift_int", "prepend, answering the new count");
        let count = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
        ctx.fb.ins(&format!("local.set {count}"), "php answers the new element count");
        ctx.emit_store_value(source)?;
        crate::codegen_wasm::inst::write_back_container_slot(ctx, source)?;
        ctx.fb.ins(
            &format!("(call $__rt_decref_any (local.get {values}))"),
            "the value list was copied out of",
        );
        if inst.result.is_some() {
            ctx.fb.ins(&format!("local.get {count}"), "the new count");
            return store_result(ctx, inst);
        }
        return Ok(());
    }
    ctx.emit_load_value(source)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    if inst.operands.len() >= 3 {
        ctx.emit_load_value(operand(inst, 2)?)?;
        ctx.fb.ins("i32.const 1", "a length was given");
    } else {
        ctx.fb.ins("i64.const 0", "no length");
        ctx.fb.ins("i32.const 0", "which means everything from the offset");
    }
    if inst.operands.len() >= 4 {
        ctx.emit_load_value(operand(inst, 3)?)?;
    } else {
        ctx.fb.ins("i32.const 0", "no replacement");
    }
    ctx.fb.ins(
        "call $__rt_array_splice_any",
        "the window is removed and the replacement takes its place",
    );
    let removed = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
    ctx.fb.ins(&format!("local.set {removed}"), "what php answers");
    ctx.emit_store_value(source)?;
    crate::codegen_wasm::inst::write_back_container_slot(ctx, source)?;
    if inst.result.is_some() {
        ctx.fb.ins(&format!("local.get {removed}"), "the removed window");
        return store_result(ctx, inst);
    }
    ctx.fb.ins(
        &format!("(call $__rt_decref_any (local.get {removed}))"),
        "nothing reads what was removed",
    );
    Ok(())
}

/// Validates `array_pad`, `array_unique` and `array_column`.
///
/// The first two WRITE elements, so they are served only for the raw i64 representation: a
/// value's width and its refcount obligation both follow the array's `value_type`, and there is
/// no representation-agnostic way to place one. `array_column` writes cells by construction, so
/// it takes any row storage its lookup can read.
pub(super) fn array_rebuild_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let Some(source) = call.operands.first().and_then(|id| function.value(*id)) else {
        return Some(format!("{target:?} source is missing from the value table"));
    };
    if source.ir_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "{target:?} source {:?} is not an indexed array",
            source.ir_type
        ));
    }
    // `array_unique` preserves the source keys, so its answer is a sparse array — a HASH.
    // The other two rebuild a dense one.
    let answer = if target == RuntimeFnId::ArrayUnique {
        IrType::Heap(IrHeapKind::Hash)
    } else {
        IrType::Heap(IrHeapKind::Array)
    };
    if call.result.is_none() || call.result_type != answer {
        return Some(format!("{target:?} must answer {answer:?}"));
    }
    match target {
        RuntimeFnId::ArrayColumn => {
            // php's third argument re-keys the result, which this form does not build.
            if call.operands.len() != 2 {
                return Some(format!(
                    "array_column with {} operands on wasm32-wasi (the index-key form re-keys \
                     the result, which is a different array)",
                    call.operands.len()
                ));
            }
            let Some(key) = function.value(call.operands[1]) else {
                return Some("array_column key is missing from the value table".to_string());
            };
            if key.ir_type != IrType::Str {
                return Some(format!("array_column key {:?} is not a string", key.ir_type));
            }
            match call.result_php_type.codegen_repr() {
                PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed => None,
                other => Some(format!("array_column answers array<mixed>, got {other:?}")),
            }
        }
        _ => {
            let int_list = matches!(
                source.php_type.codegen_repr(),
                PhpType::Array(element) if matches!(*element, PhpType::Int | PhpType::Never)
            );
            if !int_list {
                return Some(format!(
                    "{target:?} of {:?} on wasm32-wasi (an element it must WRITE has to be a raw \
                     i64 slot)",
                    source.php_type.codegen_repr()
                ));
            }
            // `array_unique` keeps the source's ELEMENT type but changes its container: the
            // preserved keys make the answer an `array<int, T>`. `array_pad` stays dense.
            let expected = if target == RuntimeFnId::ArrayUnique {
                match source.php_type.codegen_repr() {
                    PhpType::Array(element) => PhpType::AssocArray {
                        key: Box::new(PhpType::Int),
                        value: element,
                    },
                    other => other,
                }
            } else {
                source.php_type.codegen_repr()
            };
            if call.result_php_type.codegen_repr() != expected {
                return Some(format!(
                    "{target:?} answers its source's element type, got {:?}",
                    call.result_php_type.codegen_repr()
                ));
            }
            if target == RuntimeFnId::ArrayPad && call.operands.len() != 3 {
                return Some(format!(
                    "array_pad takes a size and a value, got {} operands",
                    call.operands.len()
                ));
            }
            if target == RuntimeFnId::ArrayUnique && call.operands.len() != 1 {
                return Some(format!(
                    "array_unique with {} operands on wasm32-wasi (the sort-flags form compares \
                     differently)",
                    call.operands.len()
                ));
            }
            None
        }
    }
}

/// Raises php's `TypeError` when either of a call's first two array arguments is a null pointer.
///
/// A statically array-typed operand can be null at run time (a missed element read of an
/// `array<array<T>>`), and php names the argument position in its `must be of type array, null
/// given` error, so the first and second argument have a failure code each. The arguments are
/// checked in order, as php does, before the helper dereferences either.
pub(super) fn emit_null_array_argument_guard(
    ctx: &mut FnCtx,
    inst: &Instruction,
    first_code: i32,
    second_code: i32,
    reason: &str,
) -> Result<()> {
    for (index, code) in [(0usize, first_code), (1usize, second_code)] {
        ctx.emit_load_value(operand(inst, index)?)?;
        ctx.fb.ins("i32.eqz", "argument is a null pointer?");
        ctx.fb.ins("if", reason);
        crate::codegen_wasm::inst::emit_runtime_failure(ctx, code, reason);
        ctx.fb.ins("end", "end null-argument guard");
    }
    Ok(())
}

/// The `__rt_fail` code php's oversized-`array_pad` `ValueError` reports under.
pub(super) const ARRAY_PAD_SIZE_FAILURE_CODE: i32 = 32;

/// Lowers `array_pad`, `array_unique` and `array_column`.
pub(super) fn lower_array_rebuild(
    ctx: &mut FnCtx,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Result<()> {
    // php refuses a size past 2^30 in magnitude BEFORE it allocates, with a catchable
    // `ValueError`; `PHP_INT_MIN` belongs to the same arm because negating it wraps. The size is
    // spilled so the guard can read it before the helper consumes it.
    let pad_size = if matches!(target, RuntimeFnId::ArrayPad) {
        let size = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
        ctx.emit_load_value(operand(inst, 1)?)?;
        ctx.fb.ins(&format!("local.set {size}"), "spill the array_pad size");
        ctx.fb.ins(&format!("local.get {size}"), "requested size");
        ctx.fb.ins("i64.const 1073741824", "php's array size limit (2^30)");
        ctx.fb.ins("i64.gt_s", "positive size past the limit?");
        ctx.fb.ins(&format!("local.get {size}"), "requested size");
        ctx.fb.ins("i64.const -1073741824", "the same limit on the left");
        ctx.fb.ins("i64.lt_s", "negative size past the limit (includes PHP_INT_MIN)?");
        ctx.fb.ins("i32.or", "either bound exceeded?");
        ctx.fb.ins("if", "array_pad() rejects a size past the array limit");
        crate::codegen_wasm::inst::emit_runtime_failure(ctx, ARRAY_PAD_SIZE_FAILURE_CODE, "array_pad() size limit");
        ctx.fb.ins("end", "end array_pad size guard");
        Some(size)
    } else {
        None
    };
    ctx.emit_load_value(operand(inst, 0)?)?;
    match target {
        RuntimeFnId::ArrayPad => {
            ctx.fb.ins(
                &format!("local.get {}", pad_size.as_deref().expect("array_pad spills its size above")),
                "the validated size",
            );
            ctx.emit_load_value(operand(inst, 2)?)?;
            ctx.fb.ins(
                "call $__rt_array_pad_int",
                "a negative size pads on the left, and the size is a target not an amount",
            );
        }
        RuntimeFnId::ArrayUnique => {
            ctx.fb.ins(
                "call $__rt_array_unique_int",
                "the FIRST occurrence of each value survives",
            );
        }
        _ => {
            ctx.emit_load_value(operand(inst, 1)?)?;
            ctx.fb.ins(
                "call $__rt_array_column",
                "the named value of every row, boxed",
            );
        }
    }
    store_result(ctx, inst)
}
