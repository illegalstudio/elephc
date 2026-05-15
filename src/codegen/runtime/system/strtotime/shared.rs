//! Purpose:
//! Holds shared asm helpers used by the `__rt_strtotime` strategy parsers (trim, lowercase, table matching).
//! All helpers are pure asm emitters; they expose stable labels under the `__rt_strtotime_*` namespace.
//!
//! Called from:
//! - `crate::codegen::runtime::system::strtotime::mod` for the dispatcher and per-strategy parsers.
//!
//! Key details:
//! - Helpers preserve the caller's saved frame slots `[sp+48..63]` (orig ptr/len) and the lc16 buffer at `[sp+64..79]`.
//! - Comparison logic is ASCII-only — non-ASCII bytes are intentionally not case-folded.
//! - On x86_64 helpers are reached via `call` (which pushes the return address), so they access dispatcher slots through `[rbp - <constant>]`. Mapping: dispatcher `[rsp + N]` ↔ helper `[rbp - 128 + N]`.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emit the shared strtotime asm helpers (trim, lc16, match_word, skip_ws, parse_dec, lc_cursor).
pub(crate) fn emit_helpers(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_trim_linux_x86_64(emitter);
        emit_lc16_linux_x86_64(emitter);
        emit_match_word_linux_x86_64(emitter);
        emit_skip_ws_linux_x86_64(emitter);
        emit_parse_dec_linux_x86_64(emitter);
        emit_lc_cursor_linux_x86_64(emitter);
        return;
    }

    emit_trim_arm64(emitter);
    emit_lc16_arm64(emitter);
    emit_match_word_arm64(emitter);
    emit_skip_ws_arm64(emitter);
    emit_parse_dec_arm64(emitter);
    emit_lc_cursor_arm64(emitter);
}

