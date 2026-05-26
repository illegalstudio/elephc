//! Purpose:
//! Emits the keyword strategy sub-routines for the `__rt_strtotime` dispatcher.
//! Handles the bare relative keywords: `now`, `today`, `tomorrow`, `yesterday`, `midnight`, `noon`.
//!
//! Called from:
//! - `crate::codegen::runtime::system::strtotime::mod::emit_strtotime()` for the keyword-dispatch arm.
//!
//! Key details:
//! - Strategy bodies execute inside the dispatcher's frame and end with a branch to `__rt_strtotime_ret` / `__rt_strtotime_ret_linux_x86_64`.
//! - The shared `__rt_strtotime_today_tm` helper materializes today's localtime fields at `[sp+0..36]` with sec/min/hour zeroed and `tm_isdst=-1`; callers then mutate before calling libc `mktime`.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Dispatches to the target-specific keyword emitter based on `emitter.target.arch`.
/// `x9` (ARM64) or `rdx` (x86_64) holds the keyword kind (0=now, 1=today, 2=tomorrow,
/// 3=yesterday, 4=midnight, 5=noon) as returned by `match_word`.
pub(crate) fn emit_keywords(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_keywords_linux_x86_64(emitter);
        return;
    }

    emit_keywords_arm64(emitter);
}

/// Emits ARM64 keyword strategy sub-routines: `__rt_strtotime_kw_entry` dispatcher,
/// per-keyword bodies (`kw_now`, `kw_today`, `kw_tomorrow`, `kw_yesterday`, `kw_noon`),
/// plus the `__rt_strtotime_today_tm` and `__rt_strtotime_now_tm` helpers.
/// Uses `__rt_strtotime_ret` as the shared return point. Expects `x9` to hold the
/// keyword kind on entry. The today_tm helper allocates a 16-byte sub-frame and saves
/// the link register; the struct tm fields land in the caller's `[sp+0..36]` which
/// maps to `[sp+16..52]` while the sub-frame is live.
fn emit_keywords_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: keyword strategy dispatcher ---");
    emitter.label("__rt_strtotime_kw_entry");
    // x9 = kind (0..5: now, today, tomorrow, yesterday, midnight, noon)
    emitter.instruction("cmp x9, #0");                                          // now ?
    emitter.instruction("b.eq __rt_strtotime_kw_now");                          // → now strategy
    emitter.instruction("cmp x9, #2");                                          // tomorrow ?
    emitter.instruction("b.eq __rt_strtotime_kw_tomorrow");                     // → tomorrow strategy
    emitter.instruction("cmp x9, #3");                                          // yesterday ?
    emitter.instruction("b.eq __rt_strtotime_kw_yesterday");                    // → yesterday strategy
    emitter.instruction("cmp x9, #5");                                          // noon ?
    emitter.instruction("b.eq __rt_strtotime_kw_noon");                         // → noon strategy
    emitter.instruction("b __rt_strtotime_kw_today");                           // default: today/midnight (kinds 1,4)

    emitter.blank();
    emitter.comment("--- strtotime: kw_now ---");
    emitter.label("__rt_strtotime_kw_now");
    emitter.instruction("bl __rt_time");                                        // x0 = current Unix timestamp
    emitter.instruction("b __rt_strtotime_ret");                                // return through shared epilogue

    emitter.blank();
    emitter.comment("--- strtotime: kw_today / kw_midnight ---");
    emitter.label("__rt_strtotime_kw_today");
    emitter.instruction("bl __rt_strtotime_today_tm");                          // populate [sp+0..36] with today midnight tm
    emitter.instruction("mov x0, sp");                                          // x0 = &tm for libc mktime
    emitter.bl_c("mktime");                                                     // → x0 = Unix timestamp
    emitter.instruction("b __rt_strtotime_ret");                                // return

    emitter.blank();
    emitter.comment("--- strtotime: kw_tomorrow ---");
    emitter.label("__rt_strtotime_kw_tomorrow");
    emitter.instruction("bl __rt_strtotime_today_tm");                          // populate today midnight tm
    emitter.instruction("ldr w9, [sp, #12]");                                   // load tm_mday
    emitter.instruction("add w9, w9, #1");                                      // tm_mday + 1 (mktime normalizes overflow)
    emitter.instruction("str w9, [sp, #12]");                                   // store updated tm_mday
    emitter.instruction("mov x0, sp");                                          // x0 = &tm
    emitter.bl_c("mktime");                                                     // → x0 = ts
    emitter.instruction("b __rt_strtotime_ret");                                // return

    emitter.blank();
    emitter.comment("--- strtotime: kw_yesterday ---");
    emitter.label("__rt_strtotime_kw_yesterday");
    emitter.instruction("bl __rt_strtotime_today_tm");                          // populate today midnight tm
    emitter.instruction("ldr w9, [sp, #12]");                                   // load tm_mday
    emitter.instruction("sub w9, w9, #1");                                      // tm_mday - 1 (mktime normalizes underflow)
    emitter.instruction("str w9, [sp, #12]");                                   // store updated tm_mday
    emitter.instruction("mov x0, sp");                                          // x0 = &tm
    emitter.bl_c("mktime");                                                     // → x0 = ts
    emitter.instruction("b __rt_strtotime_ret");                                // return

    emitter.blank();
    emitter.comment("--- strtotime: kw_noon ---");
    emitter.label("__rt_strtotime_kw_noon");
    emitter.instruction("bl __rt_strtotime_today_tm");                          // populate today midnight tm
    emitter.instruction("mov w9, #12");                                         // hour = 12
    emitter.instruction("str w9, [sp, #8]");                                    // tm_hour = 12
    emitter.instruction("mov x0, sp");                                          // x0 = &tm
    emitter.bl_c("mktime");                                                     // → x0 = ts
    emitter.instruction("b __rt_strtotime_ret");                                // return

    emit_today_tm_arm64(emitter);
    emit_now_tm_arm64(emitter);
}

