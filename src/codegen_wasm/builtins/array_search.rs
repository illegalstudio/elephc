//! Purpose:
//! Array search builtins: `in_array`, `array_search`, `array_key_exists` and the hash lookups behind them.
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.

use super::*;

/// How `in_array` scans, for one (needle, element) pair.
///
/// `blocks_strict` marks a pair whose types DIFFER: `===` compares types first, so a strict
/// request there can never match and the scan is skipped. `widen_elements` marks the one mix that
/// converts the elements rather than the needle.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(in crate::codegen_wasm) struct InArrayScan {
    pub(super) kind: InArrayKind,
    pub(super) blocks_strict: bool,
    pub(super) widen_elements: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(in crate::codegen_wasm) enum InArrayKind {
    /// 8-byte slots compared as integers; loose and strict agree.
    Int,
    /// 8-byte slots compared as doubles.
    Float,
    /// 16-byte (pointer, length) slots; loose uses the numeric-string rule.
    Str,
}

/// Classifies an `in_array` call, or `None` when the pair has no measured rule here.
///
/// PHP's table is wider than this: a string against a number, a bool against a number, and
/// anything boxed each need their own measured behaviour. An EMPTY haystack (`Void`, what an empty
/// literal's `Never` normalizes to) always answers false, but the scan still has to type-check —
/// the helper takes the needle BY VALUE — so its kind follows the needle.
pub(super) fn in_array_scan(needle: &PhpType, element: &PhpType) -> Option<InArrayScan> {
    let scan = |kind, blocks_strict, widen_elements| {
        Some(InArrayScan { kind, blocks_strict, widen_elements })
    };
    match (needle.codegen_repr(), element.codegen_repr()) {
        // An empty haystack never matches, but the scan still has to TYPE-CHECK: the helper takes
        // the needle by value, so the kind follows the needle rather than the (absent) elements.
        (PhpType::Int | PhpType::Bool, PhpType::Void) => scan(InArrayKind::Int, false, false),
        (PhpType::Float, PhpType::Void) => scan(InArrayKind::Float, false, false),
        (PhpType::Str, PhpType::Void) => scan(InArrayKind::Str, false, false),
        (PhpType::Int, PhpType::Int) => scan(InArrayKind::Int, false, false),
        (PhpType::Bool, PhpType::Bool) => scan(InArrayKind::Int, false, false),
        (PhpType::Float, PhpType::Float) => scan(InArrayKind::Float, false, false),
        (PhpType::Str, PhpType::Str) => scan(InArrayKind::Str, false, false),
        // Mixed numbers: PHP widens and compares as doubles, and `===` can never match.
        (PhpType::Int, PhpType::Float) => scan(InArrayKind::Float, true, false),
        (PhpType::Float, PhpType::Int) => scan(InArrayKind::Float, true, true),
        _ => None,
    }
}

/// Validates `in_array`/`array_search`: a needle, an indexed haystack, and an optional strict
/// flag. `array_search` never carries one — the front-end admits exactly two operands — so the
/// same rule covers both.
pub(super) fn in_array_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let (needle, haystack, strict) = match call.operands.as_slice() {
        [needle, haystack] => (needle, haystack, None),
        [needle, haystack, strict] => (needle, haystack, Some(strict)),
        other => {
            return Some(format!(
                "the search takes a needle, an array and an optional strict flag, got {} operands",
                other.len()
            ))
        }
    };
    let Some(needle_value) = function.value(*needle) else {
        return Some("in_array needle is missing from the value table".to_string());
    };
    let Some(haystack_value) = function.value(*haystack) else {
        return Some("in_array haystack is missing from the value table".to_string());
    };
    // A HASH haystack takes the generic walk, which compares through php's own comparison and so
    // needs no needle/element pairing — only that the needle can be boxed for it.
    if haystack_value.ir_type == IrType::Heap(IrHeapKind::Hash)
        || (haystack_value.ir_type == IrType::Heap(IrHeapKind::Mixed)
            && haystack_value.php_type.is_php_array())
    {
        if !crate::codegen_wasm::capability::closure_argument_is_boxable(
            needle_value.ir_type,
            &needle_value.php_type,
        ) {
            return Some(format!(
                "the search needle {:?}/{:?} cannot be boxed for an associative haystack",
                needle_value.ir_type,
                needle_value.php_type.codegen_repr()
            ));
        }
        return strict_flag_issue(function, strict);
    }
    if haystack_value.ir_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "in_array takes an indexed array, got {:?}",
            haystack_value.ir_type
        ));
    }
    let PhpType::Array(element) = haystack_value.php_type.codegen_repr() else {
        return Some(format!(
            "in_array takes an indexed array, got {:?}",
            haystack_value.php_type.codegen_repr()
        ));
    };
    if in_array_scan(&needle_value.php_type, &element).is_none() {
        return Some(format!(
            "in_array has no measured rule for {:?} against elements of {:?}",
            needle_value.php_type.codegen_repr(),
            element
        ));
    }
    strict_flag_issue(function, strict)
}

