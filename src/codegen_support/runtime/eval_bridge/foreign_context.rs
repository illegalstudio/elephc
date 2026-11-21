//! Purpose:
//! Restores the explicitly captured eval arena at generated C wrapper entries.
//!
//! Called from:
//! - `super::label_c_global()` while emitting the full Magician bridge.
//!
//! Key details:
//! - Integer and floating-point C arguments survive the thread-local getter.
//! - Foreign callee-saved context registers are restored after the native body.
//! - Entries outside a scoped eval retain their existing behavior pending the wider audit.

use super::*;

/// Adds a C gateway and leaves the emitter positioned at the original wrapper body.
pub(crate) fn emit_gateway(emitter: &mut Emitter, name: &str) {
    let body = format!("__rt_eval_context_body_{}", name.trim_start_matches("__elephc_"));
    let ready = format!("{body}_ready");
    let integer_count = match emitter.target.arch { Arch::AArch64 => 8, Arch::X86_64 => 6 };
    let stack_count = integer_argument_count(name).saturating_sub(integer_count);
    let stack_bytes = (stack_count * 8).next_multiple_of(16);
    let frame = 160 + stack_bytes;
    abi::emit_frame_prologue(emitter, frame);
    let ctx = crate::codegen_support::runtime::ctx::ctx_reg(emitter);
    abi::store_at_offset(emitter, ctx, 136);
    for index in 0..integer_count {
        let reg = abi::int_arg_reg_name(emitter.target, index);
        abi::store_at_offset(emitter, reg, 8 * (index + 1));
    }
    for index in 0..8 {
        let reg = abi::float_arg_reg_name(emitter.target, index);
        abi::store_at_offset(emitter, reg, 72 + 8 * index);
    }
    let getter = emitter.target.extern_symbol("__elephc_eval_active_runtime_context");
    abi::emit_call_label(emitter, &getter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {ready}"));                   // retain the native caller context outside a scoped Rust eval activation
            emitter.instruction("mov x28, x0");                                 // enter the arena explicitly captured by this thread's eval caller
        },
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // determine whether an explicit eval activation owns this call
            emitter.instruction(&format!("jz {ready}"));                        // retain existing native-entry behavior outside that activation
            emitter.instruction("mov r14, rax");                                // enter this thread's captured arena rather than the foreign register value
        },
    }
    emitter.label(&ready);
    for index in 0..integer_count {
        let reg = abi::int_arg_reg_name(emitter.target, index);
        abi::load_at_offset(emitter, reg, 8 * (index + 1));
    }
    for index in 0..8 {
        let reg = abi::float_arg_reg_name(emitter.target, index);
        abi::load_at_offset(emitter, reg, 72 + 8 * index);
    }
    for index in 0..stack_count {
        match emitter.target.arch {
            Arch::AArch64 => {
                let offset = 16 + index * 8;
                emitter.instruction(&format!("ldr x9, [x29, #{offset}]"));      // borrow one stack argument from the original C caller
                emitter.instruction(&format!("str x9, [sp, #{}]", index * 8));  // forward it at the body's original C argument position
            },
            Arch::X86_64 => {
                emitter.instruction(&format!("mov r10, QWORD PTR [rbp + {}]", 16 + index * 8)); // recover the caller's seventh or later C argument
                emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", index * 8)); // preserve its stack position across the extra gateway call
            },
        }
    }
    abi::emit_call_label(emitter, &body);
    abi::load_at_offset(emitter, ctx, 136);
    abi::emit_frame_restore(emitter, frame);
    abi::emit_return(emitter);
    emitter.label_shared(&body);
}

