//! Purpose:
//! Output builtins: `print_r`, `var_dump`, output buffering and the `json_encode` admission predicate.
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.

use super::*;

/// Lowers `print_r($value, $return = false)` through the runtime renderer.
///
/// The value crosses as a boxed Mixed cell — the transfer layer boxes a concrete
/// operand and hands back the cell it allocated, which is freed once the call
/// returns. The helper answers a boxed cell either way: the rendered string in
/// capture mode, `true` in echo mode. An unused result (echo-mode statement) is
/// dropped so the operand stack stays balanced.
pub(super) fn lower_print_r(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let value = operand(inst, 0)?;
    let allocated = crate::codegen_wasm::transfer::emit_push_builtin_argument(
        ctx,
        value,
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
    )?;
    if inst.operands.len() >= 2 {
        ctx.emit_load_value(operand(inst, 1)?)?;
    } else {
        ctx.fb.ins("i64.const 0", "the return flag defaults to false");
    }
    ctx.fb.ins("call $__rt_print_r", "render php's print_r form");
    if let Some((cell, _kind)) = allocated {
        ctx.fb.ins(
            &format!("local.get {cell}"),
            "the cell synthesized for the argument",
        );
        ctx.fb
            .ins("call $__rt_decref_any", "free it now the call is done");
    }
    if inst.result.is_none() {
        ctx.fb.ins("drop", "echo-mode statement: the true is unused");
        return Ok(());
    }
    // A site the checker narrowed to a BOOL result (echo mode, proven) takes the bool
    // out of the cell; storing the cell pointer into an i64 bool would be nonsense the
    // validator cannot see.
    if inst.result_type == IrType::I64 {
        let cell = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
        ctx.fb.ins(&format!("local.tee {cell}"), "the boxed answer");
        ctx.fb
            .ins("call $__rt_mixed_cast_bool", "unbox the bool answer");
        ctx.fb.ins(&format!("local.get {cell}"), "the now-redundant cell");
        ctx.fb.ins("call $__rt_decref_any", "free it");
    }
    // And a site narrowed to a STRING result (capture mode with a literal true flag)
    // takes the rendered string out — `__rt_mixed_cast_string` hands back a persisted
    // copy the result owns, so the cell can be released here.
    if inst.result_type == IrType::Str {
        let cell = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
        let sptr = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
        let slen = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
        ctx.fb.ins(&format!("local.tee {cell}"), "the boxed answer");
        ctx.fb.ins(
            "call $__rt_mixed_cast_string",
            "unbox the rendered string (persisted copy)",
        );
        ctx.fb.ins(&format!("local.set {slen}"), "rendered length");
        ctx.fb.ins(&format!("local.set {sptr}"), "rendered pointer");
        ctx.fb.ins(&format!("local.get {cell}"), "the now-redundant cell");
        ctx.fb.ins("call $__rt_decref_any", "free it");
        ctx.fb.ins(&format!("local.get {sptr}"), "rendered pointer");
        ctx.fb.ins(&format!("local.get {slen}"), "rendered length");
        ctx.fb
            .ins("i64.extend_i32_u", "widen the length to the Str shape");
    }
    store_result(ctx, inst)
}

