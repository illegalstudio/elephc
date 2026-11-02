//! Purpose:
//! Eval registry entry and implementation for `get_debug_type`.
//!
//! Called from:
//! - `crate::interpreter::builtins::types`.
//!
//! Key details:
//! - PHP 8's spellings, NOT `gettype()`'s legacy ones: `"int"`/`"float"`/`"bool"`/`"null"`,
//!   never `"integer"`/`"double"`/`"boolean"`/`"NULL"` (`gettype(1.0)` is `"double"`;
//!   `get_debug_type(1.0)` is `"float"`, measured).
//! - The object arm names the class (trimmed of its leading `\`) rather than `"object"`;
//!   php truncates that name at the first NUL byte, which only matters for an anonymous class
//!   whose internal name carries a `\0file:line` suffix -- the interpreter's own anonymous
//!   class names (`class@anonymous#evalN`, `crate::parser::state::next_anonymous_class_name`)
//!   never contain one, so the truncation is implemented for parity but is not observable here
//!   today.
//! - An EVAL-DECLARED closure (`function(){}`/`fn()=>1`) is NOT eval's tag 10
//!   (`EVAL_TAG_CALLABLE`) -- `eval_closure_object_expr()`
//!   (`interpreter/expressions/evaluation.rs:634`) materializes it as an ordinary tag-6 object
//!   built with `new_object("stdClass")`, and the SAME identity is registered separately with
//!   `context.register_closure_object_target()`. `get_class()`'s own implementation
//!   (`builtins/symbols/get_class.rs`) resolves the name through
//!   `context.dynamic_object_class_name()`, which checks `closure_objects` before falling back
//!   to the declared class and answers `"Closure"` for exactly this case
//!   (`context/classlike_objects.rs:469`-`:472`); this file copies that same route so the two
//!   builtins cannot silently disagree. Tag 10 is kept as a belt-and-suspenders arm in case a
//!   compiled/AOT closure ever crosses the eval bridge tagged that way -- `sprintf`'s own
//!   numeric-coercion path already answers `"Closure"` for it (`formatting/sprintf_format.rs:237`).
//! - A resource is `"resource (stream)"` while open and `"resource (closed)"` once closed -- a
//!   DIFFERENT spelling from `get_resource_type()`'s `"Unknown"` for the same closed handle.

use super::super::super::*;

eval_builtin! {
    contract: "get_debug_type",
    area: Types,
    direct: GetDebugType,
    values: GetDebugType,
}

/// Evaluates PHP `get_debug_type(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_get_debug_type(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_get_debug_type_result(value, context, values)
        }
        _ => eval_throw_argument_count_error(
            &format!(
                "get_debug_type() expects exactly 1 argument, {} given",
                args.len()
            ),
            context,
            values,
        ),
    }
}

/// Evaluates PHP `get_debug_type(...)` over one already-evaluated argument cell.
pub(in crate::interpreter) fn eval_get_debug_type_values(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [value] => eval_get_debug_type_result(*value, context, values),
        _ => eval_throw_argument_count_error(
            &format!(
                "get_debug_type() expects exactly 1 argument, {} given",
                evaluated_args.len()
            ),
            context,
            values,
        ),
    }
}

/// Resolves the php-8 debug type name for one already-evaluated runtime cell.
fn eval_get_debug_type_result(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    match tag {
        EVAL_TAG_INT => values.string("int"),
        EVAL_TAG_FLOAT => values.string("float"),
        EVAL_TAG_STRING => values.string("string"),
        EVAL_TAG_BOOL => values.string("bool"),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => values.string("array"),
        EVAL_TAG_CALLABLE => values.string("Closure"),
        EVAL_TAG_OBJECT => {
            let class_name = eval_get_debug_type_object_class_name(value, context, values)?;
            let truncated = eval_debug_type_truncate_at_nul(&class_name);
            values.string(&truncated)
        }
        EVAL_TAG_RESOURCE => {
            let closed = eval_resource_is_closed(value, context, values)?;
            values.string(if closed {
                "resource (closed)"
            } else {
                "resource (stream)"
            })
        }
        _ => values.string("null"),
    }
}

/// Resolves the PHP-visible class name for one object cell, the SAME route `get_class()` uses
/// (`eval_get_class_result`, `builtins/symbols/get_class.rs`) -- `context.dynamic_object_class_name()`
/// checks `closure_objects` first and answers `"Closure"` for an eval-declared closure before
/// falling back to a declared eval class, then to the runtime's own `object_class_name()` for an
/// object with no eval-tracked identity at all.
fn eval_get_debug_type_object_class_name(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    if let Ok(identity) = values.object_identity(value) {
        if let Some(class_name) = context.dynamic_object_class_name(identity) {
            return Ok(class_name);
        }
    }
    let class_name = values.object_class_name(value)?;
    let bytes = values.string_bytes(class_name)?;
    values.release(class_name)?;
    let bytes = bytes.strip_prefix(b"\\").unwrap_or(&bytes);
    String::from_utf8(bytes.to_vec()).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Truncates a class name at its first NUL byte, matching php's `get_debug_type()` (as opposed
/// to `get_class()`, which keeps the whole internal name).
fn eval_debug_type_truncate_at_nul(name: &str) -> String {
    match name.find('\0') {
        Some(index) => name[..index].to_string(),
        None => name.to_string(),
    }
}
