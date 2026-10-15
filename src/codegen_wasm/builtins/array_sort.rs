//! Purpose:
//! Array sort builtins over scalar lists and hashes, with their admission predicates.
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.

use super::*;

/// Validates a `sort()`-family call over a scalar-element array, returning the reason this
/// target cannot lower it, or `None` when the element type has a byte-exact comparison here.
pub(super) fn scalar_sort_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
) -> Option<String> {
    let [array] = call.operands.as_slice() else {
        return Some(format!(
            "sort takes one array, got {} operands",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*array) else {
        return Some("sort array is missing from the value table".to_string());
    };
    if value.ir_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "sort takes an indexed array, got {:?}",
            value.ir_type
        ));
    }
    let PhpType::Array(element) = value.php_type.codegen_repr() else {
        return Some(format!(
            "sort takes an indexed array, got {:?}",
            value.php_type.codegen_repr()
        ));
    };
    // `Void` is what an empty literal's `Never` normalizes to: nothing to order.
    if !matches!(
        element.codegen_repr(),
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Str | PhpType::Void
    ) {
        return Some(format!(
            "sort has no lowered ordering for elements of {element:?}"
        ));
    }
    // The string ordering runs through `__rt_str_smart_cmp`, which lives in the command-only
    // numeric runtime because its classifier's diagnostics write to stderr.
    if element.codegen_repr() == PhpType::Str
        && !module.functions.iter().any(|candidate| candidate.flags.is_main)
    {
        return Some(
            "ordering strings requires a main-bearing command module for the numeric-string \
             classifier"
                .to_string(),
        );
    }
    None
}

/// Lowers `sort`/`rsort` over a scalar array, writing the sorted pointer back into the by-
/// reference argument.
pub(super) fn lower_scalar_sort(ctx: &mut FnCtx, inst: &Instruction, descending: bool) -> Result<()> {
    let array = operand(inst, 0)?;
    let element = match ctx.function.value(array).map(|v| v.php_type.codegen_repr()) {
        Some(PhpType::Array(element)) => *element,
        other => {
            return Err(WasmError::Unsupported(format!(
                "sort takes an indexed array, got {other:?}"
            )))
        }
    };
    // A string array has 16-byte slots and its own comparison, so it takes a dedicated walk.
    if element.codegen_repr() == PhpType::Str {
        ctx.emit_load_value(array)?;
        ctx.fb
            .ins(&format!("i32.const {}", i32::from(descending)), "rsort reverses the order");
        ctx.fb.ins(
            "call $__rt_sort_string",
            "stable in-place sort by php-src's string ordering (COW)",
        );
    } else {
        let is_float = i32::from(element.codegen_repr() == PhpType::Float);
        ctx.emit_load_value(array)?;
        ctx.fb
            .ins(&format!("i32.const {}", i32::from(descending)), "rsort reverses the order");
        ctx.fb
            .ins(&format!("i32.const {is_float}"), "read the slots as doubles?");
        ctx.fb.ins(
            "call $__rt_sort_scalar",
            "stable in-place sort (COW), returns the array pointer",
        );
    }
    // `sort($a)` rebinds `$a`: the runtime may have cloned, so the pointer goes back.
    ctx.emit_store_value(array)?;
    crate::codegen_wasm::inst::write_back_container_slot(ctx, array)?;
    if inst.result.is_some() {
        ctx.fb.ins("i64.const 1", "php's array sorts answer true");
        store_result(ctx, inst)?;
    }
    Ok(())
}

/// Validates `ksort`/`krsort`/`asort`/`arsort`: one ASSOCIATIVE array, and nothing else.
///
/// The comparison is `zend_compare` over boxed keys or values, so no key or value type is refused
/// here — what is refused is the receiver being anything but a hash, since these reorder a chain
/// an indexed array does not have. php's own `sort` on an indexed array is the other function.
///
/// The FLAG argument (`SORT_NUMERIC` and friends) changes the comparison, so only the default
/// form is admitted rather than accepting a flag and ignoring it. The EIR spells that default
/// either by leaving the flag out or by passing its value, `SORT_REGULAR` (0), explicitly.
pub(super) fn hash_sort_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let hash = match call.operands.as_slice() {
        [hash] => hash,
        [hash, flag] if constant_i64_operand(function, *flag) == Some(0) => hash,
        _ => {
            return Some(format!(
                "an associative sort takes one array and at most the SORT_REGULAR flag, got {} \
                 operands (another flag changes the comparison, so only the default is served)",
                call.operands.len()
            ))
        }
    };
    let Some(value) = function.value(*hash) else {
        return Some("the sorted array is missing from the value table".to_string());
    };
    // An INDEXED array holds its keys as slot positions 0..n-1, so `ksort` on one is already
    // sorted — php runs it and answers `true`. Descending key order has no representation there
    // (the positions ARE the keys), and the value sorts need a slot permuter this target does
    // not have, so those three stay refused.
    if value.ir_type == IrType::Heap(IrHeapKind::Array) {
        if target == RuntimeFnId::Ksort {
            return None;
        }
        return Some(format!(
            "{:?} on an indexed array: its keys ARE its slot positions",
            target
        ));
    }
    if value.ir_type != IrType::Heap(IrHeapKind::Hash) {
        return Some(format!(
            "an associative sort takes an associative array, got {:?}",
            value.ir_type
        ));
    }
    None
}

