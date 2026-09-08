//! Purpose:
//! Proves ownership of concrete EIR object returns for call cleanup and boxing.
//!
//! Called from:
//! - Runtime callable wrappers and Mixed argument cleanup.
//!
//! Key details:
//! - Payload identity is not ownership. Unproven or inconsistent return paths
//!   remain Unknown rather than being inferred from PHP signatures alone.

use crate::ir::{Function, Immediate, Op, Ownership, Terminator, ValueDef};
use super::*;

/// Reference contract established by every value-returning path of a callee.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::codegen) enum ObjectReturnOwnership {
    Owned,
    Borrowed,
    Unknown,
}

/// Classifies explicit owners and unchanged by-value object parameter borrows.
pub(in crate::codegen) fn object_return_ownership(function: &Function) -> ObjectReturnOwnership {
    if !matches!(function.return_php_type, PhpType::Object(_))
        || function.signature.as_ref().is_some_and(|sig| sig.by_ref_return)
    {
        return ObjectReturnOwnership::Unknown;
    }
    let mut summary = None;
    for block in &function.blocks {
        let Some(Terminator::Return { value: Some(value) }) = &block.terminator else { continue; };
        let value = &function.values[value.as_raw() as usize];
        let current = if value.ownership == Ownership::Owned {
            ObjectReturnOwnership::Owned
        } else if unchanged_object_parameter(function, value.def) {
            ObjectReturnOwnership::Borrowed
        } else {
            ObjectReturnOwnership::Unknown
        };
        if summary.is_some_and(|previous| previous != current) {
            return ObjectReturnOwnership::Unknown;
        }
        summary = Some(current);
    }
    summary.unwrap_or(ObjectReturnOwnership::Unknown)
}

/// Proves a raw object comes directly from a parameter slot never written by the body.
fn unchanged_object_parameter(function: &Function, definition: ValueDef) -> bool {
    let ValueDef::Instruction { inst, .. } = definition else { return false; };
    let instruction = &function.instructions[inst.as_raw() as usize];
    let (Op::LoadLocal, Some(Immediate::LocalSlot(slot))) = (instruction.op, &instruction.immediate)
        else { return false; };
    let local = &function.locals[slot.as_raw() as usize];
    let is_parameter = function.params.iter().any(|param| {
        !param.by_ref && matches!(param.php_type, PhpType::Object(_))
            && local.name.as_deref() == Some(param.name.as_str())
    });
    is_parameter && !function.instructions.iter().any(|instruction| {
        matches!(instruction.op, Op::StoreLocal | Op::StoreRefCell | Op::UnsetLocal)
            && instruction.immediate == Some(Immediate::LocalSlot(*slot))
    })
}

/// Proves the actual callee ABI or every return creates a box retaining the raw argument.
pub(super) fn direct_mixed_return_owns_object_reference(
    ctx: &FunctionContext<'_>, argument: ValueId, result: ValueId,
) -> bool {
    let ValueDef::Instruction { inst, .. } = ctx.function.values[result.as_raw() as usize].def
        else { return false; };
    let call = &ctx.function.instructions[inst.as_raw() as usize];
    let (Op::Call, Some(Immediate::Data(data))) = (call.op, &call.immediate)
        else { return false; };
    let Some(function) = ctx.function_name_data(*data).ok().and_then(|name| ctx.function_by_name(name))
        else { return false; };
    if function.return_php_type != PhpType::Mixed || function.flags.by_ref_return {
        return false;
    }
    // Parameter storage can widen after caller-side signatures were inferred.
    // Consult the compiled ABI: boxing an Object into a by-value Mixed argument
    // retains the object, independently of the raw owner held by the caller.
    if call.operands.iter().position(|value| *value == argument)
        .and_then(|index| function.params.get(index))
        .is_some_and(|parameter| !parameter.by_ref && parameter.php_type == PhpType::Mixed)
    {
        return true;
    }
    let mut saw_return = false;
    for block in &function.blocks {
        let Some(Terminator::Return { value: Some(value) }) = &block.terminator else { continue; };
        saw_return = true;
        let ValueDef::Instruction { inst, .. } = function.values[value.as_raw() as usize].def
            else { return false; };
        let boxing = &function.instructions[inst.as_raw() as usize];
        if boxing.op != Op::MixedBox || !boxing.operands.first().is_some_and(|operand| {
            matches!(function.values[operand.as_raw() as usize].php_type, PhpType::Object(_))
        }) {
            return false;
        }
    }
    saw_return
}

