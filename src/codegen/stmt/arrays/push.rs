//! Purpose:
//! Lowers array append statements and runtime push helper calls.
//! Handles statement-level array mutation after expression operands are evaluated.
//!
//! Called from:
//! - `crate::codegen::stmt::arrays`
//!
//! Key details:
//! - Mutation paths must preserve source-order side effects and update heap ownership consistently.

use crate::codegen::abi;
use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_result_to_type, emit_expr, expr_result_heap_ownership};
use crate::codegen::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{merge_array_key_types, PhpType};

/// Emits `$array[] = value` (append to a named array variable).
/// Handles `ArrayAccess` objects, by-ref parameters, `Mixed` conversions, and
/// refcounted push cleanup. Routes to architecture-specific helpers for x86_64.
/// Updates the variable's inferred element type when a heterogeneous value is appended.
pub(super) fn emit_array_push_stmt(
    array: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("${}[] = ...", array));
    let var = match ctx.variables.get(array) {
        Some(v) => v,
        None => {
            emitter.comment(&format!("WARNING: undefined variable ${}", array));
            return;
        }
    };
    let offset = var.stack_offset;
    let var_ty = var.ty.clone();
    let var_static_ty = var.static_ty.clone();
    let is_ref = ctx.ref_params.contains(array);
    if crate::codegen::expr::arrays::type_is_array_access_object(&var_static_ty, ctx)
        || crate::codegen::expr::arrays::type_is_array_access_object(&var_ty, ctx)
    {
        let object = Expr::new(ExprKind::Variable(array.to_string()), value.span);
        let null_index = Expr::new(ExprKind::Null, value.span);
        crate::codegen::expr::arrays::emit_array_access_offset_set(
            &object,
            &null_index,
            value,
            emitter,
            ctx,
            data,
        );
        return;
    }
    if let PhpType::AssocArray {
        key,
        value: assoc_value,
    } = var_ty.clone()
    {
        emit_assoc_array_push_stmt(
            array,
            value,
            emitter,
            ctx,
            data,
            offset,
            is_ref,
            *key,
            *assoc_value,
        );
        return;
    }
    if emitter.target.arch == Arch::X86_64 {
        emit_array_push_stmt_linux_x86_64(array, value, emitter, ctx, data, offset, is_ref);
        return;
    }

    if is_ref {
        abi::load_at_offset(emitter, "x9", offset);                                 // load ref pointer
        emitter.instruction("ldr x0, [x9]");                                    // dereference to get array heap pointer
    } else {
        abi::load_at_offset(emitter, "x0", offset);                                 // load array heap pointer from stack frame
    }
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack to preserve it
    let elem_ty = match ctx.variables.get(array) {
        Some(v) => match &v.ty {
            PhpType::Array(t) => *t.clone(),
            _ => PhpType::Int,
        },
        None => PhpType::Int,
    };
    let source_owned = expr_result_heap_ownership(value) == HeapOwnership::Owned;
    let mut val_ty = emit_expr(value, emitter, ctx, data);
    let boxed_iterable =
        crate::codegen::emit_box_iterable_value_for_mixed_container(emitter, &mut val_ty);
    let effective_elem_ty = effective_indexed_push_type(&elem_ty, &val_ty, ctx);
    let converted_to_mixed =
        matches!(effective_elem_ty, PhpType::Mixed) && !matches!(elem_ty, PhpType::Mixed);
    let mut boxed_value_for_mixed = false;
    if matches!(effective_elem_ty, PhpType::Mixed)
        && !matches!(val_ty, PhpType::Mixed | PhpType::Union(_))
    {
        crate::codegen::emit_box_current_expr_value_as_mixed_for_container(
            emitter, value, &val_ty,
        );
        val_ty = PhpType::Mixed;
        boxed_value_for_mixed = true;
    } else if matches!(effective_elem_ty, PhpType::Mixed) && matches!(val_ty, PhpType::Union(_)) {
        val_ty = PhpType::Mixed;
    }
    update_callable_array_metadata(array, value, &val_ty, ctx);
    let release_after_refcounted_push = boxed_value_for_mixed
        || boxed_iterable
        || (source_owned
            && matches!(
                val_ty,
                PhpType::Mixed
                    | PhpType::Union(_)
                    | PhpType::Array(_)
                    | PhpType::AssocArray { .. }
                    | PhpType::Object(_)
            ));
    emitter.instruction("ldr x9, [sp], #16");                                   // pop saved array pointer into x9
    if elem_ty != effective_elem_ty {
        let updated_ty = PhpType::Array(Box::new(effective_elem_ty.clone()));
        ctx.update_var_type_and_ownership(
            array,
            updated_ty.clone(),
            super::super::helpers::local_slot_ownership_after_store(&updated_ty),
        );
    }
    if converted_to_mixed {
        emitter.instruction("str x0, [sp, #-16]!");                             // preserve the boxed appended value across mixed-array conversion
        emitter.instruction("mov x0, x9");                                      // pass the current indexed-array pointer to the mixed conversion helper
        let conversion_source_ty = if matches!(elem_ty, PhpType::Never) {
            &effective_elem_ty
        } else {
            &elem_ty
        };
        abi::emit_load_int_immediate(
            emitter,
            "x1",
            super::super::helpers::indexed_array_runtime_value_tag(conversion_source_ty),
        );
        abi::emit_call_label(emitter, "__rt_array_to_mixed");                   // box existing typed slots before appending a heterogeneous value
        emitter.instruction("mov x9, x0");                                      // keep the converted indexed-array pointer as the append receiver
        emitter.instruction("ldr x0, [sp], #16");                               // restore the boxed appended value after conversion
    }
    match &val_ty {
        PhpType::Int | PhpType::Bool => {
            emitter.instruction("mov x1, x0");                                  // move value to x1 (second arg for runtime call)
            emitter.instruction("mov x0, x9");                                  // move array pointer to x0 (first arg)
            emitter.instruction("bl __rt_array_push_int");                      // call runtime: append integer to dynamic array
        }
        PhpType::Float => {
            emitter.instruction("fmov x1, d0");                                 // move float bits to integer register
            emitter.instruction("mov x0, x9");                                  // move array pointer to x0 (first arg)
            emitter.instruction("bl __rt_array_push_int");                      // call runtime: append float bits as 8 bytes
        }
        PhpType::Str => {
            emitter.instruction("mov x0, x9");                                  // move array pointer to x0
            emitter.instruction("bl __rt_array_push_str");                      // call runtime: persist + append string to array
        }
        PhpType::Mixed | PhpType::Union(_) | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            if release_after_refcounted_push {
                abi::emit_push_reg(emitter, "x0");
            }
            emitter.instruction("mov x1, x0");                                  // move nested array/object pointer to x1
            emitter.instruction("mov x0, x9");                                  // move outer array pointer to x0
            emitter.instruction("bl __rt_array_push_refcounted");               // append retained pointer and stamp array metadata
            if release_after_refcounted_push {
                crate::codegen::emit_release_pushed_refcounted_temp_after_array_push(emitter, &val_ty);
            }
        }
        PhpType::Callable => {
            emitter.instruction("mov x1, x0");                                  // move callable descriptor pointer to x1
            emitter.instruction("mov x0, x9");                                  // move outer array pointer to x0
            emitter.instruction("bl __rt_array_push_int");                      // append descriptor pointer without refcount ownership
        }
        _ => {}
    }
    if is_ref {
        abi::load_at_offset(emitter, "x9", offset);                                 // load ref pointer
        emitter.instruction("str x0, [x9]");                                    // store new array ptr through ref
    } else {
        abi::store_at_offset(emitter, "x0", offset);                                // save possibly-new array pointer
    }
}

