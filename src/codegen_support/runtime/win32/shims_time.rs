//! Win32 shims for the time/env family: clock_gettime, getenv/putenv/tzset,
//! time/localtime/gmtime/mktime/gettimeofday, getrusage, clock_getres.
//!
//! Key details:
//! - `emit_shim_localtime`/`emit_shim_mktime` additionally resolve the active
//!   default timezone's UTC offset through the elephc-tz bridge (WF9): when a
//!   date/mktime/strtotime call site has published `elephc_tz_offset` into the
//!   `_elephc_tz_offset_fn` slot (`crate::codegen_support::tz_bridge`), these
//!   shims use it to compute a real IANA-correct offset instead of relying on
//!   msvcrt's TZ-only, no-zoneinfo `localtime`/`mktime`. See each function's
//!   docblock for the exact fallback contract when the slot is null or the zone
//!   is unresolvable.

use crate::codegen::emit::Emitter;

/// The `i64::MIN` "no data" sentinel `elephc_tz_offset` returns for an unknown or
/// false zone (see `crates/elephc-tz/src/abi.rs`), as a GAS hex literal: no real
/// packed `(offset, isdst)` value can equal it, since real UTC offsets are many
/// orders of magnitude smaller than `i64::MIN / 2`.
const TZ_OFFSET_SENTINEL_HEX: &str = "0x8000000000000000";

/// Emits the Win32 implementation of `clock_gettime(clock_id, timespec*)`.
///
/// SysV input is `rdi = clock_id`, `rsi = timespec*`. `CLOCK_MONOTONIC` (the
/// Linux-compatible id `1` emitted by the shared x86_64 runtime) is backed by
/// `QueryPerformanceCounter`; every other id uses `GetSystemTimeAsFileTime` as
/// the realtime clock. `rsi` is nonvolatile in the Win64 ABI, so the Win32 calls
/// preserve the output pointer.
pub(super) fn emit_shim_clock_gettime(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_clock_gettime");
    emitter.instruction("cmp edi, 1");                                          // CLOCK_MONOTONIC uses the Linux-compatible id emitted by __rt_hrtime
    emitter.instruction("je .Lclock_gettime_monotonic");                        // route monotonic requests through QueryPerformanceCounter
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + FILETIME(8)
    emitter.instruction("lea rcx, [rsp + 32]");                                 // &filetime
    emitter.instruction("call GetSystemTimeAsFileTime");                        // get 100ns intervals since 1601
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // load FILETIME (64-bit)
    emitter.instruction("mov r10, 116444736000000000");                         // Unix epoch offset (100ns intervals from 1601 to 1970)
    emitter.instruction("sub rax, r10");                                        // convert to Unix epoch (100ns intervals since 1970)
    emitter.instruction("xor rdx, rdx");                                        // clear high 64 bits of dividend
    emitter.instruction("mov r11, 10000000");                                   // divisor: 100ns intervals per second
    emitter.instruction("div r11");                                             // RDX:RAX / r11 → RAX = seconds, RDX = remainder
    emitter.instruction("mov QWORD PTR [rsi], rax");                            // store realtime seconds
    emitter.instruction("imul rdx, 100");                                       // convert remainder to nanoseconds
    emitter.instruction("mov QWORD PTR [rsi + 8], rdx");                        // store realtime nanoseconds
    emitter.instruction("xor rax, rax");                                        // return 0 (success)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return

    emitter.label(".Lclock_gettime_monotonic");
    emitter.instruction("sub rsp, 56");                                         // shadow(32) + counter/frequency qwords + alignment
    emitter.instruction("lea rcx, [rsp + 32]");                                 // &counter
    emitter.instruction("call QueryPerformanceCounter");                        // read the monotonic high-resolution counter
    emitter.instruction("test eax, eax");                                       // did QueryPerformanceCounter succeed?
    emitter.instruction("jz .Lclock_gettime_error");                            // no -> report failure without publishing uninitialized data
    emitter.instruction("lea rcx, [rsp + 40]");                                 // &frequency
    emitter.instruction("call QueryPerformanceFrequency");                      // read ticks per second
    emitter.instruction("test eax, eax");                                       // did QueryPerformanceFrequency succeed?
    emitter.instruction("jz .Lclock_gettime_error");                            // no -> report failure
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // rax = counter ticks
    emitter.instruction("xor rdx, rdx");                                        // clear the high dividend word
    emitter.instruction("div QWORD PTR [rsp + 40]");                            // rax = whole seconds, rdx = remaining ticks
    emitter.instruction("mov QWORD PTR [rsi], rax");                            // store monotonic seconds
    emitter.instruction("imul rax, rdx, 1000000000");                           // scale the sub-second remainder to nanosecond ticks
    emitter.instruction("xor rdx, rdx");                                        // clear the high dividend word
    emitter.instruction("div QWORD PTR [rsp + 40]");                            // rax = monotonic nanoseconds within the second
    emitter.instruction("mov QWORD PTR [rsi + 8], rax");                        // store monotonic nanoseconds
    emitter.instruction("xor eax, eax");                                        // return 0 (success)
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lclock_gettime_error");
    emitter.instruction("mov rax, -1");                                         // return -1 when the performance counter is unavailable
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return failure
    emitter.blank();
}

/// Emits a shim that wraps msvcrt `getenv(name)`: SysV `rdi`=name pointer → MSx64
/// `rcx`=name pointer, single argument, one-instruction shuffle. Return value (a
/// `char*` pointer, or NULL when unset) passes through unmodified in `rax` — no
/// `cdqe`, since this is a pointer, not a sign-tested int32 status.
pub(super) fn emit_shim_getenv(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_getenv");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // name
    emitter.instruction("call getenv");                                         // msvcrt getenv(name) -> char* or NULL
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (rax unchanged)
    emitter.blank();
}

/// Emits a shim that wraps msvcrt `putenv(string)`: SysV `rdi`=assignment string
/// pointer → MSx64 `rcx`=assignment string pointer, single argument, one-instruction
/// shuffle. Return value (int status, 0 on success) passes through unmodified in
/// `rax`; callers that sign-test/compare it (e.g. `cmp rax, 0`) still see the correct
/// low bits.
pub(super) fn emit_shim_putenv(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_putenv");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // "NAME=value" assignment string
    emitter.instruction("call _putenv");                                        // msvcrt _putenv(string) -> int status
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return status in rax
    emitter.blank();
}

