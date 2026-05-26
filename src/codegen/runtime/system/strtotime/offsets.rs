//! Purpose:
//! Emits the relative-offset parser sub-routine for `__rt_strtotime` — supports `[+-]?N unit`, `a/an unit`, composite forms (`"+1 day 2 hours"`), and trailing `ago`.
//! Combines per-term offsets into `tm_*` fields via the now_tm helper, then normalizes through libc `mktime` for DST-aware day/week math.
//!
//! Called from:
//! - `crate::codegen::runtime::system::strtotime::mod::emit_strtotime()` when the dispatcher classifies the first byte as `+`, `-`, or a non-iso/non-time digit.
//!
//! Key details:
//! - 6 i32 accumulators stored at `[sp+80..103]`: sec, min, hour, mday, mon, year. Week unit folds into mday (× 7).
//! - Trailing `ago` (case-insensitive) sets a flag at `[sp+108]`; if set, all accumulators are negated before tm assembly.
//! - Base time is `now_tm` (preserves current h/m/s) — matching PHP's `strtotime("+1 day")` = `now + 86400`-ish (DST-honored via mktime).
//! - Sign temp at `[sp+104]`, had_ago flag at `[sp+108]`. x86_64 also uses `[rsp+112]` (cursor save) and `[rsp+120]` (value save) across `match_word`/helpers that clobber rdi/rax.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Dispatches to the target-specific offsets emitter: ARM64 or x86_64 Linux.
/// The offsets strategy handles `[+-]?N unit`, `a/an unit`, composite forms, and trailing `ago`,
/// accumulating into `tm_*` fields before normalization via `mktime`.
pub(crate) fn emit_offsets(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_offsets_linux_x86_64(emitter);
        return;
    }

    emit_offsets_arm64(emitter);
}

