//! Purpose:
//! Builds associative variadic argument containers from runtime hash sources.
//! Shares keyed `...$rest` construction between call_user_func_array() and spread lowering.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::call_user_func_array`
//! - `crate::codegen::expr::calls::args::spread`
//!
//! Key details:
//! - Numeric keys consumed by fixed parameters are skipped, while unknown string keys
//!   remain in the variadic hash for user-defined callable targets.

use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::{abi, context::Context};
use crate::types::{FunctionSig, PhpType};

/// Emits assembly for a loaded associative variadic array argument.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_loaded_assoc_variadic_array_arg(
    source_hash_reg: &str,
    elem_ty: &PhpType,
    sig: &FunctionSig,
    skip_numeric_before: usize,
    skip_param_names_before: usize,
    context_label: &str,
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
            PhpType::Iterable => Some(PhpType::Mixed),
            _ => None,
        })
        .unwrap_or_else(|| elem_ty.clone());
    let variadic_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(variadic_elem_ty.clone()),
    };
    let capacity_reg = abi::int_arg_reg_name(emitter.target, 0);
    let tag_reg = abi::int_arg_reg_name(emitter.target, 1);

    emitter.comment(context_label);
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
        skip_numeric_before,
        skip_param_names_before,
        emitter,
        ctx,
        data,
    );

    variadic_ty
}

/// Emits assembly for copying loaded associative source entries into the variadic hash.
fn emit_loaded_assoc_variadic_entries(
    source_hash_reg: &str,
    sig: &FunctionSig,
    skip_numeric_before: usize,
    skip_param_names_before: usize,
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

    let loop_label = ctx.next_label("assoc_variadic_loop");
    let done_label = ctx.next_label("assoc_variadic_done");
    let skip_label = ctx.next_label("assoc_variadic_skip");
    let numeric_key_label = ctx.next_label("assoc_variadic_numeric_key");
    let string_key_label = ctx.next_label("assoc_variadic_string_key");
    let insert_label = ctx.next_label("assoc_variadic_insert");
    let value_string_label = ctx.next_label("assoc_variadic_value_string");
    let value_ref_label = ctx.next_label("assoc_variadic_value_ref");
    let value_scalar_label = ctx.next_label("assoc_variadic_value_scalar");
    let insert_call_label = ctx.next_label("assoc_variadic_insert_call");

    abi::emit_reserve_temporary_stack(emitter, SCRATCH_BYTES);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("str {}, [sp, #{}]", source_hash_reg, SOURCE_HASH_OFF)); // save the source hash for the variadic scan
            emitter.instruction(&format!("str xzr, [sp, #{}]", CURSOR_OFF));    // start hash iteration from the insertion-order head
            emitter.instruction(&format!("str xzr, [sp, #{}]", NUMERIC_KEY_OFF)); // start numeric variadic keys from zero
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov QWORD PTR [rsp + {}], {}", SOURCE_HASH_OFF, source_hash_reg)); // save the source hash for the variadic scan
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
            emitter.instruction("cmn x2, #1");                                  // is the current source key numeric?
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
            emitter.instruction("cmp rdx, -1");                                 // is the current source key numeric?
            emitter.instruction(&format!("je {}", numeric_key_label));          // numeric keys are positional and may belong to ...$rest
            emitter.instruction(&format!("jmp {}", string_key_label));          // string keys must be filtered by regular parameter names
        }
    }

    emitter.label(&numeric_key_label);
    emit_skip_if_consumed_numeric_key(skip_numeric_before, &skip_label, emitter);
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
    for (param_name, _) in sig.params.iter().take(skip_param_names_before) {
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
            emitter.instruction(&format!("b {}", loop_label));                  // continue scanning source entries after skipping a consumed key
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("jmp {}", loop_label));                // continue scanning source entries after skipping a consumed key
        }
    }

    emitter.label(&done_label);
    abi::emit_release_temporary_stack(emitter, SCRATCH_BYTES);
}

/// Emits assembly that skips a numeric source key already consumed by regular parameters.
fn emit_skip_if_consumed_numeric_key(
    skip_numeric_before: usize,
    skip_label: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x8", 16);
            abi::emit_load_int_immediate(emitter, "x9", skip_numeric_before as i64);
            emitter.instruction("cmp x8, x9");                                  // has this numeric key already filled a regular callback parameter?
            emitter.instruction(&format!("b.lt {}", skip_label));               // skip numeric keys consumed by the fixed callback prefix
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r10", 16);
            abi::emit_load_int_immediate(emitter, "r11", skip_numeric_before as i64);
            emitter.instruction("cmp r10, r11");                                // has this numeric key already filled a regular callback parameter?
            emitter.instruction(&format!("jl {}", skip_label));                 // skip numeric keys consumed by the fixed callback prefix
        }
    }
}

/// Emits assembly that rewrites an accepted numeric tail key to the next compact variadic key.
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

/// Emits assembly that skips a string key matching an already-bound regular parameter.
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

/// Emits assembly that prepares a hash entry payload and inserts it into the variadic hash.
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
