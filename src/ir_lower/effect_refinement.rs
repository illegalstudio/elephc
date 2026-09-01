//! Purpose:
//! Refines call and read effects after the complete checked EIR module has been lowered.
//! Computes closed-world callable summaries and attaches them to direct and instance calls.
//!
//! Called from:
//! - `crate::ir_lower::program::lower()` before final EIR validation.
//!
//! Key details:
//! - Direct-call summaries use a monotone fixed point so pure recursion remains precise.
//! - Instance dispatch unions every concrete implementation reachable from the receiver type.
//! - Mixed, external, eval-defined, or otherwise unresolved targets retain opcode defaults.

use std::collections::HashMap;

use crate::ir::{
    Effects, Function, Immediate, Module, Op, Terminator, ValueDef, ValueId,
};
use crate::names::php_symbol_key;
use crate::types::{ClassInfo, PhpType};

const MAX_EFFECT_ITERATIONS: usize = 64;

/// Read-only module facts used while summarizing and rewriting instructions.
struct RefinementContext<'a> {
    data: &'a crate::ir::DataPool,
    classes: &'a HashMap<String, ClassInfo>,
    summaries: &'a HashMap<String, Effects>,
    /// True when the runtime eval bridge can register subclasses absent from `classes`.
    has_dynamic_class_barrier: bool,
}

/// Refines every eligible instruction from whole-module callable and class metadata.
pub(super) fn refine_module(module: &mut Module) {
    let has_dynamic_class_barrier = module.required_runtime_features.eval_bridge;
    let mut summaries = all_functions(module)
        .map(|function| (callable_key(&function.name), Effects::PURE))
        .collect::<HashMap<_, _>>();

    for iteration in 0..MAX_EFFECT_ITERATIONS {
        let context = RefinementContext {
            data: &module.data,
            classes: &module.class_infos,
            summaries: &summaries,
            has_dynamic_class_barrier,
        };
        let next = all_functions(module)
            .map(|function| {
                (
                    callable_key(&function.name),
                    summarize_function(function, &context),
                )
            })
            .collect::<HashMap<_, _>>();
        if next == summaries {
            summaries = next;
            break;
        }
        summaries = next;
        assert!(
            iteration + 1 < MAX_EFFECT_ITERATIONS,
            "EIR effect refinement did not converge after {MAX_EFFECT_ITERATIONS} iterations"
        );
    }

    let data = &module.data;
    let classes = &module.class_infos;
    let context = RefinementContext {
        data,
        classes,
        summaries: &summaries,
        has_dynamic_class_barrier,
    };
    for function in &mut module.functions {
        refine_function(function, &context);
    }
    for function in &mut module.class_methods {
        refine_function(function, &context);
    }
    for function in &mut module.closures {
        refine_function(function, &context);
    }
    for function in &mut module.fiber_wrappers {
        refine_function(function, &context);
    }
    for function in &mut module.callback_wrappers {
        refine_function(function, &context);
    }
    for function in &mut module.extern_callback_trampolines {
        refine_function(function, &context);
    }
    for function in &mut module.runtime_callable_invokers {
        refine_function(function, &context);
    }
}

/// Iterates every function-like EIR body stored in a module.
fn all_functions(module: &Module) -> impl Iterator<Item = &Function> {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
}

/// Computes the caller-visible union of instruction and terminator effects for one body.
fn summarize_function(function: &Function, context: &RefinementContext<'_>) -> Effects {
    let mut effects = Effects::PURE;
    for instruction in &function.instructions {
        effects |= refined_instruction_effects(function, instruction, context);
    }
    for block in &function.blocks {
        effects |= match block.terminator.as_ref() {
            Some(Terminator::Throw { .. }) => Effects::MAY_THROW | Effects::WRITES_GLOBAL,
            Some(Terminator::Fatal { .. }) => Effects::MAY_FATAL,
            Some(Terminator::GeneratorSuspend { .. }) => {
                Effects::READS_HEAP | Effects::WRITES_HEAP | Effects::MAY_DEOPT
            }
            _ => Effects::PURE,
        };
    }
    effects.difference(Effects::READS_LOCAL | Effects::WRITES_LOCAL)
}

/// Rewrites refinable instructions in one function to their final fixed-point effects.
fn refine_function(function: &mut Function, context: &RefinementContext<'_>) {
    let refined = function
        .instructions
        .iter()
        .map(|instruction| refined_instruction_effects(function, instruction, context))
        .collect::<Vec<_>>();
    for (instruction, effects) in function.instructions.iter_mut().zip(refined) {
        if instruction.op.allows_effect_refinement() {
            instruction.effects = effects;
        }
    }
}

