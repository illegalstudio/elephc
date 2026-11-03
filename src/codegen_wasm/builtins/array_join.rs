//! Purpose:
//! `implode` and `explode`: lowering and the admission predicates for string/array conversion.
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.

use super::*;

/// The `__rt_fail` code PHP's empty-separator `explode` `ValueError` reports under.
pub(super) const EXPLODE_EMPTY_SEP_FAILURE_CODE: i32 = 13;

/// Lowers `explode` in its two-argument form.
///
/// PHP refuses an empty separator outright — unlike `str_pad`'s empty pad, which only raises when
/// it would be used — because there is no split it could mean and the scan would not advance. The
/// `$limit` form is refused: a positive limit caps the count with the remainder in the last
/// element, a negative one drops elements from the END, and zero behaves as one, which is three
/// different rules rather than a default.
pub(super) fn lower_explode(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let pptr = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
    let plen = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins(&format!("local.set {plen}"), "spill separator length");
    ctx.fb.ins(&format!("local.set {pptr}"), "spill separator pointer");
    ctx.fb.ins(&format!("local.get {plen}"), "separator length");
    ctx.fb.ins("i64.eqz", "an empty separator?");
    ctx.fb.ins("if", "explode() has no split an empty separator could mean");
    crate::codegen_wasm::inst::emit_runtime_failure(
        ctx,
        EXPLODE_EMPTY_SEP_FAILURE_CODE,
        "explode() empty separator",
    );
    ctx.fb.ins("end", "end empty-separator guard");
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(&format!("local.get {pptr}"), "separator pointer");
    ctx.fb.ins(&format!("local.get {plen}"), "separator length");
    ctx.fb.ins("call $__rt_explode", "split into a fresh indexed array");
    store_result(ctx, inst)
}

/// Validates `explode`: a separator and a subject string, an indexed string array out.
pub(super) fn explode_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [separator, subject] = call.operands.as_slice() else {
        return Some(format!(
            "expected a separator and a subject with no limit, got {} operands",
            call.operands.len()
        ));
    };
    for operand in [separator, subject] {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
            return Some(format!(
                "explode takes strings, got {:?}/{:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none() || call.result_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "explode result {:?} is not an indexed array",
            call.result_type
        ));
    }
    if !matches!(&call.result_php_type, PhpType::Array(element) if **element == PhpType::Str) {
        return Some(format!(
            "explode must produce exactly array<string>, got {:?}",
            call.result_php_type
        ));
    }
    None
}

/// Lowers `implode` over an indexed array of strings.
///
/// PHP also accepts an array of ints or mixed values, joining their string coercions, and a
/// one-argument form whose glue is empty. Only the two-argument all-string shape is admitted:
/// coercing elements would need the per-tag conversions this backend keeps fail-closed, and
/// admitting the shorter form would mean inventing an empty glue with no literal behind it.
pub(super) fn lower_implode(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let array = operand(inst, 1)?;
    // A string element needs no conversion; an int, a float or a Mixed cell does, and the
    // converted piece is owned so the shared helper releases it.
    let boxed_container = matches!(
        ctx.function.value(array).map(|v| v.ir_type),
        Some(IrType::Heap(IrHeapKind::Mixed))
    );
    let has_main = ctx.module.functions.iter().any(|function| function.flags.is_main);
    if boxed_container
        && (has_main
            || !crate::codegen_wasm::capability::value_is_container_by_construction(ctx.function, array))
    {
        // A declared `array` — list or hash — and even an `array_map` result: a callback that
        // returns a string answers a list of string slots, not of cells. The generic walk reads
        // any layout (see `__rt_implode_boxed`); only a reactor, which has no walk, keeps the
        // cell-list path below.
        ctx.emit_load_value(array)?;
        ctx.emit_load_value(operand(inst, 0)?)?;
        ctx.fb.ins("call $__rt_implode_boxed", "join a boxed php array of any layout");
        return store_result(ctx, inst);
    }
    let element_kind = if boxed_container {
        // The admitted boxes come from `array_map`/`array_filter`, whose runtime always
        // answers a fresh Mixed-CELL array; the capability rule is what guarantees it.
        Some(1)
    } else {
        match ctx.function.value(array).map(|v| &v.php_type) {
            Some(PhpType::Array(element)) => match **element {
                PhpType::Int => Some(0),
                PhpType::Mixed => Some(1),
                PhpType::Float => Some(2),
                _ => None,
            },
            _ => None,
        }
    };
    if boxed_container {
        // The cell's low word IS the array pointer; nothing else in it is read.
        ctx.emit_load_value(array)?;
        ctx.fb.ins(
            "(i32.wrap_i64 (i64.load offset=8))",
            "the array the cell holds (tag 4 low word)",
        );
    } else {
        ctx.emit_load_value(array)?;
    }
    ctx.emit_load_value(operand(inst, 0)?)?;
    match element_kind {
        Some(kind) => {
            ctx.fb.ins(
                &format!("i32.const {kind}"),
                "0 renders an int slot, 1 casts a Mixed cell, 2 casts a float slot",
            );
            ctx.fb.ins(
                "call $__rt_implode_owned",
                "each element converts as an explicit (string) cast would",
            );
        }
        None => {
            ctx.fb.ins("call $__rt_implode", "join the elements with the glue");
        }
    }
    store_result(ctx, inst)
}

