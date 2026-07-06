//! Purpose:
//! Lowers `$obj[$key]` syntax for objects that implement PHP's `ArrayAccess`.
//! Routes read, write, isset, and unset forms through the corresponding `offset*` methods.
//!
//! Called from:
//! - `crate::codegen_support::expr::arrays::access`
//! - `crate::codegen_support::stmt::arrays`
//! - `crate::codegen_support::builtins`
//!
//! Key details:
//! - Receiver evaluation and argument evaluation reuse normal method-call lowering so Mixed boxing
//!   follows the declared `ArrayAccess` method signatures.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::functions;
use crate::parser::ast::Expr;
use crate::types::{FunctionSig, PhpType};

/// Dispatch target for ArrayAccess method calls.
///
/// `Class` indicates a single concrete class that directly implements ArrayAccess
/// and allows static dispatch. `Interface` indicates either an interface type or
/// a union of multiple distinct classes that require virtual dispatch through
/// the ArrayAccess vtable.
enum ArrayAccessDispatchTarget {
    Class(String),
    Interface(String),
}

/// Returns true if the expression type resolves to a PHP ArrayAccess object.
pub(crate) fn expr_is_array_access_object(expr: &Expr, ctx: &Context) -> bool {
    let ty = functions::infer_contextual_type(expr, ctx);
    type_is_array_access_object(&ty, ctx)
}

/// Returns true if the type is an object implementing PHP's ArrayAccess interface.
pub(crate) fn type_is_array_access_object(ty: &PhpType, ctx: &Context) -> bool {
    match ty {
        PhpType::Object(name) => ctx.object_type_implements_interface(name, "ArrayAccess"),
        PhpType::Union(members) => {
            let mut saw_object = false;
            for member in members {
                match member {
                    PhpType::Void => {}
                    PhpType::Object(name) => {
                        if !ctx.object_type_implements_interface(name, "ArrayAccess") {
                            return false;
                        }
                        saw_object = true;
                    }
                    _ => return false,
                }
            }
            saw_object
        }
        _ => false,
    }
}

/// Emits `$obj[$key]` read via ArrayAccess::offsetGet.
pub(crate) fn emit_offset_get(
    object: &Expr,
    index: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emit_array_access_method(object, "offsetget", &[index.clone()], emitter, ctx, data)
}

/// Emits `$obj[$key] = $value` via ArrayAccess::offsetSet.
pub(crate) fn emit_offset_set(
    object: &Expr,
    index: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emit_array_access_method(
        object,
        "offsetset",
        &[index.clone(), value.clone()],
        emitter,
        ctx,
        data,
    )
}

/// Emits `isset($obj[$key])` via ArrayAccess::offsetExists.
pub(crate) fn emit_offset_exists(
    object: &Expr,
    index: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emit_array_access_method(object, "offsetexists", &[index.clone()], emitter, ctx, data)
}

/// Emits `unset($obj[$key])` via ArrayAccess::offsetUnset.
pub(crate) fn emit_offset_unset(
    object: &Expr,
    index: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emit_array_access_method(object, "offsetunset", &[index.clone()], emitter, ctx, data)
}

/// Shared lowering for all ArrayAccess subscript operations.
///
/// Evaluates the receiver and index/value arguments, infers the static dispatch target
/// (class or interface), unboxes Mixed receivers, then delegates to the class or interface
/// method call emitter. Returns the declared return type of the resolved `offset*` method.
fn emit_array_access_method(
    object: &Expr,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("ArrayAccess::{}()", method));
    let static_ty = functions::infer_contextual_type(object, ctx);
    let Some(target) = array_access_dispatch_target(&static_ty, ctx) else {
        emitter.comment("WARNING: ArrayAccess subscript on non-ArrayAccess receiver");
        return PhpType::Int;
    };

    let runtime_ty = crate::codegen_support::expr::emit_expr(object, emitter, ctx, data);
    if matches!(runtime_ty, PhpType::Mixed | PhpType::Union(_)) {
        let message = format!(
            "Fatal error: Call to a member function {}() on null\n",
            method
        );
        crate::codegen_support::expr::objects::emit_unbox_mixed_object_or_fatal(
            message.as_bytes(),
            emitter,
            ctx,
            data,
        );
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // save the ArrayAccess receiver below evaluated offset/value arguments

    let sig = array_access_method_sig(&target, method, ctx);
    let emitted_args = crate::codegen_support::expr::objects::emit_pushed_method_args(
        args,
        sig.as_ref(),
        emitter,
        ctx,
        data,
    );

    match target {
        ArrayAccessDispatchTarget::Class(class_name) => {
            crate::codegen_support::expr::objects::emit_method_call_with_saved_receiver_below_args(
                &class_name,
                method,
                &emitted_args.arg_types,
                emitted_args.source_temp_bytes,
                emitter,
                ctx,
            )
        }
        ArrayAccessDispatchTarget::Interface(interface_name) => {
            emit_interface_method_call_with_saved_receiver_below_args(
                &interface_name,
                method,
                &emitted_args.arg_types,
                emitted_args.source_temp_bytes,
                emitter,
                ctx,
            )
        }
    }
}

