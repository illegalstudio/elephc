//! Purpose:
//! Emits PHP `iterator_count()` calls for arrays and Iterator/IteratorAggregate objects.
//! Reuses the statement foreach iterator driver for object traversal.
//!
//! Called from:
//! - `crate::codegen::builtins::spl::emit()`
//!
//! Key details:
//! - Object iteration calls rewind(), valid(), and next() just like PHP and leaves the iterator exhausted.
//! - The saved count lives beneath the loop driver's receiver stack slot.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::codegen::stmt::{emit_iterable_object_loop, emit_iterator_loop};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::iterator_common;

/// Emits the iterator count entry point for this module.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("iterator_count()");
    let source_ty = emit_expr(&args[0], emitter, ctx, data);
    if iterator_common::emit_count_loaded_array(&source_ty, emitter) {
        return Some(PhpType::Int);
    }

    if matches!(source_ty.codegen_repr(), PhpType::Iterable) {
        emit_count_loaded_iterable(emitter, ctx, data);
        return Some(PhpType::Int);
    }

    let Some(class_name) = iterator_common::iterator_object_name(&source_ty) else {
        return Some(PhpType::Int);
    };

    if class_name == "Traversable" {
        emit_count_loaded_traversable_object(emitter, ctx, data);
        return Some(PhpType::Int);
    }

    emit_count_loaded_iterator_object(class_name, emitter, ctx, data);
    Some(PhpType::Int)
}

/// Emits assembly for count loaded iterator object.
fn emit_count_loaded_iterator_object(
    class_name: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let receiver_reg = abi::nested_call_reg(emitter);
    emitter.instruction(&format!(
        "mov {}, {}",
        receiver_reg,
        abi::int_result_reg(emitter)
    )); // preserve iterator receiver while initializing the count slot
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // save iterator_count()'s counter underneath the loop receiver
    iterator_common::emit_restore_receiver_from_preserved_reg(emitter, receiver_reg);

    let loop_start = ctx.next_label("iterator_count_start");
    let loop_end = ctx.next_label("iterator_count_end");
    let loop_cont = ctx.next_label("iterator_count_cont");
    emit_iterator_loop(
        class_name,
        &loop_start,
        &loop_end,
        &loop_cont,
        emitter,
        ctx,
        data,
        |_, _, _, _| (),
        |_, emitter, _, _| iterator_common::emit_increment_saved_count(emitter),
        |_, _, _, _| {},
    );
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // return the final saved iterator_count() counter
}

/// Emits assembly for count loaded traversable object.
fn emit_count_loaded_traversable_object(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let receiver_reg = abi::nested_call_reg(emitter);
    emitter.instruction(&format!(
        "mov {}, {}",
        receiver_reg,
        abi::int_result_reg(emitter)
    )); // preserve Traversable receiver while initializing the count slot
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // save iterator_count()'s counter underneath the loop receiver
    iterator_common::emit_restore_receiver_from_preserved_reg(emitter, receiver_reg);

    emit_iterable_object_loop(
        "iterator_count_traversable",
        emitter,
        ctx,
        data,
        |_, _, _, _| (),
        |_, _, emitter, _, _| iterator_common::emit_increment_saved_count(emitter),
        |_, _, _, _| {},
    );
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // return the final saved iterator_count() counter
}

/// Emits assembly for count loaded iterable.
fn emit_count_loaded_iterable(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let indexed_case = ctx.next_label("iterator_count_iterable_indexed");
    let hash_case = ctx.next_label("iterator_count_iterable_hash");
    let object_case = ctx.next_label("iterator_count_iterable_object");
    let done = ctx.next_label("iterator_count_iterable_done");

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve iterable pointer across heap-kind probing
    abi::emit_call_label(emitter, "__rt_heap_kind");                            // classify the type-erased iterable payload
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #2");                                  // is the iterable an indexed array?
            emitter.instruction(&format!("b.eq {}", indexed_case));             // count indexed-array entries directly
            emitter.instruction("cmp x0, #3");                                  // is the iterable an associative hash?
            emitter.instruction(&format!("b.eq {}", hash_case));                // count hash entries directly
            emitter.instruction("cmp x0, #4");                                  // is the iterable an object?
            emitter.instruction(&format!("b.eq {}", object_case));              // count a Traversable object through Iterator dispatch
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 2");                                  // is the iterable an indexed array?
            emitter.instruction(&format!("je {}", indexed_case));               // count indexed-array entries directly
            emitter.instruction("cmp rax, 3");                                  // is the iterable an associative hash?
            emitter.instruction(&format!("je {}", hash_case));                  // count hash entries directly
            emitter.instruction("cmp rax, 4");                                  // is the iterable an object?
            emitter.instruction(&format!("je {}", object_case));                // count a Traversable object through Iterator dispatch
        }
    }
    abi::emit_call_label(emitter, "__rt_iterable_unsupported_kind");            // unsupported iterable payloads abort with a fatal diagnostic

    emitter.label(&object_case);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the object pointer before Traversable counting
    emit_count_loaded_traversable_object(emitter, ctx, data);
    abi::emit_jump(emitter, &done);                                             // skip array counting paths after object traversal

    emitter.label(&hash_case);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the hash pointer before reading its entry count
    iterator_common::emit_count_loaded_array(
        &PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        },
        emitter,
    );
    abi::emit_jump(emitter, &done);                                             // skip indexed-array count after hash counting

    emitter.label(&indexed_case);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the indexed-array pointer before reading its length
    iterator_common::emit_count_loaded_array(&PhpType::Array(Box::new(PhpType::Mixed)), emitter);

    emitter.label(&done);
}
