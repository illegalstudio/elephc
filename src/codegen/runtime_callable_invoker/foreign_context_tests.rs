//! Purpose:
//! Verifies native descriptors entered by Rust install the captured eval arena.
//!
//! Called from:
//! - The compiler library's focused descriptor-invoker tests.
//!
//! Key details:
//! - ABI v4 arguments and foreign callee-saved context registers must survive the gateway.
//! - Ordinary native invokers must not acquire a Magician/TLS dependency.

use super::*;

/// Protects the first context access on every target without wrapping native-only invokers.
#[test]
fn rust_descriptor_entry_uses_explicit_eval_context_on_all_targets() {
    let sig = crate::types::first_class_callable_builtin_sig("trim").unwrap();
    let defaults: InvokerDefaults = vec![None; sig.params.len()];
    let invoker = RuntimeCallableInvoker {
        label: "eval_context_probe", sig: &sig, captures: &[],
        mbstring_operation: None, mbstring_variable_ref_warnings: false,
        parallel_fiber_suspend_guard: None, owns_string_return: false,
        php_return_status: false, defaults: &defaults,
    };
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for foreign in [false, true] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            let mut data = DataSection::new();
            emit_runtime_callable_invoker_impl(&mut emitter, &mut data, &invoker, foreign);
            let arch = emitter.target.arch;
            let asm = emitter.output();
            let getter = asm.find("__elephc_eval_active_runtime_context");
            assert_eq!(getter.is_some(), foreign, "{name}: only Rust-entry invokers need an eval gateway");
            if foreign {
                let body = asm.find("__rt_eval_context_body_eval_context_probe:").unwrap();
                assert!(getter.unwrap() < body, "{name}: install context before the ownership/binding body");
                assert!(asm.contains(if arch == Arch::AArch64 {
                    "mov x28, x0"
                } else { "mov r14, rax" }), "{name}");
                assert!(asm[..body].contains(if arch == Arch::AArch64 {
                    "ldur x28, [x29, #-136]"
                } else { "mov r14, QWORD PTR [rbp - 136]" }), "{name}: restore the foreign caller context");
            }
        }
    }
}
