//! Purpose:
//! Removes offsets from boxed PHP arrays without renumbering surviving keys.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction` for `Op::OffsetUnset`.
//! - `super::hashes::lower_hash_unset` for shared key-diagnostic snapshots.
//!
//! Key details:
//! - EIR publishes a detached cell before this lowering promotes and publishes its unique hash.
//! - Destructors can throw or replace the array; no receiver writeback follows value release.
//! - Non-array payloads follow PHP: null is quiet, false emits a deprecation, scalars throw,
//!   and an `ArrayAccess` object has its `offsetUnset()` called.

use crate::codegen::{abi, CodegenIrError, Result};
use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::{expect_operand, hashes};

/// Promotes a rooted array cell to sparse storage, then removes the key from its installed hash.
///
/// A cell that holds no array gets PHP's answer: null is quiet, false reports a deprecation, a string
/// raises `Error("Cannot unset string offsets")`, an object raises `Error("Cannot use object of
/// type C as array")` unless it is `ArrayAccess`, and any other payload raises `Error("Cannot
/// unset offset in a non-array variable")`. An object goes to `__rt_mixed_object_offset_unset`,
/// which calls its `offsetUnset()` (an SPL container or a PHP class) or raises that `Error`.
pub(super) fn lower_offset_unset(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let cell = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    if ctx.value_php_type(cell)?.codegen_repr() != PhpType::Mixed {
        return Err(CodegenIrError::invalid_module("offset_unset expects a boxed PHP array"));
    }
    ctx.load_value_to_reg(cell, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_cell_promote_to_hash");
    let valid = ctx.next_label("offset_unset_array");
    let done = ctx.next_label("offset_unset_done");
    let fallback_done = ctx.next_label("offset_unset_fallback_done");
    let string_offset = ctx.next_label("offset_unset_string");
    let non_array = ctx.next_label("offset_unset_non_array");
    let object = ctx.next_label("offset_unset_object");
    let false_offset = ctx.next_label("offset_unset_false");
    // PHP answers a receiver that is not an array by what it holds: null (a removed untyped
    // property is recreated as null by the write fetch) and false leave everything alone, a
    // string refuses its offsets, and every other scalar is not a container at all.
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {valid}"));              // a nonzero result is the cell's installed unique hash
            ctx.load_value_to_reg(cell, "x9")?;
            ctx.emitter.instruction(&format!("cbz x9, {fallback_done}"));       // preserve the conservative safe point for a non-array payload
            ctx.emitter.instruction("ldr x10, [x9]");                           // load the runtime tag of the non-array payload
            ctx.emitter.instruction("cmp x10, #8");                             // runtime tag 8 = null
            ctx.emitter.instruction(&format!("b.eq {fallback_done}"));          // unset of an offset of null is a silent no-op
            ctx.emitter.instruction("cmp x10, #1");                             // runtime tag 1 = string
            ctx.emitter.instruction(&format!("b.eq {string_offset}"));          // strings refuse offset removal with their own Error
            ctx.emitter.instruction("cmp x10, #6");                             // runtime tag 6 = object
            ctx.emitter.instruction(&format!("b.eq {object}"));                 // an object refuses indexing with a message naming its class
            ctx.emitter.instruction("cmp x10, #3");                             // runtime tag 3 = bool
            ctx.emitter.instruction(&format!("b.ne {non_array}"));              // every other payload is not a container
            ctx.emitter.instruction("ldr x10, [x9, #8]");                       // load the bool payload
            ctx.emitter.instruction(&format!("cbz x10, {false_offset}"));       // false reports its conversion deprecation without removing a key
            ctx.emitter.instruction(&format!("b {non_array}"));                 // true is not a container
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // a nonzero result is the cell's installed unique hash
            ctx.emitter.instruction(&format!("jnz {valid}"));                   // take the removal path for a real array
            ctx.load_value_to_reg(cell, "r10")?;
            ctx.emitter.instruction("test r10, r10");                           // a missing cell reads as null, which PHP leaves alone
            ctx.emitter.instruction(&format!("jz {fallback_done}"));            // preserve the conservative safe point for a non-array payload
            ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                // load the runtime tag of the non-array payload
            ctx.emitter.instruction("cmp r11, 8");                              // runtime tag 8 = null
            ctx.emitter.instruction(&format!("je {fallback_done}"));            // unset of an offset of null is a silent no-op
            ctx.emitter.instruction("cmp r11, 1");                              // runtime tag 1 = string
            ctx.emitter.instruction(&format!("je {string_offset}"));            // strings refuse offset removal with their own Error
            ctx.emitter.instruction("cmp r11, 6");                              // runtime tag 6 = object
            ctx.emitter.instruction(&format!("je {object}"));                   // an object refuses indexing with a message naming its class
            ctx.emitter.instruction("cmp r11, 3");                              // runtime tag 3 = bool
            ctx.emitter.instruction(&format!("jne {non_array}"));               // every other payload is not a container
            ctx.emitter.instruction("cmp QWORD PTR [r10 + 8], 0");              // is the bool payload false?
            ctx.emitter.instruction(&format!("je {false_offset}"));             // false reports its conversion deprecation without removing a key
            ctx.emitter.instruction(&format!("jmp {non_array}"));               // true is not a container
        }
    }
    ctx.emitter.label(&object);
    // `offsetUnset()` receives the offset exactly as written (`"12"` stays a string, `1.5` a
    // float), so the key is boxed from its own value rather than normalized as an array key.
    // The helper takes ownership of that box: a borrowed `mixed` key is retained first.
    let key_ty = ctx.value_php_type(key)?;
    ctx.load_value_to_result(key)?;
    if key_ty.codegen_repr() == PhpType::Mixed {
        abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    } else {
        crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &key_ty);
    }
    let boxed_key = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, boxed_key);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x1");
            ctx.load_value_to_reg(cell, "x9")?;
            ctx.emitter.instruction("ldr x0, [x9, #8]");                        // pass the unboxed object
            abi::emit_call_label(ctx.emitter, "__rt_mixed_object_offset_unset");
            ctx.emitter.instruction(&format!("b {fallback_done}"));             // retain collection after an arbitrary offsetUnset callback
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rsi");
            ctx.load_value_to_reg(cell, "r10")?;
            ctx.emitter.instruction("mov rdi, QWORD PTR [r10 + 8]");            // pass the unboxed object
            abi::emit_call_label(ctx.emitter, "__rt_mixed_object_offset_unset");
            ctx.emitter.instruction(&format!("jmp {fallback_done}"));           // retain collection after an arbitrary offsetUnset callback
        }
    }
    ctx.emitter.label(&false_offset);
    super::runtime_class_messages::emit_static_diagnostic(
        ctx,
        "Deprecated: Automatic conversion of false to array is deprecated\n",
        true,
    );
    abi::emit_jump(ctx.emitter, &fallback_done);
    ctx.emitter.label(&string_offset);
    super::exceptions::emit_error(ctx, "Cannot unset string offsets");
    ctx.emitter.label(&non_array);
    super::exceptions::emit_error(ctx, "Cannot unset offset in a non-array variable");
    ctx.emitter.label(&fallback_done);
    if inst.result.is_some() {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        super::store_if_result(ctx, inst)?;
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&valid);
    lower_array_offset_unset(ctx, key)?;
    hashes::store_unset_result(ctx, inst)?;
    ctx.emitter.label(&done);
    Ok(())
}