/// Emits the relative-offset parser sub-routine for ARM64.
/// Parses `[+-]?N unit` terms (second/minute/hour/day/week/month/year), PHP articles
/// `a`/`an` (= 1), composite forms, and an optional trailing `ago`. Accumulators are
/// stored at `[sp+80..103]` (6 i32 slots). Week folds into day (× 7). The `ago` flag
/// at `[sp+108]` causes all accumulators to be negated before adding to the base `tm`
/// from `now_tm`. Finally calls libc `mktime` for normalization and DST-aware math.
fn emit_offsets_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: offsets (+N unit / -N unit / N unit ago) sub-routine ---");
    emitter.label("__rt_strtotime_offsets_entry");

    // -- initialize 6 accumulators and had_ago flag --
    emitter.instruction("stp xzr, xzr, [sp, #80]");                             // zero sec_acc + min_acc
    emitter.instruction("stp xzr, xzr, [sp, #88]");                             // zero hour_acc + mday_acc
    emitter.instruction("stp xzr, xzr, [sp, #96]");                             // zero mon_acc + year_acc
    emitter.instruction("str wzr, [sp, #108]");                                 // clear had_ago flag

    // -- set up cursor (x3) and end (x4) --
    emitter.instruction("ldr x3, [sp, #48]");                                   // cursor = trimmed input pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // x2 = trimmed length
    emitter.instruction("add x4, x3, x2");                                      // end = ptr + len

    emitter.label("__rt_strtotime_offsets_loop");
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip leading WS
    emitter.instruction("cmp x3, x4");                                          // end of input ?
    emitter.instruction("b.ge __rt_strtotime_offsets_after_loop");              // yes → apply

    // -- parse optional sign --
    emitter.instruction("mov w12, #1");                                         // sign = +1
    emitter.instruction("ldrb w9, [x3]");                                       // first char of term
    emitter.instruction("cmp w9, #43");                                         // '+' ?
    emitter.instruction("b.ne __rt_strtotime_offsets_check_neg");               // no → check '-'
    emitter.instruction("add x3, x3, #1");                                      // consume '+'
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip WS after sign
    emitter.instruction("b __rt_strtotime_offsets_save_sign");                  // proceed
    emitter.label("__rt_strtotime_offsets_check_neg");
    emitter.instruction("cmp w9, #45");                                         // '-' ?
    emitter.instruction("b.ne __rt_strtotime_offsets_save_sign");               // no sign → default +1
    emitter.instruction("mov w12, #-1");                                        // sign = -1
    emitter.instruction("add x3, x3, #1");                                      // consume '-'
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip WS after sign

    emitter.label("__rt_strtotime_offsets_save_sign");
    emitter.instruction("str w12, [sp, #104]");                                 // save sign across helper calls

    // -- parse decimal magnitude --
    emitter.instruction("mov x12, x3");                                         // remember cursor before parse_dec
    emitter.instruction("bl __rt_strtotime_parse_dec");                         // x5 = value, x3 = new cursor
    emitter.instruction("cmp x3, x12");                                         // cursor advanced ?
    emitter.instruction("b.ne __rt_strtotime_offsets_value_ready");             // numeric value parsed → continue

    // -- accept PHP relative articles: "a day", "an hour" --
    emitter.instruction("cmp x3, x4");                                          // any bytes left for a/an ?
    emitter.instruction("b.ge __rt_strtotime_fail");                            // no → fail
    emitter.instruction("ldrb w14, [x3]");                                      // load candidate article first byte
    emitter.instruction("orr w14, w14, #0x20");                                 // lowercase ASCII
    emitter.instruction("cmp w14, #97");                                        // 'a' ?
    emitter.instruction("b.ne __rt_strtotime_fail");                            // no article → fail
    emitter.instruction("add x15, x3, #1");                                     // position after "a"
    emitter.instruction("cmp x15, x4");                                         // input ended after "a" ?
    emitter.instruction("b.ge __rt_strtotime_offsets_article_a");               // consume it and let unit parsing fail if missing
    emitter.instruction("ldrb w14, [x3, #1]");                                  // load byte after "a"
    emitter.instruction("orr w14, w14, #0x20");                                 // lowercase ASCII
    emitter.instruction("cmp w14, #110");                                       // 'n' ?
    emitter.instruction("b.eq __rt_strtotime_offsets_article_an_check");        // maybe "an"
    emitter.instruction("sub w14, w14, #97");                                   // normalize byte after "a" for alpha boundary check
    emitter.instruction("cmp w14, #25");                                        // alpha immediately after "a" ?
    emitter.instruction("b.ls __rt_strtotime_fail");                            // yes → not the article word
    emitter.label("__rt_strtotime_offsets_article_a");
    emitter.instruction("add x3, x3, #1");                                      // consume "a"
    emitter.instruction("mov x5, #1");                                          // article magnitude = 1
    emitter.instruction("b __rt_strtotime_offsets_value_ready");                // continue with unit parsing
    emitter.label("__rt_strtotime_offsets_article_an_check");
    emitter.instruction("add x15, x3, #2");                                     // position after "an"
    emitter.instruction("cmp x15, x4");                                         // input ended after "an" ?
    emitter.instruction("b.ge __rt_strtotime_offsets_article_an");              // consume it and let unit parsing fail if missing
    emitter.instruction("ldrb w14, [x3, #2]");                                  // load byte after "an"
    emitter.instruction("orr w14, w14, #0x20");                                 // lowercase ASCII
    emitter.instruction("sub w14, w14, #97");                                   // normalize for alpha boundary check
    emitter.instruction("cmp w14, #25");                                        // alpha immediately after "an" ?
    emitter.instruction("b.ls __rt_strtotime_fail");                            // yes → not the article word
    emitter.label("__rt_strtotime_offsets_article_an");
    emitter.instruction("add x3, x3, #2");                                      // consume "an"
    emitter.instruction("mov x5, #1");                                          // article magnitude = 1

    emitter.label("__rt_strtotime_offsets_value_ready");
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // WS between number and unit

    // -- lowercase next 16 bytes from cursor into [sp+64..79] for unit match --
    emitter.instruction("bl __rt_strtotime_lc_cursor");                         // lc16 buffer rewritten

    // -- set up match_word args and call --
    emitter.instruction("add x6, sp, #64");                                     // candidate = lc16 buffer
    emitter.adrp("x7", "_strtotime_unit_tab");                                         // table base page
    emitter.add_lo12("x7", "x7", "_strtotime_unit_tab");                               // resolve table base
    emitter.instruction("sub x8, x4, x3");                                      // remaining bytes
    emitter.instruction("mov x11, #16");                                        // cap to lc16 size
    emitter.instruction("cmp x8, x11");                                         // remaining > 16 ?
    emitter.instruction("csel x8, x8, x11, lo");                                // x8 = min(remaining, 16)
    emitter.instruction("bl __rt_strtotime_match_word");                        // x9 = unit kind, x10 = consumed (0 = no match)

    emitter.instruction("cbz x10, __rt_strtotime_fail");                        // unit unmatched → fail

    // -- advance cursor past unit --
    emitter.instruction("add x3, x3, x10");                                     // consume unit bytes

    // -- compute signed value (signed_value = sign * magnitude) --
    emitter.instruction("ldr w12, [sp, #104]");                                 // reload sign
    emitter.instruction("sxtw x12, w12");                                       // sign-extend to i64
    emitter.instruction("mul x13, x5, x12");                                    // signed_value (low 32 used)

    // -- adjust kind: week (4) folds into mday with × 7; month/year (5,6) shift down by one --
    emitter.instruction("cmp w9, #4");                                          // week ?
    emitter.instruction("b.ne __rt_strtotime_offsets_kind_post_week");          // no → continue
    emitter.instruction("mov x14, #7");                                         // multiplier 7
    emitter.instruction("mul x13, x13, x14");                                   // signed_value *= 7
    emitter.instruction("mov w9, #3");                                          // kind = day
    emitter.instruction("b __rt_strtotime_offsets_kind_adjusted");              // skip month/year shift
    emitter.label("__rt_strtotime_offsets_kind_post_week");
    emitter.instruction("cmp w9, #4");                                          // kind > 4 ?
    emitter.instruction("b.le __rt_strtotime_offsets_kind_adjusted");           // kind 0..3 → no shift
    emitter.instruction("sub w9, w9, #1");                                      // kind 5→4, 6→5

    emitter.label("__rt_strtotime_offsets_kind_adjusted");
    // -- accumulator offset = 80 + adjusted_kind * 4 --
    emitter.instruction("lsl x14, x9, #2");                                     // adjusted_kind * 4
    emitter.instruction("add x14, x14, #80");                                   // absolute offset
    emitter.instruction("ldr w15, [sp, x14]");                                  // load current accumulator
    emitter.instruction("add w15, w15, w13");                                   // add signed_value
    emitter.instruction("str w15, [sp, x14]");                                  // store updated accumulator

    // -- check trailing 'ago' (case-insensitive) --
    emitter.instruction("bl __rt_strtotime_skip_ws");                           // skip WS after unit
    emitter.instruction("sub x11, x4, x3");                                     // remaining bytes
    emitter.instruction("cmp x11, #3");                                         // at least 3 for "ago" ?
    emitter.instruction("b.lt __rt_strtotime_offsets_loop");                    // no → next iter
    emitter.instruction("ldrb w14, [x3, #0]");                                  // first byte
    emitter.instruction("orr w14, w14, #0x20");                                 // lowercase ASCII
    emitter.instruction("cmp w14, #97");                                        // 'a' ?
    emitter.instruction("b.ne __rt_strtotime_offsets_loop");                    // no
    emitter.instruction("ldrb w14, [x3, #1]");                                  // second byte
    emitter.instruction("orr w14, w14, #0x20");                                 // lowercase
    emitter.instruction("cmp w14, #103");                                       // 'g' ?
    emitter.instruction("b.ne __rt_strtotime_offsets_loop");                    // no
    emitter.instruction("ldrb w14, [x3, #2]");                                  // third byte
    emitter.instruction("orr w14, w14, #0x20");                                 // lowercase
    emitter.instruction("cmp w14, #111");                                       // 'o' ?
    emitter.instruction("b.ne __rt_strtotime_offsets_loop");                    // no

    // -- word boundary: byte at offset 3 (if present) must not be ASCII alpha --
    emitter.instruction("cmp x11, #4");                                         // more than 3 bytes left ?
    emitter.instruction("b.lt __rt_strtotime_offsets_ago_matched");             // exactly 3 → boundary OK
    emitter.instruction("ldrb w14, [x3, #3]");                                  // next byte after "ago"
    emitter.instruction("orr w14, w14, #0x20");                                 // lowercase
    emitter.instruction("sub w14, w14, #97");                                   // offset from 'a'
    emitter.instruction("cmp w14, #25");                                        // alpha follows ?
    emitter.instruction("b.ls __rt_strtotime_offsets_loop");                    // yes → not "ago"

    emitter.label("__rt_strtotime_offsets_ago_matched");
    emitter.instruction("add x3, x3, #3");                                      // consume "ago"
    emitter.instruction("mov w14, #1");                                         // had_ago = 1
    emitter.instruction("str w14, [sp, #108]");                                 // record flag
    emitter.instruction("b __rt_strtotime_offsets_loop");                       // continue loop

    emitter.label("__rt_strtotime_offsets_after_loop");
    // -- if had_ago, negate all accumulators --
    emitter.instruction("ldr w9, [sp, #108]");                                  // load had_ago
    emitter.instruction("cbz w9, __rt_strtotime_offsets_build_tm");             // not set → skip
    emitter.instruction("ldr w9, [sp, #80]");                                   // sec_acc
    emitter.instruction("neg w9, w9");                                          // negate
    emitter.instruction("str w9, [sp, #80]");                                   // store
    emitter.instruction("ldr w9, [sp, #84]");                                   // min_acc
    emitter.instruction("neg w9, w9");                                          // negate
    emitter.instruction("str w9, [sp, #84]");                                   // store
    emitter.instruction("ldr w9, [sp, #88]");                                   // hour_acc
    emitter.instruction("neg w9, w9");                                          // negate
    emitter.instruction("str w9, [sp, #88]");                                   // store
    emitter.instruction("ldr w9, [sp, #92]");                                   // mday_acc
    emitter.instruction("neg w9, w9");                                          // negate
    emitter.instruction("str w9, [sp, #92]");                                   // store
    emitter.instruction("ldr w9, [sp, #96]");                                   // mon_acc
    emitter.instruction("neg w9, w9");                                          // negate
    emitter.instruction("str w9, [sp, #96]");                                   // store
    emitter.instruction("ldr w9, [sp, #100]");                                  // year_acc
    emitter.instruction("neg w9, w9");                                          // negate
    emitter.instruction("str w9, [sp, #100]");                                  // store

    emitter.label("__rt_strtotime_offsets_build_tm");
    emitter.instruction("bl __rt_strtotime_now_tm");                            // build NOW's tm into [sp+0..36]

    // -- add accumulators to tm fields --
    emitter.instruction("ldr w9, [sp, #0]");                                    // tm_sec
    emitter.instruction("ldr w10, [sp, #80]");                                  // sec_acc
    emitter.instruction("add w9, w9, w10");                                     // tm_sec += sec_acc
    emitter.instruction("str w9, [sp, #0]");                                    // store

    emitter.instruction("ldr w9, [sp, #4]");                                    // tm_min
    emitter.instruction("ldr w10, [sp, #84]");                                  // min_acc
    emitter.instruction("add w9, w9, w10");                                     // tm_min += min_acc
    emitter.instruction("str w9, [sp, #4]");                                    // store

    emitter.instruction("ldr w9, [sp, #8]");                                    // tm_hour
    emitter.instruction("ldr w10, [sp, #88]");                                  // hour_acc
    emitter.instruction("add w9, w9, w10");                                     // tm_hour += hour_acc
    emitter.instruction("str w9, [sp, #8]");                                    // store

    emitter.instruction("ldr w9, [sp, #12]");                                   // tm_mday
    emitter.instruction("ldr w10, [sp, #92]");                                  // mday_acc
    emitter.instruction("add w9, w9, w10");                                     // tm_mday += mday_acc
    emitter.instruction("str w9, [sp, #12]");                                   // store

    emitter.instruction("ldr w9, [sp, #16]");                                   // tm_mon
    emitter.instruction("ldr w10, [sp, #96]");                                  // mon_acc
    emitter.instruction("add w9, w9, w10");                                     // tm_mon += mon_acc
    emitter.instruction("str w9, [sp, #16]");                                   // store

    emitter.instruction("ldr w9, [sp, #20]");                                   // tm_year
    emitter.instruction("ldr w10, [sp, #100]");                                 // year_acc
    emitter.instruction("add w9, w9, w10");                                     // tm_year += year_acc
    emitter.instruction("str w9, [sp, #20]");                                   // store

    // -- call mktime --
    emitter.instruction("mov x0, sp");                                          // x0 = &tm
    emitter.bl_c("mktime");                                                     // → x0 = ts (normalizes overflow)
    emitter.instruction("b __rt_strtotime_ret");                                // return through shared epilogue
}

