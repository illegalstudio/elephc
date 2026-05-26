//! Purpose:
//! Dispatches foreach lowering for indexed arrays, associative arrays, runtime Mixed iterables, and Iterator objects.
//! Prepares iterable values, loop labels, and key/value storage before body emission.
//!
//! Called from:
//! - `crate::codegen::stmt::control_flow`
//!
//! Key details:
//! - Iterable temporaries must remain live for the loop and be released after all exit paths.

mod assoc;
mod indexed;
mod iterator;

use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind, Stmt};
use crate::span::Span;
use crate::types::PhpType;

pub(crate) use iterator::{
    emit_iterable_object_loop, emit_iterator_loop, reload_iterator_receiver, IteratorDispatchTarget,
};

/// Holds the original type of a value variable before it was converted to a by-reference
/// binding in a foreach loop. Used to restore the correct type after the loop completes.
pub(super) struct ForeachRefFallback {
    value_ty: PhpType,
}

/// Lowers a `foreach` statement into target assembly. Dispatches to the appropriate
/// lowering path based on the resolved type of the iterable expression: indexed arrays,
/// associative arrays, runtime `Mixed` iterables, or `Iterator`/`IteratorAggregate` objects.
///
/// Creates loop labels (`foreach_start`, `foreach_end`, `foreach_cont`) and emits the
/// complete foreach control flow including initialization, iteration, and cleanup.
/// Handles by-reference bindings (`$v => &$ref`) by preparing fallback cells and tracking
/// ref-cell flags on the stack.
///
/// # Arguments
/// * `array` - the iterable expression to traverse
/// * `key_var` - optional key variable name (`$k` in `foreach ($arr as $k => $v)`)
/// * `value_var` - value variable name (`$v` in `foreach ($arr as $v)`)
/// * `value_by_ref` - true if the value is bound by reference (`&$v`)
/// * `body` - the loop body statements
/// * `span` - source location for label naming and diagnostics
/// * `emitter` - the assembly emitter
/// * `ctx` - codegen context with variable layout and ref tracking
/// * `data` - runtime data section for literals and metadata
pub(super) fn emit_foreach_stmt(
    array: &Expr,
    key_var: &Option<String>,
    value_var: &str,
    value_by_ref: bool,
    body: &[Stmt],
    span: Span,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let loop_start = ctx.next_label("foreach_start");
    let loop_end = ctx.next_label("foreach_end");
    let loop_cont = ctx.next_label("foreach_cont");

    emitter.blank();
    emitter.comment("foreach");

    let receiver_var = match &array.kind {
        ExprKind::Variable(name) => Some(name.as_str()),
        _ => None,
    };
    let value_was_ref = ctx.ref_params.contains(value_var);
    let local_ref_cell_flag_key = if value_by_ref && !value_was_ref {
        Some(Context::foreach_local_ref_cell_flag_key(value_var, span))
    } else {
        None
    };
    let arr_ty = emit_expr(array, emitter, ctx, data);
    if value_by_ref {
        ensure_unique_by_ref_source(receiver_var, &arr_ty, emitter, ctx);
    }

    match &arr_ty {
        PhpType::AssocArray { key, value } => {
            assoc::emit_assoc_foreach(
                key_var,
                value_var,
                value_by_ref,
                value_was_ref,
                local_ref_cell_flag_key.as_deref(),
                body,
                &loop_start,
                &loop_end,
                &loop_cont,
                &*key.clone(),
                &*value.clone(),
                emitter,
                ctx,
                data,
            );
        }
        PhpType::Object(class_name) => {
            debug_assert!(
                !value_by_ref,
                "type checker must reject by-reference foreach over Iterator/IteratorAggregate objects"
            );
            iterator::emit_iterator_foreach(
                class_name,
                receiver_var,
                key_var,
                value_var,
                body,
                &loop_start,
                &loop_end,
                &loop_cont,
                emitter,
                ctx,
                data,
            );
        }
        PhpType::Iterable => {
            // Iterable values are type-erased raw heap pointers. Dispatch on the
            // runtime heap kind: indexed arrays and hashes each use their native
            // layout, but both expose Mixed-typed values at the PHP `iterable`
            // boundary.
            let indexed_case = ctx.next_label("foreach_iter_indexed");
            let hash_case = ctx.next_label("foreach_iter_hash");
            let object_case = ctx.next_label("foreach_iter_object");
            let done = ctx.next_label("foreach_iter_done");
            let indexed_start = ctx.next_label("foreach_iter_indexed_start");
            let indexed_end = ctx.next_label("foreach_iter_indexed_end");
            let indexed_cont = ctx.next_label("foreach_iter_indexed_cont");
            let hash_start = ctx.next_label("foreach_iter_hash_start");
            let hash_end = ctx.next_label("foreach_iter_hash_end");
            let hash_cont = ctx.next_label("foreach_iter_hash_cont");

            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // preserve iterable pointer across heap-kind probe
            abi::emit_call_label(emitter, "__rt_heap_kind");                     // x0/rax = heap kind tag for the iterable payload
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("cmp x0, #2");                          // indexed-array kind?
                    emitter.instruction(&format!("b.eq {}", indexed_case));     // dispatch the indexed iterable path
                    emitter.instruction("cmp x0, #3");                          // hash table kind?
                    emitter.instruction(&format!("b.eq {}", hash_case));        // dispatch the associative iterable path
                    emitter.instruction("cmp x0, #4");                          // object kind?
                    emitter.instruction(&format!("b.eq {}", object_case));      // dispatch the Traversable object iterable path
                }
                Arch::X86_64 => {
                    emitter.instruction("cmp rax, 2");                          // indexed-array kind?
                    emitter.instruction(&format!("je {}", indexed_case));       // dispatch the indexed iterable path
                    emitter.instruction("cmp rax, 3");                          // hash table kind?
                    emitter.instruction(&format!("je {}", hash_case));          // dispatch the associative iterable path
                    emitter.instruction("cmp rax, 4");                          // object kind?
                    emitter.instruction(&format!("je {}", object_case));        // dispatch the Traversable object iterable path
                }
            }
            abi::emit_call_label(emitter, "__rt_iterable_unsupported_kind");    // unsupported iterable kind aborts with a fatal diagnostic

            emitter.label(&object_case);
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));           // restore iterable pointer for the object foreach prologue
            iterator::emit_iterable_object_foreach(
                receiver_var,
                key_var,
                value_var,
                body,
                emitter,
                ctx,
                data,
            );
            abi::emit_jump(emitter, &done);                                      // skip array iterable branches after object iteration completes

            emitter.label(&hash_case);
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));           // restore iterable pointer for the assoc foreach prologue

            assoc::emit_assoc_foreach(
                key_var,
                value_var,
                value_by_ref,
                value_was_ref,
                local_ref_cell_flag_key.as_deref(),
                body,
                &hash_start,
                &hash_end,
                &hash_cont,
                &PhpType::Mixed,
                &PhpType::Mixed,
                emitter,
                ctx,
                data,
            );
            abi::emit_jump(emitter, &done);                                      // skip the indexed foreach body after the hash path completes

            emitter.label(&indexed_case);
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));           // restore iterable pointer for the indexed foreach prologue
            indexed::emit_indexed_foreach_runtime_mixed(
                key_var,
                value_var,
                value_by_ref,
                value_was_ref,
                local_ref_cell_flag_key.as_deref(),
                body,
                &indexed_start,
                &indexed_end,
                &indexed_cont,
                emitter,
                ctx,
                data,
            );
            emitter.label(&done);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            // The foreach source expression may be compiled with the final
            // loop-variable type when the value/key variable reuses the source
            // name. In that case the slot contains a boxed mixed value, so
            // dispatch on the concrete payload tag before choosing a foreach
            // lowering path.
            let indexed_case = ctx.next_label("foreach_mixed_indexed");
            let hash_case = ctx.next_label("foreach_mixed_hash");
            let object_case = ctx.next_label("foreach_mixed_object");
            let done = ctx.next_label("foreach_mixed_done");
            let indexed_start = ctx.next_label("foreach_mixed_indexed_start");
            let indexed_end = ctx.next_label("foreach_mixed_indexed_end");
            let indexed_cont = ctx.next_label("foreach_mixed_indexed_cont");
            let hash_start = ctx.next_label("foreach_mixed_hash_start");
            let hash_end = ctx.next_label("foreach_mixed_hash_end");
            let hash_cont = ctx.next_label("foreach_mixed_hash_cont");

            if value_by_ref {
                push_mixed_source_cell(emitter);
            }
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                 // unwrap the mixed foreach source into tag plus payload words
            push_mixed_payload_lo(emitter);
            branch_on_mixed_iterable_tag(
                &indexed_case,
                &hash_case,
                &object_case,
                emitter,
            );
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));           // discard the unsupported mixed payload before raising the foreach diagnostic
            abi::emit_call_label(emitter, "__rt_iterable_unsupported_kind");    // non-iterable mixed payloads abort with a fatal diagnostic

            emitter.label(&object_case);
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));           // restore the unboxed object pointer for Traversable foreach dispatch
            if value_by_ref {
                abi::emit_call_label(
                    emitter,
                    "__rt_iterable_unsupported_kind",
                );                                                              // reject by-reference foreach over runtime object iterables
            }
            iterator::emit_iterable_object_foreach(
                receiver_var,
                key_var,
                value_var,
                body,
                emitter,
                ctx,
                data,
            );
            abi::emit_jump(emitter, &done);                                     // skip array payload foreach branches after object iteration completes

            emitter.label(&hash_case);
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));           // restore the unboxed hash pointer for associative foreach dispatch
            if value_by_ref {
                convert_runtime_mixed_hash_by_ref_source(emitter);
            }
            assoc::emit_assoc_foreach(
                key_var,
                value_var,
                value_by_ref,
                value_was_ref,
                local_ref_cell_flag_key.as_deref(),
                body,
                &hash_start,
                &hash_end,
                &hash_cont,
                &PhpType::Mixed,
                &PhpType::Mixed,
                emitter,
                ctx,
                data,
            );
            abi::emit_jump(emitter, &done);                                     // skip the indexed payload foreach branch after hash iteration completes

            emitter.label(&indexed_case);
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));           // restore the unboxed indexed-array pointer for foreach dispatch
            if value_by_ref {
                convert_runtime_mixed_indexed_by_ref_source(emitter);
            }
            indexed::emit_indexed_foreach_runtime_mixed(
                key_var,
                value_var,
                value_by_ref,
                value_was_ref,
                local_ref_cell_flag_key.as_deref(),
                body,
                &indexed_start,
                &indexed_end,
                &indexed_cont,
                emitter,
                ctx,
                data,
            );
            emitter.label(&done);
            if value_by_ref {
                discard_mixed_source_cell(emitter);
            }
        }
        _ => {
            let elem_ty = match &arr_ty {
                PhpType::Array(t) => *t.clone(),
                _ => PhpType::Int,
            };
            indexed::emit_indexed_foreach(
                key_var,
                value_var,
                value_by_ref,
                value_was_ref,
                local_ref_cell_flag_key.as_deref(),
                body,
                &loop_start,
                &loop_end,
                &loop_cont,
                &elem_ty,
                emitter,
                ctx,
                data,
            );
        }
    }

    if value_by_ref {
        ctx.ref_params.insert(value_var.to_string());
    }
}

