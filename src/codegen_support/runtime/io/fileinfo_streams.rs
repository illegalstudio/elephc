//! Purpose:
//! Exposes narrow C ABI queries for fileinfo's compiler-owned stream state.
//!
//! Called from:
//! - The fileinfo prelude through ordinary EIR extern calls.
//! - The platform runtime emitter on every supported target.
//!
//! Key details:
//! - Wrapper queries read registration slots without opening a path or invoking callbacks.
//! - Network failures retain their cause before descriptor cleanup and notifications.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits C ABI entry points whose names follow the target's external symbol convention.
pub(crate) fn emit_fileinfo_streams(emitter: &mut Emitter) {
    emit_wrapper_query(emitter);
    super::fileinfo_stat::emit_fileinfo_stat(emitter);
    super::fileinfo_cast::emit_fileinfo_cast(emitter);
    // -- reset native and optional TLS diagnostics before the next URL operation --
    let name = emitter.target.extern_symbol("elephc_runtime_stream_reset_error");
    emitter.label_global(&name);
    abi::emit_store_zero_to_symbol(emitter, "_stream_error_code", 0);
    abi::emit_store_zero_to_symbol(emitter, "_stream_error_tls", 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x9", "_elephc_tls_clear_error_fn", 0);
            emitter.instruction("cbz x9, __rt_fileinfo_reset_done");            // skip optional TLS state when the bridge is unavailable
            emitter.instruction("br x9");                                       // tail-call the TLS thread-local diagnostic reset
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_tls_clear_error_fn", 0);
            emitter.instruction("test r9, r9");                                 // determine whether the TLS reset entry is available
            emitter.instruction("jz __rt_fileinfo_reset_done");                 // skip the optional bridge when absent
            emitter.instruction("jmp r9");                                      // tail-call the TLS thread-local diagnostic reset
        }
    }
    emitter.label("__rt_fileinfo_reset_done");
    emitter.instruction("ret");                                                 // return after resetting the next network operation's diagnostic
    // -- expose the captured native errno through a scalar C ABI return --
    let name = emitter.target.extern_symbol("elephc_runtime_stream_error_code");
    emitter.label_global(&name);
    let result = if emitter.target.arch == Arch::AArch64 { "x0" } else { "rax" };
    abi::emit_load_symbol_to_reg(emitter, result, "_stream_error_code", 0);
    emitter.instruction("ret");                                                 // return the captured native errno or resolver sentinel
    // -- borrow TLS detail from its thread-local bridge storage when that bridge failed --
    let name = emitter.target.extern_symbol("elephc_runtime_stream_tls_error");
    emitter.label_global(&name);
    let empty = "__rt_fileinfo_tls_error_empty";
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x9", "_stream_error_tls", 0);
            emitter.instruction(&format!("cbz x9, {empty}"));                   // use an empty detail for non-TLS failures
            abi::emit_load_symbol_to_reg(emitter, "x9", "_elephc_tls_last_error_fn", 0);
            emitter.instruction(&format!("cbz x9, {empty}"));                   // tolerate an unavailable optional TLS bridge
            emitter.instruction("br x9");                                       // tail-call the TLS thread-local error getter through its C ABI
            emitter.label(empty);
            abi::emit_symbol_address(emitter, "x0", "_stream_empty_error");
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r9", "_stream_error_tls", 0);
            emitter.instruction("test r9, r9");                                 // determine whether the failing operation used TLS
            emitter.instruction(&format!("jz {empty}"));                        // use an empty detail for non-TLS failures
            abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_tls_last_error_fn", 0);
            emitter.instruction("test r9, r9");                                 // test whether the optional TLS error getter was published
            emitter.instruction(&format!("jz {empty}"));                        // tolerate an unavailable TLS bridge
            emitter.instruction("jmp r9");                                      // tail-call the thread-local TLS error getter through its C ABI
            emitter.label(empty);
            abi::emit_symbol_address(emitter, "rax", "_stream_empty_error");
        }
    }
    emitter.instruction("ret");                                                 // return a stable empty C string when no TLS detail is available
}

/// Retains a raw failed socket syscall before cleanup overwrites its result register.
pub(super) fn capture_socket_error(emitter: &mut Emitter) {
    // -- normalize the platform's raw syscall failure before close can overwrite it --
    match emitter.target.arch {
        Arch::AArch64 => {
            if emitter.platform.needs_cmp_before_error_branch() {
                emitter.instruction("neg x9, x0");                              // normalize a Linux negative errno into its positive native code
            } else {
                emitter.instruction("mov x9, x0");                              // retain Darwin's positive errno from the failed syscall
            }
            abi::emit_store_reg_to_symbol(emitter, "x9", "_stream_error_code", 0);
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9, rax");                                 // preserve the raw Linux failure result
            emitter.instruction("neg r9");                                      // normalize the negative errno into its native positive code
            abi::emit_store_reg_to_symbol(emitter, "r9", "_stream_error_code", 0);
        }
    }
}

