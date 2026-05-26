//! Purpose:
//! Emits PHP `iterator_to_array()` calls for arrays and Iterator/IteratorAggregate objects.
//! Reuses the statement foreach iterator driver for object traversal.
//!
//! Called from:
//! - `crate::codegen::builtins::spl::emit()`
//!
//! Key details:
//! - `$preserve_keys=false` appends current() values without calling key().
//! - `$preserve_keys=true` normalizes key() results through the associative-array hash ABI.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::coerce_to_truthiness;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::codegen::stmt::{emit_iterable_object_loop, emit_iterator_loop};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::iterator_common::{self, PreserveKeysArg};

/// Emits the iterator to array entry point for this module.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("iterator_to_array()");
    let preserve_keys = iterator_common::preserve_keys_arg(args);
    let source_ty = emit_expr(&args[0], emitter, ctx, data);

    if matches!(preserve_keys, PreserveKeysArg::Dynamic) {
        if let Some(arg) = args.get(1) {
            return emit_dynamic_preserve_keys(&source_ty, arg, emitter, ctx, data);
        }
    }

    let PreserveKeysArg::Static(preserve_keys) = preserve_keys else {
        unreachable!("dynamic preserve_keys requires a second argument")
    };
    Some(emit_to_array_loaded_source(
        &source_ty,
        preserve_keys,
        emitter,
        ctx,
        data,
    ))
}

/// Emits assembly for to array loaded source.
fn emit_to_array_loaded_source(
    source_ty: &PhpType,
    preserve_keys: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if preserve_keys {
        if let Some(cloned_ty) = iterator_common::emit_clone_loaded_array(source_ty, emitter) {
            return cloned_ty;
        }
    } else {
        match source_ty.codegen_repr() {
            PhpType::Array(_) => {
                if let Some(cloned_ty) = iterator_common::emit_clone_loaded_array(source_ty, emitter) {
                    return cloned_ty;
                }
            }
            PhpType::AssocArray { .. } => {
                return crate::codegen::builtins::arrays::array_values::emit_loaded_values(
                    source_ty,
                    emitter,
                    ctx,
                    data,
                )
                .unwrap_or_else(|| static_result_ty(source_ty, preserve_keys));
            }
            _ => {}
        }
    }

    if matches!(source_ty.codegen_repr(), PhpType::Iterable) {
        emit_to_array_loaded_iterable(preserve_keys, emitter, ctx, data);
        return static_result_ty(source_ty, preserve_keys);
    }

    let Some(class_name) = iterator_common::iterator_object_name(&source_ty) else {
        return static_result_ty(source_ty, preserve_keys);
    };

    if class_name == "Traversable" {
        emit_to_array_loaded_traversable_object(preserve_keys, emitter, ctx, data);
        return static_result_ty(source_ty, preserve_keys);
    }

    emit_to_array_loaded_iterator_object(class_name, preserve_keys, emitter, ctx, data);
    static_result_ty(source_ty, preserve_keys)
}

/// Emits assembly for dynamic preserve keys.
fn emit_dynamic_preserve_keys(
    source_ty: &PhpType,
    preserve_arg: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    if matches!(source_ty.codegen_repr(), PhpType::Array(_)) {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve iterator_to_array() source while evaluating dynamic preserve_keys
        let preserve_ty = emit_expr(preserve_arg, emitter, ctx, data);
        coerce_to_truthiness(emitter, ctx, &preserve_ty);
        abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));               // restore indexed-array source; preserve_keys does not change indexed shape
        return Some(emit_to_array_loaded_source(
            source_ty,
            true,
            emitter,
            ctx,
            data,
        ));
    }

    let false_case = ctx.next_label("iterator_to_array_preserve_false");
    let done = ctx.next_label("iterator_to_array_preserve_done");

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve iterator_to_array() source while evaluating dynamic preserve_keys
    let preserve_ty = emit_expr(preserve_arg, emitter, ctx, data);
    coerce_to_truthiness(emitter, ctx, &preserve_ty);
    abi::emit_branch_if_int_result_zero(emitter, &false_case);

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore source for the preserve_keys=true collection path
    let true_ty = emit_to_array_loaded_source(source_ty, true, emitter, ctx, data);
    emit_box_owned_result_as_mixed(&true_ty, emitter);
    abi::emit_jump(emitter, &done);                                             // skip preserve_keys=false path after producing the boxed result

    emitter.label(&false_case);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore source for the preserve_keys=false collection path
    let false_ty = emit_to_array_loaded_source(source_ty, false, emitter, ctx, data);
    emit_box_owned_result_as_mixed(&false_ty, emitter);

    emitter.label(&done);
    Some(dynamic_result_ty(source_ty))
}

