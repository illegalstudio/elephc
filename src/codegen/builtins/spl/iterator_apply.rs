//! Purpose:
//! Emits PHP `iterator_apply()` calls for Iterator/IteratorAggregate objects.
//! Reuses the statement foreach iterator driver while invoking a callback for each valid position.
//!
//! Called from:
//! - `crate::codegen::builtins::spl::emit()`
//!
//! Key details:
//! - The callback is evaluated once before rewind(), and callback falsehood stops iteration before next().
//! - The returned count includes the callback invocation that requested the stop.
//! - Runtime callable-array variables are resolved once before the loop and then invoked through
//!   the selected descriptor for each valid iterator position.

use crate::codegen::abi;
use crate::codegen::builtins::arrays::callback_env;
use crate::codegen::builtins::arrays::call_user_func_array::{
    self, LoadedArraySource,
};
use crate::codegen::builtins::arrays::receiver_call_args;
use crate::codegen::builtins::arrays::runtime_callable_array_callback;
use crate::codegen::callable_dispatch::RuntimeCallableCase;
use crate::codegen::callable_descriptor;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::calls::args as call_args;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::codegen::stmt::{emit_iterable_object_loop, emit_iterator_loop};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};

use super::iterator_common;

/// Emits the iterator apply entry point for this module.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("iterator_apply()");
    let source_ty = emit_expr(&args[0], emitter, ctx, data);
    let source_kind = match source_ty.codegen_repr() {
        PhpType::Iterable => ApplySourceKind::RuntimeIterable,
        PhpType::Object(class_name) if class_name == "Traversable" => {
            ApplySourceKind::TraversableObject
        }
        PhpType::Object(class_name) => ApplySourceKind::StaticObject(class_name),
        _ => {
            return Some(PhpType::Int);
        }
    };
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve iterator receiver while resolving the callback

    let call_reg = abi::nested_call_reg(emitter);
    let runtime_string_callback =
        call_user_func_array::callback_is_runtime_string(&args[1], ctx);
    if !runtime_string_callback
        && runtime_callable_array_callback::emit_without_saved_array(
            &args[1],
            emitter,
            ctx,
            data,
            |case, receiver_ty, emitter, ctx, data| {
                emit_runtime_callable_array_iterator_apply_case(
                    case,
                    receiver_ty,
                    &source_kind,
                    args,
                    emitter,
                    ctx,
                    data,
                );
            },
        )
    {
        return Some(PhpType::Int);
    }
    let (captures, sig, callback_slot_kind, descriptor_arg_prefix) = if runtime_string_callback {
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // save iterator_apply()'s callback-invocation counter before preserving the string callback
        let callback_ty = emit_expr(&args[1], emitter, ctx, data);
        debug_assert!(matches!(callback_ty.codegen_repr(), PhpType::Str));
        let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
        abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                     // save the runtime string callback name beneath the loop receiver
        (Vec::new(), None, CallbackSlotKind::EntryAddress, None)
    } else {
        let is_callable_expr = matches!(
            &args[1].kind,
            ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_)
        );
        let direct_fcc_function =
            crate::codegen::callables::direct_first_class_function_sig(&args[1], ctx);
        let precomputed_sig = direct_fcc_function
            .as_ref()
            .map(|(_, sig)| sig.clone())
            .or_else(|| crate::codegen::callables::callable_sig(&args[1], ctx));
        if let Some(array_callback) =
            callback_env::resolve_callable_array_descriptor_callback(&args[1], ctx, data)
        {
            let descriptor_arg_prefix = array_callback
                .receiver_prefix
                .as_ref()
                .map(|(receiver, _)| receiver.clone());
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // save iterator_apply()'s callback-invocation counter
            abi::emit_symbol_address(emitter, call_reg, &array_callback.descriptor_label);
            abi::emit_push_reg(emitter, call_reg);                              // save the callable-array descriptor beneath the loop receiver
            (
                Vec::new(),
                Some(array_callback.sig),
                CallbackSlotKind::Descriptor,
                descriptor_arg_prefix,
            )
        } else if callback_env::expr_call_needs_descriptor_callback_env(&args[1], ctx)
            && callback_env::descriptor_callback_env_supported(&args[1])
        {
            let callback_ty = emit_expr(&args[1], emitter, ctx, data);
            debug_assert!(matches!(callback_ty.codegen_repr(), PhpType::Callable));
            callback_env::retain_borrowed_descriptor_callback_result(&args[1], emitter);
            emitter.instruction(&format!("mov {}, {}", call_reg, abi::int_result_reg(emitter))); // keep the selected callable descriptor while initializing the counter
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // save iterator_apply()'s callback-invocation counter
            abi::emit_push_reg(emitter, call_reg);                              // save the selected callable descriptor beneath the loop receiver
            (Vec::new(), precomputed_sig, CallbackSlotKind::Descriptor, None)
        } else {
            let captures = if let Some((resolved_name, _)) = direct_fcc_function.as_ref() {
                let label = crate::names::function_symbol(resolved_name);
                abi::emit_symbol_address(emitter, call_reg, &label);
                Vec::new()
            } else {
                callback_env::materialize_callback_address(&args[1], call_reg, emitter, ctx, data)
            };
            let sig: Option<FunctionSig> = if direct_fcc_function.is_none() && is_callable_expr {
                ctx.deferred_closures
                    .last()
                    .map(|deferred| deferred.sig.clone())
            } else {
                precomputed_sig
            };
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // save iterator_apply()'s callback-invocation counter
            abi::emit_push_reg(emitter, call_reg);                              // save the resolved callback address beneath the loop receiver
            (captures, sig, CallbackSlotKind::EntryAddress, None)
        }
    };
    let ret_ty = sig
        .as_ref()
        .map(|sig| sig.return_type.clone())
        .unwrap_or_else(|| if runtime_string_callback { PhpType::Mixed } else { PhpType::Int });

    let callback_arg_source = match callback_args_expr(args) {
        CallbackArgsExpr::Literal(callback_args)
            if runtime_string_callback || callback_slot_kind == CallbackSlotKind::Descriptor =>
        {
            let arg_array = iterator_apply_descriptor_arg_array(
                descriptor_arg_prefix.as_ref(),
                callback_args,
                args.get(2).map(|arg| arg.span).unwrap_or(args[1].span),
            );
            let arg_array_ty = emit_expr(&arg_array, emitter, ctx, data);
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // save iterator_apply()'s synthesized callback-argument array for every invocation
            CallbackArgSource::Dynamic {
                args_offset: 16,
                callback_offset: 32,
                count_offset: 48,
                arg_array_ty,
                literal_elems: Some(callback_args),
            }
        }
        CallbackArgsExpr::Evaluated {
            expr,
            literal_elems: _,
        } if callback_slot_kind == CallbackSlotKind::Descriptor
            && descriptor_arg_prefix.is_some() =>
        {
            let arg_array_ty = crate::codegen::functions::infer_contextual_type(expr, ctx);
            let prefixed = receiver_call_args::emit_receiver_prefixed_dynamic_arg_mixed(
                descriptor_arg_prefix
                    .as_ref()
                    .expect("descriptor prefix checked before dynamic arg rewriting"),
                expr,
                &arg_array_ty,
                emitter,
                ctx,
                data,
            );
            debug_assert!(prefixed);
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // save iterator_apply()'s receiver-prefixed callback-argument array for every invocation
            CallbackArgSource::Dynamic {
                args_offset: 16,
                callback_offset: 32,
                count_offset: 48,
                arg_array_ty: PhpType::Mixed,
                literal_elems: None,
            }
        }
        CallbackArgsExpr::Evaluated {
            expr,
            literal_elems,
        } => {
            let arg_array_ty = emit_expr(expr, emitter, ctx, data);
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // save iterator_apply()'s evaluated callback-argument array for every invocation
            CallbackArgSource::Dynamic {
                args_offset: 16,
                callback_offset: 32,
                count_offset: 48,
                arg_array_ty,
                literal_elems,
            }
        }
        CallbackArgsExpr::Literal(args) => CallbackArgSource::Literal {
            args,
            callback_offset: 16,
            count_offset: 32,
        },
    };
    let receiver_offset = match callback_arg_source {
        CallbackArgSource::Dynamic { .. } => 48,
        CallbackArgSource::Literal { .. } => 32,
    };
    emit_iterator_apply_loops(
        &source_kind,
        &callback_arg_source,
        &captures,
        sig.as_ref(),
        &ret_ty,
        runtime_string_callback,
        callback_slot_kind,
        receiver_offset,
        emitter,
        ctx,
        data,
    );
    if matches!(callback_arg_source, CallbackArgSource::Dynamic { .. }) {
        abi::emit_release_temporary_stack(emitter, 16);                         // discard the evaluated iterator_apply() args array
    }
    if callback_slot_kind == CallbackSlotKind::Descriptor {
        release_saved_descriptor_callback_slot(emitter);
    } else {
        abi::emit_release_temporary_stack(emitter, 16);                         // discard the saved callback slot after iteration
    }
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // return the final iterator_apply() invocation count
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the receiver preserved while resolving the callback
    Some(PhpType::Int)
}

