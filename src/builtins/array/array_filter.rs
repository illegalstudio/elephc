//! Purpose:
//! Home of the PHP `array_filter` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `optional(&["array","callback","mode"], 1, &[null, 0])`, and
//!   php-src declares `array_filter(array $array, ?callable $callback = null, int $mode = 0)`:
//!   ONE argument is valid and keeps the truthy elements. The legacy CHECK arm required 2 or 3
//!   (`args.len() < 2 || args.len() > 3`) and `min_args: 2` faithfully reproduced that — which
//!   reproduced its BUG, refusing `array_filter($rows)` at compile time. The minimum is the
//!   golden signature's 1; the no-callback lowering supplies an implicit truthiness predicate.
//! - `check` validates the first argument is an indexed array, derives callback argument types
//!   from the static mode value, and validates the callback signature. The return type
//!   preserves the input array element type.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_filter",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayFilter,
    ),
}

/// Returns the filtered array type for an `array_filter` call.
///
/// Validates the first argument is an indexed array, derives callback argument types
/// from the optional mode argument, and validates the callback. Arity (1 to 3 args)
/// is pre-validated by `check_arity`. A call with no callback — or an explicit `null`, which
/// php accepts identically — skips callback validation: there is nothing to check, and the
/// result type is the input's element type either way.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    // An `array|false` union (scandir, glob, file) reads through to its array member;
    // the argument lowering pairs the acceptance with an unbox-or-throw for the `false`.
    let arr_ty = arr_ty.array_or_false_member().cloned().unwrap_or(arr_ty);
    if let Some(mode) = cx.args.get(2) {
        cx.checker.infer_type(mode, cx.env)?;
    }
    match arr_ty {
        PhpType::Array(elem_ty) => {
            // The null test is SYNTACTIC on purpose. Inferring the callback's type here
            // instead binds a closure's untyped parameters before
            // `check_array_callback_builtin_call` can seed them with the element type, which
            // made `array_filter($strings, fn($s) => strlen($s))` fail to type-check.
            let callback = match cx.args.get(1) {
                Some(callback)
                    if !matches!(callback.kind, crate::parser::ast::ExprKind::Null) =>
                {
                    callback
                }
                _ => return Ok(array_filter_result_type(&elem_ty)),
            };
            let arr_ty = PhpType::Array(elem_ty.clone());
            let callback_arg_types =
                crate::types::checker::builtins::array_filter_callback_arg_types(
                    &arr_ty,
                    cx.args.get(2),
                );
            crate::types::checker::builtins::check_array_callback_builtin_call(
                cx.checker,
                callback,
                &callback_arg_types,
                cx.span,
                cx.env,
                "array_filter() callback",
            )?;
            Ok(array_filter_result_type(&elem_ty))
        }
        // An associative source keeps its own key type; php preserves those keys too.
        PhpType::AssocArray { key, value } => {
            let callback = match cx.args.get(1) {
                Some(callback)
                    if !matches!(callback.kind, crate::parser::ast::ExprKind::Null) =>
                {
                    callback
                }
                _ => {
                    return Ok(PhpType::AssocArray {
                        key: key.clone(),
                        value: value.clone(),
                    })
                }
            };
            let arr_ty = PhpType::AssocArray {
                key: key.clone(),
                value: value.clone(),
            };
            let callback_arg_types =
                crate::types::checker::builtins::array_filter_callback_arg_types(
                    &arr_ty,
                    cx.args.get(2),
                );
            crate::types::checker::builtins::check_array_callback_builtin_call(
                cx.checker,
                callback,
                &callback_arg_types,
                cx.span,
                cx.env,
                "array_filter() callback",
            )?;
            Ok(PhpType::AssocArray { key, value })
        }
        _ => Err(CompileError::new(
            cx.span,
            "array_filter() first argument must be array",
        )),
    }
}

/// Returns the type `array_filter()` answers for an INDEXED source.
///
/// php preserves the keys, and the survivors of a list are not a list: `array_filter([0,1,2])`
/// answers `[1 => 1, 2 => 2]`, whose `isset($r[0])` is false. Only a keyed table can hold that,
/// so an indexed source answers one keyed by the integer indices it had.
fn array_filter_result_type(elem_ty: &PhpType) -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Int),
        value: Box::new(elem_ty.clone()),
    }
}