fn emit_trim_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime helper: trim leading/trailing ASCII whitespace ---");
    emitter.label("__rt_strtotime_trim");

    // -- load saved ptr/len from dispatcher slots --
    emitter.instruction("ldr x1, [sp, #48]");                                   // load saved input pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // load saved input length

    // -- strip leading ASCII whitespace --
    emitter.label("__rt_strtotime_trim_lead");
    emitter.instruction("cbz x2, __rt_strtotime_trim_done");                    // empty input → done
    emitter.instruction("ldrb w9, [x1]");                                       // load first byte
    emitter.instruction("cmp w9, #32");                                         // space?
    emitter.instruction("b.eq __rt_strtotime_trim_lead_adv");                   // yes → advance
    emitter.instruction("sub w10, w9, #9");                                     // normalize ASCII control whitespace range
    emitter.instruction("cmp w10, #4");                                         // tab/newline/vtab/form-feed/carriage-return?
    emitter.instruction("b.hi __rt_strtotime_trim_trail");                      // not WS → go to trail trim
    emitter.label("__rt_strtotime_trim_lead_adv");
    emitter.instruction("add x1, x1, #1");                                      // advance pointer
    emitter.instruction("sub x2, x2, #1");                                      // shrink length
    emitter.instruction("b __rt_strtotime_trim_lead");                          // continue

    // -- strip trailing ASCII whitespace --
    emitter.label("__rt_strtotime_trim_trail");
    emitter.instruction("cbz x2, __rt_strtotime_trim_done");                    // empty → done
    emitter.instruction("sub x9, x2, #1");                                      // offset to last byte
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load last byte
    emitter.instruction("cmp w10, #32");                                        // space?
    emitter.instruction("b.eq __rt_strtotime_trim_trail_adv");                  // yes → shrink
    emitter.instruction("sub w11, w10, #9");                                    // normalize ASCII control whitespace range
    emitter.instruction("cmp w11, #4");                                         // tab/newline/vtab/form-feed/carriage-return?
    emitter.instruction("b.hi __rt_strtotime_trim_done");                       // not WS → done
    emitter.label("__rt_strtotime_trim_trail_adv");
    emitter.instruction("sub x2, x2, #1");                                      // shrink length
    emitter.instruction("b __rt_strtotime_trim_trail");                         // continue

    // -- write back trimmed ptr/len and return --
    emitter.label("__rt_strtotime_trim_done");
    emitter.instruction("str x1, [sp, #48]");                                   // save trimmed pointer
    emitter.instruction("str x2, [sp, #56]");                                   // save trimmed length
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_lc16_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime helper: lowercase first 16 input bytes into [sp+64..79] ---");
    emitter.label("__rt_strtotime_lc16");

    // -- zero-pad target buffer first --
    emitter.instruction("stp xzr, xzr, [sp, #64]");                             // clear the 16-byte lc16 buffer

    // -- copy up to 16 bytes, lowercasing ASCII A-Z --
    emitter.instruction("ldr x1, [sp, #48]");                                   // load trimmed input pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // load trimmed input length
    emitter.instruction("add x14, sp, #64");                                    // x14 = base of lc16 buffer
    emitter.instruction("mov x9, #0");                                          // copy index
    emitter.instruction("mov x10, #16");                                        // cap = 16
    emitter.instruction("cmp x2, x10");                                         // len vs 16
    emitter.instruction("csel x10, x2, x10, lo");                               // x10 = min(len, 16)
    emitter.label("__rt_strtotime_lc16_loop");
    emitter.instruction("cmp x9, x10");                                         // done?
    emitter.instruction("b.ge __rt_strtotime_lc16_done");                       // yes → return
    emitter.instruction("ldrb w11, [x1, x9]");                                  // load byte
    emitter.instruction("sub w12, w11, #65");                                   // offset from 'A'=65
    emitter.instruction("cmp w12, #25");                                        // in [0,25] = uppercase
    emitter.instruction("b.hi __rt_strtotime_lc16_store");                      // not uppercase
    emitter.instruction("add w11, w11, #32");                                   // lowercase = uppercase + 32
    emitter.label("__rt_strtotime_lc16_store");
    emitter.instruction("strb w11, [x14, x9]");                                 // store into lc16 buffer
    emitter.instruction("add x9, x9, #1");                                      // advance index
    emitter.instruction("b __rt_strtotime_lc16_loop");                          // continue
    emitter.label("__rt_strtotime_lc16_done");
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_match_word_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime helper: match input prefix vs fixed-stride table ---");
    // Input:  x6 = candidate prefix pointer (lowercased), x7 = table address, x8 = available bytes
    // Output: x9 = kind byte (or -1 if no match), x10 = consumed bytes (0 if no match)
    emitter.label("__rt_strtotime_match_word");
    emitter.instruction("mov x10, #0");                                         // consumed = 0
    emitter.instruction("mov x9, #-1");                                         // kind = -1 (no match yet)

    emitter.label("__rt_strtotime_match_word_loop");
    emitter.instruction("ldrb w11, [x7, #10]");                                 // entry length byte
    emitter.instruction("cbz w11, __rt_strtotime_match_word_done");             // sentinel length 0 → end
    emitter.instruction("cmp x8, x11");                                         // input bytes >= entry length?
    emitter.instruction("b.lt __rt_strtotime_match_word_next");                 // no → skip this entry
    emitter.instruction("mov x12, #0");                                         // compare index

    emitter.label("__rt_strtotime_match_word_cmp");
    emitter.instruction("cmp x12, x11");                                        // all bytes compared?
    emitter.instruction("b.ge __rt_strtotime_match_word_boundary");             // yes → check word boundary
    emitter.instruction("ldrb w13, [x7, x12]");                                 // entry byte
    emitter.instruction("ldrb w14, [x6, x12]");                                 // input byte
    emitter.instruction("cmp w13, w14");                                        // equal?
    emitter.instruction("b.ne __rt_strtotime_match_word_next");                 // no → try next entry
    emitter.instruction("add x12, x12, #1");                                    // advance compare index
    emitter.instruction("b __rt_strtotime_match_word_cmp");                     // continue

    emitter.label("__rt_strtotime_match_word_boundary");
    emitter.instruction("cmp x12, x8");                                         // input ended right after match?
    emitter.instruction("b.eq __rt_strtotime_match_word_hit");                  // yes → match
    emitter.instruction("ldrb w13, [x6, x12]");                                 // next input byte
    emitter.instruction("sub w14, w13, #97");                                   // offset from 'a'=97
    emitter.instruction("cmp w14, #25");                                        // alpha (a-z) follows?
    emitter.instruction("b.ls __rt_strtotime_match_word_next");                 // yes → not a word boundary

    emitter.label("__rt_strtotime_match_word_hit");
    emitter.instruction("ldrb w9, [x7, #11]");                                  // kind byte from entry
    emitter.instruction("mov x10, x11");                                        // consumed = entry length
    emitter.instruction("b __rt_strtotime_match_word_done");                    // success

    emitter.label("__rt_strtotime_match_word_next");
    emitter.instruction("add x7, x7, #12");                                     // advance to next entry (12-byte stride)
    emitter.instruction("b __rt_strtotime_match_word_loop");                    // continue

    emitter.label("__rt_strtotime_match_word_done");
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_trim_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime helper: trim leading/trailing ASCII whitespace ---");
    emitter.label("__rt_strtotime_trim_linux_x86_64");

    // -- load saved ptr/len from dispatcher slots (via rbp-relative addressing) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // load saved input pointer (dispatcher [rsp+48])
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // load saved input length (dispatcher [rsp+56])

    emitter.label("__rt_strtotime_trim_lead_linux_x86_64");
    emitter.instruction("test rsi, rsi");                                       // length == 0 ?
    emitter.instruction("jz __rt_strtotime_trim_done_linux_x86_64");            // yes → done
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // load first byte
    emitter.instruction("cmp al, 32");                                          // space ?
    emitter.instruction("je __rt_strtotime_trim_lead_adv_linux_x86_64");        // yes → advance
    emitter.instruction("mov ecx, eax");                                        // copy byte for control whitespace check
    emitter.instruction("sub ecx, 9");                                          // normalize tab..carriage-return range
    emitter.instruction("cmp ecx, 4");                                          // ASCII control whitespace ?
    emitter.instruction("ja __rt_strtotime_trim_trail_linux_x86_64");           // not WS → trail
    emitter.label("__rt_strtotime_trim_lead_adv_linux_x86_64");
    emitter.instruction("inc rdi");                                             // advance pointer
    emitter.instruction("dec rsi");                                             // shrink length
    emitter.instruction("jmp __rt_strtotime_trim_lead_linux_x86_64");           // continue

    emitter.label("__rt_strtotime_trim_trail_linux_x86_64");
    emitter.instruction("test rsi, rsi");                                       // empty ?
    emitter.instruction("jz __rt_strtotime_trim_done_linux_x86_64");            // yes → done
    emitter.instruction("mov rax, rsi");                                        // copy length
    emitter.instruction("dec rax");                                             // offset of last byte
    emitter.instruction("movzx ecx, BYTE PTR [rdi + rax]");                     // load last byte
    emitter.instruction("cmp cl, 32");                                          // space ?
    emitter.instruction("je __rt_strtotime_trim_trail_adv_linux_x86_64");       // yes → shrink
    emitter.instruction("mov edx, ecx");                                        // copy byte for control whitespace check
    emitter.instruction("sub edx, 9");                                          // normalize tab..carriage-return range
    emitter.instruction("cmp edx, 4");                                          // ASCII control whitespace ?
    emitter.instruction("ja __rt_strtotime_trim_done_linux_x86_64");            // not WS → done
    emitter.label("__rt_strtotime_trim_trail_adv_linux_x86_64");
    emitter.instruction("dec rsi");                                             // shrink length
    emitter.instruction("jmp __rt_strtotime_trim_trail_linux_x86_64");          // continue

    emitter.label("__rt_strtotime_trim_done_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 80], rdi");                       // save trimmed pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], rsi");                       // save trimmed length
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_lc16_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime helper: lowercase first 16 input bytes into dispatcher [rsp+64..79] ---");
    emitter.label("__rt_strtotime_lc16_linux_x86_64");

    // -- zero-pad target buffer first --
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // clear bytes 0..7 of lc16 buffer
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // clear bytes 8..15 of lc16 buffer

    // -- compute copy length (min(len, 16)) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // load trimmed input pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // load trimmed input length
    emitter.instruction("mov rcx, 16");                                         // cap = 16
    emitter.instruction("cmp rsi, rcx");                                        // len vs 16
    emitter.instruction("cmovb rcx, rsi");                                      // rcx = min(len, 16)
    emitter.instruction("xor edx, edx");                                        // copy index = 0
    emitter.instruction("lea r11, [rbp - 64]");                                 // r11 = base of lc16 buffer

    emitter.label("__rt_strtotime_lc16_loop_linux_x86_64");
    emitter.instruction("cmp rdx, rcx");                                        // done ?
    emitter.instruction("jge __rt_strtotime_lc16_done_linux_x86_64");           // yes → return
    emitter.instruction("movzx eax, BYTE PTR [rdi + rdx]");                     // load input byte
    emitter.instruction("mov r8d, eax");                                        // copy for range check
    emitter.instruction("sub r8d, 65");                                         // offset from 'A'=65
    emitter.instruction("cmp r8d, 25");                                         // in [0,25] = uppercase
    emitter.instruction("ja __rt_strtotime_lc16_store_linux_x86_64");           // not uppercase → store as-is
    emitter.instruction("add eax, 32");                                         // to lowercase
    emitter.label("__rt_strtotime_lc16_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11 + rdx], al");                        // store into lc16 buffer
    emitter.instruction("inc rdx");                                             // advance index
    emitter.instruction("jmp __rt_strtotime_lc16_loop_linux_x86_64");           // continue

    emitter.label("__rt_strtotime_lc16_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_match_word_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime helper: match input prefix vs fixed-stride table ---");
    // Input:  rdi = candidate prefix pointer (lowercased), rsi = table address, rcx = available bytes
    // Output: rax = consumed bytes (0 if no match), rdx = kind byte (-1 if no match)
    // Caller-saved regs only: r8, r9, r10 as scratch.
    emitter.label("__rt_strtotime_match_word_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // consumed = 0
    emitter.instruction("mov rdx, -1");                                         // kind = -1 (no match yet)

    emitter.label("__rt_strtotime_match_word_loop_linux_x86_64");
    emitter.instruction("movzx r8d, BYTE PTR [rsi + 10]");                      // entry length byte
    emitter.instruction("test r8d, r8d");                                       // sentinel ?
    emitter.instruction("jz __rt_strtotime_match_word_done_linux_x86_64");      // length 0 → end
    emitter.instruction("cmp rcx, r8");                                         // available >= entry length ?
    emitter.instruction("jb __rt_strtotime_match_word_next_linux_x86_64");      // no → skip
    emitter.instruction("xor r9d, r9d");                                        // compare index = 0

    emitter.label("__rt_strtotime_match_word_cmp_linux_x86_64");
    emitter.instruction("cmp r9, r8");                                          // all bytes compared ?
    emitter.instruction("jge __rt_strtotime_match_word_boundary_linux_x86_64"); // yes → check boundary
    emitter.instruction("movzx r10d, BYTE PTR [rsi + r9]");                     // entry byte
    emitter.instruction("movzx r11d, BYTE PTR [rdi + r9]");                     // input byte
    emitter.instruction("cmp r10b, r11b");                                      // equal ?
    emitter.instruction("jne __rt_strtotime_match_word_next_linux_x86_64");     // no → try next entry
    emitter.instruction("inc r9");                                              // advance compare index
    emitter.instruction("jmp __rt_strtotime_match_word_cmp_linux_x86_64");      // continue

    emitter.label("__rt_strtotime_match_word_boundary_linux_x86_64");
    emitter.instruction("cmp r9, rcx");                                         // input ended right after match ?
    emitter.instruction("je __rt_strtotime_match_word_hit_linux_x86_64");       // yes → match
    emitter.instruction("movzx r10d, BYTE PTR [rdi + r9]");                     // next input byte
    emitter.instruction("mov r11d, r10d");                                      // copy for range check
    emitter.instruction("sub r11d, 97");                                        // offset from 'a'=97
    emitter.instruction("cmp r11d, 25");                                        // alpha follows ?
    emitter.instruction("jbe __rt_strtotime_match_word_next_linux_x86_64");     // yes → not a boundary, try next

    emitter.label("__rt_strtotime_match_word_hit_linux_x86_64");
    emitter.instruction("movsx rdx, BYTE PTR [rsi + 11]");                      // kind byte (sign-extend; future kinds may be negative)
    emitter.instruction("mov rax, r8");                                         // consumed = entry length
    emitter.instruction("jmp __rt_strtotime_match_word_done_linux_x86_64");     // success

    emitter.label("__rt_strtotime_match_word_next_linux_x86_64");
    emitter.instruction("add rsi, 12");                                         // advance to next entry (12-byte stride)
    emitter.instruction("jmp __rt_strtotime_match_word_loop_linux_x86_64");     // continue

    emitter.label("__rt_strtotime_match_word_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_skip_ws_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime helper: skip ASCII whitespace at cursor (x3) up to end (x4) ---");
    emitter.label("__rt_strtotime_skip_ws");

    emitter.label("__rt_strtotime_skip_ws_loop");
    emitter.instruction("cmp x3, x4");                                          // cursor reached end ?
    emitter.instruction("b.ge __rt_strtotime_skip_ws_done");                    // yes → return
    emitter.instruction("ldrb w9, [x3]");                                       // load byte at cursor
    emitter.instruction("cmp w9, #32");                                         // space ?
    emitter.instruction("b.eq __rt_strtotime_skip_ws_adv");                     // yes → advance
    emitter.instruction("sub w10, w9, #9");                                     // normalize ASCII control whitespace range
    emitter.instruction("cmp w10, #4");                                         // tab/newline/vtab/form-feed/carriage-return?
    emitter.instruction("b.hi __rt_strtotime_skip_ws_done");                    // not WS → done
    emitter.label("__rt_strtotime_skip_ws_adv");
    emitter.instruction("add x3, x3, #1");                                      // advance cursor
    emitter.instruction("b __rt_strtotime_skip_ws_loop");                       // continue
    emitter.label("__rt_strtotime_skip_ws_done");
    emitter.instruction("ret");                                                 // return
}