/// Emits a shim that wraps msvcrt `tzset(void)`: zero arguments, so no register
/// shuffle is needed — the shim exists purely to route the call through the
/// `emit_call_c`/`windows_c_shim_name` registry instead of a bare `call tzset`,
/// which would be wrong for any future *argumented* Windows datetime call added to
/// this call site's callers on the strength of "tzset needs no ABI fixup".
pub(super) fn emit_shim_tzset(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_tzset");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("call _tzset");                                         // msvcrt _tzset(void) -> re-reads TZ
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a shim that wraps msvcrt `time(time_t*)`: SysV `rdi`=time_t* (or NULL) →
/// MSx64 `rcx`=time_t*, single argument, one-instruction shuffle. Return value
/// (`time_t`, seconds since epoch) passes through unmodified in `rax` — no `cdqe`,
/// since `time_t` is a 64-bit quantity here, not a sign-tested int32 status.
///
/// ## time_t width
/// MinGW-w64's `libmsvcrt.a` resolves the bare `time` symbol to a one-instruction
/// `jmp [rip+__imp_time]` thunk importing from `api-ms-win-crt-time-l1-1-0.dll`
/// (the Universal-CRT time forwarder that ships on every supported Windows target),
/// verified by disassembling the archive member that defines it — NOT a legacy
/// 32-bit `__time32_t` symbol. On 64-bit Windows `time_t` is always the 64-bit
/// `__time64_t` encoding, so the bare name is used directly; no `_time64` routing
/// is needed.
pub(super) fn emit_shim_time(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_time");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // time_t* out-param (or NULL)
    emitter.instruction("call time");                                           // msvcrt time(tloc) -> time_t (64-bit)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return time_t in rax
    emitter.blank();
}

/// Number of seconds in one proleptic-Gregorian 400-year cycle (146097 days):
/// `146097 * 86400`. The calendar (year/month/day/weekday/leap pattern) repeats
/// exactly every such cycle, which both this module's `__rt_win_safe_gmtime` and
/// `crate::codegen_support::runtime::system::mktime` (the construction-direction
/// counterpart) rely on.
const GREGORIAN_CYCLE_SECONDS: i64 = 146_097 * 86_400;

/// Emits `__rt_win_safe_gmtime`: a drop-in, same-ABI replacement for msvcrt
/// `gmtime(const time_t*)` (MSx64: `rcx`=time_t*, returns `struct tm*` in `rax`)
/// that stays correct outside msvcrt/UCRT's own supported decomposition window.
///
/// ## Why
/// Microsoft's UCRT `gmtime`/`localtime`/`_gmtime64`/`_localtime64` only support
/// timestamps from `1970-01-01T00:00:00Z` through `3000-12-31T23:59:59Z`
/// (documented range) and return `NULL` outside it — unlike glibc, which
/// decomposes the full `time_t` range. elephc allows PHP programs to format
/// arbitrary (including pre-1970 and far-future/BCE-proleptic) timestamps via
/// `date()`/`gmdate()`, so a caller that blindly dereferences msvcrt's result
/// crashes on any such value (surfaced under Wine as an unrecoverable hang, not
/// a clean non-zero exit, since the crash trips Wine's unhandled-exception path
/// rather than terminating the process outright).
///
/// ## How
/// Shifts the input timestamp by a whole number of Gregorian 400-year cycles
/// (`N = floor(ts / GREGORIAN_CYCLE_SECONDS)`, via a truncating `idiv` corrected
/// to floor/Euclidean semantics when the remainder's sign disagrees with the
/// positive divisor — the same floor-vs-truncating fixup
/// `crates/elephc-tz`'s `days_and_secs` performs in Rust) so the shifted
/// timestamp always lands in `[0, GREGORIAN_CYCLE_SECONDS)` — a ~400-year window
/// starting at the epoch, comfortably inside UCRT's supported range regardless
/// of how far `ts` originally was in the past or future. Calls the real msvcrt
/// `gmtime` on the shifted value (always safe, always non-`NULL`), then adds
/// `N * 400` back onto the returned `tm_year` (`+20` in msvcrt's own struct tm,
/// which this shim mutates in place before returning msvcrt's own pointer
/// unchanged) — the only field that differs between the two dates, since the
/// 400-year cycle exactly repeats month/day/weekday/leap-year pattern
/// (146097 days is exactly divisible by 7).
///
/// Every caller in this module that used to `call gmtime` directly on a
/// timestamp that PHP code controls (not a fixed "now" value) should call this
/// instead: [`emit_shim_gmtime`] (`__rt_sys_gmtime`, backing `gmdate()`) and the
/// bridge path in [`emit_shim_localtime`] (`__rt_sys_localtime`, backing
/// `date()`'s local decomposition of `ts + offset`).
pub(super) fn emit_shim_win_safe_gmtime(emitter: &mut Emitter) {
    emitter.label_global("__rt_win_safe_gmtime");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the shift/unshift locals
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved cycle count
    emitter.instruction("sub rsp, 48");                                         // 32-byte MSx64 shadow space + 8-byte shifted-ts slot + 8-byte saved-N slot (16-byte aligned)
    emitter.instruction("mov rax, QWORD PTR [rcx]");                            // rax = ts (*time_t)
    emitter.instruction(&format!("movabs r10, {}", GREGORIAN_CYCLE_SECONDS));   // r10 = GREGORIAN_CYCLE_SECONDS (146097 * 86400)
    emitter.instruction("cqo");                                                 // sign-extend rax into rdx:rax for the signed 128-bit-by-64-bit divide
    emitter.instruction("idiv r10");                                            // rax = trunc(ts / cycle), rdx = remainder (sign follows ts, per x86 idiv)
    emitter.instruction("test rdx, rdx");                                       // truncating remainder already floor-correct (>= 0)?
    emitter.instruction("jns .Lwin_safe_gmtime_no_adjust");                     // yes -> shifted_ts is already in [0, cycle)
    emitter.instruction("dec rax");                                             // floor-correct: one fewer whole cycle...
    emitter.instruction("add rdx, r10");                                        // ...and fold that cycle back into the remainder, landing it in [0, cycle)
    emitter.label(".Lwin_safe_gmtime_no_adjust");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save N (the cycle count) across the libc call
    emitter.instruction("mov QWORD PTR [rsp + 32], rdx");                       // stage shifted_ts (always UCRT-safe: within one cycle of the epoch)
    emitter.instruction("lea rcx, [rsp + 32]");                                 // &shifted_ts
    emitter.instruction("call gmtime");                                         // msvcrt gmtime(&shifted_ts) -> rax = its own static struct tm* (never NULL here)
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload N
    emitter.instruction("imul r10d, r10d, 400");                                // N * 400 years (fits comfortably in 32 bits for any realistic timestamp)
    emitter.instruction("add DWORD PTR [rax + 20], r10d");                      // tm_year += N*400, undoing the cycle shift (only field the shift changes)
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return struct tm* (msvcrt's own buffer, tm_year corrected) in rax
    emitter.blank();
}

/// Emits a shim that resolves local time for `localtime(const time_t*)` via the
/// elephc-tz IANA offset bridge (WF9) when available, falling back to plain
/// msvcrt `localtime()` otherwise. SysV `rdi`=time_t*; returns a `struct tm*` in
/// `rax`, matching the plain-passthrough shim it replaces.
///
/// ## Bridge path
/// If a date/mktime/strtotime call site has published `elephc_tz_offset` into
/// `_elephc_tz_offset_fn` (`crate::codegen_support::tz_bridge`), this: resolves
/// the active default timezone's identifier the same way
/// `__rt_date_default_timezone_get` does (`_php_default_tz_len` /
/// `_php_tz_env+3`, or `_php_tz_utc`); calls `elephc_tz_offset(name, ts)`
/// through the slot (MSx64-corrected via `Emitter::emit_native_bridge_call`);
/// and — when it resolves (not the `i64::MIN` sentinel) — decomposes
/// `ts + offset` via [`emit_shim_win_safe_gmtime`]'s `__rt_win_safe_gmtime`
/// (UTC decomposition needs no zoneinfo, and staying correct outside msvcrt's
/// own 1970-3000 `gmtime()` range matters here too: `ts + offset` can be
/// pre-1970 or far-future even when `ts` itself would not be) into
/// **elephc's own** `_win_tz_tm_buf`, a glibc-layout `struct tm`: the 9 plain
/// `tm_sec..tm_isdst` ints at `+0..+36` (copied from msvcrt's result, except
/// `tm_isdst` which is overwritten with the bridge's DST flag), `tm_gmtoff` (an
/// `i64`, not part of MinGW's own `struct tm` — see its `<time.h>`) at `+40`, and
/// `tm_zone` (`char*`) at `+48`, left `NULL` (no abbreviation lookup yet; the
/// `'T'` format specifier already treats a `NULL` `tm_zone` as "emit nothing",
/// so this degrades gracefully rather than misformatting). This buffer, not
/// msvcrt's own (which has no room for `tm_gmtoff`/`tm_zone`), is what the
/// `'Z'`/`'O'`/`'P'`/`'I'` `date()` format specifiers read at those offsets.
///
/// ## Fallback path
/// When the slot is null or the zone is unresolvable, this asks msvcrt
/// `localtime()` for the host/TZ-rule decomposition, then copies its 36-byte
/// result into elephc's 56-byte extended buffer and derives `tm_gmtoff` by
/// interpreting the local fields through `_mkgmtime`. This is essential even in
/// fallback mode: returning msvcrt's shorter allocation directly would make the
/// shared date formatter read out of bounds at `tm_gmtoff`/`tm_zone`. If UCRT
/// rejects an out-of-range timestamp, the safe proleptic UTC decomposition is
/// returned instead of dereferencing `NULL`; this loses an unavailable local
/// offset but keeps pre-1970/post-3000 formatting memory-safe and deterministic.
pub(super) fn emit_shim_localtime(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_localtime");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the bridge lookup's stack-backed locals
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved timestamp/offset/isdst
    // 80 bytes = a full, untouched 32-byte MSx64 shadow space at [rsp .. rsp+32)
    // (msvcrt callees may scribble there even though this shim never passes a
    // 5th+ stack arg) PLUS this frame's own 4 saved qwords at [rbp-32 .. rbp),
    // i.e. [rsp+32 .. rsp+64) — a smaller frame here let msvcrt's gmtime()
    // (whose calls into other MSVCRT internals do use shadow scratch) corrupt
    // the saved offset/isdst mid-decomposition.
    emitter.instruction("sub rsp, 80");                                         // shadow space + timestamp/offset/DST/abbreviation locals
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // ts = *timer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save ts (survives every call below)

    emit_resolve_default_zone_name(emitter, "localtime");                       // rdi = active default zone name (NUL-terminated)
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // arg1 = ts

    emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tz_offset_fn]");      // load the published elephc_tz_offset entry point (or null)
    emitter.instruction("test r9, r9");                                         // no date/time call site published the bridge?
    emitter.instruction("jz .Lsys_localtime_fallback");                         // yes -> plain msvcrt localtime()
    emitter.emit_native_bridge_call("r9", 2);                                   // rax = packed (offset*2 + isdst), or the i64::MIN sentinel
    emitter.instruction(&format!("mov r10, {}", TZ_OFFSET_SENTINEL_HEX));       // load the "unresolvable zone" sentinel
    emitter.instruction("cmp rax, r10");                                        // did the bridge resolve the active zone?
    emitter.instruction("je .Lsys_localtime_fallback");                         // no -> plain msvcrt localtime()

    emitter.instruction("mov r10, rax");                                        // r10 = packed value
    emitter.instruction("and r10, 1");                                          // r10 = isdst (packed value's low bit)
    emitter.instruction("mov r11, rax");                                        // r11 = packed value
    emitter.instruction("sub r11, r10");                                        // r11 = packed - isdst (always even)
    emitter.instruction("sar r11, 1");                                          // r11 = offset seconds (exact: halves an even two's-complement value)
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // rax = ts
    emitter.instruction("add rax, r11");                                        // rax = local_ts = ts + offset
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // stage local_ts so gmtime() can take its address
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save isdst across the gmtime() call (r10 is MSx64-volatile)
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save offset across the gmtime() call (r11 is MSx64-volatile)

    // -- resolve the exact transition abbreviation into its independent stable bridge cell --
    emit_resolve_default_zone_name(emitter, "localtime_abbr");                  // rdi = active zone name
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // rsi = original UTC timestamp
    emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tz_abbreviation_fn]"); // load optional abbreviation resolver
    emitter.instruction("test r9, r9");                                         // resolver published with the offset bridge?
    emitter.instruction("jz .Lsys_localtime_no_abbr");                          // retain a null tm_zone when unavailable
    emitter.emit_native_bridge_call("r9", 2);                                   // rax = stable NUL-terminated transition abbreviation
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve pointer across calendar decomposition
    emitter.instruction("jmp .Lsys_localtime_abbr_ready");                      // continue with the resolved pointer
    emitter.label(".Lsys_localtime_no_abbr");
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // no abbreviation bridge available
    emitter.label(".Lsys_localtime_abbr_ready");

    emitter.instruction("lea rcx, [rbp - 16]");                                 // &local_ts
    emitter.instruction("call __rt_win_safe_gmtime");                           // decomposes local_ts even outside UCRT's own 1970-3000 gmtime() range -> rax = its own static struct tm* (9 ints, no gmtoff/zone)

    emitter.instruction("lea r8, [rip + _win_tz_tm_buf]");                      // r8 = elephc's own glibc-layout struct tm buffer
    emitter.instruction("mov r11, QWORD PTR [rax + 0]");                        // copy tm_sec/tm_min (msvcrt's decomposition of local_ts)
    emitter.instruction("mov QWORD PTR [r8 + 0], r11");                         // store tm_sec and tm_min in the glibc-layout buffer
    emitter.instruction("mov r11, QWORD PTR [rax + 8]");                        // copy tm_hour/tm_mday
    emitter.instruction("mov QWORD PTR [r8 + 8], r11");                         // store tm_hour and tm_mday in the glibc-layout buffer
    emitter.instruction("mov r11, QWORD PTR [rax + 16]");                       // copy tm_mon/tm_year
    emitter.instruction("mov QWORD PTR [r8 + 16], r11");                        // store tm_mon and tm_year in the glibc-layout buffer
    emitter.instruction("mov r11, QWORD PTR [rax + 24]");                       // copy tm_wday/tm_yday
    emitter.instruction("mov QWORD PTR [r8 + 24], r11");                        // store tm_wday and tm_yday in the glibc-layout buffer
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the bridge's isdst
    emitter.instruction("mov DWORD PTR [r8 + 32], r10d");                       // tm_isdst = bridge isdst (not msvcrt's gmtime()-always-0)
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the bridge offset
    emitter.instruction("mov QWORD PTR [r8 + 40], r11");                        // tm_gmtoff = offset (glibc extension field, absent from MinGW's own struct tm)
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // stable abbreviation pointer owned by the bridge cell
    emitter.instruction("mov QWORD PTR [r8 + 48], r11");                        // tm_zone = exact transition abbreviation (CEST/CET/UTC/...)
    emitter.instruction("mov rax, r8");                                         // return elephc's own buffer, not msvcrt's undersized one
    emitter.instruction("add rsp, 80");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return struct tm* in rax

    emitter.label(".Lsys_localtime_fallback");
    emitter.instruction("lea rcx, [rbp - 8]");                                  // &ts (equivalent to the caller's original time_t* — same value)
    emitter.instruction("call localtime");                                      // msvcrt localtime(timer) -> struct tm* (its own static buffer)
    emitter.instruction("test rax, rax");                                       // UCRT rejects timestamps outside its supported decomposition range
    emitter.instruction("jz .Lsys_localtime_fallback_utc");                     // rejected -> return a safe proleptic UTC decomposition
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save msvcrt's 36-byte struct tm pointer
    emitter.instruction("mov rcx, rax");                                        // pass the normalized local fields to _mkgmtime
    emitter.instruction("call _mkgmtime");                                      // naive timestamp for the same local wall-clock fields
    emitter.instruction("sub rax, QWORD PTR [rbp - 8]");                        // derive local UTC offset = naive_local_ts - original_ts
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the derived offset while copying the source tm
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload msvcrt's source struct tm pointer
    emitter.instruction("lea r8, [rip + _win_tz_tm_buf]");                      // destination is always elephc's extended glibc-layout buffer
    emitter.instruction("mov r11, QWORD PTR [rax + 0]");                        // copy tm_sec/tm_min
    emitter.instruction("mov QWORD PTR [r8 + 0], r11");                         // store tm_sec/tm_min
    emitter.instruction("mov r11, QWORD PTR [rax + 8]");                        // copy tm_hour/tm_mday
    emitter.instruction("mov QWORD PTR [r8 + 8], r11");                         // store tm_hour/tm_mday
    emitter.instruction("mov r11, QWORD PTR [rax + 16]");                       // copy tm_mon/tm_year
    emitter.instruction("mov QWORD PTR [r8 + 16], r11");                        // store tm_mon/tm_year
    emitter.instruction("mov r11, QWORD PTR [rax + 24]");                       // copy tm_wday/tm_yday
    emitter.instruction("mov QWORD PTR [r8 + 24], r11");                        // store tm_wday/tm_yday
    emitter.instruction("mov r11d, DWORD PTR [rax + 32]");                      // load msvcrt tm_isdst
    emitter.instruction("mov DWORD PTR [r8 + 32], r11d");                       // preserve the fallback DST flag
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the derived UTC offset
    emitter.instruction("mov QWORD PTR [r8 + 40], r11");                        // synthesize the glibc-extension tm_gmtoff field
    emitter.instruction("mov QWORD PTR [r8 + 48], 0");                          // no reliable abbreviation pointer is available from msvcrt
    emitter.instruction("mov rax, r8");                                         // return the extended buffer, never msvcrt's undersized one
    emitter.instruction("add rsp, 80");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return struct tm* in rax

    emitter.label(".Lsys_localtime_fallback_utc");
    emitter.instruction("lea rcx, [rbp - 8]");                                  // pass the original timestamp to the cycle-safe UTC decomposer
    emitter.instruction("call __rt_win_safe_gmtime");                           // obtain a non-null proleptic decomposition outside UCRT's localtime range
    emitter.instruction("lea r8, [rip + _win_tz_tm_buf]");                      // destination is elephc's extended struct tm buffer
    emitter.instruction("mov r11, QWORD PTR [rax + 0]");                        // copy tm_sec/tm_min
    emitter.instruction("mov QWORD PTR [r8 + 0], r11");                         // store tm_sec/tm_min
    emitter.instruction("mov r11, QWORD PTR [rax + 8]");                        // copy tm_hour/tm_mday
    emitter.instruction("mov QWORD PTR [r8 + 8], r11");                         // store tm_hour/tm_mday
    emitter.instruction("mov r11, QWORD PTR [rax + 16]");                       // copy tm_mon/tm_year
    emitter.instruction("mov QWORD PTR [r8 + 16], r11");                        // store tm_mon/tm_year
    emitter.instruction("mov r11, QWORD PTR [rax + 24]");                       // copy tm_wday/tm_yday
    emitter.instruction("mov QWORD PTR [r8 + 24], r11");                        // store tm_wday/tm_yday
    emitter.instruction("mov DWORD PTR [r8 + 32], 0");                          // UTC fallback never observes DST
    emitter.instruction("mov QWORD PTR [r8 + 40], 0");                          // UTC fallback offset is zero
    emitter.instruction("mov QWORD PTR [r8 + 48], 0");                          // no abbreviation pointer is synthesized
    emitter.instruction("mov rax, r8");                                         // return the extended buffer
    emitter.instruction("add rsp, 80");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return struct tm* in rax
    emitter.blank();
}