/// Emits `$array[] = value` for a hash-backed associative array.
///
/// Preserves PHP source-order evaluation, stores the appended value with a per-entry
/// runtime tag, and delegates next-key selection plus insertion to `__rt_hash_append`.
/// Updates local type metadata so later reads see the widened key/value shape.
#[allow(clippy::too_many_arguments)]
fn emit_assoc_array_push_stmt(
    array: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    offset: usize,
    is_ref: bool,
    assoc_key_ty: PhpType,
    assoc_value_ty: PhpType,
) {
    let table_reg = abi::int_result_reg(emitter);
    let ref_reg = abi::symbol_scratch_reg(emitter);
    if is_ref {
        abi::load_at_offset(emitter, ref_reg, offset);                         // load the by-reference slot that points at the hash-table local
        abi::emit_load_from_address(emitter, table_reg, ref_reg, 0);           // dereference the by-reference slot to get the current hash-table pointer
    } else {
        abi::load_at_offset(emitter, table_reg, offset);                       // load the current hash-table pointer from the local slot
    }
    abi::emit_push_reg(emitter, table_reg);                                    // preserve the hash-table pointer while evaluating the appended value

    let mut val_ty = emit_expr(value, emitter, ctx, data);
    if matches!(val_ty, PhpType::Mixed | PhpType::Union(_))
        && !matches!(assoc_value_ty, PhpType::Mixed | PhpType::Union(_))
        && crate::codegen::expr::can_coerce_result_to_type(&val_ty, &assoc_value_ty)
    {
        coerce_result_to_type(emitter, ctx, data, &val_ty, &assoc_value_ty);
        val_ty = assoc_value_ty.clone();
    }
    let boxed_iterable =
        crate::codegen::emit_box_iterable_value_for_mixed_container(emitter, &mut val_ty);
    if !boxed_iterable {
        super::super::helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }
    update_callable_array_metadata(array, value, &val_ty, ctx);

    let updated_key_ty = merge_array_key_types(assoc_key_ty.clone(), PhpType::Int);
    let updated_value_ty = if assoc_value_ty == val_ty {
        assoc_value_ty.clone()
    } else {
        PhpType::Mixed
    };
    if updated_key_ty != assoc_key_ty || updated_value_ty != assoc_value_ty {
        let updated_ty = PhpType::AssocArray {
            key: Box::new(updated_key_ty),
            value: Box::new(updated_value_ty),
        };
        ctx.update_var_type_and_ownership(
            array,
            updated_ty.clone(),
            super::super::helpers::local_slot_ownership_after_store(&updated_ty),
        );
    }

    match emitter.target.arch {
        Arch::AArch64 => {
            let (val_lo, val_hi) = match &val_ty {
                PhpType::Int | PhpType::Bool => ("x0", "xzr"),
                PhpType::Str => {
                    abi::emit_call_label(emitter, "__rt_str_persist");         // persist the appended string value before transferring ownership to the hash table
                    ("x1", "x2")
                }
                PhpType::Float => {
                    emitter.instruction("fmov x9, d0");                         // move the floating-point payload bits into an integer register for the hash runtime ABI
                    ("x9", "xzr")
                }
                _ => ("x0", "xzr"),
            };
            emitter.instruction(&format!("mov x1, {}", val_lo));                // place the low payload word into the hash-append helper argument
            emitter.instruction(&format!("mov x2, {}", val_hi));                // place the high payload word into the hash-append helper argument
            let value_tag = super::super::super::runtime_value_tag(&val_ty);
            emitter.instruction(&format!("mov x3, #{}", value_tag));            // materialize the runtime value tag for the appended hash payload
            abi::emit_pop_reg(emitter, "x0");                                  // restore the preserved hash-table pointer into the hash-append receiver argument
            abi::emit_call_label(emitter, "__rt_hash_append");                 // append using PHP's next automatic integer key for hash-backed arrays
        }
        Arch::X86_64 => {
            match &val_ty {
                PhpType::Str => {
                    abi::emit_call_label(emitter, "__rt_str_persist");         // persist the appended string value before transferring ownership to the hash table
                    emitter.instruction("mov rsi, rax");                        // place the owned string pointer into the hash-append low-payload register
                }
                PhpType::Float => {
                    emitter.instruction("movq rsi, xmm0");                      // move the floating-point payload bits into the hash-append low-payload register
                    emitter.instruction("xor rdx, rdx");                        // float associative-array payloads only use the low payload word
                }
                _ => {
                    emitter.instruction("mov rsi, rax");                        // place the scalar or pointer payload into the hash-append low-payload register
                    emitter.instruction("xor rdx, rdx");                        // scalar or pointer associative-array payloads only use the low payload word
                }
            }
            abi::emit_load_int_immediate(
                emitter,
                "rcx",
                super::super::super::runtime_value_tag(&val_ty) as i64,
            ); // materialize the runtime value tag that describes the appended associative-array payload
            abi::emit_pop_reg(emitter, "rdi");                                 // restore the preserved hash-table pointer into the hash-append receiver argument
            abi::emit_call_label(emitter, "__rt_hash_append");                 // append using PHP's next automatic integer key for hash-backed arrays
        }
    }

    if is_ref {
        abi::load_at_offset(emitter, ref_reg, offset);                         // reload the by-reference slot after the append helper may have reallocated the hash table
        abi::emit_store_to_address(emitter, table_reg, ref_reg, 0);            // store the updated hash-table pointer through the by-reference slot
    } else {
        abi::store_at_offset(emitter, table_reg, offset);                      // save the possibly-reallocated hash-table pointer back into the local slot
    }
}

