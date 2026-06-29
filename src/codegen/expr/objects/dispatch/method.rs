//! Purpose:
//! Lowers instance method target selection and invocation.
//! Shares receiver preparation and ABI call conventions with the object call dispatcher.
//!
//! Called from:
//! - `crate::codegen::expr::objects::dispatch`
//!
//! Key details:
//! - Receiver ownership, late/static binding, and vtable slot layout must match class metadata emission.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::functions;
use crate::intrinsics::IntrinsicCall;
use crate::names::php_symbol_key;
use crate::parser::ast::Expr;
use crate::types::{FunctionSig, PhpType};

use super::intrinsic::emit_instance_intrinsic_with_loaded_args;
use super::interface::emit_dispatch_interface_method;
use super::prep::{compute_register_assignments, eval_and_push_args, pop_args_to_registers};
use super::super::super::emit_expr;
use super::vtable::emit_dispatch_instance_method;

/// Lowers a method call where arguments are already pushed to the temporary stack.
pub(in crate::codegen::expr::objects) fn emit_method_call_with_pushed_args(
    class_name: &str,
    method: &str,
    arg_types: &[PhpType],
    source_temp_bytes: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    let assignments = compute_register_assignments(emitter, arg_types, 1);
    abi::emit_pop_reg(emitter, abi::int_arg_reg_name(emitter.target, 0));      // pop $this into the first integer argument register for the target ABI
    let overflow_bytes = pop_args_to_registers(emitter, &assignments);
    let ret_ty = if let Some(intrinsic) = IntrinsicCall::instance_method(class_name, method) {
        emit_instance_intrinsic_with_loaded_args(
            intrinsic,
            &assignments,
            overflow_bytes,
            emitter,
            ctx,
        )
    } else if ctx.interfaces.contains_key(class_name) {
        emit_dispatch_interface_method(class_name, method, emitter, ctx)
    } else {
        emit_dispatch_instance_method(class_name, method, emitter, ctx)
    };
    abi::emit_release_temporary_stack(emitter, overflow_bytes);                 // drop spilled stack arguments after the method call returns
    abi::emit_release_temporary_stack(emitter, source_temp_bytes);              // drop source-order named-argument temporaries after dispatch
    ret_ty
}

/// Lowers a method call where the receiver was saved below the pushed argument temporaries.
pub(in crate::codegen::expr::objects) fn emit_method_call_with_saved_receiver_below_args(
    class_name: &str,
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
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // duplicate the saved receiver above the evaluated arguments for normal method dispatch
    let ret_ty = emit_method_call_with_pushed_args(
        class_name,
        method,
        arg_types,
        source_temp_bytes,
        emitter,
        ctx,
    );
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the original receiver slot saved below the argument temporaries
    ret_ty
}

