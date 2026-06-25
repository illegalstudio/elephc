//! Purpose:
//! Emits PHP `call_user_func_array` builtin calls that invoke user-provided callbacks.
//! Owns callback argument materialization, result shape selection, and runtime helper calls.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Callback lowering must preserve PHP source evaluation order, captures, and callable return ownership.

use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{emit_expr, expr_result_heap_ownership};
use crate::codegen::expr::calls::args;
use crate::codegen::platform::Arch;
use crate::codegen::abi;
use crate::codegen::callable_dispatch::{
    self, RuntimeCallableCase, RuntimeCallableSelector,
};
use crate::codegen::callable_descriptor;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, StaticReceiver};
use crate::types::{FunctionSig, PhpType};
use super::callback_env;
use super::callable_forms;
use super::super::callable_lookup::{lookup_function, FunctionLookup};

/// Internal boxed-Mixed tag used only inside descriptor-invoker argument arrays.
pub(crate) const INVOKER_ARG_REF_CELL_TAG: i64 = 11;

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
    ArgumentRegister(usize),
}

#[derive(Clone, Copy)]
pub(crate) enum LoadedDescriptorSource {
    TemporaryStackSlot(usize),
}

/// Loads a previously materialized callback argument array into `dest_reg`.
fn emit_loaded_array_source_to_reg(
    array_source: LoadedArraySource,
    dest_reg: &str,
    emitter: &mut Emitter,
) {
    match array_source {
        LoadedArraySource::Result => {
            emitter.instruction(&format!("mov {}, {}", dest_reg, abi::int_result_reg(emitter))); // preserve the callback-argument array pointer from the result register
        }
        LoadedArraySource::TemporaryStackSlot(offset) => {
            abi::emit_load_temporary_stack_slot(emitter, dest_reg, offset);
        }
        LoadedArraySource::ArgumentRegister(index) => {
            let arg_reg = abi::int_arg_reg_name(emitter.target, index);
            if arg_reg != dest_reg {
                emitter.instruction(&format!("mov {}, {}", dest_reg, arg_reg)); // copy the callback-argument array from the invoker ABI register
            }
        }
    }
}

/// Loads a previously preserved callable descriptor into `dest_reg`.
fn emit_loaded_descriptor_source_to_reg(
    descriptor_source: LoadedDescriptorSource,
    dest_reg: &str,
    emitter: &mut Emitter,
) {
    match descriptor_source {
        LoadedDescriptorSource::TemporaryStackSlot(offset) => {
            abi::emit_load_temporary_stack_slot(emitter, dest_reg, offset);
        }
    }
}

/// Adjusts a descriptor stack source when this frame also preserved the argument array.
fn descriptor_source_after_array_push(
    descriptor_source: Option<LoadedDescriptorSource>,
    pushed_array: bool,
) -> Option<LoadedDescriptorSource> {
    descriptor_source.map(|source| match source {
        LoadedDescriptorSource::TemporaryStackSlot(offset) => {
            LoadedDescriptorSource::TemporaryStackSlot(offset + if pushed_array { 16 } else { 0 })
        }
    })
}

/// Adjusts an argument-array source after preserving a descriptor on the temporary stack.
fn array_source_after_descriptor_push(array_source: LoadedArraySource) -> LoadedArraySource {
    match array_source {
        LoadedArraySource::TemporaryStackSlot(offset) => {
            LoadedArraySource::TemporaryStackSlot(offset + 16)
        }
        LoadedArraySource::Result => LoadedArraySource::Result,
        LoadedArraySource::ArgumentRegister(index) => LoadedArraySource::ArgumentRegister(index),
    }
}

