//! Purpose:
//! Forwards eval fileinfo calls to the host's compiled PHP declarations.
//!
//! Called from:
//! - Eval source-call dispatch and declarative builtin hooks.
//!
//! Key details:
//! - No native dependency is linked into Magician itself.
//! - Named arguments bind against the host signature; missing capabilities raise Error.

use super::super::*;
mod finfo_buffer;
mod finfo_close;
mod finfo_file;
mod finfo_open;
mod finfo_set_flags;
mod mime_content_type;

/// Recognizes the exact PHP fileinfo surface from the shared catalog.
pub(in crate::interpreter) fn eval_fileinfo_builtin_name(name: &str) -> bool {
    elephc_builtin_contract::lookup(name).is_some_and(|c| c.module == elephc_builtin_contract::PhpModule::Fileinfo)
}

/// Reports whether an optional fileinfo function is linked in the host.
pub(in crate::interpreter) fn eval_fileinfo_builtin_linked(context: &ElephcEvalContext, name: &str) -> bool {
    !eval_fileinfo_builtin_name(name) || context.native_function(&name.to_ascii_lowercase()).is_some()
}

/// Forwards evaluated arguments through the shared named/positional native binder.
fn forward(name: &str, args: Vec<EvaluatedCallArg>, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(function) = context.native_function(&name.to_ascii_lowercase()) else {
        return eval_throw_error(&format!("Call to undefined function {name}()"), context, values);
    };
    let bound = bind_evaluated_native_function_args(&function, args, context, values)?;
    eval_native_function_with_values(function, bound, context, values)
}

/// Keeps named arguments intact when forwarding an eval source call.
pub(in crate::interpreter) fn eval_builtin_fileinfo_call(name: &str, args: &[EvalCallArg], context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, scope, values, |args, context, _, values| forward(name, args, context, values))
}

/// Adapts positional expression hooks to the shared source-call path.
pub(in crate::interpreter) fn eval_builtin_fileinfo_expr_call(name: &str, args: &[EvalExpr], context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = args.iter().cloned().map(EvalCallArg::positional).collect::<Vec<_>>();
    eval_builtin_fileinfo_call(name, &args, context, scope, values)
}

/// Forwards dynamic callable invocations after their arguments have been evaluated.
pub(in crate::interpreter) fn eval_fileinfo_values_result(name: &str, args: &[RuntimeCellHandle], context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = args.iter().map(|value| EvaluatedCallArg { name: None, value: *value, ref_target: None }).collect();
    forward(name, args, context, values)
}