/// Validates the exact `print_r` subset the renderer implements: scalars, arrays and
/// hashes (nested to any depth), null, and boxed Mixed values. An OBJECT — statically
/// typed here, or hiding inside a Mixed container at runtime — is not rendered; the
/// static shape refuses it and the runtime fails loudly (code 15) rather than printing
/// a truncated form php would never emit.
pub(super) fn print_r_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    if call.operands.is_empty() || call.operands.len() > 2 {
        return Some(format!(
            "print_r takes a value and an optional return flag, got {} operands",
            call.operands.len()
        ));
    }
    let Some(value) = call.operands.first().and_then(|id| function.value(*id)) else {
        return Some("print_r value is missing from the value table".to_string());
    };
    if matches!(value.ir_type, IrType::Heap(IrHeapKind::Object))
        || matches!(
            value.php_type.codegen_repr(),
            PhpType::Object(_) | PhpType::Callable | PhpType::Resource(_)
        )
    {
        return Some("print_r of an object is not implemented on wasm32-wasi".to_string());
    }
    if call.operands.len() == 2 {
        let Some(flag) = call.operands.get(1).and_then(|id| function.value(*id)) else {
            return Some("print_r return flag is missing from the value table".to_string());
        };
        if flag.ir_type != IrType::I64 {
            return Some(format!(
                "print_r return flag storage {:?} is not the expected I64",
                flag.ir_type
            ));
        }
    }
    // Mixed is the contract; a site the checker narrowed to a BOOL (echo mode, proven)
    // or a STRING (capture mode, literal flag) is served by the lowering's unbox
    // adaptations.
    if call.result.is_some()
        && !matches!(
            call.result_type,
            IrType::Heap(IrHeapKind::Mixed) | IrType::I64 | IrType::Str
        )
    {
        return Some(format!(
            "print_r result storage {:?} is not the expected Heap(Mixed), I64 bool, or Str",
            call.result_type
        ));
    }
    None
}

