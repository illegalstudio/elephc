//! Purpose:
//! Copies raw bytes into a separate C export owner and releases any runtime source owner.
//!
//! Called from:
//! - The compiler's owned-string export boundary.
//!
//! Key details:
//! - The byte count governs the copy, including embedded NULs.
//! - The trailing NUL is a convenience for the native caller.
//! - Borrowed host inputs must survive aliased PHP results.

use crate::{abi, Arch, Emitter};

/// Compiler-selected native spill slots for exporting one byte slice.
pub struct Slots {
    /// Offset of the source pointer.
    pub source: usize,
    /// Offset of the source byte length.
    pub length: usize,
    /// Offset that receives the new C owner.
    pub destination: usize,
}

/// Copies the staged byte slice into an independent native owner with a trailing NUL.
pub fn emit_copy(emitter: &mut impl Emitter, slots: Slots, copy: &str, copied: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            abi::load_at_offset(emitter, "x0", slots.length);
            emitter.instruction("add x0, x0, #1");                              // include one byte for the optional trailing NUL
            emitter.instruction("bl __rt_heap_alloc");                          // allocate caller-owned result storage
            abi::store_at_offset(emitter, "x0", slots.destination);
            abi::load_at_offset(emitter, "x9", slots.source);
            abi::load_at_offset(emitter, "x10", slots.length);
            emitter.instruction("mov x11, x0");                                 // retain the owned output base across the copy loop
            emitter.instruction("mov x12, #0");                                 // initialize the binary copy cursor
            emitter.label(copy);
            emitter.instruction("cmp x12, x10");                                // compare the cursor with the authoritative byte length
            emitter.instruction(&format!("b.hs {}", copied));                   // finish after every result byte is copied
            emitter.instruction("ldrb w13, [x9, x12]");                         // load one binary result byte
            emitter.instruction("strb w13, [x11, x12]");                        // copy one byte into caller-owned storage
            emitter.instruction("add x12, x12, #1");                            // advance the binary copy cursor
            emitter.instruction(&format!("b {}", copy));                        // continue the result copy loop
            emitter.label(copied);
            emitter.instruction("strb wzr, [x11, x10]");                        // append a non-authoritative trailing NUL
        }
        Arch::X86_64 => {
            abi::load_at_offset(emitter, "rax", slots.length);
            emitter.instruction("add rax, 1");                                  // include one byte for the optional trailing NUL
            emitter.instruction("call __rt_heap_alloc");                        // allocate caller-owned result storage
            abi::store_at_offset(emitter, "rax", slots.destination);
            abi::load_at_offset(emitter, "r8", slots.source);
            abi::load_at_offset(emitter, "r9", slots.length);
            emitter.instruction("mov r10, rax");                                // retain the owned output base across the copy loop
            emitter.instruction("xor r11d, r11d");                              // initialize the binary copy cursor
            emitter.label(copy);
            emitter.instruction("cmp r11, r9");                                 // compare the cursor with the authoritative byte length
            emitter.instruction(&format!("jae {}", copied));                    // finish after every result byte is copied
            emitter.instruction("movzx eax, BYTE PTR [r8 + r11]");              // load one binary result byte
            emitter.instruction("mov BYTE PTR [r10 + r11], al");                // copy one byte into caller-owned storage
            emitter.instruction("add r11, 1");                                  // advance the binary copy cursor
            emitter.instruction(&format!("jmp {}", copy));                      // continue the result copy loop
            emitter.label(copied);
            emitter.instruction("mov BYTE PTR [r10 + r9], 0");                  // append a non-authoritative trailing NUL
        }
    }
}

/// Retires an owned source pointer unless it matches a borrowed host input.
pub fn emit_release_unless_borrowed(
    emitter: &mut impl Emitter, source: usize, borrowed: impl IntoIterator<Item = usize>, skip_release: &str,
) {
    let result = abi::int_result_reg(emitter);
    abi::load_at_offset(emitter, result, source);
    for offset in borrowed {
        match emitter.arch() {
            Arch::AArch64 => {
                abi::load_at_offset(emitter, "x9", offset);
                emitter.instruction("cmp x0, x9");                              // compare the result with one borrowed host string
                emitter.instruction(&format!("b.eq {skip_release}"));           // preserve storage still owned by the host
            }
            Arch::X86_64 => {
                abi::load_at_offset(emitter, "r10", offset);
                emitter.instruction("cmp rax, r10");                            // compare the result with one borrowed host string
                emitter.instruction(&format!("je {skip_release}"));             // preserve storage still owned by the host
            }
        }
    }
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction("bl __rt_heap_free_safe"),         // release runtime-owned result storage when present
        Arch::X86_64 => emitter.instruction("call __rt_heap_free_safe"),        // release runtime-owned result storage when present
    }
    emitter.label(skip_release);
}
