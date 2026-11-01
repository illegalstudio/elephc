//! Purpose:
//! Home of the PHP `constant` builtin: its single-source registry declaration and semantic
//! metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Literal names retain the referenced constant's precise type, while runtime names use the
//!   boxed `mixed` representation returned by the constant registry.
//! - An unknown literal name compiles with a warning and, like a bare read of it, lowers to the
//!   runtime lookup, which throws php's `Error: Undefined constant "X"`.
//! - A literal class constant or enum case (`constant('Foo::BAR')`) is checked and lowered as
//!   the `Foo::BAR` expression it names ([`literal_class_constant_expr`]). Any other `Class::NAME`
//!   name (`constant(Level::class . '::' . $case)`, Monolog's level lookup) is resolved at run time
//!   by `__rt_constant` through the compiled class-constant table: public class constants and enum
//!   cases, with php's `Undefined constant` / `Class "X" not found`.
//! - Literal lowering happens one level up, in
//!   `crate::ir_lower::expr::constants::lower_static_constant_call()`, which rewrites the call
//!   into the same EIR a bare `FOO` reference produces. Other names lower through the typed
//!   runtime-function path and the closed-world constant registry.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::names::{Name, NameKind};
use crate::parser::ast::{Expr, ExprKind, StaticReceiver};
use crate::span::Span;
use crate::types::PhpType;

builtin! {
    contract: "constant",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Constant,
    ),
}

/// Returns a literal constant's precise type or `mixed` for a runtime name.
///
/// A leading `\` is stripped from literal names the way PHP's global-constant lookup does.
/// Runtime names are resolved by the emitted constant registry, and so are a literal
/// `Class::NAME` and a literal name nothing defines (recorded so lowering keeps the throwing
/// lookup).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let literal = match &cx.args[0].kind {
        ExprKind::StringLiteral(name) => Some(name.clone()),
        ExprKind::NamedArg { name, value } if name == "name" => match &value.kind {
            ExprKind::StringLiteral(name) => Some(name.clone()),
            _ => None,
        },
        _ => None,
    };
    let Some(name) = literal else {
        return Ok(PhpType::Mixed);
    };
    let name = elephc_builtin_contract::canonical_constant_name(&name).to_string();
    if let Some(access) = literal_class_constant_expr(&name, cx.span) {
        return cx.checker.infer_type(&access, cx.env);
    }
    if name.contains("::") {
        return Ok(PhpType::Mixed);
    }
    if let Some(ty) = cx.checker.constants.get(&name) {
        return Ok(ty.clone());
    }
    if !cx.checker.eval_barrier_active {
        cx.checker.record_undefined_constant_read(&name, cx.span);
    }
    Ok(PhpType::Mixed)
}

/// Builds the `Foo::BAR` expression a literal `constant('Foo::BAR')` names.
///
/// The string is always fully qualified (`constant()` performs no namespace resolution), and a
/// relative receiver (`self`, `static`, `parent`) is left to the refusal, since it would bind to
/// whatever class the call happens to sit in.
pub(crate) fn literal_class_constant_expr(name: &str, span: Span) -> Option<Expr> {
    let (class_name, constant) = name.trim_start_matches('\\').split_once("::")?;
    if class_name.is_empty()
        || constant.is_empty()
        || constant.contains("::")
        || matches!(
            class_name.to_ascii_lowercase().as_str(),
            "self" | "static" | "parent"
        )
    {
        return None;
    }
    let parts = class_name.split('\\').map(str::to_string).collect::<Vec<_>>();
    if parts.iter().any(String::is_empty) {
        return None;
    }
    Some(Expr::new(
        ExprKind::ScopedConstantAccess {
            receiver: StaticReceiver::Named(Name::from_parts(NameKind::FullyQualified, parts)),
            name: constant.to_string(),
        },
        span,
    ))
}
