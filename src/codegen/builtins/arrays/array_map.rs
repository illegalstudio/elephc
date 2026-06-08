//! Purpose:
//! Emits PHP `array_map` builtin calls that invoke user-provided callbacks.
//! Owns callback argument materialization, result shape selection, and runtime helper calls.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Callback lowering must preserve PHP source evaluation order, captures, and callable return ownership.

use crate::codegen::abi;
use crate::codegen::callable_dispatch::{self, RuntimeCallableSelector};
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;
use super::array_map_callback_returns_str::callback_returns_str;
use super::callback_env;
use super::call_user_func_array;
use super::runtime_callable_array_callback;

/// Emits the `array_map` builtin call.
///
/// Lowers `array_map(callback, array)` to a runtime helper call, selecting
/// between `__rt_array_map` (scalar results) and `__rt_array_map_str` (string
/// results) based on the inferred callback return type.
///
/// ## Evaluation order
/// The callback expression is evaluated first into `call_reg`, then the array
/// argument is evaluated. Both are pushed to the temporary stack before the
/// runtime call so they occupy the first two integer argument registers.
///
/// ## Capture handling
/// When `callback_env::materialize_callback_address` reports captures, a wrapper
/// environment is built on the temporary stack with the callback entry point in
/// slot 0, the array pointer in the last slot, and capture values in between.
/// The wrapper label address is passed as the first argument, the array pointer
/// as the second, and the environment pointer as the third.
/// Branch-shaped captured callable expressions store the selected descriptor
/// itself in the environment and invoke it through the uniform descriptor invoker.
///
/// ## Runtime helpers
/// - `__rt_array_map`: result array element type is `PhpType::Int`
/// - `__rt_array_map_str`: result array element type is `PhpType::Str`
/// - `__rt_array_map_str_owned`: descriptor-backed string results that are already owned
///
/// Returns `Some(PhpType::Array(Box::new(element_type)))` where element type is
/// `Str` if `callback_returns_str` is true, otherwise `Int`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_map()");
    if args.len() > 2 {
        return emit_two_array_map(&args[0], &args[1], &args[2], emitter, ctx, data);
    }
    let call_reg = abi::nested_call_reg(emitter);
    let result_reg = abi::int_result_reg(emitter);
    let callback_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    let env_arg_reg = abi::int_arg_reg_name(emitter.target, 2);

    // -- determine callback return type at compile time --
    let returns_str = callback_returns_str(args, ctx);
    let source_array_ty = crate::codegen::functions::infer_contextual_type(&args[1], ctx);
    let source_elem_ty = match &source_array_ty {
        PhpType::Array(elem_ty) => elem_ty.codegen_repr(),
        _ => PhpType::Int,
    };

    if call_user_func_array::callback_is_runtime_string(&args[0], ctx) {
        emit_runtime_string_descriptor_map(
            &args[0],
            &args[1],
            &source_array_ty,
            &source_elem_ty,
            emitter,
            ctx,
            data,
        );
        return Some(PhpType::Array(Box::new(PhpType::Mixed)));
    }

    if let Some(array_callback) =
        callback_env::resolve_callable_array_descriptor_callback(&args[0], ctx, data)
    {
        let descriptor_return_type = if matches!(
            array_callback.sig.return_type.codegen_repr(),
            PhpType::Str
        ) {
            PhpType::Str
        } else {
            PhpType::Int
        };
        let descriptor_prefix_types = array_callback
            .receiver_prefix
            .iter()
            .map(|(_, ty)| ty.clone())
            .collect();
        let wrapper = callback_env::emit_descriptor_callback_env_from_static_descriptor(
            &array_callback.descriptor_label,
            vec![source_elem_ty.clone()],
            descriptor_prefix_types,
            descriptor_return_type.clone(),
            emitter,
            ctx,
        );
        if let Some((receiver, receiver_ty)) = &array_callback.receiver_prefix {
            emit_expr(receiver, emitter, ctx, data);
            callback_env::store_descriptor_callback_prefix_result(
                &wrapper,
                0,
                receiver_ty,
                emitter,
            );
        }

        let _arr_ty = emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction(&format!("mov {}, {}", array_arg_reg, result_reg)); // preserve the mapped array pointer before descriptor callback setup
        callback_env::store_descriptor_callback_array_reg(&wrapper, array_arg_reg, emitter);
        callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
        abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
        callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
        if matches!(descriptor_return_type, PhpType::Str) {
            abi::emit_call_label(emitter, "__rt_array_map_str_owned");          // call the string map helper with a callable-array descriptor environment
            callback_env::release_descriptor_callback_env(&wrapper, emitter);
            return Some(PhpType::Array(Box::new(PhpType::Str)));
        }
        abi::emit_call_label(emitter, "__rt_array_map");                        // call the scalar map helper with a callable-array descriptor environment
        callback_env::release_descriptor_callback_env(&wrapper, emitter);
        return Some(PhpType::Array(Box::new(PhpType::Int)));
    }

    if runtime_callable_array_callback::emit_before_array(
        &args[0],
        &args[1],
        array_arg_reg,
        vec![source_elem_ty.clone()],
        PhpType::Mixed,
        emitter,
        ctx,
        data,
        |wrapper, emitter, _ctx, _data| {
            callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
            abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
            callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
            abi::emit_call_label(emitter, "__rt_array_map_mixed");             // call the mixed-result map helper with a runtime callable-array descriptor environment
        },
    ) {
        return Some(PhpType::Array(Box::new(PhpType::Mixed)));
    }

    if callback_env::expr_call_needs_descriptor_callback_env(&args[0], ctx)
        && callback_env::descriptor_callback_env_supported(&args[0])
    {
        // -- evaluate the selected descriptor before the mapped array, matching PHP source order --
        emit_expr(&args[0], emitter, ctx, data);
        let retained_borrowed =
            callback_env::retain_borrowed_descriptor_callback_result(&args[0], emitter);
        abi::emit_push_reg(emitter, result_reg);                                // preserve the selected callable descriptor across mapped-array evaluation

        // -- evaluate the array argument after the callback expression --
        let _arr_ty = emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction(&format!("mov {}, {}", array_arg_reg, result_reg)); // preserve the mapped array pointer before restoring the descriptor
        abi::emit_pop_reg(emitter, result_reg);                                 // restore the selected callable descriptor as the current result

        let descriptor_return_type = if returns_str {
            PhpType::Str
        } else {
            PhpType::Int
        };
        let wrapper = if retained_borrowed {
            callback_env::emit_descriptor_callback_env_from_retained_result(
                &args[0],
                array_arg_reg,
                vec![source_elem_ty.clone()],
                descriptor_return_type.clone(),
                emitter,
                ctx,
            )
        } else {
            callback_env::emit_descriptor_callback_env_from_result(
                &args[0],
                array_arg_reg,
                vec![source_elem_ty.clone()],
                descriptor_return_type.clone(),
                emitter,
                ctx,
            )
        }
        .expect("descriptor callback env support checked before emitting callback");

        callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
        abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
        callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
        if returns_str {
            abi::emit_call_label(emitter, "__rt_array_map_str_owned");          // call the string map helper that consumes descriptor-owned string results
            callback_env::release_descriptor_callback_env(&wrapper, emitter);
            return Some(PhpType::Array(Box::new(PhpType::Str)));
        }
        abi::emit_call_label(emitter, "__rt_array_map");                        // call the scalar array_map runtime helper with a descriptor environment
        callback_env::release_descriptor_callback_env(&wrapper, emitter);
        return Some(PhpType::Array(Box::new(PhpType::Int)));
    }

    // -- evaluate the callback argument first, matching PHP source order --
    let captures =
        callback_env::materialize_callback_address(&args[0], call_reg, emitter, ctx, data);
    abi::emit_push_reg(emitter, call_reg);                                      // save the callback address across mapped-array evaluation

    // -- evaluate the array argument --
    let _arr_ty = emit_expr(&args[1], emitter, ctx, data);

    // -- save array pointer before preparing runtime arguments --
    abi::emit_push_reg(emitter, result_reg);                                    // push the array pointer onto the temporary stack

    if captures.is_empty() {
        // A non-capturing inline closure is invoked directly by the runtime, which passes the
        // element in its element-typed register(s). An untyped closure param would otherwise be
        // compiled for the integer register class and misread a string/non-int element, so
        // specialize it to the source element type before the deferred closure is emitted (mirrors
        // preg_replace_callback). The captured-closure path goes through a wrapper and is left
        // unchanged here.
        specialize_inline_closure_params(&args[0], std::slice::from_ref(&source_elem_ty), ctx);
        abi::emit_pop_reg(emitter, array_arg_reg);                               // pop the mapped array pointer into the second runtime argument register
        abi::emit_pop_reg(emitter, callback_arg_reg);                            // pop the callback address into the first runtime argument register
        abi::emit_load_int_immediate(emitter, env_arg_reg, 0);
    } else {
        abi::emit_pop_reg(emitter, result_reg);                                  // recover the mapped array pointer before building the capture environment
        abi::emit_pop_reg(emitter, call_reg);                                    // recover the callback entry point for env slot zero
        let wrapper = callback_env::emit_captured_callback_env(
            call_reg,
            result_reg,
            &captures,
            vec![source_elem_ty.clone()],
            emitter,
            ctx,
        );
        callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
        abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
        callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
        if returns_str {
            abi::emit_call_label(emitter, "__rt_array_map_str");                // call the string-producing array_map runtime helper
            abi::emit_release_temporary_stack(emitter, wrapper.env_bytes);
            return Some(PhpType::Array(Box::new(PhpType::Str)));
        }
        abi::emit_call_label(emitter, "__rt_array_map");                        // call the scalar array_map runtime helper
        abi::emit_release_temporary_stack(emitter, wrapper.env_bytes);
        return Some(PhpType::Array(Box::new(PhpType::Int)));
    }

    if returns_str {
        abi::emit_call_label(emitter, "__rt_array_map_str");                    // call the string-producing array_map runtime helper
        Some(PhpType::Array(Box::new(PhpType::Str)))
    } else {
        abi::emit_call_label(emitter, "__rt_array_map");                        // call the scalar array_map runtime helper
        Some(PhpType::Array(Box::new(PhpType::Int)))
    }
}

