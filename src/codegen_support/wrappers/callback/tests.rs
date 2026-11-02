//! Purpose:
//! Regression tests for native callback argument adaptation and temporary ownership cleanup.
//!
//! Called from:
//! - `cargo test` through the Rust test harness for `codegen_support::wrappers::callback`.
//!
//! Key details:
//! - Tests inspect both supported backend architectures so callback cleanup stays ABI-neutral.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Platform, Target};
use crate::codegen_support::DeferredCallbackWrapper;
use crate::types::PhpType;

use super::emit_callback_wrapper;

/// Emits a direct callback wrapper that must adapt one boxed Mixed argument to string.
fn emit_mixed_to_string_wrapper(target: Target) -> String {
    let mut emitter = Emitter::new(target);
    emit_callback_wrapper(
        &mut emitter,
        &DeferredCallbackWrapper {
            label: "test_mixed_to_string_callback_wrapper".to_string(),
            visible_arg_types: vec![PhpType::Mixed],
            target_visible_arg_types: Some(vec![PhpType::Str]),
            capture_types: Vec::new(),
            descriptor_prefix_types: Vec::new(),
            descriptor_return_type: None,
            invocation_scope_class_id: -1,
        },
    );
    emitter.output()
}

/// Verifies ARM64 wrappers retain the converted pointer, free it after the callback,
/// and preserve scalar, string, tagged-scalar, and floating-point return registers.
#[test]
fn test_arm64_mixed_to_string_callback_adapter_releases_conversion() {
    let asm = emit_mixed_to_string_wrapper(Target::new(Platform::MacOS, Arch::AArch64));

    assert!(asm.contains("    bl __rt_mixed_cast_string\n"), "{asm}");
    assert!(asm.contains("    stur x1, [x29, #-32]\n"), "{asm}");
    assert!(asm.contains("    blr x19\n"), "{asm}");
    assert!(asm.contains("    stp x0, x1, [sp, #-16]!\n"), "{asm}");
    assert!(asm.contains("    str x2, [sp, #-16]!\n"), "{asm}");
    assert!(asm.contains("    str d0, [sp, #-16]!\n"), "{asm}");
    assert!(asm.contains("    bl __rt_heap_free_safe\n"), "{asm}");
    assert!(asm.contains("    ldp x0, x1, [sp], #16\n"), "{asm}");
}

/// Verifies x86_64 wrappers retain the converted pointer, free it after the callback,
/// and preserve scalar, string, tagged-scalar, and floating-point return registers.
#[test]
fn test_x86_64_mixed_to_string_callback_adapter_releases_conversion() {
    let asm = emit_mixed_to_string_wrapper(Target::new(Platform::Linux, Arch::X86_64));

    assert!(asm.contains("    call __rt_mixed_cast_string\n"), "{asm}");
    assert!(asm.contains("    mov QWORD PTR [rbp - 32], rax\n"), "{asm}");
    assert!(asm.contains("    call r12\n"), "{asm}");
    assert!(asm.contains("    mov QWORD PTR [rsp], rax\n"), "{asm}");
    assert!(asm.contains("    mov QWORD PTR [rsp + 8], rdx\n"), "{asm}");
    assert!(asm.contains("    movsd QWORD PTR [rsp], xmm0\n"), "{asm}");
    assert!(asm.contains("    call __rt_heap_free_safe\n"), "{asm}");
    assert!(asm.contains("    mov rdx, QWORD PTR [rsp + 8]\n"), "{asm}");
}
