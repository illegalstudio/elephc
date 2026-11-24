//! Purpose:
//! Emits `__rt_preg_compile`, `__rt_preg_match_compiled` and `__rt_preg_free`: a PCRE2
//! pattern compiled once and matched against many subjects, for the `preg_grep()` prelude.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` with the regex runtime.
//!
//! Key details:
//! - `__rt_preg_compile` strips the delimiters, rejects the patterns PHP rejects before compiling, and
//!   compiles through `elephc_pcre2_v1_compile`, returning the opaque shim handle or `0`.
//! - `__rt_preg_match_compiled` runs one exec through `__rt_preg_exec`, which records the
//!   call's own match error; a match or a failed match leaves `preg_last_error()` alone.
//!   It returns `1` (match), `0` (no match) or `-1` (match error).
//! - The handle stays owned by the caller until `__rt_preg_free`.

use elephc_builtin_contract::pcre_errors::PCRE2_SHIM_NOMATCH;

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the three compiled-pattern helpers for the current target.
pub(crate) fn emit_preg_compiled(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_compile_aarch64(emitter);
            emit_match_aarch64(emitter);
            emit_free_aarch64(emitter);
        }
        Arch::X86_64 => {
            emit_compile_x86_64(emitter);
            emit_match_x86_64(emitter);
            emit_free_x86_64(emitter);
        }
    }
}

/// `__rt_preg_compile`: input `x1`/`x2` = pattern; output `x0` = handle, or `0` on failure.
fn emit_compile_aarch64(emitter: &mut Emitter) {
    let handle_off = 0;
    let slot_count_off = 8;
    let flags_off = 16;
    let stack_size = 48;
    let save_off = 32;
    emitter.blank();
    emitter.comment("--- runtime: preg_compile ---");
    emitter.label_global("__rt_preg_compile");
    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{stack_size}"));                 // allocate the handle, slot-count and flags spill slots
    emitter.instruction(&format!("stp x29, x30, [sp, #{save_off}]"));           // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{save_off}"));                  // set new frame pointer
    // -- strip delimiters and materialize the PCRE pattern --
    emitter.instruction("bl __rt_preg_strip");                                  // → x1=stripped, x2=len, x3=flags
    emitter.instruction("tbnz x3, #30, __rt_preg_compile_fail");                // PHP rejects this pattern before compiling it
    emitter.instruction(&format!("str x3, [sp, #{flags_off}]"));                // save the compile flags across the C-string helper
    emitter.instruction("bl __rt_pcre_to_posix");                               // materialize the PCRE pattern as a C string
    super::emit_prepare_regex_locale(emitter);
    // -- compile through the opaque Elephc shim --
    emitter.instruction("mov x1, x0");                                          // pass the null-terminated pattern
    emitter.instruction(&format!("add x0, sp, #{handle_off}"));                 // pass opaque-handle output storage
    emitter.instruction(&format!("ldr x2, [sp, #{flags_off}]"));                // pass PCRE2 POSIX compile flags from delimiter parsing
    emitter.instruction(&format!("add x3, sp, #{slot_count_off}"));             // receive the compiled match-slot count (unused)
    emitter.bl_c("elephc_pcre2_v1_compile");                                    // compile without exposing PCRE2-owned layouts
    emitter.instruction("cbnz x0, __rt_preg_compile_fail");                     // a compile error returns the null handle
    emitter.instruction(&format!("ldr x0, [sp, #{handle_off}]"));               // return the compiled opaque handle
    emitter.instruction("b __rt_preg_compile_ret");                             // skip the failure path
    emitter.label("__rt_preg_compile_fail");
    emitter.instruction("mov x0, #0");                                          // the null handle marks an invalid pattern
    emitter.label("__rt_preg_compile_ret");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{save_off}]"));           // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{stack_size}"));                 // deallocate stack frame
    emitter.instruction("ret");                                                 // return the handle in x0
}