/// Resolves the active default timezone's identifier into `rdi` as a
/// NUL-terminated string, exactly as `__rt_date_default_timezone_get`
/// (`codegen_support/runtime/system/date_default_timezone.rs`) does: `_php_tz_env
/// + 3` (skipping the `"TZ="` prefix) when `_php_default_tz_len` is nonzero,
/// otherwise the literal `_php_tz_utc` ("UTC"). Both are NUL-terminated by their
/// writers (`__rt_date_default_timezone_set`/`__rt_tz_init_utc`), so the result is
/// a valid `elephc_tz_offset` `name` argument. `label_prefix` must be unique per
/// caller (this file has no per-function assembler scoping — see
/// `Emitter::label_global`'s Windows branch).
fn emit_resolve_default_zone_name(emitter: &mut Emitter, label_prefix: &str) {
    emitter.instruction("mov rax, QWORD PTR [rip + _php_default_tz_len]");      // load the configured default-timezone length
    emitter.instruction("test rax, rax");                                       // any zone explicitly set?
    emitter.instruction(&format!("jnz .Lsys_{}_have_zone", label_prefix));      // yes -> use the configured identifier
    emitter.instruction("lea rdi, [rip + _php_tz_utc]");                        // none set -> default to "UTC"
    emitter.instruction(&format!("jmp .Lsys_{}_zone_ready", label_prefix));     // bypass the configured-zone path after selecting UTC
    emitter.label(&format!(".Lsys_{}_have_zone", label_prefix));
    emitter.instruction("lea rdi, [rip + _php_tz_env]");                        // address of the configured "TZ=<id>" env buffer
    emitter.instruction("add rdi, 3");                                          // skip the "TZ=" prefix -> identifier pointer
    emitter.label(&format!(".Lsys_{}_zone_ready", label_prefix));
}

