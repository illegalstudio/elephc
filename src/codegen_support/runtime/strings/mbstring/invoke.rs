//! Purpose:
//! Connects boxed native/eval calls to the shared mbstring invocation coordinator.
//!
//! Called from:
//! - Boxed builtin dispatch and native argument adapters.
//!
//! Key details:
//! - C inputs are operation, borrowed argument-pointer array, count, strictness, and eval context.
//! - A stack-local callback table retains the correct context through nested calls.
//! - Output conversion also receives a separate response host backed by the shared request state.
//! - Results use the existing native value/status/length/kind convention without unwinding.

use super::*;

#[cfg(test)]
#[path = "invoke/tests.rs"]
mod tests;

pub(super) const CALLBACKS: [&str; 10] = ["__rt_mbstring_clone", "__rt_mbstring_input", "__rt_mbstring_stringable",
    "__rt_mbstring_float", "__rt_mbstring_diagnostic", "__rt_mbstring_release", "__rt_mbstring_array_next", "__rt_mbstring_array_value",
    "__rt_mbstring_graph_value", "__rt_mbstring_pin"];

/// Emits the complete protected coordinator call and result transfer for every supported target.
pub(super) fn emit(emitter: &mut Emitter, mbregex: bool) {
    emitter.label_global("__rt_mbstring_invoke");
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter, mbregex); } else { x86_64(emitter, mbregex); }
}

