//! Purpose:
//! Materializes owned mbstring results and catchable errors for native and eval callers.
//!
//! Called from:
//! - Optional runtime emission and typed mbstring AOT/eval dispatch paths.
//!
//! Key details:
//! - The status entry returns failures after Rust has released its state borrow.
//! - Bridge buffers are copied into owned runtime storage before their release.
//! - Only the native entry unwinds; eval receives a pending throwable status.
//! - Array wire arguments transfer temporary ownership to the status adapter before any PHP throw.

use elephc_builtin_contract::RuntimeBuiltinStatus;
use elephc_builtin_contract::mbstring_abi::{RESULT_BOOL, RESULT_NULL, RESULT_INT, RESULT_STRING, RESULT_STRING_ARRAY, RESULT_ENCODING_CATALOG, RESULT_ARRAY, RESULT_INI_STRING, RESULT_INI_ARRAY, RESULT_EXCEPTION_CHAIN, RESULT_VALUE_ERROR, RESULT_TYPE_ERROR, RESULT_ERROR};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

mod result;
mod restore;
mod catalog;
mod ini_identity;
mod ini_input;
mod native;
mod array;
mod snapshot;
mod array_argument;
mod array_value;
mod stringable;
mod input;
mod materialize;
mod exception;
mod invoke;
mod capture_invoke;
mod deferred_capture;
mod callbacks;
mod capture_hash;
mod query_remove;
mod query_enter;
mod query_register;
mod capture_destination;
mod capture_reference;
mod capture_reference_begin;
mod regex;
mod callback;

/// Emits boxed invocation entries with caller strictness/context plus the separate wire-result adapter.
pub fn emit_mbstring(emitter: &mut Emitter, eval_bridge: bool, mbregex: bool) {
    native::emit(emitter, mbregex);
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
    result::emit(emitter);
    restore::emit(emitter);
    catalog::emit(emitter);
    deferred_capture::emit(emitter);
    ini_identity::emit(emitter);
    ini_input::emit(emitter);
    array::emit(emitter);
    snapshot::emit(emitter);
    array_argument::emit(emitter);
    array_value::emit(emitter, eval_bridge);
    stringable::emit(emitter);
    input::emit(emitter, eval_bridge);
    materialize::emit(emitter);
    exception::emit(emitter);
    if mbregex { regex::emit(emitter); callback::emit(emitter, eval_bridge); }
    invoke::emit(emitter, mbregex);
    capture_invoke::emit(emitter, mbregex);
    callbacks::emit(emitter);
    capture_hash::emit(emitter);
    query_remove::emit(emitter);
    query_enter::emit(emitter);
    query_register::emit(emitter);
    capture_destination::emit(emitter);
    capture_reference::emit(emitter);
    capture_reference_begin::emit(emitter);
}

