//! Purpose:
//! Preserves successful eval local producer contracts across walker, VM and native references.
//!
//! Called from:
//! - Source assignments and bytecode stores before and after RHS evaluation.
//!
//! Key details:
//! - Producer facts are copied before effects; storage shape does not establish a source contract.
//! - Imported bindings stay open unless their existing source already supplies a constraint.

use super::*;
use crate::eval_ir::{generic_type, parameter_type};
use elephc_generics::Type;

mod arrays;
mod pending_arrays;
pub(super) use pending_arrays::{prepare as prepare_array_nested_write, refine_origin as refine_pending_array_origin,
    wrap_source as wrap_nested_array_source};
mod array_mutations;
pub(super) use array_mutations::{finish_array_mutation, replacement_element_source};
mod source_views;
mod calls;
pub(super) use calls::builtin as selected_call_builtin;
mod declared_sources;
pub(super) use declared_sources::resolve_type as resolve_declared_source_type;
pub(super) use declared_sources::method_result as declared_method_result;
pub(super) use declared_sources::offset_result as declared_offset_result;

pub(super) use source_views::{LocalAssignmentProducer, prepare_assignment, source_view, argument_source, evaluate_assignment};

pub(super) use arrays::{element_source_type, element_value_source_type, refresh_reference_sources, refresh_value_sources};
pub(super) use arrays::{prepare_write as prepare_array_local_write, publish_write as publish_array_local_write};
pub(super) use arrays::source_schema as array_source_schema;
pub(super) use arrays::prepare_value as prepare_array_value_origin;

/// Selects whether absent producer metadata opens a local binding or remains an unknown source.
#[derive(Clone, Copy)]
pub(super) enum ProducerContractRole {
    Binding,
    Source,
}

impl ProducerContractRole {
    /// Reports whether an untyped producer must keep a strict local binding open.
    fn opens_unknown(self, context: &ElephcEvalContext) -> bool {
        matches!(self, Self::Binding)
            && context.semantic_profile().contains(elephc_semantics::StrictFeature::Locals)
    }
}

/// Captures a producer's declared alternatives without executing PHP or borrowing live maps.
pub(in crate::interpreter) fn local_producer_contract(expr: &EvalExpr,
    context: &ElephcEvalContext, scope: &ElephcEvalScope,
) -> Option<EvalParameterType> {
    producer_contract(expr, context, scope, ProducerContractRole::Binding)
}

/// Captures source evidence while preserving the difference between unknown and explicit mixed.
pub(super) fn source_producer_contract(expr: &EvalExpr,
    context: &ElephcEvalContext, scope: &ElephcEvalScope,
) -> Option<EvalParameterType> {
    producer_contract(expr, context, scope, ProducerContractRole::Source)
}

