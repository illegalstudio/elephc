//! Purpose:
//! Eval registry entry and implementation for `get_debug_type`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//! - `crate::interpreter::context_calls::execute_context_object_class_name`, when compiled
//!   code with a persistent eval context asks for the debug type of an object or boxed value.
//!
//! Key details:
//! - Scalars use php-src's zend type names (`int`, `float`, `bool`, `null`, ...).
//! - An eval-declared anonymous class is named from its parent, else its first declared
//!   interface, else `class`, followed by `@anonymous`; a compiled anonymous class reads the
//!   same spelling from the generated `_class_debug_name_entries` table.
//! - A resource reads `resource (<type>)`, or `resource (closed)` once it has been closed.

use super::super::super::*;

eval_builtin! {
    contract: "get_debug_type",
    area: Types,
    direct: GetDebugType,
    values: GetDebugType,
}

/// Evaluates PHP `get_debug_type()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_get_debug_type(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_get_debug_type_result(value, context, values)
}

/// Names one boxed runtime value the way PHP's `get_debug_type()` does.
pub(in crate::interpreter) fn eval_get_debug_type_result(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = match values.type_tag(value)? {
        EVAL_TAG_NULL => "null",
        EVAL_TAG_BOOL => "bool",
        EVAL_TAG_INT => "int",
        EVAL_TAG_FLOAT => "float",
        EVAL_TAG_STRING => "string",
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => "array",
        EVAL_TAG_CALLABLE => "Closure",
        EVAL_TAG_RESOURCE => {
            let name = match eval_resource_type_name(value, context, values)? {
                "Unknown" => String::from("resource (closed)"),
                type_name => format!("resource ({type_name})"),
            };
            return values.string(&name);
        }
        EVAL_TAG_OBJECT => {
            let identity = values.object_identity(value)?;
            if let Some(class) = context.dynamic_object_class(identity) {
                let name = eval_class_debug_name(class);
                return values.string(&name);
            }
            if let Some(name) = context.dynamic_object_class_name(identity) {
                return values.string(&name);
            }
            return values.object_debug_class_name(value);
        }
        _ => "null",
    };
    values.string(name)
}

/// Returns php-src's debug spelling of one eval-declared class name.
fn eval_class_debug_name(class: &EvalClass) -> String {
    if !class.is_anonymous() {
        return class.name().trim_start_matches('\\').to_string();
    }
    let prefix = class
        .parent()
        .or_else(|| class.interfaces().first().map(String::as_str))
        .unwrap_or("class");
    format!("{}@anonymous", prefix.trim_start_matches('\\'))
}