/// `__rt_preg_match_compiled`: input `x0` = handle, `x1`/`x2` = subject; output `x0` = 1/0/-1.
fn emit_match_aarch64(emitter: &mut Emitter) {
    let handle_off = 0;
    let pair_off = 8;
    let stack_size = 48;
    let save_off = 32;
    emitter.blank();
    emitter.comment("--- runtime: preg_match_compiled ---");
    emitter.label_global("__rt_preg_match_compiled");
    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{stack_size}"));                 // allocate the handle and offset-pair slots
    emitter.instruction(&format!("stp x29, x30, [sp, #{save_off}]"));           // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{save_off}"));                  // set new frame pointer
    emitter.instruction(&format!("str x0, [sp, #{handle_off}]"));               // keep the handle across the C-string helper
    // -- execute one match with the compiled handle --
    emitter.instruction("bl __rt_cstr2");                                       // → x0=null-terminated subject copy
    emitter.instruction("mov x1, x0");                                          // pass the null-terminated subject
    emitter.instruction(&format!("ldr x0, [sp, #{handle_off}]"));               // pass the compiled opaque handle
    emitter.instruction("mov x2, #1");                                          // request only the full-match pair
    emitter.instruction(&format!("add x3, sp, #{pair_off}"));                   // receive one fixed signed-64-bit offset pair
    emitter.instruction("mov x4, #0");                                          // use default execution flags
    super::preg_error_state::emit_exec_call(emitter);                           // execute and record this call's own match error
    // -- classify the shim status --
    emitter.instruction("cbz w0, __rt_preg_match_compiled_yes");                // status 0 is a match
    emitter.instruction(&format!("cmp w0, #{}", PCRE2_SHIM_NOMATCH));           // is this an ordinary failed match?
    emitter.instruction("b.eq __rt_preg_match_compiled_no");                    // a failed match is not an error
    emitter.instruction("mov x0, #-1");                                         // any other status is a recorded match error
    emitter.instruction("b __rt_preg_match_compiled_ret");                      // return the error sentinel
    emitter.label("__rt_preg_match_compiled_yes");
    emitter.instruction("mov x0, #1");                                          // matched → return 1
    emitter.instruction("b __rt_preg_match_compiled_ret");                      // return the match flag
    emitter.label("__rt_preg_match_compiled_no");
    emitter.instruction("mov x0, #0");                                          // no match → return 0
    emitter.label("__rt_preg_match_compiled_ret");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{save_off}]"));           // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{stack_size}"));                 // deallocate stack frame
    emitter.instruction("ret");                                                 // return the status in x0
}

/// `__rt_preg_free`: input `x0` = handle from `__rt_preg_compile`.
fn emit_free_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_free ---");
    emitter.label_global("__rt_preg_free");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame
    emitter.bl_c("elephc_pcre2_v1_free");                                       // release the compiled regex resources
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return to the caller
}

/// `__rt_preg_compile`: input `rdi`/`rsi` = pattern; output `rax` = handle, or `0` on failure.
fn emit_compile_x86_64(emitter: &mut Emitter) {
    let handle_off = 0;
    let slot_count_off = 8;
    let flags_off = 16;
    let stack_size = 32;
    emitter.blank();
    emitter.comment("--- runtime: preg_compile ---");
    emitter.label_global("__rt_preg_compile");
    // -- set up stack frame --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction(&format!("sub rsp, {stack_size}"));                     // reserve the handle, slot-count and flags spill slots
    // -- strip delimiters and materialize the PCRE pattern --
    emitter.instruction("mov rax, rdi");                                        // move the pattern pointer into the preg-strip input register
    emitter.instruction("mov rdx, rsi");                                        // move the pattern length into the preg-strip input register
    emitter.instruction("call __rt_preg_strip");                                // strip delimiters and collect the regex flags
    emitter.instruction("bt rcx, 30");                                          // did delimiter parsing reject the pattern?
    emitter.instruction("jc __rt_preg_compile_fail");                           // PHP rejects this pattern before compiling it
    emitter.instruction(&format!("mov QWORD PTR [rsp + {flags_off}], rcx"));    // save the compile flags across the C-string helper
    emitter.instruction("call __rt_pcre_to_posix");                             // materialize the PCRE pattern as a C string
    super::emit_prepare_regex_locale(emitter);
    // -- compile through the opaque Elephc shim --
    emitter.instruction("mov rsi, rax");                                        // pass the null-terminated pattern
    emitter.instruction(&format!("lea rdi, [rsp + {handle_off}]"));             // pass opaque-handle output storage
    emitter.instruction(&format!("mov edx, DWORD PTR [rsp + {flags_off}]"));    // pass PCRE2 POSIX compile flags from delimiter parsing
    emitter.instruction(&format!("lea rcx, [rsp + {slot_count_off}]"));         // receive the compiled match-slot count (unused)
    emitter.bl_c("elephc_pcre2_v1_compile");                                    // compile without exposing PCRE2-owned layouts
    emitter.instruction("test eax, eax");                                       // did the shim compile the pattern?
    emitter.instruction("jnz __rt_preg_compile_fail");                          // a compile error returns the null handle
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {handle_off}]"));   // return the compiled opaque handle
    emitter.instruction("jmp __rt_preg_compile_ret");                           // skip the failure path
    emitter.label("__rt_preg_compile_fail");
    emitter.instruction("xor eax, eax");                                        // the null handle marks an invalid pattern
    emitter.label("__rt_preg_compile_ret");
    emitter.instruction(&format!("add rsp, {stack_size}"));                     // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the handle in rax
}

