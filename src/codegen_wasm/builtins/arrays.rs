//! Purpose:
//! Array builtins other than search and sort: keys/values, slice, merge, mutators, rebuilds, fold, fill, `count`, `implode`, `explode`.
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.

use super::*;

/// Validates `count($array)` against the one shape its load can serve.
///
/// A statically typed container reads its length straight from the header. A BOXED value goes
/// through `__rt_mixed_count`, which reads the container the cell holds or raises php-src's own
/// `TypeError` naming what arrived — measured: a boolean is named by VALUE there (`true given`,
/// not `bool given`), and the message carries no location. An object stays out: a `Countable`
/// receiver is not lowered on this target at all, so that tag fails loudly rather than guessing.
pub(super) fn count_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
) -> Option<String> {
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "expected one container operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("container operand is missing from the value table".to_string());
    };
    // PHP defines `count()` on a Countable as a call to the object's own `count()`, which is
    // what the lowering emits. The method must have a BODY in this module: an interface-only
    // declaration has nothing to call.
    if let PhpType::Object(class_name) = value.php_type.codegen_repr() {
        return countable_count_shape_issue(module, &class_name, call);
    }
    // A boxed container counts too when the box is PROVABLY one — `array_map` types its result
    // `mixed` because of its CALLBACK, never because the value might not be an array — so the
    // `TypeError` this refusal protects against cannot arise.
    let boxed_container = value.ir_type == IrType::Heap(IrHeapKind::Mixed)
        && crate::codegen_wasm::capability::value_is_container_by_construction(function, *operand);
    // Any other boxed value counts too, at runtime: `__rt_mixed_count` reads the container the
    // cell holds, or raises php-src's own `TypeError` naming what arrived. That diagnostic goes
    // through WASI, so it needs a main-bearing command module.
    let boxed_any = value.ir_type == IrType::Heap(IrHeapKind::Mixed)
        && value.php_type.codegen_repr() == PhpType::Mixed
        && module.functions.iter().any(|candidate| candidate.flags.is_main);
    if !boxed_container
        && !boxed_any
        && (!matches!(
            value.ir_type,
            IrType::Heap(IrHeapKind::Array | IrHeapKind::Hash)
        ) || !matches!(
            value.php_type.codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. }
        ))
    {
        return Some(format!(
            "expected a statically typed array or hash, got {:?}/{:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    if call.result.is_none()
        || call.result_type != IrType::I64
        || call.result_php_type.codegen_repr() != PhpType::Int
    {
        return Some(format!(
            "result {:?}/{:?} is not the expected I64/Int",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Validates `array_is_list($array)` against the one operand whose answer is known statically.
///
/// This backend's `Heap(Array)` IS the contiguous representation: its keys are `0..n-1` in order
/// by construction, which is exactly PHP's definition of a list, and an empty array qualifies.
/// A `Heap(Hash)` carries arbitrary keys and would need a real scan, so it is refused rather
/// than answered from the representation.
pub(super) fn array_is_list_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "expected one array operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("array operand is missing from the value table".to_string());
    };
    // A HASH has to be asked: php reads its insertion ORDER, so `[1 => "b", 0 => "a"]` holds
    // both keys of a two-element list and is still not one.
    if value.ir_type == IrType::Heap(IrHeapKind::Hash) {
        return None;
    }
    if value.ir_type != IrType::Heap(IrHeapKind::Array)
        || !matches!(value.php_type.codegen_repr(), PhpType::Array(_))
    {
        return Some(format!(
            "expected a statically typed indexed array, got {:?}/{:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    if call.result.is_none()
        || call.result_type != IrType::I64
        || call.result_php_type.codegen_repr() != PhpType::Bool
    {
        return Some(format!(
            "result {:?}/{:?} is not the expected I64/Bool",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Lowers `array_is_list($indexed)` to the constant true its representation guarantees.
///
/// The operand is still evaluated and dropped: it may be a call whose side effects PHP performs
/// before answering, and discarding the expression rather than the value would skip them.
pub(super) fn lower_array_is_list(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let source = operand(inst, 0)?;
    ctx.emit_load_value(source)?;
    if hash_operand(ctx, source) {
        // A hash has real keys, so the answer is in them — and in their ORDER.
        ctx.fb.ins(
            "call $__rt_hash_is_list",
            "keys exactly 0, 1, ... n-1, in that order",
        );
        return store_result(ctx, inst);
    }
    ctx.fb
        .ins("drop", "the answer follows from the representation, not the contents");
    ctx.fb
        .ins("i64.const 1", "an indexed array is a list by construction");
    store_result(ctx, inst)
}

/// Validates `array_keys`/`array_values`/`array_reverse`/`array_sum`/`array_product` over a
/// declared `array`, whose list-or-hash and element layout are only known at run time.
///
/// Each reads the source through the generic `foreach` walk, which is command-only. The
/// projections answer an `array<mixed>` of cells, the reversal and the folds a cell.
pub(super) fn boxed_array_builtin_shape_issue(
    module: &Module,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    if !module.functions.iter().any(|function| function.flags.is_main) {
        return Some(format!("{target:?} over a boxed array needs the command runtime's walk"));
    }
    let (want_ir, want_php) = match target {
        RuntimeFnId::ArrayKeys | RuntimeFnId::ArrayValues => {
            (IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Mixed)))
        }
        _ => (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed),
    };
    if call.result.is_none()
        || call.result_type != want_ir
        || call.result_php_type.codegen_repr() != want_php
    {
        return Some(format!(
            "{target:?} over a boxed array answers {want_ir:?}/{want_php:?}, not {:?}/{:?}",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Lowers the one-array builtins over a declared `array` (`boxed_array_builtin_shape_issue`).
pub(super) fn lower_boxed_array_builtin(ctx: &mut FnCtx, inst: &Instruction, target: RuntimeFnId) -> Result<()> {
    // A concrete list of cells is walked through a temporary cell holding one more reference.
    let temporary_cell = if mixed_list_operand(ctx.function, inst) {
        let cell = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
        ctx.fb.ins("i64.const 4", "list tag");
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb.ins("i64.extend_i32_u", "the list pointer -> lo");
        ctx.fb.ins("i64.const 0", "hi unused");
        ctx.fb.ins("call $__rt_mixed_from_value", "a cell to walk the list through");
        ctx.fb.ins(&format!("local.tee {cell}"), "released after the fold");
        Some(cell)
    } else {
        ctx.emit_load_value(operand(inst, 0)?)?;
        None
    };
    match target {
        RuntimeFnId::ArrayKeys => {
            ctx.fb.ins("call $__rt_boxed_array_keys", "positions of a list, keys of a hash");
        }
        RuntimeFnId::ArrayValues => {
            ctx.fb.ins("call $__rt_boxed_array_values", "the values, in php's order");
        }
        RuntimeFnId::ArrayReverse => {
            ctx.fb.ins("call $__rt_boxed_array_reverse", "reversed, string keys kept");
        }
        RuntimeFnId::ArraySum => {
            ctx.fb.ins("i32.const 0", "a sum");
            ctx.fb.ins("call $__rt_boxed_array_fold", "php's per-element `+`");
        }
        RuntimeFnId::ArrayProduct => {
            ctx.fb.ins("i32.const 1", "a product");
            ctx.fb.ins("call $__rt_boxed_array_fold", "php's per-element `*`");
        }
        other => {
            return Err(WasmError::Unsupported(format!("{other:?} over a boxed array")));
        }
    }
    if let Some(cell) = temporary_cell {
        let result = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
        ctx.fb.ins(&format!("local.set {result}"), "the fold's cell");
        ctx.fb.ins(&format!("(call $__rt_decref_any (local.get {cell}))"), "the temporary cell");
        ctx.fb.ins(&format!("local.get {result}"), "the fold's cell");
    }
    store_result(ctx, inst)
}

/// Validates a builtin that takes one indexed array and answers a fresh indexed array,
/// returning the reason this target cannot lower the call, or `None` when it can.
pub(super) fn indexed_array_result_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "expected one array operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("array operand is missing from the value table".to_string());
    };
    // A HASH operand is admitted too, and is a genuine projection rather than the list's
    // identity: `array_keys` walks its keys and `array_values` its values, both in INSERTION
    // order — the order `foreach` uses, not the bucket table's. The walk builds one fixed slot
    // width, so the result's element type has to be one it can store.
    let hash_projection = value.ir_type == IrType::Heap(IrHeapKind::Hash)
        && matches!(value.php_type.codegen_repr(), PhpType::AssocArray { .. })
        && matches!(
            call.result_php_type.codegen_repr(),
            // `Mixed` joins the two for `array_keys`: a hash keyed by BOTH kinds projects to
            // `array<mixed>`, and `__rt_hash_keys_mixed` boxes each key by its own kind rather
            // than forcing one width on both.
            PhpType::Array(element)
                if matches!(*element, PhpType::Int | PhpType::Str)
                    || (target == RuntimeFnId::ArrayKeys && matches!(*element, PhpType::Mixed))
        );
    // An INDEXED operand of ANY element type is admitted, which the `array<int>`-only gate used
    // to turn away. Neither helper reads an element: `__rt_array_index_keys` builds `[0..n-1]`
    // from the LENGTH alone, and `__rt_array_clone_shallow` copies the payload bytes and then
    // increfs every refcounted child — value_type 7, the Mixed cell, included. The gate was
    // therefore stricter than the code it guards, and `ArrayIterator::__construct` calls
    // `array_keys($array)` on an `array<mixed>`, so it refused the whole SPL iterator family.
    let indexed_source = value.ir_type == IrType::Heap(IrHeapKind::Array)
        && matches!(value.php_type.codegen_repr(), PhpType::Array(_));
    if !indexed_source && !hash_projection {
        return Some(format!(
            "expected a statically typed indexed array or an associative array, got {:?}/{:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    // What the result may be follows from WHICH projection this is. `array_keys` of a list
    // answers its positions, so `array<int>` whatever the elements were; `array_values` of a
    // list is the list itself, so the element type has to survive unchanged — claiming anything
    // else would hand the caller a contract the clone does not produce.
    let source_element = match value.php_type.codegen_repr() {
        PhpType::Array(element) => Some(*element),
        _ => None,
    };
    let result_element_ok = matches!(
        call.result_php_type.codegen_repr(),
        PhpType::Array(element)
            if matches!(*element, PhpType::Int | PhpType::Never)
                || (hash_projection && matches!(*element, PhpType::Str))
                // Positions boxed into cells, for a list the checker types `array<mixed>`.
                || (indexed_source
                    && target == RuntimeFnId::ArrayKeys
                    && matches!(*element, PhpType::Mixed))
                // A hash with keys of BOTH kinds projects to `array<mixed>`, which
                // `__rt_hash_keys_mixed` produces by boxing each key by its own kind.
                || (hash_projection
                    && target == RuntimeFnId::ArrayKeys
                    && matches!(*element, PhpType::Mixed))
                || (indexed_source
                    && target == RuntimeFnId::ArrayValues
                    && source_element.as_ref() == Some(&*element))
                // `array_reverse` only MOVES elements, so its result carries the source's own
                // element type whatever that is — the generic reverse swaps whole slots on a
                // clone and never reads one.
                || (indexed_source
                    && target == RuntimeFnId::ArrayReverse
                    && source_element.as_ref() == Some(&*element))
    );
    if call.result.is_none()
        || call.result_type != IrType::Heap(IrHeapKind::Array)
        || !result_element_ok
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not an expected Heap(Array) projection",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Lowers `array_keys($list)` to the positional key array its representation implies.
pub(super) fn lower_array_keys(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let source = operand(inst, 0)?;
    ctx.emit_load_value(source)?;
    if hash_operand(ctx, source) {
        // A hash's keys are real keys, not positions, and they carry their own storage.
        let helper = match element_php_type(&inst.result_php_type) {
            Some(PhpType::Str) => "call $__rt_hash_keys_str",
            // Keys of BOTH kinds: each is boxed by its own, rather than forced to one.
            Some(PhpType::Mixed) => "call $__rt_hash_keys_mixed",
            _ => "call $__rt_hash_keys_int",
        };
        ctx.fb.ins(helper, "the hash's keys, in insertion order");
    } else {
        let helper = match element_php_type(&inst.result_php_type) {
            // A list stored as `array<mixed>` wants its positions BOXED, not as raw i64 slots.
            Some(PhpType::Mixed) => "call $__rt_array_index_keys_mixed",
            _ => "call $__rt_array_index_keys",
        };
        ctx.fb.ins(helper, "keys of a list are its positions");
    }
    store_result(ctx, inst)
}

/// Lowers `array_values($list)` to a shallow clone.
///
/// Re-indexing a list changes nothing, so the values are the source's in order; the clone is
/// what makes the result an independent owned array rather than an alias of the source.
pub(super) fn lower_array_values(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let source = operand(inst, 0)?;
    ctx.emit_load_value(source)?;
    if hash_operand(ctx, source) {
        let helper = match element_php_type(&inst.result_php_type) {
            Some(PhpType::Str) => "call $__rt_hash_values_str",
            _ => "call $__rt_hash_values_int",
        };
        ctx.fb
            .ins(helper, "the hash's values, in insertion order");
    } else {
        ctx.fb
            .ins("call $__rt_array_clone_shallow", "values of a list are the list itself");
    }
    store_result(ctx, inst)
}

/// Validates `array_sum($list)` / `array_product($list)`, which fold a list to an int.
///
/// `array_sum` also takes a HETEROGENEOUS list: its elements are cells rather than raw slots, so
/// it reads them through the generic walk and casts each the way php casts it. `array_product`
/// keeps its narrower admission because no lowering serves that source.
pub(super) fn array_fold_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "expected one array operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("array operand is missing from the value table".to_string());
    };
    // Only raw integer slots are folded here. A list of boxed values reaches php's per-type
    // rules — a float stays a float, a non-numeric string, an array or an object warns
    // "Addition is not supported on type …" and is skipped — and none of those are lowered.
    if !indexed_int_array(value) {
        return Some(format!(
            "expected a statically typed array<int>, got {:?}/{:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    // php answers int|float — an overflowing fold is promoted — so the result is a Mixed cell.
    if call.result.is_none()
        || call.result_type != IrType::Heap(IrHeapKind::Mixed)
        || call.result_php_type.codegen_repr() != PhpType::Mixed
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not the expected int|float Mixed cell",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Lowers `array_reverse($list)` to a reversed copy.
pub(super) fn lower_array_reverse(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    // The generic reverse works on a CLONE, whose element representation and refcounts the
    // clone already established, so it serves every element type by swapping whole slots.
    ctx.fb.ins(
        "call $__rt_array_reverse_any",
        "reversing a list re-indexes it from zero",
    );
    store_result(ctx, inst)
}

/// Lowers `array_sum($list)` / `array_product($list)` to their accumulating scan.
///
/// The checker types both `int|float`: php promotes an overflowing sum or product to a float,
/// so `array_sum([PHP_INT_MAX, 1])` is `9.2233720368547758E+18`. The helpers fold the raw
/// slots with that promotion and answer an owned Mixed cell.
pub(super) fn lower_array_fold(ctx: &mut FnCtx, inst: &Instruction, target: RuntimeFnId) -> Result<()> {
    let source = operand(inst, 0)?;
    ctx.emit_load_value(source)?;
    let (helper, comment) = if target == RuntimeFnId::ArraySum {
        ("call $__rt_array_sum_int", "php sums an empty array to 0")
    } else {
        ("call $__rt_array_product_int", "php's empty product is 1")
    };
    ctx.fb.ins(helper, comment);
    store_result(ctx, inst)
}

/// Validates `array_fill(0, $count, $value)` against the one shape a list can represent.
///
/// A non-zero start index produces the keys `start..start+count-1`, which is not a list, so the
/// start must be the literal `0`. The value must be an int because the result's slots are raw
/// i64s.
pub(super) fn array_fill_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [start, count, value] = call.operands.as_slice() else {
        return Some(format!(
            "expected start, count and value, got {} operands",
            call.operands.len()
        ));
    };
    if !literal_zero(function, *start) {
        return Some(
            "only a literal 0 start index yields a list; other starts key from the start index"
                .to_string(),
        );
    }
    for operand in [count, value] {
        let Some(operand) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        if operand.ir_type != IrType::I64 || operand.php_type.codegen_repr() != PhpType::Int {
            return Some(format!(
                "expected an int operand, got {:?}/{:?}",
                operand.ir_type,
                operand.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Heap(IrHeapKind::Array)
        || !matches!(
            call.result_php_type.codegen_repr(),
            PhpType::Array(element) if matches!(*element, PhpType::Int | PhpType::Never)
        )
    {
        return Some(format!(
            "result {:?}/{:?} is not the expected Heap(Array)/array<int>",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Lowers `array_fill(0, $count, $value)` to a filled list.
pub(super) fn lower_array_fill(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.emit_load_value(operand(inst, 2)?)?;
    ctx.fb
        .ins("call $__rt_array_fill_int", "one repeated value per slot");
    store_result(ctx, inst)
}

/// Lowers `array_slice` over a list, answering a fresh `array<mixed>`.
///
/// The offset/length rules are `substr`'s exactly (verified on 52 pairs against php-src), and
/// live in `__rt_array_slice` so the clamping that keeps `PHP_INT_MIN` from wrapping an i64 sits
/// next to the arithmetic it protects. The element shape decides whether a source slot is boxed
/// or, for a Mixed-cell source, shared with an incref.
pub(super) fn lower_array_slice(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let array = operand(inst, 0)?;
    let element = match ctx.function.value(array).map(|v| v.php_type.codegen_repr()) {
        Some(PhpType::Array(element)) => *element,
        other => {
            return Err(WasmError::Unsupported(format!(
                "array_slice takes an indexed array, got {other:?}"
            )))
        }
    };
    let (tag, elem_size) = array_slice_element_shape(&element).ok_or_else(|| {
        WasmError::Unsupported(format!("array_slice has no element copy for {element:?}"))
    })?;
    // The audit decided which shape the EIR's result type demands; -2 is the verbatim copy.
    let tag = match array_slice_result_mode(ctx.function, inst) {
        Some(true) => -2,
        Some(false) => tag,
        None => {
            return Err(WasmError::Unsupported(
                "array_slice result element type matches neither shape".to_string(),
            ))
        }
    };
    ctx.emit_load_value(array)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    match inst.operands.len() {
        3 => {
            ctx.emit_load_value(operand(inst, 2)?)?;
            ctx.fb.ins("i32.const 1", "a length was given");
        }
        _ => {
            ctx.fb.ins("i64.const 0", "unused length");
            ctx.fb.ins("i32.const 0", "no length: slice runs to the end");
        }
    }
    ctx.fb
        .ins(&format!("i64.const {tag}"), "element cell tag (negative: share the cell)");
    ctx.fb
        .ins(&format!("i64.const {elem_size}"), "source slot stride");
    ctx.fb.ins(
        "call $__rt_array_slice",
        "copy the window into a fresh mixed-cell array",
    );
    store_result(ctx, inst)
}

/// Validates `array_merge`: two indexed arrays whose element types agree with the result.
///
/// The front-end admits exactly two operands. Both reach here with the SAME element type — when
/// they differ, EIR widens each with `Op::ArrayToMixed` first — so a mismatch here means the
/// widening did not happen and the slot layouts would disagree.
pub(super) fn array_merge_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [left, right] = call.operands.as_slice() else {
        return Some(format!(
            "array_merge takes two arrays, got {} operands",
            call.operands.len()
        ));
    };
    let mut elements = Vec::new();
    for (operand, side) in [(left, "left"), (right, "right")] {
        let Some(value) = function.value(*operand) else {
            return Some(format!("array_merge {side} is missing from the value table"));
        };
        if value.ir_type != IrType::Heap(IrHeapKind::Array) {
            return Some(format!(
                "array_merge {side} is {:?}, expected an indexed array",
                value.ir_type
            ));
        }
        let PhpType::Array(element) = value.php_type.codegen_repr() else {
            return Some(format!(
                "array_merge {side} is {:?}, expected an indexed array",
                value.php_type.codegen_repr()
            ));
        };
        // `Void` is what an empty literal's `Never` normalizes to: no element is ever read.
        if !matches!(
            element.codegen_repr(),
            PhpType::Int | PhpType::Str | PhpType::Float | PhpType::Bool | PhpType::Mixed | PhpType::Void
        ) {
            return Some(format!(
                "array_merge has no lowered element copy for {element:?}"
            ));
        }
        elements.push(element.codegen_repr());
    }
    // An empty operand carries no elements, so its type never has to agree.
    let concrete: Vec<_> = elements
        .iter()
        .filter(|element| **element != PhpType::Void)
        .collect();
    if concrete.len() == 2 && concrete[0] != concrete[1] {
        return Some(format!(
            "array_merge operands disagree on element storage: {:?} and {:?}",
            concrete[0], concrete[1]
        ));
    }
    None
}

/// Returns the cell tag and source slot stride for slicing an array, or `None` when this target
/// has no lowered layout for that element type.
///
/// A NEGATIVE tag marks a source whose slots already hold cells: `__rt_array_slice` shares those
/// with an incref instead of copying a payload.
pub(super) fn array_slice_element_shape(element: &PhpType) -> Option<(i64, i64)> {
    Some(match element.codegen_repr() {
        PhpType::Int => (0, 8),
        PhpType::Str => (1, 16),
        PhpType::Float => (2, 8),
        PhpType::Bool => (3, 8),
        PhpType::Mixed => (-1, 16),
        // `Void` is what `Never` — the element type of a literal `[]` — normalizes to. There is
        // nothing to copy, so the tag and stride are never read.
        PhpType::Void => (0, 8),
        _ => return None,
    })
}

/// Validates `array_slice`'s shape: an indexed array, an int offset, and an optional int length.
///
/// The `preserve_keys` form is NOT lowered: it answers a hash whose keys are the source's
/// positions, which is a different result type from the reindexed list this produces.
pub(super) fn array_slice_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let (array, offset, length) = match call.operands.as_slice() {
        [array, offset] => (array, offset, None),
        [array, offset, length] => (array, offset, Some(length)),
        other => {
            return Some(format!(
                "array_slice takes an array, an offset and an optional length, got {} operands",
                other.len()
            ))
        }
    };
    let Some(array_value) = function.value(*array) else {
        return Some("array is missing from the value table".to_string());
    };
    if array_value.ir_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "array_slice takes an indexed array, got {:?}",
            array_value.ir_type
        ));
    }
    let PhpType::Array(element) = array_value.php_type.codegen_repr() else {
        return Some(format!(
            "array_slice takes an indexed array, got {:?}",
            array_value.php_type.codegen_repr()
        ));
    };
    if array_slice_element_shape(&element).is_none() {
        return Some(format!(
            "array_slice has no lowered element copy for {:?}",
            element
        ));
    }
    for (operand, what) in [Some(offset), length].into_iter().zip(["offset", "length"]) {
        let Some(operand) = operand else { continue };
        let Some(value) = function.value(*operand) else {
            return Some(format!("array_slice {what} is missing from the value table"));
        };
        if value.ir_type != IrType::I64
            || !matches!(value.php_type.codegen_repr(), PhpType::Int)
        {
            return Some(format!(
                "array_slice {what} is {:?}/{:?}, expected I64/Int",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    // THE RESULT, which used to go unchecked entirely. The lowering builds either a boxed
    // `array<mixed>` or a verbatim copy of the source's own slots, and the EIR's element type
    // has to be the one it actually builds — a call whose result the checker refined to
    // something else read back as raw cell ADDRESSES, silently.
    if array_slice_result_mode(function, call).is_none() {
        let result = call.result_type;
        return Some(format!(
            "array_slice answers array<mixed> or a verbatim copy of a non-owning element type; \
             a {result:?}/{:?} result matches neither",
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// How `array_slice` must build its result: `true` for the VERBATIM copy, `false` for boxing.
///
/// The verbatim copy is admitted only when the result carries the SOURCE's own element type and
/// that type owns nothing — an int, float or bool slot is a flat word, so copying the window is
/// a `memory.copy` with no refcount to adjust. A `Str` or container element would need per-slot
/// persisting or increfs, so those keep the boxing path and therefore require a `mixed` result.
pub(super) fn array_slice_result_mode(function: &Function, call: &Instruction) -> Option<bool> {
    if call.result.is_none() {
        // A discarded result has nothing to decode, so either shape is harmless.
        return Some(false);
    }
    if call.result_type != IrType::Heap(IrHeapKind::Array) {
        return None;
    }
    let PhpType::Array(result_element) = call.result_php_type.codegen_repr() else {
        return None;
    };
    if result_element.codegen_repr() == PhpType::Mixed {
        return Some(false);
    }
    let source_element = call
        .operands
        .first()
        .and_then(|id| function.value(*id))
        .map(|value| value.php_type.codegen_repr())
        .and_then(|php| match php {
            PhpType::Array(element) => Some(element.codegen_repr()),
            _ => None,
        })?;
    let verbatim = source_element == result_element.codegen_repr()
        && matches!(
            source_element,
            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
        );
    verbatim.then_some(true)
}

/// Lowers `count($array)` to the container header's element count at `[ptr + 0]`.
pub(super) fn lower_count(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let container = operand(inst, 0)?;
    if let PhpType::Object(class_name) = ctx.value_php_type(container)?.codegen_repr() {
        return lower_countable_count(ctx, inst, container, &class_name);
    }
    let boxed = matches!(
        ctx.function.value(container).map(|v| v.ir_type),
        Some(IrType::Heap(IrHeapKind::Mixed))
    );
    ctx.emit_load_value(container)?;
    if boxed {
        // A cell PROVEN to hold a container reads through it directly; any other boxed value
        // needs the runtime, which dispatches on the tag and raises PHP's TypeError for a
        // value that has no count.
        if crate::codegen_wasm::capability::value_is_container_by_construction(ctx.function, container) {
            ctx.fb.ins(
                "(i32.wrap_i64 (i64.load offset=8))",
                "the container the cell holds",
            );
        } else {
            ctx.fb
                .ins("call $__rt_mixed_count", "PHP's count() of a boxed value");
            return store_result(ctx, inst);
        }
    }
    // A raw container pointer can be 0 now that a missed `array<array<…>>` element reads as
    // null with the element's own non-null type. Loading the length off it reads address 0 and
    // answers whatever is there — measured `4295050542` where php-src raises
    // `TypeError: count(): Argument #1 ($value) must be of type Countable|array, null given`
    // and terminates. Boxing the null and handing it to `__rt_mixed_count` reaches that exact
    // message rather than restating it here; the helper exits, so nothing follows.
    let container_ptr = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
    ctx.fb.ins(
        &format!("local.tee {}", container_ptr),
        "keep the container pointer for the null test",
    );
    ctx.fb.ins(
        &format!("(if (i32.eqz (local.get {}))", container_ptr),
        "count() of a null is a PHP TypeError, not a header load",
    );
    ctx.fb.ins(
        "(then (drop (call $__rt_mixed_count (call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0))))))",
        "php-src's own count() diagnostic for null",
    );
    ctx.fb.ins("i64.load", "container element count @ +0");
    store_result(ctx, inst)
}
