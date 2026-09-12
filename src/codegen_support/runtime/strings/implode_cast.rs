//! Purpose:
//! Converts boxed join elements through scalar formatting, warnings or native object string hooks.
//!
//! Called from:
//! - The `__rt_implode` runtime emitter.
//!
//! Key details:
//! - Object conversion reuses the formatter's class table and owned-string contract.
//! - PHP callbacks may reset concat scratch, so its published prefix is saved and restored.
//! - The prefix owner is visible to exception unwinding while user code runs.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits a borrowed-box conversion entry that preserves pending join bytes across PHP callbacks.
pub(super) fn emit_implode_element_cast(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let (ptr, len) = abi::string_result_regs(emitter);
    let owner = abi::tertiary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_implode_cast_string");
    abi::emit_frame_prologue(emitter, 64);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #4");                                  // indexed arrays may invoke a PHP warning handler
            emitter.instruction("b.eq __rt_implode_cast_object");               // preserve scratch before dispatching the array warning
            emitter.instruction("cmp x0, #5");                                  // associative arrays share the warning-capable conversion
            emitter.instruction("b.eq __rt_implode_cast_object");               // preserve scratch before dispatching the hash warning
            emitter.instruction("cmp x0, #6");                                  // native objects require their PHP string conversion hook
            emitter.instruction("b.eq __rt_implode_cast_object");               // preserve scratch before invoking object code
            emitter.instruction("cmp x0, #10");                                 // closures use the formatter's catchable conversion error
            emitter.instruction("b.eq __rt_implode_cast_object");               // reject callable objects without silently returning empty text
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 4");                                  // indexed arrays may invoke a PHP warning handler
            emitter.instruction("je __rt_implode_cast_object");                 // preserve scratch before dispatching the array warning
            emitter.instruction("cmp rax, 5");                                  // associative arrays share the warning-capable conversion
            emitter.instruction("je __rt_implode_cast_object");                 // preserve scratch before dispatching the hash warning
            emitter.instruction("cmp rax, 6");                                  // native objects require their PHP string conversion hook
            emitter.instruction("je __rt_implode_cast_object");                 // preserve scratch before invoking object code
            emitter.instruction("cmp rax, 10");                                 // closures use the formatter's catchable conversion error
            emitter.instruction("je __rt_implode_cast_object");                 // reject callable objects without silently returning empty text
        }
    }
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_mixed_cast_string");
    abi::emit_jump(emitter, "__rt_implode_cast_done");

    emitter.label("__rt_implode_cast_object");
    abi::emit_load_symbol_to_reg(emitter, len, "_concat_off", 0);
    abi::store_at_offset(emitter, len, 16);
    abi::emit_symbol_address(emitter, ptr, "_concat_buf");
    abi::emit_call_label(emitter, "__rt_str_persist");
    abi::store_at_offset(emitter, ptr, 24);
    abi::emit_frame_slot_address(emitter, owner, 24);
    abi::emit_push_call_operand_owner(emitter, owner, false);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x2, #0");                                  // native join conversion has no interpreter execution context
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rdi");                                // preserve the unboxed object payload as the second C argument
            emitter.instruction("mov rdi, rax");                                // pass the concrete tag as the first C argument
            emitter.instruction("xor edx, edx");                                // native join conversion has no interpreter execution context
        }
    }
    abi::emit_call_label(emitter, "__rt_sprintf_mixed_to_string");
    abi::store_at_offset(emitter, ptr, 32);
    abi::store_at_offset(emitter, len, 40);
    abi::emit_pop_call_operand_owner(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    let arg2 = abi::int_arg_reg_name(emitter.target, 2);
    abi::emit_symbol_address(emitter, arg0, "_concat_buf");
    abi::load_at_offset(emitter, arg1, 24);
    abi::load_at_offset(emitter, arg2, 16);
    abi::emit_call_label(emitter, &emitter.target.extern_symbol("memcpy"));
    abi::load_at_offset(emitter, result, 16);
    abi::emit_store_reg_to_symbol(emitter, result, "_concat_off", 0);
    abi::load_at_offset(emitter, result, 24);
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    abi::load_at_offset(emitter, ptr, 32);
    abi::load_at_offset(emitter, len, 40);
    emitter.label("__rt_implode_cast_done");
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every ABI roots the prefix before user conversion and restores it before releasing the copy.
    #[test]
    fn implode_object_cast_preserves_prefix_ownership_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_implode_element_cast(&mut emitter);
            let asm = emitter.output();
            let publish = asm.find("__rt_cleanup_call_operand_owner").unwrap();
            let invoke = asm.find("__rt_sprintf_mixed_to_string").unwrap();
            let restore = asm.find("memcpy").unwrap();
            let release = asm.find("__rt_heap_free_safe").unwrap();
            assert!(publish < invoke && invoke < restore && restore < release, "{name}: {asm}");
            assert!(asm.contains("__rt_mixed_cast_string"), "{name}: scalar fallback");
        }
    }
}