/// Pushes the low word of an unboxed Mixed payload onto the stack to preserve it
/// across subsequent tag testing and dispatch. On AArch64 this is x1; on x86_64 this is rdi.
fn push_mixed_payload_lo(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(emitter, "x1");                                  // preserve the unboxed mixed payload pointer while testing its runtime tag
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rdi");                                 // preserve the unboxed mixed payload pointer while testing its runtime tag
        }
    }
}

/// Pushes the boxed Mixed source cell onto the stack to preserve it while iterating
/// over by-reference array payloads. The cell is discarded after the array foreach completes.
fn push_mixed_source_cell(emitter: &mut Emitter) {
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the boxed Mixed source cell for by-reference array payload updates
}

/// Pops and discards the boxed Mixed source cell from the stack after a by-reference
/// array foreach completes. Uses the target's temporary integer register for the pop.
fn discard_mixed_source_cell(emitter: &mut Emitter) {
    abi::emit_pop_reg(emitter, abi::temp_int_reg(emitter.target));              // discard the preserved boxed Mixed source cell after array foreach completes
}

/// Converts a runtime unboxed indexed-array payload to boxed Mixed cells in-place,
/// then publishes the unique indexed-array pointer into the preserved Mixed source cell.
/// Called when a by-reference foreach iterates over a Mixed that contains an indexed array.
fn convert_runtime_mixed_indexed_by_ref_source(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x1, [x0, #-8]");                           // load the packed indexed-array metadata before mixed-slot conversion
            emitter.instruction("lsr x1, x1, #8");                              // move the runtime indexed-array value_type tag into the low bits
            emitter.instruction("and x1, x1, #0x7f");                           // isolate the current indexed-array value_type tag
            abi::emit_call_label(emitter, "__rt_array_to_mixed");              // convert indexed-array slots to boxed Mixed cells before binding refs
            emitter.instruction("ldr x9, [sp]");                                // reload the preserved boxed Mixed source cell
            emitter.instruction("str x0, [x9, #8]");                            // publish the unique converted indexed-array pointer into the Mixed cell
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, QWORD PTR [rax - 8]");                // load the packed indexed-array metadata before mixed-slot conversion
            emitter.instruction("shr rsi, 8");                                  // move the runtime indexed-array value_type tag into the low bits
            emitter.instruction("and rsi, 0x7f");                               // isolate the current indexed-array value_type tag
            emitter.instruction("mov rdi, rax");                                // pass the indexed-array pointer to the mixed-slot conversion helper
            abi::emit_call_label(emitter, "__rt_array_to_mixed");              // convert indexed-array slots to boxed Mixed cells before binding refs
            emitter.instruction("mov r10, QWORD PTR [rsp]");                    // reload the preserved boxed Mixed source cell
            emitter.instruction("mov QWORD PTR [r10 + 8], rax");                // publish the unique converted indexed-array pointer into the Mixed cell
        }
    }
}

