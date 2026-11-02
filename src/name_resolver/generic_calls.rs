//! Purpose:
//! Resolves type arguments encoded in written generic function and method call names.
//!
//! Called from:
//! - Function-reference resolution and expression method/callable resolution.
//!
//! Key details:
//! - Arguments use the caller's namespace and imports before their spelling becomes an identity.
//! - A rooted width-named class remains distinct from a bare native storage width.

use crate::names::{php_symbol_key, Name};

use super::{Imports, Symbols};

/// Resolves a written generic function's base independently from its type arguments.
pub(super) fn resolve_function_reference(
    name: &Name, namespace: Option<&str>, imports: &Imports, symbols: &Symbols,
) -> Option<Name> {
    let (base, arguments) = crate::generics::split_written_instantiation(name.as_str())?;
    let mut base = Name::from_parts(name.kind, base.split('\\').map(str::to_string).collect());
    if name.function_fallback().is_some() {
        base = base.with_function_fallback();
    }
    let arguments = arguments.iter()
        .map(|ty| super::names::resolve_type_expr(ty, namespace, imports, symbols))
        .collect::<Vec<_>>();
    Some(super::names::resolve_function_reference(&base, namespace, imports, symbols)
        .with_type_arguments(&arguments))
}

/// Resolves method type arguments, optionally folding only the method base for callable keys.
pub(super) fn resolve_method_reference(
    method: &str, namespace: Option<&str>, imports: &Imports, symbols: &Symbols, key: bool,
) -> String {
    let Some((base, arguments)) = crate::generics::split_written_instantiation(method) else {
        return if key { php_symbol_key(method) } else { method.to_string() };
    };
    let arguments = arguments.iter()
        .map(|ty| super::names::resolve_type_expr(ty, namespace, imports, symbols))
        .collect::<Vec<_>>();
    Name::unqualified(if key { php_symbol_key(&base) } else { base })
        .with_type_arguments(&arguments).as_canonical()
}