/// Shares structural producer discovery while keeping binding and source roles independent.
fn producer_contract(expr: &EvalExpr, context: &ElephcEvalContext,
    scope: &ElephcEvalScope, role: ProducerContractRole,
) -> Option<EvalParameterType> {
    let arrays = context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes);
    if !arrays && !context.semantic_profile().contains(elephc_semantics::StrictFeature::Locals)
        && !context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) { return None; }
    let mixed = || EvalParameterType::new(vec![EvalParameterTypeVariant::Mixed], false);
    match expr {
        EvalExpr::LoadVar(name) => {
            let kind = scope.local_type(name).or_else(|| matches!(role, ProducerContractRole::Binding)
                .then(|| scope.inferred_local_type(name)).flatten())
                .or_else(|| scope.native_binding_type(name)).cloned()
                .or_else(|| role.opens_unknown(context).then(mixed));
            let contents = if arrays { generic_local_contracts(scope, name, context).into_iter()
                .filter(|ty| arrays::contains_contents(&generic_type(ty))).collect::<Vec<_>>() }
                else { Vec::new() };
            if contents.is_empty() { kind } else {
                let mut members = kind.as_ref().map(generic_type).into_iter().collect::<Vec<_>>();
                members.extend(contents.iter().map(generic_type));
                Some(EvalParameterType::new(vec![EvalParameterTypeVariant::Generic(Type::Intersection(members))], false))
            }
        },
        EvalExpr::Assign { value, .. } => producer_contract(value, context, scope, role),
        EvalExpr::Const(value) => Some(match value {
            EvalConst::Null => EvalParameterType::new(Vec::new(), true),
            EvalConst::Int(_) => EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false),
            EvalConst::Float(_) => EvalParameterType::new(vec![EvalParameterTypeVariant::Float], false),
            EvalConst::Bool(_) => EvalParameterType::new(vec![EvalParameterTypeVariant::Bool], false),
            EvalConst::String(_) | EvalConst::Bytes(_) => EvalParameterType::new(vec![EvalParameterTypeVariant::String], false),
        }),
        EvalExpr::Array(_) if arrays => None,
        EvalExpr::Array(_) => Some(EvalParameterType::new(vec![EvalParameterTypeVariant::Array], false)),
        EvalExpr::Closure { .. } | EvalExpr::FunctionCallable { .. }
            | EvalExpr::InvokableCallable { .. } | EvalExpr::MethodCallable { .. }
            | EvalExpr::StaticMethodCallable { .. } | EvalExpr::DynamicStaticMethodCallable { .. } =>
            Some(EvalParameterType::new(vec![EvalParameterTypeVariant::Callable], false)),
        EvalExpr::Cast { target, .. } => Some(EvalParameterType::new(vec![match target {
            EvalCastType::Array => EvalParameterTypeVariant::Array,
            EvalCastType::Int => EvalParameterTypeVariant::Int,
            EvalCastType::Float => EvalParameterTypeVariant::Float,
            EvalCastType::String | EvalCastType::StringFormat => EvalParameterTypeVariant::String,
            EvalCastType::Bool => EvalParameterTypeVariant::Bool,
        }], false)),
        EvalExpr::Ternary { condition, then_branch, else_branch } => {
            let first = producer_contract(then_branch.as_deref().unwrap_or(condition), context, scope, role);
            let second = producer_contract(else_branch, context, scope, role);
            Some(match (first, second) {
                (Some(first), Some(second)) => {
                    let nullable = first.allows_null() || second.allows_null();
                    let mut variants = first.variants().to_vec();
                    for variant in second.variants() { if !variants.contains(variant) { variants.push(variant.clone()); } }
                    EvalParameterType::new(variants, nullable)
                }
                _ => return None,
            })
        }
        EvalExpr::Call { name, .. } => calls::producer(name, None, context, role),
        EvalExpr::NamespacedCall { name, fallback_name, .. } => calls::producer(name, Some(fallback_name), context, role),
        EvalExpr::DynamicCall { .. } | EvalExpr::DynamicMethodCall { .. }
            | EvalExpr::DynamicStaticMethodCall { .. } => role.opens_unknown(context).then(mixed),
        _ => declared_sources::producer(expr, context, scope),
    }
}

/// Adapts the neutral builtin return contract without parsing rendered documentation as semantics.
pub(in crate::interpreter) fn builtin_producer_type(ty: elephc_builtin_contract::TypeSpec) -> Type {
    use elephc_builtin_contract::TypeSpec;
    let name = match ty {
        TypeSpec::Int => "int", TypeSpec::Float => "float", TypeSpec::Str => "string",
        TypeSpec::Bool => "bool", TypeSpec::Object => "object", TypeSpec::False => "false",
        TypeSpec::Null | TypeSpec::Void => "null", TypeSpec::Mixed | TypeSpec::Ptr => "mixed",
        TypeSpec::Array => "array", TypeSpec::Callable => "callable", TypeSpec::Resource => elephc_semantics::native_local_contract::RESOURCE_TYPE_SPEC,
        TypeSpec::Nullable(inner) => return Type::Nullable(Box::new(builtin_producer_type(*inner))),
        TypeSpec::Union(members) => return Type::Union(members.iter().copied().map(builtin_producer_type).collect()),
    };
    Type::atom(name)
}