/// Resolves a callback to its entry address and preserves its descriptor when available.
fn materialize_callback_address_and_preserve_descriptor(
    callback: &Expr,
    call_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> (Vec<(String, PhpType, bool)>, Option<LoadedDescriptorSource>) {
    match &callback.kind {
        ExprKind::StringLiteral(name) => {
            let resolved_name = match lookup_function(ctx, name) {
                Some(FunctionLookup::UserFunction(name))
                | Some(FunctionLookup::IncludeVariant(name)) => name,
                _ => name.clone(),
            };
            let label = crate::names::function_symbol(&resolved_name);
            abi::emit_symbol_address(emitter, call_reg, &label);
            (Vec::new(), None)
        }
        ExprKind::Variable(name) => {
            let var = ctx.variables.get(name).expect("undefined callback variable");
            abi::load_at_offset(emitter, call_reg, var.stack_offset);           // load the callback descriptor from the callable variable slot
            if ctx.ref_params.contains(name) {
                abi::emit_load_from_address(emitter, call_reg, call_reg, 0);
            }
            abi::emit_push_reg(emitter, call_reg);                              // preserve the callable descriptor for descriptor-invoker dispatch
            callable_descriptor::emit_load_entry_from_descriptor(emitter, call_reg, call_reg);
            (
                crate::codegen::callables::callable_captures(callback, ctx),
                Some(LoadedDescriptorSource::TemporaryStackSlot(0)),
            )
        }
        _ => {
            emit_expr(callback, emitter, ctx, data);
            emitter.instruction(&format!("mov {}, {}", call_reg, abi::int_result_reg(emitter))); // keep the evaluated callback descriptor in the nested-call scratch register
            abi::emit_push_reg(emitter, call_reg);                              // preserve the evaluated callable descriptor for descriptor-invoker dispatch
            callable_descriptor::emit_load_entry_from_descriptor(emitter, call_reg, call_reg);
            (
                crate::codegen::callables::callable_captures(callback, ctx),
                Some(LoadedDescriptorSource::TemporaryStackSlot(0)),
            )
        }
    }
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
    if expr_call_needs_descriptor_invoker(&args[0], ctx) {
        if let Some(ret_ty) = emit_descriptor_invoker_call_user_func_array_expr(
            &args[0],
            &args[1],
            call_reg,
            save_concat_before_args,
            emitter,
            ctx,
            data,
        ) {
            return Some(ret_ty);
        }
    }

    // -- resolve callback function address and signature --
    let direct_fcc_function =
        crate::codegen::callables::direct_first_class_function_sig(&args[0], ctx);
    let precomputed_sig = direct_fcc_function
        .as_ref()
        .map(|(_, sig)| sig.clone())
        .or_else(|| crate::codegen::callables::callable_sig(&args[0], ctx));
    let (captures, descriptor_source) = if let Some((resolved_name, _)) = direct_fcc_function.as_ref() {
        let label = crate::names::function_symbol(resolved_name);
        abi::emit_symbol_address(emitter, call_reg, &label);
        (Vec::new(), None)
    } else {
        materialize_callback_address_and_preserve_descriptor(
            &args[0],
            call_reg,
            emitter,
            ctx,
            data,
        )
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
            if literal_arg_elems.is_none() {
                if let Some(source) = descriptor_source {
                    emit_loaded_descriptor_source_to_reg(source, call_reg, emitter);
                    emit_call_descriptor_array_invoker(
                        LoadedArraySource::Result,
                        &arr_ty,
                        call_reg,
                        save_concat_before_args,
                        emitter,
                        ctx,
                        data,
                    );
                    PhpType::Mixed
                } else {
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
                }
            } else {
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
            }
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
                    descriptor_source,
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
            descriptor_source,
            save_concat_before_args,
            emitter,
            ctx,
            data,
        )
    };

    if descriptor_source.is_some() {
        abi::emit_release_temporary_stack(emitter, 16);                         // discard the preserved callable descriptor after call_user_func_array()
    }

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

/// Emits descriptor-invoker dispatch for branch-shaped captured `call_user_func_array()` callbacks.
///
/// Branch expressions can select receiver-bound or captured descriptors at runtime, so the
/// invocation must load captures from the selected descriptor instead of trying to use a
/// direct ABI call with compile-time hidden arguments.
#[allow(clippy::too_many_arguments)]
fn emit_descriptor_invoker_call_user_func_array_expr(
    callback: &Expr,
    arg_array: &Expr,
    call_reg: &str,
    concat_saved_before_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let ownership = callable_descriptor_result_ownership(callback);
    if !matches!(ownership, HeapOwnership::Owned | HeapOwnership::Borrowed) {
        return None;
    }

    let sig = crate::codegen::callables::callable_sig(callback, ctx);
    let _callback_ty = emit_expr(callback, emitter, ctx, data);
    if matches!(ownership, HeapOwnership::Borrowed) {
        callable_descriptor::emit_retain_current_descriptor(emitter);
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the selected callable descriptor while building call_user_func_array() args

    let (arr_ty, release_arg_array) = emit_descriptor_invoker_arg_array_for_call_user_func_array(
        arg_array,
        sig.as_ref(),
        emitter,
        ctx,
        data,
    );
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the descriptor-invoker argument array for invocation and cleanup
    abi::emit_load_temporary_stack_slot(emitter, call_reg, 16);
    emit_call_descriptor_array_invoker(
        LoadedArraySource::TemporaryStackSlot(0),
        &arr_ty,
        call_reg,
        concat_saved_before_args,
        emitter,
        ctx,
        data,
    );
    release_preserved_descriptor_invoker_arg_array_after_mixed_result(
        &arr_ty,
        release_arg_array,
        emitter,
    );
    release_preserved_descriptor_after_mixed_result(emitter);
    Some(PhpType::Mixed)
}

/// Returns the ownership class for a callable descriptor expression result.
fn callable_descriptor_result_ownership(callback: &Expr) -> HeapOwnership {
    if matches!(callback.kind, ExprKind::Assignment { .. }) {
        return HeapOwnership::Borrowed;
    }
    expr_result_heap_ownership(callback)
}

/// Emits or reuses the argument container passed to a descriptor invoker.
fn emit_descriptor_invoker_arg_array_for_call_user_func_array(
    arg_array: &Expr,
    sig: Option<&FunctionSig>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> (PhpType, bool) {
    if let ExprKind::ArrayLiteral(elems) = &arg_array.kind {
        if should_encode_invoker_array_literal_refs(sig, elems) {
            let arr_ty = emit_descriptor_invoker_indexed_arg_array_for_call_user_func_array(
                elems,
                sig,
                emitter,
                ctx,
                data,
            );
            return (arr_ty, true);
        }
    }

    let arr_ty = emit_expr(arg_array, emitter, ctx, data);
    let release_arg_array = expr_result_heap_ownership(arg_array) == HeapOwnership::Owned;
    (arr_ty, release_arg_array)
}

/// Returns true when a literal indexed argument array needs ref-cell markers for the invoker.
fn should_encode_invoker_array_literal_refs(sig: Option<&FunctionSig>, elems: &[Expr]) -> bool {
    elems.iter().enumerate().any(|(index, elem)| {
        matches!(elem.kind, ExprKind::Variable(_))
            && sig.is_none_or(|sig| sig.ref_params.get(index).copied().unwrap_or(false))
    })
}

/// Builds a Mixed indexed argument array with invoker-only reference-cell markers.
fn emit_descriptor_invoker_indexed_arg_array_for_call_user_func_array(
    elems: &[Expr],
    sig: Option<&FunctionSig>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("call_user_func_array() descriptor literal argument array");
    let capacity = elems.len().max(4);
    let capacity_reg = abi::int_arg_reg_name(emitter.target, 0);
    let elem_size_reg = abi::int_arg_reg_name(emitter.target, 1);
    abi::emit_load_int_immediate(emitter, capacity_reg, capacity as i64);
    abi::emit_load_int_immediate(emitter, elem_size_reg, 8);
    abi::emit_call_label(emitter, "__rt_array_new");
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // keep the descriptor argument array alive while filling Mixed slots
    abi::emit_load_temporary_stack_slot(emitter, abi::symbol_scratch_reg(emitter), 0);
    crate::codegen::expr::arrays::emit_array_value_type_stamp(
        emitter,
        abi::symbol_scratch_reg(emitter),
        &PhpType::Mixed,
    );

    for (index, elem) in elems.iter().enumerate() {
        if should_encode_invoker_literal_ref_arg(sig, index, elem) {
            if let ExprKind::Variable(var_name) = &elem.kind {
                if !args::emit_ref_arg_variable_address(
                    var_name,
                    "call_user_func_array descriptor arg",
                    emitter,
                    ctx,
                ) {
                    panic!("call_user_func_array() descriptor argument variable not found");
                }
                emit_box_current_ref_arg_address_for_invoker(var_name, emitter, ctx);
                emit_store_descriptor_invoker_arg_array_slot(index, emitter);
                continue;
            }
        }

        let mut ty = emit_expr(elem, emitter, ctx, data);
        let boxed_iterable = crate::codegen::emit_box_iterable_value_for_mixed_container(
            emitter,
            &mut ty,
        );
        if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
            crate::codegen::emit_box_current_expr_value_as_mixed_for_container(emitter, elem, &ty);
        } else if !boxed_iterable {
            retain_borrowed_mixed_arg_for_invoker(emitter, elem, &ty);
        }
        emit_store_descriptor_invoker_arg_array_slot(index, emitter);
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // return the filled descriptor argument array
    PhpType::Array(Box::new(PhpType::Mixed))
}

/// Returns true when this literal element should carry a source-variable ref marker.
fn should_encode_invoker_literal_ref_arg(
    sig: Option<&FunctionSig>,
    index: usize,
    elem: &Expr,
) -> bool {
    matches!(elem.kind, ExprKind::Variable(_))
        && sig.is_none_or(|sig| sig.ref_params.get(index).copied().unwrap_or(false))
}

/// Retains a borrowed boxed Mixed argument before storing it in the invoker array.
fn retain_borrowed_mixed_arg_for_invoker(emitter: &mut Emitter, arg: &Expr, ty: &PhpType) {
    if ty.codegen_repr().is_refcounted() && expr_result_heap_ownership(arg) != HeapOwnership::Owned {
        abi::emit_incref_if_refcounted(emitter, &ty.codegen_repr());
    }
}

/// Boxes the current variable storage address as an invoker-only Mixed marker.
fn emit_box_current_ref_arg_address_for_invoker(
    var_name: &str,
    emitter: &mut Emitter,
    ctx: &Context,
) {
    let ref_cell_reg = abi::secondary_scratch_reg(emitter);
    let marker_tag_reg = abi::tertiary_scratch_reg(emitter);
    let source_tag_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", ref_cell_reg, abi::int_result_reg(emitter))); // preserve the source variable storage address before Mixed marker boxing
    abi::emit_load_int_immediate(emitter, marker_tag_reg, INVOKER_ARG_REF_CELL_TAG);
    abi::emit_load_int_immediate(
        emitter,
        source_tag_reg,
        variable_runtime_value_tag(var_name, ctx) as i64,
    );
    crate::codegen::emit_box_runtime_payload_as_mixed(
        emitter,
        marker_tag_reg,
        ref_cell_reg,
        source_tag_reg,
    );
}

/// Returns the runtime tag for a variable's current codegen type.
fn variable_runtime_value_tag(var_name: &str, ctx: &Context) -> u8 {
    ctx.variables
        .get(var_name)
        .map(|var| crate::codegen::runtime_value_tag(&var.ty.codegen_repr()))
        .unwrap_or_else(|| crate::codegen::runtime_value_tag(&PhpType::Int))
}

/// Stores the current boxed Mixed argument into the synthetic invoker array.
fn emit_store_descriptor_invoker_arg_array_slot(index: usize, emitter: &mut Emitter) {
    let array_reg = abi::symbol_scratch_reg(emitter);
    let len_reg = abi::secondary_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, array_reg, 0);
    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), array_reg, 24 + index * 8);
    abi::emit_load_int_immediate(emitter, len_reg, (index + 1) as i64);
    abi::emit_store_to_address(emitter, len_reg, array_reg, 0);
}

