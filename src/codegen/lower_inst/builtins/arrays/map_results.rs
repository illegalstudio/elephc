//! Purpose:
//! Array map runtime calls, result typing, and ownership.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;

/// Emits the `array_map()` runtime call for the resolved callback and destination shape.
///
/// The caller has already loaded argument registers 0/1/2 with the callback address, the source
/// container and the callback environment. An indexed destination needs nothing more; a hash
/// destination additionally takes the result-kind selector and the destination `value_type` tag
/// in argument registers 3 and 4, which are untouched by the shared setup above.
pub(super) fn emit_array_map_runtime_call(
    ctx: &mut FunctionContext<'_>,
    callback_elem_ty: &PhpType,
    env_bytes: usize,
    target: ArrayMapTarget,
) -> Result<()> {
    match target {
        ArrayMapTarget::Indexed => {
            abi::emit_call_label(
                ctx.emitter,
                array_map_runtime_label(callback_elem_ty, env_bytes),
            );
        }
        ArrayMapTarget::Hash | ArrayMapTarget::Boxed => {
            let result_kind = hash_map_result_kind(callback_elem_ty, env_bytes);
            let dest_value_tag = runtime_value_tag("array_map", callback_elem_ty)?;
            let kind_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 3);
            let tag_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 4);
            abi::emit_load_int_immediate(ctx.emitter, kind_arg_reg, result_kind as i64);
            abi::emit_load_int_immediate(ctx.emitter, tag_arg_reg, dest_value_tag as i64);
            let helper = if target == ArrayMapTarget::Boxed {
                "__rt_array_map_boxed"
            } else {
                "__rt_hash_map"
            };
            abi::emit_call_label(ctx.emitter, helper);
        }
    }
    Ok(())
}

/// Returns where `__rt_hash_map` must look for the callback result, and who owns it.
///
/// The mapping mirrors `array_map_runtime_label` one-for-one, so the hash destination inherits
/// exactly the callback ABI contracts the indexed helpers already implement:
/// `__rt_array_map_mixed` and `__rt_array_map` leave a pointer-sized result in the integer
/// return register, `__rt_array_map_str` leaves a BORROWED string pair, and
/// `__rt_array_map_str_owned` leaves an already-owned one.
pub(super) fn hash_map_result_kind(callback_elem_ty: &PhpType, env_bytes: usize) -> HashMapResultKind {
    if callback_elem_ty == &PhpType::Str {
        if env_bytes == 0 {
            HashMapResultKind::Persist
        } else {
            HashMapResultKind::Owned
        }
    } else {
        HashMapResultKind::Scalar
    }
}

/// Returns the source VALUE type when `__rt_hash_map` can map a hash faithfully.
///
/// Only `Int`, `Bool` and `Str` values are accepted. A `Mixed`-valued hash is refused ON
/// PURPOSE, for the same reason `hash_flip_source_value_type` refuses one: an associative array
/// built entry by entry currently mis-tags heterogeneous values UPSTREAM of this lowering —
/// `$a["k1"] = 1; $a["k2"] = "s";` stores the string payload under the int tag, which
/// `var_dump()` of the source array already renders as `int(<pointer>)` with no `array_map()`
/// involved. `__rt_hash_map` picks the callback ARGUMENT ABI from that per-entry tag, so
/// accepting a Mixed-valued source would feed a raw string pointer to a callback expecting a
/// boxed cell. Refusing keeps the failure honest until the hash-construction path tags Mixed
/// values correctly.
pub(super) fn hash_map_source_value_type(source_ty: &PhpType) -> Result<PhpType> {
    match source_ty {
        PhpType::AssocArray { value, .. } => {
            let value = value.codegen_repr();
            if matches!(value, PhpType::Int | PhpType::Bool | PhpType::Str) {
                return Ok(value);
            }
            Err(CodegenIrError::unsupported(format!(
                "array_map for associative value PHP type {:?}",
                value
            )))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_map for PHP type {:?}",
            other
        ))),
    }
}

/// Stamps or widens the `array_map()` result for the destination shape that produced it.
///
/// An indexed destination keeps the existing element stamp / `__rt_array_to_mixed` widening.
/// A hash destination is already stamped by `__rt_hash_new` with the tag
/// `emit_array_map_runtime_call` passed, so it only needs the Mixed widening when the EIR result
/// slot is wider than what the callback actually produced.
pub(super) fn finish_array_map_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: ArrayMapTarget,
    callback_elem_ty: &PhpType,
    result_elem_ty: &PhpType,
) -> Result<()> {
    match target {
        ArrayMapTarget::Boxed => {
            let valid = ctx.next_label("array_map_boxed_valid");
            abi::emit_branch_if_int_result_nonzero(ctx.emitter, &valid);
            crate::codegen::lower_inst::exceptions::emit_type_error(
                ctx, "array_map(): Argument #2 ($array) must be of type array",
            );
            ctx.emitter.label(&valid);
            box_hash_result_for_mixed_builtin(ctx, inst, &PhpType::Mixed);
        }
        ArrayMapTarget::Indexed => {
            normalize_indexed_array_result(ctx, "array_map", callback_elem_ty, result_elem_ty)?;
            box_array_result_for_mixed_builtin(ctx, inst, result_elem_ty);
        }
        ArrayMapTarget::Hash => {
            if result_elem_ty == &PhpType::Mixed && callback_elem_ty != &PhpType::Mixed {
                if ctx.emitter.target.arch == Arch::X86_64 {
                    ctx.emitter.instruction("mov rdi, rax");                    // pass the mapped hash to the Mixed-entry conversion helper
                }
                abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            }
            box_hash_result_for_mixed_builtin(ctx, inst, result_elem_ty);
        }
    }
    Ok(())
}

/// Boxes an associative-array result when the EIR builtin result slot is Mixed-like.
///
/// The hash counterpart of `box_array_result_for_mixed_builtin`, and it transfers ownership for
/// exactly the same reason — see that function for the full retain/release argument. The hash
/// arrives fresh from `__rt_hash_new` inside `__rt_hash_map`, so the reference in the result
/// register is untracked and must be handed to the Mixed cell rather than shared with it.
///
/// The key type is not part of the boxed payload (the boxing emitters only read the runtime tag
/// for `AssocArray`, and the matching release only needs it to select `__rt_decref_hash`), so
/// `Str` stands in for it.
pub(super) fn box_hash_result_for_mixed_builtin(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    value_ty: &PhpType,
) {
    if inst.result.is_some()
        && matches!(
            inst.result_php_type.codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
    {
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(value_ty.clone()),
            },
        );
    }
}

/// Returns the runtime helper selected for an `array_map()` callback result shape.
pub(super) fn array_map_runtime_label(callback_elem_ty: &PhpType, env_bytes: usize) -> &'static str {
    if callback_elem_ty == &PhpType::Mixed {
        return "__rt_array_map_mixed";
    }
    if callback_elem_ty == &PhpType::Str {
        if env_bytes == 0 {
            "__rt_array_map_str"
        } else {
            "__rt_array_map_str_owned"
        }
    } else {
        "__rt_array_map"
    }
}
