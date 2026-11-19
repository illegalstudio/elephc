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
mod source_views;

pub(super) use source_views::{LocalAssignmentProducer, prepare_assignment, source_view, evaluate_assignment};

pub(super) use arrays::{source_schema as array_source_schema, contains_contents as contains_array_contents};
pub(super) use arrays::{prepare_write as prepare_array_local_write, publish_write as publish_array_local_write};

/// Captures a producer's declared alternatives without executing PHP or borrowing live maps.
pub(in crate::interpreter) fn local_producer_contract(expr: &EvalExpr,
    context: &ElephcEvalContext, scope: &ElephcEvalScope,
) -> Option<EvalParameterType> {
    let arrays = context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes);
    if !arrays && !context.semantic_profile().contains(elephc_semantics::StrictFeature::Locals) { return None; }
    let mixed = || EvalParameterType::new(vec![EvalParameterTypeVariant::Mixed], false);
    match expr {
        EvalExpr::LoadVar(name) => {
            let kind = scope.local_type(name).or_else(|| scope.inferred_local_type(name))
                .or_else(|| scope.native_binding_type(name)).cloned().unwrap_or_else(mixed);
            let contents = if arrays { generic_local_contracts(scope, name, context).into_iter()
                .filter(|ty| arrays::contains_contents(&generic_type(ty))).collect::<Vec<_>>() }
                else { Vec::new() };
            if contents.is_empty() { Some(kind) } else {
                let mut members = vec![generic_type(&kind)];
                members.extend(contents.iter().map(generic_type));
                Some(EvalParameterType::new(vec![EvalParameterTypeVariant::Generic(Type::Intersection(members))], false))
            }
        },
        EvalExpr::Assign { value, .. } => local_producer_contract(value, context, scope),
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
            let first = local_producer_contract(then_branch.as_deref().unwrap_or(condition), context, scope);
            let second = local_producer_contract(else_branch, context, scope);
            Some(match (first, second) {
                (Some(first), Some(second)) => {
                    let nullable = first.allows_null() || second.allows_null();
                    let mut variants = first.variants().to_vec();
                    for variant in second.variants() { if !variants.contains(variant) { variants.push(variant.clone()); } }
                    EvalParameterType::new(variants, nullable)
                }
                _ => mixed(),
            })
        }
        EvalExpr::Call { name, .. } => {
            let name = name.trim_start_matches('\\').to_ascii_lowercase();
            let builtin = elephc_builtin_contract::lookup(&name)
                .map(|contract| parameter_type(&builtin_producer_type(contract.returns)));
            builtin.or_else(|| context.function(&name).and_then(|function| function.return_type()).cloned())
                .or_else(|| context.native_function(&name).and_then(|function| function.return_type().cloned()))
                .map(|ty| if arrays { ty } else { parameter_type(&binding_kind_type(generic_type(&ty))) })
                .or_else(|| Some(mixed()))
        }
        EvalExpr::DynamicCall { .. } | EvalExpr::DynamicMethodCall { .. }
            | EvalExpr::DynamicStaticMethodCall { .. } => Some(mixed()),
        _ => None,
    }
}

/// Adapts the neutral builtin return contract without parsing rendered documentation as semantics.
fn builtin_producer_type(ty: elephc_builtin_contract::TypeSpec) -> Type {
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
    let LocalAssignmentProducer { binding: producer, source } = producer;
    let source = if context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes) {
        match source.map(Ok).unwrap_or_else(|| value_contract(value, context, values)) {
            Ok(source) => Some(source),
            Err(status) => { let _ = eval_release_value(context, values, value); return Err(status); }
        }
    } else { None };
    let fresh = context.semantic_profile().contains(elephc_semantics::StrictFeature::Locals)
        && visible_scope_cell(context, scope, &name).is_none()
        && scope.local_type(&name).is_none();
    let array_origin = match arrays::prepare(scope, &name, producer.as_ref(), value, context, values) {
        Ok(origin) => origin,
        Err(status) => { let _ = eval_release_value(context, values, value); return Err(status); }
    };
    let origin = if fresh {
        match producer.map(|ty| Ok(parameter_type(&binding_kind_type(generic_type(&ty))))).unwrap_or_else(|| value_contract(value, context, values)) {
            Ok(origin) => Some(origin),
            Err(status) => { let _ = eval_release_value(context, values, value); return Err(status); }
        }
    } else { None };
    let replaced = set_owned_scope_cell(context, scope, name.clone(), value, values)?;
    if let Some(source) = source { scope.record_local_source_view(name.clone(), source); }
    if let Some(origin) = array_origin {
        scope.infer_array_local_type(name.clone(), origin);
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
    Ok(replaced)
}