fn emit_parse_dec_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime helper: parse decimal digits at cursor (x3..x4) into x5 ---");
    // Outputs: x5 = parsed value, x3 = new cursor (after digits). Caller detects "no digits" by checking if x3 advanced.
    emitter.label("__rt_strtotime_parse_dec");
    emitter.instruction("mov x5, #0");                                          // accumulator = 0

    emitter.label("__rt_strtotime_parse_dec_loop");
    emitter.instruction("cmp x3, x4");                                          // reached end ?
    emitter.instruction("b.ge __rt_strtotime_parse_dec_done");                  // yes → return
    emitter.instruction("ldrb w9, [x3]");                                       // load byte
    emitter.instruction("sub w10, w9, #48");                                    // numeric value if digit
    emitter.instruction("cmp w10, #9");                                         // in [0,9] ?
    emitter.instruction("b.hi __rt_strtotime_parse_dec_done");                  // not a digit → done
    emitter.instruction("mov x11, #10");                                        // base 10
    emitter.instruction("mul x5, x5, x11");                                     // shift accumulator left by base
    emitter.instruction("add x5, x5, x10");                                     // accumulator += digit (sub-w cleared upper 32 bits of x10)
    emitter.instruction("add x3, x3, #1");                                      // advance cursor
    emitter.instruction("b __rt_strtotime_parse_dec_loop");                     // continue
    emitter.label("__rt_strtotime_parse_dec_done");
    emitter.instruction("ret");                                                 // return
}

