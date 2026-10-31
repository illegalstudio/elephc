//! Purpose:
//! Emits `__rt_base64_decoded_len`, the allocation-free length computation for
//! `strlen(base64_decode(...))`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Lax mode counts accepted sextets and returns `floor(accepted * 6 / 8)`; padding,
//!   whitespace and invalid bytes do not produce output bytes.
//! - Strict mode reproduces php-src's rejection rules: data after padding, one leftover
//!   sextet, over-padding and misplaced padding all return zero, matching `strlen(false)`.
//! - ABI: AArch64 x1/x2 = encoded string, x3 = strict flag; x86_64 rax/rdx = encoded
//!   string, rdi = strict flag. Result in x0/rax.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

use super::base64_decode::{B64_DECODE_INVALID, B64_DECODE_SKIP};

/// Emits the `__rt_base64_decoded_len` helper for the active target.
pub fn emit_base64_decoded_len(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_base64_decoded_len_x86_64(emitter);
        return;
    }
    emit_base64_decoded_len_aarch64(emitter);
}

/// Emits the AArch64 length helper.
fn emit_base64_decoded_len_aarch64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: base64_decoded_len ---");
    e.label_global("__rt_base64_decoded_len");
    e.instruction("mov x9, xzr");                                               // accepted sextet count
    e.instruction("mov x10, xzr");                                              // padding count
    e.instruction("mov x11, xzr");                                              // has padding been seen?
    abi::emit_symbol_address(e, "x12", "_b64_decode_tbl");                      // reverse lookup table

    // -- four-byte fast path while no padding has been seen --
    e.label("__rt_b64dec_len_fast");
    e.instruction("cbnz x11, __rt_b64dec_len_loop");                            // padding forces the scalar state machine
    e.instruction("cmp x2, #4");                                                // is a full quartet available?
    e.instruction("b.lo __rt_b64dec_len_loop");                                 // short tails use the scalar loop
    e.instruction("ldrb w4, [x1]");                                             // load the first encoded byte
    e.instruction("ldrb w4, [x12, x4]");                                        // classify the first byte
    e.instruction("cmp w4, #64");                                               // is it a Base64 sextet?
    e.instruction("b.hs __rt_b64dec_len_loop");                                 // a non-sextet falls back to the scalar loop
    e.instruction("ldrb w5, [x1, #1]");                                         // load the second encoded byte
    e.instruction("ldrb w5, [x12, x5]");                                        // classify the second byte
    e.instruction("cmp w5, #64");                                               // is it a Base64 sextet?
    e.instruction("b.hs __rt_b64dec_len_loop");                                 // a non-sextet falls back to the scalar loop
    e.instruction("ldrb w6, [x1, #2]");                                         // load the third encoded byte
    e.instruction("ldrb w6, [x12, x6]");                                        // classify the third byte
    e.instruction("cmp w6, #64");                                               // is it a Base64 sextet?
    e.instruction("b.hs __rt_b64dec_len_loop");                                 // a non-sextet falls back to the scalar loop
    e.instruction("ldrb w7, [x1, #3]");                                         // load the fourth encoded byte
    e.instruction("ldrb w7, [x12, x7]");                                        // classify the fourth byte
    e.instruction("cmp w7, #64");                                               // is it a Base64 sextet?
    e.instruction("b.hs __rt_b64dec_len_loop");                                 // a non-sextet falls back to the scalar loop
    e.instruction("add x9, x9, #4");                                            // count four accepted sextets
    e.instruction("add x1, x1, #4");                                            // consume the four encoded bytes
    e.instruction("sub x2, x2, #4");                                            // account for the consumed quartet
    e.instruction("b __rt_b64dec_len_fast");                                    // process the next complete quartet

    e.label("__rt_b64dec_len_loop");
    e.instruction("cbz x2, __rt_b64dec_len_end");                               // stop at the end of the input
    e.instruction("ldrb w4, [x1], #1");                                         // load the next encoded byte
    e.instruction("sub x2, x2, #1");                                            // consume one input byte
    e.instruction("cmp w4, #61");                                               // is this '=' padding?
    e.instruction("b.eq __rt_b64dec_len_pad");                                  // padding is counted, never decoded
    e.instruction("ldrb w5, [x12, x4]");                                        // classify the byte through the reverse table
    e.instruction(&format!("cmp w5, #{}", B64_DECODE_SKIP));                    // is this skippable whitespace?
    e.instruction("b.eq __rt_b64dec_len_loop");                                 // whitespace is ignored in both modes
    e.instruction(&format!("cmp w5, #{}", B64_DECODE_INVALID));                 // is this outside the Base64 alphabet?
    e.instruction("b.eq __rt_b64dec_len_invalid");                              // strict mode rejects it, lax mode drops it
    e.instruction("cbz x11, __rt_b64dec_len_accept");                           // data before padding is accepted directly
    e.instruction("cbnz x3, __rt_b64dec_len_fail");                             // strict mode forbids data after padding
    e.label("__rt_b64dec_len_accept");
    e.instruction("add x9, x9, #1");                                            // count one accepted sextet
    e.instruction("b __rt_b64dec_len_fast");                                    // retry the quartet fast path
    e.label("__rt_b64dec_len_pad");
    e.instruction("add x10, x10, #1");                                          // count this padding character
    e.instruction("mov x11, #1");                                               // remember that padding has been seen
    e.instruction("b __rt_b64dec_len_loop");                                    // classify the next byte
    e.label("__rt_b64dec_len_invalid");
    e.instruction("cbz x3, __rt_b64dec_len_fast");                              // lax mode ignores invalid bytes and retries the fast path
    e.instruction("b __rt_b64dec_len_fail");                                    // strict mode rejects them
    e.label("__rt_b64dec_len_end");
    e.instruction("cbz x3, __rt_b64dec_len_compute");                           // lax mode skips the strict end checks
    e.instruction("and x4, x9, #3");                                            // recover the final quartet lane
    e.instruction("cmp x4, #1");                                                // one leftover sextet cannot encode a byte
    e.instruction("b.eq __rt_b64dec_len_fail");                                 // strict rejection maps to strlen(false) == 0
    e.instruction("cbz x10, __rt_b64dec_len_compute");                          // unpadded input is accepted
    e.instruction("cmp x10, #2");                                               // more than two padding characters is never valid
    e.instruction("b.gt __rt_b64dec_len_fail");                                 // reject over-padded input
    e.instruction("add x4, x9, x10");                                           // sextets plus padding must complete quartets
    e.instruction("and x4, x4, #3");                                            // check the combined count against the quartet size
    e.instruction("cbnz x4, __rt_b64dec_len_fail");                             // reject misplaced padding
    e.label("__rt_b64dec_len_compute");
    e.instruction("add x0, x9, x9");                                            // 2 * accepted sextets
    e.instruction("add x0, x0, x9");                                            // 3 * accepted sextets
    e.instruction("lsr x0, x0, #2");                                            // decoded bytes = floor(3 * accepted / 4)
    e.instruction("ret");                                                       // return the decoded length
    e.label("__rt_b64dec_len_fail");
    e.instruction("mov x0, xzr");                                               // strlen(false) is zero
    e.instruction("ret");                                                       // return the rejected length
}