/// Releases the preserved descriptor-invoker argument array while keeping the call result live.
fn release_preserved_descriptor_invoker_arg_array_after_mixed_result(
    arr_ty: &PhpType,
    should_release: bool,
    emitter: &mut Emitter,
) {
    if should_release {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the boxed call result while releasing the invoker argument array
        abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
        abi::emit_decref_if_refcounted(emitter, arr_ty);
        abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the boxed call result after argument-array cleanup
    }
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the preserved descriptor-invoker argument-array slot
}

/// Releases the preserved callable descriptor while keeping the boxed Mixed call result live.
fn release_preserved_descriptor_after_mixed_result(emitter: &mut Emitter) {
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the descriptor-invoker result while releasing the selected descriptor
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
    callable_descriptor::emit_release_current_descriptor(emitter);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the boxed Mixed descriptor-invoker result
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the preserved selected descriptor slot
}

/// Returns true when `call_user_func_array()` must invoke a descriptor-owned environment.
fn expr_call_needs_descriptor_invoker(callback: &Expr, ctx: &Context) -> bool {
    if runtime_callable_expr_result_needs_descriptor_invoker(callback, ctx) {
        return true;
    }

    match &callback.kind {
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_) | ExprKind::Variable(_) => false,
        ExprKind::Assignment { value, .. } => expr_produces_captured_callable(value, ctx),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            expr_produces_captured_callable(then_expr, ctx)
                || expr_produces_captured_callable(else_expr, ctx)
        }
        ExprKind::ShortTernary { value, default }
        | ExprKind::NullCoalesce { value, default } => {
            expr_produces_captured_callable(value, ctx)
                || expr_produces_captured_callable(default, ctx)
        }
        _ => false,
    }
}

/// Returns true when a runtime callable expression must use descriptor-owned metadata.
fn runtime_callable_expr_result_needs_descriptor_invoker(callback: &Expr, ctx: &Context) -> bool {
    if !matches!(
        crate::codegen::functions::infer_contextual_type(callback, ctx).codegen_repr(),
        PhpType::Callable
    ) {
        return false;
    }
    match &callback.kind {
        ExprKind::Variable(name) => ctx.runtime_callable_vars.contains(name),
        ExprKind::ArrayAccess { .. }
        | ExprKind::PropertyAccess { .. }
        | ExprKind::DynamicPropertyAccess { .. }
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::Assignment { .. }
        | ExprKind::Ternary { .. }
        | ExprKind::ShortTernary { .. }
        | ExprKind::NullCoalesce { .. }
        | ExprKind::FunctionCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::ExprCall { .. } => true,
        _ => false,
    }
}

