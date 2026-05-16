//! Purpose:
//! Dispatches the `__rt_strtotime` runtime helper to strategy-specific emitter modules.
//! The module owns the public entry point `__rt_strtotime`, the dispatcher frame, and the shared epilogue (`__rt_strtotime_ret` / `__rt_strtotime_fail`).
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//! - `crate::codegen::runtime::data::fixed` for the `emit_strtotime_data()` lookup tables.
//!
//! Key details:
//! - Public label `__rt_strtotime`: `x1=ptr, x2=len → x0=timestamp`; `-1` on parse failure. ABI unchanged from the original implementation.
//! - 128-byte stack frame layout (ARM64; x86_64 mirrors numerically via `[rbp - 128 + N]`):
//!     `[sp+ 0..47]` struct tm scratch     —   9 ints for libc mktime
//!     `[sp+48..55]` saved trimmed ptr
//!     `[sp+56..63]` saved trimmed len
//!     `[sp+64..79]` lc16 buffer            — first 16 lowercased input bytes, zero-padded
//!     `[sp+80..111]` scratch slots          — used by today_tm and future strategies
//!     `[sp+112..127]` saved x29/x30 (ARM64)
//! - Dispatcher first-byte switch: digit → iso_date; ASCII alpha → keyword table; else → fail.

mod data;
mod iso_date;
mod keywords;
mod offsets;
mod shared;
mod time_only;
mod weekdays;

use crate::codegen::{emit::Emitter, platform::Arch};

pub(crate) use data::emit_strtotime_data;

/// `__rt_strtotime`: parse a date/time string into a Unix timestamp.
/// Input:  `x1=string ptr, x2=string len`   (`rdi`/`rsi` on x86_64)
/// Output: `x0=Unix timestamp` (or `-1` on failure)   (`rax` on x86_64)
pub(crate) fn emit_strtotime(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_dispatcher_linux_x86_64(emitter);
        iso_date::emit_iso_date(emitter);
        time_only::emit_time_only(emitter);
        offsets::emit_offsets(emitter);
        keywords::emit_keywords(emitter);
        weekdays::emit_weekdays(emitter);
        shared::emit_helpers(emitter);
        emit_epilogue_linux_x86_64(emitter);
        return;
    }

    emit_dispatcher_arm64(emitter);
    iso_date::emit_iso_date(emitter);
    time_only::emit_time_only(emitter);
    offsets::emit_offsets(emitter);
    keywords::emit_keywords(emitter);
    weekdays::emit_weekdays(emitter);
    shared::emit_helpers(emitter);
    emit_epilogue_arm64(emitter);
}

