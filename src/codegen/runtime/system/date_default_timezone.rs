//! Purpose:
//! Emits the `__rt_date_default_timezone_set` / `__rt_date_default_timezone_get` runtime helpers.
//! These back PHP's default-timezone API by driving libc's own timezone machinery.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - `_set` writes `"TZ=<id>"` into the static `_php_tz_env` buffer, calls libc `putenv` + `tzset`,
//!   and records the identifier length in `_php_default_tz_len`. libc then resolves the zone
//!   (offsets + DST) from the system tz database for every later `localtime` (so `date()` becomes
//!   timezone-aware with no embedded tzdata). `_get` returns the stored identifier, or `"UTC"`.
//! - String result convention: pointer in `x1`/`rax`, length in `x2`/`rdx`. `_set` returns the PHP
//!   boolean in `x0`/`rax`. The `_php_tz_env` buffer is static so the pointer handed to `putenv`
//!   stays valid (putenv does not copy).

use crate::codegen::abi::emit_symbol_address;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits both default-timezone runtime helpers for the active target.
///
/// `__rt_date_default_timezone_set` takes a string (ptr in `x1`/`rax`, len in `x2`/`rdx`) and
/// returns PHP `true` (`1`) in `x0`/`rax`. `__rt_date_default_timezone_get` takes no arguments and
/// returns the stored timezone string (ptr/len) or the literal `"UTC"`.
pub fn emit_date_default_timezone(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_set_aarch64(emitter);
            emit_get_aarch64(emitter);
            emit_tz_init_utc_aarch64(emitter);
        }
        Arch::X86_64 => {
            emit_set_x86_64(emitter);
            emit_get_x86_64(emitter);
            emit_tz_init_utc_x86_64(emitter);
        }
    }
}

/// Emits the AArch64 `__rt_date_default_timezone_set`: writes `"TZ=<id>"` to `_php_tz_env`,
/// applies it via libc `putenv`+`tzset`, stores the id length, and returns `1`.
fn emit_set_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: date_default_timezone_set ---");
    emitter.label_global("__rt_date_default_timezone_set");

    // -- frame; preserve x19 (timezone id length) across the libc calls --
    emitter.instruction("sub sp, sp, #32");                                     // allocate a 16-aligned frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set the frame pointer
    emitter.instruction("str x19, [sp]");                                       // preserve x19 across putenv/tzset

    // -- clamp the identifier length to the buffer capacity (250 = 264 - "TZ=" - NUL margin) --
    emitter.instruction("mov x4, #250");                                        // maximum stored identifier length
    emitter.instruction("cmp x2, x4");                                          // requested length vs cap
    emitter.instruction("csel x19, x2, x4, ls");                                // x19 = min(len, 250)

    // -- write the "TZ=" prefix into the static env buffer --
    emit_symbol_address(emitter, "x3", "_php_tz_env");
    emitter.instruction("mov w4, #84");                                         // 'T'
    emitter.instruction("strb w4, [x3]");                                       // _php_tz_env[0] = 'T'
    emitter.instruction("mov w4, #90");                                         // 'Z'
    emitter.instruction("strb w4, [x3, #1]");                                   // _php_tz_env[1] = 'Z'
    emitter.instruction("mov w4, #61");                                         // '='
    emitter.instruction("strb w4, [x3, #2]");                                   // _php_tz_env[2] = '='

    // -- copy the identifier bytes after the prefix --
    emitter.instruction("add x6, x3, #3");                                      // destination = buffer + 3
    emitter.instruction("mov x5, #0");                                          // copy index
    emitter.label("__rt_ddtz_set_copy");
    emitter.instruction("cmp x5, x19");                                         // all identifier bytes copied?
    emitter.instruction("b.ge __rt_ddtz_set_copy_done");                        // yes → terminate
    emitter.instruction("ldrb w7, [x1, x5]");                                   // load identifier byte
    emitter.instruction("strb w7, [x6, x5]");                                   // store it after the prefix
    emitter.instruction("add x5, x5, #1");                                      // advance the copy index
    emitter.instruction("b __rt_ddtz_set_copy");                                // continue copying
    emitter.label("__rt_ddtz_set_copy_done");
    emitter.instruction("strb wzr, [x6, x19]");                                 // NUL-terminate the env string

    // -- apply via libc and re-read the zone --
    emit_symbol_address(emitter, "x0", "_php_tz_env");
    emitter.bl_c("putenv");                                                     // putenv("TZ=<id>")
    emitter.bl_c("tzset");                                                      // re-read TZ so localtime uses it

    // -- record the identifier length for date_default_timezone_get and return true --
    emit_symbol_address(emitter, "x3", "_php_default_tz_len");
    emitter.instruction("str x19, [x3]");                                       // _php_default_tz_len = identifier length
    emitter.instruction("mov x0, #1");                                          // PHP true
    emitter.instruction("ldr x19, [sp]");                                       // restore x19
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate the frame
    emitter.instruction("ret");                                                 // return
}

