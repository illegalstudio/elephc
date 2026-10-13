//! Purpose:
//! Converts borrowed boxed values into independently owned native string results.
//!
//! Called from:
//! - Descriptor argument coercion and typed static-property stores.
//!
//! Key details:
//! - Input is a borrowed Mixed cell in the integer result register.
//! - String payloads bypass the allocating cast; borrowed results persist once.
//! - A cast's explicit owned-result status transfers its existing owner instead of copying again.
//! - Callers supply unique labels and remain responsible for the source box owner.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use super as abi;

/// Borrows boxed string bytes or casts another tag, then creates one independent string owner.
pub fn emit_owned_mixed_string(emitter: &mut Emitter, string_label: &str, persist_label: &str) {
    let owned_label = format!("{persist_label}_owned");
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
    emit_branch_if_php_return_owned(emitter, &owned_label);
    abi::emit_jump(emitter, persist_label);
    emitter.label(string_label);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rax, rdi");                                    // expose the borrowed string payload in the native result pair
    }
    emitter.label(persist_label);
    abi::emit_call_label(emitter, "__rt_str_persist");
    emitter.label(&owned_label);
    abi::emit_pop_reg(emitter, abi::secondary_scratch_reg(emitter));
}

/// Branches on the private caller-saved PHP return sideband before another helper clobbers it.
pub(crate) fn emit_branch_if_php_return_owned(emitter: &mut Emitter, owned_label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbnz x15, {owned_label}"));           // transfer the result's existing owner instead of persisting again
        }
        Arch::X86_64 => {
            emitter.instruction("test r11, r11");                               // inspect the compiled PHP return ownership sideband
            emitter.instruction(&format!("jne {owned_label}"));                 // transfer the result's existing owner without copying
        }
    }
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
            assert!(asm.contains(if name.ends_with("x86_64") { "test r11, r11" } else { "cbnz x15" }), "{name}: {asm}");
            assert!(asm.find("__rt_mixed_cast_string").unwrap() < asm.find("persist_payload_owned:").unwrap(), "{name}: {asm}");
        }
    }
}