/// Emits the relative-offset parser sub-routine for x86_64 Linux.
fn emit_offsets_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: offsets sub-routine ---");
    emitter.label("__rt_strtotime_offsets_entry_linux_x86_64");

    // -- initialize accumulators + had_ago flag --
    emitter.instruction("mov QWORD PTR [rsp + 80], 0");                         // sec_acc + min_acc
    emitter.instruction("mov QWORD PTR [rsp + 88], 0");                         // hour_acc + mday_acc
    emitter.instruction("mov QWORD PTR [rsp + 96], 0");                         // mon_acc + year_acc
    emitter.instruction("mov DWORD PTR [rsp + 108], 0");                        // had_ago flag

    // -- cursor (rdi), end (r10) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // cursor = trimmed pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // rsi = trimmed length
    emitter.instruction("mov r10, rdi");                                        // r10 = cursor (will become end)
    emitter.instruction("add r10, rsi");                                        // end = ptr + len

    emitter.label("__rt_strtotime_offsets_loop_linux_x86_64");
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip leading WS
    emitter.instruction("cmp rdi, r10");                                        // end of input ?
    emitter.instruction("jge __rt_strtotime_offsets_after_loop_linux_x86_64");  // yes → apply

    // -- optional sign --
    emitter.instruction("mov ecx, 1");                                          // sign = +1
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // first char of term
    emitter.instruction("cmp al, 43");                                          // '+' ?
    emitter.instruction("jne __rt_strtotime_offsets_check_neg_linux_x86_64");   // no → check '-'
    emitter.instruction("inc rdi");                                             // consume '+'
    emitter.instruction("mov DWORD PTR [rsp + 104], ecx");                      // save sign before whitespace helper clobbers ecx
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip WS after sign
    emitter.instruction("jmp __rt_strtotime_offsets_parse_value_linux_x86_64"); // proceed
    emitter.label("__rt_strtotime_offsets_check_neg_linux_x86_64");
    emitter.instruction("cmp al, 45");                                          // '-' ?
    emitter.instruction("jne __rt_strtotime_offsets_save_sign_linux_x86_64");   // no sign → default +1
    emitter.instruction("mov ecx, -1");                                         // sign = -1
    emitter.instruction("inc rdi");                                             // consume '-'
    emitter.instruction("mov DWORD PTR [rsp + 104], ecx");                      // save sign before whitespace helper clobbers ecx
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip WS after sign
    emitter.instruction("jmp __rt_strtotime_offsets_parse_value_linux_x86_64"); // proceed

    emitter.label("__rt_strtotime_offsets_save_sign_linux_x86_64");
    emitter.instruction("mov DWORD PTR [rsp + 104], ecx");                      // save sign across helper calls

    // -- parse decimal magnitude or PHP relative articles a/an --
    emitter.label("__rt_strtotime_offsets_parse_value_linux_x86_64");
    emitter.instruction("mov r11, rdi");                                        // cursor before parse_dec
    emitter.instruction("call __rt_strtotime_parse_dec_linux_x86_64");          // rax = value, rdi = new cursor
    emitter.instruction("cmp rdi, r11");                                        // cursor advanced ?
    emitter.instruction("jne __rt_strtotime_offsets_value_ready_linux_x86_64"); // numeric value parsed → continue
    emitter.instruction("cmp rdi, r10");                                        // any bytes left for a/an ?
    emitter.instruction("jge __rt_strtotime_fail_linux_x86_64");                // no → fail
    emitter.instruction("movzx eax, BYTE PTR [rdi]");                           // load candidate article first byte
    emitter.instruction("or al, 32");                                           // lowercase ASCII
    emitter.instruction("cmp al, 97");                                          // 'a' ?
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // no article → fail
    emitter.instruction("lea r8, [rdi + 1]");                                   // position after "a"
    emitter.instruction("cmp r8, r10");                                         // input ended after "a" ?
    emitter.instruction("jge __rt_strtotime_offsets_article_a_linux_x86_64");   // consume it and let unit parsing fail if missing
    emitter.instruction("movzx eax, BYTE PTR [rdi + 1]");                       // load byte after "a"
    emitter.instruction("or al, 32");                                           // lowercase ASCII
    emitter.instruction("cmp al, 110");                                         // 'n' ?
    emitter.instruction("je __rt_strtotime_offsets_article_an_check_linux_x86_64"); // maybe "an"
    emitter.instruction("mov ecx, eax");                                        // copy byte for boundary check
    emitter.instruction("sub ecx, 97");                                         // normalize byte after "a"
    emitter.instruction("cmp ecx, 25");                                         // alpha immediately after "a" ?
    emitter.instruction("jbe __rt_strtotime_fail_linux_x86_64");                // yes → not the article word
    emitter.label("__rt_strtotime_offsets_article_a_linux_x86_64");
    emitter.instruction("inc rdi");                                             // consume "a"
    emitter.instruction("mov rax, 1");                                          // article magnitude = 1
    emitter.instruction("jmp __rt_strtotime_offsets_value_ready_linux_x86_64"); // continue with unit parsing
    emitter.label("__rt_strtotime_offsets_article_an_check_linux_x86_64");
    emitter.instruction("lea r8, [rdi + 2]");                                   // position after "an"
    emitter.instruction("cmp r8, r10");                                         // input ended after "an" ?
    emitter.instruction("jge __rt_strtotime_offsets_article_an_linux_x86_64");  // consume it and let unit parsing fail if missing
    emitter.instruction("movzx eax, BYTE PTR [rdi + 2]");                       // load byte after "an"
    emitter.instruction("or al, 32");                                           // lowercase ASCII
    emitter.instruction("mov ecx, eax");                                        // copy byte for boundary check
    emitter.instruction("sub ecx, 97");                                         // normalize byte after "an"
    emitter.instruction("cmp ecx, 25");                                         // alpha immediately after "an" ?
    emitter.instruction("jbe __rt_strtotime_fail_linux_x86_64");                // yes → not the article word
    emitter.label("__rt_strtotime_offsets_article_an_linux_x86_64");
    emitter.instruction("add rdi, 2");                                          // consume "an"
    emitter.instruction("mov rax, 1");                                          // article magnitude = 1

    emitter.label("__rt_strtotime_offsets_value_ready_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rsp + 120], rax");                      // save value across upcoming helpers
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // WS between number and unit

    // -- lowercase next 16 bytes from cursor into [rbp-64..rbp-49] --
    emitter.instruction("call __rt_strtotime_lc_cursor_linux_x86_64");          // lc16 buffer rewritten

    // -- save cursor before match_word (which clobbers rdi/r10) --
    emitter.instruction("mov QWORD PTR [rsp + 112], rdi");                      // save cursor

    // -- set up match_word args (rdi=cand, rsi=table, rcx=avail) --
    emitter.instruction("lea rdi, [rbp - 64]");                                 // cand_ptr = lc16 base
    emitter.instruction("lea rsi, [rip + _strtotime_unit_tab]");                // table base
    emitter.instruction("mov rcx, r10");                                        // rcx = end
    emitter.instruction("sub rcx, QWORD PTR [rsp + 112]");                      // rcx = end - saved_cursor = remaining
    emitter.instruction("mov r8, 16");                                          // cap window
    emitter.instruction("cmp rcx, r8");                                         // remaining > 16 ?
    emitter.instruction("cmovae rcx, r8");                                      // rcx = min(remaining, 16)
    emitter.instruction("call __rt_strtotime_match_word_linux_x86_64");         // rax = consumed, rdx = kind

    // -- restore cursor + recompute end --
    emitter.instruction("mov rdi, QWORD PTR [rsp + 112]");                      // restore cursor
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload original trimmed ptr
    emitter.instruction("add r10, QWORD PTR [rbp - 72]");                       // recompute end = ptr + len

    emitter.instruction("test rax, rax");                                       // unit matched ?
    emitter.instruction("jz __rt_strtotime_fail_linux_x86_64");                 // no → fail

    emitter.instruction("add rdi, rax");                                        // advance cursor past unit

    // -- accumulate: signed_value = sign * value; accumulator[adjusted_kind] += signed_value --
    emitter.instruction("mov rcx, QWORD PTR [rsp + 120]");                      // reload value
    emitter.instruction("movsxd r11, DWORD PTR [rsp + 104]");                   // sign-extended sign
    emitter.instruction("imul rcx, r11");                                       // rcx = sign * value

    // adjust kind: week (4) → mday (3) with × 7; kinds 5,6 → 4,5
    emitter.instruction("cmp rdx, 4");                                          // week ?
    emitter.instruction("jne __rt_strtotime_offsets_kind_post_week_linux_x86_64"); // non-week unit → remap higher kinds
    emitter.instruction("imul rcx, rcx, 7");                                    // signed_value *= 7
    emitter.instruction("mov rdx, 3");                                          // adjusted kind = day
    emitter.instruction("jmp __rt_strtotime_offsets_kind_adjusted_linux_x86_64"); // use adjusted unit kind
    emitter.label("__rt_strtotime_offsets_kind_post_week_linux_x86_64");
    emitter.instruction("cmp rdx, 4");                                          // kind > 4 ?
    emitter.instruction("jle __rt_strtotime_offsets_kind_adjusted_linux_x86_64"); // day-or-smaller unit needs no remap
    emitter.instruction("dec rdx");                                             // kind 5→4, 6→5

    emitter.label("__rt_strtotime_offsets_kind_adjusted_linux_x86_64");
    emitter.instruction("mov r8, rdx");                                         // r8 = adjusted kind
    emitter.instruction("shl r8, 2");                                           // r8 = adjusted_kind * 4
    emitter.instruction("add r8, 80");                                          // r8 = absolute offset
    emitter.instruction("mov r9d, DWORD PTR [rsp + r8]");                       // load accumulator
    emitter.instruction("add r9d, ecx");                                        // add signed_value (low 32)
    emitter.instruction("mov DWORD PTR [rsp + r8], r9d");                       // store back

    // -- check trailing 'ago' (case-insensitive) --
    emitter.instruction("call __rt_strtotime_skip_ws_linux_x86_64");            // skip WS after unit
    emitter.instruction("mov r11, r10");                                        // r11 = end
    emitter.instruction("sub r11, rdi");                                        // remaining bytes
    emitter.instruction("cmp r11, 3");                                          // at least 3 ?
    emitter.instruction("jl __rt_strtotime_offsets_loop_linux_x86_64");         // no → next iter

    emitter.instruction("movzx eax, BYTE PTR [rdi + 0]");                       // first byte
    emitter.instruction("or al, 32");                                           // lowercase
    emitter.instruction("cmp al, 97");                                          // 'a' ?
    emitter.instruction("jne __rt_strtotime_offsets_loop_linux_x86_64");        // not "ago" → parse next term
    emitter.instruction("movzx eax, BYTE PTR [rdi + 1]");                       // second byte
    emitter.instruction("or al, 32");                                           // lowercase
    emitter.instruction("cmp al, 103");                                         // 'g' ?
    emitter.instruction("jne __rt_strtotime_offsets_loop_linux_x86_64");        // not "ago" → parse next term
    emitter.instruction("movzx eax, BYTE PTR [rdi + 2]");                       // third byte
    emitter.instruction("or al, 32");                                           // lowercase
    emitter.instruction("cmp al, 111");                                         // 'o' ?
    emitter.instruction("jne __rt_strtotime_offsets_loop_linux_x86_64");        // not "ago" → parse next term

    emitter.instruction("cmp r11, 4");                                          // more than 3 bytes left ?
    emitter.instruction("jl __rt_strtotime_offsets_ago_matched_linux_x86_64");  // exactly 3 → boundary OK
    emitter.instruction("movzx eax, BYTE PTR [rdi + 3]");                       // next byte
    emitter.instruction("or al, 32");                                           // lowercase
    emitter.instruction("mov ecx, eax");                                        // copy
    emitter.instruction("sub ecx, 97");                                         // offset from 'a'
    emitter.instruction("cmp ecx, 25");                                         // alpha ?
    emitter.instruction("jbe __rt_strtotime_offsets_loop_linux_x86_64");        // yes → not "ago"

    emitter.label("__rt_strtotime_offsets_ago_matched_linux_x86_64");
    emitter.instruction("add rdi, 3");                                          // consume "ago"
    emitter.instruction("mov DWORD PTR [rsp + 108], 1");                        // had_ago = 1
    emitter.instruction("jmp __rt_strtotime_offsets_loop_linux_x86_64");        // continue

    emitter.label("__rt_strtotime_offsets_after_loop_linux_x86_64");
    // -- if had_ago, negate all accumulators --
    emitter.instruction("mov ecx, DWORD PTR [rsp + 108]");                      // load had_ago
    emitter.instruction("test ecx, ecx");                                       // set ?
    emitter.instruction("jz __rt_strtotime_offsets_build_tm_linux_x86_64");     // no → skip
    emitter.instruction("neg DWORD PTR [rsp + 80]");                            // -sec_acc
    emitter.instruction("neg DWORD PTR [rsp + 84]");                            // -min_acc
    emitter.instruction("neg DWORD PTR [rsp + 88]");                            // -hour_acc
    emitter.instruction("neg DWORD PTR [rsp + 92]");                            // -mday_acc
    emitter.instruction("neg DWORD PTR [rsp + 96]");                            // -mon_acc
    emitter.instruction("neg DWORD PTR [rsp + 100]");                           // -year_acc

    emitter.label("__rt_strtotime_offsets_build_tm_linux_x86_64");
    emitter.instruction("call __rt_strtotime_now_tm_linux_x86_64");             // populate tm with NOW

    // -- add accumulators to tm fields --
    emitter.instruction("mov eax, DWORD PTR [rsp + 80]");                       // sec_acc
    emitter.instruction("add DWORD PTR [rsp + 0], eax");                        // tm_sec += sec_acc
    emitter.instruction("mov eax, DWORD PTR [rsp + 84]");                       // min_acc
    emitter.instruction("add DWORD PTR [rsp + 4], eax");                        // tm_min += min_acc
    emitter.instruction("mov eax, DWORD PTR [rsp + 88]");                       // hour_acc
    emitter.instruction("add DWORD PTR [rsp + 8], eax");                        // tm_hour += hour_acc
    emitter.instruction("mov eax, DWORD PTR [rsp + 92]");                       // mday_acc
    emitter.instruction("add DWORD PTR [rsp + 12], eax");                       // tm_mday += mday_acc
    emitter.instruction("mov eax, DWORD PTR [rsp + 96]");                       // mon_acc
    emitter.instruction("add DWORD PTR [rsp + 16], eax");                       // tm_mon += mon_acc
    emitter.instruction("mov eax, DWORD PTR [rsp + 100]");                      // year_acc
    emitter.instruction("add DWORD PTR [rsp + 20], eax");                       // tm_year += year_acc

    // -- call mktime --
    emitter.instruction("mov rdi, rsp");                                        // rdi = &tm
    emitter.instruction("call mktime");                                         // → rax = ts (normalizes overflow)
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return through shared epilogue
}
