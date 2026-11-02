//! Purpose:
//! Resolves EIR static-call effects through lexical scope and checked class hierarchies.
//!
//! Called from:
//! - The whole-module effect-refinement fixed point and final instruction rewrite.
//!
//! Key details:
//! - `self` and `parent` select an early-bound implementation.
//! - `static` includes concrete implementations even on abstract classes, except across eval.

use super::{
    callable_key, class_is_same_or_subclass, php_symbol_key, resolved_callable_effects,
    Effects, Function, Immediate, RefinementContext,
};

/// Resolves named and lexical receivers, unioning closed-world late-static implementations.
pub(super) fn static_call_effects(
    function: &Function,
    instruction: &crate::ir::Instruction,
    context: &RefinementContext<'_>,
) -> Option<Effects> {
    let Immediate::Data(name_id) = instruction.immediate.as_ref()? else { return None; };
    let name = context.data.strings.get(name_id.as_raw() as usize)?;
    let (receiver, method) = name.rsplit_once("::")?;
    let receiver = receiver.trim_start_matches('\\');
    let class_name = match receiver {
        "self" | "static" => function.lexical_class.as_deref()?,
        "parent" => context.classes.get(function.lexical_class.as_deref()?)?.parent.as_deref()?,
        _ => receiver,
    };
    let class_key = php_symbol_key(class_name);
    let (class_name, class) = context.classes.iter()
        .find(|(name, _)| php_symbol_key(name) == class_key)?;
    let method_key = php_symbol_key(method);
    if receiver != "static" {
        return implementation_effects(function, instruction, context, class, &method_key,
            matches!(receiver, "self" | "parent"));
    }
    if context.has_dynamic_class_barrier { return None; }
    // With no lexical vtable slot, codegen calls this implementation directly.
    // Private methods are absent from descendant implementation maps.
    if !class.static_vtable_slots.contains_key(&method_key) {
        return implementation_effects(function, instruction, context, class, &method_key, false);
    }

    let mut effects = Effects::PURE;
    let mut found = false;
    for (candidate, class) in context.classes {
        if !class_is_same_or_subclass(context.classes, candidate, class_name) { continue; }
        // Calling an abstract class can select its abstract slot instead of any child body.
        // Keep this failure observable even when every concrete override is pure.
        if class.abstract_static_methods.contains(&method_key) {
            effects |= Effects::READS_HEAP | Effects::MAY_FATAL;
            found = true;
            continue;
        }
        effects |= implementation_effects(function, instruction, context, class, &method_key, false)?;
        found = true;
    }
    found.then_some(effects)
}

/// Includes argument coercions and the actual inherited static or lexical-instance body.
fn implementation_effects(
    function: &Function,
    instruction: &crate::ir::Instruction,
    context: &RefinementContext<'_>,
    class: &crate::types::ClassInfo,
    method: &str,
    lexical_instance: bool,
) -> Option<Effects> {
    let (implementation, parameter_offset) =
        if let Some(implementation) = class.static_method_impl_classes.get(method) {
            (implementation, 0)
        } else if lexical_instance {
            // Codegen supplies `$this` separately; EIR operands contain only user arguments.
            (class.method_impl_classes.get(method)?, 1)
        } else {
            return None;
        };
    let target = callable_key(&format!("{implementation}::{method}"));
    resolved_callable_effects(function, instruction, &target, parameter_offset, context)
}
