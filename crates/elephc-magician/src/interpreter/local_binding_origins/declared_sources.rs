//! Purpose:
//! Resolves declaration-backed source types without inspecting boxed value occupants.
//!
//! Called from:
//! - Logical constructor/member producers and reached declared-return boundaries.
//!
//! Key details:
//! - Generic Type terms remain the sole structural representation.
//! - Class keywords retain their declaring owner and late-static called scope.

use super::*;

/// Resolves class keywords recursively while preserving every structural declaration alternative.
pub(in crate::interpreter) fn resolve_type(ty: &Type, owner: Option<&str>, called: Option<&str>,
    context: &ElephcEvalContext,
) -> Result<Type, EvalStatus> {
    let resolve = |ty: &Type| resolve_type(ty, owner, called, context);
    Ok(match ty {
        Type::Atom(name) if matches!(name.to_ascii_lowercase().as_str(),"self"|"static"|"parent") =>
            Type::atom(eval_declared_type_runtime_class_name(name,owner,called,context)?),
        Type::Apply(name,args) => Type::Apply(name.clone(),args.iter().map(resolve).collect::<Result<_,_>>()?),
        Type::Union(members) => Type::Union(members.iter().map(resolve).collect::<Result<_,_>>()?),
        Type::Intersection(members) => Type::Intersection(members.iter().map(resolve).collect::<Result<_,_>>()?),
        Type::Nullable(inner) => Type::Nullable(Box::new(resolve(inner)?)),
        Type::Callable(params,result) => Type::Callable(params.iter().map(resolve).collect::<Result<_,_>>()?,Box::new(resolve(result)?)),
        ty => ty.clone(),
    })
}

/// Converts an existing declaration without replacing absent return metadata with intentional mixed.
fn declared(ty: Option<&EvalParameterType>,owner: &str,called: &str,
    context: &ElephcEvalContext,
) -> Option<Type> {
    resolve_type(&generic_type(ty?),Some(owner),Some(called),context).ok()
}

/// Resolves a class spelling using the same declaration and called scopes as runtime dispatch.
fn class_name(name: &str, context: &ElephcEvalContext) -> Option<String> {
    if name.eq_ignore_ascii_case("static") {
        return context.current_called_class_scope().or_else(|| context.current_class_scope()).map(str::to_string);
    }
    eval_method_parameter_runtime_class_name(name,context).ok()
}

/// Reads a method's declared result from immutable user/native metadata.
pub(in crate::interpreter) fn method_result(class: &str, called: &str, method: &str, static_call: bool,
    context: &ElephcEvalContext,
) -> Option<Type> {
    if let Some((owner,method))=context.class_method(class,method) {
        return declared(method.return_type(),&owner,called,context);
    }
    if let Some((owner,method))=context.interface_method_requirements_with_owners(class).into_iter()
        .find(|(_,declaration)|declaration.name().eq_ignore_ascii_case(method)) {
        return declared(method.return_type(),&owner,called,context);
    }
    let signature=if static_call { context.native_static_method_signature(class,method) }
        else { context.native_method_signature(class,method) }?;
    declared(signature.return_type(),class,called,context)
}

/// Requires a declaration for every nominal receiver alternative before exposing its result.
fn receiver_result(receiver: &Type, context: &ElephcEvalContext,
    result: &mut impl FnMut(&str,&ElephcEvalContext)->Option<Type>,
) -> Option<Type> {
    match receiver {
        Type::Atom(name) if !matches!(name.as_str(),"mixed"|"object"|"null") => result(name,context),
        Type::Apply(name,_) if name != "array" => result(&receiver.to_string(),context),
        Type::Union(members) => {
            let mut results=Vec::new();
            for member in members {
                let ty=receiver_result(member,context,result)?;
                if !results.contains(&ty) { results.push(ty); }
            }
            match results.len() {0=>None,1=>results.pop(),_=>Some(Type::Union(results))}
        }
        _=>None,
    }
}

