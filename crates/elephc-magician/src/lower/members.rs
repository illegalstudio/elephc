//! Purpose:
//! Lowers source-level unary plus and computed member access into EvalIR.
//!
//! Called from:
//! - The shared AST-to-EvalIR expression lowering.
//!
//! Key details:
//! - Class expressions and computed names remain separate interpreter operands.
//! - Object calls keep the nullsafe check ahead of method name and argument evaluation.

use elephc_chopper::syntax::parser::ast::{
    MemberArguments, RuntimeClassMember, RuntimeClassReceiver, RuntimeExpr,
};

use crate::eval_ir::{EvalConst, EvalExpr, EvalUnaryOp};

use super::{Scope, classes::lower_receiver, lower_args, lower_expression, lower_unary};

/// Builds the EvalIR form of syntax retained before compiler desugaring.
pub(super) fn lower_runtime_expression(scope: &Scope<'_>, expression: RuntimeExpr) -> Option<EvalExpr> {
    Some(match expression {
        RuntimeExpr::GenericFunction { name, type_args, arguments } => {
            let (name, fallback) = scope.function_name(&name);
            let type_args = super::templates::lower_types(scope, type_args)?;
            let name = elephc_generics::Type::Apply(name, type_args.clone()).to_string();
            let fallback_name = fallback.map(|name| elephc_generics::Type::Apply(name, type_args).to_string());
            match arguments {
                MemberArguments::Call(args) => {
                    let args = lower_args(scope, args)?;
                    match fallback_name {
                        Some(fallback_name) => EvalExpr::NamespacedCall { name, fallback_name, args },
                        None => EvalExpr::Call { name, args },
                    }
                }
                MemberArguments::Callable => EvalExpr::FunctionCallable { name, fallback_name },
            }
        }
        RuntimeExpr::UnaryPlus(value) => lower_unary(scope, value, EvalUnaryOp::Plus)?,
        RuntimeExpr::GenericObjectMethod { object, method, type_args, arguments, nullsafe } => {
            let object = Box::new(lower_expression(scope, object)?);
            let method = elephc_generics::Type::Apply(method, super::templates::lower_types(scope, type_args)?).to_string();
            match arguments {
                MemberArguments::Call(args) => {
                    let args = lower_args(scope, args)?;
                    if nullsafe {
                        EvalExpr::NullsafeMethodCall { object, method, args }
                    } else {
                        EvalExpr::MethodCall { object, method, args }
                    }
                }
                MemberArguments::Callable if !nullsafe => EvalExpr::MethodCallable {
                    object, method: Box::new(EvalExpr::Const(EvalConst::String(method))),
                },
                MemberArguments::Callable => return None,
            }
        }
        RuntimeExpr::ObjectMemberCall { object, member, arguments, nullsafe } => {
            let object = Box::new(lower_expression(scope, object)?);
            let method = Box::new(lower_expression(scope, member)?);
            match arguments {
                MemberArguments::Call(args) => {
                    let args = lower_args(scope, args)?;
                    if nullsafe {
                        EvalExpr::NullsafeDynamicMethodCall { object, method, args }
                    } else {
                        EvalExpr::DynamicMethodCall { object, method, args }
                    }
                }
                MemberArguments::Callable if !nullsafe => EvalExpr::MethodCallable { object, method },
                MemberArguments::Callable => return None,
            }
        }
        RuntimeExpr::StaticMember { class, member } => match class {
            RuntimeClassReceiver::Named(receiver) => {
                lower_named_class_member(scope, lower_receiver(scope, receiver)?, member)?
            }
            RuntimeClassReceiver::Computed(receiver) => {
                lower_computed_class_member(scope, lower_expression(scope, receiver)?, member)?
            }
        },
        RuntimeExpr::Write { target, operation } => super::writes::lower_write_expression(scope, target, operation)?,
    })
}

/// Lowers static access when the class is written as a name.
fn lower_named_class_member(
    scope: &Scope<'_>,
    class_name: String,
    member: RuntimeClassMember,
) -> Option<EvalExpr> {
    Some(match member {
        RuntimeClassMember::ClassName => EvalExpr::ClassNameFetch { class_name },
        RuntimeClassMember::Property(property) => EvalExpr::StaticPropertyGet { class_name, property },
        RuntimeClassMember::Constant(constant) => EvalExpr::ClassConstantFetch { class_name, constant },
        RuntimeClassMember::GenericMethod { method, type_args, arguments } => {
            let method = elephc_generics::Type::Apply(method, super::templates::lower_types(scope, type_args)?).to_string();
            match arguments {
                MemberArguments::Call(args) => EvalExpr::StaticMethodCall {
                    class_name, method, args: lower_args(scope, args)?,
                },
                MemberArguments::Callable => EvalExpr::StaticMethodCallable {
                    class_name, method: Box::new(EvalExpr::Const(EvalConst::String(method))),
                },
            }
        }
        RuntimeClassMember::Method { name, arguments: MemberArguments::Callable } => {
            EvalExpr::StaticMethodCallable { class_name, method: Box::new(lower_expression(scope, name)?) }
        }
        member => lower_computed_class_member(scope, EvalExpr::ClassNameFetch { class_name }, member)?,
    })
}

/// Lowers static access whose class is obtained by evaluating an expression.
fn lower_computed_class_member(
    scope: &Scope<'_>,
    class: EvalExpr,
    member: RuntimeClassMember,
) -> Option<EvalExpr> {
    let class_name = Box::new(class);
    Some(match member {
        RuntimeClassMember::ClassName => EvalExpr::DynamicClassNameFetch { class_name },
        RuntimeClassMember::Property(property) => EvalExpr::DynamicStaticPropertyGet { class_name, property },
        RuntimeClassMember::ComputedProperty(property) => EvalExpr::DynamicStaticPropertyNameGet {
            class_name, property: Box::new(lower_expression(scope, property)?),
        },
        RuntimeClassMember::Constant(constant) => EvalExpr::DynamicClassConstantFetch { class_name, constant },
        RuntimeClassMember::ComputedConstant(constant) => EvalExpr::DynamicClassConstantNameFetch {
            class_name, constant: Box::new(lower_expression(scope, constant)?),
        },
        RuntimeClassMember::GenericMethod { method, type_args, arguments } => {
            let method = elephc_generics::Type::Apply(method, super::templates::lower_types(scope, type_args)?).to_string();
            let method = Box::new(EvalExpr::Const(EvalConst::String(method)));
            match arguments {
                MemberArguments::Call(args) => EvalExpr::DynamicStaticMethodCall {
                    class_name, method, args: lower_args(scope, args)?,
                },
                MemberArguments::Callable => EvalExpr::DynamicStaticMethodCallable { class_name, method },
            }
        }
        RuntimeClassMember::Method { name, arguments } => {
            let method = Box::new(lower_expression(scope, name)?);
            match arguments {
                MemberArguments::Call(args) => EvalExpr::DynamicStaticMethodCall {
                    class_name, method, args: lower_args(scope, args)?,
                },
                MemberArguments::Callable => EvalExpr::DynamicStaticMethodCallable { class_name, method },
            }
        }
    })
}