/// Rejects invalid array offsets only after selecting the array receiver branch.
/// The same key remains legal for a user ArrayAccess receiver reached through the object branch.
fn lower_array_offset_unset(ctx: &mut FunctionContext<'_>, key: ValueId) -> Result<()> {
    match ctx.value_php_type(key)?.codegen_repr() {
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            super::exceptions::emit_type_error(ctx, "Cannot unset offset of type array on array");
            return Ok(());
        }
        PhpType::Object(_) => {
            ctx.load_value_to_result(key)?;
            emit_object_key_error(ctx);
            return Ok(());
        }
        _ => {}
    }
    if matches!(ctx.value_php_type(key)?.codegen_repr(), PhpType::Float | PhpType::Mixed) {
        return lower_array_unset_with_key_diagnostics(ctx, key);
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            hashes::materialize_hash_key_aarch64(ctx, key)?;
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            hashes::materialize_hash_key_x86_64(ctx, key)?;
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_unset");
    Ok(())
}

/// Retains the selected hash across key diagnostics without delaying ordinary destructor reentry.
/// An attached hash drops this snapshot owner before deletion; a detached one keeps it to the end.
pub(super) fn lower_array_unset_with_key_diagnostics(
    ctx: &mut FunctionContext<'_>, key: ValueId,
) -> Result<()> {
    // -- publish one snapshot owner across any key-conversion warning callback --
    abi::emit_reserve_temporary_stack(ctx.emitter, 96);
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result, 8);
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 8);
    abi::emit_store_to_sp(ctx.emitter, result, 0);
    let owner_address = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_temporary_stack_address(ctx.emitter, owner_address, 0);
    abi::emit_link_call_operand_owner_at_stack(ctx.emitter, owner_address, false, 32);
    if ctx.value_php_type(key)?.codegen_repr() == PhpType::Mixed {
        guard_boxed_array_key(ctx, key)?;
    }
    let (key_lo, key_hi) = match ctx.emitter.target.arch {
        Arch::AArch64 => {
            hashes::materialize_hash_key_aarch64(ctx, key)?;
            ("x1", "x2")
        }
        Arch::X86_64 => {
            hashes::materialize_hash_key_x86_64(ctx, key)?;
            ("rsi", "rdx")
        }
    };
    abi::emit_store_to_sp(ctx.emitter, key_lo, 16);
    abi::emit_store_to_sp(ctx.emitter, key_hi, 24);
    let remove = ctx.next_label("unset_diagnostic_remove");
    let done = ctx.next_label("unset_diagnostic_done");
    // -- preserve ordinary destruction timing when another owner still holds the hash --
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr w9, [x0, #-12]");                      // read the uniform hash reference count after the callback
            ctx.emitter.instruction("cmp w9, #1");                              // the snapshot is the only owner of a detached hash
            ctx.emitter.instruction(&format!("b.ls {remove}"));                 // keep detached storage alive through deletion and unwind
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp DWORD PTR [rax - 12], 1");             // compare the physical owner count with our snapshot reference
            ctx.emitter.instruction(&format!("jbe {remove}"));                  // keep detached storage alive through deletion and unwind
        }
    }
    abi::emit_unlink_call_operand_owner_at_stack(ctx.emitter, 32);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 0);
    abi::emit_load_int_immediate(ctx.emitter, abi::tertiary_scratch_reg(ctx.emitter), 0);
    abi::emit_store_to_sp(ctx.emitter, abi::tertiary_scratch_reg(ctx.emitter), 0);
    abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
    ctx.emitter.label(&remove);
    let array_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, array_arg, 8);
    abi::emit_load_temporary_stack_slot(ctx.emitter, key_lo, 16);
    abi::emit_load_temporary_stack_slot(ctx.emitter, key_hi, 24);
    abi::emit_call_label(ctx.emitter, "__rt_hash_unset");
    let removed = if ctx.emitter.target.arch == Arch::AArch64 { "x1" } else { "rdx" };
    abi::emit_store_to_sp(ctx.emitter, removed, 80);
    // -- only detached snapshots still have an owner record to retire --
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {done}"));                // an attached hash already returned its snapshot owner
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // an attached hash already returned its snapshot owner
            ctx.emitter.instruction(&format!("jz {done}"));                     // avoid releasing a consumed snapshot twice
        }
    }
    abi::emit_unlink_call_operand_owner_at_stack(ctx.emitter, 32);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 0);
    abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
    ctx.emitter.label(&done);
    abi::emit_load_temporary_stack_slot(ctx.emitter, removed, 80);
    abi::emit_release_temporary_stack(ctx.emitter, 96);
    Ok(())
}

