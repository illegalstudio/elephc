//! Purpose:
//! Emits the `__rt_ltrim`, `__rt_ltrim_loop` runtime helper assembly for ltrim.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - Trim helpers scan byte ranges without copying unless the returned pointer/length slice changes.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits the `__rt_ltrim` runtime helper for the current target.
///
/// dispatches to the target-specific emitter. On ARM64 uses x1 (pointer) and x2
/// (length); on x86_64 uses rax (pointer) and rdx (length). Both registers are
/// read and updated in place: on return, x1/rax points to the first non-default-mask
/// byte and x2/rdx holds the remaining length. Trims PHP's default mask bytes:
/// NUL, tab, newline, vertical tab, form feed, carriage return, and space.
pub fn emit_ltrim(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ltrim_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ltrim ---");
    emitter.label_global("__rt_ltrim");
    emitter.label("__rt_ltrim_loop");
    emitter.instruction("cbz x2, __rt_ltrim_done");                             // if string is empty, nothing to trim
    emitter.instruction("ldrb w9, [x1]");                                       // peek at first byte without advancing
    emitter.instruction("cmp w9, #0");                                          // check for NUL (0x00)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if NUL, skip it
    emitter.instruction("cmp w9, #32");                                         // check for space (0x20)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if space, skip it
    emitter.instruction("cmp w9, #9");                                          // check for tab (0x09)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if tab, skip it
    emitter.instruction("cmp w9, #10");                                         // check for newline (0x0A)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if newline, skip it
    emitter.instruction("cmp w9, #11");                                         // check for vertical tab (0x0B)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if vertical tab, skip it
    emitter.instruction("cmp w9, #12");                                         // check for form feed (0x0C)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if form feed, skip it
    emitter.instruction("cmp w9, #13");                                         // check for carriage return (0x0D)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if CR, skip it
    emitter.instruction("b __rt_ltrim_done");                                   // byte outside PHP's default trim mask found, stop trimming

    // -- advance past default-mask character --
    emitter.label("__rt_ltrim_skip");
    emitter.instruction("add x1, x1, #1");                                      // advance string pointer past the trimmed byte
    emitter.instruction("sub x2, x2, #1");                                      // decrement string length
    emitter.instruction("b __rt_ltrim_loop");                                   // check next character

    emitter.label("__rt_ltrim_done");
    emitter.instruction("ret");                                                 // return with adjusted x1 and x2
}

/// Emits the `__rt_ltrim` runtime helper for the x86_64 Linux target.
///
/// Reads rax (pointer) and rdx (length) in place. On return, rax points to the
/// first non-default-mask byte and rdx holds the remaining length. Trims PHP's default
/// mask bytes: NUL, tab, newline, vertical tab, form feed, carriage return, and space.
fn emit_ltrim_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ltrim ---");
    emitter.label_global("__rt_ltrim");
    emitter.label("__rt_ltrim_loop_x86");
    emitter.instruction("test rdx, rdx");                                       // is the borrowed string slice already empty before trimming any leading whitespace?
    emitter.instruction("je __rt_ltrim_done_x86");                              // stop immediately when there are no bytes left to inspect
    emitter.instruction("movzx ecx, BYTE PTR [rax]");                           // peek at the first byte of the borrowed string slice without advancing the pointer yet
    emitter.instruction("cmp cl, 0");                                           // is the first byte a NUL that ltrim() should discard?
    emitter.instruction("je __rt_ltrim_skip_x86");                              // strip the leading NUL and continue trimming from the new front
    emitter.instruction("cmp cl, 32");                                          // is the first byte a space that ltrim() should discard?
    emitter.instruction("je __rt_ltrim_skip_x86");                              // strip the leading space and continue trimming from the new front
    emitter.instruction("cmp cl, 9");                                           // is the first byte a tab that ltrim() should discard?
    emitter.instruction("je __rt_ltrim_skip_x86");                              // strip the leading tab and continue trimming from the new front
    emitter.instruction("cmp cl, 10");                                          // is the first byte a newline that ltrim() should discard?
    emitter.instruction("je __rt_ltrim_skip_x86");                              // strip the leading newline and continue trimming from the new front
    emitter.instruction("cmp cl, 11");                                          // is the first byte a vertical tab that ltrim() should discard?
    emitter.instruction("je __rt_ltrim_skip_x86");                              // strip the leading vertical tab and continue trimming from the new front
    emitter.instruction("cmp cl, 12");                                          // is the first byte a form feed that ltrim() should discard?
    emitter.instruction("je __rt_ltrim_skip_x86");                              // strip the leading form feed and continue trimming from the new front
    emitter.instruction("cmp cl, 13");                                          // is the first byte a carriage return that ltrim() should discard?
    emitter.instruction("je __rt_ltrim_skip_x86");                              // strip the leading carriage return and continue trimming from the new front
    emitter.instruction("jmp __rt_ltrim_done_x86");                             // stop once the first byte is outside PHP's default trim mask

    emitter.label("__rt_ltrim_skip_x86");
    emitter.instruction("add rax, 1");                                          // advance the borrowed string pointer past the stripped leading byte
    emitter.instruction("sub rdx, 1");                                          // shrink the borrowed string length to match the removed leading byte
    emitter.instruction("jmp __rt_ltrim_loop_x86");                             // continue trimming from the new front of the borrowed string slice

    emitter.label("__rt_ltrim_done_x86");
    emitter.instruction("ret");                                                 // return the adjusted borrowed string slice in rax/rdx
}
