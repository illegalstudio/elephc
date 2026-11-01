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

/// Bucket mask of the eval owner filter: `(identity >> 4) & EVAL_OWNER_FILTER_MASK`. Must match
/// `OWNER_FILTER_BUCKETS - 1` in elephc-magician's `ffi::dynamic_destructors`, which keeps the
/// counts this reads.
pub(crate) const EVAL_OWNER_FILTER_MASK: u64 = 4095;

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

/// Emits the ARM64 wrapper that installs the dynamic object destructor callback and the
/// eval owner filter (x1) that `__rt_eval_object_has_owner` and the destructor path read.
pub(super) fn emit_aarch64_install_dynamic_object_destructor_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_dynamic_object_destructor_hook_v2");
    abi::emit_symbol_address(emitter, "x9", "_elephc_eval_dynamic_object_destruct_fn");
    emitter.instruction("str x0, [x9]");                                        // store the Rust callback pointer for object destruction
    abi::emit_symbol_address(emitter, "x9", "_elephc_eval_owner_filter");
    emitter.instruction("str x1, [x9]");                                        // publish the per-bucket count of eval-owned identities
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
}

/// Emits the ARM64 `__rt_eval_object_has_owner`: x0 = object identity, returns x0 = 1 when an
/// eval context owns it. A zero filter bucket answers without leaving generated code; only a
/// bucket that holds a registered identity asks the registry.
pub(super) fn emit_aarch64_eval_object_has_owner(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: eval_object_has_owner ---");
    emitter.label_global("__rt_eval_object_has_owner");
    abi::emit_symbol_address(emitter, "x9", "_elephc_eval_owner_filter");
    emitter.instruction("ldr x9, [x9]");                                        // x9 = owner filter, published when the first eval context installed its hooks
    emitter.instruction("cbz x9, __rt_eval_object_has_owner_none");             // no context yet means no eval-owned object either
    emitter.instruction("lsr x10, x0, #4");                                     // drop the identity's alignment bits, as the registry does
    emitter.instruction(&format!("and x10, x10, #{}", EVAL_OWNER_FILTER_MASK)); // x10 = filter bucket of this identity
    emitter.instruction("ldr w10, [x9, x10, lsl #2]");                          // w10 = registered identities in that bucket
    emitter.instruction("cbz w10, __rt_eval_object_has_owner_none");            // an empty bucket proves the identity is unregistered
    let owner_probe = emitter.target.extern_symbol("__elephc_eval_object_has_dynamic_owner");
    emitter.instruction(&format!("b {}", owner_probe));                         // a shared bucket still needs the exact registry answer
    emitter.label("__rt_eval_object_has_owner_none");
    emitter.instruction("mov x0, #0");                                          // not eval-owned
    emitter.instruction("ret");                                                 // return to the native dispatch path
}

/// Emits the ARM64 wrapper that installs the eval `Generator` protocol callback.
pub(super) fn emit_aarch64_install_generator_protocol_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_generator_protocol_hook");
    abi::emit_symbol_address(emitter, "x9", "_elephc_eval_generator_protocol_fn");
    emitter.instruction("str x0, [x9]");                                        // store the Rust callback pointer for interpreted generators
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
}

/// Emits the ARM64 C-ABI wrappers that let the INTERPRETER drive a compiled generator.
///
/// `__elephc_eval_install_generator_protocol_hook` is the other direction: it lets compiled code
/// drive an interpreted generator. Both are needed the moment a compiled class and an
/// eval-declared subclass each own one end of a `yield from` -- `twig/twig`'s `Template::yield()`
/// is compiled while the template's `doDisplay()` is interpreted, and each delegates to the other.
///
/// The `__rt_gen_*` helpers are emitted with a plain assembler label rather than a C symbol, so
/// they cannot be named from Rust portably (Mach-O would ask for one underscore too many). These
/// wrappers give them a C name; each is a tail call, so the ABI is the helper's own.
pub(super) fn emit_aarch64_eval_generator_drive(emitter: &mut Emitter) {
    for (wrapper, helper) in EVAL_GENERATOR_DRIVE_WRAPPERS {
        label_c_global(emitter, wrapper);
        emitter.instruction(&format!("b {}", helper));                          // tail-call the runtime helper, keeping its ABI
    }
}

