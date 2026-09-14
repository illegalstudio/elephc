//! Purpose:
//! Connects native response events to the shared mbstring MIME metadata owner.
//!
//! Called from:
//! - Header and terminal stdout emitters when the mbstring bridge is selected.
//!
//! Key details:
//! - Protected output-header callbacks return status before Rust can observe a PHP unwind.
//! - Accepted header bytes borrow the bridge result until the web sink returns.
//! - All supported targets share the same C metadata and result layouts.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use elephc_builtin_contract::mbstring_abi::RESULT_STRING;

#[cfg(test)]
mod tests;

/// Emits native header propagation, the protected output callback, and their shared status entry.
pub(super) fn emit_header(emitter: &mut Emitter, web: bool) {
    emitter.label_global("__rt_header");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("stp x29, x30, [sp, #-16]!");                       // retain the native caller until the protected response operation returns
        emitter.instruction("mov x29, sp");                                     // establish aligned native linkage
        emitter.instruction("mov x4, #0");                                      // identify ordinary header diagnostics
        emitter.instruction("bl __rt_mbstring_header_status");                  // finish Rust ownership and protected callbacks before any unwind
        emitter.instruction("ldp x29, x30, [sp], #16");                         // restore native linkage before selecting the final status
        emitter.instruction("cbz x0, __rt_mbstring_header_done");               // header is void after normal acceptance or refusal
        emitter.instruction("cmp x0, #2");                                      // distinguish a pending throwable from a fatal protocol failure
        emitter.instruction("b.ne __rt_mbstring_header_fatal");                 // select fatal handling through a local conditional branch on every target
        emitter.instruction("b __rt_throw_current");                            // unwind only after the Rust header operation has returned
        emitter.label("__rt_mbstring_header_fatal");
        emitter.instruction("mov x0, #1");                                      // preserve a nonzero exit status for fatal host failures
        emitter.bl_c("exit");
        emitter.label("__rt_mbstring_header_done");
        emitter.instruction("ret");                                             // return without an owned PHP value
        emitter.label_global("__rt_mbstring_output_header");
        emitter.instruction("mov x0, x1");                                      // discard the metadata context and select the borrowed header pointer
        emitter.instruction("mov x1, x2");                                      // preserve the exact header byte length
        emitter.instruction("mov x2, #0");                                      // mb_output_handler adds without replacing the host header list
        emitter.instruction("mov x3, #0");                                      // output conversion does not request a response status code
        emitter.instruction("mov x4, #1");                                      // label protected diagnostics with the output-handler caller
        emitter.instruction("b __rt_mbstring_header_status");                   // return raw status to Rust without native unwinding
        aarch64(emitter, web);
    } else {
        emitter.instruction("push rbp");                                        // preserve native linkage and align the nested C call
        emitter.instruction("mov rbp, rsp");                                    // establish stable wrapper linkage
        emitter.instruction("xor r8d, r8d");                                    // identify ordinary header diagnostics
        emitter.instruction("call __rt_mbstring_header_status");                // retire Rust result ownership before any native unwind
        emitter.instruction("pop rbp");                                         // restore caller linkage before interpreting protected status
        emitter.instruction("test eax, eax");                                   // normal acceptance and ordinary refusal both return void
        emitter.instruction("jz __rt_mbstring_header_done");                    // return after successful protected execution
        emitter.instruction("cmp eax, 2");                                      // identify a pending PHP throwable
        emitter.instruction("je __rt_throw_current");                           // propagate only after Rust has returned
        emitter.instruction("mov edi, 1");                                      // preserve nonzero process status for a fatal host protocol failure
        emitter.instruction("jmp __rt_mbstring_header_fatal");                  // align the terminal exit call separately from ordinary return
        emitter.label("__rt_mbstring_header_done");
        emitter.instruction("ret");                                             // header transfers no PHP value
        emitter.label("__rt_mbstring_header_fatal");
        emitter.instruction("sub rsp, 8");                                      // align the C stack after restoring native caller linkage
        emitter.bl_c("exit");
        emitter.label_global("__rt_mbstring_output_header");
        emitter.instruction("mov rdi, rsi");                                    // discard metadata context and select the borrowed header pointer
        emitter.instruction("mov rsi, rdx");                                    // retain the exact header length
        emitter.instruction("xor edx, edx");                                    // preserve mb_output_handler's nonreplacement header mode
        emitter.instruction("xor ecx, ecx");                                    // leave the response status unchanged
        emitter.instruction("mov r8d, 1");                                      // use the output-handler diagnostic origin
        emitter.instruction("jmp __rt_mbstring_header_status");                 // return protected raw status without unwinding through Rust
        x86_64(emitter, web);
    }
}

