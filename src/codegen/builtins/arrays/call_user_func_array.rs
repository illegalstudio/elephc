//! Purpose:
//! Emits PHP `call_user_func_array` builtin calls that invoke user-provided callbacks.
//! Owns callback argument materialization, result shape selection, and runtime helper calls.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Callback lowering must preserve PHP source evaluation order, captures, and callable return ownership.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::expr::calls::args;
use crate::codegen::platform::Arch;
use crate::codegen::abi;
use crate::codegen::callable_dispatch::{
    self, RuntimeCallableCase, RuntimeCallableSelector,
};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};
use super::callback_env;
use super::callable_forms;
use super::super::callable_lookup::{lookup_function, FunctionLookup};

/// Stamps the heap header of a runtime array with a runtime value-type tag derived from
/// `elem_ty`. Used by `call_user_func_array` to mark variadic tail arrays so the runtime
/// can distinguish element types without compile-time layout information.
///
/// - `array_reg`: register holding the array pointer.
/// - `elem_ty`: the element type whose runtime tag is written into the packed array kind word.
/// Preserves the indexed-array kind and persistent COW flag from the heap header.
fn emit_array_value_type_stamp(emitter: &mut Emitter, array_reg: &str, elem_ty: &PhpType) {
    let value_type_tag = match elem_ty {
        PhpType::Float => 2,
        PhpType::Bool => 3,
        PhpType::Str => 1,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed => 7,
        PhpType::Union(_) => 7,
        PhpType::Void => 8,
        _ => return,
    };
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("ldr x10, [{}, #-8]", array_reg));     // load the packed array kind word from the heap header
            emitter.instruction("mov x12, #0x80ff");                            // preserve the indexed-array kind and persistent COW flag
            emitter.instruction("and x10, x10, x12");                           // keep only the persistent indexed-array metadata bits
            emitter.instruction(&format!("mov x11, #{}", value_type_tag));      // materialize the runtime array value_type tag
            emitter.instruction("lsl x11, x11, #8");                            // move the value_type tag into the packed kind-word byte lane
            emitter.instruction("orr x10, x10, x11");                           // combine the heap kind with the array value_type tag
            emitter.instruction(&format!("str x10, [{}, #-8]", array_reg));     // persist the packed array kind word in the heap header
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("mov r10, QWORD PTR [{} - 8]", array_reg)); // load the packed array kind word from the heap header
            emitter.instruction("mov rdx, 0xffffffff000080ff");                 // materialize the x86_64 indexed-array metadata preservation mask
            emitter.instruction("and r10, rdx");                                // preserve heap marker, indexed-array kind, and persistent COW bits
            emitter.instruction(&format!("mov rcx, {}", value_type_tag));       // materialize the runtime array value_type tag
            emitter.instruction("shl rcx, 8");                                  // move the value_type tag into the packed kind-word byte lane
            emitter.instruction("or r10, rcx");                                 // combine the heap kind with the array value_type tag
            emitter.instruction(&format!("mov QWORD PTR [{} - 8], r10", array_reg)); // persist the packed array kind word in the heap header
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum LoadedArraySource {
    Result,
    TemporaryStackSlot(usize),
}

/// Emits code for the `call_user_func_array($callback, $args)` builtin.
/// Dispatches to extern/builtin call handlers when the callback is statically resolvable,
/// otherwise falls through to full callback resolution, array element extraction, argument
/// materialization, and indirect call via the resolved function address.
///
/// - `$callback` (args[0]): a string naming a function, a Closure, a first-class callable,
///   or any other expression the resolver can materialize into a function pointer + signature.
/// - `$args` (args[1]): an array whose elements are unpacked as positional call arguments.
///
/// Returns the return type of the invoked callback, or `PhpType::Void` if unresolvable.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("call_user_func_array()");
    if let (ExprKind::StringLiteral(name), ExprKind::ArrayLiteral(elems)) =
        (&args[0].kind, &args[1].kind)
    {
        match lookup_function(ctx, name) {
            Some(FunctionLookup::Extern(extern_name)) => {
                return Some(crate::codegen::ffi::emit_extern_call(
                    &extern_name,
                    elems,
                    args[0].span,
                    emitter,
                    ctx,
                    data,
                ));
            }
            Some(FunctionLookup::Builtin(builtin_name)) => {
                if let Some(ret_ty) = crate::codegen::builtins::emit_builtin_call(
                    &builtin_name,
                    elems,
                    args[0].span,
                    emitter,
                    ctx,
                    data,
                ) {
                    return Some(ret_ty);
                }
            }
            Some(FunctionLookup::UserFunction(_)) | Some(FunctionLookup::IncludeVariant(_)) | None => {}
        }
    }
    if let Some(ret_ty) = callable_forms::emit_call_user_func_array_form(
        &args[0],
        &args[1],
        emitter,
        ctx,
        data,
    ) {
        return Some(ret_ty);
    }
    let save_concat_before_args =
        emitter.target.arch == crate::codegen::platform::Arch::X86_64;
    if save_concat_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }
    let call_reg = abi::nested_call_reg(emitter);
    if callback_is_runtime_string(&args[0], ctx) {
        let ret_ty = emit_dynamic_string_callback_with_array_expr(
            &args[0],
            &args[1],
            call_reg,
            save_concat_before_args,
            emitter,
            ctx,
            data,
        );
        return Some(ret_ty);
    }

    // -- resolve callback function address and signature --
    let direct_fcc_function =
        crate::codegen::callables::direct_first_class_function_sig(&args[0], ctx);
    let precomputed_sig = direct_fcc_function
        .as_ref()
        .map(|(_, sig)| sig.clone())
        .or_else(|| crate::codegen::callables::callable_sig(&args[0], ctx));
    let captures = if let Some((resolved_name, _)) = direct_fcc_function.as_ref() {
        let label = crate::names::function_symbol(resolved_name);
        abi::emit_symbol_address(emitter, call_reg, &label);
        Vec::new()
    } else {
        callback_env::materialize_callback_address(&args[0], call_reg, emitter, ctx, data)
    };
    let sig =
        if direct_fcc_function.is_none()
            && matches!(
                &args[0].kind,
                ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_)
            )
        {
            Some(
                ctx.deferred_closures
                    .last()
                    .expect("call_user_func_array: missing synthesized callable signature")
                    .sig
                    .clone(),
            )
        } else {
            precomputed_sig
        };

    let ret_ty = if let Some(sig) = sig {
        if sig.ref_params.iter().any(|is_ref| *is_ref) {
            let arr_ty = emit_expr(&args[1], emitter, ctx, data);
            let literal_arg_elems = match &args[1].kind {
                ExprKind::ArrayLiteral(elems) => Some(elems.as_slice()),
                _ => None,
            };
            emit_loaded_array_callback_call(
                LoadedArraySource::Result,
                &arr_ty,
                literal_arg_elems,
                call_reg,
                &captures,
                &sig,
                save_concat_before_args,
                emitter,
                ctx,
                data,
            )
        } else {
            let inferred_arg_array_ty =
                crate::codegen::functions::infer_contextual_type(&args[1], ctx);
            if should_use_unknown_indexed_dispatch(&sig, &inferred_arg_array_ty, &args[1]) {
                let arr_ty = emit_expr(&args[1], emitter, ctx, data);
                emit_loaded_array_unknown_callback_call(
                    LoadedArraySource::Result,
                    &arr_ty,
                    call_reg,
                    &captures,
                    save_concat_before_args,
                    emitter,
                    ctx,
                    data,
                )
            } else if matches!(inferred_arg_array_ty, PhpType::AssocArray { .. })
                && sig.variadic.is_some()
            {
                let arr_ty = emit_expr(&args[1], emitter, ctx, data);
                emit_loaded_array_callback_call(
                    LoadedArraySource::Result,
                    &arr_ty,
                    None,
                    call_reg,
                    &captures,
                    &sig,
                    save_concat_before_args,
                    emitter,
                    ctx,
                    data,
                )
            } else {
                emit_spread_callback_call_from_array_expr(
                    &args[1],
                    call_reg,
                    &captures,
                    &sig,
                    save_concat_before_args,
                    emitter,
                    ctx,
                    data,
                )
            }
        }
    } else {
        // Evaluate the array argument (second arg)
        let arr_ty = emit_expr(&args[1], emitter, ctx, data);
        emit_loaded_array_unknown_callback_call(
            LoadedArraySource::Result,
            &arr_ty,
            call_reg,
            &captures,
            save_concat_before_args,
            emitter,
            ctx,
            data,
        )
    };

    Some(ret_ty)
}

/// Returns true when use unknown indexed dispatch.
fn should_use_unknown_indexed_dispatch(
    sig: &FunctionSig,
    arg_array_ty: &PhpType,
    arg_array: &Expr,
) -> bool {
    matches!(arg_array_ty, PhpType::Array(_))
        && !matches!(arg_array.kind, ExprKind::ArrayLiteral(_))
        && sig.variadic.is_none()
        && sig.ref_params.iter().all(|is_ref| !*is_ref)
        && sig.defaults.iter().all(Option::is_none)
        && sig.declared_params.iter().all(|declared| !*declared)
}

/// Provides the Callback is runtime string helper used by the call user func array module.
pub(crate) fn callback_is_runtime_string(callback: &Expr, ctx: &Context) -> bool {
    !matches!(callback.kind, ExprKind::StringLiteral(_))
        && matches!(
            crate::codegen::functions::infer_contextual_type(callback, ctx).codegen_repr(),
            PhpType::Str
        )
}

/// Emits assembly for dynamic string callback with array expr.
pub(crate) fn emit_dynamic_string_callback_with_array_expr(
    callback: &Expr,
    arg_array: &Expr,
    call_reg: &str,
    concat_saved_before_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let callback_ty = emit_expr(callback, emitter, ctx, data);
    debug_assert!(matches!(callback_ty.codegen_repr(), PhpType::Str));
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                         // preserve the runtime string callback name while evaluating argument array
    let arr_ty = emit_expr(arg_array, emitter, ctx, data);
    let ret_ty = emit_loaded_array_string_callback_call(
        LoadedArraySource::Result,
        &arr_ty,
        0,
        8,
        call_reg,
        concat_saved_before_args,
        emitter,
        ctx,
        data,
    );
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the preserved runtime string callback name
    ret_ty
}

/// Emits assembly for spread callback call from array expr.
fn emit_spread_callback_call_from_array_expr(
    arg_array: &Expr,
    call_reg: &str,
    captures: &[(String, PhpType, bool)],
    sig: &FunctionSig,
    concat_saved_before_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    abi::emit_push_reg(emitter, call_reg);                                      // preserve the callback address while unpacking call_user_func_array() args
    let spread_arg = Expr::new(ExprKind::Spread(Box::new(arg_array.clone())), arg_array.span);
    let visible_param_count = sig.params.len();
    let regular_param_count = if sig.variadic.is_some() {
        visible_param_count.saturating_sub(1)
    } else {
        visible_param_count
    };
    let emitted_args = args::emit_pushed_call_args(
        &[spread_arg],
        Some(sig),
        regular_param_count,
        "call_user_func_array ref arg",
        true,
        true,
        emitter,
        ctx,
        data,
    );
    let mut arg_types = emitted_args.arg_types;
    callback_env::push_captures_as_hidden_args(captures, emitter, ctx, &mut arg_types);

    let callback_offset = args::pushed_temp_bytes(&arg_types) + emitted_args.source_temp_bytes;
    abi::emit_load_temporary_stack_slot(emitter, call_reg, callback_offset);
    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, &arg_types, 0);
    let overflow_bytes = abi::materialize_outgoing_args(emitter, &assignments);
    let ret_ty = sig.return_type.clone();

    if !concat_saved_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }
    abi::emit_call_reg(emitter, call_reg);
    if concat_saved_before_args {
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
        abi::emit_release_temporary_stack(emitter, emitted_args.source_temp_bytes);
        abi::emit_release_temporary_stack(emitter, 16);
        crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
    } else {
        crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
        abi::emit_release_temporary_stack(emitter, emitted_args.source_temp_bytes);
        abi::emit_release_temporary_stack(emitter, 16);
    }

    ret_ty
}