/// Converts a runtime unboxed hash payload to boxed Mixed cells in-place, then publishes
/// the unique hash pointer into the preserved Mixed source cell. Called when a by-reference
/// foreach iterates over a Mixed that contains an associative array.
fn convert_runtime_mixed_hash_by_ref_source(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_call_label(emitter, "__rt_hash_to_mixed");               // convert hash entries to boxed Mixed cells before binding refs
            emitter.instruction("ldr x9, [sp]");                                // reload the preserved boxed Mixed source cell
            emitter.instruction("str x0, [x9, #8]");                            // publish the unique converted hash pointer into the Mixed cell
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // pass the hash pointer to the mixed-entry conversion helper
            abi::emit_call_label(emitter, "__rt_hash_to_mixed");               // convert hash entries to boxed Mixed cells before binding refs
            emitter.instruction("mov r10, QWORD PTR [rsp]");                    // reload the preserved boxed Mixed source cell
            emitter.instruction("mov QWORD PTR [r10 + 8], rax");                // publish the unique converted hash pointer into the Mixed cell
        }
    }
}

/// Ensures the foreach source array/hash is uniquely owned before by-reference binding.
/// Calls `__rt_array_ensure_unique` or `__rt_hash_ensure_unique` depending on the type.
/// If the source variable is itself a referenced parameter, global, or local variable,
/// stores the unique source pointer through the appropriate binding mechanism.
fn ensure_unique_by_ref_source(
    receiver_var: Option<&str>,
    arr_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let helper = match arr_ty {
        PhpType::Array(_) => "__rt_array_ensure_unique",
        PhpType::AssocArray { .. } => "__rt_hash_ensure_unique",
        _ => return,
    };

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("bl {}", helper));                     // split shared foreach source before binding by-reference values
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // pass the foreach source pointer to the uniqueness helper
            emitter.instruction(&format!("call {}", helper));                   // split shared foreach source before binding by-reference values
        }
    }

    let Some(name) = receiver_var else {
        return;
    };
    if ctx.ref_params.contains(name) {
        let Some(var) = ctx.variables.get(name) else {
            return;
        };
        let pointer_reg = abi::symbol_scratch_reg(emitter);
        abi::load_at_offset(emitter, pointer_reg, var.stack_offset);            // load referenced foreach source slot address
        abi::emit_store_to_address(
            emitter,
            abi::int_result_reg(emitter),
            pointer_reg,
            0,
        );                                                                      // store the unique source pointer through the reference
        return;
    }
    if ctx.extern_globals.contains_key(name) {
        super::super::emit_extern_global_store(emitter, name, arr_ty);
        return;
    }
    if ctx.global_vars.contains(name) || (ctx.in_main && ctx.all_global_var_names.contains(name)) {
        let label = format!("_gvar_{}", name);
        abi::emit_store_reg_to_symbol(emitter, abi::int_result_reg(emitter), &label, 0);
        return;
    }
    if let Some(var) = ctx.variables.get(name) {
        abi::store_at_offset(emitter, abi::int_result_reg(emitter), var.stack_offset);
        ctx.update_var_type_and_ownership(
            name,
            arr_ty.clone(),
            HeapOwnership::local_owner_for_type(arr_ty),
        );
    }
}