/// Emits one iterator_apply() branch for a runtime-selected callable-array descriptor.
fn emit_runtime_callable_array_iterator_apply_case(
    case: &RuntimeCallableCase,
    receiver_ty: Option<&PhpType>,
    source_kind: &ApplySourceKind,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let call_reg = abi::nested_call_reg(emitter);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // save iterator_apply()'s callback-invocation counter for the selected descriptor
    abi::emit_symbol_address(emitter, call_reg, &case.descriptor_label);
    abi::emit_push_reg(emitter, call_reg);                                      // save the selected callable-array descriptor beneath callback args

    let arg_array_ty =
        emit_runtime_callable_array_iterator_args(receiver_ty, args, emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // save iterator_apply()'s descriptor argument container for every invocation

    let selected_receiver_bytes = usize::from(receiver_ty.is_some()) * 16;
    let callback_arg_source = CallbackArgSource::Dynamic {
        args_offset: 16,
        callback_offset: 32,
        count_offset: 48,
        arg_array_ty,
        literal_elems: None,
    };
    emit_iterator_apply_loops(
        source_kind,
        &callback_arg_source,
        &[],
        Some(&case.sig),
        &PhpType::Mixed,
        false,
        CallbackSlotKind::Descriptor,
        48 + selected_receiver_bytes,
        emitter,
        ctx,
        data,
    );
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the selected descriptor argument container
    release_saved_descriptor_callback_slot(emitter);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // return the final iterator_apply() invocation count
    if selected_receiver_bytes != 0 {
        abi::emit_release_temporary_stack(emitter, selected_receiver_bytes);     // discard the saved runtime callable-array receiver
    }
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the receiver preserved while resolving the callback
}

/// Evaluates iterator_apply() callback args for a runtime-selected callable-array descriptor.
fn emit_runtime_callable_array_iterator_args(
    receiver_ty: Option<&PhpType>,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let args_span = args.get(2).map(|arg| arg.span).unwrap_or(args[1].span);
    let callback_args = callback_args_expr(args);
    match receiver_ty {
        Some(_) => {
            let (arg_expr, arg_ty) = match callback_args {
                CallbackArgsExpr::Literal(callback_args) => (
                    iterator_apply_descriptor_arg_array(None, callback_args, args_span),
                    PhpType::Array(Box::new(PhpType::Mixed)),
                ),
                CallbackArgsExpr::Evaluated { expr, .. } => {
                    (expr.clone(), crate::codegen::functions::infer_contextual_type(expr, ctx))
                }
            };
            let prefixed = receiver_call_args::emit_saved_receiver_prefixed_dynamic_arg_mixed(
                32,
                &arg_expr,
                &arg_ty,
                emitter,
                ctx,
                data,
            );
            debug_assert!(prefixed);
            PhpType::Mixed
        }
        None => match callback_args {
            CallbackArgsExpr::Literal(callback_args) => {
                let arg_array =
                    iterator_apply_descriptor_arg_array(None, callback_args, args_span);
                emit_expr(&arg_array, emitter, ctx, data)
            }
            CallbackArgsExpr::Evaluated { expr, .. } => emit_expr(expr, emitter, ctx, data),
        },
    }
}

/// Emits the shared iterator loop body for iterator_apply() once callback state is staged.
#[allow(clippy::too_many_arguments)]
fn emit_iterator_apply_loops(
    source_kind: &ApplySourceKind,
    callback_arg_source: &CallbackArgSource<'_>,
    captures: &[(String, PhpType, bool)],
    sig: Option<&FunctionSig>,
    ret_ty: &PhpType,
    runtime_string_callback: bool,
    callback_slot_kind: CallbackSlotKind,
    receiver_offset: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    abi::emit_load_temporary_stack_slot(
        emitter,
        abi::int_result_reg(emitter),
        receiver_offset,
    );

    let loop_start = ctx.next_label("iterator_apply_start");
    let loop_end = ctx.next_label("iterator_apply_end");
    let loop_cont = ctx.next_label("iterator_apply_cont");
    match source_kind {
        ApplySourceKind::StaticObject(class_name) => {
            emit_iterator_loop(
                class_name,
                &loop_start,
                &loop_end,
                &loop_cont,
                emitter,
                ctx,
                data,
                |_, _, _, _| (),
                |_, emitter, ctx, data| {
                    emit_callback_invocation(
                        callback_arg_source,
                        captures,
                        sig,
                        ret_ty,
                        runtime_string_callback,
                        callback_slot_kind,
                        &loop_end,
                        emitter,
                        ctx,
                        data,
                    );
                },
                |_, _, _, _| {},
            );
        }
        ApplySourceKind::TraversableObject => {
            emit_iterable_object_loop(
                "iterator_apply_traversable",
                emitter,
                ctx,
                data,
                |_, _, _, _| (),
                |_, active_loop_end, emitter, ctx, data| {
                    emit_callback_invocation(
                        callback_arg_source,
                        captures,
                        sig,
                        ret_ty,
                        runtime_string_callback,
                        callback_slot_kind,
                        active_loop_end,
                        emitter,
                        ctx,
                        data,
                    );
                },
                |_, _, _, _| {},
            );
        }
        ApplySourceKind::RuntimeIterable => {
            emit_apply_loaded_iterable(
                callback_arg_source,
                captures,
                sig,
                ret_ty,
                runtime_string_callback,
                callback_slot_kind,
                emitter,
                ctx,
                data,
            );
        }
    }
}

enum ApplySourceKind {
    StaticObject(String),
    TraversableObject,
    RuntimeIterable,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CallbackSlotKind {
    EntryAddress,
    Descriptor,
}

enum CallbackArgSource<'a> {
    Literal {
        args: &'a [Expr],
        callback_offset: usize,
        count_offset: usize,
    },
    Dynamic {
        args_offset: usize,
        callback_offset: usize,
        count_offset: usize,
        arg_array_ty: PhpType,
        literal_elems: Option<&'a [Expr]>,
    },
}

enum CallbackArgsExpr<'a> {
    Literal(&'a [Expr]),
    Evaluated {
        expr: &'a Expr,
        literal_elems: Option<&'a [Expr]>,
    },
}

/// Builds the AST expression for callback args.
fn callback_args_expr(args: &[Expr]) -> CallbackArgsExpr<'_> {
    match args.get(2) {
        Some(arg) => match &arg.kind {
            ExprKind::Null => CallbackArgsExpr::Literal(&[]),
            ExprKind::ArrayLiteral(elems) if elems.iter().all(is_static_callback_arg_literal) => {
                CallbackArgsExpr::Literal(elems.as_slice())
            }
            ExprKind::ArrayLiteral(elems) => CallbackArgsExpr::Evaluated {
                expr: arg,
                literal_elems: Some(elems.as_slice()),
            },
            _ => CallbackArgsExpr::Evaluated {
                expr: arg,
                literal_elems: None,
            },
        },
        None => CallbackArgsExpr::Literal(&[]),
    }
}