/// Builds an AArch64 callback table without disturbing the four incoming call arguments.
fn aarch64(emitter: &mut Emitter, mbregex: bool) {
    emitter.instruction("sub sp, sp, #208");                                    // reserve value and response hosts, result, status, and caller linkage
    emitter.instruction("stp x29, x30, [sp, #192]");                            // preserve the native caller across Rust and materialization
    emitter.instruction("add x29, sp, #192");                                   // establish an aligned coordinator frame
    if mbregex { abi::emit_call_label(emitter, "__rt_mbregex_init"); }
    abi::emit_load_int_immediate(emitter, "x9", (96_i64 << 32) | 3);
    emitter.instruction("str x9, [sp]");                                        // publish callback ABI version three and its 96-byte size
    emitter.instruction("str x4, [sp, #8]");                                    // retain the actual caller's optional eval context
    for (index, symbol) in CALLBACKS.iter().enumerate() {
        abi::emit_symbol_address(emitter, "x9", symbol);
        emitter.instruction(&format!("str x9, [sp, #{}]", 16 + index * 8));     // publish the required non-unwinding host callback
    }
    emitter.instruction(&format!("cmp x0, #{}", elephc_builtin_contract::RuntimeBuiltinId::SharedIni.as_u32())); // select identity-aware input only for the internal INI operation
    emitter.instruction("b.ne __rt_mbstring_invoke_host_ready");                // ordinary text operations keep their allocation-free input metadata
    abi::emit_symbol_address(emitter, "x9", "__rt_mbstring_ini_input");
    emitter.instruction("str x9, [sp, #24]");                                   // preserve source string identity while retaining the existing host version
    emitter.label("__rt_mbstring_invoke_host_ready");
    if mbregex {
        emitter.instruction(&format!("cmp x0, #{}", elephc_builtin_contract::RuntimeBuiltinId::MbEregReplaceCallback.as_u32())); // select the protected replacement callback host
        emitter.instruction("b.eq __rt_mbstring_invoke_callback");              // use ordered callable preparation before matching
    }
    emitter.instruction(&format!("cmp x0, #{}", elephc_builtin_contract::RuntimeBuiltinId::MbOutputHandler.as_u32())); // select response-aware dispatch after building the common value host
    emitter.instruction("b.ne __rt_mbstring_invoke_ordinary");                  // ordinary operations need only the value callback table
    response_host(emitter);
    emitter.instruction("mov x0, x1");                                          // pass the borrowed argument pointer array first
    emitter.instruction("mov x1, x2");                                          // retain the caller's complete argument count
    emitter.instruction("mov x2, x3");                                          // preserve PHP scalar coercion strictness
    emitter.instruction("mov x3, sp");                                          // borrow the common value host with the original eval context
    emitter.instruction("add x4, sp, #96");                                     // borrow disjoint response metadata and header callbacks
    emitter.instruction("add x5, sp, #128");                                    // provide shared owned bridge-result storage
    emitter.bl_c("elephc_mbstring_output_invoke_v1");
    emitter.instruction("b __rt_mbstring_invoke_completed");                    // join common materialization and failure cleanup
    if mbregex {
        emitter.label("__rt_mbstring_invoke_callback");
        callback_host(emitter);
        emitter.instruction("mov x0, x1");                                      // pass the original boxed argument pointers
        emitter.instruction("mov x1, x2");                                      // preserve the supplied argument count
        emitter.instruction("mov x2, x3");                                      // retain scalar coercion strictness
        emitter.instruction("mov x3, sp");                                      // borrow the value host for casting and cleanup
        emitter.instruction("add x4, sp, #96");                                 // borrow the replacement callback host
        emitter.instruction("add x5, sp, #128");                                // transfer the bridge result into existing storage
        emitter.bl_c("elephc_mbstring_callback_invoke_v1");
        emitter.instruction("b __rt_mbstring_invoke_completed");                // share result transfer and pending throwable handling
    }
    emitter.label("__rt_mbstring_invoke_ordinary");
    emitter.instruction("mov x4, sp");                                          // pass the immutable callback table to the Rust coordinator
    emitter.instruction("add x5, sp, #128");                                    // provide separate owned bridge-result storage
    emitter.bl_c("elephc_mbstring_invoke_v1");
    emitter.label("__rt_mbstring_invoke_completed");
    emitter.instruction("cbnz x0, __rt_mbstring_invoke_failed");                // return pending throws or fatal failures without PHP result allocation
    emitter.instruction("add x0, sp, #128");                                    // transfer the completed owned result into native materialization
    emitter.instruction("bl __rt_mbstring_materialize");                        // preserve PHP error classes and result ownership through the shared path
    emitter.instruction("b __rt_mbstring_invoke_done");                         // return the materialized value/status/length/kind tuple
    emitter.label("__rt_mbstring_invoke_failed");
    emitter.instruction("str x0, [sp, #176]");                                  // retain the coordinator's failure status across buffer release
    emitter.instruction("add x0, sp, #128");                                    // release any empty fatal result using its owning allocator
    emitter.bl_c("elephc_mbstring_release_v1");
    emitter.instruction("ldr x1, [sp, #176]");                                  // restore the non-unwinding failure status
    emitter.instruction("mov x0, #0");                                          // failure transfers no PHP result value
    emitter.instruction("mov x2, #0");                                          // failure has no string length
    emitter.instruction("mov x3, #0");                                          // failure has no successful result kind
    emitter.label("__rt_mbstring_invoke_done");
    emitter.instruction("ldp x29, x30, [sp, #192]");                            // restore caller linkage after every callback and Rust frame returned
    emitter.instruction("add sp, sp, #208");                                    // release invocation-local callback and result storage
    emitter.instruction("ret");                                                 // return the internal native result tuple
}