/// x86_64-specific array-push lowering. Handles by-ref parameters and emits typed push
/// runtime calls (`__rt_array_push_int`, `__rt_array_push_str`, `__rt_array_push_refcounted`).
/// May trigger `__rt_array_to_mixed` if a heterogeneous value is appended to a typed array.
/// Clobbers `rax`, `r11`, `rsi`, `rdi`, `xmm0`.
fn emit_array_push_stmt_linux_x86_64(
    array: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    offset: usize,
    is_ref: bool,
) {
    if is_ref {
        abi::load_at_offset(emitter, "r11", offset);                              // load the by-reference slot that points at the indexed-array local
        abi::emit_load_from_address(emitter, "rax", "r11", 0);                   // dereference the by-reference slot to get the current indexed-array pointer
    } else {
        abi::load_at_offset(emitter, "rax", offset);                              // load the current indexed-array pointer from the local slot
    }
    abi::emit_push_reg(emitter, "rax");                                           // preserve the indexed-array pointer while evaluating the appended value
    let elem_ty = match ctx.variables.get(array) {
        Some(v) => match &v.ty {
            PhpType::Array(t) => *t.clone(),
            _ => PhpType::Int,
        },
        None => PhpType::Int,
    };
    let source_owned = expr_result_heap_ownership(value) == HeapOwnership::Owned;
    let mut val_ty = emit_expr(value, emitter, ctx, data);
    let boxed_iterable =
        crate::codegen::emit_box_iterable_value_for_mixed_container(emitter, &mut val_ty);
    let effective_elem_ty = effective_indexed_push_type(&elem_ty, &val_ty, ctx);
    let converted_to_mixed =
        matches!(effective_elem_ty, PhpType::Mixed) && !matches!(elem_ty, PhpType::Mixed);
    let mut boxed_value_for_mixed = false;
    if matches!(effective_elem_ty, PhpType::Mixed)
        && !matches!(val_ty, PhpType::Mixed | PhpType::Union(_))
    {
        crate::codegen::emit_box_current_expr_value_as_mixed_for_container(
            emitter, value, &val_ty,
        );
        val_ty = PhpType::Mixed;
        boxed_value_for_mixed = true;
    } else if matches!(effective_elem_ty, PhpType::Mixed) && matches!(val_ty, PhpType::Union(_)) {
        val_ty = PhpType::Mixed;
    }
    update_callable_array_metadata(array, value, &val_ty, ctx);
    let release_after_refcounted_push = boxed_value_for_mixed
        || boxed_iterable
        || (source_owned
            && matches!(
                val_ty,
                PhpType::Mixed
                    | PhpType::Union(_)
                    | PhpType::Array(_)
                    | PhpType::AssocArray { .. }
                    | PhpType::Object(_)
            ));
    abi::emit_pop_reg(emitter, "r11");                                            // restore the indexed-array pointer after evaluating the appended value
    if elem_ty != effective_elem_ty {
        let updated_ty = PhpType::Array(Box::new(effective_elem_ty.clone()));
        ctx.update_var_type_and_ownership(
            array,
            updated_ty.clone(),
            super::super::helpers::local_slot_ownership_after_store(&updated_ty),
        );
    }
    if converted_to_mixed {
        abi::emit_push_reg(emitter, "rax");                                      // preserve the boxed appended value across mixed-array conversion
        emitter.instruction("mov rdi, r11");                                    // pass the current indexed-array pointer to the mixed conversion helper
        let conversion_source_ty = if matches!(elem_ty, PhpType::Never) {
            &effective_elem_ty
        } else {
            &elem_ty
        };
        abi::emit_load_int_immediate(
            emitter,
            "rsi",
            super::super::helpers::indexed_array_runtime_value_tag(conversion_source_ty),
        );
        abi::emit_call_label(emitter, "__rt_array_to_mixed");                   // box existing typed slots before appending a heterogeneous value
        emitter.instruction("mov r11, rax");                                    // keep the converted indexed-array pointer as the append receiver
        abi::emit_pop_reg(emitter, "rax");                                      // restore the boxed appended value after conversion
    }
    match &val_ty {
        PhpType::Int | PhpType::Bool => {
            emitter.instruction("mov rsi, rax");                                // place the appended scalar payload in the x86_64 runtime value register
            emitter.instruction("mov rdi, r11");                                // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_int");                 // append the scalar payload and return the possibly-grown indexed-array pointer
        }
        PhpType::Float => {
            emitter.instruction("movq rsi, xmm0");                              // move the floating-point payload bits into the scalar append register
            emitter.instruction("mov rdi, r11");                                // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_int");                 // append the floating-point payload bits as an 8-byte scalar slot
        }
        PhpType::Str => {
            emitter.instruction("mov rsi, rax");                                // place the appended string pointer in the x86_64 runtime payload register
            emitter.instruction("mov rdi, r11");                                // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_str");                 // persist and append the string payload, returning the possibly-grown indexed-array pointer
        }
        PhpType::Callable => {
            emitter.instruction("mov rsi, rax");                                // place the callable descriptor pointer bits in the x86_64 scalar append register
            emitter.instruction("mov rdi, r11");                                // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_int");                 // append the callable descriptor pointer bits as a plain scalar slot
        }
        PhpType::Mixed | PhpType::Union(_) | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            if release_after_refcounted_push {
                abi::emit_push_reg(emitter, "rax");
            }
            emitter.instruction("mov rsi, rax");                                // place the retained refcounted payload pointer in the x86_64 runtime child register
            emitter.instruction("mov rdi, r11");                                // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_refcounted");          // append the retained heap payload and stamp the indexed-array value_type metadata
            if release_after_refcounted_push {
                crate::codegen::emit_release_pushed_refcounted_temp_after_array_push(emitter, &val_ty);
            }
        }
        _ => {}
    }
    if is_ref {
        abi::load_at_offset(emitter, "r11", offset);                              // reload the by-reference slot after the append helper may have reallocated the indexed array
        abi::emit_store_to_address(emitter, "rax", "r11", 0);                   // store the updated indexed-array pointer through the by-reference slot
    } else {
        abi::store_at_offset(emitter, "rax", offset);                              // save the possibly-grown indexed-array pointer back into the local slot
    }
}