/// Emits assembly for to array loaded iterator object.
fn emit_to_array_loaded_iterator_object(
    class_name: &str,
    preserve_keys: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let receiver_reg = abi::nested_call_reg(emitter);
    emitter.instruction(&format!(
        "mov {}, {}",
        receiver_reg,
        abi::int_result_reg(emitter)
    )); // preserve iterator receiver while allocating iterator_to_array()'s result
    if preserve_keys {
        iterator_common::emit_new_mixed_hash(emitter);
    } else {
        iterator_common::emit_new_mixed_indexed_array(emitter);
    }
    iterator_common::emit_save_result_under_receiver(emitter);
    iterator_common::emit_restore_receiver_from_preserved_reg(emitter, receiver_reg);

    let loop_start = ctx.next_label("iterator_to_array_start");
    let loop_end = ctx.next_label("iterator_to_array_end");
    let loop_cont = ctx.next_label("iterator_to_array_cont");
    emit_iterator_loop(
        class_name,
        &loop_start,
        &loop_end,
        &loop_cont,
        emitter,
        ctx,
        data,
        |_, _, _, _| (),
        |dispatch_target, emitter, ctx, data| {
            if preserve_keys {
                iterator_common::emit_insert_current_with_iterator_key(
                    dispatch_target,
                    emitter,
                    ctx,
                    data,
                );
            } else {
                iterator_common::emit_append_current_to_saved_array(
                    dispatch_target,
                    emitter,
                    ctx,
                );
            }
        },
        |_, _, _, _| {},
    );
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // return the completed iterator_to_array() result container
}

/// Provides the Static result ty helper used by the iterator to array module.
fn static_result_ty(source_ty: &PhpType, preserve_keys: bool) -> PhpType {
    match source_ty.codegen_repr() {
        PhpType::Array(elem_ty) => PhpType::Array(elem_ty),
        PhpType::AssocArray { key, value } if preserve_keys => PhpType::AssocArray { key, value },
        PhpType::AssocArray { value, .. } => PhpType::Array(value),
        _ if preserve_keys => PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        },
        _ => PhpType::Array(Box::new(PhpType::Mixed)),
    }
}

/// Provides the Dynamic result ty helper used by the iterator to array module.
fn dynamic_result_ty(source_ty: &PhpType) -> PhpType {
    merge_result_types(
        static_result_ty(source_ty, true),
        static_result_ty(source_ty, false),
    )
}

/// Provides the Merge result types helper used by the iterator to array module.
fn merge_result_types(a: PhpType, b: PhpType) -> PhpType {
    if a == b {
        a
    } else {
        PhpType::Union(vec![a, b])
    }
}

/// Emits assembly for box owned result as mixed.
fn emit_box_owned_result_as_mixed(result_ty: &PhpType, emitter: &mut Emitter) {
    let result_ty = result_ty.codegen_repr();
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve the owned iterator_to_array() result while boxing it
            crate::codegen::emit_box_current_value_as_mixed(emitter, &result_ty);
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve the boxed mixed result while releasing the original owner
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the original iterator_to_array() result retained by the mixed box
            abi::emit_decref_if_refcounted(emitter, &result_ty);
            emitter.instruction("ldr x0, [sp], #16");                           // restore the boxed iterator_to_array() result
            emitter.instruction("add sp, sp, #16");                             // discard the saved original iterator_to_array() result pointer
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax");                                  // preserve the owned iterator_to_array() result while boxing it
            crate::codegen::emit_box_current_value_as_mixed(emitter, &result_ty);
            abi::emit_push_reg(emitter, "rax");                                  // preserve the boxed mixed result while releasing the original owner
            emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // reload the original iterator_to_array() result retained by the mixed box
            abi::emit_decref_if_refcounted(emitter, &result_ty);
            abi::emit_pop_reg(emitter, "rax");                                   // restore the boxed iterator_to_array() result
            emitter.instruction("add rsp, 16");                                 // discard the saved original iterator_to_array() result pointer
        }
    }
}

