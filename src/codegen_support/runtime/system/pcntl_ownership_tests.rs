//! Purpose:
//! Verifies signal callback arguments remain visible to exceptional cleanup on every target.
//!
//! Called from:
//! - Unit coverage for the shared PCNTL runtime emitter.
//!
//! Key details:
//! - The boxed argument array owns the signal and siginfo graph throughout PHP invocation.
//! - Normal cleanup detaches that same record before deeply releasing the arguments.

use crate::codegen_support::{emit::Emitter, platform::{Arch, Target}};

/// Signal callback adapters publish and retire their argument graph across all supported ABIs.
#[test]
fn signal_callback_argument_owners_cover_unwinding_on_every_target() {
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut emitter = Emitter::new(Target::parse(target).unwrap());
        let arm = emitter.target.arch == Arch::AArch64;
        super::emit_pcntl_signal_dispatch(&mut emitter);
        let asm = emitter.output();
        let adapter = asm.split_once("__rt_pcntl_invoke_descriptor:").unwrap().1
            .split_once("__rt_pcntl_dispatch_pending:").unwrap().0;
        let publish = adapter.find("publish temporary call operand owner").unwrap();
        let invoke = adapter.find(if arm { "blr x10" } else { "call r11" }).unwrap();
        let detach = adapter.find("detach temporary call operand owner").unwrap();
        let release = adapter[detach..].find("__rt_decref_any").unwrap() + detach;
        assert!(publish < invoke && invoke < detach && detach < release, "{target}");
        assert!(adapter[..invoke].contains("__rt_cleanup_call_operand_owner"), "{target}");
        assert!(adapter[..invoke].contains("__rt_gc_operand_frame_owners"), "{target}");
        if arm {
            assert!(adapter[publish..invoke].contains("ldr x9, [sp, #48]"), "{target}");
            assert!(adapter[publish..invoke].contains("ldr x1, [sp, #96]"), "{target}");
        } else {
            assert!(adapter[..publish].contains("lea r11, [rbp - 56]"), "{target}");
            assert!(adapter[publish..invoke].contains("mov rsi, QWORD PTR [rbp - 56]"), "{target}");
        }
    }
}
