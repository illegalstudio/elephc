//! Purpose:
//! Home of the PHP `array_diff` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `variadic(&["array"], "arrays")` (one regular `array`
//!   param plus a variadic `arrays`). The legacy CHECK arm required exactly 2 arguments,
//!   so `min_args: 2, max_args: 2` reproduce that enforcement in `check_arity` only;
//!   `function_sig` and the parity gate keep the variadic shape from the golden.
//! - `check` accepts concrete and gradual array operands. Concrete inputs preserve
//!   their shape, while gradual inputs keep their conservative type until the runtime
//!   array guard selects the active representation.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_diff",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayDiff,
    ),
}

/// Validates the first argument is an array and returns its (preserved) type.
///
/// Arity (exactly 2 args) is pre-validated by `check_arity`. The first argument is
/// re-inferred here to drive the return type; the registry already inferred every
/// argument once for side effects. The result preserves the first-operand array shape
/// or its gradual boundary type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty1 = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(
        ty1,
        PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Mixed
            | PhpType::Union(_)
    ) {
        return Err(CompileError::new(
            cx.span,
            &format!("{}() first argument must be array", cx.name),
        ));
    }
    // The two-argument form is lowered through `__elephc_array_diff_string`
    // (`ir_lower::expr::compat_preludes`), which returns a key-preserving HASH of boxed values.
    // Announcing the input's own type instead hid that change of representation from the loop
    // storage analysis: `$columns = range(...); foreach (...) { $columns = array_diff($columns,
    // ...); }` kept its packed slot while the body stored a hash into it, and symfony/console's
    // `Table::getRowColumns()` read the result back as heap addresses.
    if cx.args.len() == 2 {
        return Ok(PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        });
    }
    Ok(ty1)
}
