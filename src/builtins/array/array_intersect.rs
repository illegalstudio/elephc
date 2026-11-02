//! Purpose:
//! Home of the PHP `array_intersect` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `variadic(&["array"], "arrays")` (one regular `array`
//!   param plus a variadic `arrays`). The legacy CHECK arm required exactly 2 arguments,
//!   so `min_args: 2, max_args: 2` reproduce that enforcement in `check_arity` only;
//!   `function_sig` and the parity gate keep the variadic shape from the golden.
//! - `check` rejects known non-arrays while allowing gradual array boundaries. Results stay
//!   gradual because key preservation can select indexed or hash storage at runtime and the
//!   compatibility helper stores boxed payloads independent of the operand's static shape.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_intersect",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayIntersect,
    ),
}

/// Validates every argument as a concrete or gradual array and returns gradual storage.
///
/// Arity is pre-validated by `check_arity`. Gradual values remain accepted because the
/// lowering path binds them to array-typed helper parameters with runtime tag checks.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty1 = cx.checker.infer_type(&cx.args[0], cx.env)?;
    for (index, arg) in cx.args.iter().enumerate() {
        let ty = if index == 0 {
            ty1.clone()
        } else {
            cx.checker.infer_type(arg, cx.env)?
        };
        if !crate::types::checker::builtins::arrays::array_arg_is_gradually_acceptable(&ty) {
            let position = if index == 0 { "first" } else { "second" };
            return Err(CompileError::new(
                arg.span,
                &format!("{}() {} argument must be array", cx.name, position),
            ));
        }
    }
    Ok(PhpType::Mixed)
}