/// Returns true if an expression produces a callable with descriptor-owned environment.
fn expr_produces_captured_callable(expr: &Expr, ctx: &Context) -> bool {
    match &expr.kind {
        ExprKind::Closure { captures, .. } => !captures.is_empty(),
        ExprKind::FirstClassCallable(target) => first_class_target_needs_runtime_capture(target),
        ExprKind::Variable(name) => {
            ctx.closure_captures
                .get(name)
                .is_some_and(|captures| !captures.is_empty())
                || ctx
                    .first_class_callable_targets
                    .get(name)
                    .is_some_and(first_class_target_needs_runtime_capture)
        }
        ExprKind::Assignment { value, .. } => expr_produces_captured_callable(value, ctx),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            expr_produces_captured_callable(then_expr, ctx)
                || expr_produces_captured_callable(else_expr, ctx)
        }
        ExprKind::ShortTernary { value, default }
        | ExprKind::NullCoalesce { value, default } => {
            expr_produces_captured_callable(value, ctx)
                || expr_produces_captured_callable(default, ctx)
        }
        _ => false,
    }
}

/// Returns true when a first-class callable target carries receiver environment.
fn first_class_target_needs_runtime_capture(target: &CallableTarget) -> bool {
    matches!(
        target,
        CallableTarget::Method { .. }
            | CallableTarget::StaticMethod {
                receiver: StaticReceiver::Static,
                ..
            }
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
    if matches!(arr_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return emit_loaded_mixed_array_callback_call(
            array_source,
            call_reg,
            captures,
            sig,
            concat_saved_before_args,
            emitter,
            ctx,
            data,
        );
    }
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

    emit_loaded_array_source_to_reg(array_source, array_reg, emitter);
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
                push_loaded_indexed_array_ref_arg(
                    &elem_ty,
                    target_ty,
                    emitter,
                    ctx,
                    data,
                );
                emitter.label(&done_label);
            } else {
                args::load_array_element_to_result(emitter, &elem_ty, array_reg, 24 + i * elem_size);
                push_loaded_indexed_array_ref_arg(
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
            let _ = push_loaded_indexed_array_value_arg(&elem_ty, target_ty, emitter, ctx, data);
            emitter.label(&done_label);
        } else {
            args::load_array_element_to_result(emitter, &elem_ty, array_reg, 24 + i * elem_size);
            let _ = push_loaded_indexed_array_value_arg(&elem_ty, target_ty, emitter, ctx, data);
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
            PhpType::TaggedScalar => {
                unreachable!("TaggedScalar must be narrowed or boxed before variadic array storage")
            }
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

/// Pushes a loaded indexed-array element as a by-reference callback argument.
fn push_loaded_indexed_array_ref_arg(
    source_elem_ty: &PhpType,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if !matches!(source_elem_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return args::push_current_result_ref_arg_address(
            source_elem_ty,
            target_ty,
            emitter,
            ctx,
            data,
        );
    }

    let special_label = ctx.next_label("cufa_invoker_ref_cell");
    let temp_label = ctx.next_label("cufa_invoker_ref_temp");
    let done_label = ctx.next_label("cufa_invoker_ref_done");
    let result_reg = abi::int_result_reg(emitter);
    let tag_reg = abi::secondary_scratch_reg(emitter);

    abi::emit_load_from_address(emitter, tag_reg, result_reg, 0);
    emit_branch_if_invoker_ref_cell_tag(tag_reg, &special_label, emitter);
    abi::emit_jump(emitter, &temp_label);

    emitter.label(&special_label);
    abi::emit_load_from_address(emitter, result_reg, result_reg, 8);
    args::push_arg_value(emitter, &PhpType::Int);
    abi::emit_jump(emitter, &done_label);

    emitter.label(&temp_label);
    args::push_current_result_ref_arg_address(source_elem_ty, target_ty, emitter, ctx, data);

    emitter.label(&done_label);
    PhpType::Int
}

/// Pushes a loaded indexed-array element as a normal callback argument.
fn push_loaded_indexed_array_value_arg(
    source_elem_ty: &PhpType,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if !matches!(source_elem_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return args::push_loaded_array_element_arg(source_elem_ty, target_ty, emitter, ctx, data);
    }

    let special_label = ctx.next_label("cufa_invoker_ref_value");
    let done_label = ctx.next_label("cufa_invoker_value_done");
    let result_reg = abi::int_result_reg(emitter);
    let tag_reg = abi::secondary_scratch_reg(emitter);

    abi::emit_load_from_address(emitter, tag_reg, result_reg, 0);
    emit_branch_if_invoker_ref_cell_tag(tag_reg, &special_label, emitter);
    let ordinary_ty = args::push_loaded_array_element_arg(source_elem_ty, target_ty, emitter, ctx, data);
    abi::emit_jump(emitter, &done_label);

    emitter.label(&special_label);
    let ref_cell_ty = push_loaded_invoker_ref_cell_value_arg(target_ty, emitter, ctx, data);

    emitter.label(&done_label);
    widen_callback_arg_type(&ordinary_ty, &ref_cell_ty)
}

/// Pushes the value inside an invoker reference-cell marker for a non-ref parameter.
fn push_loaded_invoker_ref_cell_value_arg(
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emit_box_loaded_invoker_ref_cell_value_as_mixed(emitter, ctx);
    let release_mixed_after_coerce = target_ty.is_some_and(|target_ty| {
        !matches!(target_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
            && crate::codegen::expr::can_coerce_result_to_type(&PhpType::Mixed, target_ty)
    });
    if release_mixed_after_coerce {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the boxed ref-cell value while coercing it for a by-value parameter
    }
    let (pushed_ty, _boxed_to_mixed) =
        args::coerce_current_value_to_target(emitter, ctx, data, &PhpType::Mixed, target_ty);
    if release_mixed_after_coerce {
        args::release_preserved_mixed_after_arg_coercion(emitter, &pushed_ty);
    }
    args::push_arg_value(emitter, &pushed_ty);
    pushed_ty
}

/// Boxes the value referenced by an invoker marker into an owned Mixed cell.
fn emit_box_loaded_invoker_ref_cell_value_as_mixed(emitter: &mut Emitter, ctx: &mut Context) {
    let result_reg = abi::int_result_reg(emitter);
    let ref_cell_reg = abi::symbol_scratch_reg(emitter);
    let tag_reg = abi::secondary_scratch_reg(emitter);
    let lo_reg = abi::tertiary_scratch_reg(emitter);
    let hi_reg = match emitter.target.arch {
        Arch::AArch64 => "x12",
        Arch::X86_64 => "rdx",
    };
    let string_hi_label = ctx.next_label("cufa_invoker_ref_string_hi");
    let box_label = ctx.next_label("cufa_invoker_ref_box");

    abi::emit_load_from_address(emitter, ref_cell_reg, result_reg, 8);
    abi::emit_load_from_address(emitter, tag_reg, result_reg, 16);
    abi::emit_load_from_address(emitter, lo_reg, ref_cell_reg, 0);
    abi::emit_load_int_immediate(emitter, hi_reg, 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #1", tag_reg));               // does the referenced value use a two-word string slot?
            emitter.instruction(&format!("b.eq {}", string_hi_label));          // load the string length only for string reference cells
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, 1", tag_reg));                // does the referenced value use a two-word string slot?
            emitter.instruction(&format!("je {}", string_hi_label));            // load the string length only for string reference cells
        }
    }
    abi::emit_jump(emitter, &box_label);

    emitter.label(&string_hi_label);
    abi::emit_load_from_address(emitter, hi_reg, ref_cell_reg, 8);

    emitter.label(&box_label);
    crate::codegen::emit_box_runtime_payload_as_mixed(emitter, tag_reg, lo_reg, hi_reg);
}

/// Branches when a boxed Mixed element represents an invoker reference-cell marker.
fn emit_branch_if_invoker_ref_cell_tag(tag_reg: &str, label: &str, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #{}", tag_reg, INVOKER_ARG_REF_CELL_TAG)); // check for an invoker-only by-reference argument marker
            emitter.instruction(&format!("b.eq {}", label));                    // use the original caller storage when this slot is a marker
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", tag_reg, INVOKER_ARG_REF_CELL_TAG)); // check for an invoker-only by-reference argument marker
            emitter.instruction(&format!("je {}", label));                      // use the original caller storage when this slot is a marker
        }
    }
}

/// Emits assembly for a callback call whose argument container is boxed as `Mixed`.
#[allow(clippy::too_many_arguments)]
fn emit_loaded_mixed_array_callback_call(
    array_source: LoadedArraySource,
    call_reg: &str,
    captures: &[(String, PhpType, bool)],
    sig: &FunctionSig,
    concat_saved_before_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let (mixed_reg, tag_reg, payload_reg) = match emitter.target.arch {
        Arch::AArch64 => ("x20", "x21", "x22"),
        Arch::X86_64 => ("r13", "r14", "r15"),
    };
    let indexed_label = ctx.next_label("cufa_mixed_indexed");
    let assoc_label = ctx.next_label("cufa_mixed_assoc");
    let done_label = ctx.next_label("cufa_mixed_done");
    let indexed_ty = PhpType::Array(Box::new(PhpType::Mixed));
    let assoc_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    };

    emit_loaded_array_source_to_reg(array_source, mixed_reg, emitter);
    abi::emit_load_from_address(emitter, tag_reg, mixed_reg, 0);
    abi::emit_load_from_address(emitter, payload_reg, mixed_reg, 8);
    abi::emit_push_reg(emitter, payload_reg);                                   // preserve the unboxed argument container while branching by Mixed tag
    emit_branch_if_mixed_arg_tag(
        tag_reg,
        crate::codegen::runtime_value_tag(&indexed_ty),
        &indexed_label,
        emitter,
    );
    emit_branch_if_mixed_arg_tag(
        tag_reg,
        crate::codegen::runtime_value_tag(&assoc_ty),
        &assoc_label,
        emitter,
    );
    emit_call_user_func_array_invalid_mixed_args_abort(emitter, data);

    emitter.label(&indexed_label);
    emit_loaded_array_callback_call(
        LoadedArraySource::TemporaryStackSlot(0),
        &indexed_ty,
        None,
        call_reg,
        captures,
        sig,
        concat_saved_before_args,
        emitter,
        ctx,
        data,
    );
    abi::emit_jump(emitter, &done_label);

    emitter.label(&assoc_label);
    emit_loaded_assoc_array_callback_call(
        LoadedArraySource::TemporaryStackSlot(0),
        &assoc_ty,
        call_reg,
        captures,
        sig,
        concat_saved_before_args,
        emitter,
        ctx,
        data,
    );
    abi::emit_jump(emitter, &done_label);

    emitter.label(&done_label);
    abi::emit_release_temporary_stack(emitter, 16);                             // drop the borrowed unboxed argument-container pointer
    sig.return_type.clone()
}