/// Emits the AArch64 `__rt_date_default_timezone_get`: returns the stored id (`_php_tz_env+3` /
/// `_php_default_tz_len`) or the literal `"UTC"` when no zone has been set.
fn emit_get_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: date_default_timezone_get ---");
    emitter.label_global("__rt_date_default_timezone_get");

    emit_symbol_address(emitter, "x3", "_php_default_tz_len");
    emitter.instruction("ldr x2, [x3]");                                        // load the stored identifier length
    emitter.instruction("cbz x2, __rt_ddtz_get_default");                       // none set → default to "UTC"
    emit_symbol_address(emitter, "x1", "_php_tz_env");
    emitter.instruction("add x1, x1, #3");                                      // skip the "TZ=" prefix → identifier ptr
    emitter.instruction("ret");                                                 // return ptr in x1, len in x2
    emitter.label("__rt_ddtz_get_default");
    emit_symbol_address(emitter, "x1", "_php_tz_utc");
    emitter.instruction("mov x2, #3");                                          // length of "UTC"
    emitter.instruction("ret");                                                 // return the default zone
}

/// Emits the x86_64 `__rt_date_default_timezone_set` (System V): input ptr in `rax`, len in `rdx`;
/// returns `1` in `rax`. Mirrors the AArch64 helper, preserving the length in `rbx`.
fn emit_set_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: date_default_timezone_set ---");
    emitter.label_global("__rt_date_default_timezone_set");

    // -- frame; preserve rbx (id length) across libc calls; keep rsp 16-aligned at the calls --
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame pointer
    emitter.instruction("push rbx");                                            // preserve rbx across putenv/tzset
    emitter.instruction("sub rsp, 8");                                          // 16-byte align rsp for the libc calls
    emitter.instruction("mov rbx, rdx");                                        // rbx = identifier length

    // -- clamp the identifier length to the buffer capacity --
    emitter.instruction("cmp rbx, 250");                                        // requested length vs cap
    emitter.instruction("jbe __rt_ddtz_set_x86_len_ok");                        // within cap → keep
    emitter.instruction("mov rbx, 250");                                        // otherwise clamp to 250
    emitter.label("__rt_ddtz_set_x86_len_ok");

    // -- write the "TZ=" prefix into the static env buffer --
    emit_symbol_address(emitter, "rsi", "_php_tz_env");
    emitter.instruction("mov BYTE PTR [rsi], 84");                              // _php_tz_env[0] = 'T'
    emitter.instruction("mov BYTE PTR [rsi + 1], 90");                          // _php_tz_env[1] = 'Z'
    emitter.instruction("mov BYTE PTR [rsi + 2], 61");                          // _php_tz_env[2] = '='

    // -- copy the identifier bytes after the prefix (rax = source ptr) --
    emitter.instruction("lea rdi, [rsi + 3]");                                  // destination = buffer + 3
    emitter.instruction("xor rcx, rcx");                                        // copy index
    emitter.label("__rt_ddtz_set_x86_copy");
    emitter.instruction("cmp rcx, rbx");                                        // all identifier bytes copied?
    emitter.instruction("jge __rt_ddtz_set_x86_copy_done");                     // yes → terminate
    emitter.instruction("mov r8b, BYTE PTR [rax + rcx]");                       // load identifier byte
    emitter.instruction("mov BYTE PTR [rdi + rcx], r8b");                       // store it after the prefix
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_ddtz_set_x86_copy");                          // continue copying
    emitter.label("__rt_ddtz_set_x86_copy_done");
    emitter.instruction("mov BYTE PTR [rdi + rbx], 0");                         // NUL-terminate the env string

    // -- apply via libc and re-read the zone --
    emit_symbol_address(emitter, "rdi", "_php_tz_env");
    emitter.instruction("call putenv");                                         // putenv("TZ=<id>")
    emitter.instruction("call tzset");                                          // re-read TZ so localtime uses it

    // -- record the identifier length for date_default_timezone_get and return true --
    emit_symbol_address(emitter, "rsi", "_php_default_tz_len");
    emitter.instruction("mov QWORD PTR [rsi], rbx");                            // _php_default_tz_len = identifier length
    emitter.instruction("mov rax, 1");                                          // PHP true
    emitter.instruction("add rsp, 8");                                          // undo the alignment padding
    emitter.instruction("pop rbx");                                             // restore rbx
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return
}