/// Builds the SysV callback table and passes all six coordinator inputs in C argument registers.
fn x86_64(emitter: &mut Emitter, mbregex: bool) {
    emitter.instruction("push rbp");                                            // preserve linkage and align the subsequent Rust C call
    emitter.instruction("mov rbp, rsp");                                        // establish a stable coordinator frame
    emitter.instruction("sub rsp, 192");                                        // reserve value and response hosts, bridge result, and failure status
    if mbregex { abi::emit_call_label(emitter, "__rt_mbregex_init"); }
    abi::emit_load_int_immediate(emitter, "r10", (96_i64 << 32) | 3);
    emitter.instruction("mov QWORD PTR [rsp], r10");                            // publish callback ABI version three and its complete size
    emitter.instruction("mov QWORD PTR [rsp + 8], r8");                         // retain the incoming caller's optional eval context
    for (index, symbol) in CALLBACKS.iter().enumerate() {
        abi::emit_symbol_address(emitter, "r10", symbol);
        emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", 16 + index * 8)); // publish the required protected host callback
    }
    emitter.instruction(&format!("cmp edi, {}", elephc_builtin_contract::RuntimeBuiltinId::SharedIni.as_u32())); // select identity-aware input only for the internal INI operation
    emitter.instruction("jne __rt_mbstring_invoke_host_ready");                 // ordinary text operations retain their existing input metadata path
    abi::emit_symbol_address(emitter, "r10", "__rt_mbstring_ini_input");
    emitter.instruction("mov QWORD PTR [rsp + 24], r10");                       // preserve source identity through the same validated callback table
    emitter.label("__rt_mbstring_invoke_host_ready");
    if mbregex {
        emitter.instruction(&format!("cmp edi, {}", elephc_builtin_contract::RuntimeBuiltinId::MbEregReplaceCallback.as_u32())); // select the replacement callback coordinator
        emitter.instruction("je __rt_mbstring_invoke_callback");                // retain the original arguments for ordered validation
    }
    emitter.instruction(&format!("cmp edi, {}", elephc_builtin_contract::RuntimeBuiltinId::MbOutputHandler.as_u32())); // select output conversion after retaining the caller's value context
    emitter.instruction("jne __rt_mbstring_invoke_ordinary");                   // ordinary operations use the existing coordinator
    response_host(emitter);
    emitter.instruction("mov rdi, rsi");                                        // pass the original borrowed argument pointers first
    emitter.instruction("mov rsi, rdx");                                        // retain the complete PHP argument count
    emitter.instruction("mov edx, ecx");                                        // preserve scalar coercion strictness
    emitter.instruction("mov rcx, rsp");                                        // borrow the value callback table with its original eval context
    emitter.instruction("lea r8, [rsp + 96]");                                  // borrow the independent response callback table
    emitter.instruction("lea r9, [rsp + 128]");                                 // provide owned result storage shared with ordinary dispatch
    emitter.bl_c("elephc_mbstring_output_invoke_v1");
    emitter.instruction("jmp __rt_mbstring_invoke_completed");                  // join the common result and failure ownership paths
    if mbregex {
        emitter.label("__rt_mbstring_invoke_callback");
        callback_host(emitter);
        emitter.instruction("mov rdi, rsi");                                    // pass the original borrowed argument pointers
        emitter.instruction("mov rsi, rdx");                                    // preserve the supplied argument count
        emitter.instruction("mov edx, ecx");                                    // preserve scalar coercion strictness
        emitter.instruction("mov rcx, rsp");                                    // borrow the value and cleanup host
        emitter.instruction("lea r8, [rsp + 96]");                              // borrow the replacement callback host
        emitter.instruction("lea r9, [rsp + 128]");                             // provide the shared owned result storage
        emitter.bl_c("elephc_mbstring_callback_invoke_v1");
        emitter.instruction("jmp __rt_mbstring_invoke_completed");              // join ordinary materialization and exception transfer
    }
    emitter.label("__rt_mbstring_invoke_ordinary");
    emitter.instruction("mov r8, rsp");                                         // pass the callback table as the fifth C argument
    emitter.instruction("lea r9, [rsp + 128]");                                 // pass result ownership storage as the sixth C argument
    emitter.bl_c("elephc_mbstring_invoke_v1");
    emitter.label("__rt_mbstring_invoke_completed");
    emitter.instruction("test eax, eax");                                       // distinguish a completed engine result from callback failure
    emitter.instruction("jnz __rt_mbstring_invoke_failed");                     // preserve pending PHP exceptions without result construction
    emitter.instruction("lea rdi, [rsp + 128]");                                // transfer the owned bridge result into native materialization
    emitter.instruction("call __rt_mbstring_materialize");                      // reuse scalar/array copying and catchable PHP error construction
    emitter.instruction("jmp __rt_mbstring_invoke_done");                       // return the shared native result tuple
    emitter.label("__rt_mbstring_invoke_failed");
    emitter.instruction("mov DWORD PTR [rsp + 176], eax");                      // preserve the callback failure status during bridge-buffer release
    emitter.instruction("lea rdi, [rsp + 128]");                                // release the empty fatal or pending result through its Rust allocator
    emitter.bl_c("elephc_mbstring_release_v1");
    emitter.instruction("mov edx, DWORD PTR [rsp + 176]");                      // restore the non-unwinding runtime status
    emitter.instruction("xor eax, eax");                                        // failure transfers no PHP result value
    emitter.instruction("xor ecx, ecx");                                        // failure has no binary string length
    emitter.instruction("xor r8d, r8d");                                        // failure has no successful result kind
    emitter.label("__rt_mbstring_invoke_done");
    emitter.instruction("leave");                                               // release callback/result storage and restore the caller frame
    emitter.instruction("ret");                                                 // return value/status/length/kind through the native convention
}

