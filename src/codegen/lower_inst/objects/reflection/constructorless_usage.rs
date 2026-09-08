//! Purpose:
//! Detects ReflectionClass values used only for constructorless allocation.
//!
//! Called from:
//! - Reflection owner allocation dispatch.
//!
//! Key details:
//! - Tracks EIR aliases, locals, properties, and terminator escapes conservatively.

use super::*;
/// Returns the canonical reflected class-like name without collecting its member graph.
pub(super) fn reflection_class_reflected_name(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<Option<String>> {
    let Some(class_operand) = inst.operands.first().copied() else {
        return Ok(None);
    };
    let reflected_class = const_string_or_class_operand(ctx, class_operand, "ReflectionClass")?;
    if let Some((class_name, _)) = resolve_reflection_class(ctx, &reflected_class) {
        return Ok(Some(class_name.to_string()));
    }
    if let Some(interface_name) = resolve_reflection_interface(ctx, &reflected_class) {
        return Ok(Some(interface_name.to_string()));
    }
    if let Some(trait_name) = resolve_reflection_trait(ctx, &reflected_class) {
        return Ok(Some(trait_name.to_string()));
    }
    Ok(None)
}
/// `newInstanceWithoutConstructor()`'s lowered `__name` read and constructorless allocation.
pub(super) fn function_only_uses_reflection_class_for_constructorless_allocation(
    ctx: &FunctionContext<'_>,
) -> bool {
    let roots = ctx
        .function
        .instructions
        .iter()
        .filter(|candidate| reflection_class_object_new_result(ctx, candidate).is_some())
        .filter_map(|candidate| candidate.result)
        .collect::<std::collections::HashSet<_>>();
    if roots.is_empty() {
        return false;
    }

    let mut aliases = roots;
    let mut slots = std::collections::HashSet::<LocalSlotId>::new();
    loop {
        let previous_alias_count = aliases.len();
        let previous_slot_count = slots.len();
        for candidate in &ctx.function.instructions {
            if reflection_alias_operation(candidate.op)
                && candidate
                    .operands
                    .first()
                    .is_some_and(|value| aliases.contains(value))
            {
                if let Some(result) = candidate.result {
                    aliases.insert(result);
                }
            }
            if candidate.op == Op::StoreLocal
                && candidate
                    .operands
                    .first()
                    .is_some_and(|value| aliases.contains(value))
            {
                if let Some(Immediate::LocalSlot(slot)) = candidate.immediate.as_ref() {
                    slots.insert(*slot);
                }
            }
            if candidate.op == Op::LoadLocal {
                if let Some(Immediate::LocalSlot(slot)) = candidate.immediate.as_ref() {
                    if slots.contains(slot) {
                        if let Some(result) = candidate.result {
                            aliases.insert(result);
                        }
                    }
                }
            }
        }
        if aliases.len() == previous_alias_count && slots.len() == previous_slot_count {
            break;
        }
    }

    if reflection_candidate_slots_escape(ctx, &slots, &aliases) {
        return false;
    }

    let mut name_values = std::collections::HashSet::<ValueId>::new();
    for candidate in &ctx.function.instructions {
        if !candidate
            .operands
            .iter()
            .any(|operand| aliases.contains(operand))
        {
            continue;
        }
        match candidate.op {
            Op::Acquire | Op::Move | Op::Borrow | Op::EnsureOwned | Op::Release | Op::Nop => {}
            Op::StoreLocal => {
                let Some(Immediate::LocalSlot(slot)) = candidate.immediate.as_ref() else {
                    return false;
                };
                if !slots.contains(slot) {
                    return false;
                }
            }
            Op::PropGet => {
                if reflection_data_name(ctx, candidate) != Some("__name") {
                    return false;
                }
                let Some(result) = candidate.result else {
                    return false;
                };
                name_values.insert(result);
            }
            _ => return false,
        }
    }
    if function_terminators_use_values(ctx, &aliases) {
        return false;
    }
    if name_values.is_empty() {
        // Static newInstanceWithoutConstructor() calls lower directly and no longer read __name.
        // An otherwise ownership-only reflector can therefore use the compact owner safely.
        return ctx
            .function
            .instructions
            .iter()
            .any(|candidate| candidate.op == Op::ObjectNewWithoutConstructor);
    }
    reflection_name_values_only_feed_constructorless_allocation(ctx, name_values)
}

/// Returns the result of a fixed ReflectionClass allocation instruction.
fn reflection_class_object_new_result(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Option<ValueId> {
    if inst.op != Op::ObjectNew {
        return None;
    }
    let Immediate::Data(class_id) = inst.immediate.as_ref()? else {
        return None;
    };
    let class_name = ctx
        .module
        .data
        .class_names
        .get((*class_id).as_raw() as usize)?;
    (php_symbol_key(class_name.trim_start_matches('\\')) == "reflectionclass")
        .then_some(inst.result?)
}

/// Returns true for ownership-only EIR operations that preserve a value's identity.
fn reflection_alias_operation(op: Op) -> bool {
    matches!(op, Op::Acquire | Op::Move | Op::Borrow | Op::EnsureOwned)
}

/// Returns true when a tracked ReflectionClass local is overwritten or aliased elsewhere.
fn reflection_candidate_slots_escape(
    ctx: &FunctionContext<'_>,
    slots: &std::collections::HashSet<LocalSlotId>,
    aliases: &std::collections::HashSet<ValueId>,
) -> bool {
    for candidate in &ctx.function.instructions {
        match candidate.immediate.as_ref() {
            Some(Immediate::LocalSlot(slot)) if slots.contains(slot) => match candidate.op {
                Op::LoadLocal | Op::UnsetLocal | Op::ReleaseLocalSlot => {}
                Op::StoreLocal
                    if candidate
                        .operands
                        .first()
                        .is_some_and(|value| aliases.contains(value)) => {}
                _ => return true,
            },
            Some(Immediate::LocalSlotPair { first, second })
                if slots.contains(first) || slots.contains(second) =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// Resolves an EIR data-string immediate when present.
fn reflection_data_name<'a>(
    ctx: &'a FunctionContext<'_>,
    inst: &Instruction,
) -> Option<&'a str> {
    let Immediate::Data(data) = inst.immediate.as_ref()? else {
        return None;
    };
    ctx.module
        .data
        .strings
        .get((*data).as_raw() as usize)
        .map(String::as_str)
}

/// Returns true when any block terminator exposes one of the tracked values.
fn function_terminators_use_values(
    ctx: &FunctionContext<'_>,
    values: &std::collections::HashSet<ValueId>,
) -> bool {
    ctx.function.blocks.iter().any(|block| {
        block
            .terminator
            .as_ref()
            .is_some_and(|terminator| terminator_uses_values(terminator, values))
    })
}

/// Returns true when one terminator consumes any tracked value.
fn terminator_uses_values(
    terminator: &Terminator,
    values: &std::collections::HashSet<ValueId>,
) -> bool {
    match terminator {
        Terminator::Br { args, .. } => args.iter().any(|value| values.contains(value)),
        Terminator::CondBr {
            cond,
            then_args,
            else_args,
            ..
        } => {
            values.contains(cond)
                || then_args.iter().any(|value| values.contains(value))
                || else_args.iter().any(|value| values.contains(value))
        }
        Terminator::Switch {
            scrutinee,
            cases,
            default_args,
            ..
        } => {
            values.contains(scrutinee)
                || cases
                    .iter()
                    .flat_map(|case| case.args.iter())
                    .any(|value| values.contains(value))
                || default_args.iter().any(|value| values.contains(value))
        }
        Terminator::Return { value } => value.is_some_and(|value| values.contains(&value)),
        Terminator::Throw { value } => values.contains(value),
        Terminator::GeneratorSuspend {
            key,
            value,
            resume_args,
            ..
        } => {
            key.is_some_and(|value| values.contains(&value))
                || value.is_some_and(|value| values.contains(&value))
                || resume_args.iter().any(|value| values.contains(value))
        }
        Terminator::Fatal { .. } | Terminator::Unreachable => false,
    }
}

/// Returns true when reflected-name values flow exclusively into constructorless allocation.
fn reflection_name_values_only_feed_constructorless_allocation(
    ctx: &FunctionContext<'_>,
    mut name_values: std::collections::HashSet<ValueId>,
) -> bool {
    loop {
        let previous_count = name_values.len();
        for candidate in &ctx.function.instructions {
            if reflection_alias_operation(candidate.op)
                && candidate
                    .operands
                    .first()
                    .is_some_and(|value| name_values.contains(value))
            {
                if let Some(result) = candidate.result {
                    name_values.insert(result);
                }
            }
        }
        if name_values.len() == previous_count {
            break;
        }
    }

    let mut constructorless_calls = 0usize;
    for candidate in &ctx.function.instructions {
        if !candidate
            .operands
            .iter()
            .any(|operand| name_values.contains(operand))
        {
            continue;
        }
        match candidate.op {
            Op::Acquire | Op::Move | Op::Borrow | Op::EnsureOwned | Op::Release | Op::Nop => {}
            Op::DynamicObjectNewWithoutConstructorMixed => constructorless_calls += 1,
            _ => return false,
        }
    }
    constructorless_calls > 0 && !function_terminators_use_values(ctx, &name_values)
}
