//! Purpose:
//! Materializes owned reference candidates from immutable native property declarations.
//!
//! Called from:
//! - The typed EIR ReferencePropertyCoerce operation in generated declaration callbacks.
//!
//! Key details:
//! - Context bit zero carries assignment-site strictness, independently of declaration scope.
//! - The original RHS is borrowed and every accepted result owns one boxed value.
//! - Declaration metadata never depends on a live receiver or a borrowed Rust registry entry.

use super::*;
use crate::codegen::lower_inst::{expect_operand, store_if_result};

/// Emits one boxed declaration coercion, selecting strictness from its native context.
pub(in crate::codegen::lower_inst) fn lower(
    ctx: &mut FunctionContext<'_>, inst: &Instruction, class_id: u64, index: u32,
    is_static: bool,
) -> Result<()> {
    let (class_name, info) = ctx.module.class_infos.iter()
        .find(|(_, info)| info.class_id == class_id)
        .ok_or_else(|| CodegenIrError::invalid_module("unknown reference validator class"))?;
    let properties = if is_static { &info.static_properties } else { &info.properties };
    let (property, declared) = properties.get(index as usize)
        .ok_or_else(|| CodegenIrError::invalid_module("unknown reference validator property"))?;
    let declared = declared.clone();
    let mut slot = PropertySlot {
        class_name: class_name.clone(), property: property.clone(), php_type: declared.clone(),
        offset: 0, is_declared: true, is_packed: false, width: None, is_reference: true,
        abstract_setter_type_error_prefix: None, reference_declaration: None,
    };
    let weak = declaration_actions(ctx, &slot)?;
    let strict = strict_actions(&declared, &weak);
    // Candidate storage is uniformly boxed. Diagnostics retain the source declaration.
    slot.reference_declaration = Some(declared);
    slot.php_type = PhpType::Mixed;
    let value = expect_operand(inst, 1)?;
    let weak_label = ctx.next_label("reference_validator_weak");
    let done = ctx.next_label("reference_validator_done");
    ctx.load_value_to_result(expect_operand(inst, 0)?)?;
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, result, result, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("and x0, x0, #1"),             // assignment-site strictness is context bit zero
        Arch::X86_64 => ctx.emitter.instruction("and eax, 1"),                  // assignment-site strictness is context bit zero
    }
    abi::emit_branch_if_int_result_zero(ctx.emitter, &weak_label);
    emit_candidate(ctx, value, &slot, &strict)?;
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&weak_label);
    emit_candidate(ctx, value, &slot, &weak)?;
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Converts ordinary store acceptance into a precise, always-boxed PHP candidate plan.
fn declaration_actions(ctx: &FunctionContext<'_>, slot: &PropertySlot) -> Result<Vec<TagAction>> {
    if slot.php_type == PhpType::False {
        return Ok(actions_for(|tag| if tag == BOOL_TAG {
            TagAction::FalseOrScalar(None)
        } else { TagAction::Reject }));
    }
    let mut actions = plan_tag_actions(ctx, slot)?
        .ok_or_else(|| CodegenIrError::unsupported("unsupported native reference declaration"))?;
    for ((tag, _), action) in RUNTIME_TAGS.iter().zip(actions.iter_mut()) {
        let scalar = match slot.php_type {
            PhpType::Int => Some((0, PhpType::Int)),
            PhpType::Float => Some((2, PhpType::Float)),
            PhpType::Bool => Some((3, PhpType::Bool)),
            PhpType::Str => Some((1, PhpType::Str)),
            _ => None,
        };
        if let Some((exact, target)) = scalar {
            if *tag != exact && matches!(action,
                TagAction::Accept | TagAction::AcceptIntoInt | TagAction::NumericString)
            {
                *action = TagAction::CoerceScalar {
                    require_numeric_string: *tag == STRING_TAG
                        && matches!(target, PhpType::Int | PhpType::Float),
                    target, fallback: None,
                };
            }
        }
    }
    Ok(actions)
}

/// Strict assignments preserve exact members and permit only PHP's int-to-float widening.
fn strict_actions(declared: &PhpType, weak: &[TagAction]) -> Vec<TagAction> {
    let members = match declared {
        PhpType::Union(members) => members.iter().flat_map(expand_union_member).collect(),
        other => expand_union_member(other),
    };
    let exact = |tag| members.iter().any(|member| match (tag, member) {
        (_, PhpType::Mixed) => true,
        (0, PhpType::Int) | (1, PhpType::Str) | (2, PhpType::Float)
        | (3, PhpType::Bool) | (8, PhpType::Void | PhpType::Never) => true,
        (4 | 5, PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable) => true,
        _ => false,
    });
    RUNTIME_TAGS.iter().zip(weak).map(|((tag, _), action)| {
        if exact(*tag) { return TagAction::Accept; }
        if *tag == 0 && members.contains(&PhpType::Float) {
            return TagAction::CoerceScalar {
                target: PhpType::Float, require_numeric_string: false, fallback: None,
            };
        }
        if *tag == BOOL_TAG && members.contains(&PhpType::False) {
            return TagAction::FalseOrScalar(None);
        }
        if let TagAction::ObjectClasses(plan) = action {
            let mut plan = plan.clone();
            plan.stringable.clear();
            return TagAction::ObjectClasses(plan);
        }
        TagAction::Reject
    }).collect()
}

/// Retains an exact accepted RHS; coercion arms already return their independent owner.
fn emit_candidate(
    ctx: &mut FunctionContext<'_>, value: ValueId, slot: &PropertySlot, actions: &[TagAction],
) -> Result<()> {
    let coerced = emit_guard_for_actions(ctx, value, slot, actions)?;
    ctx.load_value_to_result(value)?;
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    if let Some(label) = coerced { ctx.emitter.label(&label); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Strict union planning accepts exact integers before considering float widening.
    #[test]
    fn strict_reference_union_candidates_keep_exact_members_and_false() {
        let weak = actions_for(|_| TagAction::Accept);
        let plans = strict_actions(&PhpType::Union(vec![PhpType::Int, PhpType::Float]), &weak);
        assert!(matches!(plans[0], TagAction::Accept));
        assert!(matches!(plans[1], TagAction::Reject));
        let plans = strict_actions(&PhpType::Float, &weak);
        assert!(matches!(plans[0], TagAction::CoerceScalar { target: PhpType::Float, .. }));
        assert!(matches!(plans[3], TagAction::Reject));
        let plans = strict_actions(&PhpType::Union(vec![PhpType::False, PhpType::Void]), &weak);
        assert!(matches!(plans[3], TagAction::FalseOrScalar(None)));
        assert!(matches!(plans[7], TagAction::Accept));
    }
}