/// Emits assembly for loaded array callback call.
pub(crate) fn emit_loaded_array_callback_call(
    array_source: LoadedArraySource,
    arr_ty: &PhpType,
    literal_arg_elems: Option<&[Expr]>,
    call_reg: &str,
    captures: &[(String, PhpType, bool)],
    sig: &FunctionSig,
    concat_saved_before_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if matches!(arr_ty, PhpType::AssocArray { .. }) {
        return emit_loaded_assoc_array_callback_call(
            array_source,
            arr_ty,
            call_reg,
            captures,
            sig,
            concat_saved_before_args,
            emitter,
            ctx,
            data,
        );
    }

    let (array_reg, len_reg, tail_count_reg, tail_index_reg, index_reg, offset_reg, data_reg, peek_reg, array_new_capacity_reg, array_new_elem_size_reg, len_store_reg) =
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => (
                "x20", "x21", "x22", "x23", "x24", "x25", "x26", "x9", "x0", "x1", "x10"
            ),
            crate::codegen::platform::Arch::X86_64 => (
                "r13", "r14", "r15", "rbx", "rcx", "r8", "r9", "r11", "rdi", "rsi", "r10"
            ),
        };

    // Determine element type and size from the array type
    let elem_ty = match arr_ty {
        PhpType::Array(t) => *t.clone(),
        PhpType::AssocArray { value, .. } => *value.clone(),
        _ => PhpType::Int,
    };
    let elem_size = args::array_element_stride(&elem_ty);
    let visible_param_count = sig.params.len();
    let regular_param_count = if sig.variadic.is_some() {
        visible_param_count.saturating_sub(1)
    } else {
        visible_param_count
    };

    match array_source {
        LoadedArraySource::Result => {
            emitter.instruction(&format!("mov {}, {}", array_reg, abi::int_result_reg(emitter))); // preserve the callback-argument array pointer across element boxing
        }
        LoadedArraySource::TemporaryStackSlot(offset) => {
            abi::emit_load_temporary_stack_slot(emitter, array_reg, offset);
        }
    }
    abi::emit_load_from_address(emitter, len_reg, array_reg, 0);                // load callback-argument array length
    emit_indexed_required_arg_count_check(
        sig,
        regular_param_count,
        len_reg,
        emitter,
        ctx,
        data,
    );

    // -- extract elements from array and push them as regular call arguments --
    let mut arg_types = Vec::new();
    for i in 0..regular_param_count {
        let is_ref = sig.ref_params.get(i).copied().unwrap_or(false);
        if is_ref {
            if let Some(Expr {
                kind: ExprKind::Variable(var_name),
                ..
            }) = literal_arg_elems.and_then(|elems| elems.get(i))
            {
                if !args::emit_ref_arg_variable_address(
                    var_name,
                    "call_user_func_array ref arg",
                    emitter,
                    ctx,
                ) {
                    panic!("call_user_func_array() by-reference callback argument variable not found");
                }
                args::push_arg_value(emitter, &PhpType::Int);
                arg_types.push(PhpType::Int);
                continue;
            }
            let has_default = sig.defaults.get(i).and_then(|d| d.as_ref()).is_some();
            let target_ty = callback_arg_target_ty(sig, i, has_default, &elem_ty);
            if let Some(default_expr) = sig.defaults.get(i).and_then(|d| d.as_ref()) {
                let load_label = ctx.next_label("cufa_ref_load_arg");
                let done_label = ctx.next_label("cufa_ref_arg_done");
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction(&format!("cmp {}, #{}", len_reg, i + 1)); // compare provided array length before binding a by-reference callback argument
                        emitter.instruction(&format!("b.ge {}", load_label));   // bind the provided element when this by-reference slot exists
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction(&format!("cmp {}, {}", len_reg, i + 1)); // compare provided array length before binding a by-reference callback argument
                        emitter.instruction(&format!("jge {}", load_label));    // bind the provided element when this by-reference slot exists
                    }
                }
                args::push_non_variable_ref_arg_address(
                    default_expr,
                    target_ty,
                    emitter,
                    ctx,
                    data,
                );
                abi::emit_jump(emitter, &done_label);
                emitter.label(&load_label);
                args::load_array_element_to_result(emitter, &elem_ty, array_reg, 24 + i * elem_size);
                args::push_current_result_ref_arg_address(
                    &elem_ty,
                    target_ty,
                    emitter,
                    ctx,
                    data,
                );
                emitter.label(&done_label);
            } else {
                args::load_array_element_to_result(emitter, &elem_ty, array_reg, 24 + i * elem_size);
                args::push_current_result_ref_arg_address(
                    &elem_ty,
                    target_ty,
                    emitter,
                    ctx,
                    data,
                );
            }
            arg_types.push(PhpType::Int);
            continue;
        }
        let has_default = sig.defaults.get(i).and_then(|d| d.as_ref()).is_some();
        let target_ty = callback_arg_target_ty(sig, i, has_default, &elem_ty);
        let pushed_ty = target_ty
            .map(PhpType::codegen_repr)
            .unwrap_or_else(|| elem_ty.codegen_repr());

        if let Some(default_expr) = sig.defaults.get(i).and_then(|d| d.as_ref()) {
            let load_label = ctx.next_label("cufa_load_arg");
            let done_label = ctx.next_label("cufa_arg_done");
            match emitter.target.arch {
                crate::codegen::platform::Arch::AArch64 => {
                    emitter.instruction(&format!("cmp {}, #{}", len_reg, i + 1)); // compare provided array length against required positional index
                }
                crate::codegen::platform::Arch::X86_64 => {
                    emitter.instruction(&format!("cmp {}, {}", len_reg, i + 1)); // compare provided array length against required positional index
                }
            }
            match emitter.target.arch {
                crate::codegen::platform::Arch::AArch64 => {
                    emitter.instruction(&format!("b.ge {}", load_label));       // load an explicit array element when present
                }
                crate::codegen::platform::Arch::X86_64 => {
                    emitter.instruction(&format!("jge {}", load_label));        // load an explicit array element when present
                }
            }
            let _ = args::push_expr_arg(default_expr, target_ty, emitter, ctx, data);
            abi::emit_jump(emitter, &done_label);
            emitter.label(&load_label);
            args::load_array_element_to_result(emitter, &elem_ty, array_reg, 24 + i * elem_size);
            let _ =
                args::push_loaded_array_element_arg(&elem_ty, target_ty, emitter, ctx, data);
            emitter.label(&done_label);
        } else {
            args::load_array_element_to_result(emitter, &elem_ty, array_reg, 24 + i * elem_size);
            let _ =
                args::push_loaded_array_element_arg(&elem_ty, target_ty, emitter, ctx, data);
        }
        arg_types.push(pushed_ty);
    }

    if sig.variadic.is_some() {
        let variadic_elem_ty = sig
            .params
            .get(visible_param_count.saturating_sub(1))
            .and_then(|(_, ty)| match ty {
                PhpType::Array(elem) => Some((**elem).clone()),
                _ => None,
            })
            .unwrap_or_else(|| elem_ty.clone());
        let build_label = ctx.next_label("cufa_build_variadic");
        let done_label = ctx.next_label("cufa_variadic_done");
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("cmp {}, #{}", len_reg, regular_param_count)); // compare provided array length against the fixed arity prefix
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("cmp {}, {}", len_reg, regular_param_count)); // compare provided array length against the fixed arity prefix
            }
        }
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("b.gt {}", build_label));          // build a tail array only when extra positional elements exist
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("jg {}", build_label));            // build a tail array only when extra positional elements exist
            }
        }
        emitter.comment("empty variadic array for call_user_func_array()");
        abi::emit_load_int_immediate(emitter, array_new_capacity_reg, 4);
        abi::emit_load_int_immediate(emitter, array_new_elem_size_reg, variadic_elem_ty.stack_size() as i64);
        abi::emit_call_label(emitter, "__rt_array_new");
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // push the empty variadic array onto the temporary arg stack
        abi::emit_jump(emitter, &done_label);

        emitter.label(&build_label);
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("sub {}, {}, #{}", tail_count_reg, len_reg, regular_param_count)); // compute the count of variadic tail arguments
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, {}", tail_count_reg, len_reg)); // seed the tail count from the provided array length
                emitter.instruction(&format!("sub {}, {}", tail_count_reg, regular_param_count)); // compute the count of variadic tail arguments
            }
        }
        emitter.instruction(&format!("mov {}, {}", array_new_capacity_reg, tail_count_reg)); // pass the exact tail argument count as the initial capacity
        abi::emit_load_int_immediate(emitter, array_new_elem_size_reg, variadic_elem_ty.stack_size() as i64);
        abi::emit_call_label(emitter, "__rt_array_new");
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // keep the variadic array pointer on the stack while filling it
        emitter.instruction(&format!("mov {}, {}", peek_reg, abi::int_result_reg(emitter))); // copy the variadic array pointer into a scratch register for metadata stamping
        emit_array_value_type_stamp(emitter, peek_reg, &variadic_elem_ty);      // stamp the array header with the variadic element runtime tag
        abi::emit_load_int_immediate(emitter, tail_index_reg, 0);
        let loop_label = ctx.next_label("cufa_variadic_loop");
        let loop_done_label = ctx.next_label("cufa_variadic_loop_done");
        emitter.label(&loop_label);
        emitter.instruction(&format!("cmp {}, {}", tail_index_reg, tail_count_reg)); // stop once every tail element has been copied
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("b.ge {}", loop_done_label));      // exit the fill loop when the tail array is complete
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("jge {}", loop_done_label));       // exit the fill loop when the tail array is complete
            }
        }
        emitter.instruction(&format!("mov {}, {}", index_reg, tail_index_reg)); // copy the tail index into a scratch register
        if regular_param_count > 0 {
            match emitter.target.arch {
                crate::codegen::platform::Arch::AArch64 => {
                    emitter.instruction(&format!("add {}, {}, #{}", index_reg, index_reg, regular_param_count)); // offset the tail index by the fixed-arity prefix length
                }
                crate::codegen::platform::Arch::X86_64 => {
                    emitter.instruction(&format!("add {}, {}", index_reg, regular_param_count)); // offset the tail index by the fixed-arity prefix length
                }
            }
        }
        emitter.instruction(&format!("mov {}, {}", data_reg, array_reg));       // start from the callback-argument array pointer before indexing into payload data
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("add {}, {}, #24", data_reg, data_reg)); // skip the fixed array header before indexing variadic source elements
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("add {}, 24", data_reg));          // skip the fixed array header before indexing variadic source elements
            }
        }
        match elem_ty.codegen_repr() {
            PhpType::Str => {
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction(&format!("lsl {}, {}, #4", offset_reg, index_reg)); // compute the 16-byte source slot offset for a string element
                        emitter.instruction(&format!("add {}, {}, {}", data_reg, data_reg, offset_reg)); // advance to the selected source string element
                        let (ptr_reg, len_reg_out) = abi::string_result_regs(emitter);
                        abi::emit_load_from_address(emitter, ptr_reg, data_reg, 0);
                        abi::emit_load_from_address(emitter, len_reg_out, data_reg, 8);
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction(&format!("mov {}, {}", offset_reg, index_reg)); // copy the element index before scaling to bytes
                        emitter.instruction(&format!("shl {}, 4", offset_reg)); // compute the 16-byte source slot offset for a string element
                        emitter.instruction(&format!("add {}, {}", data_reg, offset_reg)); // advance to the selected source string element
                        let (ptr_reg, len_reg_out) = abi::string_result_regs(emitter);
                        abi::emit_load_from_address(emitter, ptr_reg, data_reg, 0);
                        abi::emit_load_from_address(emitter, len_reg_out, data_reg, 8);
                    }
                }
            }
            PhpType::Float => {
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction(&format!("lsl {}, {}, #3", offset_reg, index_reg)); // compute the 8-byte source slot offset for a float element
                        emitter.instruction(&format!("add {}, {}, {}", data_reg, data_reg, offset_reg)); // advance to the selected source float element
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction(&format!("mov {}, {}", offset_reg, index_reg)); // copy the element index before scaling to bytes
                        emitter.instruction(&format!("shl {}, 3", offset_reg)); // compute the 8-byte source slot offset for a float element
                        emitter.instruction(&format!("add {}, {}", data_reg, offset_reg)); // advance to the selected source float element
                    }
                }
                abi::emit_load_from_address(emitter, abi::float_result_reg(emitter), data_reg, 0);
            }
            PhpType::Void => {}
            _ => {
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction(&format!("lsl {}, {}, #3", offset_reg, index_reg)); // compute the 8-byte source slot offset for a scalar or boxed element
                        emitter.instruction(&format!("add {}, {}, {}", data_reg, data_reg, offset_reg)); // advance to the selected source scalar element
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction(&format!("mov {}, {}", offset_reg, index_reg)); // copy the element index before scaling to bytes
                        emitter.instruction(&format!("shl {}, 3", offset_reg)); // compute the 8-byte source slot offset for a scalar or boxed element
                        emitter.instruction(&format!("add {}, {}", data_reg, offset_reg)); // advance to the selected source scalar element
                    }
                }
                abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), data_reg, 0);
            }
        }
        let (stored_ty, boxed_to_mixed) = args::coerce_current_value_to_target(
            emitter,
            ctx,
            data,
            &elem_ty,
            Some(&variadic_elem_ty),
        );
        if !boxed_to_mixed {
            abi::emit_incref_if_refcounted(emitter, &elem_ty.codegen_repr());   // retain refcounted tail elements copied into the new variadic array
        }
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("ldr {}, [sp]", peek_reg));        // reload the variadic array pointer from the stack
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", peek_reg)); // reload the variadic array pointer from the stack
            }
        }
        match stored_ty {
            PhpType::Int
            | PhpType::Bool
            | PhpType::Resource(_)
            | PhpType::Callable
            | PhpType::Iterable
            | PhpType::Mixed
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Buffer(_)
            | PhpType::Object(_)
            | PhpType::Packed(_)
            | PhpType::Pointer(_)
            | PhpType::Union(_) | PhpType::Never => {
                let dest_reg = len_store_reg;
                emitter.instruction(&format!("mov {}, {}", dest_reg, peek_reg)); // point at the variadic array before skipping the header
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction(&format!("add {}, {}, #24", dest_reg, dest_reg)); // point at the variadic array payload
                        emitter.instruction(&format!("lsl {}, {}, #3", offset_reg, tail_index_reg)); // compute the 8-byte destination slot offset
                        emitter.instruction(&format!("add {}, {}, {}", dest_reg, dest_reg, offset_reg)); // advance to the selected variadic destination slot
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction(&format!("add {}, 24", dest_reg));  // point at the variadic array payload
                        emitter.instruction(&format!("mov {}, {}", offset_reg, tail_index_reg)); // copy the destination index before scaling
                        emitter.instruction(&format!("shl {}, 3", offset_reg)); // compute the 8-byte destination slot offset
                        emitter.instruction(&format!("add {}, {}", dest_reg, offset_reg)); // advance to the selected variadic destination slot
                    }
                }
                abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), dest_reg, 0);
            }
            PhpType::Float => {
                let dest_reg = len_store_reg;
                emitter.instruction(&format!("mov {}, {}", dest_reg, peek_reg)); // point at the variadic array before skipping the header
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction(&format!("add {}, {}, #24", dest_reg, dest_reg)); // point at the variadic array payload
                        emitter.instruction(&format!("lsl {}, {}, #3", offset_reg, tail_index_reg)); // compute the 8-byte destination slot offset
                        emitter.instruction(&format!("add {}, {}, {}", dest_reg, dest_reg, offset_reg)); // advance to the selected variadic destination slot
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction(&format!("add {}, 24", dest_reg));  // point at the variadic array payload
                        emitter.instruction(&format!("mov {}, {}", offset_reg, tail_index_reg)); // copy the destination index before scaling
                        emitter.instruction(&format!("shl {}, 3", offset_reg)); // compute the 8-byte destination slot offset
                        emitter.instruction(&format!("add {}, {}", dest_reg, offset_reg)); // advance to the selected variadic destination slot
                    }
                }
                abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), dest_reg, 0);
            }
            PhpType::Str => {
                let dest_reg = len_store_reg;
                let (ptr_reg, len_reg_out) = abi::string_result_regs(emitter);
                emitter.instruction(&format!("mov {}, {}", dest_reg, peek_reg)); // point at the variadic array before skipping the header
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction(&format!("add {}, {}, #24", dest_reg, dest_reg)); // point at the variadic array payload
                        emitter.instruction(&format!("lsl {}, {}, #4", offset_reg, tail_index_reg)); // compute the 16-byte destination slot offset
                        emitter.instruction(&format!("add {}, {}, {}", dest_reg, dest_reg, offset_reg)); // advance to the selected variadic destination slot
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction(&format!("add {}, 24", dest_reg));  // point at the variadic array payload
                        emitter.instruction(&format!("mov {}, {}", offset_reg, tail_index_reg)); // copy the destination index before scaling
                        emitter.instruction(&format!("shl {}, 4", offset_reg)); // compute the 16-byte destination slot offset
                        emitter.instruction(&format!("add {}, {}", dest_reg, offset_reg)); // advance to the selected variadic destination slot
                    }
                }
                abi::emit_store_to_address(emitter, ptr_reg, dest_reg, 0);
                abi::emit_store_to_address(emitter, len_reg_out, dest_reg, 8);
            }
            PhpType::Void => {}
        }
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("add {}, {}, #1", tail_index_reg, tail_index_reg)); // advance to the next tail element
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("add {}, 1", tail_index_reg));     // advance to the next tail element
            }
        }
        abi::emit_store_to_address(emitter, tail_index_reg, peek_reg, 0);       // persist the updated variadic array length
        abi::emit_jump(emitter, &loop_label);
        emitter.label(&loop_done_label);
        emitter.label(&done_label);
        arg_types.push(PhpType::Array(Box::new(variadic_elem_ty)));
    }
    callback_env::push_captures_as_hidden_args(&captures, emitter, ctx, &mut arg_types);

    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, &arg_types, 0);
    let overflow_bytes = abi::materialize_outgoing_args(emitter, &assignments);

    let ret_ty = sig.return_type.clone();

    // -- call callback via the resolved address in x19 --
    if !concat_saved_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }
    abi::emit_call_reg(emitter, call_reg);
    if concat_saved_before_args {
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
        crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
    } else {
        crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
    }

    ret_ty
}