/// Emits AArch64 owned result/error materialization with value/status/length/kind returns.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_status");
    emitter.instruction("sub sp, sp, #128");                                    // reserve the wire result, returned values, exception storage, and linkage
    emitter.instruction("stp x29, x30, [sp, #112]");                            // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #112");                                   // establish the bridge adapter frame
    emitter.instruction("stp xzr, xzr, [sp, #64]");                             // initialize the returned byte length and kind
    emitter.instruction("stp x1, x2, [sp, #80]");                               // retain mutable argument slots for transferred array-owner cleanup
    emitter.instruction("mov x3, sp");                                          // pass writable bridge result storage
    emitter.bl_c("elephc_mbstring_call_v1");
    emitter.instruction("ldp x0, x1, [sp, #80]");                               // restore the argument array and supplied count after Rust returns
    emitter.instruction("bl __rt_mbstring_release_array_arguments");            // release array wire owners before diagnostics or exception construction
    emitter.label_shared("__rt_mbstring_status_diagnostics");
    emitter.instruction("ldp x1, x2, [sp, #32]");                               // load complete diagnostic lines and their byte length
    emitter.instruction("cbz x2, __rt_mbstring_status_result");                 // skip diagnostic output when the bridge reported none
    emitter.instruction("bl __rt_diag_warning");                                // emit diagnostics with the runtime suppression policy
    emitter.label("__rt_mbstring_status_result");
    emitter.instruction("ldr x9, [sp, #0]");                                    // inspect the bridge result kind
    emitter.instruction("str x9, [sp, #72]");                                   // retain the result kind for boxed callers
    emitter.instruction(&format!("cmp x9, #{}", RESULT_INT));                   // accept an integer outcome
    emitter.instruction("b.eq __rt_mbstring_status_int");                       // materialize successful character counts
    emitter.instruction(&format!("cmp x9, #{}", RESULT_BOOL));                  // recognize a boolean result, including false
    emitter.instruction("b.eq __rt_mbstring_status_int");                       // boolean payloads share integer materialization
    emitter.instruction(&format!("cmp x9, #{}", RESULT_NULL));                  // recognize a successful null with an empty scalar payload
    emitter.instruction("b.eq __rt_mbstring_status_int");                       // preserve null identity for the common result boxer
    emitter.instruction(&format!("cmp x9, #{}", RESULT_STRING));                // recognize an owned string result
    emitter.instruction("b.eq __rt_mbstring_status_string");                    // copy the string before releasing Rust ownership
    emitter.instruction(&format!("cmp x9, #{}", RESULT_INI_STRING));            // recognize a string carrying an independently owned INI lease
    emitter.instruction("b.eq __rt_mbstring_status_string");                    // persist bytes before binding the original identity
    emitter.instruction(&format!("cmp x9, #{}", RESULT_INI_ARRAY));             // recognize a graph followed by per-string INI identities
    emitter.instruction("b.eq __rt_mbstring_status_ini_graph");                 // preserve identity records through native graph construction
    emitter.instruction(&format!("cmp x9, #{}", RESULT_ARRAY));                 // recognize a completed recursive array graph
    emitter.instruction("b.eq __rt_mbstring_status_graph");                     // restore independent arrays before releasing wire bytes
    emitter.instruction(&format!("cmp x9, #{}", RESULT_ENCODING_CATALOG));      // recognize the V2 cached catalog result
    emitter.instruction("b.eq __rt_mbstring_status_array");                     // use the shared packed-array branch
    emitter.instruction(&format!("cmp x9, #{}", RESULT_STRING_ARRAY));          // recognize a packed array of binary strings
    emitter.instruction("b.eq __rt_mbstring_status_array");                     // copy array elements before releasing Rust ownership
    for kind in [RESULT_VALUE_ERROR, RESULT_TYPE_ERROR, RESULT_ERROR, RESULT_EXCEPTION_CHAIN,
        elephc_builtin_contract::mbstring_abi::coercion::PREPARED_ARGUMENT_COUNT_ERROR] {
        emitter.instruction(&format!("cmp x9, #{}", kind));                     // recognize a single PHP exception or a complete ordered chain
        emitter.instruction("b.eq __rt_mbstring_status_error");                 // share validated Throwable construction for every error outcome
    }
    emitter.label("__rt_mbstring_status_fatal");
    emitter.instruction(&format!("mov x9, #{}", RuntimeBuiltinStatus::RuntimeFatal as i32)); // fail closed for any unexpected bridge result kind
    emitter.instruction("stp xzr, x9, [sp, #48]");                              // record a fatal status without a PHP value
    emitter.instruction("b __rt_mbstring_status_release");                      // release all bridge payloads on failure
    emitter.label("__rt_mbstring_status_int");
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the character count returned by the engine
    emitter.instruction("stp x9, xzr, [sp, #48]");                              // record the integer and Success status
    emitter.instruction("b __rt_mbstring_status_release");                      // release optional diagnostic storage after success
    emitter.label("__rt_mbstring_status_string");
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // borrow the complete bridge string
    emitter.instruction("bl __rt_str_persist");                                 // create independent runtime-owned result storage
    emitter.instruction("stp x1, xzr, [sp, #48]");                              // save the owned pointer and successful status
    emitter.instruction("str x2, [sp, #64]");                                   // save the result byte length
    emitter.instruction("ldr x9, [sp]");                                        // inspect the untouched wire kind before releasing its lease
    emitter.instruction(&format!("cmp x9, #{}", RESULT_INI_STRING));            // ordinary strings need no explicit INI identity binding
    emitter.instruction("b.ne __rt_mbstring_status_release");                   // return ordinary strings through the existing ownership path
    emitter.instruction("mov x0, x1");                                          // pass the new native allocation to the identity registry
    emitter.instruction("mov x1, x2");                                          // bind its complete logical byte range
    emitter.instruction("ldr x2, [sp, #8]");                                    // borrow the wire result's still-live identity lease
    emitter.bl_c("elephc_mbstring_native_string_bind_v1");
    emitter.instruction("cbz x0, __rt_mbstring_status_ini_string_ready");       // normalize successful INI results only after acquiring native ownership
    emitter.instruction("ldr x0, [sp, #48]");                                   // recover the unbound string owner after metadata rejection
    emitter.instruction("bl __rt_decref_any");                                  // release the failed native allocation before wire cleanup
    emitter.instruction("b __rt_mbstring_status_fatal");                        // preserve a recoverable fatal status for a broken wire identity
    emitter.label("__rt_mbstring_status_ini_string_ready");
    emitter.instruction(&format!("mov x9, #{}", RESULT_STRING));                // expose ordinary string ownership to the shared result boxer
    emitter.instruction("str x9, [sp, #72]");                                   // retain the wire INI kind separately for lease release
    emitter.instruction("b __rt_mbstring_status_release");                      // release the original bridge buffers
    emitter.label("__rt_mbstring_status_ini_graph");
    emitter.instruction("ldp x0, x1, [sp, #16]");                               // borrow the graph and complete identity trailer
    emitter.instruction("ldr x2, [sp, #8]");                                    // delimit the graph prefix without dropping identity records
    emitter.instruction("bl __rt_mbstring_restore_ini_array");                  // retain native string identities before consuming the wire result
    emitter.instruction("b __rt_mbstring_status_graph_ready");                  // classify the completed root through the common ownership path
    emitter.label("__rt_mbstring_status_graph");
    emitter.instruction("ldp x0, x1, [sp, #16]");                               // borrow the complete validated graph buffer
    emitter.instruction("bl __rt_mbstring_restore_array");                      // construct keys, values, and completed child arrays
    emitter.label("__rt_mbstring_status_graph_ready");
    emitter.instruction("cbz x0, __rt_mbstring_status_fatal");                  // preserve explicit failure after restorer cleanup
    emitter.instruction("stp x0, xzr, [sp, #48]");                              // transfer the restored root with successful status
    emitter.instruction("bl __rt_heap_kind");                                   // classify the completed root without changing ownership
    emitter.instruction("cmp x0, #2");                                          // distinguish indexed roots from associative roots
    emitter.instruction(&format!("mov x9, #{}", RESULT_STRING_ARRAY));          // reuse the indexed Mixed-array result convention
    emitter.instruction(&format!("mov x10, #{}", RESULT_ARRAY));                // preserve the associative graph result convention
    emitter.instruction("csel x9, x9, x10, eq");                                // expose the actual root representation to boxed callers
    emitter.instruction("str x9, [sp, #72]");                                   // retain the selected native result kind
    emitter.instruction("b __rt_mbstring_status_release");                      // consume graph wire ownership before returning
    emitter.label("__rt_mbstring_status_array");
    emitter.instruction("ldr x0, [sp, #16]");                                   // borrow the packed string-array bytes
    emitter.instruction("ldr x1, [sp, #24]");                                   // pass the complete packed byte length
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the expected element count
    emitter.instruction("ldr x9, [sp, #72]");                                   // inspect the original packed-array result kind
    emitter.instruction(&format!("cmp x9, #{}", RESULT_ENCODING_CATALOG));      // distinguish catalog identity from ordinary arrays
    emitter.instruction("b.ne __rt_mbstring_status_array_fresh");               // materialize independent ordinary string arrays
    emitter.instruction("bl __rt_mbstring_catalog");                            // retain the current request's shared catalog
    emitter.instruction("b __rt_mbstring_status_array_ready");                  // join ordinary array status normalization
    emitter.label("__rt_mbstring_status_array_fresh");
    emitter.instruction("bl __rt_mbstring_string_array");                       // copy binary elements into owned runtime cells
    emitter.label("__rt_mbstring_status_array_ready");
    emitter.instruction(&format!("mov x9, #{}", RESULT_STRING_ARRAY));          // expose the ordinary owned-array convention to existing callers
    emitter.instruction("str x9, [sp, #72]");                                   // normalize the native result kind after cache selection
    emitter.instruction("cbz x0, __rt_mbstring_status_fatal");                  // reject malformed framing after partial-array cleanup
    emitter.instruction("stp x0, xzr, [sp, #48]");                              // retain the fresh array with successful status
    emitter.instruction("b __rt_mbstring_status_release");                      // release the original packed bridge buffer
    emitter.label("__rt_mbstring_status_error");
    emitter.instruction("mov x0, sp");                                          // borrow the completed error result without consuming its buffers
    emitter.instruction("bl __rt_mbstring_exception_chain");                    // construct the complete previous chain before publishing a PHP exception
    emitter.instruction("stp xzr, x0, [sp, #48]");                              // retain the non-unwinding error status without a successful value
    emitter.label("__rt_mbstring_status_release");
    emitter.instruction("mov x0, sp");                                          // pass the complete result to its owning allocator
    emitter.bl_c("elephc_mbstring_release_v1");
    emitter.instruction("ldp x2, x3, [sp, #64]");                               // restore result byte length and kind for boxed callers
    emitter.instruction("ldp x0, x1, [sp, #48]");                               // restore the primary result and runtime status
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore caller linkage after all Rust calls have returned
    emitter.instruction("add sp, sp, #128");                                    // release the bridge adapter frame
    emitter.instruction("ret");                                                 // return value/status in x0/x1 and length/kind in x2/x3
}

