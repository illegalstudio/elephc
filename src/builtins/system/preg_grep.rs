//! Purpose:
//! Home of PHP's `preg_grep` builtin and its key-preserving regex-filter semantics.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The checked result uses associative mixed storage because original numeric and string keys are
//!   preserved rather than compacted.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "preg_grep",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PregGrep,
    ),
}

/// Returns the key-preserving hash layout materialized by the regex filter runtime.
///
/// A pattern that can fail makes the result `array|false`, as php's.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let hash = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    };
    if cx
        .args
        .first()
        .is_some_and(crate::builtins::system::preg_match::preg_pattern_can_fail)
    {
        return Ok(PhpType::Union(vec![hash, PhpType::False]));
    }
    Ok(hash)
}
