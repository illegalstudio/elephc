//! Purpose:
//! Emits the `__rt_mktime` runtime helper assembly for mktime.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - The helper normalizes date/time fields through target libc conventions while returning PHP-style timestamps.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits `__rt_mktime`, the runtime helper that converts date/time components into a Unix timestamp.
 ///
 /// ## Input registers (System V ABI)
 /// - `x0` = hour, `x1` = minute, `x2` = second
 /// - `x3` = month (1–12, PHP-style), `x4` = day, `x5` = year (full Gregorian year)
 ///
 /// ## Output
 /// - `x0` = Unix timestamp (seconds since epoch)
 ///
 /// ## Behavior
 /// - x86_64: delegates to `emit_mktime_linux_x86_64` (Linux System V AMD64 ABI)
 /// - ARM64 (Linux): builds a `struct tm` on the stack and calls libc `mktime`
 ///
 /// ## struct tm memory layout (ARM64, 40 bytes at `sp+0`)
 /// `tm_sec(+0)`, `tm_min(+4)`, `tm_hour(+8)`, `tm_mday(+12)`,
 /// `tm_mon(+16)`, `tm_year(+20)`, `tm_wday(+24)`, `tm_yday(+28)`, `tm_isdst(+32)`
 ///
 /// Fields `tm_wday` and `tm_yday` are ignored by libc `mktime`. `tm_isdst = -1` instructs
 /// libc to infer DST automatically.