/// Binds the foreach value variable to a by-reference alias target address.
/// Stores the address from `value_addr_reg` into the value variable's stack slot,
/// marks the variable as a ref parameter, and updates its type to a borrowed alias
/// of `value_ty`.
pub(super) fn bind_foreach_value_ref(
    value_var: &str,
    value_addr_reg: &str,
    value_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let Some(var) = ctx.variables.get(value_var) else {
        emitter.comment(&format!("WARNING: undefined foreach value variable ${}", value_var));
        return;
    };
    let offset = var.stack_offset;
    abi::store_at_offset_scratch(emitter, value_addr_reg, offset, abi::temp_int_reg(emitter.target));
    ctx.ref_params.insert(value_var.to_string());
    ctx.update_var_type_static_and_ownership(
        value_var,
        value_ty.codegen_repr(),
        value_ty.clone(),
        HeapOwnership::borrowed_alias_for_type(value_ty),
    );
}

/// Allocates a fallback reference cell for a by-reference foreach value variable
/// that is not already a ref parameter. Copies the current local value into the cell,
/// sets the ref-cell flag to 1, and updates the variable's type to a borrowed alias.
/// Returns `Some(ForeachRefFallback)` with the original value type for later cleanup,
/// or `None` if the variable is not defined or is already a ref parameter.
pub(super) fn prepare_foreach_value_ref_slot(
    value_var: &str,
    value_ty: &PhpType,
    flag_key: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> Option<ForeachRefFallback> {
    if ctx.ref_params.contains(value_var) {
        return None;
    }
    let Some(var) = ctx.variables.get(value_var) else {
        emitter.comment(&format!("WARNING: undefined foreach value variable ${}", value_var));
        return None;
    };
    let slot_offset = var.stack_offset;
    let current_ty = var.ty.clone();
    let current_ownership = var.ownership;

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the foreach source pointer while preparing the value alias cell
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_call_label(emitter, "__rt_heap_alloc");                          // allocate a stable fallback cell for empty by-reference foreach loops
    let cell_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", cell_reg, abi::int_result_reg(emitter))); // keep the fallback reference cell address while copying the current value
    copy_local_value_to_ref_cell(&current_ty, slot_offset, cell_reg, emitter);
    release_owned_local_value_after_ref_cell_copy(
        &current_ty,
        current_ownership,
        slot_offset,
        cell_reg,
        emitter,
    );
    abi::store_at_offset_scratch(
        emitter,
        cell_reg,
        slot_offset,
        abi::temp_int_reg(emitter.target),
    );
    ctx.set_local_ref_cell_flag_type(flag_key, current_ty.clone());
    let Some(flag_offset) = ctx
        .local_ref_cell_flags
        .get(flag_key)
        .map(|flag| flag.offset)
    else {
        emitter.comment(&format!("WARNING: missing foreach ref-cell flag for ${}", value_var));
        abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));               // restore the foreach source pointer after failed fallback flag lookup
        return None;
    };
    let flag_reg = abi::temp_int_reg(emitter.target);
    abi::emit_load_int_immediate(emitter, flag_reg, 1);
    abi::store_at_offset_scratch(
        emitter,
        flag_reg,
        flag_offset,
        abi::secondary_scratch_reg(emitter),
    );
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the foreach source pointer for loop setup

    ctx.ref_params.insert(value_var.to_string());
    ctx.update_var_type_static_and_ownership(
        value_var,
        value_ty.codegen_repr(),
        value_ty.clone(),
        HeapOwnership::borrowed_alias_for_type(value_ty),
    );
    Some(ForeachRefFallback { value_ty: current_ty })
}