/// Emits the x86_64 C-ABI wrappers that let the INTERPRETER drive a compiled generator.
pub(super) fn emit_x86_64_eval_generator_drive(emitter: &mut Emitter) {
    for (wrapper, helper) in EVAL_GENERATOR_DRIVE_WRAPPERS {
        label_c_global(emitter, wrapper);
        emitter.instruction(&format!("jmp {}", helper));                        // tail-call the runtime helper, keeping its ABI
    }
}

/// The C name each generator helper is exposed under, paired with the helper itself.
const EVAL_GENERATOR_DRIVE_WRAPPERS: &[(&str, &str)] = &[
    ("__elephc_eval_gen_valid", "__rt_gen_valid"),
    ("__elephc_eval_gen_current", "__rt_gen_current"),
    ("__elephc_eval_gen_key", "__rt_gen_key"),
    ("__elephc_eval_gen_next", "__rt_gen_next"),
    ("__elephc_eval_gen_get_return", "__rt_gen_get_return"),
];

/// Emits the ARM64 wrapper that installs the eval class-autoload callback.
pub(super) fn emit_aarch64_install_class_autoload_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_class_autoload_hook");
    abi::emit_symbol_address(emitter, "x9", "_elephc_eval_class_autoload_fn");
    emitter.instruction("str x0, [x9]");                                        // store the Rust callback pointer for class autoloading
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
}

/// Emits the ARM64 wrapper that installs the eval unserialize-object callback.
pub(super) fn emit_aarch64_install_unserialize_object_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_unserialize_object_hook");
    abi::emit_symbol_address(emitter, "x9", "_elephc_eval_unserialize_object_fn");
    emitter.instruction("str x0, [x9]");                                        // store the Rust callback pointer for eval-declared hydration
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
}

/// Emits the ARM64 wrapper that installs the eval object class-relation callback.
pub(super) fn emit_aarch64_install_object_relation_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_object_relation_hook");
    abi::emit_symbol_address(emitter, "x9", "_elephc_eval_object_relation_fn");
    emitter.instruction("str x0, [x9]");                                        // store the Rust callback pointer for eval class relations
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
}

/// Emits the ARM64 wrapper that installs the compiled-clone adoption callback.
pub(super) fn emit_aarch64_install_object_clone_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_object_clone_hook");
    abi::emit_symbol_address(emitter, "x9", "_elephc_eval_object_clone_adopt_fn");
    emitter.instruction("str x0, [x9]");                                        // store the Rust callback pointer for compiled clone adoption
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
}

/// Emits the ARM64 wrapper that installs the eval `ArrayAccess` read callback.
pub(super) fn emit_aarch64_install_array_access_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_array_access_hook");
    abi::emit_symbol_address(emitter, "x9", "_elephc_eval_array_access_fn");
    emitter.instruction("str x0, [x9]");                                        // store the Rust callback pointer for eval ArrayAccess reads
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
}

/// Emits the ARM64 wrapper that installs the eval serialize-object callback.
pub(super) fn emit_aarch64_install_serialize_object_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_serialize_object_hook");
    abi::emit_symbol_address(emitter, "x9", "_elephc_eval_serialize_object_fn");
    emitter.instruction("str x0, [x9]");                                        // store the Rust callback pointer for eval object rendering
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
}

/// Emits the ARM64 wrapper that installs the eval Throwable-trace callback.
pub(super) fn emit_aarch64_install_throwable_trace_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_throwable_trace_hook");
    abi::emit_symbol_address(emitter, "x9", "_elephc_eval_throwable_trace_fn");
    emitter.instruction("str x0, [x9]");                                        // store the Rust callback pointer for Throwable traces
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