pub fn emit_mktime(emitter: &mut Emitter) {
    emit_gmmktime(emitter);
    emit_mktime_shifted(emitter);
    if emitter.target.arch == Arch::X86_64 {
        emit_mktime_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mktime ---");
    emitter.label_global("__rt_mktime");

    // -- set up stack frame --
    // Need 44 bytes for struct tm (rounded up to 48 for alignment) + 16 for frame
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set new frame pointer

    // -- build struct tm at sp+0 --
    emitter.instruction("str w2, [sp, #0]");                                    // tm_sec = second
    emitter.instruction("str w1, [sp, #4]");                                    // tm_min = minute
    emitter.instruction("str w0, [sp, #8]");                                    // tm_hour = hour
    emitter.instruction("str w4, [sp, #12]");                                   // tm_mday = day
    emitter.instruction("sub w3, w3, #1");                                      // convert month from 1-based to 0-based
    emitter.instruction("str w3, [sp, #16]");                                   // tm_mon = month - 1
    // -- pre-1900: shift out-of-range years into libc's range (undone after the call) --
    emit_pre1900_shift_prologue(emitter, "__rt_mktime_in_range");

    emitter.instruction("mov w9, #1900");                                       // load 1900 for year adjustment
    emitter.instruction("sub w5, w5, w9");                                      // tm_year = year - 1900
    emitter.instruction("str w5, [sp, #20]");                                   // store tm_year
    emitter.instruction("str wzr, [sp, #24]");                                  // tm_wday = 0 (ignored by mktime)
    emitter.instruction("str wzr, [sp, #28]");                                  // tm_yday = 0 (ignored by mktime)
    emitter.instruction("mov w9, #-1");                                         // tm_isdst = -1 (let mktime determine DST)
    emitter.instruction("str w9, [sp, #32]");                                   // store tm_isdst
    emitter.instruction("bl __rt_tz_init_utc");                                 // default the timezone to UTC on first use (PHP-compatible) unless already set

    // -- call libc mktime --
    emitter.instruction("mov x0, sp");                                          // x0 = pointer to struct tm
    emitter.bl_c("mktime");                                          // mktime(&tm) → x0=time_t

    emit_pre1900_shift_epilogue(emitter);

    // -- tear down stack frame --
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_mktime`.
 ///
 /// ## Input registers (System V AMD64 ABI)
 /// - `edi` = hour, `esi` = minute, `edx` = second
 /// - `ecx` = month (1–12, PHP-style), `r8d` = day, `r9d` = year (full Gregorian year)
 ///
 /// ## Output
 /// - `rax` = Unix timestamp (seconds since epoch)
 ///
 /// ## Behavior
 /// - Pushes a frame pointer and reserves 64 bytes on the stack for a `struct tm`.
 /// - Initializes all fields: `tm_sec`, `tm_min`, `tm_hour`, `tm_mday`, `tm_mon` (converted from
 ///   1-based to 0-based), `tm_year` (converted from full year to years since 1900).
 /// - `tm_wday` and `tm_yday` are set to 0 (ignored by libc `mktime`).
 /// - `tm_isdst = -1` instructs libc to infer DST automatically.
 /// - Calls `mktime(rdi)` where `rdi` points to the on-stack `struct tm`.
 /// - Restores `rsp` and `rbp`, returns the timestamp in `rax`.
fn emit_mktime_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mktime ---");
    emitter.label_global("__rt_mktime");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before materializing the libc struct tm on the stack
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the temporary struct tm storage
    emitter.instruction("sub rsp, 64");                                         // reserve aligned stack space for the leading struct tm fields consumed by libc mktime()

    emitter.instruction("mov DWORD PTR [rsp + 0], edx");                        // tm_sec = second
    emitter.instruction("mov DWORD PTR [rsp + 4], esi");                        // tm_min = minute
    emitter.instruction("mov DWORD PTR [rsp + 8], edi");                        // tm_hour = hour
    emitter.instruction("mov DWORD PTR [rsp + 12], r8d");                       // tm_mday = day
    emitter.instruction("mov eax, ecx");                                        // copy the 1-based month into a scratch register before converting to libc's 0-based tm_mon
    emitter.instruction("sub eax, 1");                                          // convert the month component from PHP's 1-12 range to libc's 0-11 range
    emitter.instruction("mov DWORD PTR [rsp + 16], eax");                       // tm_mon = month - 1
    // -- pre-1900: shift out-of-range years into libc's range (undone after the call) --
    emit_pre1900_shift_prologue(emitter, "__rt_mktime_in_range");

    emitter.instruction("mov eax, r9d");                                        // copy the full Gregorian year into a scratch register before converting to struct tm encoding
    emitter.instruction("sub eax, 1900");                                       // convert the Gregorian year to libc's year-since-1900 encoding
    emitter.instruction("mov DWORD PTR [rsp + 20], eax");                       // tm_year = year - 1900
    emitter.instruction("mov DWORD PTR [rsp + 24], 0");                         // tm_wday = 0 because libc mktime() ignores the incoming weekday field
    emitter.instruction("mov DWORD PTR [rsp + 28], 0");                         // tm_yday = 0 because libc mktime() ignores the incoming yearday field
    emitter.instruction("mov DWORD PTR [rsp + 32], -1");                        // tm_isdst = -1 so libc mktime() infers daylight-saving time automatically
    emitter.instruction("call __rt_tz_init_utc");                               // default the timezone to UTC on first use (PHP-compatible) unless already set

    emitter.instruction("mov rdi, rsp");                                        // pass the temporary struct tm as the first SysV integer argument to libc mktime()
    emitter.instruction("call mktime");                                         // ask libc to convert the PHP date/time components into a Unix timestamp

    emit_pre1900_shift_epilogue(emitter);

    emitter.instruction("add rsp, 64");                                         // release the temporary struct tm storage after libc mktime() returns
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the Unix timestamp
    emitter.instruction("ret");                                                 // return the resulting Unix timestamp in the standard x86_64 integer result register
}

/// Emits `__rt_gmmktime`: the UTC counterpart of `__rt_mktime`, converting date/time
/// components into a Unix timestamp via libc `timegm()` (which interprets the fields as UTC and
/// ignores the TZ environment), so the result is independent of the configured default zone.
pub fn emit_gmmktime(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_gmmktime_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: gmmktime ---");
    emitter.label_global("__rt_gmmktime");

    // -- set up stack frame --
    // Need 44 bytes for struct tm (rounded up to 48 for alignment) + 16 for frame
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set new frame pointer

    // -- build struct tm at sp+0 --
    emitter.instruction("str w2, [sp, #0]");                                    // tm_sec = second
    emitter.instruction("str w1, [sp, #4]");                                    // tm_min = minute
    emitter.instruction("str w0, [sp, #8]");                                    // tm_hour = hour
    emitter.instruction("str w4, [sp, #12]");                                   // tm_mday = day
    emitter.instruction("sub w3, w3, #1");                                      // convert month from 1-based to 0-based
    emitter.instruction("str w3, [sp, #16]");                                   // tm_mon = month - 1
    // -- pre-1900: shift out-of-range years into libc's range (undone after the call) --
    emit_pre1900_shift_prologue(emitter, "__rt_gmmktime_in_range");

    emitter.instruction("mov w9, #1900");                                       // load 1900 for year adjustment
    emitter.instruction("sub w5, w5, w9");                                      // tm_year = year - 1900
    emitter.instruction("str w5, [sp, #20]");                                   // store tm_year
    emitter.instruction("str wzr, [sp, #24]");                                  // tm_wday = 0 (ignored by mktime)
    emitter.instruction("str wzr, [sp, #28]");                                  // tm_yday = 0 (ignored by mktime)
    emitter.instruction("mov w9, #-1");                                         // tm_isdst = -1 (ignored by timegm; UTC has no DST)
    emitter.instruction("str w9, [sp, #32]");                                   // store tm_isdst

    // -- call libc mktime --
    emitter.instruction("mov x0, sp");                                          // x0 = pointer to struct tm
    emitter.bl_c("timegm");                                          // mktime(&tm) → x0=time_t

    emit_pre1900_shift_epilogue(emitter);

    // -- tear down stack frame --
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_mktime`.
 ///
 /// ## Input registers (System V AMD64 ABI)
 /// - `edi` = hour, `esi` = minute, `edx` = second
 /// - `ecx` = month (1–12, PHP-style), `r8d` = day, `r9d` = year (full Gregorian year)
 ///
 /// ## Output
 /// - `rax` = Unix timestamp (seconds since epoch)
 ///
 /// ## Behavior
 /// - Pushes a frame pointer and reserves 64 bytes on the stack for a `struct tm`.
 /// - Initializes all fields: `tm_sec`, `tm_min`, `tm_hour`, `tm_mday`, `tm_mon` (converted from
 ///   1-based to 0-based), `tm_year` (converted from full year to years since 1900).
 /// - `tm_wday` and `tm_yday` are set to 0 (ignored by libc `mktime`).
 /// - `tm_isdst = -1` instructs libc to infer DST automatically.
 /// - Calls `mktime(rdi)` where `rdi` points to the on-stack `struct tm`.
 /// - Restores `rsp` and `rbp`, returns the timestamp in `rax`.

/// Emits the x86_64 Linux variant of `__rt_gmmktime` (UTC `timegm()` instead of `mktime()`).
fn emit_gmmktime_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: gmmktime ---");
    emitter.label_global("__rt_gmmktime");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before materializing the libc struct tm on the stack
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the temporary struct tm storage
    emitter.instruction("sub rsp, 64");                                         // reserve aligned stack space for the leading struct tm fields consumed by libc timegm()

    emitter.instruction("mov DWORD PTR [rsp + 0], edx");                        // tm_sec = second
    emitter.instruction("mov DWORD PTR [rsp + 4], esi");                        // tm_min = minute
    emitter.instruction("mov DWORD PTR [rsp + 8], edi");                        // tm_hour = hour
    emitter.instruction("mov DWORD PTR [rsp + 12], r8d");                       // tm_mday = day
    emitter.instruction("mov eax, ecx");                                        // copy the 1-based month into a scratch register before converting to libc's 0-based tm_mon
    emitter.instruction("sub eax, 1");                                          // convert the month component from PHP's 1-12 range to libc's 0-11 range
    emitter.instruction("mov DWORD PTR [rsp + 16], eax");                       // tm_mon = month - 1
    // -- pre-1900: shift out-of-range years into libc's range (undone after the call) --
    emit_pre1900_shift_prologue(emitter, "__rt_gmmktime_in_range");

    emitter.instruction("mov eax, r9d");                                        // copy the full Gregorian year into a scratch register before converting to struct tm encoding
    emitter.instruction("sub eax, 1900");                                       // convert the Gregorian year to libc's year-since-1900 encoding
    emitter.instruction("mov DWORD PTR [rsp + 20], eax");                       // tm_year = year - 1900
    emitter.instruction("mov DWORD PTR [rsp + 24], 0");                         // tm_wday = 0 because libc timegm() ignores the incoming weekday field
    emitter.instruction("mov DWORD PTR [rsp + 28], 0");                         // tm_yday = 0 because libc timegm() ignores the incoming yearday field
    emitter.instruction("mov DWORD PTR [rsp + 32], -1");                        // tm_isdst = -1; libc timegm() ignores it because UTC has no daylight-saving time

    emitter.instruction("mov rdi, rsp");                                        // pass the temporary struct tm as the first SysV integer argument to libc timegm()
    emitter.instruction("call timegm");                                         // ask libc to convert the PHP date/time components into a UTC Unix timestamp

    emit_pre1900_shift_epilogue(emitter);

    emitter.instruction("add rsp, 64");                                         // release the temporary struct tm storage after libc timegm() returns
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the Unix timestamp
    emitter.instruction("ret");                                                 // return the resulting Unix timestamp in the standard x86_64 integer result register
}

/// Emits the pre-1900 year-shift prologue shared by `__rt_mktime` and `__rt_gmmktime`.
///
/// libc `mktime`/`timegm` reject years before 1900, but the proleptic Gregorian calendar repeats
/// exactly every 400 years (146097 days). For such years this shifts the year forward by whole
/// 400-year cycles into libc's range (the shifted date is identical) and saves the cycle count in a
/// stack slot for `emit_pre1900_shift_epilogue` to undo on the result; `in_range_label` must be
/// unique per call site. On ARM64 the year is in `w5`; on x86_64 it is in `r9d` and the prologue must
/// run after the month field has been stored, since it clobbers `eax`/`ecx`/`edx`.
fn emit_pre1900_shift_prologue(emitter: &mut Emitter, in_range_label: &str) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("cmp r9d, 100");                                    // PHP 2-digit shorthand only applies to years 0-100
        emitter.instruction(&format!("jg {}_remap", in_range_label));           // year > 100 is a literal year
        emitter.instruction("cmp r9d, 0");                                      // negative years are left as-is
        emitter.instruction(&format!("jl {}_remap", in_range_label));           // skip the remap for negative years
        emitter.instruction("lea r11d, [r9d + 2000]");                          // candidate year for the 0-69 range
        emitter.instruction("lea ecx, [r9d + 1900]");                           // candidate year for the 70-100 range
        emitter.instruction("cmp r9d, 69");                                     // 0-69 map to 2000-2069, 70-100 map to 1970-2000
        emitter.instruction("mov r9d, r11d");                                   // default to the 2000s mapping
        emitter.instruction("cmovg r9d, ecx");                                  // use the 1900s mapping when year > 69
        emitter.label(&format!("{}_remap", in_range_label));
        emitter.instruction("xor r10d, r10d");                                  // cycle count = 0 (no shift needed for years already in libc range)
        emitter.instruction("cmp r9d, 1900");                                   // is the year before 1900 (outside libc mktime/timegm range)?
        emitter.instruction(&format!("jge {}", in_range_label));                // years >= 1900 convert directly through libc
        emitter.instruction("cmp r9d, 100");                                    // years 0-100 are PHP 2-digit shorthand (left to libc, not remapped here)
        emitter.instruction(&format!("jle {}", in_range_label));                // skip the cycle shift for those
        emitter.instruction("mov r11d, 1900");                                  // load 1900
        emitter.instruction("sub r11d, r9d");                                   // 1900 - year
        emitter.instruction("add r11d, 399");                                   // round the 400-year cycle count up
        emitter.instruction("mov eax, r11d");                                   // move the dividend into eax for idiv
        emitter.instruction("cdq");                                             // sign-extend eax into edx:eax
        emitter.instruction("mov ecx, 400");                                    // load the 400-year Gregorian cycle length
        emitter.instruction("idiv ecx");                                        // eax = (1900 - year + 399) / 400
        emitter.instruction("mov r10d, eax");                                   // blocks = quotient (also zero-extends into r10)
        emitter.instruction("imul eax, eax, 400");                              // blocks * 400 years
        emitter.instruction("add r9d, eax");                                    // shift the year forward into libc range (Gregorian repeats every 400y)
        emitter.label(in_range_label);
        emitter.instruction("mov QWORD PTR [rsp + 56], r10");                   // save the cycle count across the libc call
    } else {
        emitter.instruction("cmp w5, #100");                                    // PHP 2-digit shorthand only applies to years 0-100
        emitter.instruction(&format!("b.gt {}_remap", in_range_label));         // year > 100 is a literal year
        emitter.instruction("cmp w5, #0");                                      // negative years are left as-is
        emitter.instruction(&format!("b.lt {}_remap", in_range_label));         // skip the remap for negative years
        emitter.instruction("cmp w5, #69");                                     // 0-69 map to 2000-2069, 70-100 map to 1970-2000
        emitter.instruction("add w13, w5, #2000");                              // candidate year for the 0-69 range
        emitter.instruction("add w14, w5, #1900");                              // candidate year for the 70-100 range
        emitter.instruction("csel w5, w13, w14, le");                           // select the 2000s mapping when year <= 69
        emitter.label(&format!("{}_remap", in_range_label));
        emitter.instruction("mov w12, #0");                                     // cycle count = 0 (no shift needed for years already in libc range)
        emitter.instruction("cmp w5, #1900");                                   // is the year before 1900 (outside libc mktime/timegm range)?
        emitter.instruction(&format!("b.ge {}", in_range_label));               // years >= 1900 convert directly through libc
        emitter.instruction("cmp w5, #100");                                    // years 0-100 are PHP 2-digit shorthand (left to libc, not remapped here)
        emitter.instruction(&format!("b.le {}", in_range_label));               // skip the cycle shift for those
        emitter.instruction("mov w13, #1900");                                  // load 1900
        emitter.instruction("sub w13, w13, w5");                                // 1900 - year
        emitter.instruction("add w13, w13, #399");                              // round the 400-year cycle count up
        emitter.instruction("mov w14, #400");                                   // load the 400-year Gregorian cycle length
        emitter.instruction("udiv w12, w13, w14");                              // blocks = (1900 - year + 399) / 400
        emitter.instruction("mul w14, w12, w14");                               // blocks * 400 years
        emitter.instruction("add w5, w5, w14");                                 // shift the year forward into libc range (Gregorian repeats every 400y)
        emitter.label(in_range_label);
        emitter.instruction("str x12, [sp, #56]");                              // save the cycle count across the libc call
    }
}

