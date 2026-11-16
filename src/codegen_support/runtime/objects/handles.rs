//! Purpose:
//! Combines the memory handle pool with PHP spl_object_hash formatting.
//!
//! Called from:
//! - Compiler runtime orchestration and object allocation sites.
//!
//! Key details:
//! - Pool storage and recycling live in Memoir; PHP hash string formatting stays here.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
pub(crate) use elephc_memoir::object_handles::emit_acquire_object_handle;
#[cfg(test)]
use elephc_memoir::object_handles::{object_handle_index_slots, object_handle_free_slots};

/// Emits the memory pool followed by the PHP hash formatter in the original order.
pub(crate) fn emit_object_handles(emitter: &mut Emitter) {
    elephc_memoir::object_handles::emit_object_handles(emitter);
    match emitter.target.arch {
        Arch::AArch64 => emit_spl_object_hash_arm64(emitter),
        Arch::X86_64 => emit_spl_object_hash_x86_64(emitter),
    }
}

/// Emits the AArch64 `__rt_spl_object_hash`.
///
/// PHP's `spl_object_hash()` is the object handle rendered as 16 zero-padded hex
/// digits followed by 16 more zeros; `spl_object_id($o) === 1` gives
/// `"00000000000000010000000000000000"` (verified against 8.5.6). Deriving it from
/// the same handle keeps `spl_object_hash`, `spl_object_id` and `var_dump`'s `#N`
/// mutually consistent.
///
/// Input: `x0` = object pointer. Output: `x1` = byte pointer, `x2` = 32, matching
/// `__rt_itoa`'s string-return convention. Writes into the `_concat_buf` scratch.
fn emit_spl_object_hash_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: spl_object_hash ---");
    emitter.label_global("__rt_spl_object_hash");

    emitter.instruction("sub sp, sp, #16");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    emitter.instruction("bl __rt_object_handle_of");                            // x0 = this object's PHP handle
    abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // x8 = current scratch-buffer offset
    abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // x9 = write cursor for the 32 hash bytes
    emitter.instruction("add x10, x8, #32");                                    // reserve exactly 32 scratch bytes
    emitter.instruction("str x10, [x6]");                                       // publish the advanced scratch offset

    emitter.instruction("mov x11, #15");                                        // start at the most significant of the 16 nibbles
    emitter.label("__rt_spl_object_hash_nibble");
    emitter.instruction("lsl x12, x11, #2");                                    // x12 = bit shift for this nibble
    emitter.instruction("lsr x13, x0, x12");                                    // move the selected nibble into the low bits
    emitter.instruction("and x13, x13, #0xf");                                  // isolate the nibble value
    emitter.instruction("cmp x13, #10");                                        // is this nibble a decimal digit?
    emitter.instruction("b.lo __rt_spl_object_hash_digit");                     // yes; render it as '0'..'9'
    emitter.instruction("add x13, x13, #87");                                   // render 10..15 as lowercase 'a'..'f'
    emitter.instruction("b __rt_spl_object_hash_store");                        // store the rendered hex character
    emitter.label("__rt_spl_object_hash_digit");
    emitter.instruction("add x13, x13, #48");                                   // render 0..9 as '0'..'9'
    emitter.label("__rt_spl_object_hash_store");
    emitter.instruction("strb w13, [x9], #1");                                  // write the hex character and advance the cursor
    emitter.instruction("subs x11, x11, #1");                                   // step down to the next nibble
    emitter.instruction("b.ge __rt_spl_object_hash_nibble");                    // keep rendering while nibbles remain

    emitter.instruction("mov w13, #48");                                        // PHP pads the second half with 16 literal '0' bytes
    emitter.instruction("mov x11, #16");                                        // 16 padding bytes remain
    emitter.label("__rt_spl_object_hash_pad");
    emitter.instruction("strb w13, [x9], #1");                                  // write one padding byte
    emitter.instruction("subs x11, x11, #1");                                   // count the padding byte
    emitter.instruction("b.ne __rt_spl_object_hash_pad");                       // keep padding to the full 32 bytes

    emitter.instruction("add x1, x7, x8");                                      // x1 = pointer to the first hash byte
    emitter.instruction("mov x2, #32");                                         // x2 = PHP's fixed spl_object_hash length
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the (ptr, len) string pair
}