fn emit_lc_cursor_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime helper: lowercase 16 bytes from cursor (x3..x4) into [sp+64..79] ---");
    emitter.label("__rt_strtotime_lc_cursor");
    emitter.instruction("stp xzr, xzr, [sp, #64]");                             // zero the lc16 buffer
    emitter.instruction("add x14, sp, #64");                                    // x14 = base of lc16 buffer
    emitter.instruction("mov x9, #0");                                          // copy index
    emitter.instruction("mov x10, #16");                                        // cap = 16
    emitter.instruction("sub x11, x4, x3");                                     // remaining bytes
    emitter.instruction("cmp x11, x10");                                        // remaining vs 16
    emitter.instruction("csel x10, x11, x10, lo");                              // x10 = min(remaining, 16)

    emitter.label("__rt_strtotime_lc_cursor_loop");
    emitter.instruction("cmp x9, x10");                                         // done ?
    emitter.instruction("b.ge __rt_strtotime_lc_cursor_done");                  // yes → ret
    emitter.instruction("ldrb w11, [x3, x9]");                                  // load byte from input at cursor
    emitter.instruction("sub w12, w11, #65");                                   // offset from 'A'
    emitter.instruction("cmp w12, #25");                                        // uppercase A-Z ?
    emitter.instruction("b.hi __rt_strtotime_lc_cursor_store");                 // no → store as-is
    emitter.instruction("add w11, w11, #32");                                   // to lowercase
    emitter.label("__rt_strtotime_lc_cursor_store");
    emitter.instruction("strb w11, [x14, x9]");                                 // store lowercased byte
    emitter.instruction("add x9, x9, #1");                                      // advance index
    emitter.instruction("b __rt_strtotime_lc_cursor_loop");                     // continue
    emitter.label("__rt_strtotime_lc_cursor_done");
    emitter.instruction("ret");                                                 // return
}

