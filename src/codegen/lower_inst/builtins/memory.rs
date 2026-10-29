//! Purpose:
//! Lowers typed native heap metric calls to allocation-free runtime helpers.
//!
//! Called from:
//! - `runtime_functions::group_11` for the five memory operations.
//!
//! Key details:
//! - Optional boolean arguments have already passed shared call normalization.

use super::*;
use crate::ir::RuntimeFnId;

/// Materializes the optional arena flag and calls the matching native heap helper.
pub(crate) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Result<()> {
    let (symbol, has_flag) = match target {
        RuntimeFnId::MemoryGetUsage => ("__rt_memory_get_usage", true),
        RuntimeFnId::MemoryGetPeakUsage => ("__rt_memory_get_peak_usage", true),
        RuntimeFnId::MemoryResetPeakUsage => ("__rt_memory_reset_peak_usage", false),
        RuntimeFnId::MemoryGetAllocations => ("__rt_memory_get_allocations", false),
        RuntimeFnId::MemoryGetFrees => ("__rt_memory_get_frees", false),
        _ => return Err(CodegenIrError::invalid_module("expected a memory runtime operation")),
    };
    shared::ensure_arg_count_between(inst, symbol, 0, usize::from(has_flag))?;
    if has_flag {
        if let Some(&flag) = inst.operands.first() {
            match ctx.raw_value_php_type(flag)?.codegen_repr() {
                PhpType::Mixed | PhpType::Union(_) => validate_dynamic_flag(ctx, flag, target)?,
                PhpType::Bool | PhpType::False | PhpType::Int | PhpType::Float | PhpType::Str
                | PhpType::Void | PhpType::Never | PhpType::TaggedScalar => {}
                // EIR can refine a mixed-returning call back to its concrete container type.
                ty => {
                    emit_static_flag_type_error(ctx, flag, target, &ty)?;
                    return store_if_result(ctx, inst);
                }
            }
            predicates::emit_value_truthiness(ctx, flag, "memory real_usage")?;
            super::super::move_int_result_to_first_arg(ctx);
        } else {
            let reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
            abi::emit_load_int_immediate(ctx.emitter, reg, 0);
        }
    }
    abi::emit_call_label(ctx.emitter, symbol);
    store_if_result(ctx, inst)
}

/// Rejects container flags before PHP truthiness can silently accept them.
fn validate_dynamic_flag(
    ctx: &mut FunctionContext<'_>,
    flag: ValueId,
    target: RuntimeFnId,
) -> Result<()> {
    ctx.load_value_to_result(flag)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let valid = ctx.next_label("memory_scalar_flag");
    // -- weak bool parameters admit scalar tags zero through three and null --
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #3");                              // ordinary scalars can be converted without a type error
            ctx.emitter.instruction(&format!("b.ls {valid}"));                  // preserve integer, string, float, and boolean inputs
            ctx.emitter.instruction("cmp x0, #8");                              // null is accepted by weak builtin boolean parameters
            ctx.emitter.instruction(&format!("b.eq {valid}"));                  // leave null conversion to the ordinary truthiness path
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 3");                              // scalar tags zero through three satisfy weak bool parameters
            ctx.emitter.instruction(&format!("jbe {valid}"));                   // reject containers before testing their truthiness
            ctx.emitter.instruction("cmp rax, 8");                              // null also coerces to false for weak builtin calls
            ctx.emitter.instruction(&format!("je {valid}"));                    // reload and coerce the original borrowed scalar below
        }
    }
    emit_dynamic_flag_type_error(ctx, target);
    ctx.emitter.label(&valid);
    Ok(())
}

/// Names the rejected runtime tag without converting or allocating on the valid path.
fn emit_dynamic_flag_type_error(ctx: &mut FunctionContext<'_>, target: RuntimeFnId) {
    let tag_reg = abi::int_result_reg(ctx.emitter);
    let cases = super::super::objects::RUNTIME_TAGS.into_iter()
        .filter(|(tag, _)| *tag > 3 && *tag != 8)
        .map(|(tag, name)| (tag, name, ctx.next_label("memory_invalid_flag")))
        .collect::<Vec<_>>();
    for (tag, _, label) in &cases {
        super::super::enums::emit_mixed_tag_branch(ctx, tag_reg, i64::from(*tag), label);
    }
    emit_flag_type_error(ctx, target, "mixed");
    for (tag, name, label) in cases {
        ctx.emitter.label(&label);
        if tag == super::super::objects::OBJECT_TAG {
            let payload = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
            abi::emit_reg_move(ctx.emitter, tag_reg, payload);
            emit_object_flag_type_error(ctx, target);
        } else {
            emit_flag_type_error(ctx, target, name);
        }
    }
}

