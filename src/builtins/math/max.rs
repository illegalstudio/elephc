//! Purpose:
//! Home of the PHP `max` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook is required because the return type depends on argument types:
//!   the single-array form returns the array's element type, while the variadic
//!   form is `int` for all-`int` arguments, `float` for all-`float` arguments and `mixed`
//!   for every other comparable mix (`min_max_gmp::check`).
//! - `min_args: 1` matches PHP: `max()` with no argument is an ArgumentCountError,
//!   one argument must be an array, and two or more arguments compare the values.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "max",
    check: check,
    semantics: crate::builtins::semantics::with_argument_lowering(
        super::min_max_gmp::semantics(crate::ir::RuntimeFnId::Max),
        crate::builtins::semantics::BuiltinArgumentLowering::BareArrayValues {
            arg: 0,
            sole: true,
        },
    ),
}

/// Types the call through the shared `min()` / `max()` rule.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    super::min_max_gmp::check(cx, "max")
}
