//! Purpose:
//! Home of the PHP `array_map` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `variadic(&["callback","array"], "arrays")` (two
//!   required params plus a variadic `arrays`). The legacy CHECK arm required exactly
//!   2 arguments, so `min_args: 2, max_args: 2` reproduce that enforcement in
//!   `check_arity` only; `function_sig` and the parity gate keep the variadic shape.
//! - `check` validates that the second argument is an array — indexed or associative — and
//!   infers the callback return element type; the result preserves the input array element
//!   type unless the callback returns Mixed. An associative source keeps its KEY type, which
//!   is what makes the single-array form key-preserving the way php-src is.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::builtins::semantics::{
    runtime_fn_semantics, BuiltinResultType, BuiltinSemanticInput, BuiltinSemantics,
};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_map",
    check: check,
    semantics: array_map_semantics(),
}

/// Builds semantics with a boxed Mixed result for runtime-selected callback shapes.
const fn array_map_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::ArrayMap);
    semantics.result_type = BuiltinResultType::Shared(eir_result_type);
    semantics
}

/// Returns Mixed because a string or descriptor callback can select its result ABI at runtime.
///
/// DO NOT narrow this to the concrete container type to "match `array_flip`". That was measured:
/// dropping this override makes the EIR result slot the checker's `Array(Int)`/`AssocArray`, and
/// every STRING-callback call site — `array_map('double', $a)` — then dies at compile time with
/// `array_map result element PHP type Int for callback result PHP type Mixed`. A string callback
/// is bound through a runtime descriptor (`__rt_array_map_mixed`), whose element ABI is Mixed and
/// is only known once the descriptor resolves, so a statically concrete result slot is genuinely
/// incompatible with it. Closure / arrow-fn / first-class-callable / callable-array sites do
/// compile under a narrowed slot, which is precisely why the breakage is easy to miss.
///
/// The heap consequence of the Mixed slot — the result container gets boxed and must therefore be
/// TRANSFERRED into the Mixed cell rather than shared with it — is handled in the lowering, by
/// `box_array_result_for_mixed_builtin` / `box_hash_result_for_mixed_builtin` in
/// `src/codegen/lower_inst/builtins/arrays.rs`.
fn eir_result_type(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::Mixed
}

/// Returns the element type of the array `array_map` produces for a callback returning
/// `callback_ret_ty`.
///
/// The mapped array holds the CALLBACK's results, so the callback return type — not the input
/// element type — decides the element type. `null`/`never` returns have no element
/// representation of their own and a union is boxed at runtime anyway, so both collapse to
/// `Mixed`, which is the type the runtime cells actually carry.
fn mapped_element_type(callback_ret_ty: PhpType) -> PhpType {
    match callback_ret_ty {
        PhpType::Void | PhpType::Never | PhpType::Union(_) => PhpType::Mixed,
        other => other,
    }
}

/// Returns the mapped array type for an `array_map()` call.
///
/// Validates that the second argument is an array — indexed OR associative — checks the
/// callback with its contextual element type, and derives the result element type from the
/// callback return type through `mapped_element_type`. Arity (exactly 2 args) is pre-validated
/// by `check_arity`.
///
/// The single-array form of php-src `array_map()` PRESERVES the source keys, so an associative
/// source produces an associative result under the SAME key type and only the value type is
/// rewritten by the callback. (The reindexing php-src performs from two arrays onward is out
/// of reach here: `check_arity` already refuses more than two arguments.)
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let arr_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    // An `array|false` union (scandir, glob, file) reads through to its array member;
    // the argument lowering pairs the acceptance with an unbox-or-throw for the `false`.
    let arr_ty = arr_ty.array_or_false_member().cloned().unwrap_or(arr_ty);
    match arr_ty {
        PhpType::Array(elem_ty) => {
            if matches!(elem_ty.as_ref(), PhpType::Object(_)) {
                return Err(CompileError::new(
                    cx.span,
                    "array_map() does not yet support object array elements",
                ));
            }
            let callback_ret_ty = check_map_callback(cx, elem_ty.as_ref())?;
            Ok(PhpType::Array(Box::new(mapped_element_type(callback_ret_ty))))
        }
        PhpType::AssocArray { key, value } => {
            if matches!(value.as_ref(), PhpType::Object(_)) {
                return Err(CompileError::new(
                    cx.span,
                    "array_map() does not yet support object array elements",
                ));
            }
            let callback_ret_ty = check_map_callback(cx, value.as_ref())?;
            Ok(PhpType::AssocArray {
                key,
                value: Box::new(mapped_element_type(callback_ret_ty)),
            })
        }
        _ => Err(CompileError::new(
            cx.span,
            "array_map() second argument must be array",
        )),
    }
}

/// Checks the `array_map()` callback against one source element type and returns its return type.
///
/// PHP hands the callback the VALUE only, so the single argument slot carries the element type
/// of an indexed source and the VALUE type of an associative one.
fn check_map_callback(
    cx: &mut BuiltinCheckCtx,
    element_ty: &PhpType,
) -> Result<PhpType, CompileError> {
    let callback_arg_types = [element_ty.clone()];
    crate::types::checker::builtins::check_array_callback_builtin_call(
        cx.checker,
        &cx.args[0],
        &callback_arg_types,
        cx.span,
        cx.env,
        "array_map() callback",
    )
}
