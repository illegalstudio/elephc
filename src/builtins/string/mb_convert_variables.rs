//! Purpose:
//! Binds mb_convert_variables to the shared live-variable conversion contract.
//!
//! Called from:
//! - Compiler builtin checking and typed EIR runtime-call lowering.
//!
//! Key details:
//! - All variable arguments are by reference, including the variadic tail.
//! - The protected V6 host owns live storage traversal and PHP array COW.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::{CompileError, CompileWarning};
use crate::types::PhpType;

builtin! {
    contract: "mb_convert_variables",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::MbConvertVariables),
}

/// Runs the shared mbstring capture validation and reports the PHP 8.6 object deprecation.
///
/// php-src emits the deprecation from `mb_recursive_convert_variable()` for every object it
/// meets, always naming the ROOT variadic argument the object belongs to (`n + 3`). AOT can
/// prove the root argument's static type, so a `$vars` argument typed as an object is
/// reported here; object values nested inside arrays keep their runtime traversal.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let result = crate::builtins::mbstring::capture_check(cx)?;
    if crate::codegen_support::compile_php_version() >= crate::php_version::PhpVersion::Php86 {
        for (index, argument) in cx.args.iter().enumerate().skip(2) {
            let ty = cx.checker.infer_type(argument, cx.env)?;
            if matches!(ty.codegen_repr(), PhpType::Object(_)) {
                cx.checker.warnings.push(CompileWarning::new(
                    argument.span,
                    &format!(
                        "Passing an object for argument #{} ($vars) to mb_convert_variables() is deprecated, call get_object_vars() first instead",
                        index + 1
                    ),
                ));
            }
        }
    }
    Ok(result)
}