/// Emits ARM64 `__rt_strtotime_today_tm`: a non-leaf helper that materializes today's
/// localtime fields at `[sp+0..36]` with `tm_sec`, `tm_min`, `tm_hour` zeroed, and
/// `tm_isdst=-1`. Allocates a 16-byte sub-frame (saves link register at `[sp+0]`);
/// while the sub-frame is active the caller's tm is accessible at `[sp+16..52]`.
/// Calls `__rt_time` and `localtime`; restores link register and deallocates sub-frame
/// before returning to the caller (strategy body) via `ret`.
fn emit_today_tm_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime helper: populate [sp+0..36] with today @ midnight struct tm ---");
    emitter.label("__rt_strtotime_today_tm");
    // Non-leaf helper: allocates a 16-byte sub-frame to save link register + ts.
    // Caller's `[sp+0..36]` is accessed as `[sp+16..52]` while the sub-frame is in place.
    emitter.instruction("sub sp, sp, #16");                                     // allocate today_tm sub-frame
    emitter.instruction("str x30, [sp, #0]");                                   // save link register before nested calls
    emitter.instruction("bl __rt_time");                                        // x0 = current Unix timestamp
    emitter.instruction("str x0, [sp, #8]");                                    // save ts into local slot
    emitter.instruction("add x0, sp, #8");                                      // x0 = &ts (libc localtime takes time_t*)
    emitter.bl_c("localtime");                                                  // x0 = static struct tm*
    // -- copy 36 bytes (9 ints) from libc tm into dispatcher tm scratch (caller's [sp+0..36] = current [sp+16..52]) --
    emitter.instruction("ldp x9, x10, [x0, #0]");                               // load tm_sec/tm_min/tm_hour/tm_mday
    emitter.instruction("ldp x11, x12, [x0, #16]");                             // load tm_mon/tm_year/tm_wday/tm_yday
    emitter.instruction("ldr w13, [x0, #32]");                                  // load tm_isdst
    emitter.instruction("stp x9, x10, [sp, #16]");                              // store first 16 bytes of caller's tm
    emitter.instruction("stp x11, x12, [sp, #32]");                             // store next 16 bytes of caller's tm
    emitter.instruction("str w13, [sp, #48]");                                  // store tm_isdst
    // -- zero out tm_sec/tm_min/tm_hour for midnight --
    emitter.instruction("str wzr, [sp, #16]");                                  // tm_sec = 0
    emitter.instruction("str wzr, [sp, #20]");                                  // tm_min = 0
    emitter.instruction("str wzr, [sp, #24]");                                  // tm_hour = 0
    emitter.instruction("mov w9, #-1");                                         // tm_isdst sentinel for mktime
    emitter.instruction("str w9, [sp, #48]");                                   // tm_isdst = -1 (let mktime infer DST)
    emitter.instruction("ldr x30, [sp, #0]");                                   // restore link register
    emitter.instruction("add sp, sp, #16");                                     // deallocate sub-frame
    emitter.instruction("ret");                                                 // return to caller (strategy body)
}

