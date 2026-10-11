//! Purpose:
//! Gives the shared mbstring V6 host target-independent C entry points for native owners.
//!
//! Called from:
//! - Rust live-variable callbacks in the optional elephc-mbstring bridge.
//!
//! Key details:
//! - Internal runtime helpers use different register conventions on AArch64 and x86_64.
//! - These wrappers preserve C linkage and return only after helper ownership changes finish.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Exports uniform C signatures for array COW, boxed values, strings, and release.
pub(super) fn emit(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    for (name, helper) in [
        ("elephc_mbstring_variable_array_unique_v1", "__rt_array_ensure_unique"),
        ("elephc_mbstring_variable_hash_unique_v1", "__rt_hash_ensure_unique"),
        ("elephc_mbstring_variable_reference_new_v1", "__rt_reference_cell_new"),
    ] {
        abi::emit_c_callback_entry(emitter, name);
        emitter.instruction(&format!("{} {helper}", if arm { "b" } else { "jmp" })); // reuse the runtime's C-compatible pointer convention
    }
    abi::emit_c_callback_entry(emitter, "elephc_mbstring_variable_box_v1");
    if arm {
        emitter.instruction("b __rt_mixed_from_value");                         // C and native boxing use the same three argument registers
    } else {
        emitter.instruction("mov rax, rdi");                                    // move the C tag into the native mixed-box tag register
        emitter.instruction("mov rdi, rsi");                                    // move the C low payload into its native register
        emitter.instruction("mov rsi, rdx");                                    // move the C high payload into its native register
        emitter.instruction("jmp __rt_mixed_from_value");                       // let the boxer return its fresh cell directly to Rust
    }
    abi::emit_c_callback_entry(emitter, "elephc_mbstring_variable_clone_v1");
    if arm {
        emitter.instruction("b __rt_mixed_clone");                              // clone a borrowed Mixed value into an independent PHP cell
    } else {
        emitter.instruction("mov rax, rdi");                                    // adapt the first C pointer argument to the native Mixed register
        emitter.instruction("jmp __rt_mixed_clone");                            // transfer the independent cell back through the C ABI
    }
    abi::emit_c_callback_entry(emitter, "elephc_mbstring_variable_persist_v1");
    if arm {
        emitter.instruction("stp x29, x30, [sp, #-16]!");                       // keep C caller linkage across heap-backed string allocation
        emitter.instruction("mov x2, x1");                                      // present the C length as the native string length
        emitter.instruction("mov x1, x0");                                      // present the borrowed C bytes as the native string pointer
        emitter.instruction("bl __rt_str_persist");                             // acquire one independent native string owner
        emitter.instruction("mov x0, x1");                                      // return only the new pointer through the C ABI
        emitter.instruction("ldp x29, x30, [sp], #16");                         // restore caller linkage after persistence
        emitter.instruction("ret");                                             // leave the Rust callback with one owned string
    } else {
        emitter.instruction("mov rax, rdi");                                    // adapt the C byte pointer to the native string register
        emitter.instruction("mov rdx, rsi");                                    // adapt the C length to the native pair
        emitter.instruction("jmp __rt_str_persist");                            // return the new pointer directly through the C ABI
    }
    abi::emit_c_callback_entry(emitter, "elephc_mbstring_variable_release_v1");
    if arm {
        emitter.instruction("b __rt_decref_any");                               // the native release already accepts the first C pointer argument
    } else {
        emitter.instruction("mov rax, rdi");                                    // adapt the C pointer to the native release register
        emitter.instruction("jmp __rt_decref_any");                             // return after the old string, box, or reference owner retires
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Ensures every V7 runtime ownership callback exposes a native C entry and internal body.
    #[test]
    fn windows_variable_runtime_callbacks_adapt_msx64_to_internal_abi() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit(&mut emitter);
        let asm = emitter.output();
        for name in [
            "elephc_mbstring_variable_clone_v1",
            "elephc_mbstring_variable_box_v1",
            "elephc_mbstring_variable_reference_new_v1",
            "elephc_mbstring_variable_release_v1",
            "elephc_mbstring_variable_array_unique_v1",
            "elephc_mbstring_variable_hash_unique_v1",
            "elephc_mbstring_variable_persist_v1",
        ] {
            let body = asm.split(&format!("{name}:\n")).nth(1).expect("public callback entry");
            let internal = format!("{name}__internal:");
            let entry = body.split(&internal).next().expect("callback adapter prefix");
            assert!(entry.contains("mov rdi, rcx"), "{name}: missing argument one adapter");
            assert!(entry.contains("mov rsi, rdx"), "{name}: missing argument two adapter");
            assert!(body.contains(&internal), "{name}: missing internal SysV entry");
        }
    }
}
