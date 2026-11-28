//! Purpose:
//! Validates iterable sources and projects logical foreach key/value declarations.
//!
//! Called from:
//! - The shared array, Iterator and IteratorAggregate loop dispatchers.
//!
//! Key details:
//! - Source declarations stay independent from actual element tags and runtime owner leases.

use super::*;
use elephc_generics::Type;

/// Requires every represented alternative to support iteration before entering a loop body.
pub(super) fn require(source: Option<&Type>, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(),EvalStatus> {
    let source=source.cloned().map(mixed_source_proofs::value_source).unwrap_or_else(||Type::atom("mixed"));
    mixed_source_proofs::require_source(&source,"foreach iteration",context,values,|ty,context| {
        matches!(ty,Type::Atom(name)|Type::Apply(name,_) if name=="array"||name=="iterable")
            || context.class_is_a(&ty.to_string(),"Traversable",false)
    })
}

/// Projects one declared array's key and element domains, preserving unknown and explicit mixed roles.
pub(super) fn array_parts(source: Option<&Type>) -> (Option<Type>,Option<Type>) {
    let Some(source)=source else {return (None,None);};
    let source=local_binding_origins::element_source_type(source.clone(),true);
    match source {
        Type::Atom(name) if name=="array" => (Some(Type::Union(vec![Type::atom("int"),Type::atom("string")])),Some(Type::atom("mixed"))),
        Type::Apply(name,args) if name=="array" && !args.is_empty()=> {
            (Some(if args.len()==1 {Type::atom("int")} else {args[0].clone()}),args.last().cloned())
        }
        Type::Union(members)=> {
            let parts=members.iter().map(|ty|array_parts(Some(ty))).collect::<Vec<_>>();
            (joined(parts.iter().map(|parts|parts.0.clone())),joined(parts.into_iter().map(|parts|parts.1)))
        }
        _=>(None,None),
    }
}

/// Keeps finite alternatives and declines to replace missing declaration evidence with a value observation.
fn joined(parts: impl Iterator<Item=Option<Type>>) -> Option<Type> {
    let mut members=Vec::new();
    for part in parts {let part=part?;if !members.contains(&part) {members.push(part);}}
    match members.len() {0=>None,1=>members.pop(),_=>Some(Type::Union(members))}
}

/// Captures a selected iterator method declaration before invoking its observable body.
pub(super) fn method(object: RuntimeCellHandle, name: &str, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle,Option<Type>),EvalStatus> {
    let source=if context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed)
        || context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes) {
        let class=mixed_callable_sources::object_class(object,context,values)?;
        local_binding_origins::declared_method_result(&class,&class,name,false,context)
    } else {None};
    eval_method_call_result(object,name,Vec::new(),context,values).map(|value|(value,source))
}
