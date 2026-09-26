//! Purpose:
//! Home of the PHP `array_chunk` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - PHP's signature is `array_chunk(array $array, int $length, bool $preserve_keys = false)`;
//!   both the positional and the `preserve_keys:` named form are accepted.
//! - `preserve_keys` CHANGES THE RESULT SHAPE, so it must be a literal in AOT mode (same rule as
//!   `array_reverse()`'s and `array_slice()`'s flags). With `false` an indexed `Array<T>` chunks
//!   into `Array<Array<T>>`; with `true` each chunk keeps the source integer keys of its own
//!   window, which is `Array<AssocArray { key: Int, value: T }>` because elephc's dense indexed
//!   representation cannot hold a window that does not start at key 0.
//! - An ASSOCIATIVE receiver chunks into `Array<AssocArray { … }>` in both modes: with the flag
//!   each window keeps the source's own keys, and without it the keys restart at 0 inside every
//!   chunk — which is where chunk's `preserve_keys` rule parts company with `array_slice()`'s,
//!   since chunk drops STRING keys too. Non-array inputs are still rejected. A check hook is
//!   required because the return type depends on the inferred argument type and on that literal
//!   flag.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

builtin! {
    contract: "array_chunk",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayChunk,
    ),
}

/// Reads a literal `preserve_keys` flag, returning `None` when the argument is not a literal.
///
/// An absent argument reads as a literal `false` so callers can treat "omitted" and "explicit
/// false" identically. Integer literals follow PHP truthiness, matching `array_reverse()`.
fn literal_preserve_keys(flag: Option<&Expr>) -> Option<bool> {
    match flag {
        None => Some(false),
        Some(flag) => match flag.kind {
            ExprKind::BoolLiteral(value) => Some(value),
            ExprKind::IntLiteral(value) => Some(value != 0),
            _ => None,
        },
    }
}

/// Returns the nested chunk-array type for an `array_chunk` call.
///
/// An indexed `Array<elem>` chunks into `Array<Array<elem>>`, or into
/// `Array<AssocArray { key: Int, value: elem }>` when a literal `preserve_keys: true` keeps each
/// window's source integer keys. Associative arrays are rejected (only indexed arrays are
/// supported), non-array arguments are rejected, and so is a non-literal flag. The first argument
/// is re-inferred here to drive the return type; the registry already inferred every argument once
/// for side effects, and arity (2 or 3) is pre-validated.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let preserve = literal_preserve_keys(cx.args.get(2)).ok_or_else(|| {
        CompileError::new(
            cx.span,
            "array_chunk() preserve_keys argument must be a literal bool in AOT mode",
        )
    })?;
    match ty {
        PhpType::Array(elem_ty) if preserve => Ok(PhpType::Array(Box::new(PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: elem_ty,
        }))),
        PhpType::Array(elem_ty) => Ok(PhpType::Array(Box::new(PhpType::Array(elem_ty)))),
        // An associative receiver chunks into hashes in BOTH modes, because neither mode's inner
        // container is a dense list elephc can represent: with `preserve_keys` the window keeps
        // the source's own keys, and without it the keys restart at 0 but the values may be any
        // width — which is the 16-byte string-slot problem. A hash keyed 0,1,2,… reads and prints
        // exactly like the list PHP returns.
        PhpType::AssocArray { key, value } if preserve => {
            Ok(PhpType::Array(Box::new(PhpType::AssocArray { key, value })))
        }
        PhpType::AssocArray { value, .. } => Ok(PhpType::Array(Box::new(PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value,
        }))),
        _ => Err(CompileError::new(
            cx.span,
            "array_chunk() first argument must be array",
        )),
    }
}
