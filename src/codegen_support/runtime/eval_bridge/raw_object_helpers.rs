//! Purpose:
//! Boxes raw objects and installs dynamic-object destructor hooks.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Both supported architectures expose the same bridge callbacks.
//! - The v2 destructor installer rejects archives using the legacy callback ABI at link time.
//! - The v1 clone installer fills the slot the generated `clone` lowering consults before its
//!   own shallow-clone adapter, so an eval-declared object is cloned by Magician instead.

use super::*;

/// Emits the ARM64 wrapper that boxes a borrowed raw object pointer for Rust eval.
pub(super) fn emit_aarch64_object_from_raw_wrapper(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_object_from_raw");
    emitter.instruction("cbz x0, __elephc_eval_value_object_from_raw_null");    // null raw object pointers become PHP null cells
    emitter.instruction("mov x1, x0");                                          // move the raw object pointer into the Mixed payload
    emitter.instruction("mov x0, #6");                                          // runtime tag 6 = object
    emitter.instruction("mov x2, xzr");                                         // object payloads do not use a high word
    emitter.instruction("b __rt_mixed_from_value");                             // box and retain the borrowed object for eval
    emitter.label("__elephc_eval_value_object_from_raw_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null
    emitter.instruction("mov x1, xzr");                                         // null has no low payload word
    emitter.instruction("mov x2, xzr");                                         // null has no high payload word
    emitter.instruction("b __rt_mixed_from_value");                             // box the null payload and return to Rust
}

/// Emits the ARM64 wrapper that installs the dynamic object destructor callback.
pub(super) fn emit_aarch64_install_dynamic_object_destructor_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_dynamic_object_destructor_hook_v2");
    abi::emit_symbol_address(emitter, "x9", "_elephc_eval_dynamic_object_destruct_fn");
    emitter.instruction("str x0, [x9]");                                        // store the Rust callback pointer for object destruction
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
    emit_install_object_owner_hooks(emitter);
    emit_install_object_clone_hook(emitter);
    emit_install_closure_bind_hook(emitter);
}

/// Emits the x86_64 wrapper that boxes a borrowed raw object pointer for Rust eval.
pub(super) fn emit_x86_64_object_from_raw_wrapper(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_object_from_raw");
    emitter.instruction("test rdi, rdi");                                       // null raw object pointers become PHP null cells
    emitter.instruction("jz __elephc_eval_value_object_from_raw_null_x86");     // branch to null boxing for missing object payloads
    emitter.instruction("mov eax, 6");                                          // runtime tag 6 = object
    emitter.instruction("xor esi, esi");                                        // object payloads do not use a high word
    emitter.instruction("jmp __rt_mixed_from_value");                           // box and retain the borrowed object for eval
    emitter.label("__elephc_eval_value_object_from_raw_null_x86");
    emitter.instruction("mov eax, 8");                                          // runtime tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null has no low payload word
    emitter.instruction("xor esi, esi");                                        // null has no high payload word
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the null payload and return to Rust
}

/// Emits the x86_64 wrapper that installs the dynamic object destructor callback.
pub(super) fn emit_x86_64_install_dynamic_object_destructor_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_dynamic_object_destructor_hook_v2");
    abi::emit_symbol_address(emitter, "r10", "_elephc_eval_dynamic_object_destruct_fn");
    emitter.instruction("mov QWORD PTR [r10], rdi");                            // store the Rust callback pointer for object destruction
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
    emit_install_object_owner_hooks(emitter);
    emit_install_object_clone_hook(emitter);
    emit_install_closure_bind_hook(emitter);
}

/// Installs distinct four-argument legacy and five-argument context-aware rebinding hooks.
/// Installing either version disables the other, including when the supplied pointer is null.
fn emit_install_closure_bind_hook(emitter: &mut Emitter) {
    for (entry, slot, other) in [
        ("__elephc_eval_install_closure_bind_hook_v1", "_elephc_eval_closure_bind_fn", "_elephc_eval_closure_bind_ctx_fn"),
        ("__elephc_eval_install_closure_bind_hook_v2", "_elephc_eval_closure_bind_ctx_fn", "_elephc_eval_closure_bind_fn"),
    ] {
        label_c_global(emitter, entry);
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_symbol_address(emitter, "x9", slot);
                emitter.instruction("str x0, [x9]");                            // publish a callback with this entry's exact C signature
            }
            Arch::X86_64 => {
                abi::emit_symbol_address(emitter, "r10", slot);
                emitter.instruction("mov QWORD PTR [r10], rdi");                // publish the versioned rebinding callback
            }
        }
        abi::emit_store_zero_to_symbol(emitter, other, 0);
        abi::emit_return(emitter);
    }
}