/// Keeps declared producer alternatives while leaving array element inference to ArrayTypes.
fn binding_kind_type(ty: Type) -> Type {
    if crate::eval_ir::array_contents_schema(&ty).is_some() { return Type::atom("array"); }
    match ty {
        Type::Atom(name) if name == "void" => Type::atom("null"),
        Type::Apply(name, _) if name == "array" => Type::atom("array"),
        Type::Callable(_, _) => Type::atom("callable"),
        Type::Nullable(inner) => Type::Nullable(Box::new(binding_kind_type(*inner))),
        Type::Union(members) => Type::Union(members.into_iter().map(binding_kind_type).collect()),
        Type::Intersection(members) => {
            let mut kinds = members.into_iter().filter(|member| !arrays::contains_contents(member))
                .map(binding_kind_type).collect::<Vec<_>>();
            match kinds.len() { 0 => Type::atom("array"), 1 => kinds.remove(0), _ => Type::Intersection(kinds) }
        },
        ty => ty,
    }
}

/// Seeds a parameter binding from its declaration rather than its incidental incoming value.
pub(in crate::interpreter) fn bind_parameter_origin(scope: &mut ElephcEvalScope, name: &str,
    declared: Option<&EvalParameterType>, variadic: bool, context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes) {
        let declared = declared.map(generic_type).unwrap_or_else(|| Type::atom("mixed"));
        let schema = if variadic { Some(Type::Apply("array".to_string(), vec![Type::atom("int"), declared])) }
            else { arrays::source_schema(&declared) };
        if let Some(schema) = schema.and_then(crate::eval_ir::array_contents_type) {
            scope.infer_array_local_type(name.to_string(), parameter_type(&schema));
        }
    }
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Locals) { return Ok(()); }
    let ty = if variadic { Type::atom("array") }
        else { declared.map(generic_type).unwrap_or_else(|| Type::atom("mixed")) };
    let origin = parameter_type(&binding_kind_type(ty));
    let variants = origin.variants().iter().map(|variant| match variant {
        EvalParameterTypeVariant::Class(name) => eval_method_parameter_runtime_class_name(name, context)
            .map(EvalParameterTypeVariant::Class),
        variant => Ok(variant.clone()),
    }).collect::<Result<Vec<_>, _>>()?;
    let origin = if origin.is_intersection() { EvalParameterType::intersection(variants) }
        else { EvalParameterType::new(variants, origin.allows_null()) };
    scope.infer_local_type(name.to_string(), origin);
    Ok(())
}