/// Provides the Callback arg target ty helper used by the call user func array module.
fn callback_arg_target_ty<'a>(
    sig: &'a FunctionSig,
    index: usize,
    has_default: bool,
    source_elem_ty: &PhpType,
) -> Option<&'a PhpType> {
    if args::declared_target_ty(Some(sig), index).is_some()
        || has_default
        || matches!(source_elem_ty.codegen_repr(), PhpType::Mixed)
    {
        sig.params.get(index).map(|(_, ty)| ty)
    } else {
        None
    }
}

/// Emits assembly for indexed required arg count check.
fn emit_indexed_required_arg_count_check(
    sig: &FunctionSig,
    regular_param_count: usize,
    len_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let required_count = (0..regular_param_count)
        .filter(|idx| sig.defaults.get(*idx).and_then(|default| default.as_ref()).is_none())
        .map(|idx| idx + 1)
        .max()
        .unwrap_or(0);
    if required_count == 0 {
        return;
    }
    let ok_label = ctx.next_label("cufa_indexed_required_ok");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #{}", len_reg, required_count)); // check that the dynamic indexed arg array contains all required callback parameters
            emitter.instruction(&format!("b.ge {}", ok_label));                 // continue when every required callback parameter is present
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", len_reg, required_count)); // check that the dynamic indexed arg array contains all required callback parameters
            emitter.instruction(&format!("jge {}", ok_label));                  // continue when every required callback parameter is present
        }
    }
    emit_call_user_func_array_missing_arg_abort(emitter, data);
    emitter.label(&ok_label);
}

/// Emits assembly for loaded assoc array callback call.
fn emit_loaded_assoc_array_callback_call(
    array_source: LoadedArraySource,
    arr_ty: &PhpType,
    call_reg: &str,
    captures: &[(String, PhpType, bool)],
    sig: &FunctionSig,
    concat_saved_before_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let hash_reg = match emitter.target.arch {
        Arch::AArch64 => "x20",
        Arch::X86_64 => "r13",
    };
    let elem_ty = match arr_ty {
        PhpType::AssocArray { value, .. } => *value.clone(),
        _ => PhpType::Int,
    };

    match array_source {
        LoadedArraySource::Result => {
            emitter.instruction(&format!("mov {}, {}", hash_reg, abi::int_result_reg(emitter))); // preserve the callback-argument hash pointer across named lookups
        }
        LoadedArraySource::TemporaryStackSlot(offset) => {
            abi::emit_load_temporary_stack_slot(emitter, hash_reg, offset);
        }
    }

    let visible_param_count = sig.params.len();
    let regular_param_count = if sig.variadic.is_some() {
        visible_param_count.saturating_sub(1)
    } else {
        visible_param_count
    };
    let mut arg_types = Vec::new();

    for i in 0..regular_param_count {
        let has_default = sig.defaults.get(i).and_then(|d| d.as_ref()).is_some();
        let target_ty = callback_arg_target_ty(sig, i, has_default, &elem_ty);
        let param_name = sig.params.get(i).map(|(name, _)| name.as_str());
        emitter.comment("lookup call_user_func_array() named argument");
        args::emit_hash_lookup_for_param_or_index(
            hash_reg,
            param_name,
            i,
            emitter,
            ctx,
            data,
        );

        let is_ref = sig.ref_params.get(i).copied().unwrap_or(false);
        if is_ref {
            if let Some(default_expr) = sig.defaults.get(i).and_then(|d| d.as_ref()) {
                let use_default = ctx.next_label("cufa_assoc_ref_default");
                let done = ctx.next_label("cufa_assoc_ref_done");
                abi::emit_branch_if_int_result_zero(emitter, &use_default);
                args::push_loaded_hash_value_ref_arg(&elem_ty, target_ty, emitter, ctx, data);
                abi::emit_jump(emitter, &done);
                emitter.label(&use_default);
                args::push_non_variable_ref_arg_address(
                    default_expr,
                    target_ty,
                    emitter,
                    ctx,
                    data,
                );
                emitter.label(&done);
            } else {
                let missing = ctx.next_label("cufa_assoc_ref_missing");
                let done = ctx.next_label("cufa_assoc_ref_done");
                abi::emit_branch_if_int_result_zero(emitter, &missing);
                args::push_loaded_hash_value_ref_arg(&elem_ty, target_ty, emitter, ctx, data);
                abi::emit_jump(emitter, &done);
                emitter.label(&missing);
                emit_call_user_func_array_missing_arg_abort(emitter, data);
                emitter.label(&done);
            }
            arg_types.push(PhpType::Int);
            continue;
        }

        let pushed_ty = if let Some(default_expr) = sig.defaults.get(i).and_then(|d| d.as_ref()) {
            let use_default = ctx.next_label("cufa_assoc_default");
            let done = ctx.next_label("cufa_assoc_done");
            abi::emit_branch_if_int_result_zero(emitter, &use_default);
            let loaded_ty = args::push_loaded_hash_value_arg(&elem_ty, target_ty, emitter, ctx, data);
            abi::emit_jump(emitter, &done);
            emitter.label(&use_default);
            let default_ty = args::push_expr_arg(default_expr, target_ty, emitter, ctx, data);
            emitter.label(&done);
            widen_callback_arg_type(&loaded_ty, &default_ty)
        } else {
            let missing = ctx.next_label("cufa_assoc_missing");
            let done = ctx.next_label("cufa_assoc_done");
            abi::emit_branch_if_int_result_zero(emitter, &missing);
            let loaded_ty = args::push_loaded_hash_value_arg(&elem_ty, target_ty, emitter, ctx, data);
            abi::emit_jump(emitter, &done);
            emitter.label(&missing);
            emit_call_user_func_array_missing_arg_abort(emitter, data);
            emitter.label(&done);
            loaded_ty
        };
        arg_types.push(pushed_ty);
    }

    if sig.variadic.is_some() {
        let variadic_ty = emit_loaded_assoc_variadic_array_arg(
            hash_reg,
            &elem_ty,
            sig,
            regular_param_count,
            emitter,
            ctx,
            data,
        );
        arg_types.push(variadic_ty);
    }

    callback_env::push_captures_as_hidden_args(captures, emitter, ctx, &mut arg_types);
    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, &arg_types, 0);
    let overflow_bytes = abi::materialize_outgoing_args(emitter, &assignments);
    let ret_ty = sig.return_type.clone();

    if !concat_saved_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }
    abi::emit_call_reg(emitter, call_reg);
    if concat_saved_before_args {
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
        crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
    } else {
        crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
    }

    ret_ty
}