/// Validates `var_dump`, which this target serves for SCALARS only.
///
/// A float is refused too: php uses serialize_precision for `var_dump`, which is not the
/// precision `echo` uses, so the existing formatter would print a different number.
///
/// php renders an array or an object with a recursive, indented walk that this backend has no
/// property-walking machinery for. Printing something that merely looks close would be worse
/// than refusing, so only a boxed value whose tag is provably scalar is admitted — and the tag
/// is only provable when the EIR type says so.
pub(super) fn var_dump_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
) -> Option<String> {
    if !module.functions.iter().any(|candidate| candidate.flags.is_main) {
        return Some("var_dump writes through the command runtime".to_string());
    }
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "var_dump of {} values is not supported on wasm32-wasi (one at a time)",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("operand is missing from the value table".to_string());
    };
    // A container reaches the call as a RAW POINTER, a scalar as its own representation, and a
    // union already boxed; the lowering boxes whatever arrives, so what has to be proven here
    // is the TYPE rather than the storage.
    if !matches!(
        value.ir_type,
        IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Array | IrHeapKind::Hash)
            | IrType::I64
            | IrType::Str
    ) {
        return Some(format!(
            "var_dump operand {:?}/{:?} has no boxing on this target",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    // The UNNORMALIZED type, because `codegen_repr` collapses every union to `Mixed`: the two
    // shapes this serves — `int|false` and `string|false` — would be indistinguishable from a
    // genuine `mixed` after that collapse, and a genuine `mixed` must keep refusing.
    if !php_type_is_dumpable_scalar(&value.php_type) {
        return Some(format!(
            "var_dump of {:?} on wasm32-wasi (only int/bool/string/null render here)",
            value.php_type
        ));
    }
    None
}

/// Whether every value this type can hold is one `__rt_var_dump` renders.
///
/// A union is dumpable only when EVERY arm is: `int|false` is, `int|array` is not.
pub(super) fn php_type_is_dumpable_scalar(php: &PhpType) -> bool {
    match php {
        PhpType::Int
        | PhpType::Bool
        | PhpType::False
        | PhpType::Str
        | PhpType::Void => true,
        // A container renders through the same generic walk the dynamic `foreach` uses, so
        // whichever storage it holds at runtime is read correctly. Its ELEMENTS still have to
        // be renderable: an array of objects is refused, not printed half-way.
        PhpType::Array(element) => php_type_is_dumpable_scalar(element),
        PhpType::AssocArray { key, value } => {
            php_type_is_dumpable_scalar(key) && php_type_is_dumpable_scalar(value)
        }
        PhpType::Union(arms) => !arms.is_empty() && arms.iter().all(php_type_is_dumpable_scalar),
        // A named alias of a scalar still dumps as that scalar.
        other if *other != other.codegen_repr() => {
            php_type_is_dumpable_scalar(&other.codegen_repr())
        }
        _ => false,
    }
}

/// Validates one output-buffering call.
///
/// `ob_start` is the only one with operands: an optional callable handler and an optional chunk
/// size. The other four take none, which is what makes the whole family a stack operation rather
/// than something addressed by handle.
pub(super) fn output_buffer_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    if !module.functions.iter().any(|candidate| candidate.flags.is_main) {
        return Some("output buffering writes through the command runtime".to_string());
    }
    if target == RuntimeFnId::ObStart {
        if call.operands.len() > 2 {
            return Some(format!(
                "ob_start takes at most a handler and a chunk size, got {} operands",
                call.operands.len()
            ));
        }
        // A handler must be a CALLABLE DESCRIPTOR: this target invokes it through the closure
        // ladder, which is keyed on that descriptor. A string function name would need the
        // runtime name lookup this backend does not have.
        if let Some(handler) = call.operands.first() {
            let Some(value) = function.value(*handler) else {
                return Some("ob_start handler is missing from the value table".to_string());
            };
            if value.ir_type != IrType::I64
                || value.php_type.codegen_repr() != PhpType::Callable
            {
                return Some(format!(
                    "ob_start handler {:?}/{:?} is not a callable descriptor",
                    value.ir_type,
                    value.php_type.codegen_repr()
                ));
            }
        }
        if let Some(chunk) = call.operands.get(1) {
            let Some(value) = function.value(*chunk) else {
                return Some("ob_start chunk size is missing from the value table".to_string());
            };
            if value.ir_type != IrType::I64 {
                return Some(format!(
                    "ob_start chunk size {:?} is not an int",
                    value.ir_type
                ));
            }
        }
        return None;
    }
    if !call.operands.is_empty() {
        return Some(format!(
            "{target:?} takes no operands, got {}",
            call.operands.len()
        ));
    }
    None
}

/// Lowers one output-buffering call.
///
/// `ob_start`'s two optional operands are supplied here when the call site omits them: zero for
/// the handler means "no handler", and zero for the chunk size means "no auto-flush", which is
/// what php's defaults are.
pub(super) fn lower_output_buffer(
    ctx: &mut FnCtx,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Result<()> {
    if target == RuntimeFnId::ObStart {
        if inst.operands.is_empty() {
            ctx.fb.ins("i64.const 0", "no handler");
        } else {
            ctx.emit_load_value(operand(inst, 0)?)?;
        }
        if inst.operands.len() >= 2 {
            ctx.emit_load_value(operand(inst, 1)?)?;
        } else {
            ctx.fb.ins("i64.const 0", "no chunk size: never auto-flushes");
        }
        ctx.fb.ins("call $__rt_ob_start", "push a buffer level");
        return store_result(ctx, inst);
    }
    let helper = match target {
        RuntimeFnId::ObGetClean => "call $__rt_ob_get_clean",
        RuntimeFnId::ObEndFlush => "call $__rt_ob_end_flush",
        RuntimeFnId::ObEndClean => "call $__rt_ob_end_clean",
        _ => "call $__rt_ob_get_status",
    };
    ctx.fb.ins(helper, "output-buffer stack operation");
    store_result(ctx, inst)
}

/// Validates `json_encode`, which this target serves for values it can prove encodable.
///
/// php's second argument is `$flags`, and every flag changes the OUTPUT — `JSON_PRETTY_PRINT`
/// its whitespace, `JSON_UNESCAPED_SLASHES` its escaping — so only the default form is admitted
/// rather than accepting a flag and ignoring it.
pub(super) fn json_encode_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
) -> Option<String> {
    if !module.functions.iter().any(|candidate| candidate.flags.is_main) {
        return Some("json_encode writes through the command runtime".to_string());
    }
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "json_encode with {} operands on wasm32-wasi (flags change the output, so only the \
             default form is served)",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("json_encode operand is missing from the value table".to_string());
    };
    if !crate::codegen_wasm::json::type_is_encodable(module, &value.php_type) {
        return Some(format!(
            "json_encode of {:?} on wasm32-wasi (an object whose class or runtime subclasses \
             have a public property with no JSON form, or a typed property that may be left \
             uninitialized and php would skip, including through a JsonSerializable object or a \
             `mixed` value)",
            value.php_type
        ));
    }
    None
}
