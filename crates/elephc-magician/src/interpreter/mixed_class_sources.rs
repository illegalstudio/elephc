//! Purpose:
//! Owns dynamic class operands and validates their logical source before member selection.
//!
//! Called from:
//! - Mixed constructors, static reads/calls/callables and static mutation statements.
//!
//! Key details:
//! - Class resolution precedes selectors and arguments, unlike ordinary object member reads.
//! - Logical metadata is invocation-local and is never indexed by a runtime value handle.

use super::*;
use elephc_generics::Type;
use crate::eval_ir::generic_type;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot,with_runtime_owners};
mod statements;

pub(super) use statements::{handles as handles_statement,execute as execute_statement};

/// Recognizes expression consumers with a dynamic class operand.
pub(super) fn handles(expr: &EvalExpr) -> bool {
    matches!(expr,EvalExpr::DynamicNewObject {..}|EvalExpr::DynamicStaticMethodCall {..}
        |EvalExpr::DynamicStaticPropertyGet {..}|EvalExpr::DynamicStaticPropertyNameGet {..}
        |EvalExpr::DynamicClassConstantFetch {..}|EvalExpr::DynamicClassConstantNameFetch {..}
        |EvalExpr::DynamicClassNameFetch {..}|EvalExpr::DynamicStaticMethodCallable {..})
}

/// Checks and resolves the class before evaluating an optional dynamic member name.
fn select(value: RuntimeCellHandle, source: Option<&Type>, selector: Option<&EvalExpr>, object_only: bool,
    context: &mut ElephcEvalContext,scope: &mut ElephcEvalScope,values: &mut impl RuntimeValueOps,
) -> Result<(String,Option<String>),EvalStatus> {
    let source=source.cloned().map(mixed_source_proofs::value_source).unwrap_or_else(||Type::atom("mixed"));
    mixed_source_proofs::require_source(&source,"class selection",context,values,|ty,context|
        mixed_method_results::object_source(ty,context) || !object_only && *ty==Type::atom("string"))?;
    let class=if object_only {String::new()} else {eval_dynamic_class_name(value,context,values)?};
    let selector=selector.map(|selector|eval_dynamic_member_name(selector,context,scope,values)).transpose()?;
    Ok((class,selector))
}

/// Roots one class operand through selection and a value-producing consumer.
fn with_value<V:RuntimeValueOps>(class: &EvalExpr, selector: Option<&EvalExpr>, object_only: bool,
    context: &mut ElephcEvalContext,scope: &mut ElephcEvalScope,values: &mut V,
    consume: impl FnOnce(&str,Option<&str>,RuntimeCellHandle,&mut ElephcEvalContext,&mut ElephcEvalScope,&mut V)
        ->Result<(RuntimeCellHandle,Option<Type>),EvalStatus>,
) -> Result<(RuntimeCellHandle,Option<Type>),EvalStatus> {
    mixed_eval::with_sources(&[class],context,scope,values,|operands,sources,context,scope,values| {
        let (class,selector)=select(operands[0],sources[0].as_ref(),selector,object_only,context,scope,values)?;
        consume(&class,selector.as_deref(),operands[0],context,scope,values)
    })
}

/// Roots a class through a statement consumer without creating a synthetic PHP return value.
fn with_void<V:RuntimeValueOps>(class: &EvalExpr, selector: Option<&EvalExpr>,
    context: &mut ElephcEvalContext,scope: &mut ElephcEvalScope,values: &mut V,
    consume: impl FnOnce(&str,Option<&str>,&mut ElephcEvalContext,&mut ElephcEvalScope,&mut V)->Result<(),EvalStatus>,
) -> Result<(),EvalStatus> {
    let (value,source)=mixed_eval::evaluate(class,context,scope,values)?;
    let owner=RuntimeOwnerSlot::new(value);
    let result=with_runtime_owners(&owner,|| {
        let (class,selector)=select(owner.borrowed(),source.as_ref(),selector,false,context,scope,values)?;
        consume(&class,selector.as_deref(),context,scope,values)
    });
    finish_eval_void_arguments(result,[owner.take()],context,values)
}

/// Reads a static property's declaration using the selected class without running its getter twice.
fn property_source(class: &str, property: &str, context: &ElephcEvalContext, scope: &ElephcEvalScope) -> Option<Type> {
    local_producer_contract(&EvalExpr::StaticPropertyGet {class_name:class.into(),property:property.into()},context,scope)
        .as_ref().map(generic_type)
}