/// Emits a shim that wraps `gmtime(const time_t*)`: SysV `rdi`=time_t* →
/// MSx64 `rcx`=time_t*, single argument, one-instruction shuffle, calling
/// [`emit_shim_win_safe_gmtime`]'s `__rt_win_safe_gmtime` (not bare msvcrt
/// `gmtime` — see its docblock: msvcrt/UCRT's `gmtime` returns `NULL` outside
/// 1970-3000, and `date()`/`gmdate()` must format arbitrary PHP-supplied
/// timestamps) and then copies the 9-int result into elephc's own
/// glibc-layout `struct tm` buffer (`_win_tz_tm_buf` — shared with
/// [`emit_shim_localtime`], which never runs concurrently with this shim
/// within one `date()`/`gmdate()` formatting call) so the `'Z'`/`'O'`/`'P'`/`'I'`
/// `date()` format specifiers find a real `tm_gmtoff`/`tm_zone` at offsets
/// `+40`/`+48` instead of reading whatever bytes happen to follow msvcrt's own
/// undersized 9-int buffer.
///
/// ## Not part of the WF9 offset bridge
/// `gmtime` itself always decomposes in UTC regardless of `TZ`, so it needs no
/// offset resolution (unlike [`emit_shim_localtime`], which the bridge routes
/// through this same `__rt_win_safe_gmtime` helper internally, on its own
/// frame, when it decomposes `ts + offset`). The synthesized fields are
/// therefore fixed constants, not a bridge lookup: `tm_gmtoff = 0` and
/// `tm_isdst = 0` (UTC is always offset zero and never observes
/// daylight-saving time), and `tm_zone = NULL` (no abbreviation lookup; the
/// `'T'` format specifier already hardcodes `"GMT"` for `gmdate()` without
/// reading `tm_zone` at all — see
/// `codegen_support::runtime::system::date::linux_x86_64` — and treats a
/// `NULL` `tm_zone` as "emit nothing" on the `date()`/local path, matching
/// `emit_shim_localtime`'s own fallback convention).
pub(super) fn emit_shim_gmtime(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_gmtime");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // const time_t*
    emitter.instruction("call __rt_win_safe_gmtime");                           // decomposes timer even outside UCRT's own 1970-3000 gmtime() range -> rax = its own static struct tm* (9 ints, no gmtoff/zone)
    emitter.instruction("lea r8, [rip + _win_tz_tm_buf]");                      // r8 = elephc's own glibc-layout struct tm buffer
    emitter.instruction("mov r9, QWORD PTR [rax + 0]");                         // copy tm_sec/tm_min
    emitter.instruction("mov QWORD PTR [r8 + 0], r9");                          // store tm_sec and tm_min in the glibc-layout buffer
    emitter.instruction("mov r9, QWORD PTR [rax + 8]");                         // copy tm_hour/tm_mday
    emitter.instruction("mov QWORD PTR [r8 + 8], r9");                          // store tm_hour and tm_mday in the glibc-layout buffer
    emitter.instruction("mov r9, QWORD PTR [rax + 16]");                        // copy tm_mon/tm_year
    emitter.instruction("mov QWORD PTR [r8 + 16], r9");                         // store tm_mon and tm_year in the glibc-layout buffer
    emitter.instruction("mov r9, QWORD PTR [rax + 24]");                        // copy tm_wday/tm_yday
    emitter.instruction("mov QWORD PTR [r8 + 24], r9");                         // store tm_wday and tm_yday in the glibc-layout buffer
    emitter.instruction("mov DWORD PTR [r8 + 32], 0");                          // tm_isdst = 0 (UTC never observes DST)
    emitter.instruction("mov QWORD PTR [r8 + 40], 0");                          // tm_gmtoff = 0 (UTC is always offset zero)
    emitter.instruction("mov QWORD PTR [r8 + 48], 0");                          // tm_zone = NULL (no abbreviation lookup; see docblock)
    emitter.instruction("mov rax, r8");                                         // return elephc's own buffer, not msvcrt's undersized one
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return struct tm* in rax
    emitter.blank();
}

