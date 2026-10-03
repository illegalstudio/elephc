//! Purpose:
//! Emits the `__rt_date_default_timezone_set` / `__rt_date_default_timezone_get` runtime helpers.
//! These back PHP's default-timezone API by driving libc's own timezone machinery.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//!
//! Key details:
//! - `_set` writes `"TZ=<id>"` into the static `_php_tz_env` buffer, calls libc `putenv` + `tzset`,
//!   and records the identifier length in `_php_default_tz_len`. libc then resolves the zone
//!   (offsets + DST) from the system tz database for every later `localtime` (so `date()` becomes
//!   timezone-aware with no embedded tzdata). `_get` returns the stored identifier, or `"UTC"`.
//! - String result convention: pointer in `x1`/`rax`, length in `x2`/`rdx`. `_set` returns the PHP
//!   boolean in `x0`/`rax`. The `_php_tz_env` buffer is static so the pointer handed to `putenv`
//!   stays valid (putenv does not copy).

use crate::codegen_support::abi::{emit_load_symbol_to_reg, emit_symbol_address};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

const INVALID_TIMEZONE_PREFIX: &str = "Warning: date_default_timezone_set(): Timezone ID '";
const INVALID_TIMEZONE_SUFFIX: &str = "' is invalid\n";

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

/// Emits the AArch64 setter transaction: validate first, then mutate libc and
/// the stored identifier together. A rejected identifier never changes either.
fn emit_set_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: date_default_timezone_set ---");
    emitter.label_global("__rt_date_default_timezone_set");

    // -- retain the complete input across validation, diagnostics, and libc --
    emitter.instruction("sub sp, sp, #48");                                     // allocate a 16-aligned callee-saved frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set the frame pointer
    emitter.instruction("stp x19, x20, [sp, #16]");                             // preserve identifier pointer and length registers
    emitter.instruction("mov x19, x1");                                         // retain identifier bytes across bridge calls
    emitter.instruction("mov x20, x2");                                         // retain the exact identifier length across bridge calls

    // -- validate before any persistent state can change --
    emit_load_symbol_to_reg(emitter, "x9", "_elephc_tz_validate_fn", 0);
    emitter.instruction("cbz x9, __rt_ddtz_set_invalid");                       // an unpublished validator safely rejects the call
    emitter.instruction("mov x0, x19");                                         // C ABI arg 1 = identifier bytes
    emitter.instruction("mov x1, x20");                                         // C ABI arg 2 = exact identifier length
    emitter.instruction("blr x9");                                              // elephc_tz_is_valid(ptr, len) -> nonzero when PHP accepts it
    emitter.instruction("cbz x0, __rt_ddtz_set_invalid");                       // invalid identifiers keep the old timezone untouched
    emitter.instruction("mov x4, #250");                                        // static environment-buffer capacity after TZ=
    emitter.instruction("cmp x20, x4");                                         // validated length vs capacity
    emitter.instruction("b.hi __rt_ddtz_set_invalid");                          // defensive rejection preserves atomicity

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
    emitter.instruction("cmp x5, x20");                                         // all identifier bytes copied?
    emitter.instruction("b.ge __rt_ddtz_set_copy_done");                        // yes → terminate
    emitter.instruction("ldrb w7, [x19, x5]");                                  // load retained identifier byte
    emitter.instruction("strb w7, [x6, x5]");                                   // store it after the prefix
    emitter.instruction("add x5, x5, #1");                                      // advance the copy index
    emitter.instruction("b __rt_ddtz_set_copy");                                // continue copying
    emitter.label("__rt_ddtz_set_copy_done");
    emitter.instruction("strb wzr, [x6, x20]");                                 // NUL-terminate the env string

    // -- apply via libc and re-read the zone --
    emit_symbol_address(emitter, "x0", "_php_tz_env");
    emitter.emit_call_c("putenv");                                                     // putenv("TZ=<id>")
    emitter.emit_call_c("tzset");                                                      // re-read TZ so localtime uses it

    // -- record the identifier length for date_default_timezone_get and return true --
    emit_symbol_address(emitter, "x3", "_php_default_tz_len");
    emitter.instruction("str x20, [x3]");                                       // _php_default_tz_len = validated identifier length
    emitter.instruction("mov x0, #1");                                          // PHP true
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore callee-saved input registers
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate the frame
    emitter.instruction("ret");                                                 // return successful PHP boolean

    emitter.label("__rt_ddtz_set_invalid");
    emit_symbol_address(emitter, "x1", "_date_default_timezone_invalid_prefix");
    emitter.instruction(&format!("mov x2, #{}", INVALID_TIMEZONE_PREFIX.len())); // diagnostic prefix length
    emitter.instruction("bl __rt_diag_warning");                                // emit or suppress the PHP warning prefix
    emitter.instruction("mov x1, x19");                                         // warning includes the rejected identifier bytes
    emitter.instruction("mov x2, x20");                                         // warning includes the complete rejected identifier length
    emitter.instruction("bl __rt_diag_warning");                                // emit or suppress the PHP warning identifier
    emit_symbol_address(emitter, "x1", "_date_default_timezone_invalid_suffix");
    emitter.instruction(&format!("mov x2, #{}", INVALID_TIMEZONE_SUFFIX.len())); // diagnostic suffix length
    emitter.instruction("bl __rt_diag_warning");                                // emit or suppress the PHP warning suffix
    emitter.instruction("mov x0, #0");                                          // PHP false after a transactional rejection
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore callee-saved input registers
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate the frame
    emitter.instruction("ret");                                                 // return false
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