/// Builds the AArch64 diagnostic table and borrows normalized header bytes through web publication.
fn aarch64(emitter: &mut Emitter, web: bool) {
    emitter.label_global("__rt_mbstring_header_status");
    emitter.instruction("sub sp, sp, #128");                                    // reserve the diagnostic table, result, saved header options, status, and linkage
    emitter.instruction("stp x29, x30, [sp, #112]");                            // retain the native or Rust caller across protected callbacks
    emitter.instruction("add x29, sp, #112");                                   // establish aligned status-entry linkage
    emitter.instruction("stp x2, x3, [sp, #72]");                               // retain replacement mode and response code for the web sink
    emitter.instruction("mov x2, x4");                                          // pass the caller's diagnostic origin as the third Rust argument
    abi::emit_load_int_immediate(emitter, "x9", (24_i64 << 32) | 1);
    emitter.instruction("stp x9, xzr, [sp]");                                   // publish a version-one diagnostic table with no eval context
    abi::emit_symbol_address(emitter, "x9", "__rt_mbstring_diagnostic");
    emitter.instruction("str x9, [sp, #16]");                                   // install the existing protected diagnostic callback
    emitter.instruction("mov x3, sp");                                          // borrow the complete diagnostic host table
    emitter.instruction("add x4, sp, #24");                                     // provide separate bridge-owned result storage
    emitter.bl_c("elephc_mbstring_response_header_v1");
    emitter.instruction("str x0, [sp, #88]");                                   // retain the protected status through result release
    if web {
        emitter.instruction("cbnz x0, __rt_mbstring_header_release");           // suppress transport publication after a pending or fatal failure
        emitter.instruction("ldr x9, [sp, #24]");                               // distinguish accepted normalized bytes from ordinary rejection
        emitter.instruction(&format!("cmp x9, #{RESULT_STRING}"));              // only successful string results carry a header to publish
        emitter.instruction("b.ne __rt_mbstring_header_release");               // leave rejected headers out of the web response
        emitter.instruction("ldp x0, x1, [sp, #40]");                           // borrow the bridge-owned wire header and exact byte length
        emitter.instruction("ldp x2, x3, [sp, #72]");                           // restore original replacement and response-code options
        emitter.bl_c("elephc_web_header");
    }
    emitter.label("__rt_mbstring_header_release");
    emitter.instruction("add x0, sp, #24");                                     // release the result through its original Rust allocator
    emitter.bl_c("elephc_mbstring_release_v1");
    emitter.instruction("ldr x0, [sp, #88]");                                   // return protected runtime status to the native wrapper or Rust callback
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore caller linkage after every borrowed header byte is retired
    emitter.instruction("add sp, sp, #128");                                    // release invocation-local metadata and result storage
    emitter.instruction("ret");                                                 // return without a PHP unwind
}