/// Infers a PHP value kind without accidentally enabling inferred array element constraints.
pub(in crate::interpreter) fn value_contract(value: RuntimeCellHandle, context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalParameterType, EvalStatus> {
    let variant = match values.type_tag(value)? {
        EVAL_TAG_NULL => return Ok(EvalParameterType::new(Vec::new(), true)),
        EVAL_TAG_BOOL => EvalParameterTypeVariant::Bool,
        EVAL_TAG_INT => EvalParameterTypeVariant::Int,
        EVAL_TAG_FLOAT => EvalParameterTypeVariant::Float,
        EVAL_TAG_STRING => EvalParameterTypeVariant::String,
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => EvalParameterTypeVariant::Array,
        EVAL_TAG_OBJECT => {
            let ty = runtime_generic_type(value, context, values, 0)?;
            return Ok(parameter_type(&binding_kind_type(ty)));
        }
        EVAL_TAG_CALLABLE => EvalParameterTypeVariant::Callable,
        EVAL_TAG_RESOURCE => EvalParameterTypeVariant::Resource,
        _ => EvalParameterTypeVariant::Mixed,
    };
    Ok(EvalParameterType::new(vec![variant], false))
}

/// Consumes an assignment value, then establishes only a genuinely new successful binding.
pub(in crate::interpreter) fn store_local_with_origin(context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, name: String, value: RuntimeCellHandle,
    producer: LocalAssignmentProducer, values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    store_local_with_roles(context,scope,name,value,producer,true,values)
}

/// Publishes loop source and content roles without adding a new inferred Locals kind contract.
pub(in crate::interpreter) fn store_foreach_with_origin(context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, name: String, value: RuntimeCellHandle,
    source: Option<&Type>, values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>,EvalStatus> {
    let source=source.map(parameter_type);
    let producer=LocalAssignmentProducer {binding:source.clone(),source,arguments:None};
    store_local_with_roles(context,scope,name,value,producer,false,values)
}

/// Shares source, array-content and owner publication while callers select the binding-kind role.
fn store_local_with_roles(context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, name: String, value: RuntimeCellHandle,
    producer: LocalAssignmentProducer, infer_kind: bool, values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>,EvalStatus> {
    let LocalAssignmentProducer { binding: producer, source, arguments } = producer;
    let array_source = source.clone();
    let source = if context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes)
        && !context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) {
        match source.map(Ok).unwrap_or_else(|| value_contract(value, context, values)) {
            Ok(source) => Some(source),
            Err(status) => { let _ = eval_release_value(context, values, value); return Err(status); }
        }
    } else if context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) { source }
    else { None };
    let fresh = infer_kind && context.semantic_profile().contains(elephc_semantics::StrictFeature::Locals)
        && visible_scope_cell(context, scope, &name).is_none()
        && scope.local_type(&name).is_none();
    let binding_contents = producer.as_ref().filter(|ty| arrays::contains_contents(&generic_type(ty)));
    let array_producer = arguments.as_ref().map(|packet| &packet.ty)
        .or(array_source.as_ref()).or(binding_contents);
    let array_origin = match arrays::prepare(scope, &name, array_producer, value, context, values) {
        Ok(origin) => origin,
        Err(status) => { let _ = eval_release_value(context, values, value); return Err(status); }
    };
    if let Some(origin) = &array_origin {
        let checked = (|| {
            if let Some(reference) = visible_scope_cell(context, scope, &name) {
                if values.is_reference(reference)? {
                    array_reference_contracts::validate_new_owner(reference, value, std::slice::from_ref(origin), context, values)?;
                }
            }
            Ok(())
        })();
        if let Err(status) = checked { let _ = eval_release_value(context, values, value); return Err(status); }
    }
    let origin = if fresh {
        match producer.map(|ty| Ok(parameter_type(&binding_kind_type(generic_type(&ty))))).unwrap_or_else(|| value_contract(value, context, values)) {
            Ok(origin) => Some(origin),
            Err(status) => { let _ = eval_release_value(context, values, value); return Err(status); }
        }
    } else { None };
    let replaced = set_owned_scope_cell(context, scope, name.clone(), value, values)?;
    if let Some(source) = source { scope.record_local_source_view(name.clone(), source, context.source_view_epoch()); }
    if let Some(origin) = array_origin {
        scope.infer_array_local_type(name.clone(), origin);
        refresh_reference_sources(scope, &name, context, values)?;
        if values.supports_persistent_references() {
            eval_persistent_variable_reference(&name, context, scope, values)?;
        }
    }
    if let Some(origin) = origin {
        let open = origin.variants().contains(&EvalParameterTypeVariant::Mixed);
        scope.infer_local_type(name.clone(), origin);
        if !open && values.supports_persistent_references() {
            eval_persistent_variable_reference(&name, context, scope, values)?;
        }
    }
    refresh_reference_sources(scope, &name, context, values)?;
    if let Some(arguments) = arguments { scope.record_local_argument_source(name, arguments); }
    Ok(replaced)
}