fn emit_dispatcher_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtotime ---");
    emitter.label_global("__rt_strtotime");

    // -- set up dispatcher frame (128 bytes, 16-byte aligned) --
    emitter.instruction("sub sp, sp, #128");                                    // allocate dispatcher frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // new frame pointer

    // -- save original input ptr/len into reserved slots --
    emitter.instruction("str x1, [sp, #48]");                                   // save input pointer
    emitter.instruction("str x2, [sp, #56]");                                   // save input length

    // -- trim leading/trailing ASCII whitespace --
    emitter.instruction("bl __rt_strtotime_trim");                              // [sp+48]/[sp+56] now hold trimmed values
    emitter.instruction("ldr x2, [sp, #56]");                                   // reload trimmed length
    emitter.instruction("cbz x2, __rt_strtotime_fail");                         // empty after trim → fail

    // -- lowercase first 16 bytes into [sp+64..79] --
    emitter.instruction("bl __rt_strtotime_lc16");                              // fills lc16 buffer

    // -- classify on first lowercased char --
    emitter.instruction("ldrb w9, [sp, #64]");                                  // load first lc16 byte
    emitter.instruction("sub w10, w9, #48");                                    // '0' = 48
    emitter.instruction("cmp w10, #9");                                         // digit (0-9) ?
    emitter.instruction("b.hi __rt_strtotime_classify_alpha");                  // not digit → try alpha

    // -- digit: probe for HH:MM[:SS] then ISO date, else offsets --
    emitter.instruction("ldr x2, [sp, #56]");                                   // reload trimmed length
    emitter.instruction("cmp x2, #4");                                          // shortest time-only is "H:MM" = 4 chars
    emitter.instruction("b.lt __rt_strtotime_offsets_entry");                   // too short for date/time → offsets
    emitter.instruction("ldrb w11, [sp, #65]");                                 // lc16[1] (second char)
    emitter.instruction("cmp w11, #58");                                        // ':' ?
    emitter.instruction("b.eq __rt_strtotime_time_entry");                      // → time-only (H:MM[:SS])
    emitter.instruction("ldrb w11, [sp, #66]");                                 // lc16[2] (third char)
    emitter.instruction("cmp w11, #58");                                        // ':' ?
    emitter.instruction("b.eq __rt_strtotime_time_entry");                      // → time-only (HH:MM[:SS])
    emitter.instruction("cmp x2, #10");                                         // ISO date needs ≥ 10 chars
    emitter.instruction("b.lt __rt_strtotime_offsets_entry");                   // too short for ISO → offsets
    emitter.instruction("ldrb w11, [sp, #68]");                                 // lc16[4] (offset 4 of date)
    emitter.instruction("cmp w11, #45");                                        // '-' ?
    emitter.instruction("b.eq __rt_strtotime_iso_entry");                       // YYYY-MM-DD → ISO
    emitter.instruction("b __rt_strtotime_offsets_entry");                      // default for digit-starting: offsets

    emitter.label("__rt_strtotime_classify_alpha");
    // -- check for '+' / '-' signs → offsets entry --
    emitter.instruction("cmp w9, #43");                                         // '+' ?
    emitter.instruction("b.eq __rt_strtotime_offsets_entry");                   // → offsets
    emitter.instruction("cmp w9, #45");                                         // '-' ?
    emitter.instruction("b.eq __rt_strtotime_offsets_entry");                   // → offsets
    emitter.instruction("sub w10, w9, #97");                                    // 'a' = 97
    emitter.instruction("cmp w10, #25");                                        // ASCII alpha (a-z) ?
    emitter.instruction("b.hi __rt_strtotime_fail");                            // not alpha → fail
    emitter.instruction("cmp w9, #97");                                         // possible "a/an <unit>" article-relative form ?
    emitter.instruction("b.eq __rt_strtotime_offsets_entry");                   // let offsets parse or reject the article form

    // -- alpha: try keyword table match --
    emitter.instruction("add x6, sp, #64");                                     // x6 = lc16 buffer ptr (candidate)
    emitter.adrp("x7", "_strtotime_keyword_tab");                                      // load page of keyword table
    emitter.add_lo12("x7", "x7", "_strtotime_keyword_tab");                            // resolve full address
    emitter.instruction("ldr x8, [sp, #56]");                                   // x8 = trimmed input length
    emitter.instruction("mov x11, #16");                                        // cap candidate window to lc16 size
    emitter.instruction("cmp x8, x11");                                         // available > 16 ?
    emitter.instruction("csel x8, x8, x11, lo");                                // x8 = min(len, 16)
    emitter.instruction("bl __rt_strtotime_match_word");                        // → x9=kind (-1 if no match), x10=consumed

    emitter.instruction("cbz x10, __rt_strtotime_fail");                        // no match → fail
    emitter.instruction("cmp x9, #5");                                          // kind in 0..5 = bare keyword ?
    emitter.instruction("b.hi __rt_strtotime_alpha_not_keyword");               // no → check modifiers/weekdays
    emitter.instruction("ldr x8, [sp, #56]");                                   // reload trimmed input length
    emitter.instruction("cmp x10, x8");                                         // keyword consumed the whole input ?
    emitter.instruction("b.ne __rt_strtotime_fail");                            // trailing junk after keyword → fail
    emitter.instruction("b __rt_strtotime_kw_entry");                           // → keyword strategy

    emitter.label("__rt_strtotime_alpha_not_keyword");
    emitter.instruction("cmp x9, #8");                                          // kind 6..8 = next/last/this modifier ?
    emitter.instruction("b.ls __rt_strtotime_weekdays_entry");                  // → weekdays strategy with modifier
    emitter.instruction("cmp x9, #10");                                         // kind 9 = bare "ago" (not a top-level term)
    emitter.instruction("b.lt __rt_strtotime_fail");                            // → fail
    emitter.instruction("cmp x9, #16");                                         // kind 10..16 = weekday name ?
    emitter.instruction("b.le __rt_strtotime_alpha_direct_weekday");            // yes → direct weekday strategy
    emitter.instruction("cmp x9, #18");                                         // kind 17..18 = a/an relative magnitude ?
    emitter.instruction("b.le __rt_strtotime_offsets_entry");                   // let the offsets strategy parse the full relative expression
    emitter.instruction("b __rt_strtotime_fail");                               // unknown kind → fail
    emitter.label("__rt_strtotime_alpha_direct_weekday");
    emitter.instruction("ldr x8, [sp, #56]");                                   // reload trimmed input length
    emitter.instruction("cmp x10, x8");                                         // weekday consumed the whole input ?
    emitter.instruction("b.ne __rt_strtotime_fail");                            // trailing junk after weekday → fail
    emitter.instruction("b __rt_strtotime_weekdays_entry");                     // → weekdays strategy
}