/// Publishes callable resolution and invocation with the same eval context as the value host.
fn callback_host(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 {
        abi::emit_load_int_immediate(emitter, "x9", (32_i64 << 32) | 1);
        emitter.instruction("str x9, [sp, #96]");                               // publish version one and the complete host size
        emitter.instruction("ldr x9, [sp, #8]");                                // recover the actual caller's eval context
        emitter.instruction("str x9, [sp, #104]");                              // retain the context for both callable operations
        for (offset, symbol) in [(112, "__rt_mbstring_callback_resolve"), (120, "__rt_mbstring_callback_call")] {
            abi::emit_symbol_address(emitter, "x9", symbol);
            emitter.instruction(&format!("str x9, [sp, #{offset}]"));           // publish an exception-contained callback
        }
    } else {
        abi::emit_load_int_immediate(emitter, "r10", (32_i64 << 32) | 1);
        emitter.instruction("mov QWORD PTR [rsp + 96], r10");                   // publish version one and the complete host size
        emitter.instruction("mov r10, QWORD PTR [rsp + 8]");                    // recover the caller's optional eval context
        emitter.instruction("mov QWORD PTR [rsp + 104], r10");                  // share the context across both callable callbacks
        for (offset, symbol) in [(112, "__rt_mbstring_callback_resolve"), (120, "__rt_mbstring_callback_call")] {
            abi::emit_symbol_address(emitter, "r10", symbol);
            emitter.instruction(&format!("mov QWORD PTR [rsp + {offset}], r10")); // publish a protected callback address
        }
    }
}

/// Publishes the response host without changing incoming operation, argument, or strictness registers.
fn response_host(emitter: &mut Emitter) {
    let info = emitter.target.extern_symbol("elephc_mbstring_response_info_v1");
    if emitter.target.arch == Arch::AArch64 {
        abi::emit_load_int_immediate(emitter, "x9", (32_i64 << 32) | 1);
        emitter.instruction("str x9, [sp, #96]");                               // publish version one and the complete response host size
        for (offset, symbol) in [(104, "_ob_in_handler"), (112, info.as_str()), (120, "__rt_mbstring_output_header")] {
            abi::emit_symbol_address(emitter, "x9", symbol);
            emitter.instruction(&format!("str x9, [sp, #{offset}]"));           // retain the live handler flag and protected response callbacks
        }
    } else {
        abi::emit_load_int_immediate(emitter, "r10", (32_i64 << 32) | 1);
        emitter.instruction("mov QWORD PTR [rsp + 96], r10");                   // publish version one and the complete response host size
        for (offset, symbol) in [(104, "_ob_in_handler"), (112, info.as_str()), (120, "__rt_mbstring_output_header")] {
            abi::emit_symbol_address(emitter, "r10", symbol);
            emitter.instruction(&format!("mov QWORD PTR [rsp + {offset}], r10")); // retain the live handler flag and protected response callbacks
        }
    }
}