/// Emits assembly for to array loaded traversable object.
fn emit_to_array_loaded_traversable_object(
    preserve_keys: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let receiver_reg = abi::nested_call_reg(emitter);
    emitter.instruction(&format!(
        "mov {}, {}",
        receiver_reg,
        abi::int_result_reg(emitter)
    )); // preserve Traversable receiver while allocating iterator_to_array()'s result
    if preserve_keys {
        iterator_common::emit_new_mixed_hash(emitter);
    } else {
        iterator_common::emit_new_mixed_indexed_array(emitter);
    }
    iterator_common::emit_save_result_under_receiver(emitter);
    iterator_common::emit_restore_receiver_from_preserved_reg(emitter, receiver_reg);

    emit_iterable_object_loop(
        "iterator_to_array_traversable",
        emitter,
        ctx,
        data,
        |_, _, _, _| (),
        |dispatch_target, _, emitter, ctx, data| {
            if preserve_keys {
                iterator_common::emit_insert_current_with_iterator_key(
                    dispatch_target,
                    emitter,
                    ctx,
                    data,
                );
            } else {
                iterator_common::emit_append_current_to_saved_array(
                    dispatch_target,
                    emitter,
                    ctx,
                );
            }
        },
        |_, _, _, _| {},
    );
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // return the completed iterator_to_array() result container
}

/// Emits assembly for to array loaded iterable.
fn emit_to_array_loaded_iterable(
    preserve_keys: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let indexed_case = ctx.next_label("iterator_to_array_iterable_indexed");
    let hash_case = ctx.next_label("iterator_to_array_iterable_hash");
    let object_case = ctx.next_label("iterator_to_array_iterable_object");
    let done = ctx.next_label("iterator_to_array_iterable_done");

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve iterable pointer across heap-kind probing
    abi::emit_call_label(emitter, "__rt_heap_kind");                            // classify the type-erased iterable payload
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #2");                                  // is the iterable an indexed array?
            emitter.instruction(&format!("b.eq {}", indexed_case));             // convert or clone the indexed-array payload
            emitter.instruction("cmp x0, #3");                                  // is the iterable an associative hash?
            emitter.instruction(&format!("b.eq {}", hash_case));                // convert or clone the hash payload
            emitter.instruction("cmp x0, #4");                                  // is the iterable an object?
            emitter.instruction(&format!("b.eq {}", object_case));              // collect a Traversable object through Iterator dispatch
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 2");                                  // is the iterable an indexed array?
            emitter.instruction(&format!("je {}", indexed_case));               // convert or clone the indexed-array payload
            emitter.instruction("cmp rax, 3");                                  // is the iterable an associative hash?
            emitter.instruction(&format!("je {}", hash_case));                  // convert or clone the hash payload
            emitter.instruction("cmp rax, 4");                                  // is the iterable an object?
            emitter.instruction(&format!("je {}", object_case));                // collect a Traversable object through Iterator dispatch
        }
    }
    abi::emit_call_label(emitter, "__rt_iterable_unsupported_kind");            // unsupported iterable payloads abort with a fatal diagnostic

    emitter.label(&object_case);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the object pointer before Traversable collection
    emit_to_array_loaded_traversable_object(preserve_keys, emitter, ctx, data);
    abi::emit_jump(emitter, &done);                                             // skip array payload paths after object traversal

    emitter.label(&hash_case);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the hash pointer before result materialization
    if preserve_keys {
        iterator_common::emit_clone_loaded_runtime_hash_as_mixed(emitter);
    } else {
        let hash_ty = PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        };
        let _ = crate::codegen::builtins::arrays::array_values::emit_loaded_values(
            &hash_ty,
            emitter,
            ctx,
            data,
        );
    }
    abi::emit_jump(emitter, &done);                                             // skip indexed-array payload handling after hash materialization

    emitter.label(&indexed_case);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the indexed-array pointer before result materialization
    iterator_common::emit_clone_loaded_runtime_indexed_array_as_mixed(emitter);

    emitter.label(&done);
}