/// Emits the two-input-array form `array_map($callback, $a, $b)`.
///
/// Bounded multi-array support (checker-gated): both arrays carry the same scalar element type
/// (integer or string) and the callback is a named function or a closure.
///
/// For integer arrays the callback is invoked directly with two integer arguments (non-capturing)
/// or through a two-visible-argument wrapper environment (capturing closure), and `__rt_array_map2`
/// collects integer results. For string arrays — restricted by the checker to a non-capturing
/// callback — the closure's untyped params are specialized to `Str` and `__rt_array_map2_str` zips
/// the two string arrays through the callback (padding the shorter with the empty string),
/// collecting string results. Returns `Some(PhpType::Array(elem))` for the input element type.
fn emit_two_array_map(
    callback: &Expr,
    arr0: &Expr,
    arr1: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_map() with two input arrays");
    // The checker guarantees both arrays share one scalar element type (integer or string).
    let elem_is_str = matches!(
        crate::codegen::functions::infer_contextual_type(arr0, ctx),
        PhpType::Array(inner) if matches!(inner.codegen_repr(), PhpType::Str)
    );
    let call_reg = abi::nested_call_reg(emitter);
    let result_reg = abi::int_result_reg(emitter);
    let cb_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let arr0_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    let arr1_arg_reg = abi::int_arg_reg_name(emitter.target, 2);
    let env_arg_reg = abi::int_arg_reg_name(emitter.target, 3);

    // -- evaluate the callback first, then both arrays, in PHP source order --
    let captures =
        callback_env::materialize_callback_address(callback, call_reg, emitter, ctx, data);
    abi::emit_push_reg(emitter, call_reg);                                      // save the callback entry point across array evaluation
    emit_expr(arr0, emitter, ctx, data);
    abi::emit_push_reg(emitter, result_reg);                                    // save the first array pointer
    emit_expr(arr1, emitter, ctx, data);
    abi::emit_push_reg(emitter, result_reg);                                    // save the second array pointer

    // -- restore the arrays into the runtime argument registers and the callback entry --
    abi::emit_pop_reg(emitter, arr1_arg_reg);                                   // restore the second array pointer into the third runtime argument register
    abi::emit_pop_reg(emitter, arr0_arg_reg);                                   // restore the first array pointer into the second runtime argument register
    abi::emit_pop_reg(emitter, call_reg);                                       // restore the callback entry point

    if captures.is_empty() {
        // -- non-capturing callback: invoke it directly with no environment --
        emitter.instruction(&format!("mov {}, {}", cb_arg_reg, call_reg));      // move the callback entry point into the first runtime argument register
        abi::emit_load_int_immediate(emitter, env_arg_reg, 0);                  // non-capturing callbacks need no capture environment
        if elem_is_str {
            // Specialize an untyped closure's params to Str so the closure reads each element as a
            // string (ptr/len) rather than the default integer register, matching what the runtime
            // passes; then collect string results.
            specialize_inline_closure_params(
                callback,
                &[PhpType::Str, PhpType::Str],
                ctx,
            );
            abi::emit_call_label(emitter, "__rt_array_map2_str");              // zip the two string arrays through the callback into a new string list
            return Some(PhpType::Array(Box::new(PhpType::Str)));
        }
        abi::emit_call_label(emitter, "__rt_array_map2");                       // zip the two arrays through the callback into a new integer list
        return Some(PhpType::Array(Box::new(PhpType::Int)));
    }

    // -- capturing closure: build a two-visible-argument wrapper environment and pass it --
    let wrapper = callback_env::emit_captured_callback_env(
        call_reg,
        arr0_arg_reg,
        &captures,
        vec![PhpType::Int, PhpType::Int],
        emitter,
        ctx,
    );
    abi::emit_symbol_address(emitter, cb_arg_reg, &wrapper.wrapper_label);      // move the wrapper entry point into the first runtime argument register
    callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);               // pass the capture environment pointer as the fourth runtime argument
    abi::emit_call_label(emitter, "__rt_array_map2");                           // zip the two arrays through the wrapper into a new integer list
    abi::emit_release_temporary_stack(emitter, wrapper.env_bytes);              // release the temporary capture environment after the runtime call
    Some(PhpType::Array(Box::new(PhpType::Int)))
}