fn emit_skip_ws_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime helper: skip ASCII whitespace at cursor (rdi) up to end (r10) ---");
    emitter.label("__rt_strtotime_skip_ws_linux_x86_64");

    emitter.label("__rt_strtotime_skip_ws_loop_linux_x86_64");
    emitter.instruction("cmp rdi, r10");                                        // cursor reached end ?
    emitter.instruction("jge __rt_strtotime_skip_ws_done_linux_x86_64");        // yes → return
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // load byte at cursor
    emitter.instruction("cmp al, 32");                                          // space ?
    emitter.instruction("je __rt_strtotime_skip_ws_adv_linux_x86_64");          // yes → advance
    emitter.instruction("mov ecx, eax");                                        // copy byte for control whitespace check
    emitter.instruction("sub ecx, 9");                                          // normalize tab..carriage-return range
    emitter.instruction("cmp ecx, 4");                                          // ASCII control whitespace ?
    emitter.instruction("ja __rt_strtotime_skip_ws_done_linux_x86_64");         // not WS → done
    emitter.label("__rt_strtotime_skip_ws_adv_linux_x86_64");
    emitter.instruction("inc rdi");                                             // advance cursor
    emitter.instruction("jmp __rt_strtotime_skip_ws_loop_linux_x86_64");        // continue
    emitter.label("__rt_strtotime_skip_ws_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return
}

