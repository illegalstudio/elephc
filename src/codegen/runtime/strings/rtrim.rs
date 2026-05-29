//! Purpose:
//! Emits the `__rt_rtrim`, `__rt_rtrim_loop` runtime helper assembly for rtrim.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - Trim helpers scan byte ranges without copying unless the returned pointer/length slice changes.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits the `__rt_rtrim` runtime helper for ARM64 or x86_64 Linux.
///
/// On entry the borrowed string slice lives in `(x1, x2)` for ARM64 or `(rax, rdx)` for x86_64.
/// On exit the length is reduced by stripping PHP's default trailing mask bytes.
/// ARM64 dispatches directly; x86_64 invokes `emit_rtrim_linux_x86_64`.
pub fn emit_rtrim(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_rtrim_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: rtrim ---");
    emitter.label_global("__rt_rtrim");
    emitter.label("__rt_rtrim_loop");
    emitter.instruction("cbz x2, __rt_rtrim_done");                             // if string is empty, nothing to trim
    emitter.instruction("sub x9, x2, #1");                                      // compute index of last character
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load last byte of string
    emitter.instruction("cmp w10, #0");                                         // check for NUL (0x00)
    emitter.instruction("b.eq __rt_rtrim_strip");                               // if NUL, strip it
    emitter.instruction("cmp w10, #32");                                        // check for space (0x20)
    emitter.instruction("b.eq __rt_rtrim_strip");                               // if space, strip it
    emitter.instruction("cmp w10, #9");                                         // check for tab (0x09)
    emitter.instruction("b.eq __rt_rtrim_strip");                               // if tab, strip it
    emitter.instruction("cmp w10, #10");                                        // check for newline (0x0A)
    emitter.instruction("b.eq __rt_rtrim_strip");                               // if newline, strip it
    emitter.instruction("cmp w10, #11");                                        // check for vertical tab (0x0B)
    emitter.instruction("b.eq __rt_rtrim_strip");                               // if vertical tab, strip it
    emitter.instruction("cmp w10, #12");                                        // check for form feed (0x0C)
    emitter.instruction("b.eq __rt_rtrim_strip");                               // if form feed, strip it
    emitter.instruction("cmp w10, #13");                                        // check for carriage return (0x0D)
    emitter.instruction("b.eq __rt_rtrim_strip");                               // if CR, strip it
    emitter.instruction("b __rt_rtrim_done");                                   // byte outside PHP's default trim mask found, stop trimming

    // -- shrink length to strip trailing default-mask byte --
    emitter.label("__rt_rtrim_strip");
    emitter.instruction("sub x2, x2, #1");                                      // reduce length by one stripped trailing byte
    emitter.instruction("b __rt_rtrim_loop");                                   // check new last character

    emitter.label("__rt_rtrim_done");
    emitter.instruction("ret");                                                 // return with adjusted x2
}

/// Emits the x86_64 Linux variant of `__rt_rtrim`.
///
/// On entry the borrowed string slice is in `(rax, rdx)` where rax holds the pointer and rdx holds the length.
/// On exit rdx holds the reduced length after stripping PHP's default trailing mask bytes.
fn emit_rtrim_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: rtrim ---");
    emitter.label_global("__rt_rtrim");
    emitter.label("__rt_rtrim_loop_x86");
    emitter.instruction("test rdx, rdx");                                       // is the borrowed string slice already empty before trimming any trailing whitespace?
    emitter.instruction("je __rt_rtrim_done_x86");                              // stop immediately when there are no bytes left to inspect
    emitter.instruction("mov rcx, rdx");                                        // copy the borrowed string length so rtrim() can inspect the current last-byte index
    emitter.instruction("sub rcx, 1");                                          // compute the index of the last byte in the borrowed string slice
    emitter.instruction("movzx esi, BYTE PTR [rax + rcx]");                     // load the last byte of the borrowed string slice for whitespace classification
    emitter.instruction("cmp sil, 0");                                          // is the trailing byte a NUL that rtrim() should discard?
    emitter.instruction("je __rt_rtrim_strip_x86");                             // strip the trailing NUL and continue trimming from the new end
    emitter.instruction("cmp sil, 32");                                         // is the trailing byte a space that rtrim() should discard?
    emitter.instruction("je __rt_rtrim_strip_x86");                             // strip the trailing space and continue trimming from the new end
    emitter.instruction("cmp sil, 9");                                          // is the trailing byte a tab that rtrim() should discard?
    emitter.instruction("je __rt_rtrim_strip_x86");                             // strip the trailing tab and continue trimming from the new end
    emitter.instruction("cmp sil, 10");                                         // is the trailing byte a newline that rtrim() should discard?
    emitter.instruction("je __rt_rtrim_strip_x86");                             // strip the trailing newline and continue trimming from the new end
    emitter.instruction("cmp sil, 11");                                         // is the trailing byte a vertical tab that rtrim() should discard?
    emitter.instruction("je __rt_rtrim_strip_x86");                             // strip the trailing vertical tab and continue trimming from the new end
    emitter.instruction("cmp sil, 12");                                         // is the trailing byte a form feed that rtrim() should discard?
    emitter.instruction("je __rt_rtrim_strip_x86");                             // strip the trailing form feed and continue trimming from the new end
    emitter.instruction("cmp sil, 13");                                         // is the trailing byte a carriage return that rtrim() should discard?
    emitter.instruction("je __rt_rtrim_strip_x86");                             // strip the trailing carriage return and continue trimming from the new end
    emitter.instruction("jmp __rt_rtrim_done_x86");                             // stop once the trailing byte is outside PHP's default trim mask

    emitter.label("__rt_rtrim_strip_x86");
    emitter.instruction("sub rdx, 1");                                          // shrink the borrowed string length to exclude the stripped trailing byte
    emitter.instruction("jmp __rt_rtrim_loop_x86");                             // continue trimming from the new end of the borrowed string slice

    emitter.label("__rt_rtrim_done_x86");
    emitter.instruction("ret");                                                 // return the adjusted borrowed string slice in rax/rdx
}