/// Releases the original local value after copying it into a by-reference fallback cell.
/// Only releases values that are `Owned` and either `Str` or refcounted; other types
/// are left unchanged since they don't carry heap ownership. Uses `__rt_heap_free_safe`
/// for strings and `emit_decref_if_refcounted` for refcounted heap values.
fn release_owned_local_value_after_ref_cell_copy(
    value_ty: &PhpType,
    ownership: HeapOwnership,
    slot_offset: usize,
    cell_reg: &str,
    emitter: &mut Emitter,
) {
    if ownership != HeapOwnership::Owned {
        return;
    }
    if !matches!(value_ty.codegen_repr(), PhpType::Str) && !value_ty.is_refcounted() {
        return;
    }

    abi::emit_push_reg(emitter, cell_reg);                                      // preserve the fallback reference cell while releasing the replaced local owner
    if matches!(value_ty.codegen_repr(), PhpType::Str) {
        abi::load_at_offset_scratch(
            emitter,
            abi::int_result_reg(emitter),
            slot_offset,
            abi::temp_int_reg(emitter.target),
        );
        abi::emit_call_label(emitter, "__rt_heap_free_safe");                  // release the old local string now that the fallback cell owns a copy
    } else {
        abi::load_at_offset_scratch(
            emitter,
            abi::int_result_reg(emitter),
            slot_offset,
            abi::temp_int_reg(emitter.target),
        );
        abi::emit_decref_if_refcounted(emitter, value_ty);
    }
    abi::emit_pop_reg(emitter, cell_reg);                                       // restore the fallback reference cell for storage in the local slot
}