/// Infers the static dispatch target for an ArrayAccess method call from a static type.
/// Returns `Class(name)` for a single concrete class, `Interface("ArrayAccess")` when virtual
/// dispatch is required, or `None` if the type does not implement ArrayAccess. Union types must
/// contain only objects implementing ArrayAccess (or Void); any other member yields `None`.
fn array_access_dispatch_target(
    ty: &PhpType,
    ctx: &Context,
) -> Option<ArrayAccessDispatchTarget> {
    match ty {
        PhpType::Object(name) if ctx.classes.contains_key(name) => ctx
            .object_type_implements_interface(name, "ArrayAccess")
            .then(|| ArrayAccessDispatchTarget::Class(name.clone())),
        PhpType::Object(name) if ctx.interfaces.contains_key(name) => ctx
            .object_type_implements_interface(name, "ArrayAccess")
            .then(|| ArrayAccessDispatchTarget::Interface("ArrayAccess".to_string())),
        PhpType::Union(members) => {
            let mut class_target: Option<String> = None;
            let mut needs_interface_dispatch = false;
            let mut saw_array_access_object = false;
            for member in members {
                match member {
                    PhpType::Void => {}
                    PhpType::Object(name) if ctx.classes.contains_key(name) => {
                        if !ctx.object_type_implements_interface(name, "ArrayAccess") {
                            return None;
                        }
                        saw_array_access_object = true;
                        match &class_target {
                            Some(existing) if existing != name => {
                                needs_interface_dispatch = true;
                            }
                            None => {
                                class_target = Some(name.clone());
                            }
                            _ => {}
                        }
                    }
                    PhpType::Object(name) if ctx.interfaces.contains_key(name) => {
                        if !ctx.object_type_implements_interface(name, "ArrayAccess") {
                            return None;
                        }
                        saw_array_access_object = true;
                        needs_interface_dispatch = true;
                    }
                    _ => return None,
                }
            }
            if !saw_array_access_object {
                None
            } else if needs_interface_dispatch {
                Some(ArrayAccessDispatchTarget::Interface(
                    "ArrayAccess".to_string(),
                ))
            } else {
                class_target.map(ArrayAccessDispatchTarget::Class)
            }
        }
        _ => None,
    }
}

/// Looks up the `offsetGet`/`offsetSet`/`offsetExists`/`offsetUnset` method signature
/// from the class or interface metadata for the given dispatch target.
fn array_access_method_sig(
    target: &ArrayAccessDispatchTarget,
    method: &str,
    ctx: &Context,
) -> Option<FunctionSig> {
    match target {
        ArrayAccessDispatchTarget::Class(class_name) => ctx
            .classes
            .get(class_name)
            .and_then(|class_info| class_info.methods.get(method))
            .cloned(),
        ArrayAccessDispatchTarget::Interface(interface_name) => ctx
            .interfaces
            .get(interface_name)
            .and_then(|interface_info| interface_info.methods.get(method))
            .cloned(),
    }
}

/// Emits an interface ArrayAccess method call with the receiver saved below the evaluated arguments.
///
/// The receiver was pushed onto the stack before argument evaluation; this function duplicates
/// it above the arguments, then emits the interface method call and discards all saved slots.
/// Returns the declared return type of the interface method.
fn emit_interface_method_call_with_saved_receiver_below_args(
    interface_name: &str,
    method: &str,
    arg_types: &[PhpType],
    source_temp_bytes: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    let arg_temp_bytes = pushed_arg_temp_bytes(arg_types) + source_temp_bytes;
    abi::emit_load_temporary_stack_slot(
        emitter,
        abi::int_result_reg(emitter),
        arg_temp_bytes,
    );
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // duplicate the saved ArrayAccess receiver above the evaluated arguments
    let ret_ty = emit_interface_method_call_with_pushed_args(
        interface_name,
        method,
        arg_types,
        source_temp_bytes,
        emitter,
        ctx,
    );
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the original receiver slot saved below the argument temporaries
    ret_ty
}

/// Emits an interface method call after arguments have been pushed onto the stack.
///
/// Pops the receiver into the first integer argument register, materializes outgoing
/// arguments per the target ABI, dispatches through the interface vtable, then releases
/// overflow stack arguments and source-order named-argument temporaries. Returns the
/// declared return type of the dispatched interface method.
fn emit_interface_method_call_with_pushed_args(
    interface_name: &str,
    method: &str,
    arg_types: &[PhpType],
    source_temp_bytes: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, arg_types, 1);
    abi::emit_pop_reg(emitter, abi::int_arg_reg_name(emitter.target, 0));      // pop the ArrayAccess receiver into the first integer argument register
    let overflow_bytes = abi::materialize_outgoing_args(emitter, &assignments);
    let ret_ty = crate::codegen_support::expr::objects::dispatch::emit_dispatch_interface_method(
        interface_name,
        method,
        emitter,
        ctx,
    );
    abi::emit_release_temporary_stack(emitter, overflow_bytes);                 // drop spilled stack arguments after the interface method call returns
    abi::emit_release_temporary_stack(emitter, source_temp_bytes);              // drop source-order named-argument temporaries after dispatch
    ret_ty
}

/// Computes the total temporary stack bytes consumed by pushed arguments for
/// ArrayAccess method calls. Non-Void arguments consume 16 bytes each; Void arguments
/// consume 0 bytes.
fn pushed_arg_temp_bytes(arg_types: &[PhpType]) -> usize {
    arg_types
        .iter()
        .map(|ty| if matches!(ty, PhpType::Void) { 0 } else { 16 })
        .sum()
}
