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
//! - Checker and EIR share ONE result type. See `array_map_semantics`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::builtins::semantics::{
    runtime_fn_semantics, BuiltinResultType, BuiltinSemantics,
};
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "array_map",
    check: check,
    semantics: array_map_semantics(),
}

/// Builds semantics whose EIR result slot IS the checker's result type.
///
/// This builtin used to carry two answers for one call: the checker's precise container
/// (`array<int>` for a callback declared `: int`) and a separate EIR result slot of `Mixed`,
/// on the ground that a STRING callback binds through a runtime descriptor whose element ABI
/// is only known once that descriptor resolves.
///
/// Nothing reconciled them, and every boundary that quotes the CHECKER's type — a return
/// contract, a parameter contract — then read a boxed Mixed cell as though it were an array:
///
/// ```php
/// function c(array $s) { return array_map(fn(int $x): int => $x * $x, $s); }
/// echo c([1, 2, 3])[0];   // printed a pointer; php prints 1
/// ```
///
/// with no extension syntax in sight. The same call used inside the function was correct,
/// because that consumer reads the EIR type — which is exactly what made it hard to see.
///
/// `mapped_element_type` is what makes ONE answer safe: it narrows to the callback's own
/// element type only where EVERY lowering path can build that storage, and reports `Mixed`
/// otherwise — which is the storage `__rt_array_map_mixed` actually builds. So the checker's
/// type is always the storage, `BuiltinResultType::Checked` hands that same type to EIR
/// (`checked_result_type_fits_operands` answers `true` for `ArrayMap`), and the result slot
/// can no longer disagree with the container the runtime allocated.
///
/// DO NOT restore a `Shared` override that returns `Mixed`. Boxing every mapped element and
/// then boxing the container is not only the miscompile above, it is also slower than the
/// typed `__rt_array_map` path a narrowed slot selects.
const fn array_map_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::ArrayMap);
    semantics.result_type = BuiltinResultType::Checked;
    semantics
}

/// Returns the element type of the array `array_map` produces for a callback returning
/// `callback_ret_ty`.
///
/// The mapped array holds the CALLBACK's results, so the callback return type — not the input
/// element type — decides the element type.
///
/// The narrowing is deliberately conservative: `Int`, `Bool` and `Str` only, and only when the
/// callback does not reach EIR as a string. That set is the INTERSECTION of what the lowering
/// paths can build. `array_map_descriptor_callback_result_element_type` accepts exactly those
/// three from the result slot; a string callback is bound through a runtime descriptor whose
/// element ABI is Mixed whatever the named function returns, so a narrowed slot there is the one
/// case that genuinely cannot be honoured — it fails as `array_map result element PHP type Int
/// for callback result PHP type Mixed`. Everything else — `Float`, a union, `void`/`never`, an
/// unresolved callable, a nested container — keeps `Mixed`, which is what the runtime cells
/// carry and what the widening in `normalize_indexed_array_result` then stamps.
fn mapped_element_type(callback_ret_ty: PhpType, callback_is_string: bool) -> PhpType {
    // A callback named by STRING is no longer an exception. Its descriptor wrapper casts the boxed
    // result to the declared return type, so the narrowed storage it promises is the storage the
    // runtime builds — which is what let this arm be dropped without reintroducing the dual answer
    // the unification exists to prevent.
    let _ = callback_is_string;
    match callback_ret_ty {
        PhpType::Int | PhpType::Bool | PhpType::Str => callback_ret_ty,
        _ => PhpType::Mixed,
    }
}

/// Returns whether the callback operand reaches EIR as a `string`.
///
/// `lower_array_map` dispatches on the callback operand's EIR type, and its `PhpType::Str` arm
/// binds the callback through a runtime descriptor whose element ABI is Mixed. The element
/// decision has to agree with that arm, so it asks the same question the lowering asks.
///
/// A closure literal is answered syntactically rather than by inference: inferring it HERE would
/// type its parameters before `check_map_callback` supplies the source element type as a
/// contextual hint, which is the whole reason that hook exists.
fn callback_reaches_eir_as_string(cx: &mut BuiltinCheckCtx) -> Result<bool, CompileError> {
    match &cx.args[0].kind {
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_) => Ok(false),
        ExprKind::StringLiteral(_) => Ok(true),
        _ => Ok(matches!(
            cx.checker.infer_type(&cx.args[0], cx.env)?.codegen_repr(),
            PhpType::Str
        )),
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
    if arr_ty.is_php_array() {
        check_map_callback(cx, &PhpType::Mixed)?;
        return Ok(PhpType::php_array());
    }
    let callback_is_string = callback_reaches_eir_as_string(cx)?;
    match arr_ty {
        PhpType::Array(elem_ty) => {
            if matches!(elem_ty.as_ref(), PhpType::Object(_)) {
                return Err(CompileError::new(
                    cx.span,
                    "array_map() does not yet support object array elements",
                ));
            }
            let callback_ret_ty = check_map_callback(cx, elem_ty.as_ref())?;
            Ok(PhpType::Array(Box::new(mapped_element_type(
                callback_ret_ty,
                callback_is_string,
            ))))
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
                value: Box::new(mapped_element_type(callback_ret_ty, callback_is_string)),
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