/// Emits assembly for loaded assoc variadic array arg.
fn emit_loaded_assoc_variadic_array_arg(
    source_hash_reg: &str,
    elem_ty: &PhpType,
    sig: &FunctionSig,
    regular_param_count: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let visible_param_count = sig.params.len();
    let variadic_elem_ty = sig
        .params
        .get(visible_param_count.saturating_sub(1))
        .and_then(|(_, ty)| match ty {
            PhpType::Array(elem) => Some((**elem).clone()),
            _ => None,
        })
        .unwrap_or_else(|| elem_ty.clone());
    let variadic_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(variadic_elem_ty.clone()),
    };
    let capacity_reg = abi::int_arg_reg_name(emitter.target, 0);
    let tag_reg = abi::int_arg_reg_name(emitter.target, 1);

    emitter.comment("build associative variadic array for callback");
    abi::emit_load_int_immediate(emitter, capacity_reg, 16);
    abi::emit_load_int_immediate(
        emitter,
        tag_reg,
        crate::codegen::runtime_value_tag(&variadic_elem_ty.codegen_repr()) as i64,
    );
    abi::emit_call_label(emitter, "__rt_hash_new");
    abi::emit_push_result_value(emitter, &variadic_ty);

    emit_loaded_assoc_variadic_entries(
        source_hash_reg,
        sig,
        regular_param_count,
        emitter,
        ctx,
        data,
    );

    variadic_ty
}

/// Emits assembly for loaded assoc variadic entries.
fn emit_loaded_assoc_variadic_entries(
    source_hash_reg: &str,
    sig: &FunctionSig,
    regular_param_count: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    const SCRATCH_BYTES: usize = 96;
    const CURSOR_OFF: usize = 0;
    const SOURCE_HASH_OFF: usize = 8;
    const KEY_PTR_OFF: usize = 16;
    const KEY_LEN_OFF: usize = 24;
    const VALUE_LO_OFF: usize = 32;
    const VALUE_HI_OFF: usize = 40;
    const VALUE_TAG_OFF: usize = 48;
    const NUMERIC_KEY_OFF: usize = 56;

    let loop_label = ctx.next_label("cufa_assoc_variadic_loop");
    let done_label = ctx.next_label("cufa_assoc_variadic_done");
    let skip_label = ctx.next_label("cufa_assoc_variadic_skip");
    let numeric_key_label = ctx.next_label("cufa_assoc_variadic_numeric_key");
    let string_key_label = ctx.next_label("cufa_assoc_variadic_string_key");
    let insert_label = ctx.next_label("cufa_assoc_variadic_insert");
    let value_string_label = ctx.next_label("cufa_assoc_variadic_value_string");
    let value_ref_label = ctx.next_label("cufa_assoc_variadic_value_ref");
    let value_scalar_label = ctx.next_label("cufa_assoc_variadic_value_scalar");
    let insert_call_label = ctx.next_label("cufa_assoc_variadic_insert_call");

    abi::emit_reserve_temporary_stack(emitter, SCRATCH_BYTES);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("str {}, [sp, #{}]", source_hash_reg, SOURCE_HASH_OFF)); // save the callback-argument hash for the variadic scan
            emitter.instruction(&format!("str xzr, [sp, #{}]", CURSOR_OFF));    // start hash iteration from the insertion-order head
            emitter.instruction(&format!("str xzr, [sp, #{}]", NUMERIC_KEY_OFF)); // start numeric variadic keys from zero
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov QWORD PTR [rsp + {}], {}", SOURCE_HASH_OFF, source_hash_reg)); // save the callback-argument hash for the variadic scan
            emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", CURSOR_OFF)); // start hash iteration from the insertion-order head
            emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", NUMERIC_KEY_OFF)); // start numeric variadic keys from zero
        }
    }

    emitter.label(&loop_label);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x0", SOURCE_HASH_OFF);
            abi::emit_load_temporary_stack_slot(emitter, "x1", CURSOR_OFF);
            abi::emit_call_label(emitter, "__rt_hash_iter_next");
            emitter.instruction("cmn x0, #1");                                  // has the associative argument scan reached the terminal cursor?
            emitter.instruction(&format!("b.eq {}", done_label));               // finish the variadic hash once every source entry was visited
            abi::emit_store_to_address(emitter, "x0", "sp", CURSOR_OFF);
            abi::emit_store_to_address(emitter, "x1", "sp", KEY_PTR_OFF);
            abi::emit_store_to_address(emitter, "x2", "sp", KEY_LEN_OFF);
            abi::emit_store_to_address(emitter, "x3", "sp", VALUE_LO_OFF);
            abi::emit_store_to_address(emitter, "x4", "sp", VALUE_HI_OFF);
            abi::emit_store_to_address(emitter, "x5", "sp", VALUE_TAG_OFF);
            emitter.instruction("cmn x2, #1");                                  // is the current callback-argument key numeric?
            emitter.instruction(&format!("b.eq {}", numeric_key_label));        // numeric keys are positional and may belong to ...$rest
            emitter.instruction(&format!("b {}", string_key_label));            // string keys must be filtered by regular parameter names
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "rdi", SOURCE_HASH_OFF);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", CURSOR_OFF);
            abi::emit_call_label(emitter, "__rt_hash_iter_next");
            emitter.instruction("cmp rax, -1");                                 // has the associative argument scan reached the terminal cursor?
            emitter.instruction(&format!("je {}", done_label));                 // finish the variadic hash once every source entry was visited
            abi::emit_store_to_address(emitter, "rax", "rsp", CURSOR_OFF);
            abi::emit_store_to_address(emitter, "rdi", "rsp", KEY_PTR_OFF);
            abi::emit_store_to_address(emitter, "rdx", "rsp", KEY_LEN_OFF);
            abi::emit_store_to_address(emitter, "rcx", "rsp", VALUE_LO_OFF);
            abi::emit_store_to_address(emitter, "r8", "rsp", VALUE_HI_OFF);
            abi::emit_store_to_address(emitter, "r9", "rsp", VALUE_TAG_OFF);
            emitter.instruction("cmp rdx, -1");                                 // is the current callback-argument key numeric?
            emitter.instruction(&format!("je {}", numeric_key_label));          // numeric keys are positional and may belong to ...$rest
            emitter.instruction(&format!("jmp {}", string_key_label));          // string keys must be filtered by regular parameter names
        }
    }

    emitter.label(&numeric_key_label);
    emit_skip_if_consumed_numeric_key(
        regular_param_count,
        &skip_label,
        emitter,
    );
    emit_use_next_variadic_numeric_key(
        KEY_PTR_OFF,
        KEY_LEN_OFF,
        NUMERIC_KEY_OFF,
        emitter,
    );
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("b {}", insert_label));                // insert the numeric-keyed extra argument into ...$rest
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("jmp {}", insert_label));              // insert the numeric-keyed extra argument into ...$rest
        }
    }

    emitter.label(&string_key_label);
    for (param_name, _) in sig.params.iter().take(regular_param_count) {
        emit_skip_if_key_matches_param(param_name, &skip_label, emitter, data);
    }

    emitter.label(&insert_label);
    emit_prepare_and_insert_assoc_variadic_entry(
        SCRATCH_BYTES,
        KEY_PTR_OFF,
        KEY_LEN_OFF,
        VALUE_LO_OFF,
        VALUE_HI_OFF,
        VALUE_TAG_OFF,
        &value_string_label,
        &value_ref_label,
        &value_scalar_label,
        &insert_call_label,
        &loop_label,
        emitter,
    );

    emitter.label(&skip_label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("b {}", loop_label));                  // continue scanning callback-argument entries after skipping a consumed key
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("jmp {}", loop_label));                // continue scanning callback-argument entries after skipping a consumed key
        }
    }

    emitter.label(&done_label);
    abi::emit_release_temporary_stack(emitter, SCRATCH_BYTES);
}

/// Emits assembly for skip if consumed numeric key.
fn emit_skip_if_consumed_numeric_key(
    regular_param_count: usize,
    skip_label: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x8", 16);
            abi::emit_load_int_immediate(emitter, "x9", regular_param_count as i64);
            emitter.instruction("cmp x8, x9");                                  // has this numeric key already filled a regular callback parameter?
            emitter.instruction(&format!("b.lt {}", skip_label));               // skip numeric keys consumed by the fixed callback prefix
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r10", 16);
            abi::emit_load_int_immediate(emitter, "r11", regular_param_count as i64);
            emitter.instruction("cmp r10, r11");                                // has this numeric key already filled a regular callback parameter?
            emitter.instruction(&format!("jl {}", skip_label));                 // skip numeric keys consumed by the fixed callback prefix
        }
    }
}

/// Emits assembly for use next variadic numeric key.
fn emit_use_next_variadic_numeric_key(
    key_ptr_off: usize,
    key_len_off: usize,
    numeric_key_off: usize,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x8", numeric_key_off);
            abi::emit_store_to_address(emitter, "x8", "sp", key_ptr_off);
            abi::emit_load_int_immediate(emitter, "x9", -1);
            abi::emit_store_to_address(emitter, "x9", "sp", key_len_off);
            emitter.instruction("add x8, x8, #1");                              // advance the next numeric variadic key after accepting this positional extra
            abi::emit_store_to_address(emitter, "x8", "sp", numeric_key_off);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r10", numeric_key_off);
            abi::emit_store_to_address(emitter, "r10", "rsp", key_ptr_off);
            abi::emit_load_int_immediate(emitter, "r11", -1);
            abi::emit_store_to_address(emitter, "r11", "rsp", key_len_off);
            emitter.instruction("add r10, 1");                                  // advance the next numeric variadic key after accepting this positional extra
            abi::emit_store_to_address(emitter, "r10", "rsp", numeric_key_off);
        }
    }
}

/// Emits assembly for skip if key matches param.
fn emit_skip_if_key_matches_param(
    param_name: &str,
    skip_label: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let (key_label, key_len) = data.add_string(param_name.as_bytes());
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x1", 16);
            abi::emit_load_temporary_stack_slot(emitter, "x2", 24);
            abi::emit_symbol_address(emitter, "x3", &key_label);
            abi::emit_load_int_immediate(emitter, "x4", key_len as i64);
            abi::emit_call_label(emitter, "__rt_hash_key_eq");
            emitter.instruction("cmp x0, #0");                                  // did this source key already bind a fixed callback parameter?
            emitter.instruction(&format!("b.ne {}", skip_label));               // do not copy consumed named parameters into ...$rest
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "rdi", 16);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", 24);
            abi::emit_symbol_address(emitter, "rdx", &key_label);
            abi::emit_load_int_immediate(emitter, "rcx", key_len as i64);
            abi::emit_call_label(emitter, "__rt_hash_key_eq");
            emitter.instruction("test rax, rax");                               // did this source key already bind a fixed callback parameter?
            emitter.instruction(&format!("jne {}", skip_label));                // do not copy consumed named parameters into ...$rest
        }
    }
}

