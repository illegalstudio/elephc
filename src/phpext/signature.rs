//! Purpose:
//! Validates the PHP-visible value subset independently of the public C export ABI.
//!
//! Called from:
//! - `crate::exports::collect_for_php()` after type checking.
//!
//! Key details:
//! - References, objects, resources and variadics have no producer boundary in this phase.

use crate::{errors::CompileError, span::Span, types::{FunctionSig, PhpType}};

/// Rejects signatures for which the adapter cannot make an independent value copy.
pub(crate) fn validate(name: &str, sig: &FunctionSig, span: Span) -> Result<(), CompileError> {
    let fail = |detail: &str| CompileError::new(span, &format!("PHP export '{name}': {detail}"));
    if sig.variadic.is_some() || sig.by_ref_return || sig.ref_params.iter().any(|r| *r) {
        return Err(fail("--emit phpext requires fixed, by-value parameters and returns"));
    }
    for (_, ty) in &sig.params {
        if !input_storage_supported(ty) {
            return Err(fail("typed array parameters are not supported by --emit phpext; use the PHP array type"));
        }
        if !supported(ty) { return Err(fail(&format!("unsupported parameter type {ty:?}"))); }
    }
    if !supported(&sig.return_type) && sig.return_type != PhpType::Never {
        return Err(fail(&format!("unsupported return type {:?}", sig.return_type)));
    }
    Ok(())
}

/// Recognizes recursively copyable types; Mixed values are checked again at runtime.
fn supported(ty: &PhpType) -> bool {
    match ty {
        PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False | PhpType::Str
            | PhpType::Void | PhpType::Mixed => true,
        PhpType::Array(value) => supported(value),
        PhpType::AssocArray { key, value } => supported(key) && supported(value),
        PhpType::Union(members) => members.iter().all(supported),
        _ => false,
    }
}

/// Enumerates core value helpers whose persistent state is entirely inside the native image.
pub(crate) fn runtime_supported(target: crate::ir::RuntimeFnId) -> bool {
    use crate::ir::RuntimeFnId as R;
    matches!(target, R::Count | R::ArrayIsList | R::ArrayKeyExists | R::ArrayKeyFirst
        | R::ArrayKeyLast | R::ArrayKeys | R::ArrayValues | R::ArrayReverse | R::ArraySlice
        | R::ArrayMerge | R::ArrayPop | R::ArrayPush | R::ArrayShift | R::ArrayUnshift
        | R::ArraySum | R::ArrayProduct | R::ArraySearch | R::ArrayChunk | R::ArrayColumn
        | R::ArrayFlip | R::ArrayFill | R::ArrayFillKeys | R::ArrayCombine | R::ArrayCountValues
        | R::StrRepeat)
}

/// Keeps nullable wrappers from accidentally exposing specialized native array layouts.
fn input_storage_supported(ty: &PhpType) -> bool {
    if ty.is_php_array() { return true; }
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => false,
        PhpType::Union(members) => {
            let non_null = PhpType::Union(members.iter().filter(|ty| **ty != PhpType::Void).cloned().collect());
            non_null.is_php_array() || members.iter().all(input_storage_supported)
        }
        _ => true,
    }
}
