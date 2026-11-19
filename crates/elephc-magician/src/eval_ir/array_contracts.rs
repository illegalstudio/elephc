//! Purpose:
//! Represents array-content contracts independently of whole-binding value kinds.
//!
//! Called from:
//! - Native source metadata decoding and shared generic value/write validation.
//!
//! Key details:
//! - Non-array values remain permitted; array keys and elements retain their schema.
//! - Integer-key hash layouts satisfy the same contract as packed arrays.

use elephc_generics::Type;
use elephc_semantics::native_local_contract::ARRAY_CONTENTS_TYPE_NAME;

/// Builds a conditional content contract from an array schema without adding a kind restriction.
pub(crate) fn array_contents_type(schema: Type) -> Option<Type> {
    let schema = normalize_schema(schema)?;
    Some(Type::Apply(ARRAY_CONTENTS_TYPE_NAME.to_string(), vec![schema]))
}

/// Borrows the array schema of a generated conditional content contract.
pub(crate) fn array_contents_schema(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Apply(name, arguments) if name == ARRAY_CONTENTS_TYPE_NAME && arguments.len() == 1 =>
            arguments.first(),
        _ => None,
    }
}

/// Marks an inferred empty array whose first successful informative write supplies its schema.
pub(crate) fn pending_array_contents_type() -> Type {
    Type::Apply(ARRAY_CONTENTS_TYPE_NAME.to_string(), vec![Type::Apply("array".to_string(),
        vec![Type::atom("never"), Type::atom("never")])])
}

/// Distinguishes pending inferred contents from ordinary declared array<never> contracts.
pub(crate) fn pending_array_contents(ty: &Type) -> bool {
    array_contents_schema(ty).is_some_and(|schema| matches!(schema, Type::Apply(name, args)
        if name == "array" && args.as_slice() == [Type::atom("never"), Type::atom("never")]))
}

/// Accepts only array alternatives, preserving explicitly open schemas and closed unions.
fn normalize_schema(schema: Type) -> Option<Type> {
    match schema {
        Type::Atom(name) if name == "array" || name == "mixed" => Some(Type::atom("array")),
        Type::Apply(name, arguments) if name == "array" && matches!(arguments.len(), 1 | 2) =>
            Some(normalize_arrays(Type::Apply(name, arguments))),
        Type::Nullable(inner) => normalize_schema(*inner),
        Type::Union(members) if !members.is_empty() => members.into_iter()
            .map(normalize_schema).collect::<Option<Vec<_>>>().map(Type::Union),
        Type::Intersection(members) if !members.is_empty() => members.into_iter()
            .map(normalize_schema).collect::<Option<Vec<_>>>().map(Type::Intersection),
        _ => None,
    }
}

/// Keeps semantic integer keys independent from every nested array's storage layout.
fn normalize_arrays(ty: Type) -> Type {
    match ty {
        Type::Apply(name, arguments) => {
            let mut arguments = arguments.into_iter().map(normalize_arrays).collect::<Vec<_>>();
            if name == "array" && arguments.len() == 1 { arguments.insert(0, Type::atom("int")); }
            Type::Apply(name, arguments)
        }
        Type::Union(members) => Type::Union(members.into_iter().map(normalize_arrays).collect()),
        Type::Intersection(members) => Type::Intersection(members.into_iter().map(normalize_arrays).collect()),
        Type::Nullable(inner) => Type::Nullable(Box::new(normalize_arrays(*inner))),
        Type::Callable(parameters, result) => Type::Callable(
            parameters.into_iter().map(normalize_arrays).collect(), Box::new(normalize_arrays(*result))),
        other => other,
    }
}

/// Merges an explicitly open alternative without inventing mixed for unrelated inferred values.
pub(crate) fn merge_array_element_sources(previous: Type, next: Type) -> Type {
    if previous == next || previous == Type::atom("mixed") { return previous; }
    if next == Type::atom("mixed") { return next; }
    if matches!(&previous, Type::Union(members) if members.contains(&next)) { return previous; }
    if matches!(&next, Type::Union(_) | Type::Nullable(_)) { return next; }
    if matches!(&previous, Type::Union(_) | Type::Nullable(_)) { return previous; }
    if empty_array_schema(&previous) { return next; }
    if empty_array_schema(&next) { return previous; }
    previous
}

/// Empty nested literals contribute no element evidence until a sibling supplies one.
fn empty_array_schema(ty: &Type) -> bool {
    matches!(ty, Type::Apply(name, args) if name == "array"
        && args.as_slice() == [Type::atom("never"), Type::atom("never")])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Conditional schemas retain keys/elements and reject unrelated generated metadata.
    #[test]
    fn array_contents_contract_schema_keeps_logical_array_shapes() {
        let input = Type::Apply("array".to_string(), vec![Type::Apply("array".to_string(), vec![Type::atom("int")])]);
        let contract = array_contents_type(input).unwrap();
        let expected = Type::Apply("array".to_string(), vec![Type::atom("int"),
            Type::Apply("array".to_string(), vec![Type::atom("int"), Type::atom("int")])]);
        assert_eq!(array_contents_schema(&contract), Some(&expected));
        for name in ["int", "string", "object"] { assert!(array_contents_type(Type::atom(name)).is_none()); }
        assert!(array_contents_type(Type::Apply("array".to_string(), vec![])).is_none());
        assert!(array_contents_type(Type::Union(vec![])).is_none());
        assert!(array_contents_type(Type::Union(vec![Type::atom("array"), Type::atom("int")])).is_none());
    }
}