/// Resolves a direct call's callee before consulting its EIR return contract.
pub(super) fn direct_object_return_ownership(
    ctx: &FunctionContext<'_>,
    result: ValueId,
) -> ObjectReturnOwnership {
    let ValueDef::Instruction { inst, .. } = ctx.function.values[result.as_raw() as usize].def
        else { return ObjectReturnOwnership::Unknown; };
    let instruction = &ctx.function.instructions[inst.as_raw() as usize];
    match instruction.op {
        Op::Call => {
            let Some(Immediate::Data(data)) = &instruction.immediate
                else { return ObjectReturnOwnership::Unknown; };
            ctx.function_name_data(*data).ok().and_then(|name| ctx.function_by_name(name))
                .map(object_return_ownership).unwrap_or(ObjectReturnOwnership::Unknown)
        }
        Op::StaticMethodCall => static_object_return_ownership(ctx, instruction),
        Op::MethodCall => virtual_object_return_ownership(ctx, instruction),
        _ => ObjectReturnOwnership::Unknown,
    }
}

/// Requires agreement among all compiled implementations reachable through a receiver type.
fn virtual_object_return_ownership(
    ctx: &FunctionContext<'_>,
    instruction: &Instruction,
) -> ObjectReturnOwnership {
    let Some(receiver) = instruction.operands.first() else { return ObjectReturnOwnership::Unknown; };
    let Ok(PhpType::Object(base)) = ctx.value_php_type(*receiver)
        else { return ObjectReturnOwnership::Unknown; };
    let Ok(method) = method_name_data(ctx, instruction) else { return ObjectReturnOwnership::Unknown; };
    let key = php_symbol_key(method);
    let mut summary = None;
    for (name, info) in &ctx.module.class_infos {
        let mut ancestor = name.as_str();
        while ancestor != base.trim_start_matches('\\') {
            let Some(parent) = ctx.module.class_infos.get(ancestor).and_then(|info| info.parent.as_deref())
                else { break; };
            ancestor = parent;
        }
        if ancestor != base.trim_start_matches('\\') { continue; }
        let owner = info.method_impl_classes.get(&key).unwrap_or(name);
        let current = ctx.module.class_methods.iter().find(|function| {
            !function.flags.is_static && function.name.rsplit_once("::")
                .is_some_and(|(class, method)| class == owner && php_symbol_key(method) == key)
        }).map(object_return_ownership).unwrap_or(ObjectReturnOwnership::Unknown);
        if summary.is_some_and(|previous| previous != current) {
            return ObjectReturnOwnership::Unknown;
        }
        summary = Some(current);
    }
    summary.unwrap_or(ObjectReturnOwnership::Unknown)
}

/// Resolves explicit static and lexical parent calls with the backend's target rules.
fn static_object_return_ownership(
    ctx: &FunctionContext<'_>,
    instruction: &Instruction,
) -> ObjectReturnOwnership {
    let Ok(target) = method_name_data(ctx, instruction) else { return ObjectReturnOwnership::Unknown; };
    let Ok((receiver_label, method)) = parse_static_method_target(target)
        else { return ObjectReturnOwnership::Unknown; };
    if is_late_bound_static_receiver(receiver_label) { return ObjectReturnOwnership::Unknown; }
    let Ok(receiver) = resolve_static_method_receiver(ctx, receiver_label)
        else { return ObjectReturnOwnership::Unknown; };
    let Some(info) = ctx.module.class_infos.get(&receiver)
        else { return ObjectReturnOwnership::Unknown; };
    let key = php_symbol_key(method);
    let is_static = info.static_methods.contains_key(&key);
    let owners = if is_static { &info.static_method_impl_classes } else { &info.method_impl_classes };
    let owner = owners.get(&key).unwrap_or(&receiver);
    ctx.module.class_methods.iter().find(|function| {
        function.flags.is_static == is_static && function.name.rsplit_once("::")
            .is_some_and(|(class, name)| class == owner && php_symbol_key(name) == key)
    }).map(object_return_ownership).unwrap_or(ObjectReturnOwnership::Unknown)
}
