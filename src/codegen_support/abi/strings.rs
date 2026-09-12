//! Purpose:
//! Converts borrowed boxed values into independently owned native string results.
//!
//! Called from:
//! - Descriptor argument coercion and typed static-property stores.
//!
//! Key details:
//! - Input is a borrowed Mixed cell in the integer result register.
//! - String payloads bypass the allocating cast; all paths persist exactly once.
//! - Callers supply unique labels and remain responsible for the source box owner.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use super as abi;

/// Borrows boxed string bytes or casts another tag, then creates one independent string owner.
pub fn emit_owned_mixed_string(emitter: &mut Emitter, string_label: &str, persist_label: &str) {
    let result = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #1");                                  // a string payload can be borrowed without the allocating cast
            emitter.instruction(&format!("b.eq {string_label}"));               // keep the unboxed string pointer and length for one persistence call
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 1");                                  // a string payload can be borrowed without the allocating cast
            emitter.instruction(&format!("je {string_label}"));                 // skip the allocating string cast for an existing string
        }
    }
    abi::emit_load_temporary_stack_slot(emitter, result, 0);
    abi::emit_call_label(emitter, "__rt_mixed_cast_string");
    abi::emit_jump(emitter, persist_label);
    emitter.label(string_label);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rax, rdi");                                    // expose the borrowed string payload in the native result pair
    }
    emitter.label(persist_label);
    abi::emit_call_label(emitter, "__rt_str_persist");
    abi::emit_pop_reg(emitter, abi::secondary_scratch_reg(emitter));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Strings and converted scalars share one persistence point on every supported target.
    #[test]
    fn mixed_string_conversion_persists_exactly_once_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_owned_mixed_string(&mut emitter, "string_payload", "persist_payload");
            let asm = emitter.output();
            assert_eq!(asm.matches("__rt_str_persist").count(), 1, "{name}: {asm}");
            assert_eq!(asm.matches("__rt_mixed_cast_string").count(), 1, "{name}: {asm}");
            assert!(asm.find("__rt_mixed_cast_string").unwrap() < asm.find("string_payload:").unwrap(), "{name}: {asm}");
            assert!(asm.find("persist_payload:").unwrap() < asm.find("__rt_str_persist").unwrap(), "{name}: {asm}");
        }
    }
}