/// Reads immutable literal class-constant producer metadata without observing a returned value tag.
fn constant_source(class: &str, name: &str, context: &ElephcEvalContext, scope: &ElephcEvalScope) -> Option<Type> {
    let (_,constant)=context.class_constant(class,name)?;
    if !matches!(constant.value(),EvalExpr::Const(_)) {return None;}
    local_producer_contract(constant.value(),context,scope).as_ref().map(generic_type)
}

/// Evaluates the selected dynamic class consumer through the existing runtime operation adapters.
pub(super) fn evaluate(expr: &EvalExpr,context: &mut ElephcEvalContext,scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle,Option<Type>),EvalStatus> {
    match expr {
        EvalExpr::DynamicNewObject {class_name,args}=>with_value(class_name,None,false,context,scope,values,
            |class,_,_,context,scope,values| {
                let modes=CallArgumentModes::for_constructor(class,context);
                let source=crate::parser::generics::parse_type_string(class).unwrap_or_else(|_|Type::atom(class));
                with_eval_call_arguments_with_modes(args,&modes,context,scope,values,|arguments,context,scope,values|
                    eval_new_object_result(class,arguments,context,scope,values)).map(|value|(value,Some(source)))
            }),
        EvalExpr::DynamicStaticMethodCall {class_name,method,args}=>with_value(class_name,Some(method),false,context,scope,values,
            |class,method,_,context,scope,values| {
                let method=method.ok_or(EvalStatus::RuntimeFatal)?;
                let receiver=resolve_eval_static_method_receiver(class,context)?;
                let source=local_binding_origins::declared_method_result(&receiver.dispatch_class,&receiver.called_class,method,true,context);
                let modes=CallArgumentModes::for_class_method(class,method,true,context);
                with_eval_call_arguments_with_modes(args,&modes,context,scope,values,|arguments,context,scope,values|
                    eval_static_method_call_result_from_scope(class,method,arguments,scope,context,values)).map(|value|(value,source))
            }),
        EvalExpr::DynamicStaticPropertyGet {class_name,property}=>with_value(class_name,None,false,context,scope,values,
            |class,_,_,context,scope,values| {
                let source=property_source(class,property,context,scope);
                eval_static_property_get_with_result_ownership(class,property,context,values,true).map(|value|(value,source))
            }),
        EvalExpr::DynamicStaticPropertyNameGet {class_name,property}=>with_value(class_name,Some(property),false,context,scope,values,
            |class,property,_,context,scope,values| {
                let property=property.ok_or(EvalStatus::RuntimeFatal)?;
                let source=property_source(class,property,context,scope);
                eval_static_property_get_with_result_ownership(class,property,context,values,true).map(|value|(value,source))
            }),
        EvalExpr::DynamicClassConstantFetch {class_name,constant}=>with_value(class_name,None,false,context,scope,values,
            |class,_,_,context,scope,values| {
                let source=constant_source(class,constant,context,scope);
                eval_class_constant_fetch_result(class,constant,context,values).map(|value|(value,source))
            }),
        EvalExpr::DynamicClassConstantNameFetch {class_name,constant}=>with_value(class_name,Some(constant),false,context,scope,values,
            |class,constant,_,context,scope,values| {
                let constant=constant.ok_or(EvalStatus::RuntimeFatal)?;
                let source=constant_source(class,constant,context,scope);
                eval_class_constant_fetch_result(class,constant,context,values).map(|value|(value,source))
            }),
        EvalExpr::DynamicClassNameFetch {class_name}=>with_value(class_name,None,true,context,scope,values,
            |_,_,object,context,_,values|eval_dynamic_class_name_fetch_result(object,context,values)
                .map(|value|(value,Some(Type::atom("string"))))),
        EvalExpr::DynamicStaticMethodCallable {class_name,method}=>with_value(class_name,Some(method),false,context,scope,values,
            |class,method,_,context,scope,values| {
                let receiver=resolve_eval_static_method_receiver(class,context)?;
                let target=eval_static_method_callable_target(receiver.dispatch_class,method.ok_or(EvalStatus::RuntimeFatal)?.into(),
                    Some(receiver.called_class),Some(scope),context,values)?;
                eval_closure_object_expr(target,context,values).map(|value|(value,Some(Type::atom("callable"))))
            }),
        _=>Err(EvalStatus::UnsupportedConstruct),
    }
}