/// Branches to `label` when a boxed invoker argument carries `expected_tag`.
pub(crate) fn emit_branch_if_mixed_arg_tag(
    tag_reg: &str,
    expected_tag: u8,
    label: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #{}", tag_reg, expected_tag)); // check the runtime tag of the boxed invoker argument container
            emitter.instruction(&format!("b.eq {}", label));                    // dispatch to the handler for this argument-container shape
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", tag_reg, expected_tag)); // check the runtime tag of the boxed invoker argument container
            emitter.instruction(&format!("je {}", label));                      // dispatch to the handler for this argument-container shape
        }
    }
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

    emit_loaded_array_source_to_reg(array_source, hash_reg, emitter);

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
        let variadic_ty = args::emit_loaded_assoc_variadic_array_arg(
            hash_reg,
            &elem_ty,
            sig,
            regular_param_count,
            regular_param_count,
            "build associative variadic array for callback",
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

/// Emits assembly for loaded array unknown callback call.
pub(crate) fn emit_loaded_array_unknown_callback_call(
    array_source: LoadedArraySource,
    arr_ty: &PhpType,
    call_reg: &str,
    captures: &[(String, PhpType, bool)],
    descriptor_source: Option<LoadedDescriptorSource>,
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
            descriptor_source,
            concat_saved_before_args,
            emitter,
            ctx,
            data,
        );
    }

    let cases = callable_dispatch::runtime_callable_cases(ctx, data, captures, Some(arr_ty));
    if !cases.is_empty() {
        return emit_loaded_indexed_array_unknown_callback_call(
            array_source,
            arr_ty,
            call_reg,
            captures,
            &cases,
            descriptor_source,
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
    let cases = callable_dispatch::runtime_callable_cases(ctx, data, &[], Some(arr_ty));
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
        LoadedArraySource::ArgumentRegister(index) => (
            LoadedArraySource::ArgumentRegister(index),
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
        emit_call_descriptor_array_invoker(
            array_source,
            arr_ty,
            call_reg,
            concat_saved_before_args,
            emitter,
            ctx,
            data,
        );
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

/// Calls the uniform invoker stored in the matched callable descriptor.
pub(crate) fn emit_call_descriptor_array_invoker(
    array_source: LoadedArraySource,
    arr_ty: &PhpType,
    descriptor_reg: &str,
    concat_saved_before_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emit_call_descriptor_array_invoker_with_label(
        array_source,
        arr_ty,
        descriptor_reg,
        None,
        concat_saved_before_args,
        emitter,
        ctx,
        data,
    );
}

/// Calls a descriptor invoker, optionally overriding the invoker slot with a case label.
#[allow(clippy::too_many_arguments)]
fn emit_call_descriptor_array_invoker_with_label(
    array_source: LoadedArraySource,
    arr_ty: &PhpType,
    descriptor_reg: &str,
    invoker_label: Option<&str>,
    concat_saved_before_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let descriptor_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    let invoker_reg = abi::symbol_scratch_reg(emitter);
    let missing_label = ctx.next_label("cufa_descriptor_invoker_missing");
    let ready_label = ctx.next_label("cufa_descriptor_invoker_ready");

    abi::emit_push_reg(emitter, descriptor_reg);                                // preserve the callable descriptor while normalizing the invoker argument container
    let array_source = array_source_after_descriptor_push(array_source);
    let normalized_arg_ty =
        emit_normalized_invoker_arg_mixed(array_source, arr_ty, array_arg_reg, emitter, ctx, data);
    abi::emit_push_reg(emitter, array_arg_reg);                                  // preserve the temporary boxed Mixed argument container for release after invocation
    abi::emit_load_temporary_stack_slot(emitter, descriptor_arg_reg, 16);
    if !concat_saved_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }
    if let Some(invoker_label) = invoker_label {
        abi::emit_symbol_address(emitter, invoker_reg, invoker_label);
    } else {
        callable_descriptor::emit_load_invoker_from_descriptor(emitter, invoker_reg, descriptor_arg_reg);
    }
    emit_branch_if_descriptor_invoker_missing(invoker_reg, &missing_label, &ready_label, emitter);

    emitter.label(&missing_label);
    emit_descriptor_invoker_missing_abort(emitter, data);

    emitter.label(&ready_label);
    abi::emit_call_reg(emitter, invoker_reg);
    if concat_saved_before_args {
        emit_release_normalized_invoker_arg_mixed(&normalized_arg_ty, emitter);
        abi::emit_release_temporary_stack(emitter, 16);                         // discard the preserved callable descriptor after invocation
        crate::codegen::expr::restore_concat_offset_after_nested_call(
            emitter,
            ctx,
            &PhpType::Mixed,
        );
    } else {
        crate::codegen::expr::restore_concat_offset_after_nested_call(
            emitter,
            ctx,
            &PhpType::Mixed,
        );
        emit_release_normalized_invoker_arg_mixed(&normalized_arg_ty, emitter);
        abi::emit_release_temporary_stack(emitter, 16);                         // discard the preserved callable descriptor after invocation
    }
}

/// Loads the descriptor that should feed a matched case invoker.
fn emit_case_descriptor_for_invoker<'a>(
    case: &'a RuntimeCallableCase,
    descriptor_source: Option<LoadedDescriptorSource>,
    descriptor_reg: &str,
    emitter: &mut Emitter,
) -> Option<Option<&'a str>> {
    if !case.has_invoker {
        return None;
    }
    if let Some(source) = descriptor_source {
        emit_loaded_descriptor_source_to_reg(source, descriptor_reg, emitter);
        return Some(case.invoker_label.as_deref());
    }
    if case.captures.is_empty() {
        abi::emit_symbol_address(emitter, descriptor_reg, &case.descriptor_label);
        return Some(None);
    }
    None
}