/// `__rt_preg_match_compiled`: input `rdi` = handle, `rsi`/`rdx` = subject; output `rax` = 1/0/-1.
fn emit_match_x86_64(emitter: &mut Emitter) {
    let handle_off = 0;
    let pair_off = 8;
    let stack_size = 32;
    emitter.blank();
    emitter.comment("--- runtime: preg_match_compiled ---");
    emitter.label_global("__rt_preg_match_compiled");
    // -- set up stack frame --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction(&format!("sub rsp, {stack_size}"));                     // reserve the handle and offset-pair slots
    emitter.instruction(&format!("mov QWORD PTR [rsp + {handle_off}], rdi"));   // keep the handle across the C-string helper
    // -- execute one match with the compiled handle --
    emitter.instruction("mov rax, rsi");                                        // move the subject pointer into the C-string helper input
    emitter.instruction("call __rt_cstr2");                                     // → rax=null-terminated subject copy (length already in rdx)
    emitter.instruction("mov rsi, rax");                                        // pass the null-terminated subject
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {handle_off}]"));   // pass the compiled opaque handle
    emitter.instruction("mov edx, 1");                                          // request only the full-match pair
    emitter.instruction(&format!("lea rcx, [rsp + {pair_off}]"));               // receive one fixed signed-64-bit offset pair
    emitter.instruction("xor r8d, r8d");                                        // use default execution flags
    super::preg_error_state::emit_exec_call(emitter);                           // execute and record this call's own match error
    // -- classify the shim status --
    emitter.instruction("test eax, eax");                                       // is the status 0 (a match)?
    emitter.instruction("jz __rt_preg_match_compiled_yes");                     // status 0 is a match
    emitter.instruction(&format!("cmp eax, {}", PCRE2_SHIM_NOMATCH));           // is this an ordinary failed match?
    emitter.instruction("je __rt_preg_match_compiled_no");                      // a failed match is not an error
    emitter.instruction("mov rax, -1");                                         // any other status is a recorded match error
    emitter.instruction("jmp __rt_preg_match_compiled_ret");                    // return the error sentinel
    emitter.label("__rt_preg_match_compiled_yes");
    emitter.instruction("mov eax, 1");                                          // matched → return 1
    emitter.instruction("jmp __rt_preg_match_compiled_ret");                    // return the match flag
    emitter.label("__rt_preg_match_compiled_no");
    emitter.instruction("xor eax, eax");                                        // no match → return 0
    emitter.label("__rt_preg_match_compiled_ret");
    emitter.instruction(&format!("add rsp, {stack_size}"));                     // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the status in rax
}

/// `__rt_preg_free`: input `rdi` = handle from `__rt_preg_compile`.
fn emit_free_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_free ---");
    emitter.label_global("__rt_preg_free");
    emitter.instruction("push rbp");                                            // keep the C call 16-byte aligned
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.bl_c("elephc_pcre2_v1_free");                                       // release the compiled regex resources
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
