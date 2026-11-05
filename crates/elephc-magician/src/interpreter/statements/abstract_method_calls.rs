//! Purpose:
//! Rejects calls on abstract methods of native-factory-created instances.
//!
//! Called from:
//! - Eval method expression setup and the native EIR lookup bridge.
//!
//! Key details:
//! - Errors precede argument expressions and preserve declaring-class visibility.
//! - Concrete receivers return before method metadata lookup.

use super::*;
use crate::interpreter::reflection::EVAL_REFLECTION_CLASS_FLAG_ABSTRACT;

/// Resolves only abstractness, leaving ordinary dispatch and argument binding untouched.
pub(in crate::interpreter) fn eval_abstract_instance_method_guard(
    object: RuntimeCellHandle, method_name: &str,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Ok(identity) = values.object_identity(object) else { return Ok(()); };
    let metadata = if let Some(class) = context.dynamic_object_class(identity) {
        if !class.is_abstract() { return Ok(()); }
        let name = class.name().to_string();
        eval_dynamic_method_for_call(&name, method_name, context)
            .filter(|(_, method)| method.is_abstract())
            .map(|(declaring, method)| (declaring, method.name().to_string(), method.visibility()))
    } else {
        let Ok(name) = runtime_object_class_name(object, values) else { return Ok(()); };
        let Ok(Some(flags)) = values.reflection_class_flags(&name) else { return Ok(()); };
        if flags & EVAL_REFLECTION_CLASS_FLAG_ABSTRACT == 0 { return Ok(()); }
        eval_aot_method_dispatch_metadata_in_hierarchy(&name, method_name, context, values)?
            .filter(|(_, _, _, abstract_method)| *abstract_method)
            .map(|(declaring, visibility, _, _)| (declaring, method_name.to_string(), visibility))
    };
    if let Some((declaring, method, visibility)) = metadata {
        if validate_eval_member_access(&declaring, visibility, context).is_err() {
            return eval_throw_method_access_error(&declaring, &method, visibility, context, values);
        }
        return eval_throw_abstract_method_call_error(&declaring, &method, context, values);
    }
    Ok(())
}