/// Materializes the descriptor invoker argument as a boxed Mixed container.
fn emit_normalized_invoker_arg_mixed(
    array_source: LoadedArraySource,
    arr_ty: &PhpType,
    dest_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emit_loaded_array_source_to_reg(array_source, dest_reg, emitter);
    match arr_ty {
        PhpType::Array(elem_ty) => {
            emit_clone_indexed_array_for_invoker(dest_reg, elem_ty, emitter);
            let normalized_ty = PhpType::Array(Box::new(PhpType::Mixed));
            emit_box_invoker_arg_clone_as_mixed(dest_reg, &normalized_ty, emitter);
            PhpType::Mixed
        }
        PhpType::AssocArray { value, .. } => {
            emit_clone_assoc_array_for_invoker_with_value_type(dest_reg, value, emitter);
            let normalized_ty = PhpType::AssocArray {
                key: Box::new(PhpType::Mixed),
                value: Box::new(PhpType::Mixed),
            };
            emit_box_invoker_arg_clone_as_mixed(dest_reg, &normalized_ty, emitter);
            PhpType::Mixed
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_clone_runtime_mixed_invoker_arg_as_mixed(dest_reg, emitter, ctx, data);
            PhpType::Mixed
        }
        _ => arr_ty.codegen_repr(),
    }
}

