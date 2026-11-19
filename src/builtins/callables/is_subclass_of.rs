//! Purpose:
//! Home of the PHP `is_subclass_of` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the registry common path infers all arguments and returns
//!   the declared `Bool` type.
//! - `allow_string` defaults to `true` (PHP's default for `is_subclass_of`).


builtin! {
    contract: "is_subclass_of",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IsSubclassOf,
    ),
}

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::{CompileError, CompileWarning};
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

/// Warns when a string subject is used while `$allow_string` is explicitly or implicitly false.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    warn_string_subject(cx, "is_subclass_of", true);
    Ok(PhpType::Bool)
}

/// Emits PHP 8.6's string-subject deprecation when the flag is statically false.
fn warn_string_subject(cx: &mut BuiltinCheckCtx, function: &str, default_allow_string: bool) {
    if crate::codegen_support::compile_php_version() < crate::php_version::PhpVersion::Php86 {
        return;
    }
    let Some(subject) = cx.args.first() else { return; };
    let Ok(subject_type) = cx.checker.infer_type(subject, cx.env) else { return; };
    if !matches!(subject_type.codegen_repr(), PhpType::Str) {
        return;
    }
    let allow_string = match cx.args.get(2) {
        Some(arg) => match arg.kind {
            ExprKind::BoolLiteral(value) => Some(value),
            _ => None,
        },
        None => Some(default_allow_string),
    };
    if allow_string == Some(false) {
        cx.checker.warnings.push(CompileWarning::new(
            subject.span,
            &format!("Calling {}() with a string when $allow_string is false", function),
        ));
    }
}