/// Emits assembly for prepare and insert assoc variadic entry.
#[allow(clippy::too_many_arguments)]
fn emit_prepare_and_insert_assoc_variadic_entry(
    hash_slot_off: usize,
    key_ptr_off: usize,
    key_len_off: usize,
    value_lo_off: usize,
    value_hi_off: usize,
    value_tag_off: usize,
    value_string_label: &str,
    value_ref_label: &str,
    value_scalar_label: &str,
    insert_call_label: &str,
    loop_label: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x5", value_tag_off);
            emitter.instruction("cmp x5, #1");                                  // does the variadic hash value contain a string payload?
            emitter.instruction(&format!("b.eq {}", value_string_label));       // string payloads must be duplicated for the rest hash owner
            emitter.instruction("cmp x5, #4");                                  // is the value in the heap-backed runtime tag range?
            emitter.instruction(&format!("b.lo {}", value_scalar_label));       // scalar values can be copied directly into the rest hash
            emitter.instruction("cmp x5, #7");                                  // is the heap-backed tag one of the supported refcounted payloads?
            emitter.instruction(&format!("b.hi {}", value_scalar_label));       // unknown high tags fall back to scalar copying
            emitter.instruction(&format!("b {}", value_ref_label));             // retain refcounted payloads before insertion
            emitter.label(value_string_label);
            abi::emit_load_temporary_stack_slot(emitter, "x1", value_lo_off);
            abi::emit_load_temporary_stack_slot(emitter, "x2", value_hi_off);
            abi::emit_call_label(emitter, "__rt_str_persist");
            emitter.instruction("mov x3, x1");                                  // pass the owned string pointer as the hash value low word
            emitter.instruction("mov x4, x2");                                  // pass the owned string length as the hash value high word
            abi::emit_load_temporary_stack_slot(emitter, "x5", value_tag_off);
            emitter.instruction(&format!("b {}", insert_call_label));           // insert the persisted string without reloading the borrowed payload
            emitter.label(value_ref_label);
            abi::emit_load_temporary_stack_slot(emitter, "x0", value_lo_off);
            abi::emit_call_label(emitter, "__rt_incref");
            emitter.label(value_scalar_label);
            abi::emit_load_temporary_stack_slot(emitter, "x3", value_lo_off);
            abi::emit_load_temporary_stack_slot(emitter, "x4", value_hi_off);
            abi::emit_load_temporary_stack_slot(emitter, "x5", value_tag_off);
            emitter.label(insert_call_label);
            abi::emit_load_temporary_stack_slot(emitter, "x0", hash_slot_off);
            abi::emit_load_temporary_stack_slot(emitter, "x1", key_ptr_off);
            abi::emit_load_temporary_stack_slot(emitter, "x2", key_len_off);
            abi::emit_call_label(emitter, "__rt_hash_set");
            abi::emit_store_to_address(emitter, "x0", "sp", hash_slot_off);
            emitter.instruction(&format!("b {}", loop_label));                  // continue scanning source entries after inserting a variadic value
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r9", value_tag_off);
            emitter.instruction("cmp r9, 1");                                   // does the variadic hash value contain a string payload?
            emitter.instruction(&format!("je {}", value_string_label));         // string payloads must be duplicated for the rest hash owner
            emitter.instruction("cmp r9, 4");                                   // is the value in the heap-backed runtime tag range?
            emitter.instruction(&format!("jb {}", value_scalar_label));         // scalar values can be copied directly into the rest hash
            emitter.instruction("cmp r9, 7");                                   // is the heap-backed tag one of the supported refcounted payloads?
            emitter.instruction(&format!("ja {}", value_scalar_label));         // unknown high tags fall back to scalar copying
            emitter.instruction(&format!("jmp {}", value_ref_label));           // retain refcounted payloads before insertion
            emitter.label(value_string_label);
            abi::emit_load_temporary_stack_slot(emitter, "rax", value_lo_off);
            abi::emit_load_temporary_stack_slot(emitter, "rdx", value_hi_off);
            abi::emit_call_label(emitter, "__rt_str_persist");
            emitter.instruction("mov rcx, rax");                                // pass the owned string pointer as the hash value low word
            emitter.instruction("mov r8, rdx");                                 // pass the owned string length as the hash value high word
            abi::emit_load_temporary_stack_slot(emitter, "r9", value_tag_off);
            emitter.instruction(&format!("jmp {}", insert_call_label));         // insert the persisted string without reloading the borrowed payload
            emitter.label(value_ref_label);
            abi::emit_load_temporary_stack_slot(emitter, "rax", value_lo_off);
            abi::emit_call_label(emitter, "__rt_incref");
            emitter.label(value_scalar_label);
            abi::emit_load_temporary_stack_slot(emitter, "rcx", value_lo_off);
            abi::emit_load_temporary_stack_slot(emitter, "r8", value_hi_off);
            abi::emit_load_temporary_stack_slot(emitter, "r9", value_tag_off);
            emitter.label(insert_call_label);
            abi::emit_load_temporary_stack_slot(emitter, "rdi", hash_slot_off);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", key_ptr_off);
            abi::emit_load_temporary_stack_slot(emitter, "rdx", key_len_off);
            abi::emit_call_label(emitter, "__rt_hash_set");
            abi::emit_store_to_address(emitter, "rax", "rsp", hash_slot_off);
            emitter.instruction(&format!("jmp {}", loop_label));                // continue scanning source entries after inserting a variadic value
        }
    }
}

/// Emits assembly for loaded array unknown callback call.
pub(crate) fn emit_loaded_array_unknown_callback_call(
    array_source: LoadedArraySource,
    arr_ty: &PhpType,
    call_reg: &str,
    captures: &[(String, PhpType, bool)],
    concat_saved_before_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if matches!(arr_ty, PhpType::AssocArray { .. }) {
        return emit_loaded_assoc_array_unknown_callback_call(
            array_source,
            arr_ty,
            call_reg,
            captures,
            concat_saved_before_args,
            emitter,
            ctx,
            data,
        );
    }

    let elem_ty = callback_array_elem_ty(arr_ty);
    let cases = callable_dispatch::runtime_callable_cases(ctx, captures, Some(&elem_ty));
    if !cases.is_empty() {
        return emit_loaded_indexed_array_unknown_callback_call(
            array_source,
            arr_ty,
            call_reg,
            captures,
            &cases,
            concat_saved_before_args,
            emitter,
            ctx,
            data,
        );
    }

    emit_loaded_array_unknown_callback_call_by_arity(
        array_source,
        arr_ty,
        call_reg,
        captures,
        concat_saved_before_args,
        emitter,
        ctx,
        data,
    )
}

/// Emits assembly for loaded array string callback call.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_loaded_array_string_callback_call(
    array_source: LoadedArraySource,
    arr_ty: &PhpType,
    string_ptr_offset: usize,
    string_len_offset: usize,
    call_reg: &str,
    concat_saved_before_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let elem_ty = callback_array_elem_ty(arr_ty);
    let cases = callable_dispatch::runtime_callable_cases(ctx, &[], Some(&elem_ty));
    let done_label = ctx.next_label("cufa_string_done");
    let pushed_array = matches!(array_source, LoadedArraySource::Result);
    let (array_source, string_ptr_offset, string_len_offset) = match array_source {
        LoadedArraySource::Result => {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // preserve the callback-argument array for runtime string-name dispatch
            (
                LoadedArraySource::TemporaryStackSlot(0),
                string_ptr_offset + 16,
                string_len_offset + 16,
            )
        }
        LoadedArraySource::TemporaryStackSlot(offset) => (
            LoadedArraySource::TemporaryStackSlot(offset),
            string_ptr_offset,
            string_len_offset,
        ),
    };
    let selector = RuntimeCallableSelector::StringNameStack {
        ptr_offset: string_ptr_offset,
        len_offset: string_len_offset,
        call_reg,
    };

    for case in &cases {
        let next_case = ctx.next_label("cufa_string_next");
        callable_dispatch::emit_branch_if_callable_case_mismatch(
            &selector,
            case,
            &next_case,
            emitter,
            ctx,
            data,
        );
        let case_ret_ty = if matches!(arr_ty, PhpType::AssocArray { .. }) {
            emit_loaded_assoc_array_callback_call(
                array_source,
                arr_ty,
                call_reg,
                &case.captures,
                &case.sig,
                concat_saved_before_args,
                emitter,
                ctx,
                data,
            )
        } else {
            emit_loaded_array_callback_call(
                array_source,
                arr_ty,
                None,
                call_reg,
                &case.captures,
                &case.sig,
                concat_saved_before_args,
                emitter,
                ctx,
                data,
            )
        };
        crate::codegen::emit_box_current_value_as_mixed(emitter, &case_ret_ty.codegen_repr());
        abi::emit_jump(emitter, &done_label);
        emitter.label(&next_case);
    }

    emit_dynamic_string_callback_abort(emitter, data);
    emitter.label(&done_label);
    if pushed_array {
        abi::emit_release_temporary_stack(emitter, 16);                         // discard the preserved callback-argument array
    }
    PhpType::Mixed
}

/// Emits assembly for loaded indexed array unknown callback call.
#[allow(clippy::too_many_arguments)]
fn emit_loaded_indexed_array_unknown_callback_call(
    array_source: LoadedArraySource,
    arr_ty: &PhpType,
    call_reg: &str,
    captures: &[(String, PhpType, bool)],
    cases: &[RuntimeCallableCase],
    concat_saved_before_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let done_label = ctx.next_label("cufa_unknown_indexed_done");
    let pushed_array = matches!(array_source, LoadedArraySource::Result);
    let array_source = match array_source {
        LoadedArraySource::Result => {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // preserve the indexed callback-argument array for runtime signature dispatch
            LoadedArraySource::TemporaryStackSlot(0)
        }
        LoadedArraySource::TemporaryStackSlot(offset) => LoadedArraySource::TemporaryStackSlot(offset),
    };

    let selector = RuntimeCallableSelector::Address(call_reg);
    for case in cases {
        let next_case = ctx.next_label("cufa_unknown_indexed_next");
        callable_dispatch::emit_branch_if_callable_case_mismatch(
            &selector,
            case,
            &next_case,
            emitter,
            ctx,
            data,
        );
        let case_ret_ty = emit_loaded_array_callback_call(
            array_source,
            arr_ty,
            None,
            call_reg,
            &case.captures,
            &case.sig,
            concat_saved_before_args,
            emitter,
            ctx,
            data,
        );
        crate::codegen::emit_box_current_value_as_mixed(emitter, &case_ret_ty.codegen_repr());
        abi::emit_jump(emitter, &done_label);
        emitter.label(&next_case);
    }

    let fallback_ret_ty = emit_loaded_array_unknown_callback_call_by_arity(
        array_source,
        arr_ty,
        call_reg,
        captures,
        concat_saved_before_args,
        emitter,
        ctx,
        data,
    );
    crate::codegen::emit_box_current_value_as_mixed(emitter, &fallback_ret_ty.codegen_repr());

    emitter.label(&done_label);
    if pushed_array {
        abi::emit_release_temporary_stack(emitter, 16);                         // discard the preserved indexed callback-argument array
    }
    PhpType::Mixed
}