/// Emits x86_64 owned result/error materialization with value/status/length/kind returns.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbstring_status");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish an ABI-aligned bridge adapter frame
    emitter.instruction("sub rsp, 112");                                        // reserve the wire result, returned values, and exception storage
    emitter.instruction("mov QWORD PTR [rsp + 64], 0");                         // initialize the returned byte length
    emitter.instruction("mov QWORD PTR [rsp + 80], rsi");                       // retain mutable wire slots for transferred array-owner cleanup
    emitter.instruction("mov QWORD PTR [rsp + 88], rdx");                       // retain the supplied argument count until Rust returns
    emitter.instruction("lea rcx, [rsp + 0]");                                  // pass writable bridge result storage
    emitter.bl_c("elephc_mbstring_call_v1");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 80]");                       // restore the mutable argument array after Rust returns
    emitter.instruction("mov rsi, QWORD PTR [rsp + 88]");                       // restore the supplied argument count
    emitter.instruction("call __rt_mbstring_release_array_arguments");          // release array wire owners before diagnostics or exception construction
    emitter.label_shared("__rt_mbstring_status_diagnostics");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                       // load complete diagnostic lines
    emitter.instruction("mov rsi, QWORD PTR [rsp + 40]");                       // load their byte length
    emitter.instruction("test rsi, rsi");                                       // check whether the bridge reported diagnostics
    emitter.instruction("jz __rt_mbstring_status_result");                      // skip diagnostic output when there is none
    emitter.instruction("call __rt_diag_warning");                              // emit diagnostics with the runtime suppression policy
    emitter.label("__rt_mbstring_status_result");
    emitter.instruction("mov r10, QWORD PTR [rsp + 0]");                        // inspect the bridge result kind
    emitter.instruction("mov QWORD PTR [rsp + 72], r10");                       // retain the result kind for boxed callers
    emitter.instruction(&format!("cmp r10, {}", RESULT_INT));                   // accept an integer outcome
    emitter.instruction("je __rt_mbstring_status_int");                         // materialize successful character counts
    emitter.instruction(&format!("cmp r10, {}", RESULT_BOOL));                  // recognize a boolean result, including false
    emitter.instruction("je __rt_mbstring_status_int");                         // boolean payloads share integer materialization
    emitter.instruction(&format!("cmp r10, {}", RESULT_NULL));                  // recognize a successful null with an empty scalar payload
    emitter.instruction("je __rt_mbstring_status_int");                         // preserve null identity for the common result boxer
    emitter.instruction(&format!("cmp r10, {}", RESULT_STRING));                // recognize an owned string result
    emitter.instruction("je __rt_mbstring_status_string");                      // copy the string before releasing Rust ownership
    emitter.instruction(&format!("cmp r10, {}", RESULT_INI_STRING));            // recognize a string carrying an independently owned INI lease
    emitter.instruction("je __rt_mbstring_status_string");                      // persist bytes before binding the original identity
    emitter.instruction(&format!("cmp r10, {}", RESULT_INI_ARRAY));             // recognize a graph followed by per-string INI identities
    emitter.instruction("je __rt_mbstring_status_ini_graph");                   // preserve identity records through native graph construction
    emitter.instruction(&format!("cmp r10, {}", RESULT_ARRAY));                 // recognize a completed recursive array graph
    emitter.instruction("je __rt_mbstring_status_graph");                       // restore independent arrays before releasing wire bytes
    emitter.instruction(&format!("cmp r10, {}", RESULT_ENCODING_CATALOG));      // recognize the V2 cached catalog result
    emitter.instruction("je __rt_mbstring_status_array");                       // use the shared packed-array branch
    emitter.instruction(&format!("cmp r10, {}", RESULT_STRING_ARRAY));          // recognize a packed array of binary strings
    emitter.instruction("je __rt_mbstring_status_array");                       // copy array elements before releasing Rust ownership
    for kind in [RESULT_VALUE_ERROR, RESULT_TYPE_ERROR, RESULT_ERROR, RESULT_EXCEPTION_CHAIN,
        elephc_builtin_contract::mbstring_abi::coercion::PREPARED_ARGUMENT_COUNT_ERROR] {
        emitter.instruction(&format!("cmp r10, {}", kind));                     // recognize a single PHP exception or a complete ordered chain
        emitter.instruction("je __rt_mbstring_status_error");                   // share validated Throwable construction for every error outcome
    }
    emitter.label("__rt_mbstring_status_fatal");
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // clear the failed integer result
    emitter.instruction(&format!("mov QWORD PTR [rsp + 56], {}", RuntimeBuiltinStatus::RuntimeFatal as i32)); // fail closed for any unexpected bridge result kind
    emitter.instruction("jmp __rt_mbstring_status_release");                    // release all bridge payloads on failure
    emitter.label("__rt_mbstring_status_int");
    emitter.instruction("mov r10, QWORD PTR [rsp + 8]");                        // load the character count returned by the engine
    emitter.instruction("mov QWORD PTR [rsp + 48], r10");                       // save the integer across buffer release
    emitter.instruction("mov QWORD PTR [rsp + 56], 0");                         // record the Success status
    emitter.instruction("jmp __rt_mbstring_status_release");                    // release optional diagnostic storage after success
    emitter.label("__rt_mbstring_status_string");
    emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                       // borrow the complete bridge string
    emitter.instruction("mov rdx, QWORD PTR [rsp + 24]");                       // pass its exact byte length
    emitter.instruction("call __rt_str_persist");                               // create independent runtime-owned result storage
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // save the owned result pointer
    emitter.instruction("mov QWORD PTR [rsp + 56], 0");                         // record successful completion
    emitter.instruction("mov QWORD PTR [rsp + 64], rdx");                       // save the result byte length
    emitter.instruction(&format!("cmp QWORD PTR [rsp], {}", RESULT_INI_STRING)); // inspect the untouched wire kind before releasing its lease
    emitter.instruction("jne __rt_mbstring_status_release");                    // return ordinary strings through their existing ownership path
    emitter.instruction("mov rdi, rax");                                        // bind the newly acquired native string allocation
    emitter.instruction("mov rsi, rdx");                                        // pass its complete logical byte length
    emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                        // borrow the wire result's still-live identity lease
    emitter.bl_c("elephc_mbstring_native_string_bind_v1");
    emitter.instruction("test eax, eax");                                       // distinguish successful binding from rejected identity metadata
    emitter.instruction("jz __rt_mbstring_status_ini_string_ready");            // normalize successful INI values after native ownership is retained
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // recover the unbound string owner after metadata rejection
    emitter.instruction("call __rt_decref_any");                                // release failed native storage before consuming the wire result
    emitter.instruction("jmp __rt_mbstring_status_fatal");                      // report a recoverable fatal status for a broken identity
    emitter.label("__rt_mbstring_status_ini_string_ready");
    emitter.instruction(&format!("mov QWORD PTR [rsp + 72], {}", RESULT_STRING)); // normalize boxed ownership while retaining the wire INI kind for release
    emitter.instruction("jmp __rt_mbstring_status_release");                    // release the original bridge buffers
    emitter.label("__rt_mbstring_status_ini_graph");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");                       // borrow the graph and its complete identity trailer
    emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");                       // pass the total readable wire byte length
    emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                        // delimit the graph prefix without dropping identity records
    emitter.instruction("call __rt_mbstring_restore_ini_array");                // acquire native identity leases during graph construction
    emitter.instruction("jmp __rt_mbstring_status_graph_ready");                // classify the completed root with the common ownership path
    emitter.label("__rt_mbstring_status_graph");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");                       // borrow the completed graph wire bytes
    emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");                       // pass the complete validated byte length
    emitter.instruction("call __rt_mbstring_restore_array");                    // construct independent keys, scalars, and child arrays
    emitter.label("__rt_mbstring_status_graph_ready");
    emitter.instruction("test rax, rax");                                       // check for graph or host construction failure
    emitter.instruction("jz __rt_mbstring_status_fatal");                       // preserve failure after restorer ownership cleanup
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // transfer the restored associative root
    emitter.instruction("mov QWORD PTR [rsp + 56], 0");                         // return a successful result for the completed graph
    emitter.instruction("call __rt_heap_kind");                                 // classify the completed root without changing ownership
    emitter.instruction("cmp rax, 2");                                          // distinguish indexed roots from associative roots
    emitter.instruction(&format!("mov r10d, {}", RESULT_STRING_ARRAY));         // reuse the indexed Mixed-array result convention
    emitter.instruction(&format!("mov r11d, {}", RESULT_ARRAY));                // preserve the associative graph result convention
    emitter.instruction("cmovne r10, r11");                                     // select the concrete root representation
    emitter.instruction("mov QWORD PTR [rsp + 72], r10");                       // retain the kind for native and eval result boxing
    emitter.instruction("jmp __rt_mbstring_status_release");                    // consume graph wire ownership before returning
    emitter.label("__rt_mbstring_status_array");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");                       // borrow the packed string-array bytes
    emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");                       // pass the complete packed byte length
    emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                        // pass the expected element count
    emitter.instruction(&format!("cmp QWORD PTR [rsp + 72], {}", RESULT_ENCODING_CATALOG)); // distinguish cached catalog results
    emitter.instruction("jne __rt_mbstring_status_array_fresh");                // materialize independent ordinary arrays
    emitter.instruction("call __rt_mbstring_catalog");                          // retain the request's shared encoding catalog
    emitter.instruction("jmp __rt_mbstring_status_array_ready");                // normalize both array result paths together
    emitter.label("__rt_mbstring_status_array_fresh");
    emitter.instruction("call __rt_mbstring_string_array");                     // copy binary elements into owned runtime cells
    emitter.label("__rt_mbstring_status_array_ready");
    emitter.instruction(&format!("mov QWORD PTR [rsp + 72], {}", RESULT_STRING_ARRAY)); // expose the existing owned-array convention
    emitter.instruction("test rax, rax");                                       // detect malformed framing after partial-array cleanup
    emitter.instruction("jz __rt_mbstring_status_fatal");                       // return fatal status without leaking either allocator's storage
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // retain the fresh array across bridge-buffer release
    emitter.instruction("mov QWORD PTR [rsp + 56], 0");                         // record successful array materialization
    emitter.instruction("jmp __rt_mbstring_status_release");                    // release the original packed bridge buffer
    emitter.label("__rt_mbstring_status_error");
    emitter.instruction("mov rdi, rsp");                                        // borrow the complete error result through the C input convention
    emitter.instruction("call __rt_mbstring_exception_chain");                  // preserve binary messages and the complete previous chain
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // failure transfers no successful PHP result
    emitter.instruction("mov QWORD PTR [rsp + 56], rax");                       // preserve the non-unwinding materialization status through release
    emitter.label("__rt_mbstring_status_release");
    emitter.instruction("lea rdi, [rsp + 0]");                                  // pass the complete result to its owning allocator
    emitter.bl_c("elephc_mbstring_release_v1");
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // restore the primary result
    emitter.instruction("mov rdx, QWORD PTR [rsp + 56]");                       // restore the runtime status
    emitter.instruction("mov rcx, QWORD PTR [rsp + 64]");                       // restore the result byte length
    emitter.instruction("mov r8, QWORD PTR [rsp + 72]");                        // restore the result kind for boxed callers
    emitter.instruction("mov rsp, rbp");                                        // release the bridge adapter frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return value/status in rax/rdx and length/kind in rcx/r8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Verifies every supported target joins the same C engine and non-unwinding status entry.
    #[test]
    fn mbstring_bridge_symbols_cover_supported_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_mbstring(&mut emitter, false, true);
            let asm = emitter.output();
            assert!(asm.contains(&target.extern_symbol("elephc_mbstring_call_v1")), "{name}");
            assert!(asm.contains(&target.extern_symbol("elephc_mbstring_release_v1")), "{name}");
            assert!(asm.contains(&target.extern_symbol("elephc_mbstring_exception_at_v1")), "{name}");
            assert!(asm.contains("__rt_mbstring_exception_chain:"), "{name}");
            assert!(asm.contains(&target.extern_symbol("elephc_oniguruma_v1_provider")), "{name}");
            assert!(asm.contains(&target.extern_symbol("elephc_mbstring_regex_provider_v1")), "{name}");
            assert!(asm.contains("__rt_mbregex_init:"), "{name}");
            assert!(asm.contains("__rt_mbstring_status:"), "{name}");
            assert!(!asm.contains("iconv"), "{name}");
        }
    }
}
