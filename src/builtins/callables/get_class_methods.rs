//! Purpose:
//! Home of PHP's `get_class_methods` builtin and its AOT checker contract.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through the builtin registry.
//!
//! Key details:
//! - AOT accepts an object or a runtime class-name string.
//! - EIR filters the emitted method inventory using the lexical visibility scope.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "get_class_methods",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::GetClassMethods,
    ),
}

/// Accepts an object or string class name and returns an indexed string array.
fn check(cx: &mut BuiltinCheckCtx<'_>) -> Result<PhpType, CompileError> {
    let argument = match &cx.args[0].kind {
        ExprKind::NamedArg { name, value }
            if crate::names::php_symbol_key(name) == "object_or_class" =>
        {
            value.as_ref()
        }
        _ => &cx.args[0],
    };
    let ty = cx.checker.infer_type(argument, cx.env)?;
    if !matches!(ty.codegen_repr(), PhpType::Object(_) | PhpType::Str) {
        return Err(CompileError::new(
            cx.span,
            "get_class_methods() argument must be an object or string in AOT mode",
        ));
    }
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}