/// Emits the x86_64 wrapper that installs the dynamic object destructor callback and the
/// eval owner filter (rsi) that `__rt_eval_object_has_owner` and the destructor path read.
pub(super) fn emit_x86_64_install_dynamic_object_destructor_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_dynamic_object_destructor_hook_v2");
    abi::emit_symbol_address(emitter, "r10", "_elephc_eval_dynamic_object_destruct_fn");
    emitter.instruction("mov QWORD PTR [r10], rdi");                            // store the Rust callback pointer for object destruction
    abi::emit_symbol_address(emitter, "r10", "_elephc_eval_owner_filter");
    emitter.instruction("mov QWORD PTR [r10], rsi");                            // publish the per-bucket count of eval-owned identities
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
    emit_install_object_owner_hooks(emitter);
    emit_install_object_clone_hook(emitter);
    emit_install_closure_bind_hook(emitter);
}

/// Installs the C callback `__rt_closure_bind` rebinds an eval closure's `$this` through.
fn emit_install_closure_bind_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_closure_bind_hook_v1");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", "_elephc_eval_closure_bind_fn");
            emitter.instruction("str x0, [x9]");                                // store the eval closure rebinding callback
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r10", "_elephc_eval_closure_bind_fn");
            emitter.instruction("mov QWORD PTR [r10], rdi");                    // store the eval closure rebinding callback
        }
    }
    emitter.instruction("ret");                                                 // return after installing the optional eval bind hook
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

/// Emits the x86_64 `__rt_eval_object_has_owner`: rdi = object identity, returns rax = 1 when
/// an eval context owns it. A zero filter bucket answers without leaving generated code; only a
/// bucket that holds a registered identity asks the registry.
pub(super) fn emit_x86_64_eval_object_has_owner(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: eval_object_has_owner ---");
    emitter.label_global("__rt_eval_object_has_owner");
    abi::emit_symbol_address(emitter, "r10", "_elephc_eval_owner_filter");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // r10 = owner filter, published when the first eval context installed its hooks
    emitter.instruction("test r10, r10");                                       // has any eval context installed its hooks?
    emitter.instruction("jz __rt_eval_object_has_owner_none");                  // no context yet means no eval-owned object either
    emitter.instruction("mov r11, rdi");                                        // copy the identity to derive its bucket
    emitter.instruction("shr r11, 4");                                          // drop the identity's alignment bits, as the registry does
    emitter.instruction(&format!("and r11, {}", EVAL_OWNER_FILTER_MASK));       // r11 = filter bucket of this identity
    emitter.instruction("mov eax, DWORD PTR [r10 + r11 * 4]");                  // eax = registered identities in that bucket
    emitter.instruction("test eax, eax");                                       // is the bucket empty?
    emitter.instruction("jz __rt_eval_object_has_owner_none");                  // an empty bucket proves the identity is unregistered
    let owner_probe = emitter.target.extern_symbol("__elephc_eval_object_has_dynamic_owner");
    emitter.instruction(&format!("jmp {}", owner_probe));                       // a shared bucket still needs the exact registry answer
    emitter.label("__rt_eval_object_has_owner_none");
    emitter.instruction("xor eax, eax");                                        // not eval-owned
    emitter.instruction("ret");                                                 // return to the native dispatch path
}

/// Emits the x86_64 wrapper that installs the eval `Generator` protocol callback.
pub(super) fn emit_x86_64_install_generator_protocol_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_generator_protocol_hook");
    abi::emit_symbol_address(emitter, "r10", "_elephc_eval_generator_protocol_fn");
    emitter.instruction("mov QWORD PTR [r10], rdi");                            // store the Rust callback pointer for interpreted generators
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
}

/// Emits the x86_64 wrapper that installs the eval class-autoload callback.
pub(super) fn emit_x86_64_install_class_autoload_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_class_autoload_hook");
    abi::emit_symbol_address(emitter, "r10", "_elephc_eval_class_autoload_fn");
    emitter.instruction("mov QWORD PTR [r10], rdi");                            // store the Rust callback pointer for class autoloading
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
}

/// Emits the x86_64 wrapper that installs the eval unserialize-object callback.
pub(super) fn emit_x86_64_install_unserialize_object_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_unserialize_object_hook");
    abi::emit_symbol_address(emitter, "r10", "_elephc_eval_unserialize_object_fn");
    emitter.instruction("mov QWORD PTR [r10], rdi");                            // store the Rust callback pointer for eval-declared hydration
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
}