/// Validates the optional third argument both searches share: php's `$strict` bool.
pub(super) fn strict_flag_issue(
    function: &Function,
    strict: Option<&crate::ir::ValueId>,
) -> Option<String> {
    let strict = strict?;
    let Some(value) = function.value(*strict) else {
        return Some("in_array strict flag is missing from the value table".to_string());
    };
    if value.ir_type != IrType::I64 || !matches!(value.php_type.codegen_repr(), PhpType::Bool) {
        return Some(format!(
            "in_array strict flag is {:?}/{:?}, expected I64/Bool",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    None
}

/// Lowers `in_array` by scanning the haystack with the pair's measured comparison.
///
/// The scan answers the first matching INDEX, which `array_search` boxes and this reduces to a
/// bool — one scan, two builtins.
pub(super) fn lower_in_array(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    emit_array_find(ctx, inst)?;
    ctx.fb.ins("i64.const 0", "a miss answers -1");
    ctx.fb.ins("i64.ge_s", "found means a non-negative index");
    ctx.fb.ins("i64.extend_i32_u", "PHP booleans are i64 here");
    store_result(ctx, inst)
}

/// Lowers `array_search`: the same scan, boxed as PHP's `int|false`.
///
/// The front-end admits exactly two operands, so only the LOOSE form exists here. `int|false`
/// travels as a Mixed cell — tag 0 carrying the index, or tag 3 carrying the value false —
/// matching what `strpos` already does for the same result type.
pub(super) fn lower_array_search(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let index = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    emit_array_find(ctx, inst)?;
    ctx.fb.ins(&format!("local.set {}", index), "first matching index, or -1");
    ctx.fb.ins(&format!("local.get {}", index), "index");
    ctx.fb.ins("i64.const 0", "a miss answers -1");
    ctx.fb.ins("i64.ge_s", "found?");
    ctx.fb.ins("if (result i32)", "int|false travels as a Mixed cell");
    ctx.fb.ins("i64.const 0", "int tag");
    ctx.fb.ins(&format!("local.get {}", index), "the key");
    ctx.fb.ins("i64.const 0", "no high payload");
    ctx.fb.ins("call $__rt_mixed_from_value", "box the key");
    ctx.fb.ins("else", "not found");
    ctx.fb.ins("i64.const 3", "bool tag");
    ctx.fb.ins("i64.const 0", "the value false");
    ctx.fb.ins("i64.const 0", "no high payload");
    ctx.fb.ins("call $__rt_mixed_from_value", "box false");
    ctx.fb.ins("end", "");
    store_result(ctx, inst)
}

/// Whether an `in_array`/`array_search` haystack is a hash rather than an indexed array.
pub(super) fn in_array_haystack_is_hash(function: &Function, call: &Instruction) -> bool {
    call.operands
        .get(1)
        .and_then(|haystack| function.value(*haystack))
        .is_some_and(|value| {
            value.ir_type == IrType::Heap(IrHeapKind::Hash)
                // A declared `array` walks the same way: it may be a list or a hash.
                || (value.ir_type == IrType::Heap(IrHeapKind::Mixed) && value.php_type.is_php_array())
        })
}

/// Lowers `in_array`/`array_search` over a HASH, where the indexed specializations do not apply.
///
/// An indexed haystack has one element type and the scan is chosen for that pair; a hash may hold
/// anything, so the walk compares through php's own comparison instead. `array_search` answers
/// the KEY — a string here, not a position — which is why the helper hands one back rather than
/// an index.
pub(super) fn lower_hash_find(ctx: &mut FnCtx, inst: &Instruction, want_key: bool) -> Result<()> {
    let needle = operand(inst, 0)?;
    let haystack = operand(inst, 1)?;
    let cell = crate::codegen_wasm::objects::emit_box_value_into_mixed(ctx, needle)?;
    ctx.emit_load_value(haystack)?;
    ctx.fb.ins(&format!("local.get {}", cell), "the boxed needle");
    if inst.operands.len() == 3 {
        ctx.emit_load_value(operand(inst, 2)?)?;
        ctx.fb.ins("i32.wrap_i64", "the strict flag is a PHP bool");
    } else {
        ctx.fb.ins("i32.const 0", "no strict flag: php's loose comparison");
    }
    let boxed = ctx.function.value(haystack).map(|value| value.ir_type)
        == Some(IrType::Heap(IrHeapKind::Mixed));
    ctx.fb.ins(
        if boxed { "call $__rt_boxed_find_value" } else { "call $__rt_hash_find_value" },
        "walk the values in insertion order, answer the matching key",
    );
    let found = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
    ctx.fb
        .ins(&format!("local.set {}", found), "the matching key, or 0");
    ctx.fb.ins(
        &format!("(call $__rt_decref_any (local.get {}))", cell),
        "the needle was boxed for this call alone",
    );
    if want_key {
        // `array_search` answers `key|false`, and the key cell already IS that answer.
        ctx.fb.ins(&format!("local.get {}", found), "the matching key");
        ctx.fb.ins("if (result i32)", "key|false travels as a Mixed cell");
        ctx.fb.ins(&format!("local.get {}", found), "the key cell");
        ctx.fb.ins("else", "not found");
        ctx.fb.ins("i64.const 3", "bool tag");
        ctx.fb.ins("i64.const 0", "the value false");
        ctx.fb.ins("i64.const 0", "no high payload");
        ctx.fb.ins("call $__rt_mixed_from_value", "box false");
        ctx.fb.ins("end", "");
        return store_result(ctx, inst);
    }
    // `in_array` wanted only the fact, so the key it was handed is released.
    ctx.fb.ins(
        &format!("(call $__rt_decref_any (local.get {}))", found),
        "the key was owned and is not the answer",
    );
    ctx.fb.ins(&format!("local.get {}", found), "the matching key, or 0");
    ctx.fb.ins("i32.const 0", "0 means no entry matched");
    ctx.fb.ins("i32.ne", "a key came back");
    ctx.fb.ins("i64.extend_i32_u", "php answers a bool");
    store_result(ctx, inst)
}

/// Emits the scan shared by `in_array` and `array_search`, leaving the matching index on the
/// stack (-1 when absent).
pub(super) fn emit_array_find(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let needle = operand(inst, 0)?;
    let haystack = operand(inst, 1)?;
    let needle_php = ctx
        .function
        .value(needle)
        .map(|v| v.php_type.codegen_repr())
        .unwrap_or(PhpType::Mixed);
    let element = match ctx.function.value(haystack).map(|v| v.php_type.codegen_repr()) {
        Some(PhpType::Array(element)) => *element,
        other => {
            return Err(WasmError::Unsupported(format!(
                "in_array takes an indexed array, got {other:?}"
            )))
        }
    };
    let scan = in_array_scan(&needle_php, &element).ok_or_else(|| {
        WasmError::Unsupported(format!(
            "in_array has no rule for {needle_php:?} against {element:?}"
        ))
    })?;

    // The strict flag is a runtime value, so park it: the helpers branch on it.
    let strict = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
    if inst.operands.len() == 3 {
        ctx.emit_load_value(operand(inst, 2)?)?;
        ctx.fb.ins("i32.wrap_i64", "the strict flag is a PHP bool");
        ctx.fb.ins(&format!("local.set {}", strict), "strict comparison requested");
    } else {
        ctx.fb.ins("i32.const 0", "no strict flag: loose comparison");
        ctx.fb.ins(&format!("local.set {}", strict), "strict comparison requested");
    }

    match scan.kind {
        InArrayKind::Int => {
            ctx.emit_load_value(needle)?;
            ctx.emit_load_value(haystack)?;
            emit_strict_block(ctx, &strict, scan.blocks_strict);
            ctx.fb.ins("call $__rt_array_find_int", "scan comparing as integers");
        }
        InArrayKind::Float => {
            ctx.emit_load_value(needle)?;
            if needle_php.codegen_repr() == PhpType::Int {
                ctx.fb
                    .ins("f64.convert_i64_s", "PHP widens the needle to compare");
            }
            ctx.emit_load_value(haystack)?;
            emit_strict_block(ctx, &strict, scan.blocks_strict);
            ctx.fb.ins(
                &format!("i32.const {}", i32::from(scan.widen_elements)),
                "the elements are integers to convert",
            );
            ctx.fb.ins("call $__rt_array_find_float", "scan comparing as doubles");
        }
        InArrayKind::Str => {
            ctx.emit_load_value(needle)?;
            ctx.emit_load_value(haystack)?;
            ctx.fb.ins(&format!("local.get {}", strict), "=== or ==");
            ctx.fb.ins(
                "call $__rt_array_find_str",
                "scan; loose uses the numeric-string rule",
            );
        }
    }
    Ok(())
}

/// Pushes the "a strict request cannot match" flag: the strict flag itself when the needle and the
/// elements are different types, and a constant zero otherwise.
pub(super) fn emit_strict_block(ctx: &mut FnCtx, strict: &str, blocks: bool) {
    if blocks {
        ctx.fb
            .ins(&format!("local.get {}", strict), "types differ: === can never match");
    } else {
        ctx.fb.ins("i32.const 0", "same types: === and == agree");
    }
}

/// Validates `sort`/`rsort`: one indexed array of a scalar element the runtime sorts in place.
///
/// String and Mixed elements are NOT admitted: PHP orders strings with its standard comparison,
/// where two numeric strings compare NUMERICALLY — `sort(["10", "9"])` answers `9, 10`, not
/// `10, 9` — and that rule is not this helper's.
/// Validates `array_key_exists($key, $hash)`.
///
/// An INDEXED haystack asks a different question — an int index against a dense run — and is
/// served by its own lowering, so only the key's shape is checked for it.
///
/// For an associative one the key reuses the hash key contract, so a Mixed or float key is
/// refused for the same diagnostic reason `$h[$k]` refuses it — PHP's implicit key conversions
/// are profile-specific.
pub(super) fn array_key_exists_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [key, hash] = call.operands.as_slice() else {
        return Some(format!(
            "array_key_exists takes a key and an array, got {} operands",
            call.operands.len()
        ));
    };
    let Some(haystack) = function.value(*hash) else {
        return Some("array_key_exists array is missing from the value table".to_string());
    };
    if !matches!(
        haystack.ir_type,
        IrType::Heap(IrHeapKind::Hash) | IrType::Heap(IrHeapKind::Array)
    ) {
        return Some(format!(
            "array_key_exists has no lowering for a haystack of {:?}",
            haystack.ir_type
        ));
    }
    let Some(key_value) = function.value(*key) else {
        return Some("array_key_exists key is missing from the value table".to_string());
    };
    // An indexed array is keyed by position, so the key has to BE one: php coerces a numeric
    // string here and this target does not, and answering false for `"1"` where php answers
    // true would be silent. Refused instead.
    if haystack.ir_type == IrType::Heap(IrHeapKind::Array) {
        if key_value.ir_type != IrType::I64 {
            return Some(format!(
                "array_key_exists on an indexed array needs an integer key, got {:?}",
                key_value.ir_type
            ));
        }
        return None;
    }
    let key_php = key_value.php_type.codegen_repr();
    if key_value.ir_type == IrType::Heap(IrHeapKind::Mixed) || key_php == PhpType::Mixed {
        return Some("dynamic Mixed associative keys require exact per-tag PHP diagnostics".to_string());
    }
    if key_value.ir_type == IrType::F64 && key_php == PhpType::Float {
        return Some(
            "float associative keys require exact profile-specific implicit-conversion diagnostics"
                .to_string(),
        );
    }
    None
}