/// Returns the most precise proven effects for one instruction.
fn refined_instruction_effects(
    function: &Function,
    instruction: &crate::ir::Instruction,
    context: &RefinementContext<'_>,
) -> Effects {
    match instruction.op {
        Op::Call => direct_call_effects(instruction, context).unwrap_or(instruction.effects),
        Op::MethodCall | Op::NullsafeMethodCall => {
            instance_call_effects(function, instruction, context).unwrap_or(instruction.effects)
        }
        Op::PropGet | Op::NullsafePropGet => {
            named_property_effects(function, instruction, context).unwrap_or(instruction.effects)
        }
        Op::DynamicPropGet => {
            dynamic_property_effects(function, instruction, context).unwrap_or(instruction.effects)
        }
        _ => instruction.effects,
    }
}

/// Resolves a direct user-function call through the function-name data pool.
fn direct_call_effects(
    instruction: &crate::ir::Instruction,
    context: &RefinementContext<'_>,
) -> Option<Effects> {
    let Immediate::Data(name_id) = instruction.immediate.as_ref()? else {
        return None;
    };
    let name = context.data.function_names.get(name_id.as_raw() as usize)?;
    context.summaries.get(&callable_key(name)).copied()
}

/// Resolves and unions all concrete implementations reachable by one instance call.
fn instance_call_effects(
    function: &Function,
    instruction: &crate::ir::Instruction,
    context: &RefinementContext<'_>,
) -> Option<Effects> {
    let receiver = *instruction.operands.first()?;
    let Immediate::Data(method_id) = instruction.immediate.as_ref()? else {
        return None;
    };
    let method = context.data.strings.get(method_id.as_raw() as usize)?;
    let runtime_classes = receiver_runtime_classes_for_value(function, receiver, context)?;
    let method_key = php_symbol_key(method);
    let mut effects = Effects::READS_HEAP;
    for runtime_class in runtime_classes {
        let class = context.classes.get(&runtime_class)?;
        let implementation = class.method_impl_classes.get(&method_key)?;
        let target = callable_key(&format!("{implementation}::{method_key}"));
        effects |= context.summaries.get(&target).copied()?;
    }
    Some(effects)
}

/// Resolves a named property read, including typed-slot throws and `__get` dispatch.
fn named_property_effects(
    function: &Function,
    instruction: &crate::ir::Instruction,
    context: &RefinementContext<'_>,
) -> Option<Effects> {
    let receiver = *instruction.operands.first()?;
    let Immediate::Data(property_id) = instruction.immediate.as_ref()? else {
        return None;
    };
    let property = context.data.strings.get(property_id.as_raw() as usize)?;
    property_effects_for_value(function, receiver, property, context)
}

/// Resolves a dynamic property read when its property operand is a constant string.
///
/// Runtime-computed names still receive a narrower dynamic contract than the opcode-wide
/// fallback, but retain `may_deopt`, warning, typed-slot throw, and magic-method effects.
fn dynamic_property_effects(
    function: &Function,
    instruction: &crate::ir::Instruction,
    context: &RefinementContext<'_>,
) -> Option<Effects> {
    let receiver = *instruction.operands.first()?;
    if let Some(property) = instruction
        .operands
        .get(1)
        .and_then(|value| constant_string(function, *value, context.data))
    {
        return property_effects_for_value(function, receiver, property, context);
    }

    let runtime_classes = receiver_runtime_classes_for_value(function, receiver, context)?;
    let mut effects = Effects::READS_HEAP | Effects::MAY_WARN | Effects::MAY_DEOPT;
    for runtime_class in runtime_classes {
        let class = context.classes.get(&runtime_class)?;
        if class
            .properties
            .iter()
            .enumerate()
            .any(|(index, (name, _))| class.property_slot_is_declared(index, name))
        {
            effects |= Effects::MAY_THROW;
        }
        if class.methods.contains_key("__get") {
            effects |= method_summary_for_class(context, &runtime_class, "__get")?;
        }
    }
    Some(effects)
}

/// Computes one named property's effects across every concrete receiver implementation.
fn property_effects_for_value(
    function: &Function,
    receiver: ValueId,
    property: &str,
    context: &RefinementContext<'_>,
) -> Option<Effects> {
    let runtime_classes = receiver_runtime_classes_for_value(function, receiver, context)?;
    let mut effects = Effects::READS_HEAP;
    for runtime_class in runtime_classes {
        let class = context.classes.get(&runtime_class)?;
        if let Some((index, (name, _))) = class.visible_property(property) {
            if class.property_slot_is_declared(index, name) {
                effects |= Effects::MAY_THROW;
            }
            continue;
        }
        if class.methods.contains_key("__get") {
            effects |= method_summary_for_class(context, &runtime_class, "__get")?;
        } else {
            effects |= Effects::MAY_WARN;
        }
    }
    Some(effects)
}