/// Emits ARM64 `__rt_strtotime_now_tm`: a non-leaf helper identical to `emit_today_tm_arm64`
/// except that `tm_sec`, `tm_min`, `tm_hour` are preserved (not zeroed). Used by the
/// offsets strategy where the base time is "now", not "today midnight". Same sub-frame
/// protocol: 16-byte allocation, link register saved at `[sp+0]`, tm accessible at
/// `[sp+16..52]` while sub-frame is live. Returns via `ret`.
fn emit_now_tm_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime helper: populate [sp+0..36] with NOW (preserve current h/m/s) ---");
    emitter.label("__rt_strtotime_now_tm");
    // Same as today_tm but does NOT zero h/m/s — used by the offsets strategy where
    // the base time is "now", not "today midnight".
    emitter.instruction("sub sp, sp, #16");                                     // allocate now_tm sub-frame
    emitter.instruction("str x30, [sp, #0]");                                   // save link register before nested calls
    emitter.instruction("bl __rt_time");                                        // x0 = current Unix timestamp
    emitter.instruction("str x0, [sp, #8]");                                    // save ts into local slot
    emitter.instruction("add x0, sp, #8");                                      // x0 = &ts (libc localtime takes time_t*)
    emitter.bl_c("localtime");                                                  // x0 = static struct tm*
    emitter.instruction("ldp x9, x10, [x0, #0]");                               // load tm_sec/tm_min/tm_hour/tm_mday
    emitter.instruction("ldp x11, x12, [x0, #16]");                             // load tm_mon/tm_year/tm_wday/tm_yday
    emitter.instruction("ldr w13, [x0, #32]");                                  // load tm_isdst
    emitter.instruction("stp x9, x10, [sp, #16]");                              // store first 16 bytes of caller's tm
    emitter.instruction("stp x11, x12, [sp, #32]");                             // store next 16 bytes of caller's tm
    emitter.instruction("str w13, [sp, #48]");                                  // store tm_isdst
    emitter.instruction("mov w9, #-1");                                         // tm_isdst sentinel for mktime
    emitter.instruction("str w9, [sp, #48]");                                   // tm_isdst = -1 (let mktime infer DST)
    emitter.instruction("ldr x30, [sp, #0]");                                   // restore link register
    emitter.instruction("add sp, sp, #16");                                     // deallocate sub-frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits x86_64 Linux keyword strategy sub-routines: `__rt_strtotime_kw_entry_linux_x86_64`