/// Emits a shim that wraps msvcrt `mktime(struct tm*)`: SysV `rdi`=struct tm* →
/// MSx64 `rcx`=struct tm*, converting the local wall-clock fields to a Unix
/// timestamp via the elephc-tz IANA offset bridge (WF9) when available, falling
/// back to plain msvcrt `mktime()`'s result otherwise. Return value (`time_t`) is
/// in `rax`. `mktime` also normalizes the `struct tm*` argument in place (e.g.
/// `tm_year`/`tm_wday`), which callers such as `__rt_mktime_shifted` rely on —
/// unaffected here, since msvcrt `mktime()` is always called first for exactly
/// that normalization side effect (see below), same as the plain-passthrough
/// shim this replaces.
///
/// ## Bridge path
/// Always calls plain msvcrt `mktime(tm)` first — this normalizes the struct
/// (calendar overflow carrying is TZ-independent, so this step's normalization
/// is unaffected by which timezone ultimately supplies the offset) and its
/// return value is the fallback answer. Then, with the now-normalized fields,
/// calls msvcrt `_mkgmtime(tm)` to get `naive_utc_ts`: the timestamp that would
/// correspond to those exact `y/m/d/h/m/s` fields if read as literal UTC (this
/// re-normalizes identically, since the fields are already canonical — msvcrt
/// `_mkgmtime`, like `mktime`, only carries calendar overflow). If a call site
/// has published `elephc_tz_offset` into `_elephc_tz_offset_fn`
/// (`crate::codegen_support::tz_bridge`), resolves the active default timezone's
/// offset at `naive_utc_ts`, derives a candidate UTC instant, then resolves the
/// offset once more at that candidate. The second lookup is what makes ordinary
/// timestamps adjacent to a DST boundary use the transition rule at the actual
/// instant rather than at the naive wall-clock reading. It returns
/// `naive_utc_ts - corrected_offset` instead of the plain `mktime()` fallback.
///
/// ## Fallback path
/// When the slot is null or the zone is unresolvable (an unknown name, or one
/// of the 11 legacy abbreviation-zones with no transition data), returns the
/// plain msvcrt `mktime()` result computed at entry — byte-identical to the
/// shim this replaces: offsets are then correct only for UTC or the host's
/// system timezone.
pub(super) fn emit_shim_mktime(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_mktime");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the bridge lookup's stack-backed locals
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved tm pointer/results
    // See emit_shim_localtime's frame-sizing note: 80 bytes keeps a
    // full, untouched 32-byte MSx64 shadow space below this frame's 4 saved
    // qwords plus the UCRT pre-1970 shift marker.
    emitter.instruction("sub rsp, 80");                                         // shadow space, saved values, and cycle marker (16-byte aligned)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the struct tm pointer

    // -- UCRT accepts years 1970..3000 only; shift 1900..1969 by one identical Gregorian cycle --
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // no internal 400-year shift by default
    emitter.instruction("mov eax, DWORD PTR [rdi + 20]");                       // tm_year, measured from 1900
    emitter.instruction("cmp eax, 70");                                         // already at least 1970?
    emitter.instruction("jge .Lsys_mktime_ucrt_ready");                         // UCRT accepts the original year
    emitter.instruction("add DWORD PTR [rdi + 20], 400");                       // preserve calendar shape while entering UCRT's range
    emitter.instruction("mov QWORD PTR [rbp - 40], 1");                         // remember to undo one cycle in fields/results
    emitter.label(".Lsys_mktime_ucrt_ready");

    emitter.instruction("mov rcx, rdi");                                        // struct tm*
    emitter.instruction("call mktime");                                         // msvcrt mktime(tm) -> time_t; also normalizes tm in place (calendar overflow carry)
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // internally shifted for UCRT?
    emitter.instruction("je .Lsys_mktime_fallback_ready");                      // retain the native result
    emitter.instruction(&format!("movabs r10, {}", GREGORIAN_CYCLE_SECONDS));   // seconds in the added 400-year cycle
    emitter.instruction("sub rax, r10");                                        // recover the original pre-1970 timestamp
    emitter.label(".Lsys_mktime_fallback_ready");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the fallback/default result

    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload the (now-normalized) struct tm pointer
    emitter.instruction("call _mkgmtime");                                      // msvcrt _mkgmtime(tm) -> naive-UTC reading of the same wall-clock fields
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // internally shifted for UCRT?
    emitter.instruction("je .Lsys_mktime_naive_ready");                         // retain the native UTC reading
    emitter.instruction(&format!("movabs r10, {}", GREGORIAN_CYCLE_SECONDS));   // seconds in the added cycle
    emitter.instruction("sub rax, r10");                                        // recover the original naive UTC timestamp
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // normalized shifted struct tm
    emitter.instruction("sub DWORD PTR [r10 + 20], 400");                       // restore the caller-visible original tm_year cycle
    emitter.label(".Lsys_mktime_naive_ready");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save naive_utc_ts

    emit_resolve_default_zone_name(emitter, "mktime");                          // rdi = active default zone name (NUL-terminated)
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // arg1 = naive_utc_ts

    emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tz_offset_fn]");      // load the published elephc_tz_offset entry point (or null)
    emitter.instruction("test r9, r9");                                         // no date/time call site published the bridge?
    emitter.instruction("jz .Lsys_mktime_done");                                // yes -> keep the plain mktime() fallback result
    emitter.emit_native_bridge_call("r9", 2);                                   // rax = packed (offset*2 + isdst), or the i64::MIN sentinel
    emitter.instruction(&format!("mov r10, {}", TZ_OFFSET_SENTINEL_HEX));       // load the "unresolvable zone" sentinel
    emitter.instruction("cmp rax, r10");                                        // did the bridge resolve the active zone?
    emitter.instruction("je .Lsys_mktime_done");                                // no -> keep the plain mktime() fallback result

    emitter.instruction("mov r10, rax");                                        // r10 = packed value
    emitter.instruction("and r10, 1");                                          // r10 = isdst (unused for mktime's own result; kept for symmetry with the lookup contract)
    emitter.instruction("sub rax, r10");                                        // rax = packed - isdst (always even)
    emitter.instruction("sar rax, 1");                                          // rax = offset seconds (exact: halves an even two's-complement value)
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // r11 = naive_utc_ts
    emitter.instruction("sub r11, rax");                                        // r11 = first candidate UTC timestamp
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // preserve the first candidate across the transition-aware second lookup

    emitter.instruction("mov rsi, r11");                                        // arg1 = candidate UTC timestamp
    emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tz_offset_fn]");      // reload the bridge entry point clobbered by the first native call
    emitter.emit_native_bridge_call("r9", 2);                                   // resolve the zone offset at the actual candidate instant
    emitter.instruction(&format!("mov r10, {}", TZ_OFFSET_SENTINEL_HEX));       // load the unresolvable-zone sentinel
    emitter.instruction("cmp rax, r10");                                        // did the second lookup resolve?
    emitter.instruction("je .Lsys_mktime_use_first_candidate");                 // no -> retain the already valid first candidate
    emitter.instruction("mov r10, rax");                                        // r10 = second packed value
    emitter.instruction("and r10, 1");                                          // r10 = second lookup's DST bit
    emitter.instruction("sub rax, r10");                                        // remove the DST bit from the packed offset
    emitter.instruction("sar rax, 1");                                          // rax = corrected offset seconds
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // r11 = naive_utc_ts
    emitter.instruction("sub r11, rax");                                        // r11 = transition-corrected UTC timestamp
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // replace the first candidate with the corrected one
    emitter.label(".Lsys_mktime_use_first_candidate");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // load the best bridge-derived candidate
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // overwrite the saved result with the bridge-corrected timestamp

    emitter.label(".Lsys_mktime_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // rax = final result (bridge-corrected, or the plain mktime() fallback)
    emitter.instruction("add rsp, 80");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return time_t in rax
    emitter.blank();
}