/// Describes syntactically selected constructors and declared member reads without running PHP.
pub(super) fn producer(expr: &EvalExpr, context: &ElephcEvalContext,
    scope: &ElephcEvalScope,
) -> Option<EvalParameterType> {
    let ty=match expr {
        EvalExpr::NewObject {class_name:name,..}=>nominal_type(&class_name(name,context)?),
        EvalExpr::NewAnonymousClass {class,..}=>Type::atom(class.name()),
        EvalExpr::ClassNameFetch {..}|EvalExpr::DynamicClassNameFetch {..}=>Type::atom("string"),
        EvalExpr::InstanceOf {..}=>Type::atom("bool"),
        EvalExpr::Print(_)=>Type::atom("int"),
        EvalExpr::ArrayGet {array,..}=>offset_result(&generic_type(&source_view(array,context,scope)?))?,
        EvalExpr::Binary {op:EvalBinOp::LogicalAnd|EvalBinOp::LogicalOr,..}=>Type::atom("bool"),
        EvalExpr::StaticMethodCall {class_name:name,method,..}=> {
            let receiver=resolve_eval_static_method_receiver(name,context).ok()?;
            method_result(&receiver.dispatch_class,&receiver.called_class,method,true,context)?
        },
        EvalExpr::MethodCall {object,method,..}=> {
            let receiver=generic_type(&source_view(object,context,scope)?);
            receiver_result(&mixed_source_proofs::value_source(receiver),context,
                &mut |class,context|method_result(class,class,method,false,context))?
        }
        EvalExpr::ClassConstantFetch {class_name:name,constant}=> {
            let class=class_name(name,context)?;
            let (_,constant)=context.class_constant(&class,constant)?;
            if !matches!(constant.value(),EvalExpr::Const(_)) {return None;}
            generic_type(&source_producer_contract(constant.value(),context,scope)?)
        }
        EvalExpr::StaticPropertyGet {class_name:name,property}=> {
            let class=class_name(name,context)?;
            if let Some((owner,property))=context.class_property(&class,property) {
                declared(property.property_type(),&owner,&class,context)?
            } else { declared(context.native_property_type(&class,property).as_ref(),&class,&class,context)? }
        }
        EvalExpr::PropertyGet {object,property}=> {
            let receiver=generic_type(&source_view(object,context,scope)?);
            receiver_result(&mixed_source_proofs::value_source(receiver),context,&mut |class,context| {
                if let Some((owner,property))=context.class_property(class,property) {
                    declared(property.property_type(),&owner,class,context)
                } else { declared(context.native_property_type(class,property).as_ref(),class,class,context) }
            })?
        }
        _=>return None,
    };
    Some(parameter_type(&ty))
}

/// Uses the same structural class spelling as generic runtime declarations and specializations.
pub(super) fn nominal_type(name: &str) -> Type {
    crate::parser::generics::parse_type_string(name).unwrap_or_else(|_| Type::atom(name))
}

/// Projects declared offset elements while preserving every represented receiver alternative.
pub(in crate::interpreter) fn offset_result(receiver: &Type) -> Option<Type> {
    match receiver {
        Type::Apply(name,args) if name == "array"=>args.last().cloned(),
        Type::Atom(name) if name == "array"=>Some(Type::atom("mixed")),
        Type::Atom(name) if name == "string"=>Some(Type::atom("string")),
        Type::Union(members)=>members.iter().map(offset_result).collect::<Option<Vec<_>>>().map(Type::Union),
        Type::Intersection(_) => {
            let outer=mixed_source_proofs::value_source(receiver.clone());
            if mixed_source_proofs::every_source(&outer,&mut |ty|
                matches!(ty,Type::Atom(name)|Type::Apply(name,_) if name == "array")) {
                let schema=element_source_type(receiver.clone(),true);
                if schema != *receiver { return offset_result(&schema); }
            }
            None
        }
        _=>None,
    }
}