/// Emits the pre-1900 year-shift epilogue: subtracts the saved cycle count times 146097 * 86400
/// seconds from the libc result to recover the real timestamp. A zero cycle count leaves it
/// unchanged, so years already in libc's range are not affected.
fn emit_pre1900_shift_epilogue(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov r10, QWORD PTR [rsp + 56]");                   // reload the saved 400-year cycle count
        emitter.instruction("movabs r11, 12622780800");                         // 146097 * 86400 seconds per 400-year cycle
        emitter.instruction("imul r10, r11");                                   // blocks * 12622780800 seconds
        emitter.instruction("sub rax, r10");                                    // subtract the shifted cycles to recover the real timestamp
    } else {
        emitter.instruction("ldr x12, [sp, #56]");                              // reload the saved 400-year cycle count
        emitter.instruction("movz x13, #0x3ab1");                               // load 146097 (days per 400-year cycle), low 16 bits
        emitter.instruction("movk x13, #0x2, lsl #16");                         // load 146097, high bits
        emitter.instruction("mul x12, x12, x13");                               // blocks * 146097 days
        emitter.instruction("movz x13, #0x5180");                               // load 86400 (seconds per day), low 16 bits
        emitter.instruction("movk x13, #0x1, lsl #16");                         // load 86400, high bits
        emitter.instruction("mul x12, x12, x13");                               // blocks * 146097 * 86400 seconds
        emitter.instruction("sub x0, x0, x12");                                 // subtract the shifted cycles to recover the real timestamp
    }
}

