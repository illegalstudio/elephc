//! Purpose:
//! Home of the PHP `preg_replace` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The optional replacement limit and by-reference replacement counter follow
//!   PHP's public five-parameter signature.
//! - The checker leaves the counter destination write-only while inferring all
//!   value inputs, allowing an undefined variable to receive the result.
//! - A literal pattern with the `u` modifier returns `?string`: an invalid UTF-8 subject fails.
//! - An operand that may be an array makes the call `mixed`; see
//!   `types::checker::builtins::preg_operand_may_be_array`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "preg_replace",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::runtime_fn_semantics(
            crate::ir::RuntimeFnId::PregReplace,
        ),
        crate::builtins::semantics::BuiltinArgumentLowering::PositionalRegex,
    ),
}

/// Infers replacement inputs while leaving the optional counter destination write-only.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let mut may_be_array = false;
    for (index, argument) in cx.args.iter().take(4).enumerate() {
        let ty = cx.checker.infer_type(argument, cx.env)?;
        may_be_array |=
            index < 3 && crate::types::checker::builtins::preg_operand_may_be_array(&ty);
    }
    // A `$count` the call skipped with a named argument is the contract's injected default, which
    // carries the dummy span: no caller expression, so nothing to demand a variable of.
    if cx.args.len() >= 5
        && cx.args[4].span != crate::span::Span::dummy()
        && !cx
            .checker
            .is_by_ref_argument_lvalue(&cx.args[4], cx.env)?
    {
        return Err(CompileError::new(
            cx.args[4].span,
            "preg_replace() parameter $count must be passed a variable",
        ));
    }
    if may_be_array {
        return Ok(PhpType::Mixed);
    }
    if crate::builtins::system::preg_match::preg_pattern_can_fail(&cx.args[0]) {
        return Ok(PhpType::Union(vec![PhpType::Str, PhpType::Void]));
    }
    Ok(PhpType::Str)
}