/// Validates `array_key_first`/`array_key_last`: one associative array.
///
/// The result is a BOXED key, because php answers the key in its own kind — an integer key comes
/// back an integer, a string key a string — and `null` for an empty array.
pub(super) fn hash_edge_key_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let [hash] = call.operands.as_slice() else {
        return Some(format!(
            "{target:?} takes one array, got {} operands",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*hash) else {
        return Some("the array is missing from the value table".to_string());
    };
    if value.ir_type != IrType::Heap(IrHeapKind::Hash) {
        return Some(format!(
            "{target:?} on {:?}: this lowering reads a hash's insertion order",
            value.ir_type
        ));
    }
    if call.result_type != IrType::Heap(IrHeapKind::Mixed) {
        return Some(format!(
            "{target:?} answers a boxed key or null, not {:?}",
            call.result_type
        ));
    }
    None
}

/// Validates the three two-hash combiners: two associative arrays, one associative result.
pub(super) fn hash_pair_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let [left, right] = call.operands.as_slice() else {
        return Some(format!(
            "{target:?} takes two arrays, got {} operands",
            call.operands.len()
        ));
    };
    for operand in [left, right] {
        let Some(value) = function.value(*operand) else {
            return Some("an operand is missing from the value table".to_string());
        };
        if value.ir_type != IrType::Heap(IrHeapKind::Hash) {
            return Some(format!(
                "{target:?} on {:?}: this lowering walks two hashes",
                value.ir_type
            ));
        }
    }
    if call.result_type != IrType::Heap(IrHeapKind::Hash) {
        return Some(format!(
            "{target:?} answers an associative array, not {:?}",
            call.result_type
        ));
    }
    None
}

/// The `__rt_hash_sort` mode for one of php's four key/value sorts, or `None` for anything else.
///
/// Bit 1 selects the VALUE over the key and bit 0 reverses the order, which is the whole
/// difference between the four.
pub(super) fn hash_sort_mode(target: RuntimeFnId) -> Option<i32> {
    match target {
        RuntimeFnId::Ksort => Some(0),
        RuntimeFnId::Krsort => Some(1),
        RuntimeFnId::Asort => Some(2),
        RuntimeFnId::Arsort => Some(3),
        _ => None,
    }
}

/// Lowers `ksort`/`krsort`/`asort`/`arsort`: reorder the hash's iteration, answer `true`.
///
/// The uniqueness split comes FIRST and is the whole reason this is not a bare call. php's
/// copy-on-write means `$b = $a; ksort($a);` leaves `$b` in its original order, and these sorts
/// mutate the links in place — so a shared hash has to be separated before its order moves, or
/// the copy moves with it.
pub(super) fn lower_hash_sort(ctx: &mut FnCtx, inst: &Instruction, mode: i32) -> Result<()> {
    let hash = operand(inst, 0)?;
    // `ksort` on an indexed array is a no-op — the slot positions are already the ascending
    // keys — but php still runs it and answers `true`.
    if ctx.function.value(hash).map(|value| value.ir_type)
        == Some(IrType::Heap(IrHeapKind::Array))
    {
        if inst.result.is_some() {
            ctx.fb.ins("i64.const 1", "php's array sorts answer true");
            store_result(ctx, inst)?;
        }
        return Ok(());
    }
    ctx.emit_load_value(hash)?;
    ctx.fb.ins(
        "call $__rt_hash_ensure_unique",
        "separate a shared hash before its order moves (COW)",
    );
    ctx.fb
        .ins(&format!("i32.const {mode}"), "key/value and ascending/descending");
    ctx.fb.ins(
        "call $__rt_hash_sort",
        "stable merge sort over the insertion-order chain",
    );
    // The split may have answered a different hash, so the pointer goes back to the caller's
    // storage exactly as the indexed sorts do.
    ctx.emit_store_value(hash)?;
    crate::codegen_wasm::inst::write_back_container_slot(ctx, hash)?;
    if inst.result.is_some() {
        ctx.fb.ins("i64.const 1", "php's array sorts answer true");
        store_result(ctx, inst)?;
    }
    Ok(())
}