/// Copies the current local value at `slot_offset` into the fallback reference cell
/// at `cell_reg`. Handles all PHP types: strings (persisted), floats (direct store),
/// refcounted values (retained before store), and primitives (direct store with zero tag).
fn copy_local_value_to_ref_cell(
    value_ty: &PhpType,
    slot_offset: usize,
    cell_reg: &str,
    emitter: &mut Emitter,
) {
    let temp_reg = abi::temp_int_reg(emitter.target);
    match value_ty.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::load_at_offset_scratch(emitter, ptr_reg, slot_offset, temp_reg);
            abi::load_at_offset_scratch(emitter, len_reg, slot_offset - 8, temp_reg);
            abi::emit_push_reg(emitter, cell_reg);                              // preserve the fallback reference cell across string persistence
            abi::emit_call_label(emitter, "__rt_str_persist");                 // detach the preserved local string before storing it in the fallback cell
            abi::emit_pop_reg(emitter, cell_reg);                               // restore the fallback reference cell after string persistence
            abi::emit_store_to_address(emitter, ptr_reg, cell_reg, 0);
            abi::emit_store_to_address(emitter, len_reg, cell_reg, 8);
        }
        PhpType::Float => {
            abi::load_at_offset_scratch(
                emitter,
                abi::float_result_reg(emitter),
                slot_offset,
                temp_reg,
            );
            abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), cell_reg, 0);
            abi::emit_store_zero_to_address(emitter, cell_reg, 8);
        }
        ty if ty.is_refcounted() => {
            abi::load_at_offset_scratch(emitter, abi::int_result_reg(emitter), slot_offset, temp_reg);
            abi::emit_push_reg(emitter, cell_reg);                              // preserve the fallback reference cell across retain
            abi::emit_incref_if_refcounted(emitter, &ty);
            abi::emit_pop_reg(emitter, cell_reg);                               // restore the fallback reference cell after retain
            abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), cell_reg, 0);
            abi::emit_store_zero_to_address(emitter, cell_reg, 8);
        }
        _ => {
            abi::load_at_offset_scratch(emitter, temp_reg, slot_offset, abi::secondary_scratch_reg(emitter));
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 0);
            abi::emit_store_zero_to_address(emitter, cell_reg, 8);
        }
    }
}

