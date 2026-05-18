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
use crate::types::PhpType;

pub(super) fn emit_foreach_stmt(
    array: &Expr,
    key_var: &Option<String>,
    value_var: &str,
    value_by_ref: bool,
    body: &[Stmt],
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
            assoc::emit_assoc_foreach(
                key_var,
                value_var,
                value_by_ref,
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
            indexed::emit_indexed_foreach_runtime_mixed(
                key_var,
                value_var,
                value_by_ref,
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
        _ => {
            let elem_ty = match &arr_ty {
                PhpType::Array(t) => *t.clone(),
                _ => PhpType::Int,
            };
            indexed::emit_indexed_foreach(
                key_var,
                value_var,
                value_by_ref,
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
}

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
