//! Purpose:
//! Connects resolved native output references to the shared mbstring invocation coordinator.
//!
//! Called from:
//! - Capture/query reference adapter integration and native runtime-GC tests.
//!
//! Key details:
//! - C6 inputs are RuntimeBuiltinId, argument pointers, count, strictness, eval context, and native output state.
//! - Results use the ordinary native mbstring value/status/length/kind convention.
//! - Stack-local V4/V5 tables share base callbacks, initialization, and result ownership.
//! - Deferred initialization owners move into request storage even on pending or fatal returns.
//! - PHP lvalue lowering and type validation remain caller responsibilities.

use super::*;
use elephc_builtin_contract::mbstring_abi::invoke::{MbInvokeHostV4, MbInvokeHostV5, MbNativeCaptureV1, MbNativeQueryV1};

mod initialize;
mod query_policy;
mod frame;
use frame::Frame;


/// Emits both reference hosts with one shared callback inventory and ownership implementation.
pub(super) fn emit(emitter: &mut Emitter, mbregex: bool) {
    assert_eq!(std::mem::size_of::<MbInvokeHostV5>(), 144);
    assert_eq!(std::mem::size_of::<MbNativeQueryV1>(), 40);
    if mbregex { emit_invocation(emitter, Frame::capture(), true); }
    emit_invocation(emitter, Frame::query(), mbregex);
    for (index, callback) in invoke::CALLBACKS.iter().enumerate() {
        context_wrapper(emitter, &wrapper(index), callback);
    }
    context_wrapper(emitter, "__rt_mbstring_capture_fill_context", "__rt_mbstring_capture_reference_fill");
    initialize::emit(emitter);
    query_policy::emit(emitter);
}

/// Builds one complete host table while preserving the native caller's original arguments.
fn emit_invocation(emitter: &mut Emitter, frame: Frame, mbregex: bool) {
    let name = frame.name;
    let context = frame.context;
    let state = frame.state;
    let result = frame.result;
    let linkage = frame.linkage;
    let allocation = frame.allocation;
    assert_eq!(std::mem::size_of::<MbInvokeHostV4>(), 120);
    assert_eq!(std::mem::size_of::<MbNativeCaptureV1>(), 16);
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global(frame.name);
    if arm {
        emitter.instruction(&format!("sub sp, sp, #{allocation}"));             // reserve the host table, wrapped context, bridge result, and linkage
        emitter.instruction(&format!("stp x29, x30, [sp, #{linkage}]"));        // preserve linkage across callbacks and result materialization
        emitter.instruction(&format!("add x29, sp, #{linkage}"));               // expose the invocation frame to native diagnostics
        emitter.instruction(&format!("stp x4, x5, [sp, #{context}]"));          // retain eval context and capture state before provider initialization
    } else {
        emitter.instruction("push rbp");                                        // preserve linkage and align nested calls
        emitter.instruction("mov rbp, rsp");                                    // establish the native invocation frame
        emitter.instruction(&format!("sub rsp, {linkage}"));                    // reserve the callback table, wrapped context, result, and status
        emitter.instruction(&format!("mov QWORD PTR [rsp + {context}], r8"));   // preserve the original eval context for ordinary callbacks
        emitter.instruction(&format!("mov QWORD PTR [rsp + {state}], r9"));     // preserve capture state before the five-input provider initializer
    }
    if mbregex { abi::emit_call_label(emitter, "__rt_mbregex_init"); }
    if arm {
        emitter.instruction(&format!("ldr x9, [sp, #{state}]"));                // recover optional capture output state after provider registration
        emitter.instruction(&format!("cbz x9, {name}_state_ready"));            // two-argument calls require no output policy
        emitter.instruction("str xzr, [x9, #8]");                               // initialize deferred ownership before argument coercion or validation
    } else {
        emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {state}]"));    // recover optional capture state
        emitter.instruction("test r10, r10");                                   // distinguish two-argument invocations without output state
        emitter.instruction(&format!("jz {name}_state_ready"));                 // leave absent optional state untouched
        emitter.instruction("mov QWORD PTR [r10 + 8], 0");                      // initialize deferred ownership before any PHP callback
    }
    emitter.label(&format!("{name}_state_ready"));
    let scratch = if arm { "x9" } else { "r10" };
    abi::emit_load_int_immediate(emitter, scratch, ((frame.table_bytes as i64) << 32) | frame.version);
    emitter.instruction(if arm { "str x9, [sp]" } else { "mov QWORD PTR [rsp], r10" }); // publish the complete host table version and size
    emitter.instruction(&if arm { format!("add x9, sp, #{context}") } else { format!("lea r10, [rsp + {context}]") }); // address this invocation's wrapped callback context
    emitter.instruction(if arm { "str x9, [sp, #8]" } else { "mov QWORD PTR [rsp + 8], r10" }); // bind callbacks to their original eval context and output policy
    for index in 0..10 { table_entry(emitter, 16 + index * 8, &wrapper(index)); }
    table_entry(emitter, 96, "__rt_mbstring_capture_initialize");
    table_entry(emitter, 104, "__rt_mbstring_capture_fill_context");
    table_entry(emitter, 112, &wrapper(5));
    if frame.version == 5 { query_policy::table(emitter, frame.state); }
    if arm {
        emitter.instruction("mov x4, sp");                                      // supply the complete callback table as the fifth C input
        emitter.instruction(&format!("add x5, sp, #{result}"));                 // supply bridge-owned result storage after the wrapped context
    } else {
        emitter.instruction("mov r8, rsp");                                     // pass the host table in the fifth C register
        emitter.instruction(&format!("lea r9, [rsp + {result}]"));              // pass bridge-owned result storage in the sixth C register
    }
    emitter.bl_c("elephc_mbstring_invoke_v1");
    finish(emitter, frame);
}