/// Evaluates and pushes method arguments, returning metadata for subsequent dispatch.
pub(in crate::codegen::expr::objects) fn emit_pushed_method_args(
    args: &[Expr],
    sig: Option<&crate::types::FunctionSig>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> super::super::super::calls::args::EmittedCallArgs {
    eval_and_push_args(args, sig, emitter, ctx, data)
}

/// Computes the total size in bytes occupied by argument temporaries on the temporary stack.
///
/// Each argument occupies 16 bytes except `Void`-typed arguments which occupy 0 bytes.
/// Used to locate the saved receiver slot when preparing late-binding dispatch.
fn pushed_arg_temp_bytes(arg_types: &[PhpType]) -> usize {
    arg_types
        .iter()
        .map(|ty| if matches!(ty, PhpType::Void) { 0 } else { 16 })
        .sum()
}

/// Lowers a method call expression with receiver, method name, and arguments.
///
/// Handles receiver evaluation order (object before arguments per PHP semantics),
/// nullable/object-union unboxing with fatal-on-null, `__call` magic fallback,
/// and `Fiber::start` special-cased signature. Emits receiver below argument
/// temporaries then delegates to `emit_method_call_with_saved_receiver_below_args`.
pub(in crate::codegen::expr::objects) fn emit_method_call(
    object: &Expr,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("->{}()", method));

    // Resolve the receiver's static class. Accepts a direct object type or
    // a nullable object union (`?Foo`, `Foo|null`) — for those, the
    // singular Object member's class is used and the runtime unbox below
    // turns null receivers into a controlled fatal before dispatch.
    let obj_ty = functions::infer_contextual_type(object, ctx);
    let class_name = match functions::singular_object_class(&obj_ty) {
        Some(cn) => cn.to_string(),
        None => {
            // No single static class. When the receiver could be an object at
            // runtime (a `Mixed` value, or a union of object classes), dispatch
            // on the runtime class id instead of giving up.
            let method_key = php_symbol_key(method);
            let candidates =
                dynamic_dispatch_candidates(&obj_ty, &method_key, args.len(), ctx);
            if !candidates.is_empty() {
                return emit_dynamic_method_call(
                    object, method, args, &candidates, emitter, ctx, data,
                );
            }
            emitter.comment("WARNING: method call on non-object");
            return PhpType::Int;
        }
    };
    // Evaluate the receiver before arguments, matching PHP's left-to-right
    // call order. When the receiver's codegen-level type is Mixed (the
    // runtime representation for nullable / union object parameters), the
    // result register holds a pointer to a boxed mixed cell rather than the
    // raw object — unbox it so the downstream method dispatch receives the
    // underlying object pointer.
    let runtime_obj_ty = emit_expr(object, emitter, ctx, data);
    if matches!(runtime_obj_ty, PhpType::Mixed | PhpType::Union(_)) {
        let message = format!(
            "Fatal error: Call to a member function {}() on null\n",
            method
        );
        super::super::emit_unbox_mixed_object_or_fatal(
            message.as_bytes(),
            emitter,
            ctx,
            data,
        );
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // save the receiver below later argument temporaries for PHP evaluation order

    let method_key = php_symbol_key(method);
    let mut dispatch_method = method_key.as_str();
    let mut magic_args = None;
    let sig = if let Some(class_info) = ctx.classes.get(&class_name) {
        if let Some(sig) = class_info.methods.get(&method_key) {
            Some(sig.clone())
        } else if let Some(sig) = class_info.methods.get("__call") {
            dispatch_method = "__call";
            magic_args = Some(super::super::magic_method_args(method, args, object.span));
            Some(sig.clone())
        } else {
            None
        }
    } else {
        ctx.interfaces
            .get(&class_name)
            .and_then(|interface_info| interface_info.methods.get(&method_key))
            .cloned()
    };
    let args_to_emit = magic_args.as_deref().unwrap_or(args);
    let fiber_start_sig = if class_name == "Fiber" && dispatch_method == "start" {
        crate::codegen::fiber_sigs::fiber_start_sig_for_expr(object, ctx)
            .or_else(|| Some(fiber_start_call_sig(args_to_emit.len())))
    } else {
        None
    };
    let emitted_args = eval_and_push_args(
        args_to_emit,
        fiber_start_sig.as_ref().or(sig.as_ref()),
        emitter,
        ctx,
        data,
    );

    emit_method_call_with_saved_receiver_below_args(
        &class_name,
        dispatch_method,
        &emitted_args.arg_types,
        emitted_args.source_temp_bytes,
        emitter,
        ctx,
    )
}

/// Returns whether `sig` can accept `arg_count` positional arguments: at least the number of
/// required parameters (those without a default) and at most the declared parameter count, unless
/// the signature is variadic (in which case any count at or above the required minimum is accepted).
fn sig_accepts_arg_count(sig: &FunctionSig, arg_count: usize) -> bool {
    let required = (0..sig.params.len())
        .filter(|i| sig.defaults.get(*i).map_or(true, Option::is_none))
        .count();
    if arg_count < required {
        return false;
    }
    sig.variadic.is_some() || arg_count <= sig.params.len()
}

/// Collects the candidate classes for a dynamic method call whose receiver type
/// does not name a single class.
///
/// For a `Mixed` receiver every class that defines `method_key` is a candidate;
/// for a union, the object members that define it are. Returns `(class_name,
/// class_id)` pairs sorted by class id (and de-duplicated) so the emitted dispatch
/// chain is deterministic.
///
/// Candidates are also filtered to those whose method can accept `arg_count`
/// positional arguments. The dispatch marshals arguments once using the first
/// candidate's signature, so candidates with an incompatible arity would corrupt
/// the call — e.g. a user `add(int, int)` and `DateTime::add(DateInterval)` share
/// the name `add` but not the shape. Filtering by arity keeps the shared argument
/// layout valid and makes the candidate set independent of class-id ordering.
fn dynamic_dispatch_candidates(
    obj_ty: &PhpType,
    method_key: &str,
    arg_count: usize,
    ctx: &Context,
) -> Vec<(String, u64)> {
    // A class is a usable candidate only when it declares the method, the method
    // is a normal vtable method (not an intrinsic — SPL containers carry their own
    // argument shapes and special lowering, so a single shared argument layout
    // cannot serve them), and the method accepts `arg_count` arguments. Excluded
    // classes leave a `Mixed` value holding such an object to fault cleanly as
    // "undefined method" rather than miscompiling.
    let dispatchable = |name: &str| -> bool {
        ctx.classes.get(name).is_some_and(|info| {
            info.methods
                .get(method_key)
                .is_some_and(|sig| sig_accepts_arg_count(sig, arg_count))
        }) && IntrinsicCall::instance_method(name, method_key).is_none()
    };
    let mut out: Vec<(String, u64)> = Vec::new();
    match obj_ty {
        PhpType::Mixed => {
            for (name, info) in &ctx.classes {
                if dispatchable(name) {
                    out.push((name.clone(), info.class_id));
                }
            }
        }
        PhpType::Union(members) => {
            for member in members {
                if let PhpType::Object(name) = member {
                    if dispatchable(name) {
                        if let Some(info) = ctx.classes.get(name) {
                            out.push((name.clone(), info.class_id));
                        }
                    }
                }
            }
        }
        _ => {}
    }
    out.sort_by_key(|(_, id)| *id);
    out.dedup_by_key(|(_, id)| *id);
    out
}

/// Lowers a method call on a receiver whose static type does not name a single
/// class by dispatching on the receiver's runtime class id.
///
/// The receiver is evaluated once and unboxed to an object pointer (fataling if
/// the runtime value is not an object). Arguments are laid out once using the
/// first candidate's signature. For each candidate class the runtime class id is
/// compared and, on a match, the call is lowered through the normal static
/// dispatch path for that class (only one branch runs, so the saved receiver and
/// argument temporaries are consumed exactly once). An unmatched class id fatals.
/// Returns the first candidate's return type; same-named methods are expected to
/// share a return representation.
fn emit_dynamic_method_call(
    object: &Expr,
    method: &str,
    args: &[Expr],
    candidates: &[(String, u64)],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("dynamic ->{}() dispatch on runtime class id", method));
    let method_key = php_symbol_key(method);

    let _ = emit_expr(object, emitter, ctx, data);
    let on_non_object = format!(
        "Fatal error: Call to a member function {}() on a non-object\n",
        method
    );
    super::super::emit_unbox_mixed_object_strict_or_fatal(
        on_non_object.as_bytes(),
        emitter,
        ctx,
        data,
    );
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // save the object receiver below later argument temporaries

    let sig = ctx
        .classes
        .get(&candidates[0].0)
        .and_then(|class_info| class_info.methods.get(&method_key))
        .cloned();
    let emitted_args = eval_and_push_args(args, sig.as_ref(), emitter, ctx, data);
    let arg_types = emitted_args.arg_types;
    let source_temp_bytes = emitted_args.source_temp_bytes;

    let arg_temp_bytes = pushed_arg_temp_bytes(&arg_types) + source_temp_bytes;
    let recv_reg = abi::symbol_scratch_reg(emitter);
    let class_id_reg = abi::secondary_scratch_reg(emitter);
    let imm_reg = abi::tertiary_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, recv_reg, arg_temp_bytes);     // peek the saved object receiver beneath the argument temporaries
    abi::emit_load_from_address(emitter, class_id_reg, recv_reg, 0);            // load the runtime class id from the object header

    let done = ctx.next_label("dyn_dispatch_done");
    let mut ret_ty = PhpType::Mixed;
    for (index, (class_name, class_id)) in candidates.iter().enumerate() {
        let next = ctx.next_label("dyn_dispatch_next");
        abi::emit_load_int_immediate(emitter, imm_reg, *class_id as i64);
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("cmp {}, {}", class_id_reg, imm_reg)); // compare the runtime class id with this candidate class
                emitter.instruction(&format!("b.ne {}", next));                 // try the next candidate when the class id differs
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("cmp {}, {}", class_id_reg, imm_reg)); // compare the runtime class id with this candidate class
                emitter.instruction(&format!("jne {}", next));                  // try the next candidate when the class id differs
            }
        }
        let branch_ret = emit_method_call_with_saved_receiver_below_args(
            class_name,
            &method_key,
            &arg_types,
            source_temp_bytes,
            emitter,
            ctx,
        );
        if index == 0 {
            ret_ty = branch_ret;
        }
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("b {}", done));                    // the matched candidate handled the call
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("jmp {}", done));                  // the matched candidate handled the call
            }
        }
        emitter.label(&next);
    }
    let undefined = format!("Fatal error: Call to undefined method {}()\n", method);
    super::super::emit_fatal_str(&undefined, emitter, data);
    emitter.label(&done);
    ret_ty
}

/// Constructs a synthetic `FunctionSig` for `Fiber::start` calls where argument
/// count is determined at runtime.
///
/// The PHP `Fiber::start` method accepts an arbitrary number of `Mixed`-typed
/// arguments and returns `Mixed`. This is distinct from the type-checked catalog
/// signature because the compiler emits call sites with runtime-discovered arity.
fn fiber_start_call_sig(arg_count: usize) -> FunctionSig {
    FunctionSig {
        params: (0..arg_count)
            .map(|idx| (format!("arg{}", idx), PhpType::Mixed))
            .collect(),
        defaults: vec![None; arg_count],
        return_type: PhpType::Mixed,
        declared_return: false,
        by_ref_return: false,
        ref_params: vec![false; arg_count],
        declared_params: vec![false; arg_count],
        variadic: None,
        deprecation: None,
    }
}