/// Stores a foreach iteration value into the value variable's stack slot. When the
/// variable is a ref parameter, stores through the alias pointer loaded from the
/// slot. For strings, stores both the pointer (at offset 0) and length (at offset -8).
/// Uses scratch registers appropriate to the target ABI.
pub(super) fn store_foreach_value_from_regs(
    value_var: &str,
    value_ty: &PhpType,
    low_reg: &str,
    high_reg: Option<&str>,
    direct_offset: usize,
    emitter: &mut Emitter,
    ctx: &Context,
) {
    if ctx.ref_params.contains(value_var) {
        let pointer_reg = abi::symbol_scratch_reg(emitter);
        abi::load_at_offset(emitter, pointer_reg, direct_offset);               // load the current foreach value alias target
        match value_ty {
            PhpType::Str => {
                abi::emit_store_to_address(emitter, low_reg, pointer_reg, 0);
                abi::emit_store_to_address(
                    emitter,
                    high_reg.expect("string foreach value missing length register"),
                    pointer_reg,
                    8,
                );
            }
            PhpType::Float => {
                abi::emit_store_to_address(emitter, low_reg, pointer_reg, 0);
            }
            _ => {
                abi::emit_store_to_address(emitter, low_reg, pointer_reg, 0);
            }
        }
        return;
    }

    match value_ty {
        PhpType::Str => {
            abi::store_at_offset_scratch(emitter, low_reg, direct_offset, abi::temp_int_reg(emitter.target));
            abi::store_at_offset_scratch(
                emitter,
                high_reg.expect("string foreach value missing length register"),
                direct_offset - 8,
                abi::temp_int_reg(emitter.target),
            );
        }
        PhpType::Float => {
            abi::store_at_offset(emitter, low_reg, direct_offset);
        }
        _ => {
            abi::store_at_offset_scratch(emitter, low_reg, direct_offset, abi::temp_int_reg(emitter.target));
        }
    }
}

/// Releases any owned ref cells held by the value variable across multiple foreach sites
/// before rebinding to a new alias. Iterates over all ref-cell flags associated with the
/// variable, skips sites that didn't allocate a cell (flag = 0), loads the old cell from
/// the slot, releases it via `emit_release_local_ref_cell`, and resets the flag to 0.
pub(super) fn release_foreach_value_ref_cell_before_rebind(
    value_var: &str,
    fallback: Option<&ForeachRefFallback>,
    address_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let Some(var) = ctx.variables.get(value_var) else {
        emitter.comment(&format!("WARNING: undefined foreach value variable ${}", value_var));
        return;
    };
    let slot_offset = var.stack_offset;
    let default_value_ty = fallback
        .map(|fallback| fallback.value_ty.clone())
        .unwrap_or_else(|| var.ty.clone());
    let mut flags: Vec<_> = ctx
        .local_ref_cell_flags
        .values()
        .filter(|flag| flag.variable == value_var)
        .map(|flag| {
            (
                flag.offset,
                flag.value_ty
                    .clone()
                    .unwrap_or_else(|| default_value_ty.clone()),
            )
        })
        .collect();
    if flags.is_empty() {
        return;
    }
    flags.sort_by_key(|(offset, _)| *offset);

    abi::emit_push_reg(emitter, address_reg);                                   // preserve the new foreach alias target while releasing any owned local ref cell
    for (idx, (flag_offset, value_ty)) in flags.iter().enumerate() {
        let restore = ctx.next_label(&format!("foreach_ref_cell_restore_{}", idx));
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::load_at_offset_scratch(emitter, "x10", *flag_offset, "x11");
                emitter.instruction(&format!("cbz x10, {}", restore));          // skip release when this site did not leave an owned local ref cell
                abi::load_at_offset_scratch(emitter, "x9", slot_offset, "x11");
                abi::emit_release_local_ref_cell(emitter, "x9", value_ty);
                abi::emit_store_zero_to_local_slot(emitter, *flag_offset);      // record that this foreach site no longer owns a local ref cell
            }
            Arch::X86_64 => {
                abi::load_at_offset_scratch(emitter, "r10", *flag_offset, "r11");
                emitter.instruction(&format!("test r10, r10"));                 // check whether this foreach site owns a local reference cell
                emitter.instruction(&format!("je {}", restore));                // skip release when this site did not leave an owned local ref cell
                abi::load_at_offset_scratch(emitter, "r11", slot_offset, "r10");
                abi::emit_release_local_ref_cell(emitter, "r11", value_ty);
                abi::emit_store_zero_to_local_slot(emitter, *flag_offset);      // record that this foreach site no longer owns a local ref cell
            }
        }
        emitter.label(&restore);
    }
    abi::emit_pop_reg(emitter, address_reg);                                    // restore the new foreach alias target for binding
}