fn emit_parse_dec_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime helper: parse decimal digits at cursor (rdi..r10) into rax ---");
    // Outputs: rax = parsed value, rdi = new cursor. Caller detects "no digits" via cursor delta.
    emitter.label("__rt_strtotime_parse_dec_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // accumulator = 0

    emitter.label("__rt_strtotime_parse_dec_loop_linux_x86_64");
    emitter.instruction("cmp rdi, r10");                                        // reached end ?
    emitter.instruction("jge __rt_strtotime_parse_dec_done_linux_x86_64");      // yes → return
    emitter.instruction("movzx ecx, BYTE PTR [rdi]");                           // load byte
    emitter.instruction("mov r8d, ecx");                                        // copy for digit check
    emitter.instruction("sub r8d, 48");                                         // numeric value
    emitter.instruction("cmp r8d, 9");                                          // in [0,9] ?
    emitter.instruction("ja __rt_strtotime_parse_dec_done_linux_x86_64");       // not a digit → done
    emitter.instruction("imul rax, rax, 10");                                   // shift accumulator left by base
    emitter.instruction("movsxd r9, r8d");                                      // sign-extend digit
    emitter.instruction("add rax, r9");                                         // accumulator += digit
    emitter.instruction("inc rdi");                                             // advance cursor
    emitter.instruction("jmp __rt_strtotime_parse_dec_loop_linux_x86_64");      // continue
    emitter.label("__rt_strtotime_parse_dec_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return
}

fn emit_lc_cursor_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime helper: lowercase 16 bytes from cursor (rdi..r10) into [rbp-64..rbp-49] ---");
    emitter.label("__rt_strtotime_lc_cursor_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // zero lc16 buffer bytes 0..7
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // zero lc16 buffer bytes 8..15
    emitter.instruction("lea r11, [rbp - 64]");                                 // r11 = base of lc16 buffer
    emitter.instruction("xor edx, edx");                                        // copy index = 0
    emitter.instruction("mov rcx, 16");                                         // cap = 16
    emitter.instruction("mov r8, r10");                                         // copy end
    emitter.instruction("sub r8, rdi");                                         // remaining bytes
    emitter.instruction("cmp r8, rcx");                                         // remaining vs 16
    emitter.instruction("cmovb rcx, r8");                                       // rcx = min(remaining, 16)

    emitter.label("__rt_strtotime_lc_cursor_loop_linux_x86_64");
    emitter.instruction("cmp rdx, rcx");                                        // done ?
    emitter.instruction("jge __rt_strtotime_lc_cursor_done_linux_x86_64");      // yes → ret
    emitter.instruction("movzx eax, BYTE PTR [rdi + rdx]");                     // load input byte
    emitter.instruction("mov r8d, eax");                                        // copy for range check
    emitter.instruction("sub r8d, 65");                                         // offset from 'A'
    emitter.instruction("cmp r8d, 25");                                         // in [0,25] = uppercase
    emitter.instruction("ja __rt_strtotime_lc_cursor_store_linux_x86_64");      // no → store as-is
    emitter.instruction("add eax, 32");                                         // to lowercase
    emitter.label("__rt_strtotime_lc_cursor_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11 + rdx], al");                        // store byte
    emitter.instruction("inc rdx");                                             // advance index
    emitter.instruction("jmp __rt_strtotime_lc_cursor_loop_linux_x86_64");      // continue
    emitter.label("__rt_strtotime_lc_cursor_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return
}