/// Resolves an exact fixed construction before falling back to the receiver's PHP type.
fn receiver_runtime_classes_for_value(
    function: &Function,
    receiver: ValueId,
    context: &RefinementContext<'_>,
) -> Option<Vec<String>> {
    if let Some(class_name) = fixed_object_class(function, receiver, context.data) {
        return context
            .classes
            .contains_key(class_name)
            .then(|| vec![class_name.to_string()]);
    }
    if context.has_dynamic_class_barrier {
        return None;
    }
    let receiver_type = function.value(receiver)?.php_type.clone();
    receiver_runtime_classes(&receiver_type, context.classes)
}

/// Traces identity-like ownership instructions to a statically fixed `object_new`.
fn fixed_object_class<'a>(
    function: &Function,
    value: ValueId,
    data: &'a crate::ir::DataPool,
) -> Option<&'a str> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    let instruction = function.instruction(inst)?;
    if matches!(instruction.op, Op::Acquire | Op::Borrow | Op::Move) {
        return fixed_object_class(function, *instruction.operands.first()?, data);
    }
    if !matches!(
        instruction.op,
        Op::ObjectNew | Op::ObjectNewWithoutConstructor
    ) {
        return None;
    }
    let Immediate::Data(data_id) = instruction.immediate.as_ref()? else {
        return None;
    };
    data.class_names
        .get(data_id.as_raw() as usize)
        .map(String::as_str)
}

/// Looks up the effective method implementation and its fixed-point summary.
fn method_summary_for_class(
    context: &RefinementContext<'_>,
    runtime_class: &str,
    method: &str,
) -> Option<Effects> {
    let class = context.classes.get(runtime_class)?;
    let method_key = php_symbol_key(method);
    let implementation = class.method_impl_classes.get(&method_key)?;
    context
        .summaries
        .get(&callable_key(&format!("{implementation}::{method_key}")))
        .copied()
}

/// Expands an object or nullable-object type to all concrete checked runtime classes.
fn receiver_runtime_classes(
    receiver_type: &PhpType,
    classes: &HashMap<String, ClassInfo>,
) -> Option<Vec<String>> {
    let mut bases = Vec::new();
    match receiver_type {
        PhpType::Object(class_name) => bases.push(class_name.trim_start_matches('\\').to_string()),
        PhpType::Union(members) => {
            for member in members {
                match member {
                    PhpType::Object(class_name) => {
                        bases.push(class_name.trim_start_matches('\\').to_string());
                    }
                    PhpType::Void | PhpType::Never => {}
                    _ => return None,
                }
            }
        }
        _ => return None,
    }
    if bases.is_empty() {
        return None;
    }

    let mut runtime_classes = Vec::new();
    for base in bases {
        if !classes.contains_key(&base) {
            return None;
        }
        for (candidate, class) in classes {
            if !class.is_abstract
                && class_is_same_or_subclass(classes, candidate, &base)
                && !runtime_classes.contains(candidate)
            {
                runtime_classes.push(candidate.clone());
            }
        }
    }
    (!runtime_classes.is_empty()).then_some(runtime_classes)
}

/// Returns true when one class is the requested base or inherits from it.
fn class_is_same_or_subclass(
    classes: &HashMap<String, ClassInfo>,
    candidate: &str,
    base: &str,
) -> bool {
    let mut current = Some(candidate);
    while let Some(class_name) = current {
        if class_name == base {
            return true;
        }
        current = classes
            .get(class_name)
            .and_then(|class| class.parent.as_deref());
    }
    false
}

/// Returns a constant string operand from its defining EIR instruction.
fn constant_string<'a>(
    function: &Function,
    value: ValueId,
    data: &'a crate::ir::DataPool,
) -> Option<&'a str> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    let instruction = function.instruction(inst)?;
    if instruction.op != Op::ConstStr {
        return None;
    }
    let Immediate::Data(data_id) = instruction.immediate.as_ref()? else {
        return None;
    };
    data.strings
        .get(data_id.as_raw() as usize)
        .map(String::as_str)
}

/// Normalizes PHP callable names for case-insensitive summary lookup.
fn callable_key(name: &str) -> String {
    php_symbol_key(name.trim_start_matches('\\'))
}