/// Emits assembly for loaded array unknown callback call by arity.
#[allow(clippy::too_many_arguments)]
fn emit_loaded_array_unknown_callback_call_by_arity(
    array_source: LoadedArraySource,
    arr_ty: &PhpType,
    call_reg: &str,
    captures: &[(String, PhpType, bool)],
    concat_saved_before_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if captures.is_empty() {
        return emit_loaded_array_unknown_callback_call_dynamic(
            array_source,
            arr_ty,
            call_reg,
            concat_saved_before_args,
            emitter,
            ctx,
        );
    }

    let (array_reg, len_reg) = match emitter.target.arch {
        Arch::AArch64 => ("x20", "x21"),
        Arch::X86_64 => ("r13", "r14"),
    };
    let elem_ty = match arr_ty {
        PhpType::Array(elem_ty) => *elem_ty.clone(),
        _ => PhpType::Int,
    };
    let elem_size = args::array_element_stride(&elem_ty);

    match array_source {
        LoadedArraySource::Result => {
            emitter.instruction(&format!("mov {}, {}", array_reg, abi::int_result_reg(emitter))); // preserve the callback-argument array pointer for unknown signature dispatch
        }
        LoadedArraySource::TemporaryStackSlot(offset) => {
            abi::emit_load_temporary_stack_slot(emitter, array_reg, offset);
        }
    }
    abi::emit_load_from_address(emitter, len_reg, array_reg, 0);                // load callback-argument array length for unknown signature dispatch

    let done_label = ctx.next_label("cufa_unknown_done");
    let register_arg_capacity = unknown_callback_register_arg_capacity(emitter.target, &elem_ty);
    let case_labels: Vec<String> = (0..=register_arg_capacity)
        .map(|_| ctx.next_label("cufa_unknown_arity"))
        .collect();
    for (arg_count, label) in case_labels.iter().enumerate() {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cmp {}, #{}", len_reg, arg_count)); // compare runtime callback-argument count against this unknown-signature case
                emitter.instruction(&format!("b.eq {}", label));                // dispatch to the call shape matching the runtime argument count
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("cmp {}, {}", len_reg, arg_count)); // compare runtime callback-argument count against this unknown-signature case
                emitter.instruction(&format!("je {}", label));                  // dispatch to the call shape matching the runtime argument count
            }
        }
    }
    emit_unknown_captured_callback_overflow_dynamic(
        array_reg,
        len_reg,
        &elem_ty,
        register_arg_capacity,
        call_reg,
        captures,
        concat_saved_before_args,
        &done_label,
        emitter,
        ctx,
    );

    for (arg_count, label) in case_labels.iter().enumerate() {
        emitter.label(label);
        emit_unknown_callback_case(
            arg_count,
            &elem_ty,
            elem_size,
            array_reg,
            call_reg,
            captures,
            concat_saved_before_args,
            &done_label,
            emitter,
            ctx,
            data,
        );
    }

    emitter.label(&done_label);
    PhpType::Int
}

/// Emits assembly for loaded assoc array unknown callback call.
fn emit_loaded_assoc_array_unknown_callback_call(
    array_source: LoadedArraySource,
    arr_ty: &PhpType,
    call_reg: &str,
    captures: &[(String, PhpType, bool)],
    concat_saved_before_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let done_label = ctx.next_label("cufa_unknown_assoc_done");
    let elem_ty = callback_array_elem_ty(arr_ty);
    let cases = callable_dispatch::runtime_callable_cases(ctx, captures, Some(&elem_ty));
    let pushed_array = matches!(array_source, LoadedArraySource::Result);
    let array_source = match array_source {
        LoadedArraySource::Result => {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // preserve the associative callback-argument hash for runtime signature dispatch
            LoadedArraySource::TemporaryStackSlot(0)
        }
        LoadedArraySource::TemporaryStackSlot(offset) => LoadedArraySource::TemporaryStackSlot(offset),
    };

    let selector = RuntimeCallableSelector::Address(call_reg);
    for case in &cases {
        let next_case = ctx.next_label("cufa_unknown_assoc_next");
        callable_dispatch::emit_branch_if_callable_case_mismatch(
            &selector,
            case,
            &next_case,
            emitter,
            ctx,
            data,
        );
        let case_ret_ty = emit_loaded_assoc_array_callback_call(
            array_source,
            arr_ty,
            call_reg,
            &case.captures,
            &case.sig,
            concat_saved_before_args,
            emitter,
            ctx,
            data,
        );
        crate::codegen::emit_box_current_value_as_mixed(emitter, &case_ret_ty.codegen_repr());
        abi::emit_jump(emitter, &done_label);
        emitter.label(&next_case);
    }

    emit_call_user_func_array_unknown_assoc_abort(emitter, data);
    emitter.label(&done_label);
    if pushed_array {
        abi::emit_release_temporary_stack(emitter, 16);                         // discard the preserved associative callback-argument hash
    }
    PhpType::Mixed
}

/// Provides the Callback array elem ty helper used by the call user func array module.
fn callback_array_elem_ty(arr_ty: &PhpType) -> PhpType {
    match arr_ty {
        PhpType::Array(elem_ty) => *elem_ty.clone(),
        PhpType::AssocArray { value, .. } => *value.clone(),
        _ => PhpType::Int,
    }
}

/// Provides the Unknown callback register arg capacity helper used by the call user func array module.
fn unknown_callback_register_arg_capacity(target: crate::codegen::platform::Target, elem_ty: &PhpType) -> usize {
    match elem_ty.codegen_repr() {
        PhpType::Float => 8,
        PhpType::Str => match target.arch {
            Arch::AArch64 => 4,
            Arch::X86_64 => 3,
        },
        PhpType::Void | PhpType::Never => 0,
        _ => match target.arch {
            Arch::AArch64 => 8,
            Arch::X86_64 => 6,
        },
    }
}

/// Emits assembly for unknown captured callback overflow dynamic.
#[allow(clippy::too_many_arguments)]
fn emit_unknown_captured_callback_overflow_dynamic(
    array_reg: &str,
    len_reg: &str,
    elem_ty: &PhpType,
    register_arg_capacity: usize,
    call_reg: &str,
    captures: &[(String, PhpType, bool)],
    concat_saved_before_args: bool,
    done_label: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let (overflow_count_reg, overflow_bytes_reg) = match emitter.target.arch {
        Arch::AArch64 => ("x22", "x23"),
        Arch::X86_64 => ("r15", "rbx"),
    };
    let capture_assignments = unknown_dynamic_capture_assignments(
        emitter.target,
        elem_ty,
        register_arg_capacity,
        captures,
    );
    let capture_stack_bytes = capture_assignments
        .iter()
        .filter(|(_, assignment)| !assignment.in_register())
        .count()
        * 16;
    let visible_register_temp_bytes = register_arg_capacity * 16;
    let capture_register_temp_bytes = capture_assignments
        .iter()
        .filter(|(_, assignment)| assignment.in_register())
        .count()
        * 16;
    let register_temp_bytes = visible_register_temp_bytes + capture_register_temp_bytes;

    emit_unknown_dynamic_overflow_size(
        len_reg,
        overflow_count_reg,
        overflow_bytes_reg,
        register_arg_capacity,
        emitter,
        ctx,
    );
    if capture_stack_bytes > 0 {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("add {}, {}, #{}", overflow_bytes_reg, overflow_bytes_reg, capture_stack_bytes)); // reserve trailing stack slots for captured callback arguments
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("add {}, {}", overflow_bytes_reg, capture_stack_bytes)); // reserve trailing stack slots for captured callback arguments
            }
        }
    }
    emit_dynamic_stack_adjust(emitter, overflow_bytes_reg, true);
    abi::emit_reserve_temporary_stack(emitter, register_temp_bytes);

    emit_unknown_dynamic_register_arg_temps(
        array_reg,
        len_reg,
        elem_ty,
        register_arg_capacity,
        emitter,
        ctx,
    );
    emit_unknown_dynamic_stack_args(
        array_reg,
        overflow_count_reg,
        elem_ty,
        register_arg_capacity,
        register_temp_bytes,
        emitter,
        ctx,
    );
    let capture_register_temps = emit_unknown_dynamic_capture_args(
        captures,
        &capture_assignments,
        overflow_count_reg,
        register_temp_bytes,
        visible_register_temp_bytes,
        emitter,
        ctx,
    );
    emit_unknown_dynamic_load_register_args(
        len_reg,
        elem_ty,
        register_arg_capacity,
        emitter,
        ctx,
    );
    emit_unknown_dynamic_load_capture_register_args(&capture_register_temps, emitter);

    abi::emit_release_temporary_stack(emitter, register_temp_bytes);
    let ret_ty = PhpType::Int;
    if !concat_saved_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }
    abi::emit_call_reg(emitter, call_reg);
    if concat_saved_before_args {
        emit_dynamic_stack_adjust(emitter, overflow_bytes_reg, false);
        crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
    } else {
        crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
        emit_dynamic_stack_adjust(emitter, overflow_bytes_reg, false);
    }
    abi::emit_jump(emitter, done_label);
}

/// Provides the Unknown dynamic capture assignments helper used by the call user func array module.
fn unknown_dynamic_capture_assignments(
    target: crate::codegen::platform::Target,
    elem_ty: &PhpType,
    register_arg_capacity: usize,
    captures: &[(String, PhpType, bool)],
) -> Vec<(PhpType, abi::OutgoingArgAssignment)> {
    let mut arg_types = vec![elem_ty.codegen_repr(); register_arg_capacity];
    let capture_types: Vec<PhpType> = captures
        .iter()
        .map(|(_, ty, by_ref)| if *by_ref { PhpType::Int } else { ty.codegen_repr() })
        .collect();
    arg_types.extend(capture_types.iter().cloned());
    abi::build_outgoing_arg_assignments_for_target(target, &arg_types, 0)
        .into_iter()
        .skip(register_arg_capacity)
        .zip(capture_types)
        .map(|(assignment, ty)| (ty, assignment))
        .collect()
}

/// Emits assembly for loaded array unknown callback call dynamic.
fn emit_loaded_array_unknown_callback_call_dynamic(
    array_source: LoadedArraySource,
    arr_ty: &PhpType,
    call_reg: &str,
    concat_saved_before_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    let (array_reg, len_reg, overflow_count_reg, overflow_bytes_reg) = match emitter.target.arch {
        Arch::AArch64 => ("x20", "x21", "x22", "x23"),
        Arch::X86_64 => ("r13", "r14", "r15", "rbx"),
    };
    let elem_ty = match arr_ty {
        PhpType::Array(elem_ty) => *elem_ty.clone(),
        _ => PhpType::Int,
    };
    let register_arg_capacity = unknown_callback_register_arg_capacity(emitter.target, &elem_ty);
    let register_temp_bytes = register_arg_capacity * 16;

    match array_source {
        LoadedArraySource::Result => {
            emitter.instruction(&format!("mov {}, {}", array_reg, abi::int_result_reg(emitter))); // preserve the callback-argument array pointer for dynamic unknown-signature dispatch
        }
        LoadedArraySource::TemporaryStackSlot(offset) => {
            abi::emit_load_temporary_stack_slot(emitter, array_reg, offset);
        }
    }
    abi::emit_load_from_address(emitter, len_reg, array_reg, 0);                // load the dynamic callback-argument count
    emit_unknown_dynamic_overflow_size(
        len_reg,
        overflow_count_reg,
        overflow_bytes_reg,
        register_arg_capacity,
        emitter,
        ctx,
    );
    emit_dynamic_stack_adjust(emitter, overflow_bytes_reg, true);
    abi::emit_reserve_temporary_stack(emitter, register_temp_bytes);

    emit_unknown_dynamic_register_arg_temps(
        array_reg,
        len_reg,
        &elem_ty,
        register_arg_capacity,
        emitter,
        ctx,
    );
    emit_unknown_dynamic_stack_args(
        array_reg,
        overflow_count_reg,
        &elem_ty,
        register_arg_capacity,
        register_temp_bytes,
        emitter,
        ctx,
    );
    emit_unknown_dynamic_load_register_args(
        len_reg,
        &elem_ty,
        register_arg_capacity,
        emitter,
        ctx,
    );

    abi::emit_release_temporary_stack(emitter, register_temp_bytes);
    let ret_ty = PhpType::Int;
    if !concat_saved_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }
    abi::emit_call_reg(emitter, call_reg);
    if concat_saved_before_args {
        emit_dynamic_stack_adjust(emitter, overflow_bytes_reg, false);
        crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
    } else {
        crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
        emit_dynamic_stack_adjust(emitter, overflow_bytes_reg, false);
    }

    ret_ty
}