/// Checks an owned registration's explicit byte length before comparing its protocol bytes.
fn emit_wrapper_query(emitter: &mut Emitter) {
    let name = emitter.target.extern_symbol("elephc_runtime_stream_has_wrapper");
    emitter.label_global(&name);
    // -- scan owned wrapper registration names through the target's C ABI --
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- retain the C string and its length across registry comparisons --
            emitter.instruction("sub sp, sp, #48");                             // reserve the query inputs, loop index and caller frame
            emitter.instruction("stp x29, x30, [sp, #32]");                     // preserve the caller frame and return address
            emitter.instruction("str x0, [sp]");                                // retain the caller's NUL-terminated scheme
            emitter.bl_c("strlen");
            emitter.instruction("str x0, [sp, #8]");                            // retain its explicit byte length
            emitter.instruction("str xzr, [sp, #16]");                          // begin at registration slot zero
            emitter.label("__rt_fileinfo_wrapper_scan");
            emitter.instruction("ldr x9, [sp, #16]");                           // load the current registration index
            emitter.instruction("cmp x9, #64");                                 // bound the scan by the registration table's capacity
            emitter.instruction("b.ge __rt_fileinfo_wrapper_missing");          // return false after inspecting every slot
            abi::emit_symbol_address(emitter, "x10", "_user_wrappers");
            emitter.instruction("add x10, x10, x9, lsl #5");                    // locate the 32-byte registration record
            emitter.instruction("ldr x1, [x10]");                               // load its retained protocol bytes
            emitter.instruction("cbz x1, __rt_fileinfo_wrapper_next");          // skip empty registration slots
            emitter.instruction("ldr x2, [sp, #8]");                            // load the caller's scheme length
            emitter.instruction("ldr x11, [x10, #8]");                          // load the registration's explicit byte length
            emitter.instruction("cmp x2, x11");                                 // compare lengths before reading either byte span
            emitter.instruction("b.ne __rt_fileinfo_wrapper_next");             // skip protocols with a different length
            emitter.instruction("ldr x0, [sp]");                                // pass the requested scheme to memcmp
            emitter.bl_c("memcmp");
            emitter.instruction("cbz w0, __rt_fileinfo_wrapper_found");         // return true for a complete byte match
            emitter.label("__rt_fileinfo_wrapper_next");
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the index after the C comparison
            emitter.instruction("add x9, x9, #1");                              // advance to the next registration
            emitter.instruction("str x9, [sp, #16]");                           // retain the next loop index
            emitter.instruction("b __rt_fileinfo_wrapper_scan");                // inspect the next slot
            emitter.label("__rt_fileinfo_wrapper_missing");
            emitter.instruction("mov x0, #0");                                  // return the C false value
            emitter.instruction("b __rt_fileinfo_wrapper_return");              // share the balanced frame cleanup
            emitter.label("__rt_fileinfo_wrapper_found");
            emitter.instruction("mov x0, #1");                                  // return the C true value
            emitter.label("__rt_fileinfo_wrapper_return");
            emitter.instruction("ldp x29, x30, [sp, #32]");                     // restore the caller frame and return address
            emitter.instruction("add sp, sp, #48");                             // release all query scratch storage
        }
        Arch::X86_64 => {
            // -- retain query state in an aligned C ABI frame --
            emitter.instruction("sub rsp, 40");                                 // align the stack while retaining query inputs and index
            emitter.instruction("mov QWORD PTR [rsp], rdi");                    // preserve the caller's scheme pointer
            emitter.bl_c("strlen");
            emitter.instruction("mov QWORD PTR [rsp + 8], rax");                // retain the scheme's explicit byte length
            emitter.instruction("mov QWORD PTR [rsp + 16], 0");                 // begin at registration slot zero
            emitter.label("__rt_fileinfo_wrapper_scan");
            emitter.instruction("mov r9, QWORD PTR [rsp + 16]");                // load the current registration index
            emitter.instruction("cmp r9, 64");                                  // bound the scan by the registration table's capacity
            emitter.instruction("jge __rt_fileinfo_wrapper_missing");           // return false after inspecting every slot
            abi::emit_symbol_address(emitter, "r10", "_user_wrappers");
            emitter.instruction("shl r9, 5");                                   // scale the index by the 32-byte record size
            emitter.instruction("add r10, r9");                                 // locate the current registration
            emitter.instruction("mov rsi, QWORD PTR [r10]");                    // load its retained protocol bytes
            emitter.instruction("test rsi, rsi");                               // distinguish an occupied slot from a hole
            emitter.instruction("jz __rt_fileinfo_wrapper_next");               // skip empty registrations
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // load the requested scheme length
            emitter.instruction("cmp rdx, QWORD PTR [r10 + 8]");                // require a matching registered length before reading bytes
            emitter.instruction("jne __rt_fileinfo_wrapper_next");              // skip protocols with a different length
            emitter.instruction("mov rdi, QWORD PTR [rsp]");                    // pass the requested bytes to memcmp
            emitter.bl_c("memcmp");
            emitter.instruction("test eax, eax");                               // test whether all protocol bytes match
            emitter.instruction("jz __rt_fileinfo_wrapper_found");              // return true for a complete match
            emitter.label("__rt_fileinfo_wrapper_next");
            emitter.instruction("inc QWORD PTR [rsp + 16]");                    // advance the retained registration index
            emitter.instruction("jmp __rt_fileinfo_wrapper_scan");              // inspect the next slot
            emitter.label("__rt_fileinfo_wrapper_missing");
            emitter.instruction("xor eax, eax");                                // return the C false value
            emitter.instruction("jmp __rt_fileinfo_wrapper_return");            // share the balanced frame cleanup
            emitter.label("__rt_fileinfo_wrapper_found");
            emitter.instruction("mov eax, 1");                                  // return the C true value
            emitter.label("__rt_fileinfo_wrapper_return");
            emitter.instruction("add rsp, 40");                                 // release the aligned query frame
        }
    }
    emitter.instruction("ret");                                                 // return the registration result through the C ABI
}
