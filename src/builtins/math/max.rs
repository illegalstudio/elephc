//! Purpose:
//! Home of the PHP `max` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook is required because the return type depends on argument types:
//!   the single-array form returns the array's element type, while the variadic
//!   form widens to Float as soon as any argument is Float.
//! - `min_args: 1` matches PHP: `max()` with no argument is an ArgumentCountError,
//!   one argument must be an array, and two or more arguments compare the values.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "max",
    check: check,
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::runtime_fn_semantics(
            crate::ir::RuntimeFnId::Max,
        ),
        crate::builtins::semantics::BuiltinArgumentLowering::BareArrayValues {
            arg: 0,
            sole: true,
        },
    ),
}

/// Returns the array element type for the single-array form; otherwise returns Float
/// when any argument is Float and Int in every other case.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if cx.args.len() == 1 {
        return super::min_max_array_element_type(cx, "max");
    }
    let mut has_float = false;
    for arg in cx.args {
        let t = cx.checker.infer_type(arg, cx.env)?;
        if t == PhpType::Float {
            has_float = true;
        }
    }
    if has_float {
        Ok(PhpType::Float)
    } else {
        Ok(PhpType::Int)
    }
}