/// Reports EIR-refined containers using the same PHP names as the dynamic tag guard.
fn emit_static_flag_type_error(
    ctx: &mut FunctionContext<'_>, flag: ValueId, target: RuntimeFnId, ty: &PhpType,
) -> Result<()> {
    if matches!(ty, PhpType::Object(_)) {
        ctx.load_value_to_result(flag)?;
        emit_object_flag_type_error(ctx, target);
    } else {
        let given = match ty {
            PhpType::Callable => "Closure".to_string(),
            PhpType::Resource(_) => "resource".to_string(),
            _ => crate::types::diagnostic_names::php_type_name(ty),
        };
        emit_flag_type_error(ctx, target, &given);
    }
    Ok(())
}

/// Includes the concrete runtime class when the rejected flag is an object.
fn emit_object_flag_type_error(ctx: &mut FunctionContext<'_>, target: RuntimeFnId) {
    let result = abi::int_result_reg(ctx.emitter);
    super::super::runtime_class_messages::emit_runtime_class_name_to_string_result(ctx, result, "object");
    super::super::runtime_class_messages::emit_concat_static_prefix(ctx, &flag_type_error_prefix(target));
    super::super::runtime_class_messages::emit_concat_static_suffix(ctx, " given");
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    exceptions::emit_type_error_from_string_result(ctx);
}

/// Uses PHP's complete parameter diagnostic for a statically named rejected type.
fn emit_flag_type_error(ctx: &mut FunctionContext<'_>, target: RuntimeFnId, given: &str) {
    exceptions::emit_type_error(ctx, &format!("{}{given} given", flag_type_error_prefix(target)));
}

/// Shares the parameter prefix between static and runtime class diagnostics.
fn flag_type_error_prefix(target: RuntimeFnId) -> String {
    format!("{}(): Argument #1 ($real_usage) must be of type bool, ", target.as_eir())
}

/// Lowers `__elephc_set_memory_limit(bytes)` by clamping the request to the reserved arena
/// capacity and publishing it as the allocator's live ceiling.
///
/// `-1` is PHP's "unlimited" spelling. The heap arena is a fixed reservation, so unlimited
/// means "the whole arena" here, exactly like the compile-time `--ini memory_limit=-1` path.
/// A positive request is clamped to `_heap_max` for the same reason.
pub(crate) fn lower_set_memory_limit(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    shared::ensure_arg_count(inst, "__elephc_set_memory_limit", 1)?;
    ctx.load_value_to_result(expect_operand(inst, 0)?)?;
    ctx.emitter.comment("__elephc_set_memory_limit()");
    let unlimited = ctx.next_label("memory_limit_unlimited");
    let publish = ctx.next_label("memory_limit_publish");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmn x0, #1");                              // is the request exactly -1 (PHP's unlimited spelling)?
            ctx.emitter.instruction(&format!("b.eq {unlimited}"));             // -1 selects the reserved arena capacity
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_heap_max", 0);
            ctx.emitter.instruction("cmp x0, x9");                              // compare the request against the reserved capacity
            ctx.emitter.instruction("csel x0, x0, x9, lo");                     // keep the smaller of the request and the arena
            ctx.emitter.instruction(&format!("b {publish}"));                  // publish the clamped ceiling
            ctx.emitter.label(&unlimited);
            abi::emit_load_symbol_to_reg(ctx.emitter, "x0", "_heap_max", 0);   // unlimited is the whole reserved arena
            ctx.emitter.label(&publish);
            abi::emit_store_reg_to_symbol(ctx.emitter, "x0", "_heap_limit", 0);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, -1");                            // is the request exactly -1 (PHP's unlimited spelling)?
            ctx.emitter.instruction(&format!("je {unlimited}"));               // -1 selects the reserved arena capacity
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_heap_max", 0);
            ctx.emitter.instruction("cmp rax, r10");                           // compare the request against the reserved capacity
            ctx.emitter.instruction("cmovg rax, r10");                         // keep the smaller of the request and the arena
            ctx.emitter.instruction(&format!("jmp {publish}"));                // publish the clamped ceiling
            ctx.emitter.label(&unlimited);
            abi::emit_load_symbol_to_reg(ctx.emitter, "rax", "_heap_max", 0);  // unlimited is the whole reserved arena
            ctx.emitter.label(&publish);
            abi::emit_store_reg_to_symbol(ctx.emitter, "rax", "_heap_limit", 0);
        }
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    store_if_result(ctx, inst)
}
