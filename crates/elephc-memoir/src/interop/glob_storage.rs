//! Purpose:
//! Owns the native glob directory envelope and the iterator prefix before libc storage.
//!
//! Called from:
//! - Directory-glob resource construction.
//!
//! Key details:
//! - The frame and libc field offset are supplied by the target-aware caller.
//! - libc remains the owner of the path vector inside the embedded glob state.

use crate::{Arch, Emitter};

/// Allocates an opaque glob envelope for the native frame.
pub fn emit_allocate(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #160");                                // 160 bytes covers struct + glob_t on both platforms
            emitter.instruction("bl __rt_heap_alloc");                          // x0 = struct pointer
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, 160");                                // 160 bytes covers struct + glob_t
            emitter.instruction("call __rt_heap_alloc");                        // rax = struct pointer
        }
    }
}

/// Initializes the native iteration prefix after libc has filled its own structure.
pub fn emit_initialize(emitter: &mut impl Emitter, pathv_offset: usize) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp, #40]");                           // struct pointer
            emitter.instruction("ldr x10, [x9, #24]");                          // gl_pathc lives at glob_t offset 0
            emitter.instruction("str x10, [x9, #8]");                           // struct.pathc = gl_pathc
            emitter.instruction("add x11, x9, #24");                            // &struct.glob_t
            emitter.instruction(&format!("ldr x12, [x11, #{}]", pathv_offset)); // gl_pathv at the platform-specific offset
            emitter.instruction("str x12, [x9, #0]");                           // struct.pathv = gl_pathv
            emitter.instruction("str xzr, [x9, #16]");                          // struct.index = 0
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                // struct pointer
            emitter.instruction("mov r10, QWORD PTR [r9 + 24]");                // gl_pathc lives at glob_t offset 0
            emitter.instruction("mov QWORD PTR [r9 + 8], r10");                 // struct.pathc = gl_pathc
            emitter.instruction(&format!("mov r10, QWORD PTR [r9 + 24 + {}]", pathv_offset)); // gl_pathv at the platform-specific offset
            emitter.instruction("mov QWORD PTR [r9 + 0], r10");                 // struct.pathv = gl_pathv
            emitter.instruction("mov QWORD PTR [r9 + 16], 0");                  // struct.index = 0
        }
    }
}
