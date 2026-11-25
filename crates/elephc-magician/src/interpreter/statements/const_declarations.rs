//! Purpose:
//! Executes a top-level `const NAME = value;` statement inside `eval()`.
//!
//! Called from:
//! - `crate::interpreter::statements::dispatch` for `EvalStmt::ConstDecl`.
//!
//! Key details:
//! - The statement registers the constant when it runs, in source order, like php-src's
//!   `ZEND_DECLARE_CONST`. The name arrives already qualified with the fragment namespace.
//! - A name that already resolves keeps its first value and raises PHP's redefinition
//!   warning, whose PHP 9 suffix depends on the selected PHP profile.

use super::*;

/// First PHP profile whose redefinition warning announces the PHP 9 error.
const PHP_9_ERROR_SUFFIX_VERSION_ID: u32 = 80500;

/// Evaluates the value and registers it under `name` unless the name is already defined.
pub(super) fn eval_const_decl_stmt(
    name: &str,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let value = eval_owned_expr(value, context, scope, values)?;
    if eval_constant_already_defined(name, context) {
        let warned = values.warning(&const_redefinition_warning(name));
        eval_release_value(context, values, value)?;
        return warned;
    }
    if !context.define_constant(name, value) {
        eval_release_value(context, values, value)?;
    }
    Ok(())
}

/// Formats PHP's warning for a constant declared twice under the active profile.
///
/// PHP names the constant by its lookup key: namespace segments lowercased, the constant's own
/// name kept as written.
fn const_redefinition_warning(name: &str) -> String {
    let name = name.trim_start_matches('\\');
    let name = match name.rsplit_once('\\') {
        Some((namespace, constant)) => format!("{}\\{constant}", namespace.to_ascii_lowercase()),
        None => name.to_owned(),
    };
    if crate::eval_php_profile::eval_php_version_id() >= PHP_9_ERROR_SUFFIX_VERSION_ID {
        format!("Warning: Constant {name} already defined, this will be an error in PHP 9\n")
    } else {
        format!("Warning: Constant {name} already defined\n")
    }
}