/// Validates `implode`: a glue string and an indexed array of strings, a string out.
pub(super) fn implode_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [glue, array] = call.operands.as_slice() else {
        return Some(format!(
            "expected a glue and an array, got {} operands",
            call.operands.len()
        ));
    };
    let Some(glue_value) = function.value(*glue) else {
        return Some("glue is missing from the value table".to_string());
    };
    if glue_value.ir_type != IrType::Str || glue_value.php_type.codegen_repr() != PhpType::Str {
        return Some(format!(
            "implode glue is {:?}/{:?}, expected Str/Str",
            glue_value.ir_type,
            glue_value.php_type.codegen_repr()
        ));
    }
    let Some(array_value) = function.value(*array) else {
        return Some("array is missing from the value table".to_string());
    };
    // A boxed container answers here too, when the box is PROVABLY one: `array_map` and
    // `array_filter` type their result `mixed` because of their CALLBACK, never because the
    // value might not be an array — so there is no `TypeError` path to get wrong.
    if array_value.ir_type == IrType::Heap(IrHeapKind::Mixed)
        && crate::codegen_wasm::capability::value_is_container_by_construction(function, *array)
    {
        return None;
    }
    // A declared `array` — php's exact array contract, boxed — is an array by the checker's own
    // guarantee, so it has no `TypeError` path either; its layout is only known at runtime.
    if array_value.ir_type == IrType::Heap(IrHeapKind::Mixed) && array_value.php_type.is_php_array()
    {
        return None;
    }
    if array_value.ir_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "implode takes an indexed array, got {:?}",
            array_value.ir_type
        ));
    }
    // The element type has to be exactly string: `__rt_array_get_str` reads a slot as a
    // (pointer, length) pair, and a slot holding an int or a boxed Mixed is a different layout.
    // `Never` is admitted alongside it because it is the type of an array proven to have no
    // elements — `implode(",", [])` — where the element read never happens and the answer is the
    // empty string. Refusing it would reject a shape whose result cannot be got wrong.
    //
    // `Mixed` elements are admitted through a different helper: PHP converts each with the same
    // rule as an explicit `(string)` cast, measured element by element, so the conversion is one
    // this backend already implements exactly. That is what makes `implode(",", array_slice(…))`
    // reachable, since a slice always answers `array<mixed>`.
    if !matches!(
        &array_value.php_type,
        PhpType::Array(element)
            if matches!(
                **element,
                PhpType::Str
                    | PhpType::Never
                    | PhpType::Int
                    | PhpType::Float
                    | PhpType::Mixed
            )
    ) {
        return Some(format!(
            "implode elements must be exactly string, got {:?}",
            array_value.php_type
        ));
    }
    if call.result.is_none()
        || call.result_type != IrType::Str
        || call.result_php_type.codegen_repr() != PhpType::Str
    {
        return Some(format!(
            "implode result {:?}/{:?} is not the expected Str/Str",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}