/// Emits the x86_64 `__rt_date_default_timezone_get` (System V): returns the stored id
/// (`_php_tz_env+3` / `_php_default_tz_len`) in `rax`/`rdx`, or the literal `"UTC"`.
fn emit_get_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: date_default_timezone_get ---");
    emitter.label_global("__rt_date_default_timezone_get");

    emit_symbol_address(emitter, "rsi", "_php_default_tz_len");
    emitter.instruction("mov rdx, QWORD PTR [rsi]");                            // load the stored identifier length
    emitter.instruction("test rdx, rdx");                                       // any zone set?
    emitter.instruction("jz __rt_ddtz_get_x86_default");                        // none → default to "UTC"
    emit_symbol_address(emitter, "rax", "_php_tz_env");
    emitter.instruction("add rax, 3");                                          // skip the "TZ=" prefix → identifier ptr
    emitter.instruction("ret");                                                 // return ptr in rax, len in rdx
    emitter.label("__rt_ddtz_get_x86_default");
    emit_symbol_address(emitter, "rax", "_php_tz_utc");
    emitter.instruction("mov rdx, 3");                                          // length of "UTC"
    emitter.instruction("ret");                                                 // return the default zone
}

/// Emits the AArch64 `__rt_tz_init_utc`: if no default timezone has been configured yet,
/// applies `"TZ=UTC"` via libc `putenv`+`tzset` and records length 3. Self-guarding and
/// idempotent, so the date helpers can call it unconditionally on entry; a later
/// `date_default_timezone_set` overrides it. Makes the default zone UTC like PHP.
fn emit_tz_init_utc_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: tz_init_utc (default zone = UTC until date_default_timezone_set, like PHP) ---");
    emitter.label_global("__rt_tz_init_utc");

    // -- skip if a zone is already configured (user set, or we already defaulted) --
    emit_symbol_address(emitter, "x3", "_php_default_tz_len");
    emitter.instruction("ldr x2, [x3]");                                        // load the configured default-timezone length
    emitter.instruction("cbnz x2, __rt_tz_init_utc_done");                      // already configured → leave libc's TZ as-is

    // -- frame for the libc calls --
    emitter.instruction("sub sp, sp, #16");                                     // allocate a 16-aligned frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // set the frame pointer

    // -- write "TZ=UTC\0" into the static env buffer --
    emit_symbol_address(emitter, "x3", "_php_tz_env");
    emitter.instruction("mov w4, #84");                                         // 'T'
    emitter.instruction("strb w4, [x3]");                                       // _php_tz_env[0] = 'T'
    emitter.instruction("mov w4, #90");                                         // 'Z'
    emitter.instruction("strb w4, [x3, #1]");                                   // _php_tz_env[1] = 'Z'
    emitter.instruction("mov w4, #61");                                         // '='
    emitter.instruction("strb w4, [x3, #2]");                                   // _php_tz_env[2] = '='
    emitter.instruction("mov w4, #85");                                         // 'U'
    emitter.instruction("strb w4, [x3, #3]");                                   // _php_tz_env[3] = 'U'
    emitter.instruction("mov w4, #84");                                         // 'T'
    emitter.instruction("strb w4, [x3, #4]");                                   // _php_tz_env[4] = 'T'
    emitter.instruction("mov w4, #67");                                         // 'C'
    emitter.instruction("strb w4, [x3, #5]");                                   // _php_tz_env[5] = 'C'
    emitter.instruction("strb wzr, [x3, #6]");                                  // NUL-terminate "TZ=UTC"

    // -- record length 3 (reports "UTC" and marks the default as initialised) --
    emit_symbol_address(emitter, "x3", "_php_default_tz_len");
    emitter.instruction("mov x4, #3");                                          // length of "UTC"
    emitter.instruction("str x4, [x3]");                                        // _php_default_tz_len = 3

    // -- apply via libc --
    emit_symbol_address(emitter, "x0", "_php_tz_env");
    emitter.bl_c("putenv");                                                     // putenv("TZ=UTC")
    emitter.bl_c("tzset");                                                      // re-read TZ so localtime resolves UTC
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate the frame
    emitter.label("__rt_tz_init_utc_done");
    emitter.instruction("ret");                                                 // return
}