/// Emits `__rt_mktime_shifted`, a wrapper used by the strtotime parsers in place of libc `mktime`.
///
/// It has the same ABI as `mktime` (struct tm pointer in the first integer argument, timestamp in
/// the result register), but first applies the 400-year Gregorian-cycle shift for years 101-1899
/// (which libc rejects): it reads `tm_year`, shifts it forward by whole cycles, calls libc `mktime`,
/// restores the original `tm_year` (so re-entrant callers like `first/last day of` re-shift), and
/// subtracts the shifted cycles from the result. Years in libc's range pass through unchanged.
pub fn emit_mktime_shifted(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mktime (pre-1900 cycle-shift wrapper) ---");
    if emitter.target.arch == Arch::X86_64 {
        emitter.label_global("__rt_mktime_shifted");
        emitter.instruction("push rbp");                                        // preserve the caller frame pointer
        emitter.instruction("mov rbp, rsp");                                    // establish the frame base
        emitter.instruction("sub rsp, 32");                                     // reserve space for the saved pointer/year/cycle count
        emitter.instruction("mov QWORD PTR [rsp + 0], rdi");                    // save the struct tm pointer
        emitter.instruction("mov eax, DWORD PTR [rdi + 20]");                   // eax = tm_year (years since 1900)
        emitter.instruction("xor r10d, r10d");                                  // cycle count = 0
        emitter.instruction("test eax, eax");                                   // tm_year >= 0 means year >= 1900 (in libc range)
        emitter.instruction("jns __rt_mkts_skip_x86");                          // no shift needed for years >= 1900
        emitter.instruction("cmp eax, -1800");                                  // compare tm_year with -1800 (year 100)
        emitter.instruction("jle __rt_mkts_skip_x86");                          // years 0-100 are 2-digit shorthand; leave them to libc
        emitter.instruction("mov r11d, 399");                                   // load 399 to round the cycle count up
        emitter.instruction("sub r11d, eax");                                   // 399 - tm_year
        emitter.instruction("mov eax, r11d");                                   // move the dividend into eax for idiv
        emitter.instruction("cdq");                                             // sign-extend eax into edx:eax
        emitter.instruction("mov ecx, 400");                                    // load the 400-year Gregorian cycle length
        emitter.instruction("idiv ecx");                                        // eax = (399 - tm_year) / 400
        emitter.instruction("mov r10d, eax");                                   // blocks = quotient (zero-extends into r10)
        emitter.instruction("imul eax, eax, 400");                              // blocks * 400 years
        emitter.instruction("mov edx, DWORD PTR [rdi + 20]");                   // reload the original tm_year
        emitter.instruction("add edx, eax");                                    // tm_year + blocks * 400
        emitter.instruction("mov DWORD PTR [rdi + 20], edx");                   // write the shifted tm_year back into the struct tm
        emitter.label("__rt_mkts_skip_x86");
        emitter.instruction("mov QWORD PTR [rsp + 16], r10");                   // save the cycle count across the libc call
        emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                    // rdi = struct tm pointer
        emitter.bl_c("mktime");                                                 // call libc mktime on the (possibly shifted) struct tm
        emitter.instruction("mov rcx, QWORD PTR [rsp + 0]");                    // reload the struct tm pointer
        emitter.instruction("mov edx, DWORD PTR [rcx + 20]");                   // mktime normalized tm_year for the shifted year
        emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                   // reload the cycle count
        emitter.instruction("mov r8d, r10d");                                   // copy blocks for the year math (eax would alias the rax result)
        emitter.instruction("imul r8d, r8d, 400");                              // blocks * 400 years
        emitter.instruction("sub edx, r8d");                                    // un-shift the normalized tm_year back to the real year
        emitter.instruction("mov DWORD PTR [rcx + 20], edx");                   // write the corrected tm_year for re-entrant callers
        emitter.instruction("movabs r11, 12622780800");                         // 146097 * 86400 seconds per 400-year cycle
        emitter.instruction("imul r10, r11");                                   // blocks * 12622780800 seconds
        emitter.instruction("sub rax, r10");                                    // subtract the shifted cycles to recover the real timestamp
        emitter.instruction("add rsp, 32");                                     // release the frame
        emitter.instruction("pop rbp");                                         // restore the caller frame pointer
        emitter.instruction("ret");                                             // return the corrected timestamp
    } else {
        emitter.label_global("__rt_mktime_shifted");
        emitter.instruction("sub sp, sp, #32");                                 // allocate a small frame for the saved pointer/year/cycle count
        emitter.instruction("stp x29, x30, [sp, #16]");                         // save frame pointer and return address
        emitter.instruction("add x29, sp, #16");                                // set new frame pointer
        emitter.instruction("str x0, [sp, #0]");                                // save the struct tm pointer
        emitter.instruction("ldr w1, [x0, #20]");                               // w1 = tm_year (years since 1900)
        emitter.instruction("mov w12, #0");                                     // cycle count = 0
        emitter.instruction("cmp w1, #0");                                      // tm_year >= 0 means year >= 1900 (in libc range)
        emitter.instruction("b.ge __rt_mkts_skip");                             // no shift needed for years >= 1900
        emitter.instruction("cmn w1, #1800");                                   // compare tm_year with -1800 (year 100)
        emitter.instruction("b.le __rt_mkts_skip");                             // years 0-100 are 2-digit shorthand; leave them to libc
        emitter.instruction("mov w13, #399");                                   // load 399 to round the cycle count up
        emitter.instruction("sub w13, w13, w1");                                // 399 - tm_year
        emitter.instruction("mov w14, #400");                                   // load the 400-year Gregorian cycle length
        emitter.instruction("udiv w12, w13, w14");                              // blocks = (399 - tm_year) / 400
        emitter.instruction("mul w14, w12, w14");                               // blocks * 400 years
        emitter.instruction("add w1, w1, w14");                                 // shift tm_year forward into libc range
        emitter.instruction("str w1, [x0, #20]");                               // write the shifted tm_year back into the struct tm
        emitter.label("__rt_mkts_skip");
        emitter.instruction("str x12, [sp, #8]");                               // save the cycle count across the libc call
        emitter.instruction("ldr x0, [sp, #0]");                                // x0 = struct tm pointer
        emitter.bl_c("mktime");                                                 // call libc mktime on the (possibly shifted) struct tm
        emitter.instruction("ldr x9, [sp, #0]");                                // reload the struct tm pointer
        emitter.instruction("ldr w10, [x9, #20]");                              // mktime normalized tm_year for the shifted year
        emitter.instruction("ldr x12, [sp, #8]");                               // reload the cycle count
        emitter.instruction("mov w13, #400");                                   // load the 400-year shift amount
        emitter.instruction("mul w14, w12, w13");                               // blocks * 400 years
        emitter.instruction("sub w10, w10, w14");                               // un-shift the normalized tm_year back to the real year
        emitter.instruction("str w10, [x9, #20]");                              // write the corrected tm_year for re-entrant callers
        emitter.instruction("movz x13, #0x3ab1");                               // load 146097 (days per 400-year cycle), low 16 bits
        emitter.instruction("movk x13, #0x2, lsl #16");                         // load 146097, high bits
        emitter.instruction("mul x12, x12, x13");                               // blocks * 146097 days
        emitter.instruction("movz x13, #0x5180");                               // load 86400 (seconds per day), low 16 bits
        emitter.instruction("movk x13, #0x1, lsl #16");                         // load 86400, high bits
        emitter.instruction("mul x12, x12, x13");                               // blocks * 146097 * 86400 seconds
        emitter.instruction("sub x0, x0, x12");                                 // subtract the shifted cycles to recover the real timestamp
        emitter.instruction("ldp x29, x30, [sp, #16]");                         // restore frame pointer and return address
        emitter.instruction("add sp, sp, #32");                                 // release the frame
        emitter.instruction("ret");                                             // return the corrected timestamp
    }
}
