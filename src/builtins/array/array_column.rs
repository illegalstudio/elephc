//! Purpose:
//! Home of the PHP `array_column` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` accepts indexed or associative collections of associative/gradual rows whose shape is
//!   runtime-known. The result preserves a known associative value type and otherwise becomes
//!   `array<mixed>`. Other shapes are rejected. A check hook is required because the return type
//!   depends on the inferred argument type.
//!   Concrete associative rows preserve their value type in the indexed result; declared PHP
//!   arrays use runtime row lookup and return indexed Mixed cells.
//! - Arity (exactly 2 arguments) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.
//!   Note elephc only supports the 2-argument form (`array`, `column_key`).

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_column",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayColumn,
    ),
}

/// Returns the extracted-column array type for an `array_column` call.
///
/// The first argument must be an indexed or associative array of associative or gradual rows. Known
/// associative rows preserve their value type; gradual rows produce `array<mixed>`. Other
/// shapes are rejected. The argument is re-inferred here to drive the return type; the registry
/// already inferred every argument once for side effects, and arity (exactly 2) is pre-validated.
/// Declared PHP arrays require runtime row lookup and return Mixed elements.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if cx.args.len() == 3 {
        // `$index_key` re-keys the result from the data, so it is a hash whatever the rows look
        // like. `__elephc_array_column_indexed` answers it, and the type here is the one that
        // helper's body infers from its single `$result[$key] = $value;` site.
        for arg in &cx.args[1..] {
            cx.checker.infer_type(arg, cx.env)?;
        }
        return Ok(PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        });
    }
    if ty.is_php_array() {
        return Ok(PhpType::Array(Box::new(PhpType::Mixed)));
    }
    if rows_need_generic_helper(&ty) {
        // Object rows (an enum's `cases()`) and list rows (`array_column($errors, 0)`) are read by
        // the prelude helper, which lowering routes them to with a null index key.
        return Ok(PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        });
    }
    match ty {
        PhpType::Array(inner) => match *inner {
            PhpType::AssocArray { value, .. } => Ok(PhpType::Array(value)),
            PhpType::Mixed | PhpType::Union(_) => {
                Ok(PhpType::Array(Box::new(PhpType::Mixed)))
            }
            _ => Err(CompileError::new(
                cx.span,
                "array_column() requires an array of associative arrays",
            )),
        },
        PhpType::AssocArray { value, .. } => match *value {
            PhpType::AssocArray { value, .. } => Ok(PhpType::Array(value)),
            PhpType::Mixed | PhpType::Union(_) => {
                Ok(PhpType::Array(Box::new(PhpType::Mixed)))
            }
            _ => Ok(PhpType::Array(Box::new(PhpType::Mixed))),
        },
        PhpType::Mixed | PhpType::Union(_) => Ok(PhpType::Array(Box::new(PhpType::Mixed))),
        _ => Err(CompileError::new(
            cx.span,
            "array_column() first argument must be array",
        )),
    }
}

/// Whether a two-argument `array_column()` over `ty` needs the generic prelude helper: an indexed
/// array whose rows are objects or lists, which the native column reader cannot address.
pub(crate) fn rows_need_generic_helper(ty: &PhpType) -> bool {
    if ty.is_php_array() {
        return false;
    }
    matches!(
        ty,
        PhpType::Array(inner)
            if matches!(**inner, PhpType::Object(_) | PhpType::Array(_))
    )
}