/// Writes a bound flag (value 1) to the ref-cell flag offset on the stack to record
/// that this by-reference foreach site successfully bound at least one element.
/// Used by loop cleanup to determine whether the fallback cell must be released.
pub(super) fn mark_foreach_value_ref_bound(flag_offset: usize, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x10, #1");                                 // mark that this by-reference foreach bound at least one element
            emitter.instruction(&format!("str x10, [sp, #{}]", flag_offset));   // persist the by-reference foreach bound flag for loop cleanup
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 1", flag_offset)); // mark that this by-reference foreach bound at least one element
        }
    }
}

/// Finalizes a by-reference foreach value variable after loop completion. Inserts
/// the variable into `ref_params` and updates its type to a borrowed alias of `value_ty`.
/// The `_value_was_ref`, `_flag_offset`, and `_saved_ref_offset` parameters are unused
/// but kept for API compatibility.
pub(super) fn finish_foreach_value_ref(
    value_var: &str,
    value_ty: &PhpType,
    _value_was_ref: bool,
    _flag_offset: usize,
    _saved_ref_offset: Option<usize>,
    _emitter: &mut Emitter,
    ctx: &mut Context,
) {
    ctx.ref_params.insert(value_var.to_string());
    ctx.update_var_type_static_and_ownership(
        value_var,
        value_ty.codegen_repr(),
        value_ty.clone(),
        HeapOwnership::borrowed_alias_for_type(value_ty),
    );
}

/// Emits conditional jumps on the unboxed Mixed tag in `x0`/`rax` to dispatch
/// indexed-array (tag 4), hash (tag 5), and object (tag 6) payloads to their
/// respective foreach lowering paths. Non-iterable tags fall through to a fatal
/// diagnostic.
fn branch_on_mixed_iterable_tag(
    indexed_case: &str,
    hash_case: &str,
    object_case: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #4");                                  // mixed tag 4 = indexed array
            emitter.instruction(&format!("b.eq {}", indexed_case));             // dispatch indexed-array mixed payloads to indexed foreach
            emitter.instruction("cmp x0, #5");                                  // mixed tag 5 = associative array
            emitter.instruction(&format!("b.eq {}", hash_case));                // dispatch hash mixed payloads to associative foreach
            emitter.instruction("cmp x0, #6");                                  // mixed tag 6 = object
            emitter.instruction(&format!("b.eq {}", object_case));              // dispatch object mixed payloads to Traversable foreach
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 4");                                  // mixed tag 4 = indexed array
            emitter.instruction(&format!("je {}", indexed_case));               // dispatch indexed-array mixed payloads to indexed foreach
            emitter.instruction("cmp rax, 5");                                  // mixed tag 5 = associative array
            emitter.instruction(&format!("je {}", hash_case));                  // dispatch hash mixed payloads to associative foreach
            emitter.instruction("cmp rax, 6");                                  // mixed tag 6 = object
            emitter.instruction(&format!("je {}", object_case));                // dispatch object mixed payloads to Traversable foreach
        }
    }
}
