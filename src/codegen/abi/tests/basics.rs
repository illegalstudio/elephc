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