fn emit_epilogue_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: shared epilogue ---");
    emitter.label("__rt_strtotime_fail");
    emitter.instruction("mov x0, #-1");                                         // failure result

    emitter.label("__rt_strtotime_ret");
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // deallocate dispatcher frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_dispatcher_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtotime ---");
    emitter.label_global("__rt_strtotime");

    // -- set up dispatcher frame (128 bytes, 16-byte aligned) --
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish dispatcher frame base
    emitter.instruction("sub rsp, 128");                                        // reserve dispatcher locals; rsp 16-byte aligned (128 % 16 == 0)

    // -- save original input ptr/len --
    emitter.instruction("mov QWORD PTR [rbp - 80], rdi");                       // save input pointer ([rsp+48] in dispatcher view)
    emitter.instruction("mov QWORD PTR [rbp - 72], rsi");                       // save input length ([rsp+56] in dispatcher view)

    // -- trim leading/trailing whitespace --
    emitter.instruction("call __rt_strtotime_trim_linux_x86_64");               // [rbp-80]/[rbp-72] now hold trimmed values
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // reload trimmed length
    emitter.instruction("test rsi, rsi");                                       // empty after trim ?
    emitter.instruction("jz __rt_strtotime_fail_linux_x86_64");                 // yes → fail

    // -- lowercase first 16 bytes into [rbp-64..rbp-49] --
    emitter.instruction("call __rt_strtotime_lc16_linux_x86_64");               // fills lc16 buffer

    // -- classify on first lowercased char --
    emitter.instruction("movzx eax, BYTE PTR [rbp - 64]");                      // load first lc16 byte
    emitter.instruction("mov ecx, eax");                                        // copy for range checks
    emitter.instruction("sub ecx, 48");                                         // '0' = 48
    emitter.instruction("cmp ecx, 9");                                          // digit ?
    emitter.instruction("ja __rt_strtotime_classify_alpha_linux_x86_64");       // not digit → try alpha

    // -- digit: probe for HH:MM[:SS] then ISO date, else offsets --
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // reload trimmed length
    emitter.instruction("cmp rsi, 4");                                          // shortest time-only is "H:MM"
    emitter.instruction("jl __rt_strtotime_offsets_entry_linux_x86_64");        // too short for date/time → offsets
    emitter.instruction("movzx r8d, BYTE PTR [rbp - 63]");                      // lc16[1]
    emitter.instruction("cmp r8b, 58");                                         // ':' ?
    emitter.instruction("je __rt_strtotime_time_entry_linux_x86_64");           // → time-only (H:MM[:SS])
    emitter.instruction("movzx r8d, BYTE PTR [rbp - 62]");                      // lc16[2]
    emitter.instruction("cmp r8b, 58");                                         // ':' ?
    emitter.instruction("je __rt_strtotime_time_entry_linux_x86_64");           // → time-only (HH:MM[:SS])
    emitter.instruction("cmp rsi, 10");                                         // ISO date needs ≥ 10 chars
    emitter.instruction("jl __rt_strtotime_offsets_entry_linux_x86_64");        // too short for ISO → offsets
    emitter.instruction("movzx r8d, BYTE PTR [rbp - 60]");                      // lc16[4] (offset 4 of date)
    emitter.instruction("cmp r8b, 45");                                         // '-' ?
    emitter.instruction("je __rt_strtotime_iso_entry_linux_x86_64");            // YYYY-MM-DD → ISO
    emitter.instruction("jmp __rt_strtotime_offsets_entry_linux_x86_64");       // default for digit-starting: offsets

    emitter.label("__rt_strtotime_classify_alpha_linux_x86_64");
    // -- check for '+' / '-' signs → offsets entry --
    emitter.instruction("cmp al, 43");                                          // '+' ?
    emitter.instruction("je __rt_strtotime_offsets_entry_linux_x86_64");        // → offsets
    emitter.instruction("cmp al, 45");                                          // '-' ?
    emitter.instruction("je __rt_strtotime_offsets_entry_linux_x86_64");        // → offsets
    emitter.instruction("mov ecx, eax");                                        // refresh range check
    emitter.instruction("sub ecx, 97");                                         // 'a' = 97
    emitter.instruction("cmp ecx, 25");                                         // ASCII alpha ?
    emitter.instruction("ja __rt_strtotime_fail_linux_x86_64");                 // not alpha → fail
    emitter.instruction("cmp al, 97");                                          // possible "a/an <unit>" article-relative form ?
    emitter.instruction("je __rt_strtotime_offsets_entry_linux_x86_64");        // let offsets parse or reject the article form

    // -- alpha: try keyword table match --
    // Args (caller-saved): rdi = candidate ptr, rsi = table base, rcx = available bytes.
    // Returns: rax = consumed bytes (0 = no match), rdx = kind (-1 = no match).
    emitter.instruction("lea rdi, [rbp - 64]");                                 // rdi = candidate prefix (lc16 buffer)
    emitter.instruction("lea rsi, [rip + _strtotime_keyword_tab]");             // rsi = keyword table base
    emitter.instruction("mov rcx, QWORD PTR [rbp - 72]");                       // rcx = trimmed input length
    emitter.instruction("mov r8, 16");                                          // cap candidate window to 16
    emitter.instruction("cmp rcx, r8");                                         // len > 16 ?
    emitter.instruction("cmovae rcx, r8");                                      // rcx = min(len, 16)
    emitter.instruction("call __rt_strtotime_match_word_linux_x86_64");         // rax=consumed, rdx=kind (-1 if no match)

    emitter.instruction("test rax, rax");                                       // no match ?
    emitter.instruction("jz __rt_strtotime_fail_linux_x86_64");                 // yes → fail
    emitter.instruction("cmp rdx, 5");                                          // kind in 0..5 = bare keyword ?
    emitter.instruction("ja __rt_strtotime_alpha_not_keyword_linux_x86_64");    // no → check modifiers/weekdays
    emitter.instruction("cmp rax, QWORD PTR [rbp - 72]");                       // keyword consumed the whole input ?
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // trailing junk after keyword → fail
    emitter.instruction("jmp __rt_strtotime_kw_entry_linux_x86_64");            // yes → keyword strategy

    emitter.label("__rt_strtotime_alpha_not_keyword_linux_x86_64");
    emitter.instruction("cmp rdx, 8");                                          // kind 6..8 = modifier ?
    emitter.instruction("jbe __rt_strtotime_weekdays_entry_linux_x86_64");      // yes → weekdays with modifier
    emitter.instruction("cmp rdx, 10");                                         // kind 9 = bare "ago" → fail
    emitter.instruction("jl __rt_strtotime_fail_linux_x86_64");                 // below 10 → fail
    emitter.instruction("cmp rdx, 16");                                         // weekday name ?
    emitter.instruction("jle __rt_strtotime_alpha_direct_weekday_linux_x86_64"); // yes → direct weekday strategy
    emitter.instruction("cmp rdx, 18");                                         // kind 17..18 = a/an relative magnitude ?
    emitter.instruction("jle __rt_strtotime_offsets_entry_linux_x86_64");       // let the offsets strategy parse the full relative expression
    emitter.instruction("jmp __rt_strtotime_fail_linux_x86_64");                // unknown kind → fail
    emitter.label("__rt_strtotime_alpha_direct_weekday_linux_x86_64");
    emitter.instruction("cmp rax, QWORD PTR [rbp - 72]");                       // weekday consumed the whole input ?
    emitter.instruction("jne __rt_strtotime_fail_linux_x86_64");                // trailing junk after weekday → fail
    emitter.instruction("jmp __rt_strtotime_weekdays_entry_linux_x86_64");      // yes → weekdays
}

fn emit_epilogue_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: shared epilogue ---");
    emitter.label("__rt_strtotime_fail_linux_x86_64");
    emitter.instruction("mov rax, -1");                                         // failure result

    emitter.label("__rt_strtotime_ret_linux_x86_64");
    emitter.instruction("add rsp, 128");                                        // deallocate dispatcher locals
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}