/// Builds the SysV status entry with balanced bridge ownership and C-call stack alignment.
fn x86_64(emitter: &mut Emitter, web: bool) {
    emitter.label_global("__rt_mbstring_header_status");
    emitter.instruction("push rbp");                                            // align the stack while retaining the caller frame
    emitter.instruction("mov rbp, rsp");                                        // establish stable status-entry linkage
    emitter.instruction("sub rsp, 96");                                         // reserve the table, result, original header options, and status
    emitter.instruction("mov QWORD PTR [rsp + 72], rdx");                       // retain replacement policy for accepted web headers
    emitter.instruction("mov QWORD PTR [rsp + 80], rcx");                       // retain the caller's explicit response code
    emitter.instruction("mov edx, r8d");                                        // select the third Rust argument's diagnostic origin
    abi::emit_load_int_immediate(emitter, "r10", (24_i64 << 32) | 1);
    emitter.instruction("mov QWORD PTR [rsp], r10");                            // publish the exact diagnostic table version and size
    emitter.instruction("mov QWORD PTR [rsp + 8], 0");                          // diagnostics use no eval context in this response adapter
    abi::emit_symbol_address(emitter, "r10", "__rt_mbstring_diagnostic");
    emitter.instruction("mov QWORD PTR [rsp + 16], r10");                       // install the existing protected warning callback
    emitter.instruction("mov rcx, rsp");                                        // borrow the diagnostic host for this complete Rust call
    emitter.instruction("lea r8, [rsp + 24]");                                  // provide disjoint owned result storage
    emitter.bl_c("elephc_mbstring_response_header_v1");
    emitter.instruction("mov DWORD PTR [rsp + 88], eax");                       // retain callback status until all bridge buffers are released
    if web {
        emitter.instruction("test eax, eax");                                   // suppress transport headers after a protected callback failure
        emitter.instruction("jnz __rt_mbstring_header_release");                // release the empty failed result before returning status
        emitter.instruction(&format!("cmp QWORD PTR [rsp + 24], {RESULT_STRING}")); // accepted headers carry an owned byte string
        emitter.instruction("jne __rt_mbstring_header_release");                // ordinary rejection publishes no wire header
        emitter.instruction("mov rdi, QWORD PTR [rsp + 40]");                   // borrow the Rust result's normalized header bytes
        emitter.instruction("mov rsi, QWORD PTR [rsp + 48]");                   // retain the exact binary header length
        emitter.instruction("mov rdx, QWORD PTR [rsp + 72]");                   // restore the caller's replacement mode
        emitter.instruction("mov rcx, QWORD PTR [rsp + 80]");                   // restore its explicit response code
        emitter.bl_c("elephc_web_header");
    }
    emitter.label("__rt_mbstring_header_release");
    emitter.instruction("lea rdi, [rsp + 24]");                                 // retire every result through the original bridge allocator
    emitter.bl_c("elephc_mbstring_release_v1");
    emitter.instruction("mov eax, DWORD PTR [rsp + 88]");                       // return raw status after all ownership has ended
    emitter.instruction("leave");                                               // release status storage and restore native or Rust caller linkage
    emitter.instruction("ret");                                                 // return without unwinding through Rust
}

/// Commits only terminal writes while preserving the pointer/length pair for the actual sink.
pub(super) fn commit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("stp x0, x1, [sp, #-16]!");                         // retain the terminal byte span across response-state commitment
        emitter.instruction("mov x0, x1");                                      // a zero byte count must not freeze response headers
        emitter.bl_c("elephc_mbstring_response_commit_v1");
        emitter.instruction("cbnz x0, __rt_stdout_response_fatal");             // stop after an invalid request-state boundary
        emitter.instruction("ldp x0, x1, [sp], #16");                           // restore the original terminal span for web capture or the syscall
        emitter.instruction("b __rt_stdout_response_ready");                    // continue normal sink selection after successful commitment
        emitter.label("__rt_stdout_response_fatal");
        emitter.instruction("mov x0, #1");                                      // preserve a nonzero exit status for a fatal bridge failure
        emitter.bl_c("exit");
        emitter.instruction("brk #0");                                          // trap if the terminal process exit unexpectedly returns
    } else {
        emitter.instruction("sub rsp, 16");                                     // reserve aligned saved argument slots across response commitment
        emitter.instruction("mov QWORD PTR [rsp], rdi");                        // retain the original output pointer
        emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                    // retain its exact byte length
        emitter.instruction("mov rdi, rsi");                                    // commit only when a nonempty span reaches the terminal sink
        emitter.bl_c("elephc_mbstring_response_commit_v1");
        emitter.instruction("test eax, eax");                                   // detect a fatal request-state boundary before actual output
        emitter.instruction("jnz __rt_stdout_response_fatal");                  // preserve the bridge failure rather than silently writing
        emitter.instruction("mov rdi, QWORD PTR [rsp]");                        // restore the original terminal output pointer
        emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                    // restore the original terminal output length
        emitter.instruction("add rsp, 16");                                     // retire only the temporary response-commit argument storage
        emitter.instruction("jmp __rt_stdout_response_ready");                  // continue the existing web or syscall output path
        emitter.label("__rt_stdout_response_fatal");
        emitter.instruction("mov edi, 1");                                      // preserve nonzero process status for a fatal host boundary
        emitter.bl_c("exit");
        emitter.instruction("ud2");                                             // trap if the terminal process exit unexpectedly returns
    }
    emitter.label("__rt_stdout_response_ready");
}