/// dispatcher, per-keyword bodies (`kw_now`, `kw_today`, `kw_tomorrow`, `kw_yesterday`,
/// `kw_noon` with `_linux_x86_64` suffix), plus the `__rt_strtotime_today_tm_linux_x86_64`
/// and `__rt_strtotime_now_tm_linux_x86_64` helpers. Uses `__rt_strtotime_ret_linux_x86_64`
/// as the shared return point. Expects `rdx` to hold the keyword kind on entry.
/// The today_tm helper allocates an 8-byte sub-frame and preserves `rbp`-relative
/// addressing so the caller's struct tm at `[rbp-128..rbp-92]` remains accessible.
fn emit_keywords_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime: keyword strategy dispatcher ---");
    emitter.label("__rt_strtotime_kw_entry_linux_x86_64");
    // rdx = kind (0..5) returned by match_word
    emitter.instruction("cmp rdx, 0");                                          // now ?
    emitter.instruction("je __rt_strtotime_kw_now_linux_x86_64");               // → now strategy
    emitter.instruction("cmp rdx, 2");                                          // tomorrow ?
    emitter.instruction("je __rt_strtotime_kw_tomorrow_linux_x86_64");          // → tomorrow strategy
    emitter.instruction("cmp rdx, 3");                                          // yesterday ?
    emitter.instruction("je __rt_strtotime_kw_yesterday_linux_x86_64");         // → yesterday strategy
    emitter.instruction("cmp rdx, 5");                                          // noon ?
    emitter.instruction("je __rt_strtotime_kw_noon_linux_x86_64");              // → noon strategy
    emitter.instruction("jmp __rt_strtotime_kw_today_linux_x86_64");            // default: today/midnight (1,4)

    emitter.blank();
    emitter.comment("--- strtotime: kw_now ---");
    emitter.label("__rt_strtotime_kw_now_linux_x86_64");
    emitter.instruction("call __rt_time");                                      // rax = current Unix timestamp
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return through shared epilogue

    emitter.blank();
    emitter.comment("--- strtotime: kw_today / kw_midnight ---");
    emitter.label("__rt_strtotime_kw_today_linux_x86_64");
    emitter.instruction("call __rt_strtotime_today_tm_linux_x86_64");           // populate [rsp+0..36] with today midnight
    emitter.instruction("mov rdi, rsp");                                        // rdi = &tm for libc mktime
    emitter.instruction("call mktime");                                         // → rax = ts
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return

    emitter.blank();
    emitter.comment("--- strtotime: kw_tomorrow ---");
    emitter.label("__rt_strtotime_kw_tomorrow_linux_x86_64");
    emitter.instruction("call __rt_strtotime_today_tm_linux_x86_64");           // populate today midnight tm
    emitter.instruction("mov eax, DWORD PTR [rsp + 12]");                       // load tm_mday
    emitter.instruction("inc eax");                                             // tm_mday + 1
    emitter.instruction("mov DWORD PTR [rsp + 12], eax");                       // store updated tm_mday
    emitter.instruction("mov rdi, rsp");                                        // rdi = &tm
    emitter.instruction("call mktime");                                         // → rax = ts
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return

    emitter.blank();
    emitter.comment("--- strtotime: kw_yesterday ---");
    emitter.label("__rt_strtotime_kw_yesterday_linux_x86_64");
    emitter.instruction("call __rt_strtotime_today_tm_linux_x86_64");           // populate today midnight tm
    emitter.instruction("mov eax, DWORD PTR [rsp + 12]");                       // load tm_mday
    emitter.instruction("dec eax");                                             // tm_mday - 1
    emitter.instruction("mov DWORD PTR [rsp + 12], eax");                       // store updated tm_mday
    emitter.instruction("mov rdi, rsp");                                        // rdi = &tm
    emitter.instruction("call mktime");                                         // → rax = ts
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return

    emitter.blank();
    emitter.comment("--- strtotime: kw_noon ---");
    emitter.label("__rt_strtotime_kw_noon_linux_x86_64");
    emitter.instruction("call __rt_strtotime_today_tm_linux_x86_64");           // populate today midnight tm
    emitter.instruction("mov DWORD PTR [rsp + 8], 12");                         // tm_hour = 12
    emitter.instruction("mov rdi, rsp");                                        // rdi = &tm
    emitter.instruction("call mktime");                                         // → rax = ts
    emitter.instruction("jmp __rt_strtotime_ret_linux_x86_64");                 // return

    emit_today_tm_linux_x86_64(emitter);
    emit_now_tm_linux_x86_64(emitter);
}