/// Emits assembly for unknown dynamic overflow size.
fn emit_unknown_dynamic_overflow_size(
    len_reg: &str,
    overflow_count_reg: &str,
    overflow_bytes_reg: &str,
    register_arg_capacity: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let has_overflow = ctx.next_label("cufa_unknown_dynamic_overflow");
    let done = ctx.next_label("cufa_unknown_dynamic_overflow_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov {}, #0", overflow_count_reg));    // default to no stack-passed unknown callback arguments
            emitter.instruction(&format!("cmp {}, #{}", len_reg, register_arg_capacity)); // compare runtime arity with the register argument capacity
            emitter.instruction(&format!("b.gt {}", has_overflow));             // compute stack spill bytes only when runtime arity exceeds registers
            emitter.instruction(&format!("b {}", done));                        // skip overflow sizing for register-only calls
            emitter.label(&has_overflow);
            emitter.instruction(&format!("sub {}, {}, #{}", overflow_count_reg, len_reg, register_arg_capacity)); // count callback arguments that must be stack-passed
            emitter.label(&done);
            emitter.instruction(&format!("lsl {}, {}, #4", overflow_bytes_reg, overflow_count_reg)); // convert stack-passed argument count to 16-byte ABI slots
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, 0", overflow_count_reg));     // default to no stack-passed unknown callback arguments
            emitter.instruction(&format!("cmp {}, {}", len_reg, register_arg_capacity)); // compare runtime arity with the register argument capacity
            emitter.instruction(&format!("jg {}", has_overflow));               // compute stack spill bytes only when runtime arity exceeds registers
            emitter.instruction(&format!("jmp {}", done));                      // skip overflow sizing for register-only calls
            emitter.label(&has_overflow);
            emitter.instruction(&format!("mov {}, {}", overflow_count_reg, len_reg)); // seed overflow count from the runtime callback arity
            emitter.instruction(&format!("sub {}, {}", overflow_count_reg, register_arg_capacity)); // count callback arguments that must be stack-passed
            emitter.label(&done);
            emitter.instruction(&format!("mov {}, {}", overflow_bytes_reg, overflow_count_reg)); // copy overflow count before scaling to bytes
            emitter.instruction(&format!("shl {}, 4", overflow_bytes_reg));     // convert stack-passed argument count to 16-byte ABI slots
        }
    }
}

/// Emits assembly for dynamic stack adjust.
fn emit_dynamic_stack_adjust(emitter: &mut Emitter, bytes_reg: &str, subtract: bool) {
    match (emitter.target.arch, subtract) {
        (Arch::AArch64, true) => {
            emitter.instruction(&format!("sub sp, sp, {}", bytes_reg));         // reserve dynamic stack space for unknown callback overflow arguments
        }
        (Arch::AArch64, false) => {
            emitter.instruction(&format!("add sp, sp, {}", bytes_reg));         // release dynamic stack space for unknown callback overflow arguments
        }
        (Arch::X86_64, true) => {
            emitter.instruction(&format!("sub rsp, {}", bytes_reg));            // reserve dynamic stack space for unknown callback overflow arguments
        }
        (Arch::X86_64, false) => {
            emitter.instruction(&format!("add rsp, {}", bytes_reg));            // release dynamic stack space for unknown callback overflow arguments
        }
    }
}

/// Emits assembly for unknown dynamic register arg temps.
fn emit_unknown_dynamic_register_arg_temps(
    array_reg: &str,
    len_reg: &str,
    elem_ty: &PhpType,
    register_arg_capacity: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    for arg_idx in 0..register_arg_capacity {
        let load_label = ctx.next_label("cufa_unknown_dynamic_reg_load");
        let done_label = ctx.next_label("cufa_unknown_dynamic_reg_done");
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cmp {}, #{}", len_reg, arg_idx + 1)); // check whether this register-passed callback argument exists
                emitter.instruction(&format!("b.ge {}", load_label));           // materialize the register argument when present
                emitter.instruction(&format!("b {}", done_label));              // leave absent optional unknown callback argument registers untouched
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("cmp {}, {}", len_reg, arg_idx + 1)); // check whether this register-passed callback argument exists
                emitter.instruction(&format!("jge {}", load_label));            // materialize the register argument when present
                emitter.instruction(&format!("jmp {}", done_label));            // leave absent optional unknown callback argument registers untouched
            }
        }
        emitter.label(&load_label);
        args::load_array_element_to_result(
            emitter,
            elem_ty,
            array_reg,
            24 + arg_idx * args::array_element_stride(elem_ty),
        );
        abi::emit_incref_if_refcounted(emitter, &elem_ty.codegen_repr());       // retain borrowed heap arguments before passing them to the unknown callback
        emit_store_current_result_to_sp_offset(emitter, elem_ty, arg_idx * 16);
        emitter.label(&done_label);
    }
}

/// Emits assembly for unknown dynamic stack args.
fn emit_unknown_dynamic_stack_args(
    array_reg: &str,
    overflow_count_reg: &str,
    elem_ty: &PhpType,
    register_arg_capacity: usize,
    register_temp_bytes: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let loop_label = ctx.next_label("cufa_unknown_dynamic_stack_loop");
    let done_label = ctx.next_label("cufa_unknown_dynamic_stack_done");
    let (idx_reg, source_idx_reg, source_reg, dest_reg, offset_reg) = match emitter.target.arch {
        Arch::AArch64 => ("x24", "x25", "x26", "x27", "x28"),
        Arch::X86_64 => ("rcx", "r10", "r11", "rsi", "rdx"),
    };
    abi::emit_load_int_immediate(emitter, idx_reg, 0);
    emitter.label(&loop_label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, {}", idx_reg, overflow_count_reg)); // stop after all stack-passed unknown callback args are materialized
            emitter.instruction(&format!("b.ge {}", done_label));               // leave the overflow materialization loop
            emitter.instruction(&format!("add {}, {}, #{}", source_idx_reg, idx_reg, register_arg_capacity)); // convert overflow index to source array index
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", idx_reg, overflow_count_reg)); // stop after all stack-passed unknown callback args are materialized
            emitter.instruction(&format!("jge {}", done_label));                // leave the overflow materialization loop
            emitter.instruction(&format!("mov {}, {}", source_idx_reg, idx_reg)); // seed source array index from overflow index
            emitter.instruction(&format!("add {}, {}", source_idx_reg, register_arg_capacity)); // convert overflow index to source array index
        }
    }
    emit_dynamic_array_element_to_result(
        array_reg,
        source_idx_reg,
        source_reg,
        offset_reg,
        elem_ty,
        emitter,
    );
    abi::emit_incref_if_refcounted(emitter, &elem_ty.codegen_repr());           // retain borrowed heap overflow arguments before passing them to the unknown callback
    emit_unknown_dynamic_stack_arg_address(
        idx_reg,
        dest_reg,
        offset_reg,
        register_temp_bytes,
        emitter,
    );
    emit_store_current_result_to_address(emitter, elem_ty, dest_reg);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("add {}, {}, #1", idx_reg, idx_reg));  // advance to the next stack-passed unknown callback argument
            emitter.instruction(&format!("b {}", loop_label));                  // continue materializing overflow arguments
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("add {}, 1", idx_reg));                // advance to the next stack-passed unknown callback argument
            emitter.instruction(&format!("jmp {}", loop_label));                // continue materializing overflow arguments
        }
    }
    emitter.label(&done_label);
}

/// Emits assembly for dynamic array element to result.
fn emit_dynamic_array_element_to_result(
    array_reg: &str,
    index_reg: &str,
    source_reg: &str,
    offset_reg: &str,
    elem_ty: &PhpType,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov {}, {}", source_reg, array_reg)); // seed the dynamic source element pointer from the callback-argument array
            emitter.instruction(&format!("add {}, {}, #24", source_reg, source_reg)); // skip the indexed array header before dynamic argument lookup
            if args::array_element_stride(elem_ty) == 16 {
                emitter.instruction(&format!("lsl {}, {}, #4", offset_reg, index_reg)); // scale dynamic source index by the string element width
            } else {
                emitter.instruction(&format!("lsl {}, {}, #3", offset_reg, index_reg)); // scale dynamic source index by the scalar element width
            }
            emitter.instruction(&format!("add {}, {}, {}", source_reg, source_reg, offset_reg)); // address the selected dynamic callback argument slot
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, {}", source_reg, array_reg)); // seed the dynamic source element pointer from the callback-argument array
            emitter.instruction(&format!("add {}, 24", source_reg));            // skip the indexed array header before dynamic argument lookup
            emitter.instruction(&format!("mov {}, {}", offset_reg, index_reg)); // copy dynamic source index before scaling
            emitter.instruction(&format!("imul {}, {}", offset_reg, args::array_element_stride(elem_ty))); // scale dynamic source index by the element width
            emitter.instruction(&format!("add {}, {}", source_reg, offset_reg)); // address the selected dynamic callback argument slot
        }
    }
    args::load_array_element_to_result(emitter, elem_ty, source_reg, 0);
}

/// Emits assembly for unknown dynamic stack arg address.
fn emit_unknown_dynamic_stack_arg_address(
    idx_reg: &str,
    dest_reg: &str,
    offset_reg: &str,
    register_temp_bytes: usize,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov {}, sp", dest_reg));              // seed overflow destination from the current stack pointer
            emitter.instruction(&format!("add {}, {}, #{}", dest_reg, dest_reg, register_temp_bytes)); // skip register-argument temp slots to reach outgoing stack args
            emitter.instruction(&format!("lsl {}, {}, #4", offset_reg, idx_reg)); // scale overflow argument index by the 16-byte ABI slot width
            emitter.instruction(&format!("add {}, {}, {}", dest_reg, dest_reg, offset_reg)); // address the outgoing overflow argument slot
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("lea {}, [rsp + {}]", dest_reg, register_temp_bytes)); // address the first outgoing overflow argument slot
            emitter.instruction(&format!("mov {}, {}", offset_reg, idx_reg));   // copy overflow argument index before scaling to bytes
            emitter.instruction(&format!("shl {}, 4", offset_reg));             // scale overflow argument index by the 16-byte ABI slot width
            emitter.instruction(&format!("add {}, {}", dest_reg, offset_reg));  // address the outgoing overflow argument slot
        }
    }
}

/// Emits assembly for unknown dynamic load register args.
fn emit_unknown_dynamic_load_register_args(
    len_reg: &str,
    elem_ty: &PhpType,
    register_arg_capacity: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    for arg_idx in 0..register_arg_capacity {
        let load_label = ctx.next_label("cufa_unknown_dynamic_arg_load");
        let done_label = ctx.next_label("cufa_unknown_dynamic_arg_done");
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cmp {}, #{}", len_reg, arg_idx + 1)); // check whether this final register argument exists
                emitter.instruction(&format!("b.ge {}", load_label));           // load the ABI register when the argument was provided
                emitter.instruction(&format!("b {}", done_label));              // skip absent unknown callback argument registers
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("cmp {}, {}", len_reg, arg_idx + 1)); // check whether this final register argument exists
                emitter.instruction(&format!("jge {}", load_label));            // load the ABI register when the argument was provided
                emitter.instruction(&format!("jmp {}", done_label));            // skip absent unknown callback argument registers
            }
        }
        emitter.label(&load_label);
        emit_load_sp_offset_to_arg_register(emitter, elem_ty, arg_idx * 16, arg_idx);
        emitter.label(&done_label);
    }
}