/// Names a context-unwrapping trampoline for one entry in the authoritative base callback inventory.
fn wrapper(index: usize) -> String { format!("__rt_mbstring_capture_base_{index}") }

/// Installs one target-aware callback address without clobbering PHP call inputs.
fn table_entry(emitter: &mut Emitter, offset: usize, symbol: &str) {
    let arm = emitter.target.arch == Arch::AArch64;
    abi::emit_symbol_address(emitter, if arm { "x9" } else { "r10" }, symbol);
    let instruction = if arm { format!("str x9, [sp, #{offset}]") }
        else { format!("mov QWORD PTR [rsp + {offset}], r10") };
    emitter.instruction(&instruction);                                          // keep each callback at its neutral ABI offset
}

/// Restores the original eval context while preserving all other C inputs and the caller's frame.
fn context_wrapper(emitter: &mut Emitter, name: &str, callback: &str) {
    emitter.label_global(name);
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { "ldr x0, [x0]" } else { "mov rdi, QWORD PTR [rdi]" }); // unwrap only the callback's first context argument
    emitter.instruction(&format!("{} {callback}", if emitter.target.arch == Arch::AArch64 { "b" } else { "jmp" })); // reuse the shared protected callback and its ownership rules
}

/// Consumes bridge buffers or materializes success using the ordinary native value/status convention.
fn finish(emitter: &mut Emitter, frame: Frame) {
    let name = frame.name;
    let result = frame.result;
    let status = frame.status;
    let linkage = frame.linkage;
    let allocation = frame.allocation;
    let arm = emitter.target.arch == Arch::AArch64;
    if arm {
        emitter.instruction(&format!("cbnz x0, {name}_failed"));                // preserve pending or fatal status after all coordinator-owned cleanup
        emitter.instruction(&format!("add x0, sp, #{result}"));                 // transfer the completed bridge result to the common materializer
        abi::emit_call_label(emitter, "__rt_mbstring_materialize");
        emitter.instruction(&format!("b {name}_done"));                         // return materialized PHP values or catchable errors
    } else {
        emitter.instruction("test eax, eax");                                   // distinguish a completed bridge result from host failure
        emitter.instruction(&format!("jnz {name}_failed"));                     // preserve pending throw or fatal status through buffer release
        emitter.instruction(&format!("lea rdi, [rsp + {result}]"));             // transfer the completed result to the common materializer
        abi::emit_call_label(emitter, "__rt_mbstring_materialize");
        emitter.instruction(&format!("jmp {name}_done"));                       // return the same value/status tuple as ordinary mbstring invocation
    }
    emitter.label(&format!("{name}_failed"));
    emitter.instruction(&if arm { format!("str x0, [sp, #{status}]") } else { format!("mov DWORD PTR [rsp + {status}], eax") }); // preserve status while releasing any remaining Rust result buffers
    emitter.instruction(&if arm { format!("add x0, sp, #{result}") } else { format!("lea rdi, [rsp + {result}]") }); // address the result even when initialization failed
    emitter.bl_c("elephc_mbstring_release_v1");
    if arm {
        emitter.instruction(&format!("ldr x1, [sp, #{status}]"));               // return the contained pending or fatal status
        emitter.instruction("mov x0, #0");                                      // no PHP value is owned on the failed path
        emitter.instruction("mov x2, #0");                                      // clear absent result length
        emitter.instruction("mov x3, #0");                                      // clear absent result kind
    } else {
        emitter.instruction(&format!("mov edx, DWORD PTR [rsp + {status}]"));   // return the contained host status beside the empty result
        emitter.instruction("xor eax, eax");                                    // no PHP result owner is returned on failure
        emitter.instruction("xor ecx, ecx");                                    // clear absent result length
        emitter.instruction("xor r8d, r8d");                                    // clear absent result kind
    }
    emitter.label(&format!("{name}_done"));
    adopt_deferred(emitter, frame);
    if arm {
        emitter.instruction(&format!("ldp x29, x30, [sp, #{linkage}]"));        // restore linkage after all bridge ownership transfers
        emitter.instruction(&format!("add sp, sp, #{allocation}"));             // retire only stack-local invocation metadata
    } else { emitter.instruction("leave"); }                                    // restore the aligned caller frame
    emitter.instruction("ret");                                                 // return without unwinding through native or Rust adapter frames
}