/// Emits x86_64 Linux `__rt_strtotime_today_tm_linux_x86_64`: a non-leaf helper that
/// materializes today's localtime fields into the caller's struct tm at `[rbp-128..rbp-92]`
/// with `tm_sec`, `tm_min`, `tm_hour` zeroed and `tm_isdst=-1`. Allocates an 8-byte
/// sub-frame via `sub rsp, 8` (keeps rsp 16-aligned for nested libc calls); does NOT
/// touch `rbp` so the caller's scratch remains accessible via `rbp`-relative addressing.
/// Calls `__rt_time` and `localtime`; releases sub-frame via `add rsp, 8` before returning.
fn emit_today_tm_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime helper: populate dispatcher tm with today @ midnight ---");
    emitter.label("__rt_strtotime_today_tm_linux_x86_64");
    // Reached via `call` from a strategy body. Establishes its own sub-frame so it doesn't borrow caller scratch slots.
    // We push 1 callee-saved (rbx is unused here) — instead use plain `sub rsp` to keep rsp 16-aligned for nested libc calls.
    // Caller's `[rbp - 128]` (struct tm tm_sec) stays accessible by absolute rbp-relative addressing because this helper does NOT touch rbp.
    emitter.instruction("sub rsp, 8");                                          // reserve 8 bytes for ts; entry rsp ≡ 8 mod 16, after sub rsp ≡ 0 mod 16 (aligned for libc call)
    emitter.instruction("call __rt_time");                                      // rax = current Unix timestamp
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // save ts at top of sub-frame
    emitter.instruction("mov rdi, rsp");                                        // rdi = &ts for libc localtime
    emitter.instruction("call localtime");                                      // rax = static struct tm*
    // -- copy 36 bytes from libc tm into dispatcher tm scratch (caller's struct tm at [rbp - 128..rbp - 92]) --
    emitter.instruction("mov rcx, QWORD PTR [rax + 0]");                        // load tm_sec/tm_min/tm_hour/tm_mday (8 bytes)
    emitter.instruction("mov QWORD PTR [rbp - 128], rcx");                      // store first 8 bytes of tm
    emitter.instruction("mov rcx, QWORD PTR [rax + 8]");                        // load 8 more bytes
    emitter.instruction("mov QWORD PTR [rbp - 120], rcx");                      // store next 8 bytes of tm
    emitter.instruction("mov rcx, QWORD PTR [rax + 16]");                       // load tm_mon/tm_year (8 bytes)
    emitter.instruction("mov QWORD PTR [rbp - 112], rcx");                      // store bytes 16..24
    emitter.instruction("mov rcx, QWORD PTR [rax + 24]");                       // load tm_wday/tm_yday (8 bytes)
    emitter.instruction("mov QWORD PTR [rbp - 104], rcx");                      // store bytes 24..32
    emitter.instruction("mov ecx, DWORD PTR [rax + 32]");                       // load tm_isdst
    emitter.instruction("mov DWORD PTR [rbp - 96], ecx");                       // store tm_isdst
    // -- zero tm_sec/tm_min/tm_hour for midnight --
    emitter.instruction("mov DWORD PTR [rbp - 128], 0");                        // tm_sec = 0
    emitter.instruction("mov DWORD PTR [rbp - 124], 0");                        // tm_min = 0
    emitter.instruction("mov DWORD PTR [rbp - 120], 0");                        // tm_hour = 0
    emitter.instruction("mov DWORD PTR [rbp - 96], -1");                        // tm_isdst = -1 (mktime infers DST)
    emitter.instruction("add rsp, 8");                                          // release sub-frame
    emitter.instruction("ret");                                                 // return to strategy body
}

/// Emits x86_64 Linux `__rt_strtotime_now_tm_linux_x86_64`: a non-leaf helper identical
/// to `emit_today_tm_linux_x86_64` except that `tm_sec`, `tm_min`, `tm_hour` are preserved
/// (not zeroed). Used by the offsets strategy where the base time is "now", not "today
/// midnight". Same sub-frame and `rbp`-relative addressing protocol; releases sub-frame
/// via `add rsp, 8` before returning.
fn emit_now_tm_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- strtotime helper: populate dispatcher tm with NOW (preserve h/m/s) ---");
    emitter.label("__rt_strtotime_now_tm_linux_x86_64");
    // Same as today_tm_linux_x86_64 but does NOT zero h/m/s — used by the offsets strategy.
    emitter.instruction("sub rsp, 8");                                          // reserve 8 bytes for ts (aligns rsp to 16 for libc)
    emitter.instruction("call __rt_time");                                      // rax = current Unix timestamp
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // save ts at top of sub-frame
    emitter.instruction("mov rdi, rsp");                                        // rdi = &ts for libc localtime
    emitter.instruction("call localtime");                                      // rax = static struct tm*
    emitter.instruction("mov rcx, QWORD PTR [rax + 0]");                        // load tm_sec/tm_min/tm_hour/tm_mday
    emitter.instruction("mov QWORD PTR [rbp - 128], rcx");                      // store first 8 bytes of tm
    emitter.instruction("mov rcx, QWORD PTR [rax + 8]");                        // load 8 more bytes
    emitter.instruction("mov QWORD PTR [rbp - 120], rcx");                      // store next 8 bytes of tm
    emitter.instruction("mov rcx, QWORD PTR [rax + 16]");                       // load tm_mon/tm_year (8 bytes)
    emitter.instruction("mov QWORD PTR [rbp - 112], rcx");                      // store bytes 16..24
    emitter.instruction("mov rcx, QWORD PTR [rax + 24]");                       // load tm_wday/tm_yday (8 bytes)
    emitter.instruction("mov QWORD PTR [rbp - 104], rcx");                      // store bytes 24..32
    emitter.instruction("mov ecx, DWORD PTR [rax + 32]");                       // load tm_isdst
    emitter.instruction("mov DWORD PTR [rbp - 96], ecx");                       // store tm_isdst
    emitter.instruction("mov DWORD PTR [rbp - 96], -1");                        // tm_isdst = -1 (mktime infers DST)
    emitter.instruction("add rsp, 8");                                          // release sub-frame
    emitter.instruction("ret");                                                 // return to strategy body
}