/// Counts the integer arguments of internal eval C entries that exceed SysV registers.
///
/// These are the native hook prototypes in Magician's `runtime_hooks::externs`, not
/// PHP signatures. Every other gateway entry fits in six C integer registers.
fn integer_argument_count(name: &str) -> usize {
    match name.trim_start_matches('_') {
        "elephc_eval_value_static_property_set" | "elephc_eval_value_method_call" => 7,
        "elephc_eval_value_static_method_call" => 8,
        "elephc_eval_reflection_owner_new" => 17,
        _ => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{AppleVariant, Platform, Target};

    /// Runs the actual gateway with a foreign context and a C getter that clobbers argument registers.
    #[test]
    #[ignore = "requires a native C compiler on a supported executable host"]
    fn eval_foreign_context_gateway_restores_poisoned_native_caller() {
        let target = Target::parse(if cfg!(target_arch = "aarch64") {
            if cfg!(target_os = "macos") { "macos-aarch64" } else { "linux-aarch64" }
        } else { "linux-x86_64" }).unwrap();
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let directory = std::env::temp_dir().join(format!("eval-context-native-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let mut emitter = Emitter::new(target);
        if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
        emitter.raw(".text");
        emitter.label_global(&target.extern_symbol("eval_arena_probe"));
        emit_gateway(&mut emitter, "eval_arena_probe");
        for (index, expected) in [6, 7, 8, 9].into_iter().enumerate() {
            let reg = abi::int_arg_reg_name(target, index);
            if target.arch == Arch::AArch64 {
                emitter.instruction(&format!("cmp {reg}, #{expected}"));        // verify one C integer argument survived the getter
                emitter.instruction("b.ne eval_arena_probe_fail");              // reject an argument clobbered by the foreign getter
            } else {
                emitter.instruction(&format!("cmp {reg}, {expected}"));         // verify one SysV integer argument survived the getter
                emitter.instruction("jne eval_arena_probe_fail");               // reject a corrupted native C argument
            }
        }
        let scratch = if target.arch == Arch::AArch64 { "x9" } else { "r10" };
        abi::emit_load_int_immediate(&mut emitter, scratch, 0x12340000);
        if target.arch == Arch::AArch64 {
            emitter.instruction("cmp x28, x9");                                 // validate the explicitly installed arena instead of the poisoned caller register
            emitter.instruction("b.ne eval_arena_probe_fail");                  // reject entry without captured arena provenance
            emitter.instruction("fmov x9, d0");                                 // read the preserved floating-point C argument as bits
        } else {
            emitter.instruction("cmp r14, r10");                                // validate the getter arena rather than the foreign register
            emitter.instruction("jne eval_arena_probe_fail");                   // reject the wrong active arena
            emitter.instruction("movq r10, xmm0");                              // read the preserved floating-point C argument as bits
        }
        let expected_reg = if target.arch == Arch::AArch64 { "x10" } else { "r11" };
        abi::emit_load_int_immediate(&mut emitter, expected_reg, 3.25_f64.to_bits() as i64);
        emitter.instruction(&format!("cmp {scratch}, {expected_reg}"));         // verify the getter did not destroy the floating-point argument
        emitter.instruction(if target.arch == Arch::AArch64 {
            "b.ne eval_arena_probe_fail"
        } else { "jne eval_arena_probe_fail" });                                // reject changed floating-point argument bits
        let result_reg = abi::int_result_reg(&emitter);
        abi::emit_load_int_immediate(&mut emitter, result_reg, 1);
        abi::emit_return(&mut emitter);
        emitter.label("eval_arena_probe_fail");
        abi::emit_load_int_immediate(&mut emitter, result_reg, 0);
        abi::emit_return(&mut emitter);
        let assembly = directory.join("probe.s");
        let driver = directory.join("driver.c");
        let binary = directory.join("probe");
        std::fs::write(&assembly, emitter.output()).unwrap();
        std::fs::write(&driver, r#"
#include <stdint.h>
extern uintptr_t eval_arena_probe(uintptr_t, uintptr_t, uintptr_t, uintptr_t, double);
void *__elephc_eval_active_runtime_context(void) {
#if defined(__aarch64__)
    __asm__ volatile("mov x1, #0\nmov x2, #0\nmov x3, #0\nfmov d0, xzr" ::: "x1", "x2", "x3", "d0");
#else
    __asm__ volatile("xor %%rdi, %%rdi\nxor %%rsi, %%rsi\nxor %%rdx, %%rdx\nxor %%rcx, %%rcx\npxor %%xmm0, %%xmm0" ::: "rdi", "rsi", "rdx", "rcx", "xmm0");
#endif
    return (void *)(uintptr_t)0x12340000;
}
int main(void) {
#if defined(__aarch64__)
    register uintptr_t foreign __asm__("x28") = 0x4038;
#else
    register uintptr_t foreign __asm__("r14") = 0x4038;
#endif
    __asm__ volatile("" : "+r"(foreign));
    uintptr_t ok = eval_arena_probe(6, 7, 8, 9, 3.25);
    __asm__ volatile("" : "+r"(foreign));
    return ok == 1 && foreign == 0x4038 ? 0 : 1;
}
"#).unwrap();
        let compiled = std::process::Command::new("cc").arg("-O2").arg(&driver).arg(&assembly)
            .arg("-o").arg(&binary).output().unwrap();
        assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
        let ran = std::process::Command::new(&binary).output().unwrap();
        assert!(ran.status.success(), "native gateway lost context or C operands: {ran:?}");
        std::fs::remove_dir_all(directory).unwrap();
    }

    /// Cross-assembles the actual gateway, including the largest existing C stack signature.
    #[test]
    #[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
    fn eval_foreign_context_gateways_assemble_on_all_supported_targets() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let directory = std::env::temp_dir().join(format!("eval-context-targets-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        for (name, triple) in [
            ("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
            ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
            ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
        ] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
            emitter.raw(".text");
            for hook in [
                "__elephc_eval_value_float", "__elephc_eval_value_method_call",
                "__elephc_eval_value_static_method_call", "__elephc_eval_reflection_owner_new",
            ] {
                emitter.label_shared(&target.extern_symbol(hook));
                emit_gateway(&mut emitter, hook);
                abi::emit_return(&mut emitter);
            }
            let source = directory.join(format!("{name}.s"));
            std::fs::write(&source, emitter.output()).unwrap();
            let built = std::process::Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                .arg("-o").arg(source.with_extension("o")).output().unwrap();
            assert!(built.status.success(), "{name}: {}", String::from_utf8_lossy(&built.stderr));
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    /// Adding stack arguments to a hook requires updating the gateway forwarding inventory.
    #[test]
    fn eval_foreign_context_stack_arity_matches_native_hook_prototypes() {
        let hooks = include_str!("../../../../crates/elephc-magician/src/runtime_hooks/externs.rs");
        for declaration in hooks.split("fn __elephc_").skip(1) {
            let (suffix, rest) = declaration.split_once('(').unwrap();
            let args = rest.split_once(')').unwrap().0;
            let count = args.lines().flat_map(|line| line.split(',')).filter(|argument| argument.contains(':')).count();
            if count > 6 {
                let name = format!("__elephc_{suffix}");
                assert_eq!(integer_argument_count(&name), count, "{name}: stack forwarding must agree with the Rust C declaration");
            }
        }
    }

    /// The extra frame forwards every existing stack input at the C body's expected address.
    #[test]
    fn eval_foreign_context_stack_arguments_are_forwarded_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_gateway(&mut emitter, "__elephc_eval_reflection_owner_new");
            let asm = emitter.output();
            let count = 17 - if target.arch == Arch::AArch64 { 8 } else { 6 };
            for index in 0..count {
                let (load, store) = if target.arch == Arch::AArch64 {
                    (format!("ldr x9, [x29, #{}]", 16 + index * 8), format!("str x9, [sp, #{}]", index * 8))
                } else {
                    (format!("mov r10, QWORD PTR [rbp + {}]", 16 + index * 8), format!("mov QWORD PTR [rsp + {}], r10", index * 8))
                };
                assert!(asm.contains(&load) && asm.contains(&store), "{name}: argument {index}");
            }
        }
    }

    /// All supported targets use a captured context and restore the foreign register.
    #[test]
    fn eval_foreign_context_gateway_preserves_c_arguments_on_all_targets() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emitter.eval_foreign_context = true;
            super::super::label_c_global(&mut emitter, "__elephc_eval_value_float");
            let asm = emitter.output();
            let getter = asm.find(&target.extern_symbol("__elephc_eval_active_runtime_context")).unwrap();
            let body_call = asm.find(match target.arch {
                Arch::AArch64 => "bl __rt_eval_context_body_eval_value_float",
                Arch::X86_64 => "call __rt_eval_context_body_eval_value_float",
            }).unwrap();
            assert!(getter < body_call, "{target:?}");
            assert!(!asm.contains("_rt_ctx@") && !asm.contains("_rt_ctx:"), "{target:?}");
            let (save, restore, float_save, float_restore) = match target.arch {
                Arch::AArch64 => ("stur x28, [x29, #-136]", "ldur x28, [x29, #-136]", "stur d0, [x29, #-72]", "ldur d0, [x29, #-72]"),
                Arch::X86_64 => ("mov QWORD PTR [rbp - 136], r14", "mov r14, QWORD PTR [rbp - 136]", "movsd QWORD PTR [rbp - 72], xmm0", "movsd xmm0, QWORD PTR [rbp - 72]"),
            };
            assert!(asm.find(save).unwrap() < getter, "{target:?}");
            assert!(asm.find(restore).unwrap() > body_call, "{target:?}");
            assert!(asm.find(float_save).unwrap() < getter, "{target:?}");
            assert!(asm.find(float_restore).unwrap() > getter, "{target:?}");

            let mut native_only = Emitter::new(target);
            super::super::emit_eval_value_runtime(&mut native_only, RuntimeFeatures::none());
            assert!(!native_only.output().contains("__elephc_eval_active_runtime_context"), "{target:?}: native-only eval must not link Rust TLS");
        }
    }
}
