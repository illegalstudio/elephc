//! Purpose:
//! Emits the `__rt_end_boot_phase` runtime helper that marks all live heap
//! blocks allocated during the boot phase as immortal (bit 0x40 in the kind
//! byte). Immortal blocks are skipped by the cycle collector and never freed
//! by decref, protecting the shared boot state from garbage collection in
//! workers that inherited it via copy-on-write.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via the system module.
//!
//! Key details:
//! - Iterates the heap bump region [_heap_buf, _heap_buf + _heap_off) exactly
//!   like the GC clear pass: for each 16-byte header [size:4][refcount:4][kind:8],
//!   if refcount > 0 (live block), OR the kind byte with 0x40 (immortal).
//! - Called by the Rust bridge after boot_fn() returns and before fork, so the
//!   immortal marking is inherited by all workers via COW.
//! - Free blocks (refcount == 0) are skipped — they are not part of the boot
//!   state and remain available for per-request allocation.
//! - On macOS, the Rust bridge calls this via `extern "C"` which prepends a `_`
//!   to the C name, producing `___rt_end_boot_phase` (3 underscores). The
//!   assembly body is labeled `__rt_end_boot_phase` (2 underscores, matching
//!   internal `bl` calls). A macOS C-ABI alias stub is emitted under the
//!   3-underscore name so the Rust symbol resolves.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::{Arch, Platform};

/// Emits `__rt_end_boot_phase` — marks all live heap blocks as immortal.
///
/// Walks the heap bump region and sets bit 0x40 in the kind byte of every
/// block with refcount > 0. Called by the master after the PHP boot completes,
/// before forking workers, so the immortal marking is shared via COW.
pub fn emit_end_boot_phase(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_end_boot_phase_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: end_boot_phase (mark live heap blocks as immortal) ---");
    emitter.label_global("__rt_end_boot_phase");

    // Frame: save x29/x30 (no callee-saved needed — only uses x9/x10/x11).
    emitter.instruction("sub sp, sp, #16");
    emitter.instruction("stp x29, x30, [sp, #0]");
    emitter.instruction("add x29, sp, #0");

    // Load heap base and end.
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("mov x10, x9");                                           // x10 = scan pointer = heap base
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_heap_off");
    emitter.instruction("ldr x11, [x11]");                                        // x11 = heap offset (bump pointer)
    emitter.instruction("add x11, x9, x11");                                      // x11 = heap end = base + offset

    emitter.label("__rt_end_boot_phase_loop");
    emitter.instruction("cmp x10, x11");                                          // reached the end of the bump region?
    emitter.instruction("b.ge __rt_end_boot_phase_done");                         // yes — all live blocks marked
    emitter.instruction("ldr w9, [x10]");                                         // load this block payload size
    emitter.instruction("ldr w12, [x10, #4]");                                    // load this block refcount
    emitter.instruction("cbz w12, __rt_end_boot_phase_next");                     // free block (refcount 0) — skip
    // Live block: set the immortal bit (0x40) in the kind byte at [x10, #8].
    emitter.instruction("ldr x13, [x10, #8]");                                    // load the full kind word
    emitter.instruction("orr x13, x13, #0x40");                                   // set the immortal bit in the low byte
    emitter.instruction("str x13, [x10, #8]");                                    // store back the updated kind word

    emitter.label("__rt_end_boot_phase_next");
    // Advance: scan_ptr += payload_size + 16 (header).
    emitter.instruction("add x10, x10, x9");                                      // add payload size
    emitter.instruction("add x10, x10, #16");                                     // add 16-byte header
    emitter.instruction("b __rt_end_boot_phase_loop");                            // continue scanning

    emitter.label("__rt_end_boot_phase_done");
    emitter.instruction("ldp x29, x30, [sp, #0]");
    emitter.instruction("add sp, sp, #16");
    emitter.instruction("ret");

    // macOS C-ABI alias: ___rt_end_boot_phase -> __rt_end_boot_phase
    // The Rust bridge calls this via `extern "C"`, which on Mach-O prepends
    // a `_` to the C name, producing `___rt_end_boot_phase` (3 underscores).
    // The body above is labeled `__rt_end_boot_phase` (2 underscores, matching
    // internal assembly `bl` calls). Emit a tail-call stub under the Mach-O
    // name so the Rust symbol resolves. Linux has no leading underscore.
    if emitter.platform == Platform::MacOS {
        emitter.blank();
        emitter.raw(".align 2");
        emitter.comment("-- macOS C-ABI alias: ___rt_end_boot_phase -> __rt_end_boot_phase --");
        emitter.raw(".no_dead_strip ___rt_end_boot_phase");
        emitter.label_global("___rt_end_boot_phase");
        emitter.instruction("b __rt_end_boot_phase");                            // tail-call the real routine
    }
}

/// x86_64 Linux implementation of `__rt_end_boot_phase`.
fn emit_end_boot_phase_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: end_boot_phase (mark live heap blocks as immortal) ---");
    emitter.label_global("__rt_end_boot_phase");

    // Frame: push rbp for alignment.
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");

    // Load heap base into r10 (scan pointer) and heap end into r11.
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_heap_off");
    emitter.instruction("mov r11, QWORD PTR [r11]");                             // load heap offset
    emitter.instruction("add r11, r10");                                         // r11 = heap end = base + offset

    emitter.label("__rt_end_boot_phase_loop_x86");
    emitter.instruction("cmp r10, r11");                                         // reached the end?
    emitter.instruction("jae __rt_end_boot_phase_done_x86");                     // yes — done
    emitter.instruction("mov eax, DWORD PTR [r10]");                             // load payload size (32-bit)
    emitter.instruction("mov ecx, DWORD PTR [r10 + 4]");                         // load refcount (32-bit)
    emitter.instruction("test ecx, ecx");                                        // is refcount zero?
    emitter.instruction("jz __rt_end_boot_phase_next_x86");                      // free block — skip
    // Live block: set the immortal bit (0x40) in the kind byte at [r10 + 8].
    emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                         // load the full kind word
    emitter.instruction("or rax, 0x40");                                         // set the immortal bit
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                         // store back

    emitter.label("__rt_end_boot_phase_next_x86");
    // Advance: scan_ptr += payload_size + 16.
    emitter.instruction("movsxd rax, DWORD PTR [r10]");                          // re-load payload size (sign-extended)
    emitter.instruction("add r10, rax");                                         // add payload size
    emitter.instruction("add r10, 16");                                          // add 16-byte header
    emitter.instruction("jmp __rt_end_boot_phase_loop_x86");                     // continue

    emitter.label("__rt_end_boot_phase_done_x86");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}