/// Clones a boxed runtime Mixed argument container into a normalized boxed Mixed container.
pub(crate) fn emit_clone_runtime_mixed_invoker_arg_as_mixed(
    dest_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let tag_reg = abi::secondary_scratch_reg(emitter);
    let payload_reg = abi::tertiary_scratch_reg(emitter);
    let indexed_label = ctx.next_label("cufa_normalize_mixed_indexed");
    let assoc_label = ctx.next_label("cufa_normalize_mixed_assoc");
    let done_label = ctx.next_label("cufa_normalize_mixed_done");
    let indexed_ty = PhpType::Array(Box::new(PhpType::Mixed));
    let assoc_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    };

    abi::emit_load_from_address(emitter, tag_reg, dest_reg, 0);
    abi::emit_load_from_address(emitter, payload_reg, dest_reg, 8);
    abi::emit_push_reg(emitter, payload_reg);                                   // preserve the unboxed runtime argument container while normalizing by Mixed tag
    emit_branch_if_mixed_arg_tag(
        tag_reg,
        crate::codegen::runtime_value_tag(&indexed_ty),
        &indexed_label,
        emitter,
    );
    emit_branch_if_mixed_arg_tag(
        tag_reg,
        crate::codegen::runtime_value_tag(&assoc_ty),
        &assoc_label,
        emitter,
    );
    emit_call_user_func_array_invalid_mixed_args_abort(emitter, data);

    emitter.label(&indexed_label);
    abi::emit_load_temporary_stack_slot(emitter, dest_reg, 0);
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the borrowed indexed-array pointer after loading it for cloning
    emit_clone_indexed_array_for_invoker_with_runtime_tag(dest_reg, emitter);
    emit_box_invoker_arg_clone_as_mixed(dest_reg, &indexed_ty, emitter);
    abi::emit_jump(emitter, &done_label);

    emitter.label(&assoc_label);
    abi::emit_load_temporary_stack_slot(emitter, dest_reg, 0);
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the borrowed hash pointer after loading it for cloning
    emit_clone_assoc_array_for_invoker(dest_reg, emitter);
    emit_box_invoker_arg_clone_as_mixed(dest_reg, &assoc_ty, emitter);

    emitter.label(&done_label);
}

/// Clones and converts an indexed callback argument array to boxed Mixed slots.
pub(crate) fn emit_clone_indexed_array_for_invoker(
    dest_reg: &str,
    elem_ty: &PhpType,
    emitter: &mut Emitter,
) {
    let array_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let tag_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    let result_reg = abi::int_result_reg(emitter);
    if array_arg_reg != dest_reg {
        emitter.instruction(&format!("mov {}, {}", array_arg_reg, dest_reg));   // pass the callback-argument array to the clone helper without mutating caller storage
    }
    abi::emit_call_label(emitter, "__rt_array_clone_shallow");
    if array_arg_reg != result_reg {
        emitter.instruction(&format!("mov {}, {}", array_arg_reg, result_reg)); // pass the cloned argument array to the Mixed-slot conversion helper
    }
    abi::emit_load_int_immediate(
        emitter,
        tag_arg_reg,
        crate::codegen::runtime_value_tag(&elem_ty.codegen_repr()) as i64,
    );
    abi::emit_call_label(emitter, "__rt_array_to_mixed");
    if dest_reg != result_reg {
        emitter.instruction(&format!("mov {}, {}", dest_reg, result_reg));      // keep the normalized Mixed argument array in the invoker ABI register
    }
}

/// Clones and converts a runtime-typed indexed callback argument array to boxed Mixed slots.
pub(crate) fn emit_clone_indexed_array_for_invoker_with_runtime_tag(
    dest_reg: &str,
    emitter: &mut Emitter,
) {
    let array_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let tag_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    let result_reg = abi::int_result_reg(emitter);
    if array_arg_reg != dest_reg {
        emitter.instruction(&format!("mov {}, {}", array_arg_reg, dest_reg));   // pass the runtime-typed callback array to the clone helper without mutating caller storage
    }
    abi::emit_call_label(emitter, "__rt_array_clone_shallow");
    if array_arg_reg != result_reg {
        emitter.instruction(&format!("mov {}, {}", array_arg_reg, result_reg)); // pass the cloned runtime-typed array to the Mixed-slot conversion helper
    }
    emit_load_indexed_array_runtime_value_type_tag(array_arg_reg, tag_arg_reg, emitter);
    abi::emit_call_label(emitter, "__rt_array_to_mixed");
    if dest_reg != result_reg {
        emitter.instruction(&format!("mov {}, {}", dest_reg, result_reg));      // keep the normalized Mixed argument array in the invoker ABI register
    }
}

/// Loads an indexed array's runtime value-type tag from its packed heap header.
fn emit_load_indexed_array_runtime_value_type_tag(
    array_reg: &str,
    tag_reg: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {}, [{}, #-8]", tag_reg, array_reg)); // load the packed indexed-array metadata before Mixed-slot conversion
            emitter.instruction(&format!("lsr {}, {}, #8", tag_reg, tag_reg));  // move the indexed-array value_type tag into the low bits
            emitter.instruction(&format!("and {}, {}, #0x7f", tag_reg, tag_reg)); // isolate the runtime indexed-array value_type tag
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, QWORD PTR [{} - 8]", tag_reg, array_reg)); // load the packed indexed-array metadata before Mixed-slot conversion
            emitter.instruction(&format!("shr {}, 8", tag_reg));                // move the indexed-array value_type tag into the low bits
            emitter.instruction(&format!("and {}, 0x7f", tag_reg));             // isolate the runtime indexed-array value_type tag
        }
    }
}

/// Clones and converts an associative callback argument array to boxed Mixed entries.
pub(crate) fn emit_clone_assoc_array_for_invoker(dest_reg: &str, emitter: &mut Emitter) {
    emit_clone_assoc_array_for_invoker_with_value_type(dest_reg, &PhpType::Int, emitter);
}

/// Clones an associative callback argument array and boxes entries when needed.
pub(crate) fn emit_clone_assoc_array_for_invoker_with_value_type(
    dest_reg: &str,
    value_ty: &PhpType,
    emitter: &mut Emitter,
) {
    let hash_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let result_reg = abi::int_result_reg(emitter);
    if hash_arg_reg != dest_reg {
        emitter.instruction(&format!("mov {}, {}", hash_arg_reg, dest_reg));    // pass the callback-argument hash to the clone helper without mutating caller storage
    }
    abi::emit_call_label(emitter, "__rt_hash_clone_shallow");
    if hash_arg_reg != result_reg {
        emitter.instruction(&format!("mov {}, {}", hash_arg_reg, result_reg));  // pass the cloned argument hash to the Mixed-entry conversion helper
    }
    if !matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(emitter, "__rt_hash_to_mixed");
    }
    if dest_reg != result_reg {
        emitter.instruction(&format!("mov {}, {}", dest_reg, result_reg));      // keep the normalized Mixed argument hash in the invoker ABI register
    }
}

