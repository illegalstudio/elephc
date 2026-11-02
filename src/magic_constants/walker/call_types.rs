//! Purpose:
//! Walks type arguments stored in encoded generic call names through frontend passes.
//!
//! Called from:
//! - The shared expression walker for function, method and callable references.
//!
//! Key details:
//! - Only argument types are transformed; a function or method base is never a class type.
//! - Declaration-scope restoration and later substitution must reach the same arguments.

use crate::names::Name;
use crate::span::Span;

use super::Pass;

/// Transforms a function reference's written type arguments without losing PHP fallback metadata.
pub(super) fn walk_function_name<P: Pass>(name: Name, pass: &P, span: Span) -> Name {
    let Some((base, arguments)) = crate::generics::split_written_instantiation(name.as_str()) else {
        return name;
    };
    let transformed = arguments.iter().cloned().map(|ty| pass.transform_type(ty, span)).collect::<Vec<_>>();
    if transformed == arguments {
        return name;
    }
    let result = Name::from_parts(name.kind, base.split('\\').map(str::to_string).collect())
        .with_type_arguments(&transformed);
    if name.function_fallback().is_some() { result.with_function_fallback() } else { result }
}

/// Transforms written method arguments before the pass renames a resolved method call.
pub(super) fn walk_method_name<P: Pass>(method: String, pass: &P, span: Span) -> String {
    let Some((base, arguments)) = crate::generics::split_written_instantiation(&method) else {
        return method;
    };
    let transformed = arguments.iter().cloned().map(|ty| pass.transform_type(ty, span)).collect::<Vec<_>>();
    if transformed == arguments {
        return method;
    }
    Name::unqualified(base).with_type_arguments(&transformed).as_canonical()
}