/// Determines the effective element type of an indexed-array push. Returns `Mixed` when
/// the existing type and value type are incompatible, or when either is already `Mixed`
/// or a `Union`. Otherwise preserves the existing or value type, with a common object
/// type selected when both are objects.
fn effective_indexed_push_type(existing: &PhpType, value: &PhpType, ctx: &Context) -> PhpType {
    if matches!(existing, PhpType::Never) {
        return if matches!(value, PhpType::Union(_)) {
            PhpType::Mixed
        } else {
            value.clone()
        };
    }
    if matches!(value, PhpType::Never) {
        return existing.clone();
    }
    if matches!(existing, PhpType::Mixed) || matches!(value, PhpType::Mixed | PhpType::Union(_)) {
        PhpType::Mixed
    } else if existing == value {
        existing.clone()
    } else if let (PhpType::Object(left), PhpType::Object(right)) = (existing, value) {
        ctx.common_object_type(left, right).unwrap_or(PhpType::Mixed)
    } else {
        PhpType::Mixed
    }
}

/// Updates callable array metadata when writing a value into a named array.
///
/// Propagates closure signatures, captures, first-class targets, wrapper labels,
/// and runtime descriptor markers from closures, first-class callables, variables,
/// array-access sources, and callable-returning expressions. Non-callable writes
/// clear the array-level metadata because later element loads are no longer
/// guaranteed to share one callable contract.
pub(super) fn update_callable_array_metadata(
    array: &str,
    value: &Expr,
    val_ty: &PhpType,
    ctx: &mut Context,
) {
    if val_ty != &PhpType::Callable {
        clear_callable_array_metadata(array, ctx);
        return;
    }
    match &value.kind {
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_) => {
            if let Some(deferred) = ctx.deferred_closures.last() {
                ctx.closure_sigs
                    .insert(array.to_string(), deferred.sig.clone());
                if deferred.captures.is_empty() {
                    ctx.closure_captures.remove(array);
                } else {
                    ctx.closure_captures
                        .insert(array.to_string(), deferred.captures.clone());
                }
            }
            ctx.first_class_callable_targets.remove(array);
            ctx.variable_fcc_label.remove(array);
        }
        ExprKind::Variable(src_name) => copy_callable_metadata(array, src_name, ctx),
        ExprKind::ArrayAccess { array: source, .. } => {
            if let ExprKind::Variable(src_name) = &source.kind {
                copy_callable_metadata(array, src_name, ctx);
            } else {
                clear_callable_array_metadata(array, ctx);
            }
        }
        _ => {
            if let Some(sig) = crate::codegen::callables::callable_sig(value, ctx) {
                ctx.closure_sigs.insert(array.to_string(), sig);
                ctx.closure_captures.remove(array);
                ctx.first_class_callable_targets.remove(array);
                ctx.variable_fcc_label.remove(array);
                ctx.runtime_callable_vars.insert(array.to_string());
            } else {
                clear_callable_array_metadata(array, ctx);
            }
        }
    }
}