/// Boxes the normalized argument clone as `Mixed` and releases the caller-side clone owner.
pub(crate) fn emit_box_invoker_arg_clone_as_mixed(dest_reg: &str, container_ty: &PhpType, emitter: &mut Emitter) {
    let tag_reg = abi::secondary_scratch_reg(emitter);
    let zero_reg = abi::tertiary_scratch_reg(emitter);

    abi::emit_push_reg(emitter, dest_reg);                                      // preserve the cloned argument container while Mixed boxing retains it
    abi::emit_load_int_immediate(
        emitter,
        tag_reg,
        crate::codegen::runtime_value_tag(container_ty) as i64,
    );
    abi::emit_load_int_immediate(emitter, zero_reg, 0);
    crate::codegen::emit_box_runtime_payload_as_mixed(emitter, tag_reg, dest_reg, zero_reg);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the boxed Mixed argument while dropping the clone owner
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_decref_if_refcounted(emitter, container_ty);
    abi::emit_pop_reg(emitter, dest_reg);                                       // move the boxed Mixed argument into the invoker ABI register
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the preserved clone slot after ownership transfer
}

/// Releases the temporary boxed Mixed argument container while preserving the Mixed call result.
fn emit_release_normalized_invoker_arg_mixed(array_ty: &PhpType, emitter: &mut Emitter) {
    abi::emit_push_result_value(emitter, &PhpType::Mixed);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_decref_if_refcounted(emitter, array_ty);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
    abi::emit_release_temporary_stack(emitter, 16);
}

/// Branches to the abort path when a descriptor lacks an invoker pointer.
fn emit_branch_if_descriptor_invoker_missing(
    invoker_reg: &str,
    missing_label: &str,
    ready_label: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz {}, {}", invoker_reg, missing_label)); // abort when the descriptor has no uniform invoker
            emitter.instruction(&format!("b {}", ready_label));                 // continue when a descriptor invoker is available
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {}, {}", invoker_reg, invoker_reg)); // abort when the descriptor has no uniform invoker
            emitter.instruction(&format!("je {}", missing_label));              // branch to the fatal descriptor-invoker diagnostic
            emitter.instruction(&format!("jmp {}", ready_label));               // continue when a descriptor invoker is available
        }
    }
}

/// Emits the fatal diagnostic for descriptors without a generated invoker.
fn emit_descriptor_invoker_missing_abort(emitter: &mut Emitter, data: &mut DataSection) {
    let (message_label, message_len) = data.add_string(
        b"Fatal error: callable descriptor does not provide a runtime invoker\n",
    );
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the descriptor-invoker diagnostic to stderr
            abi::emit_symbol_address(emitter, "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the descriptor-invoker diagnostic byte length to write()
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // write the descriptor-invoker diagnostic to stderr
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the descriptor-invoker diagnostic byte length to write()
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal descriptor-invoker diagnostic
            abi::emit_exit(emitter, 1);
        }
    }
}

/// Emits assembly for loaded indexed array unknown callback call.
#[allow(clippy::too_many_arguments)]
fn emit_loaded_indexed_array_unknown_callback_call(
    array_source: LoadedArraySource,
    arr_ty: &PhpType,
    call_reg: &str,
    captures: &[(String, PhpType, bool)],
    cases: &[RuntimeCallableCase],
    descriptor_source: Option<LoadedDescriptorSource>,
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
        LoadedArraySource::ArgumentRegister(index) => LoadedArraySource::ArgumentRegister(index),
    };
    let descriptor_source = descriptor_source_after_array_push(descriptor_source, pushed_array);

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
        if let Some(invoker_label) =
            emit_case_descriptor_for_invoker(case, descriptor_source, call_reg, emitter)
        {
            emit_call_descriptor_array_invoker_with_label(
                array_source,
                arr_ty,
                call_reg,
                invoker_label,
                concat_saved_before_args,
                emitter,
                ctx,
                data,
            );
        } else {
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
        }
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

    emit_loaded_array_source_to_reg(array_source, array_reg, emitter);
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
    descriptor_source: Option<LoadedDescriptorSource>,
    concat_saved_before_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let done_label = ctx.next_label("cufa_unknown_assoc_done");
    let cases = callable_dispatch::runtime_callable_cases(ctx, data, captures, Some(arr_ty));
    let pushed_array = matches!(array_source, LoadedArraySource::Result);
    let array_source = match array_source {
        LoadedArraySource::Result => {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // preserve the associative callback-argument hash for runtime signature dispatch
            LoadedArraySource::TemporaryStackSlot(0)
        }
        LoadedArraySource::TemporaryStackSlot(offset) => LoadedArraySource::TemporaryStackSlot(offset),
        LoadedArraySource::ArgumentRegister(index) => LoadedArraySource::ArgumentRegister(index),
    };
    let descriptor_source = descriptor_source_after_array_push(descriptor_source, pushed_array);

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
        if let Some(invoker_label) =
            emit_case_descriptor_for_invoker(case, descriptor_source, call_reg, emitter)
        {
            emit_call_descriptor_array_invoker_with_label(
                array_source,
                arr_ty,
                call_reg,
                invoker_label,
                concat_saved_before_args,
                emitter,
                ctx,
                data,
            );
        } else {
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
        }
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

    emit_loaded_array_source_to_reg(array_source, array_reg, emitter);
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
            abi::emit_symbol_address(emitter, "x1", &message_label);
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
            abi::emit_symbol_address(emitter, "x1", &message_label);
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

/// Emits assembly for a descriptor invoker argument-container type mismatch.
pub(crate) fn emit_call_user_func_array_invalid_mixed_args_abort(emitter: &mut Emitter, data: &mut DataSection) {
    let (message_label, message_len) = data.add_string(
        b"Fatal error: callable descriptor invoker expected an indexed or associative argument array\n",
    );
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the descriptor argument-shape diagnostic to stderr
            abi::emit_symbol_address(emitter, "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the descriptor argument-shape diagnostic byte length to write()
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // write the descriptor argument-shape diagnostic to stderr
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the descriptor argument-shape diagnostic byte length to write()
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the descriptor argument-shape diagnostic
            abi::emit_exit(emitter, 1);
        }
    }
}

/// Emits assembly for dynamic string callback abort.
pub(crate) fn emit_dynamic_string_callback_abort(emitter: &mut Emitter, data: &mut DataSection) {
    let (message_label, message_len) = data.add_string(
        b"Fatal error: dynamic string callback could not be resolved\n",
    );
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the dynamic callback diagnostic to stderr
            abi::emit_symbol_address(emitter, "x1", &message_label);
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