/// Emits assembly for unknown dynamic capture args.
fn emit_unknown_dynamic_capture_args(
    captures: &[(String, PhpType, bool)],
    assignments: &[(PhpType, abi::OutgoingArgAssignment)],
    overflow_count_reg: &str,
    register_temp_bytes: usize,
    visible_register_temp_bytes: usize,
    emitter: &mut Emitter,
    ctx: &Context,
) -> Vec<(PhpType, abi::OutgoingArgAssignment, usize)> {
    let mut register_temps = Vec::new();
    let mut register_capture_idx = 0usize;
    let mut stack_capture_idx = 0usize;
    for ((capture_name, capture_ty, by_ref), (arg_ty, assignment)) in
        captures.iter().zip(assignments.iter())
    {
        emit_capture_arg_to_result(capture_name, capture_ty, *by_ref, emitter, ctx);
        if assignment.in_register() {
            let offset = visible_register_temp_bytes + register_capture_idx * 16;
            emit_store_current_result_to_sp_offset(emitter, arg_ty, offset);
            register_temps.push((arg_ty.clone(), assignment.clone(), offset));
            register_capture_idx += 1;
        } else {
            emit_store_current_result_to_dynamic_capture_stack(
                overflow_count_reg,
                register_temp_bytes,
                stack_capture_idx * 16,
                arg_ty,
                emitter,
            );
            stack_capture_idx += 1;
        }
    }
    register_temps
}

/// Emits assembly for capture arg to result.
fn emit_capture_arg_to_result(
    capture_name: &str,
    capture_ty: &PhpType,
    by_ref: bool,
    emitter: &mut Emitter,
    ctx: &Context,
) {
    emitter.comment(&format!("materialize callback capture ${}", capture_name));
    if by_ref {
        if !args::emit_ref_arg_variable_address(capture_name, "callback capture ref", emitter, ctx) {
            emitter.comment(&format!(
                "WARNING: captured callback variable ${} not found",
                capture_name
            ));
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        }
        return;
    }
    let Some(capture_info) = ctx.variables.get(capture_name) else {
        emitter.comment(&format!(
            "WARNING: captured callback variable ${} not found",
            capture_name
        ));
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        return;
    };
    abi::emit_load(emitter, capture_ty, capture_info.stack_offset);
}

/// Emits assembly for store current result to dynamic capture stack.
fn emit_store_current_result_to_dynamic_capture_stack(
    overflow_count_reg: &str,
    register_temp_bytes: usize,
    capture_stack_offset: usize,
    ty: &PhpType,
    emitter: &mut Emitter,
) {
    let (dest_reg, offset_reg) = match emitter.target.arch {
        Arch::AArch64 => ("x10", "x11"),
        Arch::X86_64 => ("r10", "r11"),
    };
    emit_dynamic_capture_stack_arg_address(
        overflow_count_reg,
        dest_reg,
        offset_reg,
        register_temp_bytes,
        capture_stack_offset,
        emitter,
    );
    emit_store_current_result_to_address(emitter, ty, dest_reg);
}

/// Emits assembly for dynamic capture stack arg address.
fn emit_dynamic_capture_stack_arg_address(
    overflow_count_reg: &str,
    dest_reg: &str,
    offset_reg: &str,
    register_temp_bytes: usize,
    capture_stack_offset: usize,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov {}, sp", dest_reg));              // seed the captured-argument stack slot from the current stack pointer
            emitter.instruction(&format!("add {}, {}, #{}", dest_reg, dest_reg, register_temp_bytes)); // skip register-argument temps before captured stack args
            emitter.instruction(&format!("lsl {}, {}, #4", offset_reg, overflow_count_reg)); // scale visible overflow count to outgoing stack bytes
            emitter.instruction(&format!("add {}, {}, {}", dest_reg, dest_reg, offset_reg)); // skip dynamic visible overflow args before captured stack args
            if capture_stack_offset > 0 {
                emitter.instruction(&format!("add {}, {}, #{}", dest_reg, dest_reg, capture_stack_offset)); // select the current captured stack argument slot
            }
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("lea {}, [rsp + {}]", dest_reg, register_temp_bytes)); // skip register-argument temps before captured stack args
            emitter.instruction(&format!("mov {}, {}", offset_reg, overflow_count_reg)); // copy visible overflow count before scaling to bytes
            emitter.instruction(&format!("shl {}, 4", offset_reg));             // scale visible overflow count to outgoing stack bytes
            emitter.instruction(&format!("add {}, {}", dest_reg, offset_reg));  // skip dynamic visible overflow args before captured stack args
            if capture_stack_offset > 0 {
                emitter.instruction(&format!("add {}, {}", dest_reg, capture_stack_offset)); // select the current captured stack argument slot
            }
        }
    }
}

/// Emits assembly for unknown dynamic load capture register args.
fn emit_unknown_dynamic_load_capture_register_args(
    register_temps: &[(PhpType, abi::OutgoingArgAssignment, usize)],
    emitter: &mut Emitter,
) {
    for (ty, assignment, offset) in register_temps {
        emit_load_sp_offset_to_assignment_register(emitter, ty, *offset, assignment);
    }
}

/// Emits assembly for store current result to sp offset.
fn emit_store_current_result_to_sp_offset(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    let stack_reg = match emitter.target.arch {
        Arch::AArch64 => "sp",
        Arch::X86_64 => "rsp",
    };
    emit_store_current_result_to_address_offset(emitter, ty, stack_reg, offset);
}

/// Emits assembly for store current result to address.
fn emit_store_current_result_to_address(emitter: &mut Emitter, ty: &PhpType, address_reg: &str) {
    emit_store_current_result_to_address_offset(emitter, ty, address_reg, 0);
}

/// Emits assembly for store current result to address offset.
fn emit_store_current_result_to_address_offset(
    emitter: &mut Emitter,
    ty: &PhpType,
    address_reg: &str,
    offset: usize,
) {
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), address_reg, offset);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_store_to_address(emitter, ptr_reg, address_reg, offset);
            abi::emit_store_to_address(emitter, len_reg, address_reg, offset + 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), address_reg, offset);
        }
    }
}

/// Emits assembly for load sp offset to arg register.
fn emit_load_sp_offset_to_arg_register(
    emitter: &mut Emitter,
    ty: &PhpType,
    offset: usize,
    arg_idx: usize,
) {
    match ty.codegen_repr() {
        PhpType::Float => {
            let reg = abi::float_arg_reg_name(emitter.target, arg_idx);
            abi::emit_load_temporary_stack_slot(emitter, reg, offset);
        }
        PhpType::Str => {
            let ptr_reg = abi::int_arg_reg_name(emitter.target, arg_idx * 2);
            let len_reg = abi::int_arg_reg_name(emitter.target, arg_idx * 2 + 1);
            abi::emit_load_temporary_stack_slot(emitter, ptr_reg, offset);
            abi::emit_load_temporary_stack_slot(emitter, len_reg, offset + 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            let reg = abi::int_arg_reg_name(emitter.target, arg_idx);
            abi::emit_load_temporary_stack_slot(emitter, reg, offset);
        }
    }
}

/// Emits assembly for load sp offset to assignment register.
fn emit_load_sp_offset_to_assignment_register(
    emitter: &mut Emitter,
    ty: &PhpType,
    offset: usize,
    assignment: &abi::OutgoingArgAssignment,
) {
    match ty.codegen_repr() {
        PhpType::Float => {
            let reg = abi::float_arg_reg_name(emitter.target, assignment.start_reg);
            abi::emit_load_temporary_stack_slot(emitter, reg, offset);
        }
        PhpType::Str => {
            let ptr_reg = abi::int_arg_reg_name(emitter.target, assignment.start_reg);
            let len_reg = abi::int_arg_reg_name(emitter.target, assignment.start_reg + 1);
            abi::emit_load_temporary_stack_slot(emitter, ptr_reg, offset);
            abi::emit_load_temporary_stack_slot(emitter, len_reg, offset + 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            let reg = abi::int_arg_reg_name(emitter.target, assignment.start_reg);
            abi::emit_load_temporary_stack_slot(emitter, reg, offset);
        }
    }
}

/// Emits assembly for unknown callback case.
#[allow(clippy::too_many_arguments)]
fn emit_unknown_callback_case(
    arg_count: usize,
    elem_ty: &PhpType,
    elem_size: usize,
    array_reg: &str,
    call_reg: &str,
    captures: &[(String, PhpType, bool)],
    concat_saved_before_args: bool,
    done_label: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let mut arg_types = Vec::with_capacity(arg_count + captures.len());
    for i in 0..arg_count {
        args::load_array_element_to_result(emitter, elem_ty, array_reg, 24 + i * elem_size);
        let pushed_ty = args::push_loaded_array_element_arg(elem_ty, None, emitter, ctx, data);
        arg_types.push(pushed_ty);
    }
    callback_env::push_captures_as_hidden_args(captures, emitter, ctx, &mut arg_types);

    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, &arg_types, 0);
    let overflow_bytes = abi::materialize_outgoing_args(emitter, &assignments);
    let ret_ty = PhpType::Int;

    if !concat_saved_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }
    abi::emit_call_reg(emitter, call_reg);
    if concat_saved_before_args {
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
        crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
    } else {
        crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
    }
    abi::emit_jump(emitter, done_label);
}

/// Emits assembly for call user func array missing arg abort.
fn emit_call_user_func_array_missing_arg_abort(emitter: &mut Emitter, data: &mut DataSection) {
    let (message_label, message_len) = data.add_string(
        b"Fatal error: call_user_func_array() argument array is missing a required callback parameter\n",
    );
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the callback argument diagnostic to stderr
            emitter.adrp("x1", &message_label);
            emitter.add_lo12("x1", "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the diagnostic byte length to write()
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // write the callback argument diagnostic to stderr
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the diagnostic byte length to write()
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal callback argument diagnostic
            abi::emit_exit(emitter, 1);
        }
    }
}

/// Emits assembly for call user func array unknown assoc abort.
fn emit_call_user_func_array_unknown_assoc_abort(emitter: &mut Emitter, data: &mut DataSection) {
    let (message_label, message_len) = data.add_string(
        b"Fatal error: call_user_func_array() could not resolve named callback arguments for this callable\n",
    );
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the callback metadata diagnostic to stderr
            emitter.adrp("x1", &message_label);
            emitter.add_lo12("x1", "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the callback metadata diagnostic byte length to write()
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // write the callback metadata diagnostic to stderr
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the callback metadata diagnostic byte length to write()
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal callback metadata diagnostic
            abi::emit_exit(emitter, 1);
        }
    }
}

/// Emits assembly for dynamic string callback abort.
fn emit_dynamic_string_callback_abort(emitter: &mut Emitter, data: &mut DataSection) {
    let (message_label, message_len) = data.add_string(
        b"Fatal error: dynamic string callback could not be resolved\n",
    );
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the dynamic callback diagnostic to stderr
            emitter.adrp("x1", &message_label);
            emitter.add_lo12("x1", "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the dynamic callback diagnostic byte length to write()
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // write the dynamic callback diagnostic to stderr
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the dynamic callback diagnostic byte length to write()
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal dynamic callback diagnostic
            abi::emit_exit(emitter, 1);
        }
    }
}

/// Computes the type metadata for widen callback arg.
fn widen_callback_arg_type(a: &PhpType, b: &PhpType) -> PhpType {
    if a == b {
        return a.clone();
    }
    if matches!(a, PhpType::Mixed | PhpType::Union(_))
        || matches!(b, PhpType::Mixed | PhpType::Union(_))
    {
        return PhpType::Mixed;
    }
    if *a == PhpType::Str || *b == PhpType::Str {
        return PhpType::Str;
    }
    if *a == PhpType::Float || *b == PhpType::Float {
        return PhpType::Float;
    }
    if *a == PhpType::Void {
        return b.clone();
    }
    if *b == PhpType::Void {
        return a.clone();
    }
    a.clone()
}
