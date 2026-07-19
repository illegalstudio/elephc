//! Purpose:
//! Emits the `__rt_zval_djbx33a` runtime helper that computes the PHP DJBX33A
//! hash of a string key, matching `zend_inline_hash_func` from the PHP source.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::zval`,
//!   and from `__rt_zval_pack_array_hash` for each string-keyed hash entry.
//!
//! Key details:
//! - Input: `x1` / `rdi` = key byte pointer, `x2` / `rsi` = key byte length.
//! - Output: `x0` / `rax` = the 64-bit DJBX33A hash with the high bit set
//!   (`hash | 0x8000000000000000`), so the hash is never zero (PHP uses zero as
//!   the "hash not yet computed" sentinel on `zend_string.h`).
//! - Per-character update is `hash = ((hash << 5) + hash) + c`, i.e. `hash * 33 + c`,
//!   starting from `5381`, matching PHP's `zend_inline_hash_func`.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// zval_djbx33a: compute the PHP DJBX33A hash of a string key.
/// Input:  x1 / rdi = key byte pointer, x2 / rsi = key byte length
/// Output: x0 / rax = 64-bit hash with the high bit set
pub fn emit_zval_djbx33a(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_zval_djbx33a_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: zval_djbx33a ---");
    emitter.label_global("__rt_zval_djbx33a");

    // -- initialize the hash accumulator and copy the loop inputs --
    emitter.instruction("mov x0, #5381");                                       // hash = 5381 (DJBX33A seed)
    emitter.instruction("mov x3, x1");                                          // x3 = cursor over the key bytes
    emitter.instruction("mov x4, x2");                                          // x4 = remaining byte count
    emitter.instruction("cbz x4, __rt_zval_djbx33a_done");                      // empty key skips straight to the high-bit mask

    // -- per-character loop: hash = (hash << 5) + hash + c = hash * 33 + c --
    emitter.label("__rt_zval_djbx33a_loop");
    emitter.instruction("ldrb w5, [x3], #1");                                   // load the next key byte (zero-extended into x5)
    emitter.instruction("lsl x6, x0, #5");                                      // hash << 5
    emitter.instruction("add x6, x6, x0");                                      // (hash << 5) + hash = hash * 33
    emitter.instruction("add x0, x6, x5");                                      // hash = hash * 33 + c
    emitter.instruction("sub x4, x4, #1");                                      // one fewer byte to process
    emitter.instruction("cbnz x4, __rt_zval_djbx33a_loop");                     // continue until every byte is folded in

    // -- set the high bit so the hash is never the zero sentinel --
    emitter.label("__rt_zval_djbx33a_done");
    emitter.instruction("mov x7, #1");                                          // materialize a one in a scratch register
    emitter.instruction("lsl x7, x7, #63");                                     // shift it into the sign bit (0x8000000000000000)
    emitter.instruction("orr x0, x0, x7");                                      // hash |= high bit (PHP's nonzero-hash marker)
    emitter.instruction("ret");                                                 // return the hash in x0
}

/// x86_64 Linux implementation of `__rt_zval_djbx33a`.
/// Input:  rdi = key byte pointer, rsi = key byte length
/// Output: rax = 64-bit hash with the high bit set
fn emit_zval_djbx33a_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: zval_djbx33a ---");
    emitter.label_global("__rt_zval_djbx33a");

    // -- initialize the hash accumulator and copy the loop inputs --
    emitter.instruction("mov rax, 5381");                                       // hash = 5381 (DJBX33A seed)
    emitter.instruction("mov rcx, rdi");                                        // rcx = cursor over the key bytes
    emitter.instruction("mov rdx, rsi");                                        // rdx = remaining byte count
    emitter.instruction("test rdx, rdx");                                       // is the key empty?
    emitter.instruction("jz __rt_zval_djbx33a_done");                           // empty key skips straight to the high-bit mask

    // -- per-character loop: hash = (hash << 5) + hash + c = hash * 33 + c --
    emitter.label("__rt_zval_djbx33a_loop");
    emitter.instruction("movzx r8, BYTE PTR [rcx]");                            // load the next key byte (zero-extended)
    emitter.instruction("inc rcx");                                             // advance the cursor
    emitter.instruction("mov r9, rax");                                         // copy hash for the shift
    emitter.instruction("shl r9, 5");                                           // hash << 5
    emitter.instruction("add r9, rax");                                         // (hash << 5) + hash = hash * 33
    emitter.instruction("lea rax, [r9 + r8]");                                  // hash = hash * 33 + c
    emitter.instruction("dec rdx");                                             // one fewer byte to process
    emitter.instruction("jnz __rt_zval_djbx33a_loop");                          // continue until every byte is folded in

    // -- set the high bit so the hash is never the zero sentinel --
    emitter.label("__rt_zval_djbx33a_done");
    emitter.instruction("mov r10, 1");                                          // materialize a one in a scratch register
    emitter.instruction("shl r10, 63");                                         // shift it into the sign bit (0x8000000000000000)
    emitter.instruction("or rax, r10");                                         // hash |= high bit (PHP's nonzero-hash marker)
    emitter.instruction("ret");                                                 // return the hash in rax
}