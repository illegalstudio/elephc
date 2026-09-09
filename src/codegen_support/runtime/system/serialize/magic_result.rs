//! Purpose:
//! Consumes __serialize() results while borrowing their raw array payload for recursive encoding.
//!
//! Called from:
//! - The object serializer after invoking the native magic method.
//!
//! Key details:
//! - PHP array returns may be boxed Mixed cells or concrete indexed/hash storage.
//! - A bounded exception handler retires the original owner after success or a nested throw.
//! - Cleanup preserves completed output and chains destructor exceptions before propagating.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::runtime::{data::SERIALIZE_RETURN_ARRAY_MSG, exceptions};

const FRAME: usize = 48;
const OWNER: usize = 8;
const PENDING: usize = 16;
const CONCAT_END: usize = 24;

/// Emits a consuming serializer boundary and its borrowed array-body dispatcher.
pub(super) fn emit_magic_result(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let older = match emitter.target.arch { Arch::AArch64 => "x1", Arch::X86_64 => "rdi" };
    emitter.blank();
    emitter.label_global("__rt_serialize_magic_result");
    abi::emit_frame_prologue(emitter, FRAME);
    abi::store_at_offset(emitter, result, OWNER);
    abi::emit_store_zero_to_local_slot(emitter, PENDING);
    exceptions::emit_guarded_cleanup_call(emitter, "__rt_serialize_magic_body", result, PENDING);
    abi::emit_load_symbol_to_reg(emitter, result, "_concat_off", 0);
    abi::store_at_offset(emitter, result, CONCAT_END);
    abi::load_at_offset(emitter, result, OWNER);
    exceptions::emit_guarded_cleanup_call(emitter, "__rt_decref_any", result, PENDING);
    abi::load_at_offset(emitter, result, CONCAT_END);
    abi::emit_store_reg_to_symbol(emitter, result, "_concat_off", 0);
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_serialize_magic_result_return");
    abi::emit_load_symbol_to_reg(emitter, older, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_jump(emitter, "__rt_throw_current");
    emitter.label("__rt_serialize_magic_result_return");
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
    emit_magic_body(emitter);
}

/// Checks storage and value tags before borrowing the payload for an ordinary body emitter.
fn emit_magic_body(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let low = match emitter.target.arch { Arch::AArch64 => "x1", Arch::X86_64 => "rdi" };
    emitter.blank();
    emitter.label_global("__rt_serialize_magic_body");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_heap_kind");
    branch_if_tag(emitter, 5, "__rt_serialize_magic_body_boxed");
    branch_if_tag(emitter, 2, "__rt_serialize_magic_body_raw_indexed");
    branch_if_tag(emitter, 3, "__rt_serialize_magic_body_raw_hash");
    abi::emit_jump(emitter, "__rt_serialize_magic_body_invalid");
    emitter.label("__rt_serialize_magic_body_boxed");
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    branch_if_tag(emitter, 4, "__rt_serialize_magic_body_boxed_indexed");
    branch_if_tag(emitter, 5, "__rt_serialize_magic_body_boxed_hash");
    abi::emit_jump(emitter, "__rt_serialize_magic_body_invalid");
    for (suffix, body, boxed) in [
        ("raw_indexed", "__rt_serialize_indexed_body", false),
        ("raw_hash", "__rt_serialize_hash_body", false),
        ("boxed_indexed", "__rt_serialize_indexed_body", true),
        ("boxed_hash", "__rt_serialize_hash_body", true),
    ] {
        emitter.label(&format!("__rt_serialize_magic_body_{suffix}"));
        if boxed {
            abi::emit_reg_move(emitter, result, low);
        } else {
            abi::load_at_offset(emitter, result, 8);
        }
        abi::emit_frame_restore(emitter, 32);
        abi::emit_jump(emitter, body);
    }
    emitter.label("__rt_serialize_magic_body_invalid");
    emit_invalid_result(emitter);
}

/// Branches within the current helper without creating cross-atom conditional relocations.
fn branch_if_tag(emitter: &mut Emitter, tag: usize, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x0, #{tag}"));                    // check the storage or boxed runtime tag
            emitter.instruction(&format!("b.eq {label}"));                      // dispatch only to a local validation label
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp rax, {tag}"));                    // check the storage or boxed runtime tag
            emitter.instruction(&format!("je {label}"));                        // dispatch only to a local validation label
        }
    }
}

/// Raises a catchable TypeError; the enclosing boundary still owns and retires the invalid result.
fn emit_invalid_result(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, result, 56);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    let kind = match emitter.target.arch {
        Arch::AArch64 => 6,
        Arch::X86_64 => crate::codegen_support::sentinels::x86_64_heap_kind_word(6) as i64,
    };
    abi::emit_load_int_immediate(emitter, scratch, kind);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("stur x10, [x0, #-8]"),            // stamp the allocated payload as a Throwable object
        Arch::X86_64 => emitter.instruction("mov QWORD PTR [rax - 8], r10"),    // stamp the allocated payload as a Throwable object
    }
    abi::emit_call_label(emitter, "__rt_object_handle_acquire");
    abi::emit_load_symbol_to_reg(emitter, scratch, "_spl_type_error_class_id", 0);
    abi::emit_store_to_address(emitter, scratch, result, 0);
    abi::emit_symbol_address(emitter, scratch, "_serialize_return_array_msg");
    abi::emit_store_to_address(emitter, scratch, result, 8);
    abi::emit_load_int_immediate(emitter, scratch, SERIALIZE_RETURN_ARRAY_MSG.len() as i64);
    abi::emit_store_to_address(emitter, scratch, result, 16);
    abi::emit_load_int_immediate(emitter, scratch, 0);
    for offset in [24, 40, 48] {
        abi::emit_store_to_address(emitter, scratch, result, offset);
    }
    crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(emitter, result);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::emit_jump(emitter, "__rt_throw_current");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Both result layouts and exceptional release use the same boundary on every supported target.
    #[test]
    fn serialize_magic_results_are_unboxed_and_retired_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_magic_result(&mut emitter);
            let asm = emitter.output();
            let (owner, body) = asm.split_once("__rt_serialize_magic_body:").unwrap();
            assert_eq!(owner.matches("__rt_cleanup_invoke").count(), 2, "{name}");
            let serialize = owner.find("__rt_serialize_magic_body").unwrap();
            let release = owner.find("__rt_decref_any").unwrap();
            let rethrow = owner.find("__rt_throw_current").unwrap();
            assert!(serialize < release && release < rethrow, "{name}");
            assert!(body.find("__rt_heap_kind").unwrap() < body.find("__rt_mixed_unbox").unwrap(), "{name}");
            assert_eq!(body.matches("__rt_serialize_indexed_body").count(), 2, "{name}");
            assert_eq!(body.matches("__rt_serialize_hash_body").count(), 2, "{name}");
            assert!(body.contains("_spl_type_error_class_id"), "{name}");
        }
    }
}
