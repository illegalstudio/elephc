//! Purpose:
//! Lowers allocation for the builtin ReflectionClass, ReflectionMethod, and
//! ReflectionProperty objects.
//!
//! Called from:
//! - `crate::codegen::expr::objects::allocation::emit_new_object()`
//!
//! Key details:
//! - The public constructors are compile-time reflection lookups: they build a
//!   normal object, then populate the private `__attrs` slot from class/member
//!   metadata captured by the type checker.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver};
use crate::types::{AttrArgValue, PhpType};

pub(super) fn is_reflection_owner_class(class_name: &str) -> bool {
    matches!(
        class_name,
        "ReflectionClass" | "ReflectionMethod" | "ReflectionProperty"
    )
}

pub(super) fn emit_new_reflection_owner(
    class_name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let normalized_args = normalized_constructor_args(class_name, args, ctx);
    let (attr_names, attr_args) = reflection_lookup(class_name, &normalized_args, ctx);

    super::allocation::emit_new_object_core(class_name, &[], false, emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // save the Reflection* object while replacing its private attribute array
    overwrite_attrs_property(&attr_names, &attr_args, emitter, ctx, data);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the populated Reflection* object as the expression result
    PhpType::Object(class_name.to_string())
}

fn normalized_constructor_args(
    class_name: &str,
    args: &[Expr],
    ctx: &Context,
) -> Vec<Expr> {
    let Some(sig) = ctx
        .classes
        .get(class_name)
        .and_then(|class_info| class_info.methods.get("__construct"))
    else {
        return args.to_vec();
    };
    let span = args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    crate::types::call_args::plan_call_args(
        sig,
        args,
        span,
        false,
        false,
    )
    .map(|plan| plan.normalized_args())
    .unwrap_or_else(|_| args.to_vec())
}

fn reflection_lookup(
    class_name: &str,
    args: &[Expr],
    ctx: &Context,
) -> (Vec<String>, Vec<Option<Vec<AttrArgValue>>>) {
    match class_name {
        "ReflectionClass" => {
            let Some(reflected_class) = args.first().and_then(|arg| class_name_arg(arg, ctx)) else {
                return empty_attrs();
            };
            ctx.classes
                .get(&reflected_class)
                .map(|info| (info.attribute_names.clone(), info.attribute_args.clone()))
                .unwrap_or_else(empty_attrs)
        }
        "ReflectionMethod" => {
            let Some(reflected_class) = args.first().and_then(|arg| class_name_arg(arg, ctx)) else {
                return empty_attrs();
            };
            let Some(method_name) = args.get(1).and_then(string_literal_arg) else {
                return empty_attrs();
            };
            let method_key = php_symbol_key(&method_name);
            ctx.classes
                .get(&reflected_class)
                .and_then(|info| {
                    Some((
                        info.method_attribute_names.get(&method_key)?.clone(),
                        info.method_attribute_args.get(&method_key)?.clone(),
                    ))
                })
                .unwrap_or_else(empty_attrs)
        }
        "ReflectionProperty" => {
            let Some(reflected_class) = args.first().and_then(|arg| class_name_arg(arg, ctx)) else {
                return empty_attrs();
            };
            let Some(property_name) = args.get(1).and_then(string_literal_arg) else {
                return empty_attrs();
            };
            ctx.classes
                .get(&reflected_class)
                .and_then(|info| {
                    Some((
                        info.property_attribute_names.get(&property_name)?.clone(),
                        info.property_attribute_args.get(&property_name)?.clone(),
                    ))
                })
                .unwrap_or_else(empty_attrs)
        }
        _ => empty_attrs(),
    }
}

fn overwrite_attrs_property(
    attr_names: &[String],
    attr_args: &[Option<Vec<AttrArgValue>>],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp]");                                // peek the saved Reflection* object pointer
            emitter.instruction("ldr x0, [x9, #8]");                            // load the default __attrs array pointer
            emitter.instruction("bl __rt_decref_array");                        // release the default empty attributes array
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rsp]");                    // peek the saved Reflection* object pointer
            emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                // load the default __attrs array pointer
            emitter.instruction("call __rt_decref_array");                      // release the default empty attributes array
        }
    }

    crate::codegen::reflection::emit_reflection_attribute_array(
        attr_names,
        attr_args,
        emitter,
        ctx,
        data,
    );

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp]");                                // reload the saved Reflection* object pointer
            emitter.instruction("str x0, [x9, #8]");                            // store the populated __attrs array pointer
            emitter.instruction("mov x10, #4");                                 // runtime kind tag 4 = indexed array
            emitter.instruction("str x10, [x9, #16]");                          // store the __attrs array kind tag
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rsp]");                    // reload the saved Reflection* object pointer
            emitter.instruction("mov QWORD PTR [r10 + 8], rax");                // store the populated __attrs array pointer
            emitter.instruction("mov QWORD PTR [r10 + 16], 4");                 // store the __attrs array kind tag
        }
    }
}

fn class_name_arg(expr: &Expr, ctx: &Context) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(name) => crate::codegen::reflection::resolve_class_name(
            &ctx.classes,
            name,
        )
        .map(str::to_string),
        ExprKind::ClassConstant { receiver } => {
            resolve_static_receiver_class(receiver, ctx)
        }
        _ => None,
    }
}

fn string_literal_arg(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(value) => Some(value.clone()),
        _ => None,
    }
}

fn resolve_static_receiver_class(receiver: &StaticReceiver, ctx: &Context) -> Option<String> {
    match receiver {
        StaticReceiver::Named(name) => crate::codegen::reflection::resolve_class_name(
            &ctx.classes,
            &name.as_canonical(),
        )
        .map(str::to_string),
        StaticReceiver::Self_ | StaticReceiver::Static => ctx.current_class.clone(),
        StaticReceiver::Parent => ctx
            .current_class
            .as_ref()
            .and_then(|class_name| ctx.classes.get(class_name))
            .and_then(|class_info| class_info.parent.clone()),
    }
}

fn empty_attrs() -> (Vec<String>, Vec<Option<Vec<AttrArgValue>>>) {
    (Vec::new(), Vec::new())
}
