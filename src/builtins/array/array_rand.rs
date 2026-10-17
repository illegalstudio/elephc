//! Purpose:
//! Home of the PHP `array_rand` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - One key from a list (`array_rand($list)` or a literal `$num` of 1) stays the typed `Int`
//!   index the `__rt_array_rand` fast path returns. Every other call (an associative, declared or
//!   boxed array, or any other `$num`) answers `mixed`: php returns the key itself for one key and
//!   a list of keys otherwise, which `__rt_array_rand_boxed` builds and boxes.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::checker::builtins::arrays::boxed_value_may_hold_array;
use crate::types::PhpType;

builtin! {
    contract: "array_rand",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayRand,
    ),
}

/// Returns `Int` for one key from a list, `mixed` (a key or a list of keys) otherwise.
///
/// The registry's `check_arity` handles arity (1 or 2 arguments).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        && !ty.is_php_array()
        && !boxed_value_may_hold_array(&ty)
    {
        return Err(CompileError::new(
            cx.span,
            "array_rand() argument must be array",
        ));
    }
    let one_key = match cx.args.get(1) {
        None => true,
        Some(num) => matches!(num.kind, ExprKind::IntLiteral(1)),
    };
    if one_key && matches!(ty, PhpType::Array(_)) {
        return Ok(PhpType::Int);
    }
    Ok(PhpType::Mixed)
}