/// Emits `__rt_sys_gettimeofday`, a custom-body shim synthesizing POSIX
/// `gettimeofday(struct timeval* tv, void* tz)` — msvcrt/UCRT export no such
/// symbol — via `GetSystemTimeAsFileTime`, modeled on [`emit_shim_clock_gettime`]
/// (which already converts a `FILETIME` to Unix-epoch seconds+remainder). SysV:
/// `rdi`=timeval*, `rsi`=tz (ignored, matching Linux's tz-ignored contract this
/// runtime already relies on). Unlike `clock_gettime`'s `timespec` (tv_nsec @ +8,
/// nanoseconds), `struct timeval` stores `tv_sec` @ +0 and `tv_usec` @ +8 in
/// MICROseconds — verified against this runtime's consumers: `time.rs`'s
/// `__rt_time` reads `tv_sec` from `[rsp]`/`[rdi]`, and `microtime.rs` reads both
/// `tv_sec` and `tv_usec` to build the fractional-second result. The FILETIME
/// remainder (100ns units) is divided by 10 a second time to convert it from
/// 100ns units to microseconds (vs. `clock_gettime`'s `imul rdx, 100` to convert
/// to nanoseconds). Always returns 0 (success) in `rax`, matching the success case
/// this runtime relies on (the return value is never checked by our callers).
///
/// Writing `[rdi]`/`[rdi + 8]` *after* `call GetSystemTimeAsFileTime` is safe with no
/// spill because `rdi` (and `rsi`) are nonvolatile (callee-saved) in the Win64 ABI —
/// the Win32 call preserves them — so do not add an unnecessary rdi save/restore here.
pub(super) fn emit_shim_gettimeofday(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_gettimeofday");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + FILETIME(8)
    emitter.instruction("lea rcx, [rsp + 32]");                                 // &filetime
    emitter.instruction("call GetSystemTimeAsFileTime");                        // get 100ns intervals since 1601
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // load FILETIME (64-bit)
    emitter.instruction("mov r10, 116444736000000000");                         // Unix epoch offset (100ns intervals from 1601 to 1970)
    emitter.instruction("sub rax, r10");                                        // convert to Unix epoch (100ns intervals since 1970)
    emitter.instruction("xor rdx, rdx");                                        // clear high 64 bits of dividend
    emitter.instruction("mov r11, 10000000");                                   // divisor: 100ns intervals per second
    emitter.instruction("div r11");                                             // RDX:RAX / r11 -> RAX = seconds, RDX = remainder (100ns units)
    emitter.instruction("mov QWORD PTR [rdi], rax");                            // store tv_sec @ +0
    emitter.instruction("mov rax, rdx");                                        // rax = remainder (100ns units, 0..9999999)
    emitter.instruction("xor rdx, rdx");                                        // clear high 64 bits of dividend
    emitter.instruction("mov r11, 10");                                         // divisor: 10 x 100ns = 1 microsecond
    emitter.instruction("div r11");                                             // rax = remainder / 10 = microseconds
    emitter.instruction("mov QWORD PTR [rdi + 8], rax");                        // store tv_usec @ +8 (microseconds, not nanoseconds)
    emitter.instruction("xor rax, rax");                                        // return 0 (success)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits the `__rt_sys_getrusage` shim: converts Linux `getrusage(who, rusage*)`
/// (syscall 98) to Win32 `GetProcessTimes` and fills a Linux `struct rusage`.
///
/// SysV: rdi = `who` (int: 0 = RUSAGE_SELF, -1 = RUSAGE_CHILDREN, 1 = RUSAGE_THREAD),
/// rsi = `struct rusage*` out. Only `RUSAGE_SELF` (who == 0) is populated: the user
/// and kernel FILETIMEs from `GetProcessTimes` are converted to `ru_utime`/`ru_stime`
/// (struct timeval: tv_sec @+0/+16, tv_usec @+8/+24). All other fields
/// (ru_maxrss..ru_nivcsw, offsets 32..144 = 14 qwords) are zeroed — Windows has no
/// clean RSS/page-fault equivalent and tests only check the time fields. For
/// `RUSAGE_CHILDREN`/`RUSAGE_THREAD` the whole struct is zeroed and 0 returned
/// (no child handle is available). Returns 0 on success, -1 on `GetProcessTimes`
/// failure.
///
/// `struct rusage` (Linux x86_64) layout, hardcoded here with offsets:
/// - 0: ru_utime.tv_sec (i64), 8: ru_utime.tv_usec (i64)
/// - 16: ru_stime.tv_sec (i64), 24: ru_stime.tv_usec (i64)
/// - 32..144: ru_maxrss..ru_nivcsw (14 × i64), total struct = 144 bytes.
///
/// FILETIME is a 64-bit count of 100ns intervals. tv_sec = ft / 10_000_000;
/// tv_usec = (ft % 10_000_000) / 10.
pub(super) fn emit_shim_getrusage(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_getrusage");
    // -- frame: shadow(32) + 5th-arg slot(8) + 4 FILETIMEs(32) + spill rusage(8) +
    //    spill who(8) = 88 bytes; 88 ≡ 8 mod 16 so rsp ≡ 0 at the Win32 call site --
    //    [rsp+32] = lpUserTime ptr (MSx64 stack-arg slot)
    //    [rsp+40] = creation FILETIME, [rsp+48] = exit, [rsp+56] = kernel, [rsp+64] = user
    //    [rsp+72] = spill rusage ptr, [rsp+80] = spill who
    emitter.instruction("sub rsp, 88");                                         // allocate frame (88 ≡ 8 mod 16)
    emitter.instruction("mov QWORD PTR [rsp + 72], rsi");                       // spill rusage out-pointer (rsi is volatile on MSx64)
    emitter.instruction("mov DWORD PTR [rsp + 80], edi");                       // spill who (edi — 32-bit int, SysV arg1)
    emitter.instruction("cmp DWORD PTR [rsp + 80], 0");                         // who == RUSAGE_SELF (0)?
    emitter.instruction("jne .Lgetrusage_zero");                                // → no child/thread handle: zero struct, return 0
    // -- GetProcessTimes(GetCurrentProcess()=(HANDLE)-1, &creation, &exit, &kernel, &user) --
    emitter.instruction("mov rcx, -1");                                         // hProcess = current-process pseudo-handle (HANDLE)-1
    emitter.instruction("lea rdx, [rsp + 40]");                                 // lpCreationTime = &creation FILETIME
    emitter.instruction("lea r8, [rsp + 48]");                                  // lpExitTime = &exit FILETIME
    emitter.instruction("lea r9, [rsp + 56]");                                  // lpKernelTime = &kernel FILETIME
    emitter.instruction("lea rax, [rsp + 64]");                                 // lpUserTime = &user FILETIME
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // 5th arg (lpUserTime) goes in the MSx64 stack-arg slot
    emitter.instruction("call GetProcessTimes");                                // fill the four FILETIMEs; eax = 0 on failure
    emitter.instruction("test eax, eax");                                       // GetProcessTimes succeeded?
    emitter.instruction("jz .Lgetrusage_fail");                                 // → failure: zero struct, return -1
    // -- convert kernel FILETIME → ru_stime (tv_sec @+16, tv_usec @+24) --
    emitter.instruction("mov r11, QWORD PTR [rsp + 72]");                       // rusage pointer (reloaded; r11 stays across no further calls)
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // kernel FILETIME (64-bit 100ns count)
    emitter.instruction("xor rdx, rdx");                                        // clear high half of dividend before unsigned div
    emitter.instruction("mov ecx, 10000000");                                   // divisor: 10_000_000 (100ns units per second)
    emitter.instruction("div rcx");                                             // rax = tv_sec, rdx = remainder (100ns units)
    emitter.instruction("mov QWORD PTR [r11 + 16], rax");                       // ru_stime.tv_sec = kernel_seconds
    emitter.instruction("mov rax, rdx");                                        // remainder (100ns units within the last second)
    emitter.instruction("xor rdx, rdx");                                        // clear high half of dividend before unsigned div
    emitter.instruction("mov ecx, 10");                                         // divisor: 10 (100ns units per microsecond)
    emitter.instruction("div rcx");                                             // rax = tv_usec
    emitter.instruction("mov QWORD PTR [r11 + 24], rax");                       // ru_stime.tv_usec = kernel_useconds
    // -- convert user FILETIME → ru_utime (tv_sec @+0, tv_usec @+8) --
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // user FILETIME (64-bit 100ns count)
    emitter.instruction("xor rdx, rdx");                                        // clear high half of dividend before unsigned div
    emitter.instruction("mov ecx, 10000000");                                   // divisor: 10_000_000 (100ns units per second)
    emitter.instruction("div rcx");                                             // rax = tv_sec, rdx = remainder (100ns units)
    emitter.instruction("mov QWORD PTR [r11 + 0], rax");                        // ru_utime.tv_sec = user_seconds
    emitter.instruction("mov rax, rdx");                                        // remainder (100ns units within the last second)
    emitter.instruction("xor rdx, rdx");                                        // clear high half of dividend before unsigned div
    emitter.instruction("mov ecx, 10");                                         // divisor: 10 (100ns units per microsecond)
    emitter.instruction("div rcx");                                             // rax = tv_usec
    emitter.instruction("mov QWORD PTR [r11 + 8], rax");                        // ru_utime.tv_usec = user_useconds
    // -- zero ru_maxrss..ru_nivcsw (offsets 32..144, 14 qwords) --
    emitter.instruction("mov rdi, r11");                                        // rep stosq destination = rusage base
    emitter.instruction("add rdi, 32");                                         // point at ru_maxrss (offset 32)
    emitter.instruction("xor rax, rax");                                        // fill value = 0
    emitter.instruction("mov rcx, 14");                                         // 14 qwords (112 bytes) cover ru_maxrss..ru_nivcsw
    emitter.instruction("rep stosq");                                           // zero the remaining rusage fields
    emitter.instruction("xor eax, eax");                                        // return 0 (success)
    emitter.instruction("add rsp, 88");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    // -- who != RUSAGE_SELF: zero the whole struct (18 qwords = 144 bytes) and return 0 --
    emitter.label(".Lgetrusage_zero");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 72]");                       // rusage pointer
    emitter.instruction("xor rax, rax");                                        // fill value = 0
    emitter.instruction("mov rcx, 18");                                         // 18 qwords (144 bytes) = full struct rusage
    emitter.instruction("rep stosq");                                           // zero the entire struct
    emitter.instruction("xor eax, eax");                                        // return 0 (success, but no times available)
    emitter.instruction("add rsp, 88");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    // -- GetProcessTimes failed: zero the whole struct and return -1 --
    emitter.label(".Lgetrusage_fail");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 72]");                       // rusage pointer
    emitter.instruction("xor rax, rax");                                        // fill value = 0
    emitter.instruction("mov rcx, 18");                                         // 18 qwords (144 bytes) = full struct rusage
    emitter.instruction("rep stosq");                                           // zero the entire struct
    emitter.instruction("mov eax, -1");                                         // return -1 (failure)
    emitter.instruction("add rsp, 88");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits a clock_getres shim — writes a 1ns resolution (best-effort) into the
/// output `struct timespec *res` (rsi), NULL-guarded since POSIX permits
/// `res == NULL`. `struct timespec` is `{ tv_sec @+0, tv_nsec @+8 }`, both
/// 8-byte fields on x64.
pub(super) fn emit_shim_clock_getres(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_clock_getres");
    emitter.instruction("test rsi, rsi");                                       // res == NULL? (POSIX permits a NULL res)
    emitter.instruction("jz .Lclock_getres_done");                              // NULL -> skip the write, just report success
    emitter.instruction("mov QWORD PTR [rsi], 0");                              // tv_sec = 0
    emitter.instruction("mov QWORD PTR [rsi + 8], 1");                          // tv_nsec = 1 (1ns best-effort, matches docblock)
    emitter.label(".Lclock_getres_done");
    emitter.instruction("xor rax, rax");                                        // return 0 (success)
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}