/// Emits the x86_64 setter transaction. The indirect native bridge call keeps
/// Windows on its MS x64 ABI while Unix retains its normal System V call.
fn emit_set_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: date_default_timezone_set ---");
    emitter.label_global("__rt_date_default_timezone_set");

    // -- retain the complete input across validation, diagnostics, and libc --
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame pointer
    emitter.instruction("push rbx");                                            // preserve identifier length register
    emitter.instruction("push r12");                                            // preserve identifier pointer register; stack remains call-aligned
    emitter.instruction("mov r12, rax");                                        // retain identifier bytes across bridge calls
    emitter.instruction("mov rbx, rdx");                                        // retain the complete identifier length across bridge calls

    // -- validate before any persistent state can change --
    emit_load_symbol_to_reg(emitter, "r9", "_elephc_tz_validate_fn", 0);
    emitter.instruction("test r9, r9");                                         // was the managed validator published at the call site?
    emitter.instruction("jz __rt_ddtz_set_x86_invalid");                        // no -> reject without changing the old zone
    emitter.instruction("mov rdi, r12");                                        // C ABI arg 1 = identifier bytes
    emitter.instruction("mov rsi, rbx");                                        // C ABI arg 2 = exact identifier length
    emitter.emit_native_bridge_call("r9", 2);                                   // elephc_tz_is_valid(ptr, len) -> nonzero when PHP accepts it
    emitter.instruction("test eax, eax");                                       // bridge result is a C int boolean
    emitter.instruction("jz __rt_ddtz_set_x86_invalid");                        // invalid identifiers keep the old timezone untouched
    emitter.instruction("cmp rbx, 250");                                        // validated length vs environment buffer capacity
    emitter.instruction("ja __rt_ddtz_set_x86_invalid");                        // defensive rejection preserves transaction atomicity

    // -- write the "TZ=" prefix into the static env buffer --
    emit_symbol_address(emitter, "rsi", "_php_tz_env");
    emitter.instruction("mov BYTE PTR [rsi], 84");                              // _php_tz_env[0] = 'T'
    emitter.instruction("mov BYTE PTR [rsi + 1], 90");                          // _php_tz_env[1] = 'Z'
    emitter.instruction("mov BYTE PTR [rsi + 2], 61");                          // _php_tz_env[2] = '='

    // -- copy the validated identifier bytes after the prefix --
    emitter.instruction("lea rdi, [rsi + 3]");                                  // destination = buffer + 3
    emitter.instruction("xor rcx, rcx");                                        // copy index
    emitter.label("__rt_ddtz_set_x86_copy");
    emitter.instruction("cmp rcx, rbx");                                        // all identifier bytes copied?
    emitter.instruction("jge __rt_ddtz_set_x86_copy_done");                     // yes → terminate
    emitter.instruction("mov r8b, BYTE PTR [r12 + rcx]");                       // load retained identifier byte
    emitter.instruction("mov BYTE PTR [rdi + rcx], r8b");                       // store it after the prefix
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_ddtz_set_x86_copy");                          // continue copying
    emitter.label("__rt_ddtz_set_x86_copy_done");
    emitter.instruction("mov BYTE PTR [rdi + rbx], 0");                         // NUL-terminate the env string

    // -- apply via libc and re-read the zone --
    emit_symbol_address(emitter, "rdi", "_php_tz_env");
    emitter.emit_call_c("putenv");                                              // putenv("TZ=<id>")
    emitter.emit_call_c("tzset");                                               // re-read TZ so localtime uses it

    // -- record the identifier length for date_default_timezone_get and return true --
    emit_symbol_address(emitter, "rsi", "_php_default_tz_len");
    emitter.instruction("mov QWORD PTR [rsi], rbx");                            // _php_default_tz_len = identifier length
    emitter.instruction("mov rax, 1");                                          // PHP true
    emitter.instruction("pop r12");                                             // restore identifier pointer register
    emitter.instruction("pop rbx");                                             // restore rbx
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return successful PHP boolean

    emitter.label("__rt_ddtz_set_x86_invalid");
    emit_symbol_address(emitter, "rdi", "_date_default_timezone_invalid_prefix");
    emitter.instruction(&format!("mov rsi, {}", INVALID_TIMEZONE_PREFIX.len())); // diagnostic prefix length
    emitter.instruction("call __rt_diag_warning");                              // emit or suppress the PHP warning prefix
    emitter.instruction("mov rdi, r12");                                        // warning includes the rejected identifier bytes
    emitter.instruction("mov rsi, rbx");                                        // warning includes the complete rejected identifier length
    emitter.instruction("call __rt_diag_warning");                              // emit or suppress the PHP warning identifier
    emit_symbol_address(emitter, "rdi", "_date_default_timezone_invalid_suffix");
    emitter.instruction(&format!("mov rsi, {}", INVALID_TIMEZONE_SUFFIX.len())); // diagnostic suffix length
    emitter.instruction("call __rt_diag_warning");                              // emit or suppress the PHP warning suffix
    emitter.instruction("xor eax, eax");                                        // PHP false after a transactional rejection
    emitter.instruction("pop r12");                                             // restore identifier pointer register
    emitter.instruction("pop rbx");                                             // restore identifier length register
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return false
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
    emitter.emit_call_c("putenv");                                                     // putenv("TZ=UTC")
    emitter.emit_call_c("tzset");                                                      // re-read TZ so localtime resolves UTC
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
    emitter.emit_call_c("putenv");                                              // putenv("TZ=UTC")
    emitter.emit_call_c("tzset");                                               // re-read TZ so localtime resolves UTC
    emitter.instruction("add rsp, 16");                                         // undo the alignment padding
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.label("__rt_tz_init_utc_done");
    emitter.instruction("ret");                                                 // return
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Guards the emitted default-timezone mutation transaction across the
    //! native AArch64 and Windows x86_64 ABI paths.
    //!
    //! Called from:
    //! - `cargo test --lib date_default_timezone` through Rust's test harness.
    //!
    //! Key details:
    //! - Validation must precede every `_php_tz_env` write, and Windows must
    //!   invoke the Rust bridge through the MS x64 adapter rather than a bare call.

    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Both executable ABI emitters validate before they can write the
    /// persistent TZ buffer, preserving the previous successful zone on error.
    #[test]
    fn setter_validates_before_mutating_timezone_state() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
            Target::new(Platform::Windows, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_date_default_timezone(&mut emitter);
            let asm = emitter.output();
            let validation = asm
                .find("_elephc_tz_validate_fn")
                .expect("setter must load the published timezone validator");
            let mutation = asm
                .find("_php_tz_env")
                .expect("setter must retain the committed timezone buffer");
            assert!(validation < mutation, "validation must precede TZ mutation for {target:?}");
            assert!(asm.contains("_date_default_timezone_invalid_prefix"));
            assert!(asm.contains("_date_default_timezone_invalid_suffix"));
        }
    }

    /// Windows bridge calls need the native MS x64 adapter; Unix keeps its
    /// direct System V call and AArch64 emits the architectural `blr` form.
    #[test]
    fn setter_uses_the_target_correct_validation_call_abi() {
        let mut windows = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_set_x86_64(&mut windows);
        let windows = windows.output();
        assert!(windows.contains("mov r11, r9"));
        assert!(windows.contains("sub rsp, 32"));
        assert!(windows.contains("call r11"));

        let mut arm = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_set_aarch64(&mut arm);
        assert!(arm.output().contains("blr x9"));
    }
}
