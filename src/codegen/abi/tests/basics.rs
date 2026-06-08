//! Purpose:
//! Contains ABI regression tests for basics helper behavior.
//! Checks emitted assembly fragments rather than running linked programs.
//!
//! Called from:
//! - `crate::codegen::abi::tests` through Rust test harness
//!
//! Key details:
//! - Assertions pin register, stack, relocation, and platform-specific instruction choices.

use super::*;

/// Tests frame setup and teardown for a small frame (64 bytes).
/// Verifies that the prologue allocates 64 bytes, saves FP/LR at sp+#48,
/// sets up x29 as the frame pointer, and that restore/return undo this correctly.
#[test]
fn test_emit_frame_helpers_small_frame() {
    let mut emitter = test_emitter();
    emit_frame_prologue(&mut emitter, 64);
    emit_frame_restore(&mut emitter, 64);
    emit_return(&mut emitter);

    assert_eq!(
        emitter.output(),
        concat!(
            "    ; prologue\n",
            "    sub sp, sp, #64\n",
            "    stp x29, x30, [sp, #48]\n",
            "    add x29, sp, #48\n",
            "    ldp x29, x30, [sp, #48]\n",
            "    add sp, sp, #64\n",
            "    ret\n",
        )
    );
}

/// Verifies the frame prologue rejects a frame too small to hold the x29/x30 footer
/// (`frame_size < 16`) with a clear assertion message in debug builds, instead of
/// underflowing the `frame_size - 16` footer-offset subtraction into a corrupt offset.
#[test]
#[should_panic(expected = "frame_size must reserve the 16-byte frame footer")]
fn test_emit_frame_prologue_rejects_undersized_frame() {
    let mut emitter = test_emitter();
    emit_frame_prologue(&mut emitter, 8);
}

/// Tests that string return values (pointer in x1, length in x2) are preserved
/// across function boundaries by storing them to the caller's stack frame at negative
/// offsets and restoring them after the call. Uses offset 32 for both stores.
#[test]
fn test_emit_preserve_and_restore_return_value_for_strings() {
    let mut emitter = test_emitter();
    emit_preserve_return_value(&mut emitter, &PhpType::Str, 32);
    emit_restore_return_value(&mut emitter, &PhpType::Str, 32);

    assert_eq!(
        emitter.output(),
        concat!(
            "    stur x1, [x29, #-32]\n",
            "    stur x2, [x29, #-24]\n",
            "    ldur x1, [x29, #-32]\n",
            "    ldur x2, [x29, #-24]\n",
        )
    );
}