/// Copies callable array metadata from `src` to `dest` in the context.
///
/// Appending is cumulative for runtime descriptor markers: once an array can contain
/// a descriptor-owned callable, later element loads must keep invoking through that
/// descriptor even if a subsequent appended callable has a direct static wrapper.
fn copy_callable_metadata(dest: &str, src: &str, ctx: &mut Context) {
    if let Some(sig) = ctx.closure_sigs.get(src).cloned() {
        ctx.closure_sigs.insert(dest.to_string(), sig);
    } else {
        ctx.closure_sigs.remove(dest);
    }
    if let Some(captures) = ctx.closure_captures.get(src).cloned() {
        ctx.closure_captures.insert(dest.to_string(), captures);
    } else {
        ctx.closure_captures.remove(dest);
    }
    if let Some(target) = ctx.first_class_callable_targets.get(src).cloned() {
        ctx.first_class_callable_targets
            .insert(dest.to_string(), target);
    } else {
        ctx.first_class_callable_targets.remove(dest);
    }
    if let Some(label) = ctx.variable_fcc_label.get(src).cloned() {
        ctx.variable_fcc_label.insert(dest.to_string(), label);
    } else {
        ctx.variable_fcc_label.remove(dest);
    }
    if ctx.runtime_callable_vars.contains(src) {
        ctx.runtime_callable_vars.insert(dest.to_string());
    }
}

/// Clears all callable array metadata entries for a named array variable.
fn clear_callable_array_metadata(array: &str, ctx: &mut Context) {
    ctx.closure_sigs.remove(array);
    ctx.closure_captures.remove(array);
    ctx.first_class_callable_targets.remove(array);
    ctx.variable_fcc_label.remove(array);
    ctx.runtime_callable_vars.remove(array);
}
