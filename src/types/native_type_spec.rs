//! Purpose:
//! Formats checked PHP value contracts for native metadata and eval registration.
//!
//! Called from:
//! - Callable signatures, local reference descriptors and scope producer publication.
//!
//! Key details:
//! - Null value contracts are distinct from void return declarations.
//! - All consumers share the generated type grammar.
//! - Resource values use a reserved metadata atom, leaving PHP class names unchanged.

use super::PhpType;

/// Formats one checked PHP type for eval native metadata registration.
pub(crate) fn php_type_spec(php_type: &PhpType, allow_return_atoms: bool) -> Option<String> {
    if php_type.is_php_array() {
        return Some("array".to_string());
    }
    match php_type {
        PhpType::Int => Some("int".to_string()),
        PhpType::Float => Some("float".to_string()),
        PhpType::Str => Some("string".to_string()),
        PhpType::Bool => Some("bool".to_string()),
        PhpType::False => Some("false".to_string()),
        PhpType::Void if allow_return_atoms => Some("void".to_string()),
        PhpType::Void => Some("null".to_string()),
        PhpType::Never if allow_return_atoms => Some("never".to_string()),
        PhpType::Never => None,
        PhpType::Iterable => Some("iterable".to_string()),
        PhpType::Mixed => Some("mixed".to_string()),
        PhpType::Array(_) | PhpType::AssocArray { .. } => Some("array".to_string()),
        PhpType::Callable | PhpType::DeclaredCallable => Some("callable".to_string()),
        PhpType::Resource(_) => Some(elephc_semantics::native_local_contract::RESOURCE_TYPE_SPEC.to_string()),
        PhpType::Object(name) if name.is_empty() => Some("object".to_string()),
        PhpType::Object(name) => Some(name.clone()),
        PhpType::Union(members) => member_specs(members),
        PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_)
        | PhpType::Narrow(_)
        | PhpType::TaggedScalar => None,
    }
}

/// Formats union members from checked PHP types for eval native metadata registration.
pub(crate) fn member_specs(members: &[PhpType]) -> Option<String> {
    members
        .iter()
        .map(|member| php_type_spec(member, false))
        .collect::<Option<Vec<_>>>()
        .map(|members| members.join("|"))
}


/// Retains full declared array/value terms separately from PHP reflection and storage spellings.
pub(crate) fn logical_type(php_type: &PhpType,allow_return_atoms: bool) -> Option<elephc_generics::Type> {
    use elephc_generics::Type;
    if php_type.is_php_array() { return Some(Type::atom("array")); }
    Some(match php_type {
        PhpType::Array(element)=>Type::Apply("array".into(),vec![Type::atom("int"),logical_type(element,false)?]),
        PhpType::AssocArray{key,value}=>Type::Apply("array".into(),vec![logical_type(key,false)?,logical_type(value,false)?]),
        PhpType::Union(members) => logical_union_type(members)?,
        ty=>Type::atom(php_type_spec(ty,allow_return_atoms)?),
    })
}

/// Rejoins both internal storage alternatives as bare PHP `array` inside a wider union.
fn logical_union_type(members: &[PhpType]) -> Option<elephc_generics::Type> {
    use elephc_generics::Type;
    let PhpType::Union(array_layouts) = PhpType::php_array() else { unreachable!() };
    let has_bare_array = array_layouts.iter().all(|layout| members.contains(layout));
    let mut terms = Vec::new();
    let mut emitted_array = false;
    for member in members {
        if has_bare_array && array_layouts.contains(member) {
            if !emitted_array {
                terms.push(Type::atom("array"));
                emitted_array = true;
            }
        } else {
            terms.push(logical_type(member, false)?);
        }
    }
    Some(Type::Union(terms))
}