/// Emits the x86_64 wrapper that installs the eval object class-relation callback.
pub(super) fn emit_x86_64_install_object_relation_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_object_relation_hook");
    abi::emit_symbol_address(emitter, "r10", "_elephc_eval_object_relation_fn");
    emitter.instruction("mov QWORD PTR [r10], rdi");                            // store the Rust callback pointer for eval class relations
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
}

/// Emits the x86_64 wrapper that installs the compiled-clone adoption callback.
pub(super) fn emit_x86_64_install_object_clone_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_object_clone_hook");
    abi::emit_symbol_address(emitter, "r10", "_elephc_eval_object_clone_adopt_fn");
    emitter.instruction("mov QWORD PTR [r10], rdi");                            // store the Rust callback pointer for compiled clone adoption
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
}

/// Emits the x86_64 wrapper that installs the eval `ArrayAccess` read callback.
pub(super) fn emit_x86_64_install_array_access_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_array_access_hook");
    abi::emit_symbol_address(emitter, "r10", "_elephc_eval_array_access_fn");
    emitter.instruction("mov QWORD PTR [r10], rdi");                            // store the Rust callback pointer for eval ArrayAccess reads
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
}

/// Emits the x86_64 wrapper that installs the eval serialize-object callback.
pub(super) fn emit_x86_64_install_serialize_object_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_serialize_object_hook");
    abi::emit_symbol_address(emitter, "r10", "_elephc_eval_serialize_object_fn");
    emitter.instruction("mov QWORD PTR [r10], rdi");                            // store the Rust callback pointer for eval object rendering
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
}

/// Emits the x86_64 wrapper that installs the eval Throwable-trace callback.
pub(super) fn emit_x86_64_install_throwable_trace_hook(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_install_throwable_trace_hook");
    abi::emit_symbol_address(emitter, "r10", "_elephc_eval_throwable_trace_fn");
    emitter.instruction("mov QWORD PTR [r10], rdi");                            // store the Rust callback pointer for Throwable traces
    emitter.instruction("ret");                                                 // return after installing the optional eval hook
}

/// Emits the ARM64 wrapper that records the live interpreter frames into a Throwable that eval
/// created through the generated allocator.
///
/// Input is a boxed Mixed object cell (C ABI `x0`). A compiled `new` records the trace itself, but
/// an interpreted `new Exception(...)` allocates through `__elephc_eval_value_new_object` and
/// would otherwise keep the declared empty trace. Non-object cells are ignored.
pub(super) fn emit_aarch64_capture_throwable_trace_wrapper(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_capture_throwable_trace");
    emitter.instruction("sub sp, sp, #16");                                     // reserve the wrapper frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save the frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the wrapper frame pointer
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0 = tag, x1 = object payload
    emitter.instruction("cmp x0, #6");                                          // only an object cell can be a Throwable
    emitter.instruction("b.ne __elephc_eval_value_capture_throwable_trace_done"); // ignore every other tag
    emitter.instruction("mov x0, x1");                                          // the trace helper takes the raw object pointer
    emitter.instruction("bl __rt_throwable_capture_trace");                     // record the frames live right now into the trace slot
    emitter.label("__elephc_eval_value_capture_throwable_trace_done");
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the wrapper frame
    emitter.instruction("ret");                                                 // return to Rust
}

/// Emits the x86_64 wrapper that records the live interpreter frames into an eval-created Throwable.
pub(super) fn emit_x86_64_capture_throwable_trace_wrapper(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_capture_throwable_trace");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the wrapper frame pointer
    emitter.instruction("mov rax, rdi");                                        // pass the boxed object cell in the runtime's Mixed register
    emitter.instruction("call __rt_mixed_unbox");                               // rax = tag, rdi = object payload
    emitter.instruction("cmp rax, 6");                                          // only an object cell can be a Throwable
    emitter.instruction("jne __elephc_eval_value_capture_throwable_trace_done"); // ignore every other tag
    emitter.instruction("mov rax, rdi");                                        // the trace helper takes the raw object pointer in rax
    emitter.instruction("call __rt_throwable_capture_trace");                   // record the frames live right now into the trace slot
    emitter.label("__elephc_eval_value_capture_throwable_trace_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to Rust
}