/// Prevents boxed array and object offsets from falling through hash normalization as key zero.
fn guard_boxed_array_key(ctx: &mut FunctionContext<'_>, key: ValueId) -> Result<()> {
    let array = ctx.next_label("unset_key_array");
    let object = ctx.next_label("unset_key_object");
    let valid = ctx.next_label("unset_key_valid");
    ctx.load_value_to_result(key)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let result = abi::int_result_reg(ctx.emitter);
    let arm = ctx.emitter.target.arch == Arch::AArch64;
    for tag in [4, 5] {
        ctx.emitter.instruction(&format!("cmp {result}, {}{tag}", if arm { "#" } else { "" })); // reject both PHP array payload representations
        ctx.emitter.instruction(&format!("{} {array}", if arm { "b.eq" } else { "je" })); // select the array-key TypeError
    }
    ctx.emitter.instruction(&format!("cmp {result}, {}6", if arm { "#" } else { "" })); // an object offset must name its runtime class
    ctx.emitter.instruction(&format!("{} {object}", if arm { "b.eq" } else { "je" })); // select the object-key TypeError
    abi::emit_jump(ctx.emitter, &valid);
    ctx.emitter.label(&array);
    super::exceptions::emit_type_error(ctx, "Cannot unset offset of type array on array");
    ctx.emitter.label(&object);
    abi::emit_reg_move(ctx.emitter, result, crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target));
    emit_object_key_error(ctx);
    ctx.emitter.label(&valid);
    Ok(())
}

/// Builds PHP's invalid offset message with the actual runtime class of an object key.
fn emit_object_key_error(ctx: &mut FunctionContext<'_>) {
    let result = abi::int_result_reg(ctx.emitter);
    super::runtime_class_messages::emit_runtime_class_name_to_string_result(ctx, result, "object");
    super::runtime_class_messages::emit_concat_static_prefix(ctx, "Cannot unset offset of type ");
    super::runtime_class_messages::emit_concat_static_suffix(ctx, " on array");
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    super::exceptions::emit_type_error_from_string_result(ctx);
}