/// Canonicalizes value-position null atoms while retaining a callable's declared void return.
pub(crate) fn logical_parameter_type(expr: &crate::parser::ast::TypeExpr) -> elephc_generics::Type {
    use crate::parser::ast::TypeExpr;
    use elephc_generics::Type;
    match expr {
        TypeExpr::CallableSig { params, ret } => {
            let returned = if matches!(ret.as_ref(), TypeExpr::Void) { Type::atom("void") }
                else { logical_parameter_type(ret) };
            Type::Callable(params.iter().map(logical_parameter_type).collect(), Box::new(returned))
        }
        TypeExpr::Array(element) => Type::Apply("array".into(), vec![logical_parameter_type(element)]),
        TypeExpr::AssocArray { key, value } => Type::Apply("array".into(),
            vec![logical_parameter_type(key), logical_parameter_type(value)]),
        TypeExpr::GenericClass { name, args } => Type::Apply(name.as_str().into(),
            args.iter().map(logical_parameter_type).collect()),
        TypeExpr::Nullable(inner) => Type::Nullable(Box::new(logical_parameter_type(inner))),
        TypeExpr::Union(members) => Type::Union(members.iter().map(logical_parameter_type).collect()),
        TypeExpr::Intersection(members) => Type::Intersection(members.iter().map(logical_parameter_type).collect()),
        _ => crate::generics::shared_type(expr),
    }
}

#[cfg(test)]
mod logical_source_tests {
    use super::*;
    use crate::parser::ast::TypeExpr;
    use elephc_generics::Type;

    /// Value nulls remain distinct from callable void returns and reflection erases only its own array role.
    #[test]
    fn strict_descriptor_sources_keep_null_values_and_callable_void_returns() {
        let array=TypeExpr::Array(Box::new(TypeExpr::Void));
        assert_eq!(logical_parameter_type(&array),Type::Apply("array".into(),vec![Type::atom("null")]));
        let callback=TypeExpr::CallableSig{params:vec![TypeExpr::Void],ret:Box::new(TypeExpr::Void)};
        assert_eq!(logical_parameter_type(&callback),Type::Callable(vec![Type::atom("null")],Box::new(Type::atom("void"))));
        let array=PhpType::Array(Box::new(PhpType::Int));
        assert_eq!(php_type_spec(&array,false).as_deref(),Some("array"));
        assert_eq!(logical_type(&array,false),Some(Type::Apply("array".into(),vec![Type::atom("int"),Type::atom("int")])));
    }
}

/// Encodes a complete checked declaration independently from its PHP reflection spelling.
pub(crate) fn logical_type_spec(ty: &PhpType, allow_return_atoms: bool) -> Option<String> {
    logical_type(ty, allow_return_atoms).map(|ty| logical_term_spec(&ty))
}

/// Encodes one already normalized structural term for the trusted native registration boundary.
pub(crate) fn logical_term_spec(ty: &elephc_generics::Type) -> String {
    format!("{}{}", elephc_semantics::native_callable_sources::TYPE_SPEC_PREFIX,
        serde_json::to_string(ty).expect("neutral Type is JSON serializable"))
}

#[cfg(test)]
mod registration_source_tests {
    use super::*;

    /// Structural declarations retain array keys/elements while ordinary PHP array remains explicitly open.
    #[test]
    fn strict_native_registration_specs_keep_full_structural_terms() {
        let ty = PhpType::Array(Box::new(PhpType::Int));
        let encoded = logical_type_spec(&ty, false).unwrap();
        let encoded = encoded.strip_prefix(elephc_semantics::native_callable_sources::TYPE_SPEC_PREFIX).unwrap();
        let actual: elephc_generics::Type = serde_json::from_str(encoded).unwrap();
        assert_eq!(actual, elephc_generics::Type::Apply("array".into(),
            vec![elephc_generics::Type::atom("int"), elephc_generics::Type::atom("int")]));
        assert_eq!(logical_type(&PhpType::php_array(), false), Some(elephc_generics::Type::atom("array")));
        assert_eq!(php_type_spec(&ty, false).as_deref(), Some("array"));
    }

    /// Native overrides compare nullable bare arrays by their PHP contract, not by layout.
    #[test]
    fn native_nullable_array_return_metadata_retains_bare_array() {
        let PhpType::Union(mut layouts) = PhpType::php_array() else { unreachable!() };
        layouts.push(PhpType::Void);
        assert_eq!(
            logical_type(&PhpType::Union(layouts), true),
            Some(elephc_generics::Type::Union(vec![
                elephc_generics::Type::atom("array"),
                elephc_generics::Type::atom("null"),
            ])),
        );
    }
}