/// Emits the x86_64 `__rt_spl_object_hash`.
///
/// Input: `rax` = object pointer. Output: `rax` = byte pointer, `rdx` = 32, matching
/// `__rt_itoa`'s string-return convention on this target.
fn emit_spl_object_hash_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: spl_object_hash ---");
    emitter.label_global("__rt_spl_object_hash");

    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("push rbx");                                            // preserve the callee-saved cursor register
    emitter.instruction("sub rsp, 8");                                          // restore the SysV 16-byte call alignment the odd push broke

    emitter.instruction("call __rt_object_handle_of");                          // rax = this object's PHP handle
    abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // r9 = current scratch-buffer offset
    abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("mov rbx, r10");                                        // rbx = write cursor for the 32 hash bytes
    emitter.instruction("add rbx, r9");                                         // advance the cursor to the reserved scratch slot
    emitter.instruction("mov r11, r9");                                         // copy the offset before reserving space
    emitter.instruction("add r11, 32");                                         // reserve exactly 32 scratch bytes
    emitter.instruction("mov QWORD PTR [r8], r11");                             // publish the advanced scratch offset

    emitter.instruction("mov r8, 15");                                          // start at the most significant of the 16 nibbles
    emitter.label("__rt_spl_object_hash_nibble");
    emitter.instruction("mov rcx, r8");                                         // variable shifts on x86_64 read the count from cl
    emitter.instruction("shl rcx, 2");                                          // rcx = bit shift for this nibble
    emitter.instruction("mov r11, rax");                                        // copy the handle before shifting it
    emitter.instruction("shr r11, cl");                                         // move the selected nibble into the low bits
    emitter.instruction("and r11, 0xf");                                        // isolate the nibble value
    emitter.instruction("cmp r11, 10");                                         // is this nibble a decimal digit?
    emitter.instruction("jb __rt_spl_object_hash_digit");                       // yes; render it as '0'..'9'
    emitter.instruction("add r11, 87");                                         // render 10..15 as lowercase 'a'..'f'
    emitter.instruction("jmp __rt_spl_object_hash_store");                      // store the rendered hex character
    emitter.label("__rt_spl_object_hash_digit");
    emitter.instruction("add r11, 48");                                         // render 0..9 as '0'..'9'
    emitter.label("__rt_spl_object_hash_store");
    emitter.instruction("mov BYTE PTR [rbx], r11b");                            // write the hex character
    emitter.instruction("add rbx, 1");                                          // advance the write cursor
    emitter.instruction("sub r8, 1");                                           // step down to the next nibble
    emitter.instruction("jns __rt_spl_object_hash_nibble");                     // keep rendering while nibbles remain

    emitter.instruction("mov r8, 16");                                          // PHP pads the second half with 16 literal '0' bytes
    emitter.label("__rt_spl_object_hash_pad");
    emitter.instruction("mov BYTE PTR [rbx], 48");                              // write one padding byte
    emitter.instruction("add rbx, 1");                                          // advance the write cursor
    emitter.instruction("sub r8, 1");                                           // count the padding byte
    emitter.instruction("jnz __rt_spl_object_hash_pad");                        // keep padding to the full 32 bytes

    abi::emit_symbol_address(emitter, "rax", "_concat_buf");
    emitter.instruction("add rax, r9");                                         // rax = pointer to the first hash byte
    emitter.instruction("mov rdx, 32");                                         // rdx = PHP's fixed spl_object_hash length
    emitter.instruction("add rsp, 8");                                          // release the alignment pad
    emitter.instruction("pop rbx");                                             // restore the callee-saved cursor register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the (ptr, len) string pair
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Locks the direct-mapped side-table sizing: one `u32` per 16-byte granule for
    /// the index, and the proven `heap_size / 24` live-object bound (plus slack) for
    /// the released-handle stack.
    #[test]
    fn side_table_sizes_follow_the_heap() {
        assert_eq!(object_handle_index_slots(8 * 1024 * 1024), 524_290);
        assert_eq!(object_handle_free_slots(8 * 1024 * 1024), 349_541);
        // The free stack must always outrank the largest possible live-object count.
        for heap in [1024usize, 65_536, 8 * 1024 * 1024, 512 * 1024 * 1024] {
            assert!(object_handle_free_slots(heap) > heap / 24);
            assert!(object_handle_index_slots(heap) > heap / 16);
        }
    }

    /// Verifies both architectures emit all three pool entry points.
    #[test]
    fn emits_the_pool_entry_points_on_both_targets() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_object_handles(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_object_handle_acquire:"), "{target:?}");
            assert!(asm.contains("__rt_object_handle_of:"), "{target:?}");
            assert!(asm.contains("__rt_object_handle_release:"), "{target:?}");
        }
    }
}