/// Installs C callbacks whose final-release hook returns an owned Throwable box or null.
fn emit_install_object_owner_hooks(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_object_owner_hooks_v2");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", "_elephc_eval_object_gc_child_fn");
            emitter.instruction("str x0, [x9]");                                // store the child enumeration callback
            abi::emit_symbol_address(emitter, "x9", "_elephc_eval_object_release_fn");
            emitter.instruction("str x1, [x9]");                                // store the final ownership release callback
            abi::emit_symbol_address(emitter, "x9", "_elephc_eval_array_reference_retire_fn");
            emitter.instruction("str x2, [x9]");                                // store the boxed array-reference retirement callback
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r10", "_elephc_eval_object_gc_child_fn");
            emitter.instruction("mov QWORD PTR [r10], rdi");                    // store the child enumeration callback
            abi::emit_symbol_address(emitter, "r10", "_elephc_eval_object_release_fn");
            emitter.instruction("mov QWORD PTR [r10], rsi");                    // store the final ownership release callback
            abi::emit_symbol_address(emitter, "r10", "_elephc_eval_array_reference_retire_fn");
            emitter.instruction("mov QWORD PTR [r10], rdx");                    // store the boxed array-reference retirement callback
        }
    }
    emitter.instruction("ret");                                                 // return after installing all optional ownership callbacks
}

/// Installs the C callback a generated `clone` offers every Mixed object to before cloning it.
fn emit_install_object_clone_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_object_clone_hook_v1");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", "_elephc_eval_object_clone_fn");
            emitter.instruction("str x0, [x9]");                                // store the eval dynamic-object clone callback
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r10", "_elephc_eval_object_clone_fn");
            emitter.instruction("mov QWORD PTR [r10], rdi");                    // store the eval dynamic-object clone callback
        }
    }
    emitter.instruction("ret");                                                 // return after installing the optional eval clone hook
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keeps installer ABI versions distinct and disables stale hooks when replacing a version.
    #[test]
    fn post_barrier_bind_installers_clear_the_other_signature_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = crate::codegen_support::platform::Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_install_closure_bind_hook(&mut emitter);
            let asm = emitter.output();
            let v1 = format!("{}:", target.extern_symbol("__elephc_eval_install_closure_bind_hook_v1"));
            let v2 = format!("{}:", target.extern_symbol("__elephc_eval_install_closure_bind_hook_v2"));
            let legacy = asm.split_once(&v1).unwrap().1.split_once(&v2).unwrap().0;
            let context = asm.split_once(&v2).unwrap().1;
            for (body, retired_slot) in [
                (legacy, "_elephc_eval_closure_bind_ctx_fn"),
                (context, "_elephc_eval_closure_bind_fn"),
            ] {
                let mut expected = Emitter::new(target);
                abi::emit_store_zero_to_symbol(&mut expected, retired_slot, 0);
                assert!(body.contains(expected.output().trim()), "{name}: replacing a hook must retire the other ABI, including for null");
            }
        }
    }

    /// Callback installers retain platform symbol mangling and all three owner hooks on every target.
    #[test]
    fn eval_object_owner_installer_has_all_target_c_abi_symbols() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = crate::codegen_support::platform::Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            match target.arch {
                Arch::AArch64 => emit_aarch64_install_dynamic_object_destructor_hook(&mut emitter),
                Arch::X86_64 => emit_x86_64_install_dynamic_object_destructor_hook(&mut emitter),
            }
            let output = emitter.output();
            let destructor = target.extern_symbol("__elephc_eval_install_dynamic_object_destructor_hook_v2");
            assert_eq!(output.matches(&format!("{destructor}:")).count(), 1, "{name}");
            let legacy = target.extern_symbol("__elephc_eval_install_dynamic_object_destructor_hook");
            assert!(!output.contains(&format!("{legacy}:")), "{name}");
            let symbol = target.extern_symbol("__elephc_eval_install_object_owner_hooks_v2");
            assert_eq!(output.matches(&format!("{symbol}:")).count(), 1, "{name}");
            assert!(output.contains("_elephc_eval_object_gc_child_fn"), "{name}");
            assert!(output.contains("_elephc_eval_object_release_fn"), "{name}");
            assert!(output.contains("_elephc_eval_array_reference_retire_fn"), "{name}");
            let clone_installer = target.extern_symbol("__elephc_eval_install_object_clone_hook_v1");
            assert_eq!(output.matches(&format!("{clone_installer}:")).count(), 1, "{name}");
            assert!(output.contains("_elephc_eval_object_clone_fn"), "{name}");
            let third_arg_store = match target.arch {
                Arch::AArch64 => "str x2, [x9]",
                Arch::X86_64 => "mov QWORD PTR [r10], rdx",
            };
            assert!(output.contains(third_arg_store), "{name}");
        }
    }
}