/// Specializes an inline closure callback's untyped parameter types to the element types
/// `array_map` passes at runtime.
///
/// An untyped closure parameter defaults to the integer register class; for a string (or other
/// non-int) source array the runtime passes the element in a different register class (`x0`/`x1`
/// pointer/length for strings), so the closure body must be compiled expecting the element type.
/// Updates the most recently deferred inline closure's signature for each visible parameter,
/// leaving user-declared parameter types untouched. No-op for non-closure callbacks or when no
/// deferred closure is pending. Mirrors `preg_replace_callback`'s closure-parameter specialization.
fn specialize_inline_closure_params(callback: &Expr, elem_tys: &[PhpType], ctx: &mut Context) {
    if !matches!(callback.kind, ExprKind::Closure { .. }) {
        return;
    }
    let Some(deferred) = ctx.deferred_closures.last_mut() else {
        return;
    };
    for (i, elem_ty) in elem_tys.iter().enumerate() {
        // Respect parameters the user annotated with an explicit type.
        if deferred.sig.declared_params.get(i).copied().unwrap_or(true) {
            continue;
        }
        if let Some((_, ty)) = deferred.sig.params.get_mut(i) {
            *ty = elem_ty.clone();
        }
    }
}

/// Emits runtime-string callback selection through descriptor-backed `array_map()`.
fn emit_runtime_string_descriptor_map(
    callback: &Expr,
    array: &Expr,
    source_array_ty: &PhpType,
    source_elem_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let call_reg = abi::nested_call_reg(emitter);
    let result_reg = abi::int_result_reg(emitter);
    let callback_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    let env_arg_reg = abi::int_arg_reg_name(emitter.target, 2);

    let callback_ty = emit_expr(callback, emitter, ctx, data);
    debug_assert!(matches!(callback_ty.codegen_repr(), PhpType::Str));
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                        // preserve the runtime string callback name across mapped-array evaluation

    let _array_ty = emit_expr(array, emitter, ctx, data);
    abi::emit_push_reg(emitter, result_reg);                                    // preserve the mapped array while selecting the runtime string descriptor

    let cases = callable_dispatch::runtime_callable_cases(ctx, data, &[], Some(source_array_ty));
    let done_label = ctx.next_label("array_map_runtime_string_done");
    let selector = RuntimeCallableSelector::StringNameStack {
        ptr_offset: 16,
        len_offset: 24,
        call_reg,
    };

    for case in &cases {
        let next_case = ctx.next_label("array_map_runtime_string_next");
        callable_dispatch::emit_branch_if_callable_case_mismatch(
            &selector,
            case,
            &next_case,
            emitter,
            ctx,
            data,
        );
        abi::emit_load_temporary_stack_slot(emitter, array_arg_reg, 0);
        let wrapper = callback_env::emit_descriptor_callback_env_from_static_descriptor(
            &case.descriptor_label,
            vec![source_elem_ty.clone()],
            Vec::new(),
            PhpType::Mixed,
            emitter,
            ctx,
        );
        callback_env::store_descriptor_callback_array_reg(&wrapper, array_arg_reg, emitter);
        callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
        abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
        callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
        abi::emit_call_label(emitter, "__rt_array_map_mixed");                 // map through the selected runtime string descriptor invoker
        callback_env::release_descriptor_callback_env(&wrapper, emitter);
        abi::emit_jump(emitter, &done_label);
        emitter.label(&next_case);
    }

    call_user_func_array::emit_dynamic_string_callback_abort(emitter, data);
    emitter.label(&done_label);
    abi::emit_release_temporary_stack(emitter, 32);                             // discard the saved mapped array and runtime string callback name
}