/// Transfers displaced ownership to the request while preserving every native result word.
fn adopt_deferred(emitter: &mut Emitter, frame: Frame) {
    let name = frame.name;
    let state = frame.state;
    let result = frame.result;
    let result_status = frame.result_status;
    let result_length = frame.result_length;
    let result_kind = frame.result_kind;
    let arm = emitter.target.arch == Arch::AArch64;
    if arm {
        emitter.instruction(&format!("stp x0, x1, [sp, #{result}]"));           // preserve native result and status after consuming bridge buffers
        emitter.instruction(&format!("stp x2, x3, [sp, #{result_length}]"));    // preserve result length and kind across ownership adoption
        emitter.instruction(&format!("ldr x9, [sp, #{state}]"));                // recover the optional capture initialization state
        emitter.instruction(&format!("cbz x9, {name}_adopted"));                // calls without output state have no displaced capture value
        emitter.instruction("ldr x0, [x9, #8]");                                // transfer any destructor-installed value to request lifetime
        emitter.instruction("str xzr, [x9, #8]");                               // prevent duplicate ownership release by the caller
    } else {
        emitter.instruction(&format!("mov QWORD PTR [rsp + {result}], rax"));   // retain the materialized PHP value across ownership adoption
        emitter.instruction(&format!("mov QWORD PTR [rsp + {result_status}], rdx")); // preserve the coordinator's pending or fatal status
        emitter.instruction(&format!("mov QWORD PTR [rsp + {result_length}], rcx")); // retain successful result length
        emitter.instruction(&format!("mov QWORD PTR [rsp + {result_kind}], r8")); // retain successful result kind
        emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {state}]"));    // recover optional capture state after coordinator cleanup
        emitter.instruction("test r10, r10");                                   // distinguish calls without output initialization state
        emitter.instruction(&format!("jz {name}_adopted"));                     // skip adoption when no output state was supplied
        emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                    // transfer the displaced boxed owner without another retain
        emitter.instruction("mov QWORD PTR [r10 + 8], 0");                      // consume caller-visible ownership exactly once
    }
    abi::emit_call_label(emitter, "__rt_mbstring_defer_capture");
    emitter.label(&format!("{name}_adopted"));
    if arm {
        emitter.instruction(&format!("ldp x0, x1, [sp, #{result}]"));           // restore native value and contained failure status
        emitter.instruction(&format!("ldp x2, x3, [sp, #{result_length}]"));    // restore result metadata after ownership transfer
    } else {
        emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {result}]"));   // restore the successful value or empty failure payload
        emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {result_status}]")); // preserve the coordinator's original status
        emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {result_length}]")); // restore result length
        emitter.instruction(&format!("mov r8, QWORD PTR [rsp + {result_kind}]")); // restore result kind
    }
}