/// Builds the callback argument array, optionally prefixing a callable-array receiver.
fn iterator_apply_descriptor_arg_array(
    descriptor_arg_prefix: Option<&Expr>,
    callback_args: &[Expr],
    span: crate::span::Span,
) -> Expr {
    let mut elems =
        Vec::with_capacity(callback_args.len() + usize::from(descriptor_arg_prefix.is_some()));
    if let Some(prefix) = descriptor_arg_prefix {
        elems.push(prefix.clone());
    }
    elems.extend(callback_args.iter().cloned());
    Expr::new(ExprKind::ArrayLiteral(elems), span)
}

/// Returns true when static callback arg literal.
fn is_static_callback_arg_literal(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => true,
        ExprKind::Negate(inner) => matches!(
            inner.kind,
            ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)
        ),
        _ => false,
    }
}

/// Emits assembly for callback invocation.
fn emit_callback_invocation(
    callback_arg_source: &CallbackArgSource<'_>,
    captures: &[(String, PhpType, bool)],
    sig: Option<&FunctionSig>,
    ret_ty: &PhpType,
    runtime_string_callback: bool,
    callback_slot_kind: CallbackSlotKind,
    loop_end: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let call_reg = abi::nested_call_reg(emitter);
    let callback_offset = match callback_arg_source {
        CallbackArgSource::Literal {
            callback_offset, ..
        }
        | CallbackArgSource::Dynamic {
            callback_offset, ..
        } => *callback_offset,
    };
    if !runtime_string_callback {
        abi::emit_load_temporary_stack_slot(emitter, call_reg, callback_offset);
    }

    if let CallbackArgSource::Dynamic {
        args_offset,
        count_offset,
        arg_array_ty,
        literal_elems,
        ..
    } = callback_arg_source
    {
        let save_concat_before_args = emitter.target.arch == Arch::X86_64;
        if save_concat_before_args {
            crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
        }
        let dynamic_ret_ty = if callback_slot_kind == CallbackSlotKind::Descriptor {
            call_user_func_array::emit_call_descriptor_array_invoker(
                LoadedArraySource::TemporaryStackSlot(*args_offset),
                arg_array_ty,
                call_reg,
                save_concat_before_args,
                emitter,
                ctx,
                data,
            );
            PhpType::Mixed
        } else if runtime_string_callback {
            call_user_func_array::emit_loaded_array_string_callback_call(
                LoadedArraySource::TemporaryStackSlot(*args_offset),
                arg_array_ty,
                callback_offset,
                callback_offset + 8,
                call_reg,
                save_concat_before_args,
                emitter,
                ctx,
                data,
            )
        } else if let Some(sig) = sig {
            call_user_func_array::emit_loaded_array_callback_call(
                LoadedArraySource::TemporaryStackSlot(*args_offset),
                arg_array_ty,
                *literal_elems,
                call_reg,
                captures,
                sig,
                save_concat_before_args,
                emitter,
                ctx,
                data,
            )
        } else {
            call_user_func_array::emit_loaded_array_unknown_callback_call(
                LoadedArraySource::TemporaryStackSlot(*args_offset),
                arg_array_ty,
                call_reg,
                captures,
                None,
                save_concat_before_args,
                emitter,
                ctx,
                data,
            )
        };
        crate::codegen::expr::coerce_to_truthiness(emitter, ctx, &dynamic_ret_ty);
        iterator_common::emit_increment_saved_count_at_offset(*count_offset, emitter);
        emit_branch_if_callback_false(emitter, loop_end);
        return;
    }

    let (callback_args, count_offset) = match callback_arg_source {
        CallbackArgSource::Literal {
            args,
            count_offset,
            ..
        } => (*args, *count_offset),
        CallbackArgSource::Dynamic { .. } => unreachable!(),
    };
    debug_assert!(callback_slot_kind == CallbackSlotKind::EntryAddress);

    let save_concat_before_args = emitter.target.arch == Arch::X86_64;
    if save_concat_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }

    let mut arg_types = Vec::new();
    for (i, arg) in callback_args.iter().enumerate() {
        let target_ty = call_args::declared_target_ty(sig, i);
        let pushed_ty = call_args::push_expr_arg(arg, target_ty, emitter, ctx, data);
        arg_types.push(pushed_ty);
    }

    if let Some(sig) = sig {
        let visible_param_count = sig.params.len();
        let regular_param_count = if sig.variadic.is_some() {
            visible_param_count.saturating_sub(1)
        } else {
            visible_param_count
        };
        for i in arg_types.len()..regular_param_count {
            if let Some(Some(default_expr)) = sig.defaults.get(i) {
                let target_ty = sig.params.get(i).map(|(_, ty)| ty);
                let pushed_ty = call_args::push_expr_arg(default_expr, target_ty, emitter, ctx, data);
                arg_types.push(pushed_ty);
            }
        }
    }
    callback_env::push_captures_as_hidden_args(captures, emitter, ctx, &mut arg_types);

    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, &arg_types, 0);
    let overflow_bytes = abi::materialize_outgoing_args(emitter, &assignments);

    if !save_concat_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }
    abi::emit_call_reg(emitter, call_reg);
    if save_concat_before_args {
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
        crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, ret_ty);
    } else {
        crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, ret_ty);
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
    }

    crate::codegen::expr::coerce_to_truthiness(emitter, ctx, ret_ty);
    iterator_common::emit_increment_saved_count_at_offset(count_offset, emitter);
    emit_branch_if_callback_false(emitter, loop_end);
}

