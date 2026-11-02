//! Purpose:
//! Validates Reflection subclass constructor forwarding before metadata population.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::fixed_new::lower_object_new()`.
//!
//! Key details:
//! - Metadata is populated before user constructors run, so every user constructor in the
//!   inheritance chain must forward the builtin owner's leading parameters unchanged.

use super::super::method_name_data;
use super::*;

/// Rejects Reflection subclass constructors whose parent metadata arguments cannot be proven.
pub(super) fn validate_reflection_subclass_constructor_forwarding(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    owner_class: &str,
    constructor: Option<&ConstructorCallTarget>,
) -> Result<()> {
    let Some(constructor) = constructor else {
        return Err(reflection_constructor_forwarding_error(class_name, owner_class));
    };
    if constructor.impl_class == owner_class {
        return Ok(());
    }
    let required_parent_params = ctx
        .module
        .class_infos
        .get(owner_class)
        .and_then(|info| info.methods.get("__construct"))
        .map(|signature| signature.params.len())
        .ok_or_else(|| reflection_constructor_forwarding_error(class_name, owner_class))?;
    let constructor_key = php_symbol_key("__construct");
    let mut impl_class = constructor.impl_class.clone();
    let mut visited = std::collections::HashSet::new();
    loop {
        if impl_class.eq_ignore_ascii_case(owner_class) {
            return Ok(());
        }
        if !visited.insert(php_symbol_key(&impl_class)) {
            return Err(reflection_constructor_forwarding_error(class_name, owner_class));
        }
        let function = ctx
            .module
            .class_methods
            .iter()
            .find(|function| {
                !function.flags.is_static
                    && function
                        .name
                        .rsplit_once("::")
                        .is_some_and(|(candidate_class, candidate_method)| {
                            candidate_class == impl_class
                                && candidate_method.eq_ignore_ascii_case("__construct")
                        })
            })
            .ok_or_else(|| reflection_constructor_forwarding_error(class_name, owner_class))?;
        let parent_calls = function
            .blocks
            .iter()
            .flat_map(|block| {
                block.instructions.iter().filter_map(move |inst_id| {
                    let instruction = function.instruction(*inst_id)?;
                    if instruction.op != Op::StaticMethodCall {
                        return None;
                    }
                    let target = method_name_data(ctx, instruction).ok()?;
                    target
                        .eq_ignore_ascii_case("parent::__construct")
                        .then_some((block, *inst_id, instruction))
                })
            })
            .collect::<Vec<_>>();
        let [(block, call_id, call)] = parent_calls.as_slice() else {
            return Err(reflection_constructor_forwarding_error(class_name, owner_class));
        };
        if block.id != function.entry || call.operands.len() < required_parent_params {
            return Err(reflection_constructor_forwarding_error(class_name, owner_class));
        }
        if function.params.len() < required_parent_params + 1 {
            return Err(reflection_constructor_forwarding_error(class_name, owner_class));
        }
        let forwarded_slots = function
            .params
            .iter()
            .skip(1)
            .take(required_parent_params)
            .map(|param| {
                function
                    .locals
                    .iter()
                    .find(|local| local.name.as_deref() == Some(param.name.as_str()))
                    .map(|local| local.id)
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| reflection_constructor_forwarding_error(class_name, owner_class))?;
        if call
            .operands
            .iter()
            .take(required_parent_params)
            .zip(forwarded_slots.iter())
            .any(|(operand, expected)| {
                reflection_forwarded_local(function, *operand) != Some(*expected)
            })
        {
            return Err(reflection_constructor_forwarding_error(class_name, owner_class));
        }
        for inst_id in block.instructions.iter().take_while(|inst_id| **inst_id != *call_id) {
            let Some(instruction) = function.instruction(*inst_id) else {
                return Err(reflection_constructor_forwarding_error(class_name, owner_class));
            };
            if matches!(
                instruction.immediate,
                Some(Immediate::LocalSlot(slot)) if forwarded_slots.contains(&slot)
            ) && instruction.op != Op::LoadLocal
            {
                return Err(reflection_constructor_forwarding_error(class_name, owner_class));
            }
            if !matches!(
                instruction.op,
                Op::ConcatReset
                    | Op::LoadLocal
                    | Op::Acquire
                    | Op::Borrow
                    | Op::Move
                    | Op::StoreLocal
                    | Op::PushCallOperandOwner
                    | Op::Nop
            ) {
                return Err(reflection_constructor_forwarding_error(class_name, owner_class));
            }
        }
        let parent_name = ctx
            .module
            .class_infos
            .get(&impl_class)
            .and_then(|info| info.parent.as_deref())
            .ok_or_else(|| reflection_constructor_forwarding_error(class_name, owner_class))?;
        let next_impl = crate::types::constructor_owner(&ctx.module.class_infos, parent_name)
            .and_then(|(_, info)| info.method_impl_classes.get(&constructor_key))
            .cloned()
            .ok_or_else(|| reflection_constructor_forwarding_error(class_name, owner_class))?;
        if next_impl == impl_class {
            return Err(reflection_constructor_forwarding_error(class_name, owner_class));
        }
        impl_class = next_impl;
    }
}

/// Peels ownership-only EIR wrappers to the parameter slot read for one parent argument.
fn reflection_forwarded_local(
    function: &crate::ir::Function,
    mut value: ValueId,
) -> Option<LocalSlotId> {
    loop {
        let value_info = function.value(value)?;
        let ValueDef::Instruction { inst, .. } = value_info.def else {
            return None;
        };
        let instruction = function.instruction(inst)?;
        if instruction.op == Op::LoadLocal {
            return match instruction.immediate {
                Some(Immediate::LocalSlot(slot)) => Some(slot),
                _ => None,
            };
        }
        if !matches!(instruction.op, Op::Acquire | Op::Borrow | Op::Move) {
            return None;
        }
        value = *instruction.operands.first()?;
    }
}

/// Builds the diagnostic for a Reflection subclass constructor Elephc cannot model safely.
fn reflection_constructor_forwarding_error(
    class_name: &str,
    owner_class: &str,
) -> CodegenIrError {
    CodegenIrError::unsupported(format!(
        "Reflection subclass {}::__construct must directly forward the leading {} constructor parameters unchanged to parent::__construct() as its first action",
        class_name, owner_class
    ))
}
