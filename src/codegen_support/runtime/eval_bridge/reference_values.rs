//! Purpose:
//! Exposes persistent reference storage and detached value copies through the eval C ABI.
//!
//! Called from:
//! - Eval bridge emission for the active target architecture.
//!
//! Key details:
//! - References own GC-traced Mixed values independently of interpreter scopes.
//! - Replacement returns the previous owner so Rust controls when cleanup occurs.

use super::*;

/// Emits narrow C adapters over the target-aware shared reference runtime helpers.
pub(super) fn emit(emitter: &mut Emitter) {
    for (name, helper) in [
        ("__elephc_eval_value_reference_new", "__rt_reference_new"),
        ("__elephc_eval_value_is_reference", "__rt_reference_is"),
        ("__elephc_eval_value_copy", "__rt_mixed_clone"),
    ] {
        label_c_global(emitter, name);
        if emitter.target.arch == Arch::AArch64 {
            emitter.instruction(&format!("b {helper}"));                        // tail-call with the existing AArch64 boxed argument
        } else {
            emitter.instruction("mov rax, rdi");                                // adapt the borrowed C argument to the native boxed-value convention
            emitter.instruction(&format!("jmp {helper}"));                      // transfer directly to the shared runtime implementation
        }
    }
    label_c_global(emitter, "__elephc_eval_value_reference_is_shared");
    crate::codegen_support::abi::emit_jump(emitter, "__rt_reference_is_shared");
    emit_retain_value_cell(emitter);
    label_c_global(emitter, "__elephc_eval_value_reference_replace");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("b __rt_reference_replace");                        // return the previous owned value using the unchanged two-argument convention
    } else {
        emitter.instruction("mov rax, rdi");                                    // pass the persistent reference identity through the internal value register
        emitter.instruction("mov rdi, rsi");                                    // pass the borrowed replacement value through the internal second argument
        emitter.instruction("jmp __rt_reference_replace");                      // transfer the previous owner back to Rust for explicit cleanup
    }
}

/// Pins a reference's current child through the target-aware internal Mixed calling convention.
fn emit_retain_value_cell(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_retain_value_cell_v1");
    abi::emit_frame_prologue(emitter, 16);
    let result = abi::int_result_reg(emitter);
    let argument = abi::int_arg_reg_name(emitter.target, 0);
    abi::emit_reg_move(emitter, result, argument);
    abi::emit_call_label(emitter, "__rt_mixed_deref");
    abi::emit_call_label(emitter, "__rt_incref");
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// All targets adapt the C argument before dereferencing and retaining the same value cell.
    #[test]
    fn eval_reference_value_pin_uses_live_identity_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            let symbol = emitter.target.extern_symbol("__elephc_eval_value_retain_value_cell_v1");
            emit_retain_value_cell(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains(&symbol), "{name}: {asm}");
            assert!(asm.find("__rt_mixed_deref").unwrap() < asm.find("__rt_incref").unwrap(), "{name}: {asm}");
            assert!(!asm.contains("__rt_mixed_clone"), "{name}: a value copy erases storage identity");
            if name.ends_with("x86_64") { assert!(asm.contains("mov rax, rdi"), "{name}: {asm}"); }
        }
    }
}