/// Emits assembly for apply loaded iterable.
fn emit_apply_loaded_iterable(
    callback_arg_source: &CallbackArgSource<'_>,
    captures: &[(String, PhpType, bool)],
    sig: Option<&FunctionSig>,
    ret_ty: &PhpType,
    runtime_string_callback: bool,
    callback_slot_kind: CallbackSlotKind,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let object_case = ctx.next_label("iterator_apply_iterable_object");

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve iterable pointer across heap-kind probing
    abi::emit_call_label(emitter, "__rt_heap_kind");                            // classify the iterator_apply() Traversable candidate
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #4");                                  // is the iterable payload an object?
            emitter.instruction(&format!("b.eq {}", object_case));              // dispatch object payloads through Traversable runtime checks
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 4");                                  // is the iterable payload an object?
            emitter.instruction(&format!("je {}", object_case));                // dispatch object payloads through Traversable runtime checks
        }
    }
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // discard non-object iterable payload before reporting unsupported input
    abi::emit_call_label(emitter, "__rt_iterable_unsupported_kind");            // iterator_apply() cannot traverse array payloads

    emitter.label(&object_case);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore object payload before Traversable dispatch
    emit_iterable_object_loop(
        "iterator_apply_iterable",
        emitter,
        ctx,
        data,
        |_, _, _, _| (),
        |_, active_loop_end, emitter, ctx, data| {
            emit_callback_invocation(
                callback_arg_source,
                captures,
                sig,
                ret_ty,
                runtime_string_callback,
                callback_slot_kind,
                active_loop_end,
                emitter,
                ctx,
                data,
            );
        },
        |_, _, _, _| {},
    );
}

/// Releases the retained descriptor stored in iterator_apply()'s saved callback slot.
fn release_saved_descriptor_callback_slot(emitter: &mut Emitter) {
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 0);
    callable_descriptor::emit_release_current_descriptor(emitter);
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the saved callable descriptor after releasing it
}

/// Emits assembly for branch if callback false.
fn emit_branch_if_callback_false(emitter: &mut Emitter, loop_end: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // did iterator_apply() callback request iteration stop?
            emitter.instruction(&format!("b.eq {}", loop_end));                 // stop before next() when callback returned false
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // did iterator_apply() callback request iteration stop?
            emitter.instruction(&format!("je {}", loop_end));                   // stop before next() when callback returned false
        }
    }
}
