//! Purpose:
//! Lowers allocation for the builtin ReflectionClass, ReflectionMethod, and
//! ReflectionProperty objects.
//!
//! Called from:
//! - `crate::codegen::expr::objects::allocation::emit_new_object()`
//!
//! Key details:
//! - The public constructors are compile-time reflection lookups: they build a
//!   normal object, then populate private metadata slots from class/member
//!   metadata captured by the type checker.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver};
use crate::types::{AttrArgEntry, PhpType};

/// Compile-time metadata used to populate a freshly allocated reflection owner
/// object before it is returned to user code.
struct ReflectionOwnerMetadata {
    reflected_name: Option<String>,
    attr_names: Vec<String>,
    attr_args: Vec<Option<Vec<AttrArgEntry>>>,
}

/// Returns true if `class_name` is one of the builtin reflection types
/// (ReflectionClass, ReflectionMethod, ReflectionProperty) that require
/// special metadata population instead of normal object construction.
pub(super) fn is_reflection_owner_class(class_name: &str) -> bool {
    matches!(
        class_name,
        "ReflectionClass" | "ReflectionMethod" | "ReflectionProperty"
    )
}

/// Emits the allocation sequence for a builtin reflection object.
///
/// Builds a normal object (ignoring constructor args), saves it on the stack,
/// populates its private metadata slots from compile-time metadata, then restores
/// it as the expression result. Returns `PhpType::Object` for the given class name.
pub(super) fn emit_new_reflection_owner(
    class_name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let normalized_args = normalized_constructor_args(class_name, args, ctx);
    let metadata = reflection_lookup(class_name, &normalized_args, ctx);

    super::allocation::emit_new_object_core(class_name, &[], false, emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // save the Reflection* object while replacing its private attribute array
    if let Some(reflected_name) = metadata.reflected_name.as_deref() {
        crate::codegen::reflection::emit_set_string_property(
            emitter,
            data,
            reflected_name,
            abi::symbol_scratch_reg(emitter),
            8,
            16,
        );
    }
    overwrite_attrs_property(
        class_name,
        &metadata.attr_names,
        &metadata.attr_args,
        emitter,
        ctx,
        data,
    );
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the populated Reflection* object as the expression result
    PhpType::Object(class_name.to_string())
}

/// Normalizes constructor call arguments using the signature for `class_name`'s
/// `__construct` method, falling back to the original args if no signature is
/// available or planning fails.
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

/// Performs compile-time reflection lookup for the given class and constructor
/// arguments, returning the reflected name and attribute metadata captured by
/// the type checker.
///
/// - `ReflectionClass(arg)` → class attribute metadata
/// - `ReflectionMethod(class, method)` → method attribute metadata
/// - `ReflectionProperty(class, prop)` → property attribute metadata
///
/// Returns empty metadata if any argument is non-static or the target doesn't
/// exist.
fn reflection_lookup(
    class_name: &str,
    args: &[Expr],
    ctx: &Context,
) -> ReflectionOwnerMetadata {
    match class_name {
        "ReflectionClass" => {
            let Some(reflected_class) = args.first().and_then(|arg| class_name_arg(arg, ctx)) else {
                return empty_metadata();
            };
            ctx.classes
                .get(&reflected_class)
                .map(|info| ReflectionOwnerMetadata {
                    reflected_name: Some(reflected_class),
                    attr_names: info.attribute_names.clone(),
                    attr_args: info.attribute_args.clone(),
                })
                .unwrap_or_else(empty_metadata)
        }
        "ReflectionMethod" => {
            let Some(reflected_class) = args.first().and_then(|arg| class_name_arg(arg, ctx)) else {
                return empty_metadata();
            };
            let Some(method_name) = args.get(1).and_then(string_literal_arg) else {
                return empty_metadata();
            };
            let method_key = php_symbol_key(&method_name);
            ctx.classes
                .get(&reflected_class)
                .and_then(|info| {
                    Some(ReflectionOwnerMetadata {
                        reflected_name: None,
                        attr_names: info.method_attribute_names.get(&method_key)?.clone(),
                        attr_args: info.method_attribute_args.get(&method_key)?.clone(),
                    })
                })
                .unwrap_or_else(empty_metadata)
        }
        "ReflectionProperty" => {
            let Some(reflected_class) = args.first().and_then(|arg| class_name_arg(arg, ctx)) else {
                return empty_metadata();
            };
            let Some(property_name) = args.get(1).and_then(string_literal_arg) else {
                return empty_metadata();
            };
            ctx.classes
                .get(&reflected_class)
                .and_then(|info| {
                    Some(ReflectionOwnerMetadata {
                        reflected_name: None,
                        attr_names: info.property_attribute_names.get(&property_name)?.clone(),
                        attr_args: info.property_attribute_args.get(&property_name)?.clone(),
                    })
                })
                .unwrap_or_else(empty_metadata)
        }
        _ => empty_metadata(),
    }
}

/// Overwrites the `__attrs` property of the Reflection object saved on the stack.
///
/// First decrements the default empty `__attrs` array, then emits a new array
/// populated from `attr_names` and `attr_args`, then stores the new array pointer
/// and its kind tag (4 = indexed array) into the object's slots at offset 8 and 16.
fn overwrite_attrs_property(
    class_name: &str,
    attr_names: &[String],
    attr_args: &[Option<Vec<AttrArgEntry>>],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let (attrs_low_offset, attrs_high_offset) = reflection_attrs_offsets(class_name);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp]");                                // peek the saved Reflection* object pointer
            emitter.instruction(&format!("ldr x0, [x9, #{}]", attrs_low_offset)); // load the default __attrs array pointer
            emitter.instruction("bl __rt_decref_array");                        // release the default empty attributes array
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rsp]");                    // peek the saved Reflection* object pointer
            emitter.instruction(&format!("mov rax, QWORD PTR [r10 + {}]", attrs_low_offset)); // load the default __attrs array pointer
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
            emitter.instruction(&format!("str x0, [x9, #{}]", attrs_low_offset)); // store the populated __attrs array pointer
            emitter.instruction("mov x10, #4");                                 // runtime kind tag 4 = indexed array
            emitter.instruction(&format!("str x10, [x9, #{}]", attrs_high_offset)); // store the __attrs array kind tag
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rsp]");                    // reload the saved Reflection* object pointer
            emitter.instruction(&format!("mov QWORD PTR [r10 + {}], rax", attrs_low_offset)); // store the populated __attrs array pointer
            emitter.instruction(&format!("mov QWORD PTR [r10 + {}], 4", attrs_high_offset)); // store the __attrs array kind tag
        }
    }
}

/// Returns the low/high object offsets for the private `__attrs` slot.
fn reflection_attrs_offsets(class_name: &str) -> (usize, usize) {
    if class_name == "ReflectionClass" {
        (24, 32)
    } else {
        (8, 16)
    }
}

/// Extracts a class name from `expr` for reflection lookup.
///
/// Handles `StringLiteral` (direct class name) and `ClassConstant` (e.g. `Foo::class`).
/// Returns `None` for other expression kinds.
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

/// Extracts a string value from `expr` if it is a `StringLiteral`.
/// Returns `None` for any other expression kind.
fn string_literal_arg(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(value) => Some(value.clone()),
        _ => None,
    }
}

/// Resolves the class name for a static receiver used in reflection lookups.
///
/// - `Named(name)` → resolves via `resolve_class_name`
/// - `Self_` / `Static` → current class from context
/// - `Parent` → parent class of current class
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

/// Returns empty metadata, used as the fallback when reflection lookup fails.
fn empty_metadata() -> ReflectionOwnerMetadata {
    ReflectionOwnerMetadata {
        reflected_name: None,
        attr_names: Vec::new(),
        attr_args: Vec::new(),
    }
}