/// Emits the x86_64 `__rt_tz_init_utc` (System V): mirrors the AArch64 helper — if no default
/// timezone is configured, applies `"TZ=UTC"` via libc `putenv`+`tzset` and records length 3.
/// Self-guarding/idempotent; called on entry by the local-timezone date helpers.
fn emit_tz_init_utc_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: tz_init_utc (default zone = UTC until date_default_timezone_set, like PHP) ---");
    emitter.label_global("__rt_tz_init_utc");

    // -- skip if a zone is already configured (user set, or we already defaulted) --
    emit_symbol_address(emitter, "rsi", "_php_default_tz_len");
    emitter.instruction("mov rax, QWORD PTR [rsi]");                            // load the configured default-timezone length
    emitter.instruction("test rax, rax");                                       // a zone already configured?
    emitter.instruction("jnz __rt_tz_init_utc_done");                           // yes → leave libc's TZ as-is

    // -- frame; keep rsp 16-aligned for the libc calls --
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame pointer
    emitter.instruction("sub rsp, 16");                                         // 16-byte align rsp for the libc calls

    // -- write "TZ=UTC\0" into the static env buffer --
    emit_symbol_address(emitter, "rsi", "_php_tz_env");
    emitter.instruction("mov BYTE PTR [rsi], 84");                              // _php_tz_env[0] = 'T'
    emitter.instruction("mov BYTE PTR [rsi + 1], 90");                          // _php_tz_env[1] = 'Z'
    emitter.instruction("mov BYTE PTR [rsi + 2], 61");                          // _php_tz_env[2] = '='
    emitter.instruction("mov BYTE PTR [rsi + 3], 85");                          // _php_tz_env[3] = 'U'
    emitter.instruction("mov BYTE PTR [rsi + 4], 84");                          // _php_tz_env[4] = 'T'
    emitter.instruction("mov BYTE PTR [rsi + 5], 67");                          // _php_tz_env[5] = 'C'
    emitter.instruction("mov BYTE PTR [rsi + 6], 0");                           // NUL-terminate "TZ=UTC"

    // -- record length 3 (reports "UTC" and marks the default as initialised) --
    emit_symbol_address(emitter, "rsi", "_php_default_tz_len");
    emitter.instruction("mov QWORD PTR [rsi], 3");                              // _php_default_tz_len = 3

    // -- apply via libc --
    emit_symbol_address(emitter, "rdi", "_php_tz_env");
    emitter.instruction("call putenv");                                         // putenv("TZ=UTC")
    emitter.instruction("call tzset");                                          // re-read TZ so localtime resolves UTC
    emitter.instruction("add rsp, 16");                                         // undo the alignment padding
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.label("__rt_tz_init_utc_done");
    emitter.instruction("ret");                                                 // return
}
