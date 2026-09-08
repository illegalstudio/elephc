//! Purpose:
//! Separates non-private eval child fields from same-named private native parent slots.
//!
//! Called from:
//! - Native property-name dispatch after the parent's private visibility check fails.
//!
//! Key details:
//! - The callback only chooses extra storage and does not authorize native private access.
//! - All receiver/name inputs come from the enclosing getter or setter frame.

use super::{abi, Arch, Emitter, Module};

/// Routes a real eval child field past the private parent slot, failing closed for other objects.
pub(super) fn emit_separate_property_probe(
    module: &Module,
    emitter: &mut Emitter,
    separate: &str,
    fail: &str,
) {
    match module.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [sp, #16]");                           // pass the raw receiver identity to its eval metadata owner
            emitter.instruction("ldr x1, [sp]");                                // pass the requested property name
            emitter.instruction("ldr x2, [sp, #8]");                            // pass the property name length
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // pass the raw receiver identity to its eval metadata owner
            emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                // pass the requested property name
            emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");               // pass the property name length
        }
    }
    let callback = module.target.extern_symbol("__elephc_eval_dynamic_object_has_separate_property");
    abi::emit_call_label(emitter, &callback);
    match module.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {fail}"));                    // ordinary private access still fails
            emitter.instruction("ldr x9, [sp, #16]");                           // restore the receiver after the metadata callback
            emitter.instruction("ldr x9, [x9]");                                // restore class dispatch state before skipping this parent slot
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // check whether an independent eval field actually exists
            emitter.instruction(&format!("jz {fail}"));                         // keep the native private field inaccessible
            emitter.instruction("mov r11, QWORD PTR [rbp - 24]");               // recover the receiver after the metadata callback
            emitter.instruction("mov r11, QWORD PTR [r11]");                    // restore class dispatch state before skipping this parent slot
        }
    }
    abi::emit_jump(emitter, separate);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Each target uses the platform callback symbol and retains the private-access failure edge.
    #[test]
    fn private_shadow_probe_preserves_target_abi_and_access_failure() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let module = Module::new(target);
            let mut emitter = Emitter::new(target);
            emit_separate_property_probe(&module, &mut emitter, "separate_field", "private_denied");
            let asm = emitter.output();
            let callback = target.extern_symbol("__elephc_eval_dynamic_object_has_separate_property");
            let call = if target.arch == Arch::AArch64 { "bl" } else { "call" };
            assert!(asm.contains(&format!("{call} {callback}")), "{name}");
            assert!(asm.contains("private_denied"), "{name}");
            assert!(asm.contains("separate_field"), "{name}");
        }
    }
}