/// Emits the x86_64 SysV length helper.
fn emit_base64_decoded_len_x86_64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: base64_decoded_len ---");
    e.label_global("__rt_base64_decoded_len");
    e.instruction("mov rsi, rax");                                              // rsi = encoded string pointer
    e.instruction("mov rcx, rdx");                                              // rcx = encoded string length
    e.instruction("xor r8d, r8d");                                              // accepted sextet count
    e.instruction("xor r9d, r9d");                                              // padding count
    e.instruction("xor r10d, r10d");                                            // has padding been seen?
    abi::emit_symbol_address(e, "r11", "_b64_decode_tbl");                      // reverse lookup table

    // -- four-byte fast path while no padding has been seen --
    e.label("__rt_b64dec_len_fast_x86_64");
    e.instruction("test r10, r10");                                             // has padding been seen?
    e.instruction("jnz __rt_b64dec_len_loop_x86_64");                           // padding forces the scalar state machine
    e.instruction("cmp rcx, 4");                                                // is a full quartet available?
    e.instruction("jb __rt_b64dec_len_loop_x86_64");                            // short tails use the scalar loop
    e.instruction("movzx eax, BYTE PTR [rsi]");                                 // load the first encoded byte
    e.instruction("movzx eax, BYTE PTR [r11 + rax]");                           // classify the first byte
    e.instruction("cmp eax, 64");                                               // is it a Base64 sextet?
    e.instruction("jae __rt_b64dec_len_loop_x86_64");                           // a non-sextet falls back to the scalar loop
    e.instruction("movzx eax, BYTE PTR [rsi + 1]");                             // load the second encoded byte
    e.instruction("movzx eax, BYTE PTR [r11 + rax]");                           // classify the second byte
    e.instruction("cmp eax, 64");                                               // is it a Base64 sextet?
    e.instruction("jae __rt_b64dec_len_loop_x86_64");                           // a non-sextet falls back to the scalar loop
    e.instruction("movzx eax, BYTE PTR [rsi + 2]");                             // load the third encoded byte
    e.instruction("movzx eax, BYTE PTR [r11 + rax]");                           // classify the third byte
    e.instruction("cmp eax, 64");                                               // is it a Base64 sextet?
    e.instruction("jae __rt_b64dec_len_loop_x86_64");                           // a non-sextet falls back to the scalar loop
    e.instruction("movzx eax, BYTE PTR [rsi + 3]");                             // load the fourth encoded byte
    e.instruction("movzx eax, BYTE PTR [r11 + rax]");                           // classify the fourth byte
    e.instruction("cmp eax, 64");                                               // is it a Base64 sextet?
    e.instruction("jae __rt_b64dec_len_loop_x86_64");                           // a non-sextet falls back to the scalar loop
    e.instruction("add r8, 4");                                                 // count four accepted sextets
    e.instruction("add rsi, 4");                                                // consume the four encoded bytes
    e.instruction("sub rcx, 4");                                                // account for the consumed quartet
    e.instruction("jmp __rt_b64dec_len_fast_x86_64");                           // process the next complete quartet

    e.label("__rt_b64dec_len_loop_x86_64");
    e.instruction("test rcx, rcx");                                             // is any input left?
    e.instruction("jz __rt_b64dec_len_end_x86_64");                             // stop at the end of the input
    e.instruction("movzx eax, BYTE PTR [rsi]");                                 // load the next encoded byte
    e.instruction("inc rsi");                                                   // advance the input cursor
    e.instruction("dec rcx");                                                   // consume one input byte
    e.instruction("cmp eax, 61");                                               // is this '=' padding?
    e.instruction("je __rt_b64dec_len_pad_x86_64");                             // padding is counted, never decoded
    e.instruction("movzx eax, BYTE PTR [r11 + rax]");                           // classify the byte through the reverse table
    e.instruction(&format!("cmp eax, {}", B64_DECODE_SKIP));                    // is this skippable whitespace?
    e.instruction("je __rt_b64dec_len_loop_x86_64");                            // whitespace is ignored in both modes
    e.instruction(&format!("cmp eax, {}", B64_DECODE_INVALID));                 // is this outside the Base64 alphabet?
    e.instruction("je __rt_b64dec_len_invalid_x86_64");                         // strict mode rejects it, lax mode drops it
    e.instruction("test r10, r10");                                             // has padding already been seen?
    e.instruction("jz __rt_b64dec_len_accept_x86_64");                          // data before padding is accepted directly
    e.instruction("test rdi, rdi");                                             // is this a strict decode?
    e.instruction("jnz __rt_b64dec_len_fail_x86_64");                           // strict mode forbids data after padding
    e.label("__rt_b64dec_len_accept_x86_64");
    e.instruction("inc r8");                                                    // count one accepted sextet
    e.instruction("jmp __rt_b64dec_len_fast_x86_64");                           // retry the quartet fast path
    e.label("__rt_b64dec_len_pad_x86_64");
    e.instruction("inc r9");                                                    // count this padding character
    e.instruction("mov r10d, 1");                                               // remember that padding has been seen
    e.instruction("jmp __rt_b64dec_len_loop_x86_64");                           // classify the next byte
    e.label("__rt_b64dec_len_invalid_x86_64");
    e.instruction("test rdi, rdi");                                             // is this a strict decode?
    e.instruction("jnz __rt_b64dec_len_fail_x86_64");                           // strict mode rejects invalid bytes
    e.instruction("jmp __rt_b64dec_len_fast_x86_64");                           // lax mode ignores them and retries the fast path
    e.label("__rt_b64dec_len_end_x86_64");
    e.instruction("test rdi, rdi");                                             // is this a strict decode?
    e.instruction("jz __rt_b64dec_len_compute_x86_64");                         // lax mode skips the strict end checks
    e.instruction("mov rax, r8");                                               // copy the accepted count
    e.instruction("and rax, 3");                                                // recover the final quartet lane
    e.instruction("cmp rax, 1");                                                // one leftover sextet cannot encode a byte
    e.instruction("je __rt_b64dec_len_fail_x86_64");                            // strict rejection maps to strlen(false) == 0
    e.instruction("test r9, r9");                                               // was the input padded?
    e.instruction("jz __rt_b64dec_len_compute_x86_64");                         // unpadded input is accepted
    e.instruction("cmp r9, 2");                                                 // more than two padding characters is never valid
    e.instruction("ja __rt_b64dec_len_fail_x86_64");                            // reject over-padded input
    e.instruction("mov rax, r8");                                               // copy the accepted count
    e.instruction("add rax, r9");                                               // sextets plus padding must complete quartets
    e.instruction("and rax, 3");                                                // check the combined count against the quartet size
    e.instruction("jnz __rt_b64dec_len_fail_x86_64");                           // reject misplaced padding
    e.label("__rt_b64dec_len_compute_x86_64");
    e.instruction("lea rax, [r8 + r8 * 2]");                                    // 3 * accepted sextets
    e.instruction("shr rax, 2");                                                // decoded bytes = floor(3 * accepted / 4)
    e.instruction("ret");                                                       // return the decoded length
    e.label("__rt_b64dec_len_fail_x86_64");
    e.instruction("xor eax, eax");                                              // strlen(false) is zero
    e.instruction("ret");                                                       // return the rejected length
}
