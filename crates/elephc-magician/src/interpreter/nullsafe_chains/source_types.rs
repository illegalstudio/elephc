//! Purpose:
//! Checks reached chain consumers and projects immutable property/offset source declarations.
//!
//! Called from:
//! - The nullsafe chain evaluator while receiver owners remain rooted.
//!
//! Key details:
//! - Quiet probes keep their missing/null protocol and never manufacture a source proof.

use super::*;
use crate::eval_ir::generic_type;

/// Validates a reached receiver domain without executing a skipped selector or reading result tags.
pub(super) fn receiver(expr: &EvalExpr, source: Option<&Type>, nullsafe: bool, quiet: bool,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(),EvalStatus> {
    if quiet { return Ok(()); }
    let source=source.cloned().map(mixed_source_proofs::value_source).unwrap_or_else(||Type::atom("mixed"));
    mixed_source_proofs::require_source(&source,"postfix receiver access",context,values,|ty,context| {
        if nullsafe && *ty==Type::atom("null") { return true; }
        match expr {
            EvalExpr::DynamicCall {..}=>mixed_callable_sources::supports(ty,context),
            EvalExpr::ArrayGet {..}=>matches!(ty,Type::Atom(name)|Type::Apply(name,_) if name=="array"||name=="string")
                || context.class_is_a(&ty.to_string(),"ArrayAccess",false),
            _=>mixed_method_results::object_source(ty,context),
        }
    })
}

/// Projects one physical property's declaration in the selected declaring and called scopes.
pub(in crate::interpreter) fn property(object: RuntimeCellHandle, property: &str, context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<Type>,EvalStatus> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) { return Ok(None); }
    if values.type_tag(object)? != EVAL_TAG_OBJECT { return Ok(None); }
    let class=mixed_callable_sources::object_class(object,context,values)?;
    let (owner,declared)=if let Some((owner,property))=context.class_property(&class,property) {
        (owner,property.property_type().cloned())
    } else {(class.clone(),context.native_property_type(&class,property))};
    declared.map(|ty|local_binding_origins::resolve_declared_source_type(&generic_type(&ty),Some(&owner),Some(&class),context)).transpose()
}

/// Detects receiver alternatives that use PHP scalar array/string key conversion.
fn uses_php_key(ty: &Type) -> bool {
    match ty {
        Type::Atom(name)|Type::Apply(name,_) => matches!(name.as_str(),"array"|"string"|"mixed"),
        Type::Union(members)|Type::Intersection(members) => members.iter().any(uses_php_key),
        Type::Nullable(inner) => uses_php_key(inner),
        _=>false,
    }
}

/// Validates a reached scalar-key consumer while custom ArrayAccess targets keep their own key domain.
pub(in crate::interpreter) fn offset_key(receiver: Option<&Type>, key: Option<&Type>, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(),EvalStatus> {
    if receiver.is_some_and(|ty| !uses_php_key(ty)) {return Ok(());}
    let key=key.cloned().map(mixed_source_proofs::value_source).unwrap_or_else(||Type::atom("mixed"));
    mixed_source_proofs::require_source(&key,"offset key access",context,values,|ty,_|
        matches!(ty,Type::Atom(name) if matches!(name.as_str(),"int"|"float"|"string"|"bool"|"true"|"false"|"null"|"void")))
}
