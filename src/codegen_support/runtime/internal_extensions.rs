//! Purpose:
//! Emits process-context and wrapper-finalization helpers for native internal extensions.
//! Keeps DOM bridge lifecycle calls target-aware and outside PHP-visible object metadata.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//! - Native-wrapper entries in the generated `_class_destruct_ptrs` table.
//!
//! Key details:
//! - One DOM context is initialized lazily per process and stored as an opaque integer ID.
//! - Wrapper finalization emits the locked flat ABI release request exactly once.
//! - SimpleXML parents release their hidden strong iterator wrapper before native ownership.

use crate::codegen_support::abi;
use crate::codegen_support::callable_descriptor::CALLABLE_DESC_INVOKER_OFFSET;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET,
    TRY_HANDLER_SLOT_SIZE,
};

mod host_loader_args;
mod host_xpath_args;
mod host_xpath_wrappers;

const ABI_VERSION: i64 = 1;
const REQUEST_HEADER_SIZE: i64 = 48;
const RESULT_HEADER_SIZE: usize = 96;
const WRAPPER_RELEASE_OPCODE: i64 = 4110;
const HOST_CALL_RESULT_OFFSET: usize = TRY_HANDLER_SLOT_SIZE;
const HOST_CALL_DESCRIPTOR_OFFSET: usize = HOST_CALL_RESULT_OFFSET + 8;
const HOST_CALL_OPCODE_OFFSET: usize = HOST_CALL_DESCRIPTOR_OFFSET + 8;
const HOST_CALL_REQUEST_OFFSET: usize = HOST_CALL_OPCODE_OFFSET + 8;
const HOST_CALL_REQUEST_LENGTH_OFFSET: usize = HOST_CALL_REQUEST_OFFSET + 8;
const HOST_CALL_ARGS_OFFSET: usize = HOST_CALL_REQUEST_LENGTH_OFFSET + 8;
const HOST_CALL_CONTEXT_OFFSET: usize = HOST_CALL_ARGS_OFFSET + 8;
const HOST_CALL_BOXED_ARGS_OFFSET: usize = HOST_CALL_CONTEXT_OFFSET + 8;
const HOST_CALL_BOXED_RESULT_OFFSET: usize = HOST_CALL_BOXED_ARGS_OFFSET + 8;
const HOST_CALL_USER_DATA_OFFSET: usize = HOST_CALL_BOXED_RESULT_OFFSET + 8;
const HOST_CALL_SAVED_FRAME_OFFSET: usize = 400;
const HOST_CALL_FRAME_SIZE: usize = HOST_CALL_SAVED_FRAME_OFFSET + 16;
const HOST_EXTERNAL_LOADER_VALUE_COUNT: i64 = 7;
const HOST_EXTERNAL_LOADER_FIXED_SIZE: i64 = 216;
const HOST_STREAM_OPEN_VALUE_COUNT: i64 = 3;
const HOST_STREAM_OPEN_FIXED_SIZE: i64 = 120;
pub(super) const DOM_XPATH_OBJECT_TYPE_ERROR: &str =
    "Only objects that are instances of DOM nodes can be converted to an XPath expression";

/// Emits all runtime helpers required by native DOM wrapper calls and finalization.
pub(crate) fn emit_dom_runtime(emitter: &mut Emitter) {
    debug_assert_eq!(RESULT_HEADER_SIZE, 96);
    emit_host_call(emitter);
    host_loader_args::emit(emitter);
    host_xpath_args::emit(emitter);
    host_xpath_wrappers::emit(emitter);
    emit_context_ensure(emitter);
    emit_copy_bytes(emitter);
    emit_wrapper_finalize(emitter);
    emit_bridge_failure(emitter);
}

/// Emits the generic native-to-PHP host callback used for retained callable ownership.
fn emit_host_call(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_host_call_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: DOM native-to-PHP host callback ---");
    emitter.label_global("__rt_dom_host_call");
    emitter.instruction(&format!("sub sp, sp, #{}", HOST_CALL_FRAME_SIZE));     // reserve one exception boundary plus callback scratch storage
    emitter.instruction(&format!(
        "stp x29, x30, [sp, #{}]",
        HOST_CALL_SAVED_FRAME_OFFSET
    ));                                                                         // preserve the caller frame across ownership helpers
    emitter.instruction("mov x29, sp");                                         // establish the host callback frame
    emitter.instruction("ldr x9, [x0]");                                        // resolve the current process-local DOM context ID
    emitter.instruction(&format!(
        "str x9, [sp, #{}]",
        HOST_CALL_USER_DATA_OFFSET
    ));                                                                         // preserve the context ID passed indirectly as host user data
    emitter.instruction(&format!("str x3, [sp, #{}]", HOST_CALL_RESULT_OFFSET)); // retain the caller-owned result pointer
    emitter.instruction(&format!("str xzr, [sp, #{}]", HOST_CALL_ARGS_OFFSET)); // initialize optional loader cleanup state
    emitter.instruction(&format!("str xzr, [sp, #{}]", HOST_CALL_BOXED_ARGS_OFFSET)); // initialize the boxed loader-argument owner
    emitter.instruction(&format!("str xzr, [sp, #{}]", HOST_CALL_BOXED_RESULT_OFFSET)); // initialize the optional leased callback result
    emitter.instruction("cbz x1, __rt_dom_host_call_fail");                     // reject a null request pointer
    emitter.instruction("cbz x3, __rt_dom_host_call_fail_no_result");           // reject a null result pointer without dereferencing it
    emitter.instruction(&format!("str x1, [sp, #{}]", HOST_CALL_REQUEST_OFFSET)); // preserve the request pointer across host helpers
    emitter.instruction(&format!("str x2, [sp, #{}]", HOST_CALL_REQUEST_LENGTH_OFFSET)); // preserve the complete request length
    emitter.instruction("cmp x2, #48");                                         // does the request contain at least its padded header?
    emitter.instruction("b.lo __rt_dom_host_call_fail");                        // reject a truncated host request
    emitter.instruction("ldr w9, [x1]");                                        // load the host request ABI version
    emitter.instruction("cmp w9, #1");                                          // is the request encoded with ABI v1?
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject an incompatible host ABI
    emitter.instruction("ldr w9, [x1, #4]");                                    // load the padded request header size
    emitter.instruction("cmp w9, #48");                                         // must the value section begin at byte 48?
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject a shifted value section
    emitter.instruction("ldr w9, [x1, #8]");                                    // load the generic host opcode
    emitter.instruction(&format!("str w9, [sp, #{}]", HOST_CALL_OPCODE_OFFSET)); // preserve it across validation and setjmp
    emitter.instruction("cmp w9, #3");                                          // is this an external-entity loader invocation?
    emitter.instruction("b.eq __rt_dom_host_call_loader_validate");             // validate and invoke the retained resolver
    emitter.instruction("cmp w9, #5");                                          // is this a bounded leased-stream read?
    emitter.instruction("b.eq __rt_dom_host_call_stream_read_validate");        // validate and read one parser input chunk
    emitter.instruction("cmp w9, #6");                                          // is this a PHP stream open for native document I/O?
    emitter.instruction("b.eq __rt_dom_host_call_stream_open_validate");        // validate and open the requested PHP stream
    emitter.instruction("cmp w9, #7");                                          // is this a bounded leased-stream write?
    emitter.instruction("b.eq __rt_dom_host_call_stream_write_validate");       // validate and write one document output chunk
    emitter.instruction("cmp w9, #8");                                          // is this a leased-stream flush?
    emitter.instruction("b.eq __rt_dom_host_call_stream_flush_validate");       // flush document output before releasing its stream
    emitter.instruction("cmp w9, #9");                                          // is this a suppressible PHP warning?
    emitter.instruction("b.eq __rt_dom_host_call_warning_validate");            // validate and emit the formatted warning bytes
    emitter.instruction("cmp w9, #10");                                         // is this a registered XPath callback invocation?
    emitter.instruction("b.eq __rt_dom_host_call_xpath_validate");              // validate, box, and invoke the retained callback
    emitter.instruction("cmp w9, #11");                                         // is this one PHP callable-name resolution?
    emitter.instruction("b.eq __rt_dom_host_call_xpath_resolve_validate");      // validate and resolve the requested callable name
    emitter.instruction("cmp x2, #72");                                         // does an ownership request contain one padded value?
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject truncated or extended ownership messages
    emitter.instruction("ldr x9, [x1, #24]");                                   // load the host request value count
    emitter.instruction("cmp x9, #1");                                          // ownership requests carry exactly one value
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject a malformed value count
    emitter.instruction("ldr x9, [x1, #32]");                                   // load the host request byte count
    emitter.instruction("cbnz x9, __rt_dom_host_call_fail");                    // ownership requests carry no byte section
    emitter.instruction("ldr w9, [x1, #48]");                                   // load the ownership value tag
    emitter.instruction(&format!("ldr w10, [sp, #{}]", HOST_CALL_OPCODE_OFFSET)); // reload the ownership or result-release opcode
    emitter.instruction("cmp w10, #4");                                         // is this a leased callback-result release?
    emitter.instruction("b.eq __rt_dom_host_call_result_tag");                  // require an opaque host-handle value for result release
    emitter.instruction("cmp w9, #9");                                          // tag 9 identifies a PHP callable descriptor
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject other callable ownership values
    emitter.instruction("b __rt_dom_host_call_value_tag_valid");                // continue with the validated callable descriptor
    emitter.label("__rt_dom_host_call_result_tag");
    emitter.instruction("cmp w9, #7");                                          // tag 7 identifies an opaque leased host result
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject mismatched result-release values
    emitter.label("__rt_dom_host_call_value_tag_valid");
    emitter.instruction("ldr x0, [x1, #56]");                                   // load the callable descriptor payload
    emitter.instruction("cbz x0, __rt_dom_host_call_fail");                     // an owned callable or result handle must be non-null
    emitter.instruction(&format!("ldr w9, [sp, #{}]", HOST_CALL_OPCODE_OFFSET)); // reload the host ownership opcode
    emitter.instruction("cmp w9, #1");                                          // is this a callable retain request?
    emitter.instruction("b.eq __rt_dom_host_call_opcode_valid");                // accept the declared callable retain opcode
    emitter.instruction("cmp w9, #2");                                          // is this a callable release request?
    emitter.instruction("b.eq __rt_dom_host_call_opcode_valid");                // accept the declared callable release opcode
    emitter.instruction("cmp w9, #4");                                          // is this a leased callback-result release?
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject unknown host opcodes
    emitter.label("__rt_dom_host_call_opcode_valid");
    emitter.instruction(&format!(
        "str x0, [sp, #{}]",
        HOST_CALL_DESCRIPTOR_OFFSET
    ));                                                                         // preserve the callable descriptor across setjmp
    emit_host_exception_boundary_push(emitter);
    emitter.instruction(&format!(
        "ldr x0, [sp, #{}]",
        HOST_CALL_DESCRIPTOR_OFFSET
    ));                                                                         // restore the callable descriptor for the selected helper
    emitter.instruction(&format!(
        "ldr w9, [sp, #{}]",
        HOST_CALL_OPCODE_OFFSET
    ));                                                                         // restore the validated ownership opcode
    emitter.instruction("cmp w9, #1");                                          // does this request retain the descriptor?
    emitter.instruction("b.eq __rt_dom_host_call_retain");                      // enter the retain helper inside the exception boundary
    emitter.instruction("cmp w9, #2");                                          // does this request release a retained descriptor?
    emitter.instruction("b.eq __rt_dom_host_call_release");                     // drop the callable descriptor owner
    emitter.instruction("b __rt_dom_host_call_release_result");                 // otherwise release the leased boxed callback result
    emitter.label("__rt_dom_host_call_retain");
    emitter.instruction("bl __rt_incref");                                      // add one native-state owner to a dynamic descriptor
    emitter.instruction("b __rt_dom_host_call_success");                        // return a pointer-free successful host result
    emitter.label("__rt_dom_host_call_release");
    emitter.instruction("bl __rt_callable_descriptor_release");                 // drop one native-state callable descriptor owner
    emitter.instruction("b __rt_dom_host_call_success");                        // publish a pointer-free successful release
    emitter.label("__rt_dom_host_call_release_result");
    emitter.instruction("bl __rt_decref_mixed");                                // release the callback result leased to native code
    emitter.label("__rt_dom_host_call_success");
    emit_host_exception_boundary_pop(emitter);
    emitter.instruction(&format!(
        "ldr x9, [sp, #{}]",
        HOST_CALL_RESULT_OFFSET
    ));                                                                         // restore the caller-owned host result pointer
    emit_host_result_init_aarch64(emitter, "x9");
    emitter.instruction("mov w0, wzr");                                         // return STATUS_OK from the host callback
    emitter.instruction("b __rt_dom_host_call_ret");                            // restore the host callback frame
    emit_host_loader_aarch64(emitter);
    emit_host_stream_read_aarch64(emitter);
    emit_host_stream_open_aarch64(emitter);
    emit_host_stream_write_aarch64(emitter);
    emit_host_stream_flush_aarch64(emitter);
    emit_host_warning_aarch64(emitter);
    emit_host_xpath_resolve_aarch64(emitter);
    emit_host_xpath_aarch64(emitter);
    emitter.label("__rt_dom_host_call_throw");
    emit_host_exception_boundary_pop(emitter);
    emitter.instruction(&format!("ldr w11, [sp, #{}]", HOST_CALL_OPCODE_OFFSET)); // identify whether the escaped call owns loader arguments
    emitter.instruction("cmp w11, #6");                                         // did a wrapper stat/open callback throw?
    emitter.instruction("b.ne __rt_dom_host_call_throw_not_open");              // only stream opens publish the temporary context flag
    abi::emit_symbol_address(emitter, "x9", "_dom_stream_context_active");
    emitter.instruction("str xzr, [x9]");                                       // clear the transient context before rethrow transport
    emitter.label("__rt_dom_host_call_throw_not_open");
    emitter.instruction("cmp w11, #3");                                         // did an external-entity resolver throw?
    emitter.instruction("b.eq __rt_dom_host_call_throw_cleanup_args");           // release its constructed argument container
    emitter.instruction("cmp w11, #10");                                        // did a custom XPath callback throw?
    emitter.instruction("b.ne __rt_dom_host_call_throw_publish");               // ownership callbacks have no argument container to release
    emitter.instruction(&format!(
        "ldr x0, [sp, #{}]",
        HOST_CALL_BOXED_RESULT_OFFSET
    ));                                                                         // load an optional callback result created before conversion threw
    emitter.instruction("cbz x0, __rt_dom_host_call_throw_cleanup_args");       // skip a result cell that was never returned
    abi::emit_call_label(emitter, "__rt_decref_mixed");                          // release the unsupported callback result before transport
    emitter.instruction(&format!(
        "str xzr, [sp, #{}]",
        HOST_CALL_BOXED_RESULT_OFFSET
    ));                                                                         // prevent repeated release on shared cleanup paths
    emitter.label("__rt_dom_host_call_throw_cleanup_args");
    emit_host_loader_args_cleanup_aarch64(emitter, "throw");
    emitter.label("__rt_dom_host_call_throw_publish");
    emitter.instruction(&format!(
        "ldr x9, [sp, #{}]",
        HOST_CALL_RESULT_OFFSET
    ));                                                                         // restore the result pointer after the contained longjmp
    emit_host_result_init_aarch64(emitter, "x9");
    emitter.instruction("mov w10, #1");                                         // STATUS_THROW transports a catchable PHP Throwable
    emitter.instruction("str w10, [x9, #8]");                                   // publish the semantic callback failure status
    emitter.instruction("mov w10, #6");                                         // kind 6 preserves the runtime's already-active Throwable
    emitter.instruction("str w10, [x9, #16]");                                  // ask generated code to rethrow the pending host object
    emitter.instruction("mov w0, wzr");                                         // the host ABI transport itself completed successfully
    emitter.instruction("b __rt_dom_host_call_ret");                            // restore the callback frame before returning to Rust
    emitter.label("__rt_dom_host_call_fail");
    emitter.instruction(&format!(
        "ldr x9, [sp, #{}]",
        HOST_CALL_RESULT_OFFSET
    ));                                                                         // restore the non-null host result pointer
    emit_host_result_init_aarch64(emitter, "x9");
    emitter.instruction("mov w10, #3");                                         // STATUS_ABI_ERROR identifies malformed host traffic
    emitter.instruction("str w10, [x9, #8]");                                   // publish the host result failure status
    emitter.instruction("mov w0, #3");                                          // return STATUS_ABI_ERROR to native code
    emitter.instruction("b __rt_dom_host_call_ret");                            // restore the host callback frame
    emitter.label("__rt_dom_host_call_fail_no_result");
    emitter.instruction("mov w0, #3");                                          // reject a null result pointer without writing through it
    emitter.label("__rt_dom_host_call_ret");
    emitter.instruction(&format!(
        "ldp x29, x30, [sp, #{}]",
        HOST_CALL_SAVED_FRAME_OFFSET
    ));                                                                         // restore the native caller frame
    emitter.instruction(&format!("add sp, sp, #{}", HOST_CALL_FRAME_SIZE));     // release the callback boundary and scratch storage
    emitter.instruction("ret");                                                 // return the host callback status
}

/// Validates, invokes, and publishes one external-entity loader host request on AArch64.
fn emit_host_loader_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_dom_host_call_loader_validate");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_REQUEST_OFFSET)); // reload the external-loader request pointer
    emitter.instruction("ldr x10, [x9, #24]");                                  // load the request value count
    emitter.instruction(&format!("cmp x10, #{}", HOST_EXTERNAL_LOADER_VALUE_COUNT)); // require descriptor plus six nullable strings
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject a shifted external-loader shape
    emitter.instruction("ldr x10, [x9, #32]");                                  // load the byte-section length
    emitter.instruction(&format!("add x11, x10, #{}", HOST_EXTERNAL_LOADER_FIXED_SIZE)); // derive the exact complete request length
    emitter.instruction(&format!("ldr x12, [sp, #{}]", HOST_CALL_REQUEST_LENGTH_OFFSET)); // reload the caller-supplied request length
    emitter.instruction("cmp x11, x12");                                        // do header, values, and bytes consume the whole request?
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject truncated or extended loader messages
    emitter.instruction("ldr w11, [x9, #48]");                                  // load the descriptor value tag
    emitter.instruction("cmp w11, #9");                                         // tag 9 identifies a PHP callable descriptor
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject non-callable resolver handles
    emitter.instruction("ldr w11, [x9, #52]");                                  // load descriptor flags
    emitter.instruction("cbnz w11, __rt_dom_host_call_fail");                   // v1 descriptors carry no value flags
    emitter.instruction("ldr x11, [x9, #56]");                                  // load the retained descriptor pointer
    emitter.instruction("cbz x11, __rt_dom_host_call_fail");                    // reject a null resolver descriptor
    emitter.instruction("ldr x12, [x9, #64]");                                  // load the unused descriptor high word
    emitter.instruction("cbnz x12, __rt_dom_host_call_fail");                   // reject malformed descriptor payloads
    emitter.instruction(&format!("str x11, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // preserve the descriptor across argument construction
    for index in 1..HOST_EXTERNAL_LOADER_VALUE_COUNT {
        let value_offset = 48 + index * 24;
        let null_label = format!("__rt_dom_host_call_loader_value_{index}_null");
        let done_label = format!("__rt_dom_host_call_loader_value_{index}_valid");
        emitter.instruction(&format!("ldr w11, [x9, #{}]", value_offset));      // load one nullable loader-string tag
        emitter.instruction(&format!("cbz w11, {}", null_label));               // null parser metadata carries no byte range
        emitter.instruction("cmp w11, #4");                                     // tag 4 identifies a byte-string range
        emitter.instruction("b.ne __rt_dom_host_call_fail");                    // reject non-string parser metadata
        emitter.instruction(&format!("ldr w11, [x9, #{}]", value_offset + 4));  // load this byte value's flags
        emitter.instruction("cbnz w11, __rt_dom_host_call_fail");               // v1 loader strings carry no flags
        emitter.instruction(&format!("ldr x11, [x9, #{}]", value_offset + 8));  // load the byte-section offset
        emitter.instruction(&format!("ldr x12, [x9, #{}]", value_offset + 16)); // load the byte-string length
        emitter.instruction("adds x13, x11, x12");                              // compute the range end with overflow detection
        emitter.instruction("b.cs __rt_dom_host_call_fail");                    // reject an overflowing byte range
        emitter.instruction("cmp x13, x10");                                    // must the range stay inside the request byte section?
        emitter.instruction("b.hi __rt_dom_host_call_fail");                    // reject an out-of-bounds string
        emitter.instruction(&format!("b {}", done_label));                      // continue after validating this string
        emitter.label(&null_label);
        emitter.instruction(&format!("ldr x11, [x9, #{}]", value_offset + 4));  // load flags plus padding for a null value
        emitter.instruction("cbnz x11, __rt_dom_host_call_fail");               // canonical null has zero flags and padding
        emitter.instruction(&format!("ldr x11, [x9, #{}]", value_offset + 8));  // load the null low payload
        emitter.instruction("cbnz x11, __rt_dom_host_call_fail");               // canonical null has no low payload
        emitter.instruction(&format!("ldr x11, [x9, #{}]", value_offset + 16)); // load the null high payload
        emitter.instruction("cbnz x11, __rt_dom_host_call_fail");               // canonical null has no high payload
        emitter.label(&done_label);
    }
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HOST_CALL_REQUEST_OFFSET)); // pass the validated request to the argument builder
    abi::emit_call_label(emitter, "__rt_dom_host_loader_build_args");            // return boxed args in x0 and their raw array in x1
    emitter.instruction(&format!("str x0, [sp, #{}]", HOST_CALL_BOXED_ARGS_OFFSET)); // preserve the boxed argument container
    emitter.instruction(&format!("str x1, [sp, #{}]", HOST_CALL_ARGS_OFFSET));  // preserve its independently owned raw array
    emitter.instruction("cbz x0, __rt_dom_host_call_loader_cleanup_fail");      // reject an allocation helper that returned no boxed arguments
    emitter.instruction("cbz x1, __rt_dom_host_call_loader_cleanup_fail");      // reject an allocation helper that returned no raw array
    emit_host_exception_boundary_push(emitter);
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // invoker arg0 is the retained callable descriptor
    emitter.instruction(&format!("ldr x1, [sp, #{}]", HOST_CALL_BOXED_ARGS_OFFSET)); // invoker arg1 is the boxed three-value argument array
    emitter.instruction(&format!("ldr x9, [x0, #{}]", CALLABLE_DESC_INVOKER_OFFSET)); // load this descriptor's uniform invoker
    emitter.instruction("cbz x9, __rt_dom_host_call_loader_missing_invoker");   // every host-visible descriptor must expose an invoker
    emitter.instruction("blr x9");                                              // invoke resolver(public, system, context)
    emitter.instruction(&format!("str x0, [sp, #{}]", HOST_CALL_BOXED_RESULT_OFFSET)); // lease or inspect the boxed PHP callback result
    emit_host_exception_boundary_pop(emitter);
    emit_host_loader_args_cleanup_aarch64(emitter, "success");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HOST_CALL_BOXED_RESULT_OFFSET)); // reload the boxed callback result
    emitter.instruction("cbz x0, __rt_dom_host_call_fail");                     // reject a missing boxed result
    emitter.instruction("ldr x11, [x0]");                                       // load the runtime result tag
    emitter.instruction("cmp x11, #8");                                         // did the resolver return PHP null?
    emitter.instruction("b.eq __rt_dom_host_call_loader_result_null");          // publish an unleased null result
    emitter.instruction("cmp x11, #1");                                         // did the resolver return a URI string?
    emitter.instruction("b.eq __rt_dom_host_call_loader_result_bytes");         // lease the string while native code copies it
    emitter.instruction("cmp x11, #9");                                         // did the resolver return a stream resource?
    emitter.instruction("b.eq __rt_dom_host_call_loader_result_resource");      // lease its boxed PHP result across native reads
    emitter.instruction("bl __rt_decref_mixed");                                // release an unsupported callback result
    emitter.instruction("b __rt_dom_host_call_fail");                           // contain unsupported result shapes as an ABI failure

    emitter.label("__rt_dom_host_call_loader_result_null");
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the boxed null before publishing it
    emitter.instruction(&format!(
        "ldr x9, [sp, #{}]",
        HOST_CALL_RESULT_OFFSET
    ));                                                                         // restore the caller-owned result header
    emit_host_result_init_aarch64(emitter, "x9");
    emitter.instruction("mov w0, wzr");                                         // report a successful host transport
    emitter.instruction("b __rt_dom_host_call_ret");                            // return the unleased null result

    emitter.label("__rt_dom_host_call_loader_result_bytes");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_RESULT_OFFSET)); // restore the caller-owned result header
    emit_host_result_init_aarch64(emitter, "x9");
    emitter.instruction("mov w11, #4");                                         // public value tag 4 is a byte-string range
    emitter.instruction("str w11, [x9, #12]");                                  // publish the callback string kind
    emitter.instruction("str x0, [x9, #24]");                                   // lease the boxed result as its host result ID
    emitter.instruction("ldr x11, [x0, #8]");                                   // load the persisted callback string pointer
    emitter.instruction("ldr x12, [x0, #16]");                                  // load the persisted callback string length
    emitter.instruction("str x12, [x9, #40]");                                  // mirror the byte length in payload1
    emitter.instruction("str x11, [x9, #48]");                                  // expose borrowed string bytes while the lease is live
    emitter.instruction("str x12, [x9, #56]");                                  // expose the exact borrowed byte count
    emitter.instruction("mov w0, wzr");                                         // report a successful host transport
    emitter.instruction("b __rt_dom_host_call_ret");                            // return the leased byte result

    emitter.label("__rt_dom_host_call_loader_result_resource");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_RESULT_OFFSET)); // restore the caller-owned result header
    emit_host_result_init_aarch64(emitter, "x9");
    emitter.instruction("mov w11, #10");                                        // public value tag 10 is a PHP stream resource
    emitter.instruction("str w11, [x9, #12]");                                  // publish the callback resource kind
    emitter.instruction("str x0, [x9, #24]");                                   // lease the boxed result as its host result ID
    emitter.instruction("ldr x11, [x0, #8]");                                   // load the runtime resource identifier
    emitter.instruction("str x11, [x9, #32]");                                  // expose the opaque resource payload
    emitter.instruction("ldr x12, [x0, #16]");                                  // load the runtime resource-kind discriminator
    emitter.instruction("str x12, [x9, #40]");                                  // expose the kind needed for safe stream reads
    emitter.instruction("mov w0, wzr");                                         // report a successful host transport
    emitter.instruction("b __rt_dom_host_call_ret");                            // return the leased resource

    emitter.label("__rt_dom_host_call_loader_missing_invoker");
    emit_host_exception_boundary_pop(emitter);
    emitter.label("__rt_dom_host_call_loader_cleanup_fail");
    emit_host_loader_args_cleanup_aarch64(emitter, "failure");
    emitter.instruction("b __rt_dom_host_call_fail");                           // reject a descriptor without its uniform invoker
}

/// Validates, invokes, and publishes one custom XPath callback on AArch64.
fn emit_host_xpath_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_dom_host_call_xpath_validate");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_REQUEST_OFFSET)); // reload the XPath callback request pointer
    emitter.instruction("ldr x10, [x9, #24]");                                  // load descriptor-plus-argument value count
    emitter.instruction("cmp x10, #1");                                         // every callback request starts with one descriptor
    emitter.instruction("b.lo __rt_dom_host_call_fail");                        // reject a request without its callable descriptor
    emitter.instruction("ldr w14, [x9, #12]");                                  // load the root-count marker and callback request flags
    emitter.instruction("tbz w14, #31, __rt_dom_host_call_fail");               // XPath callback trees require an explicit root count
    emitter.instruction("and w14, w14, #0x7fffffff");                           // isolate descriptor-plus-visible-argument roots
    emitter.instruction("cmp x14, #1");                                         // every callback tree retains one descriptor root
    emitter.instruction("b.lo __rt_dom_host_call_fail");                        // reject a missing descriptor root
    emitter.instruction("cmp x14, x10");                                        // must every root fit inside the complete flat value section?
    emitter.instruction("b.hi __rt_dom_host_call_fail");                        // reject a root count beyond the declared values
    emitter.instruction(&format!(
        "str x14, [sp, #{}]",
        HOST_CALL_CONTEXT_OFFSET
    ));                                                                         // preserve the root bound across tree validation
    emitter.instruction(&format!("str x14, [sp, #{}]", HOST_CALL_ARGS_OFFSET)); // descendants must begin immediately after all roots
    emitter.instruction(&format!("ldr x11, [sp, #{}]", HOST_CALL_REQUEST_LENGTH_OFFSET)); // reload the complete request length
    emitter.instruction("cmp x11, #72");                                        // require the header and descriptor record
    emitter.instruction("b.lo __rt_dom_host_call_fail");                        // reject a truncated descriptor request
    emitter.instruction("sub x12, x11, #48");                                   // isolate all bytes available after the fixed header
    emitter.instruction("mov x13, #24");                                        // each flat value occupies twenty-four bytes
    emitter.instruction("udiv x12, x12, x13");                                  // derive the maximum value count that can fit
    emitter.instruction("cmp x10, x12");                                        // does the declared value section fit the request?
    emitter.instruction("b.hi __rt_dom_host_call_fail");                        // reject multiplication overflow or truncation
    emitter.instruction("mov x12, #48");                                        // begin with the padded request header size
    emitter.instruction("madd x12, x10, x13, x12");                             // derive the dynamic byte-section start
    emitter.instruction("ldr x13, [x9, #32]");                                  // load the declared callback byte count
    emitter.instruction("adds x14, x12, x13");                                  // derive the exact complete request size
    emitter.instruction("b.cs __rt_dom_host_call_fail");                        // reject an overflowing byte-section length
    emitter.instruction("cmp x14, x11");                                        // do header, values, and bytes consume the request?
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject truncated or extended callback messages
    emitter.instruction("ldr w11, [x9, #48]");                                  // load the first value tag
    emitter.instruction("cmp w11, #9");                                         // public tag nine identifies a callable descriptor
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject a callback request without a descriptor
    emitter.instruction("ldr w11, [x9, #52]");                                  // load callable descriptor flags
    emitter.instruction("cbnz w11, __rt_dom_host_call_fail");                   // ABI v1 descriptors carry no flags
    emitter.instruction("ldr x11, [x9, #56]");                                  // load the retained callable descriptor pointer
    emitter.instruction("cbz x11, __rt_dom_host_call_fail");                    // reject a null callable descriptor
    emitter.instruction("ldr x14, [x9, #64]");                                  // load the unused descriptor high word
    emitter.instruction("cbnz x14, __rt_dom_host_call_fail");                   // require the canonical zero high payload
    emitter.instruction(&format!("str x11, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // preserve the descriptor across argument construction
    emitter.instruction("mov x14, #1");                                         // begin validation after the descriptor record
    emitter.label("__rt_dom_host_call_xpath_value_loop");
    emitter.instruction(&format!(
        "ldr x17, [sp, #{}]",
        HOST_CALL_CONTEXT_OFFSET
    ));                                                                         // reload the descriptor-plus-visible-root bound
    emitter.instruction("cmp x14, x17");                                        // have all root argument records been validated?
    emitter.instruction("b.hs __rt_dom_host_call_xpath_values_valid");           // continue after the complete argument prefix
    emitter.instruction("mov x15, #24");                                        // each value record occupies twenty-four bytes
    emitter.instruction("mov x16, #48");                                        // values begin after the padded request header
    emitter.instruction("madd x15, x14, x15, x16");                             // compute this value record's request offset
    emitter.instruction("add x15, x9, x15");                                    // address the selected callback argument
    emitter.instruction("ldr w16, [x15, #4]");                                  // load the ABI value flags
    emitter.instruction("cbnz w16, __rt_dom_host_call_fail");                   // callback scalar values use canonical zero flags
    emitter.instruction("ldr w16, [x15]");                                      // load the public callback argument tag
    emitter.instruction("cbz w16, __rt_dom_host_call_xpath_value_null");        // validate canonical PHP null
    emitter.instruction("cmp w16, #1");                                         // public tag one identifies a boolean
    emitter.instruction("b.eq __rt_dom_host_call_xpath_value_bool");             // validate the zero-or-one payload
    emitter.instruction("cmp w16, #3");                                         // public tag three identifies a double
    emitter.instruction("b.eq __rt_dom_host_call_xpath_value_float");            // validate its unused high payload
    emitter.instruction("cmp w16, #5");                                         // public tag five identifies a nested node-set array
    emitter.instruction("b.eq __rt_dom_host_call_xpath_value_nodes");           // validate its contiguous bridge-handle descendants
    emitter.instruction("cmp w16, #4");                                         // public tag four identifies request bytes
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject unsupported XPath callback argument shapes
    emitter.instruction("ldr x16, [x15, #8]");                                  // load this byte range's section offset
    emitter.instruction("ldr x17, [x15, #16]");                                 // load this byte range's exact length
    emitter.instruction("adds x16, x16, x17");                                  // derive the byte range end
    emitter.instruction("b.cs __rt_dom_host_call_fail");                        // reject an overflowing byte range
    emitter.instruction("cmp x16, x13");                                        // must the range remain inside the byte section?
    emitter.instruction("b.hi __rt_dom_host_call_fail");                        // reject an out-of-bounds XPath string
    emitter.instruction("b __rt_dom_host_call_xpath_value_valid");              // continue with the next callback argument
    emitter.label("__rt_dom_host_call_xpath_value_null");
    emitter.instruction("ldr x16, [x15, #8]");                                  // load the null low payload
    emitter.instruction("ldr x17, [x15, #16]");                                 // load the null high payload
    emitter.instruction("orr x16, x16, x17");                                   // combine both unused null payload words
    emitter.instruction("cbnz x16, __rt_dom_host_call_fail");                   // canonical null carries two zero payloads
    emitter.instruction("b __rt_dom_host_call_xpath_value_valid");              // continue after validating null
    emitter.label("__rt_dom_host_call_xpath_value_bool");
    emitter.instruction("ldr x16, [x15, #8]");                                  // load the boolean low payload
    emitter.instruction("cmp x16, #1");                                         // is the boolean canonical zero or one?
    emitter.instruction("b.hi __rt_dom_host_call_fail");                        // reject non-canonical boolean payloads
    emitter.instruction("ldr x17, [x15, #16]");                                 // load the unused boolean high payload
    emitter.instruction("cbnz x17, __rt_dom_host_call_fail");                   // canonical booleans use no high word
    emitter.instruction("b __rt_dom_host_call_xpath_value_valid");              // continue after validating the boolean
    emitter.label("__rt_dom_host_call_xpath_value_float");
    emitter.instruction("ldr x16, [x15, #16]");                                 // load the unused floating-point high payload
    emitter.instruction("cbnz x16, __rt_dom_host_call_fail");                   // doubles use only their exact low-word bit pattern
    emitter.instruction("b __rt_dom_host_call_xpath_value_valid");              // continue after validating the double
    emitter.label("__rt_dom_host_call_xpath_value_nodes");
    emitter.instruction("ldr x16, [x15, #8]");                                  // load the nested node-value range start
    emitter.instruction(&format!("ldr x12, [sp, #{}]", HOST_CALL_ARGS_OFFSET)); // reload the next required descendant offset
    emitter.instruction("cmp x16, x12");                                        // must node arrays partition descendants without gaps?
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject overlapping, orphaned, or reordered descendants
    emitter.instruction("ldr x17, [x15, #16]");                                 // load the nested node-value count
    emitter.instruction("adds x17, x16, x17");                                  // derive the exclusive descendant range end
    emitter.instruction("b.cs __rt_dom_host_call_fail");                        // reject an overflowing node-value range
    emitter.instruction("cmp x17, x10");                                        // must the complete range fit the declared flat values?
    emitter.instruction("b.hi __rt_dom_host_call_fail");                        // reject node handles outside the request
    emitter.instruction(&format!("str x17, [sp, #{}]", HOST_CALL_ARGS_OFFSET)); // require the next array to follow this range
    emitter.instruction("mov x12, x16");                                        // begin validating this node array at its first descendant
    emitter.label("__rt_dom_host_call_xpath_node_loop");
    emitter.instruction("cmp x12, x17");                                        // have all node handle records been validated?
    emitter.instruction("b.hs __rt_dom_host_call_xpath_value_valid");           // return to the root loop after this array
    emitter.instruction("mov x15, #24");                                        // each descendant occupies one flat value record
    emitter.instruction("mov x16, #48");                                        // the complete flat value section follows the header
    emitter.instruction("madd x15, x12, x15, x16");                             // compute this descendant record's request offset
    emitter.instruction("add x15, x9, x15");                                    // address the selected bridge-handle record
    emitter.instruction("ldr w16, [x15]");                                      // load the descendant public value tag
    emitter.instruction("cmp w16, #8");                                         // public tag eight identifies a native bridge handle
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // node arrays contain only canonical DOM wrapper handles
    emitter.instruction("ldr w16, [x15, #4]");                                  // load the descendant value flags
    emitter.instruction("cbnz w16, __rt_dom_host_call_fail");                   // ABI v1 bridge handles carry no flags
    emitter.instruction("ldr x16, [x15, #8]");                                  // load the generation-checked native handle
    emitter.instruction("cbz x16, __rt_dom_host_call_fail");                    // reject null node handles
    emitter.instruction("ldr x16, [x15, #16]");                                 // load the stable concrete wrapper kind
    emitter.instruction("cbz x16, __rt_dom_host_call_fail");                    // every node argument requires a concrete wrapper kind
    emitter.instruction("add x12, x12, #1");                                    // advance to the next node descendant
    emitter.instruction("b __rt_dom_host_call_xpath_node_loop");                // validate the remaining node handles
    emitter.label("__rt_dom_host_call_xpath_value_valid");
    emitter.instruction("add x14, x14, #1");                                    // advance to the next argument record
    emitter.instruction("b __rt_dom_host_call_xpath_value_loop");               // validate the remaining callback arguments
    emitter.label("__rt_dom_host_call_xpath_values_valid");
    emitter.instruction(&format!("ldr x12, [sp, #{}]", HOST_CALL_ARGS_OFFSET)); // reload the first unused descendant offset
    emitter.instruction("cmp x12, x10");                                        // did root arrays account for every trailing value?
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject orphaned flat descendants
    emitter.instruction("mov x0, x9");                                          // pass the validated request to the argument builder
    emitter.instruction(&format!(
        "ldr x1, [sp, #{}]",
        HOST_CALL_USER_DATA_OFFSET
    ));                                                                         // pass the owning DOM context ID for node wrappers
    abi::emit_call_label(emitter, "__rt_dom_host_xpath_build_args");             // return boxed args in x0 and their raw array in x1
    emitter.instruction(&format!("str x0, [sp, #{}]", HOST_CALL_BOXED_ARGS_OFFSET)); // preserve the boxed argument container
    emitter.instruction(&format!("str x1, [sp, #{}]", HOST_CALL_ARGS_OFFSET));   // preserve its independently owned raw array
    emitter.instruction("cbz x0, __rt_dom_host_call_xpath_cleanup_fail");       // reject a missing boxed argument container
    emitter.instruction("cbz x1, __rt_dom_host_call_xpath_cleanup_fail");       // reject a missing raw callback argument array
    emit_host_exception_boundary_push(emitter);
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // invoker arg0 is the retained callable descriptor
    emitter.instruction(&format!("ldr x1, [sp, #{}]", HOST_CALL_BOXED_ARGS_OFFSET)); // invoker arg1 is the boxed source-order argument array
    emitter.instruction(&format!("ldr x9, [x0, #{}]", CALLABLE_DESC_INVOKER_OFFSET)); // load this descriptor's uniform invoker
    emitter.instruction("cbz x9, __rt_dom_host_call_xpath_missing_invoker");     // every host-visible descriptor must expose an invoker
    emitter.instruction("blr x9");                                              // invoke the registered custom XPath callback
    emitter.instruction(&format!("str x0, [sp, #{}]", HOST_CALL_BOXED_RESULT_OFFSET)); // preserve the boxed PHP callback result
    emitter.instruction("cbz x0, __rt_dom_host_call_xpath_missing_result");     // reject a missing callback result cell
    emitter.instruction("ldr x9, [x0]");                                        // inspect the runtime tag before scalar string conversion
    emitter.instruction("cmp x9, #3");                                          // runtime tag three is a PHP boolean
    emitter.instruction("b.eq __rt_dom_host_call_xpath_result_bool");            // preserve booleans as native XPath booleans
    emitter.instruction("cmp x9, #6");                                          // runtime tag six is a PHP object
    emitter.instruction("b.eq __rt_dom_host_call_xpath_result_object");         // accept only canonical DOM node wrappers
    abi::emit_call_label(emitter, "__rt_mixed_cast_string");                     // apply PHP scalar-to-string callback result semantics
    abi::emit_call_label(emitter, "__rt_str_persist");                           // own the possibly borrowed string while releasing its cell
    emitter.instruction(&format!("str x1, [sp, #{}]", HOST_CALL_CONTEXT_OFFSET)); // preserve the independently owned string pointer
    emitter.instruction("mov x0, #1");                                          // runtime tag one boxes a PHP string
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // create the native result lease's boxed string
    emitter.instruction(&format!("str x0, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // preserve the leased string box
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HOST_CALL_CONTEXT_OFFSET)); // reload the extra persisted string owner
    abi::emit_call_label(emitter, "__rt_decref_any");                           // leave the new Mixed box as sole string owner
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HOST_CALL_BOXED_RESULT_OFFSET)); // reload the original callback result cell
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the callback result after string persistence
    emitter.instruction(&format!("str xzr, [sp, #{}]", HOST_CALL_BOXED_RESULT_OFFSET)); // prevent exceptional cleanup from seeing a released cell
    emit_host_exception_boundary_pop(emitter);
    emit_host_loader_args_cleanup_aarch64(emitter, "xpath_success");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // reload the leased boxed callback string
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_RESULT_OFFSET)); // restore the caller-owned result header
    emit_host_result_init_aarch64(emitter, "x9");
    emitter.instruction("mov w11, #4");                                         // public value tag four is a byte-string range
    emitter.instruction("str w11, [x9, #12]");                                  // publish the XPath callback string kind
    emitter.instruction("str x0, [x9, #24]");                                   // lease the boxed string as its host result ID
    emitter.instruction("ldr x11, [x0, #8]");                                   // load the persisted callback string pointer
    emitter.instruction("ldr x12, [x0, #16]");                                  // load the persisted callback string length
    emitter.instruction("str x12, [x9, #40]");                                  // mirror the byte length in payload1
    emitter.instruction("str x11, [x9, #48]");                                  // expose borrowed string bytes while the lease is live
    emitter.instruction("str x12, [x9, #56]");                                  // expose the exact borrowed byte count
    emitter.instruction("mov w0, wzr");                                         // report a successful host transport
    emitter.instruction("b __rt_dom_host_call_ret");                            // return the leased XPath callback result
    emitter.label("__rt_dom_host_call_xpath_result_object");
    emitter.instruction(&format!(
        "ldr x1, [sp, #{}]",
        HOST_CALL_USER_DATA_OFFSET
    ));                                                                         // validator arg1 is the owning DOM bridge context
    abi::emit_call_label(emitter, "__rt_dom_host_xpath_result_handle");          // return the validated native bridge handle or zero
    emitter.instruction(
        "cbz x0, __rt_dom_host_call_xpath_result_object_type_error",
    );                                                                          // ordinary PHP objects cannot become XPath expressions
    emitter.instruction(&format!(
        "str x0, [sp, #{}]",
        HOST_CALL_DESCRIPTOR_OFFSET
    ));                                                                         // preserve the bridge handle across argument cleanup
    emit_host_exception_boundary_pop(emitter);
    emit_host_loader_args_cleanup_aarch64(emitter, "xpath_node");
    emitter.instruction(&format!(
        "ldr x0, [sp, #{}]",
        HOST_CALL_BOXED_RESULT_OFFSET
    ));                                                                         // reload the leased boxed DOM callback result
    emitter.instruction(&format!(
        "ldr x9, [sp, #{}]",
        HOST_CALL_RESULT_OFFSET
    ));                                                                         // restore the caller-owned result header
    emit_host_result_init_aarch64(emitter, "x9");
    emitter.instruction("mov w11, #8");                                         // public value tag eight identifies a native bridge handle
    emitter.instruction("str w11, [x9, #12]");                                  // publish the XPath callback node kind
    emitter.instruction("str x0, [x9, #24]");                                   // lease the original boxed DOM callback result
    emitter.instruction(&format!(
        "ldr x10, [sp, #{}]",
        HOST_CALL_DESCRIPTOR_OFFSET
    ));                                                                         // reload the generation-checked bridge handle
    emitter.instruction("str x10, [x9, #32]");                                  // expose the native bridge handle to Rust
    emitter.instruction("mov w0, wzr");                                         // report a successful host transport
    emitter.instruction("b __rt_dom_host_call_ret");                            // return the leased XPath callback node
    emitter.label("__rt_dom_host_call_xpath_result_object_type_error");
    emit_xpath_object_type_error_aarch64(emitter);
    emitter.label("__rt_dom_host_call_xpath_result_bool");
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the canonical callback boolean payload
    emitter.instruction(&format!("str x10, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // preserve it across result and argument cleanup
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the boxed boolean callback result
    emitter.instruction(&format!("str xzr, [sp, #{}]", HOST_CALL_BOXED_RESULT_OFFSET)); // mark the callback result as released
    emit_host_exception_boundary_pop(emitter);
    emit_host_loader_args_cleanup_aarch64(emitter, "xpath_bool");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_RESULT_OFFSET)); // restore the caller-owned result header
    emit_host_result_init_aarch64(emitter, "x9");
    emitter.instruction(&format!("ldr x10, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // reload the canonical boolean payload after result initialization
    emitter.instruction("mov w11, #1");                                         // public value tag one is a PHP/XPath boolean
    emitter.instruction("str w11, [x9, #12]");                                  // publish the callback boolean kind
    emitter.instruction("str x10, [x9, #32]");                                  // expose the canonical zero-or-one payload
    emitter.instruction("mov w0, wzr");                                         // report a successful pointer-free host transport
    emitter.instruction("b __rt_dom_host_call_ret");                            // return the XPath boolean callback result
    emitter.label("__rt_dom_host_call_xpath_missing_result");
    emit_host_exception_boundary_pop(emitter);
    emitter.instruction("b __rt_dom_host_call_xpath_cleanup_fail");             // clean the argument container before ABI failure
    emitter.label("__rt_dom_host_call_xpath_missing_invoker");
    emit_host_exception_boundary_pop(emitter);
    emitter.label("__rt_dom_host_call_xpath_cleanup_fail");
    emit_host_loader_args_cleanup_aarch64(emitter, "xpath_failure");
    emitter.instruction("b __rt_dom_host_call_fail");                           // contain malformed callback state as an ABI failure
}

/// Validates and reads one bounded chunk from a leased PHP stream on AArch64.
fn emit_host_stream_read_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_dom_host_call_stream_read_validate");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_REQUEST_OFFSET)); // reload the stream-read request pointer
    emitter.instruction(&format!("ldr x10, [sp, #{}]", HOST_CALL_REQUEST_LENGTH_OFFSET)); // reload the complete request length
    emitter.instruction("cmp x10, #96");                                        // require a padded header plus resource and length values
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject truncated or extended stream-read messages
    emitter.instruction("ldr x10, [x9, #24]");                                  // load the declared value count
    emitter.instruction("cmp x10, #2");                                         // stream reads carry resource and maximum length
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject a shifted stream-read value section
    emitter.instruction("ldr x10, [x9, #32]");                                  // load the byte-section length
    emitter.instruction("cbnz x10, __rt_dom_host_call_fail");                   // stream-read requests carry no bytes
    emitter.instruction("ldr w10, [x9, #48]");                                  // load the first value tag
    emitter.instruction("cmp w10, #10");                                        // public tag ten identifies a PHP resource
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject non-resource stream handles
    emitter.instruction("ldr w10, [x9, #52]");                                  // load resource flags
    emitter.instruction("cbnz w10, __rt_dom_host_call_fail");                   // v1 resources carry no flags
    emitter.instruction("ldr x10, [x9, #56]");                                  // load the opaque runtime stream descriptor
    emitter.instruction(&format!("str x10, [sp, #{}]", HOST_CALL_CONTEXT_OFFSET)); // preserve the descriptor across fread
    emitter.instruction("ldr x11, [x9, #64]");                                  // load the resource-kind discriminator
    emitter.instruction("cbz x11, __rt_dom_host_call_stream_kind_valid");       // kind zero is a legacy stream resource
    emitter.instruction("cmp x11, #1");                                         // kind one is an ordinary fd-backed stream
    emitter.instruction("b.eq __rt_dom_host_call_stream_kind_valid");           // accept ordinary streams
    emitter.instruction("cmp x11, #3");                                         // kind three is a readable popen pipe fd
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject non-stream resources such as hashes/directories
    emitter.label("__rt_dom_host_call_stream_kind_valid");
    emitter.instruction("ldr w10, [x9, #72]");                                  // load the maximum-length value tag
    emitter.instruction("cmp w10, #2");                                         // public tag two identifies an integer
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject non-integer read limits
    emitter.instruction("ldr w10, [x9, #76]");                                  // load integer flags
    emitter.instruction("cbnz w10, __rt_dom_host_call_fail");                   // v1 integers carry no flags
    emitter.instruction("ldr x10, [x9, #80]");                                  // load the requested maximum byte count
    emitter.instruction("cbz x10, __rt_dom_host_call_fail");                    // native read callbacks always request a positive length
    emitter.instruction("mov x11, #0x7fffffff");                                // libxml2 read callbacks use signed-int capacities
    emitter.instruction("cmp x10, x11");                                        // does the request fit the native callback contract?
    emitter.instruction("b.hi __rt_dom_host_call_fail");                        // reject oversized read requests
    emitter.instruction("ldr x11, [x9, #88]");                                  // load the unused integer high word
    emitter.instruction("cbnz x11, __rt_dom_host_call_fail");                   // canonical integers have no high payload
    emitter.instruction(&format!("str x10, [sp, #{}]", HOST_CALL_ARGS_OFFSET)); // preserve the maximum length across setjmp
    emit_host_exception_boundary_push(emitter);
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HOST_CALL_CONTEXT_OFFSET)); // fread arg0 is the runtime stream descriptor
    emitter.instruction(&format!("ldr x1, [sp, #{}]", HOST_CALL_ARGS_OFFSET));  // fread arg1 is the bounded byte count
    abi::emit_call_label(emitter, "__rt_fread");                                // return one owned-or-borrowed chunk in x1/x2
    emitter.instruction(&format!("str x1, [sp, #{}]", HOST_CALL_CONTEXT_OFFSET)); // preserve the chunk pointer across persistence
    emitter.instruction(&format!("str x2, [sp, #{}]", HOST_CALL_ARGS_OFFSET));  // preserve the exact returned chunk length
    emitter.instruction("cbz x2, __rt_dom_host_call_stream_read_empty");        // publish EOF without allocating a lease
    emitter.instruction("mov x0, #1");                                          // runtime tag one boxes a persisted PHP string
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // copy and lease the stream chunk for Rust
    emitter.instruction(&format!("str x0, [sp, #{}]", HOST_CALL_BOXED_RESULT_OFFSET)); // preserve the leased result box
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HOST_CALL_CONTEXT_OFFSET)); // reload the original fread chunk owner
    abi::emit_call_label(emitter, "__rt_decref_any");                           // release heap-backed wrapper/filter chunks
    emit_host_exception_boundary_pop(emitter);
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HOST_CALL_BOXED_RESULT_OFFSET)); // reload the leased string result
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_RESULT_OFFSET)); // restore the caller-owned result header
    emit_host_result_init_aarch64(emitter, "x9");
    emitter.instruction("mov w11, #4");                                         // public tag four identifies returned bytes
    emitter.instruction("str w11, [x9, #12]");                                  // publish the stream-read value kind
    emitter.instruction("str x0, [x9, #24]");                                   // lease the boxed string as its result ID
    emitter.instruction("ldr x11, [x0, #8]");                                   // load the persistent result bytes
    emitter.instruction("ldr x12, [x0, #16]");                                  // load the persistent result length
    emitter.instruction("str x12, [x9, #40]");                                  // mirror the byte length in payload1
    emitter.instruction("str x11, [x9, #48]");                                  // expose borrowed bytes while the lease is live
    emitter.instruction("str x12, [x9, #56]");                                  // publish the exact byte count
    emitter.instruction("mov w0, wzr");                                         // report successful host transport
    emitter.instruction("b __rt_dom_host_call_ret");                            // return the leased stream chunk
    emitter.label("__rt_dom_host_call_stream_read_empty");
    emitter.instruction("mov x0, x1");                                          // pass the terminal empty chunk to uniform cleanup
    abi::emit_call_label(emitter, "__rt_decref_any");                           // release a heap-backed empty wrapper result
    emit_host_exception_boundary_pop(emitter);
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_RESULT_OFFSET)); // restore the caller-owned result header
    emit_host_result_init_aarch64(emitter, "x9");
    emitter.instruction("mov w11, #4");                                         // EOF is an unleased empty byte string
    emitter.instruction("str w11, [x9, #12]");                                  // publish the empty byte value kind
    emitter.instruction("mov w0, wzr");                                         // report successful EOF transport
    emitter.instruction("b __rt_dom_host_call_ret");                            // return the empty stream chunk
}

/// Releases the external-loader argument container if its slots were initialized on AArch64.
fn emit_host_loader_args_cleanup_aarch64(emitter: &mut Emitter, suffix: &str) {
    let skip_boxed =
        format!("__rt_dom_host_call_loader_cleanup_boxed_skip_{suffix}");
    let skip_raw =
        format!("__rt_dom_host_call_loader_cleanup_raw_skip_{suffix}");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HOST_CALL_BOXED_ARGS_OFFSET)); // load the optional boxed argument container
    emitter.instruction(&format!("cbz x0, {}", skip_boxed));                    // skip an argument box that was never built
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // drop the box's ownership of the raw argument array
    emitter.instruction(&format!("str xzr, [sp, #{}]", HOST_CALL_BOXED_ARGS_OFFSET)); // prevent repeated cleanup after a contained throw
    emitter.label(&skip_boxed);
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HOST_CALL_ARGS_OFFSET));  // load the optional raw argument array
    emitter.instruction(&format!("cbz x0, {}", skip_raw));                      // skip an array that was never built
    abi::emit_call_label(emitter, "__rt_decref_any");                           // release the raw array and its three boxed values
    emitter.instruction(&format!("str xzr, [sp, #{}]", HOST_CALL_ARGS_OFFSET)); // prevent repeated cleanup on shared failure paths
    emitter.label(&skip_raw);
}

/// Installs a native exception sentinel before one host callback enters PHP-owned code.
fn emit_host_exception_boundary_push(emitter: &mut Emitter) {
    emitter.comment("push DOM host callback exception boundary");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_handler_top", 0);
            emitter.instruction("str x10, [sp]");                               // save the previous native exception-handler head
            abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_call_frame_top", 0);
            emitter.instruction("str x10, [sp, #8]");                           // preserve the caller activation frame across callback unwinding
            abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_diag_suppression", 0);
            emitter.instruction(&format!(
                "str x10, [sp, #{}]",
                TRY_HANDLER_DIAG_DEPTH_OFFSET
            ));                                                                 // retain the caller's diagnostic-suppression depth
            emitter.instruction("mov x10, sp");                                 // address the callback boundary handler record
            abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
            emitter.instruction(&format!(
                "add x0, sp, #{}",
                TRY_HANDLER_JMP_BUF_OFFSET
            ));                                                                 // pass the boundary jmp_buf to setjmp
            emitter.bl_c("setjmp");                                             // snapshot the host callback stack before entering PHP
            emitter.instruction("cbnz x0, __rt_dom_host_call_throw");           // contain a Throwable instead of unwinding through Rust
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0);
            emitter.instruction(&format!(
                "mov QWORD PTR [rbp - {}], r10",
                HOST_CALL_FRAME_SIZE
            ));                                                                 // save the previous native exception-handler head
            abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_call_frame_top", 0);
            emitter.instruction(&format!(
                "mov QWORD PTR [rbp - {}], r10",
                HOST_CALL_FRAME_SIZE - 8
            ));                                                                 // preserve the caller activation frame across callback unwinding
            abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);
            emitter.instruction(&format!(
                "mov QWORD PTR [rbp - {}], r10",
                HOST_CALL_FRAME_SIZE - TRY_HANDLER_DIAG_DEPTH_OFFSET
            ));                                                                 // retain the caller's diagnostic-suppression depth
            emitter.instruction(&format!(
                "lea r10, [rbp - {}]",
                HOST_CALL_FRAME_SIZE
            ));                                                                 // address the callback boundary handler record
            abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
            emitter.instruction(&format!(
                "lea rdi, [rbp - {}]",
                HOST_CALL_FRAME_SIZE - TRY_HANDLER_JMP_BUF_OFFSET
            ));                                                                 // pass the boundary jmp_buf to setjmp
            emitter.bl_c("setjmp");                                             // snapshot the host callback stack before entering PHP
            emitter.instruction("test eax, eax");                               // did control return through longjmp?
            emitter.instruction("jne __rt_dom_host_call_throw");                // contain a Throwable instead of unwinding through Rust
        }
    }
}

/// Restores the exception and diagnostic chains after a successful or escaped host callback.
fn emit_host_exception_boundary_pop(emitter: &mut Emitter) {
    emitter.comment("pop DOM host callback exception boundary");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x10, [sp]");                               // reload the previous native exception-handler head
            abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
            emitter.instruction(&format!(
                "ldr x10, [sp, #{}]",
                TRY_HANDLER_DIAG_DEPTH_OFFSET
            ));                                                                 // reload the caller's diagnostic-suppression depth
            abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0);
        }
        Arch::X86_64 => {
            emitter.instruction(&format!(
                "mov r10, QWORD PTR [rbp - {}]",
                HOST_CALL_FRAME_SIZE
            ));                                                                 // reload the previous native exception-handler head
            abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
            emitter.instruction(&format!(
                "mov r10, QWORD PTR [rbp - {}]",
                HOST_CALL_FRAME_SIZE - TRY_HANDLER_DIAG_DEPTH_OFFSET
            ));                                                                 // reload the caller's diagnostic-suppression depth
            abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);
        }
    }
}

/// Initializes one AArch64 host result header as a successful pointer-free null value.
fn emit_host_result_init_aarch64(emitter: &mut Emitter, result_reg: &str) {
    emitter.instruction("mov w10, #1");                                         // host result ABI version is one
    emitter.instruction(&format!("str w10, [{}]", result_reg));                 // publish the host result ABI version
    emitter.instruction("mov w10, #96");                                        // host result headers contain twelve words
    emitter.instruction(&format!("str w10, [{}, #4]", result_reg));             // publish the exact result struct size
    for offset in (8..96).step_by(8) {
        emitter.instruction(&format!("str xzr, [{}, #{}]", result_reg, offset)); // clear one host result word
    }
}

/// Emits the Linux x86_64 native-to-PHP ownership host callback.
fn emit_host_call_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: DOM native-to-PHP host callback ---");
    emitter.label_global("__rt_dom_host_call");
    emitter.instruction("push rbp");                                            // preserve the native caller frame
    emitter.instruction("mov rbp, rsp");                                        // establish the host callback frame
    emitter.instruction(&format!("sub rsp, {}", HOST_CALL_FRAME_SIZE));         // reserve one exception boundary plus callback scratch storage
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // resolve the current process-local DOM context ID
    emitter.instruction(&format!(
        "mov QWORD PTR [rbp - {}], rax",
        HOST_CALL_USER_DATA_OFFSET
    ));                                                                         // preserve the context ID passed indirectly as host user data
    emitter.instruction("mov QWORD PTR [rbp - 8], rcx");                        // retain the host result pointer across ownership helpers
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize optional loader cleanup state
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // initialize the boxed loader-argument owner
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // initialize the optional leased callback result
    emitter.instruction("test rsi, rsi");                                       // is the host request pointer non-null?
    emitter.instruction("jz __rt_dom_host_call_fail");                          // reject a null request pointer
    emitter.instruction("test rcx, rcx");                                       // is the host result pointer non-null?
    emitter.instruction("jz __rt_dom_host_call_fail_no_result");                // reject null without dereferencing it
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // preserve the request pointer across host helpers
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // preserve the complete request length
    emitter.instruction("cmp rdx, 48");                                         // does the request contain at least its padded header?
    emitter.instruction("jb __rt_dom_host_call_fail");                          // reject a truncated host request
    emitter.instruction("cmp DWORD PTR [rsi], 1");                              // is the host request ABI version one?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject an incompatible host ABI
    emitter.instruction("cmp DWORD PTR [rsi + 4], 48");                         // must the value section begin at byte 48?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject a shifted value section
    emitter.instruction("mov eax, DWORD PTR [rsi + 8]");                        // load the generic host opcode
    emitter.instruction("mov DWORD PTR [rbp - 24], eax");                       // preserve it across validation and setjmp
    emitter.instruction("cmp eax, 3");                                          // is this an external-entity loader invocation?
    emitter.instruction("je __rt_dom_host_call_loader_validate");               // validate and invoke the retained resolver
    emitter.instruction("cmp eax, 5");                                          // is this a bounded leased-stream read?
    emitter.instruction("je __rt_dom_host_call_stream_read_validate");          // validate and read one parser input chunk
    emitter.instruction("cmp eax, 6");                                          // is this a PHP stream open for native document I/O?
    emitter.instruction("je __rt_dom_host_call_stream_open_validate");          // validate and open the requested PHP stream
    emitter.instruction("cmp eax, 7");                                          // is this a bounded leased-stream write?
    emitter.instruction("je __rt_dom_host_call_stream_write_validate");         // validate and write one document output chunk
    emitter.instruction("cmp eax, 8");                                          // is this a leased-stream flush?
    emitter.instruction("je __rt_dom_host_call_stream_flush_validate");         // flush document output before releasing its stream
    emitter.instruction("cmp eax, 9");                                          // is this a suppressible PHP warning?
    emitter.instruction("je __rt_dom_host_call_warning_validate");              // validate and emit the formatted warning bytes
    emitter.instruction("cmp eax, 10");                                         // is this a registered XPath callback invocation?
    emitter.instruction("je __rt_dom_host_call_xpath_validate");                // validate, box, and invoke the retained callback
    emitter.instruction("cmp eax, 11");                                         // is this one PHP callable-name resolution?
    emitter.instruction("je __rt_dom_host_call_xpath_resolve_validate");        // validate and resolve the requested callable name
    emitter.instruction("cmp rdx, 72");                                         // does an ownership request contain one padded value?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject truncated or extended messages
    emitter.instruction("cmp QWORD PTR [rsi + 24], 1");                         // ownership requests carry exactly one value
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject a malformed value count
    emitter.instruction("cmp QWORD PTR [rsi + 32], 0");                         // ownership requests carry no byte section
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject unexpected host bytes
    emitter.instruction("mov eax, DWORD PTR [rsi + 48]");                       // load the ownership value tag
    emitter.instruction("cmp DWORD PTR [rbp - 24], 4");                         // is this a leased callback-result release?
    emitter.instruction("je __rt_dom_host_call_result_tag");                    // require an opaque host-handle value for result release
    emitter.instruction("cmp eax, 9");                                          // tag 9 identifies a PHP callable descriptor
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject other callable ownership values
    emitter.instruction("jmp __rt_dom_host_call_value_tag_valid");              // continue with the validated callable descriptor
    emitter.label("__rt_dom_host_call_result_tag");
    emitter.instruction("cmp eax, 7");                                          // tag 7 identifies an opaque leased host result
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject mismatched result-release values
    emitter.label("__rt_dom_host_call_value_tag_valid");
    emitter.instruction("mov rax, QWORD PTR [rsi + 56]");                       // load the callable descriptor payload
    emitter.instruction("test rax, rax");                                       // is the retained callable descriptor non-null?
    emitter.instruction("jz __rt_dom_host_call_fail");                          // reject a null callable handle
    emitter.instruction("cmp DWORD PTR [rbp - 24], 1");                         // is this a callable retain request?
    emitter.instruction("je __rt_dom_host_call_opcode_valid");                  // accept the declared callable retain opcode
    emitter.instruction("cmp DWORD PTR [rbp - 24], 2");                         // is this a callable release request?
    emitter.instruction("je __rt_dom_host_call_opcode_valid");                  // accept the declared callable release opcode
    emitter.instruction("cmp DWORD PTR [rbp - 24], 4");                         // is this a leased callback-result release?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject unknown host opcodes
    emitter.label("__rt_dom_host_call_opcode_valid");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the callable descriptor across setjmp
    emit_host_exception_boundary_push(emitter);
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // restore the callable descriptor for the selected helper
    emitter.instruction("cmp DWORD PTR [rbp - 24], 1");                         // does this request retain the descriptor?
    emitter.instruction("je __rt_dom_host_call_retain");                        // enter the retain helper inside the exception boundary
    emitter.instruction("cmp DWORD PTR [rbp - 24], 2");                         // does this request release a retained descriptor?
    emitter.instruction("je __rt_dom_host_call_release");                       // drop the callable descriptor owner
    emitter.instruction("jmp __rt_dom_host_call_release_result");               // otherwise release the leased boxed callback result
    emitter.label("__rt_dom_host_call_retain");
    emitter.instruction("call __rt_incref");                                    // add one native-state owner to a dynamic descriptor
    emitter.instruction("jmp __rt_dom_host_call_success");                      // return a pointer-free successful host result
    emitter.label("__rt_dom_host_call_release");
    emitter.instruction("call __rt_callable_descriptor_release");               // drop one native-state callable descriptor owner
    emitter.instruction("jmp __rt_dom_host_call_success");                      // publish a pointer-free successful release
    emitter.label("__rt_dom_host_call_release_result");
    emitter.instruction("call __rt_decref_mixed");                              // release the callback result leased to native code
    emitter.label("__rt_dom_host_call_success");
    emit_host_exception_boundary_pop(emitter);
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the host result pointer
    emit_host_result_init_x86_64(emitter, "r9");
    emitter.instruction("xor eax, eax");                                        // return STATUS_OK from the host callback
    emitter.instruction("jmp __rt_dom_host_call_ret");                          // restore the host callback frame
    emit_host_loader_x86_64(emitter);
    emit_host_stream_read_x86_64(emitter);
    emit_host_stream_open_x86_64(emitter);
    emit_host_stream_write_x86_64(emitter);
    emit_host_stream_flush_x86_64(emitter);
    emit_host_warning_x86_64(emitter);
    emit_host_xpath_resolve_x86_64(emitter);
    emit_host_xpath_x86_64(emitter);
    emitter.label("__rt_dom_host_call_throw");
    emit_host_exception_boundary_pop(emitter);
    emitter.instruction("cmp DWORD PTR [rbp - 24], 6");                         // did a wrapper stat/open callback throw?
    emitter.instruction("jne __rt_dom_host_call_throw_not_open_x86");           // only stream opens publish the temporary context flag
    abi::emit_store_zero_to_symbol(emitter, "_dom_stream_context_active", 0);
    emitter.label("__rt_dom_host_call_throw_not_open_x86");
    emitter.instruction("cmp DWORD PTR [rbp - 24], 3");                         // did an external-entity resolver throw?
    emitter.instruction("je __rt_dom_host_call_throw_cleanup_args_x86");        // release its constructed argument container
    emitter.instruction("cmp DWORD PTR [rbp - 24], 10");                        // did a custom XPath callback throw?
    emitter.instruction("jne __rt_dom_host_call_throw_publish");                // ownership callbacks have no argument container to release
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // load an optional callback result created before conversion threw
    emitter.instruction("test rax, rax");                                       // was one callback result cell returned?
    emitter.instruction("jz __rt_dom_host_call_throw_cleanup_args_x86");        // skip a result cell that was never created
    abi::emit_call_label(emitter, "__rt_decref_mixed");                          // release the unsupported callback result before transport
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // prevent repeated release on shared cleanup paths
    emitter.label("__rt_dom_host_call_throw_cleanup_args_x86");
    emit_host_loader_args_cleanup_x86_64(emitter, "throw");
    emitter.label("__rt_dom_host_call_throw_publish");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the result pointer after the contained longjmp
    emit_host_result_init_x86_64(emitter, "r9");
    emitter.instruction("mov DWORD PTR [r9 + 8], 1");                           // publish STATUS_THROW for the PHP callback failure
    emitter.instruction("mov DWORD PTR [r9 + 16], 6");                          // ask generated code to rethrow the active host Throwable
    emitter.instruction("xor eax, eax");                                        // the host ABI transport itself completed successfully
    emitter.instruction("jmp __rt_dom_host_call_ret");                          // restore the callback frame before returning to Rust
    emitter.label("__rt_dom_host_call_fail");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the non-null host result pointer
    emit_host_result_init_x86_64(emitter, "r9");
    emitter.instruction("mov DWORD PTR [r9 + 8], 3");                           // publish STATUS_ABI_ERROR in the result
    emitter.instruction("mov eax, 3");                                          // return STATUS_ABI_ERROR to native code
    emitter.instruction("jmp __rt_dom_host_call_ret");                          // restore the host callback frame
    emitter.label("__rt_dom_host_call_fail_no_result");
    emitter.instruction("mov eax, 3");                                          // reject null output without writing through it
    emitter.label("__rt_dom_host_call_ret");
    emitter.instruction("mov rsp, rbp");                                        // discard host callback scratch storage
    emitter.instruction("pop rbp");                                             // restore the native caller frame
    emitter.instruction("ret");                                                 // return the host callback status
}

/// Validates, invokes, and publishes one external-entity loader host request on x86_64.
fn emit_host_loader_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_dom_host_call_loader_validate");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the external-loader request pointer
    emitter.instruction(&format!("cmp QWORD PTR [r10 + 24], {}", HOST_EXTERNAL_LOADER_VALUE_COUNT)); // require descriptor plus six nullable strings
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject a shifted external-loader shape
    emitter.instruction("mov r11, QWORD PTR [r10 + 32]");                       // load the byte-section length
    emitter.instruction(&format!("add r11, {}", HOST_EXTERNAL_LOADER_FIXED_SIZE)); // derive the exact complete request length
    emitter.instruction("cmp r11, QWORD PTR [rbp - 40]");                       // do header, values, and bytes consume the whole request?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject truncated or extended loader messages
    emitter.instruction("cmp DWORD PTR [r10 + 48], 9");                         // tag 9 identifies a PHP callable descriptor
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject non-callable resolver handles
    emitter.instruction("cmp DWORD PTR [r10 + 52], 0");                         // do descriptor flags use the v1 zero form?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject flagged resolver descriptors
    emitter.instruction("mov r11, QWORD PTR [r10 + 56]");                       // load the retained descriptor pointer
    emitter.instruction("test r11, r11");                                       // is the resolver descriptor non-null?
    emitter.instruction("jz __rt_dom_host_call_fail");                          // reject a null resolver descriptor
    emitter.instruction("cmp QWORD PTR [r10 + 64], 0");                         // is the unused descriptor high word zero?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject malformed descriptor payloads
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // preserve the descriptor across argument construction
    for index in 1..HOST_EXTERNAL_LOADER_VALUE_COUNT {
        let value_offset = 48 + index * 24;
        let null_label = format!("__rt_dom_host_call_loader_value_{index}_null");
        let done_label = format!("__rt_dom_host_call_loader_value_{index}_valid");
        emitter.instruction(&format!("mov eax, DWORD PTR [r10 + {}]", value_offset)); // load one nullable loader-string tag
        emitter.instruction("test eax, eax");                                   // is this parser metadata value PHP null?
        emitter.instruction(&format!("jz {}", null_label));                     // null metadata carries no byte range
        emitter.instruction("cmp eax, 4");                                      // tag 4 identifies a byte-string range
        emitter.instruction("jne __rt_dom_host_call_fail");                     // reject non-string parser metadata
        emitter.instruction(&format!("cmp DWORD PTR [r10 + {}], 0", value_offset + 4)); // do the byte value flags use the v1 zero form?
        emitter.instruction("jne __rt_dom_host_call_fail");                     // reject flagged loader strings
        emitter.instruction(&format!("mov rax, QWORD PTR [r10 + {}]", value_offset + 8)); // load the byte-section offset
        emitter.instruction(&format!("add rax, QWORD PTR [r10 + {}]", value_offset + 16)); // compute the byte range end
        emitter.instruction("jc __rt_dom_host_call_fail");                      // reject an overflowing byte range
        emitter.instruction("cmp rax, QWORD PTR [r10 + 32]");                   // must the range stay inside the request byte section?
        emitter.instruction("ja __rt_dom_host_call_fail");                      // reject an out-of-bounds string
        emitter.instruction(&format!("jmp {}", done_label));                    // continue after validating this string
        emitter.label(&null_label);
        emitter.instruction(&format!("cmp QWORD PTR [r10 + {}], 0", value_offset + 4)); // are null flags and padding canonical?
        emitter.instruction("jne __rt_dom_host_call_fail");                     // canonical null has zero flags and padding
        emitter.instruction(&format!("cmp QWORD PTR [r10 + {}], 0", value_offset + 8)); // is the null low payload zero?
        emitter.instruction("jne __rt_dom_host_call_fail");                     // canonical null has no low payload
        emitter.instruction(&format!("cmp QWORD PTR [r10 + {}], 0", value_offset + 16)); // is the null high payload zero?
        emitter.instruction("jne __rt_dom_host_call_fail");                     // canonical null has no high payload
        emitter.label(&done_label);
    }
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the validated request to the argument builder
    abi::emit_call_label(emitter, "__rt_dom_host_loader_build_args");            // return boxed args in rax and their raw array in rdi
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // preserve the boxed argument container
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // preserve its independently owned raw array
    emitter.instruction("test rax, rax");                                       // did argument construction return a box?
    emitter.instruction("jz __rt_dom_host_call_loader_cleanup_fail");           // reject a missing boxed argument container
    emitter.instruction("test rdi, rdi");                                       // did argument construction return its raw array?
    emitter.instruction("jz __rt_dom_host_call_loader_cleanup_fail");           // reject a missing raw argument array
    emit_host_exception_boundary_push(emitter);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // invoker arg0 is the retained callable descriptor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // invoker arg1 is the boxed three-value argument array
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", CALLABLE_DESC_INVOKER_OFFSET)); // load this descriptor's uniform invoker
    emitter.instruction("test r10, r10");                                       // does this descriptor expose an invoker?
    emitter.instruction("jz __rt_dom_host_call_loader_missing_invoker");        // every host-visible descriptor must expose one
    emitter.instruction("call r10");                                            // invoke resolver(public, system, context)
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // lease or inspect the boxed PHP callback result
    emit_host_exception_boundary_pop(emitter);
    emit_host_loader_args_cleanup_x86_64(emitter, "success");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload the boxed callback result
    emitter.instruction("test rax, rax");                                       // did the invoker return a boxed value?
    emitter.instruction("jz __rt_dom_host_call_fail");                          // reject a missing boxed result
    emitter.instruction("cmp QWORD PTR [rax], 8");                              // did the resolver return PHP null?
    emitter.instruction("je __rt_dom_host_call_loader_result_null");            // publish an unleased null result
    emitter.instruction("cmp QWORD PTR [rax], 1");                              // did the resolver return a URI string?
    emitter.instruction("je __rt_dom_host_call_loader_result_bytes");           // lease the string while native code copies it
    emitter.instruction("cmp QWORD PTR [rax], 9");                              // did the resolver return a stream resource?
    emitter.instruction("je __rt_dom_host_call_loader_result_resource");        // lease its boxed PHP result across native reads
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release an unsupported callback result
    emitter.instruction("jmp __rt_dom_host_call_fail");                         // contain unsupported result shapes as an ABI failure

    emitter.label("__rt_dom_host_call_loader_result_null");
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the boxed null before publishing it
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the caller-owned result header
    emit_host_result_init_x86_64(emitter, "r9");
    emitter.instruction("xor eax, eax");                                        // report a successful host transport
    emitter.instruction("jmp __rt_dom_host_call_ret");                          // return the unleased null result

    emitter.label("__rt_dom_host_call_loader_result_bytes");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the caller-owned result header
    emit_host_result_init_x86_64(emitter, "r9");
    emitter.instruction("mov DWORD PTR [r9 + 12], 4");                          // public value tag 4 is a byte-string range
    emitter.instruction("mov QWORD PTR [r9 + 24], rax");                        // lease the boxed result as its host result ID
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load the persisted callback string pointer
    emitter.instruction("mov r11, QWORD PTR [rax + 16]");                       // load the persisted callback string length
    emitter.instruction("mov QWORD PTR [r9 + 40], r11");                        // mirror the byte length in payload1
    emitter.instruction("mov QWORD PTR [r9 + 48], r10");                        // expose borrowed string bytes while the lease is live
    emitter.instruction("mov QWORD PTR [r9 + 56], r11");                        // expose the exact borrowed byte count
    emitter.instruction("xor eax, eax");                                        // report a successful host transport
    emitter.instruction("jmp __rt_dom_host_call_ret");                          // return the leased byte result

    emitter.label("__rt_dom_host_call_loader_result_resource");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the caller-owned result header
    emit_host_result_init_x86_64(emitter, "r9");
    emitter.instruction("mov DWORD PTR [r9 + 12], 10");                         // public value tag 10 is a PHP stream resource
    emitter.instruction("mov QWORD PTR [r9 + 24], rax");                        // lease the boxed result as its host result ID
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load the runtime resource identifier
    emitter.instruction("mov QWORD PTR [r9 + 32], r10");                        // expose the opaque resource payload
    emitter.instruction("mov r11, QWORD PTR [rax + 16]");                       // load the runtime resource-kind discriminator
    emitter.instruction("mov QWORD PTR [r9 + 40], r11");                        // expose the kind needed for safe stream reads
    emitter.instruction("xor eax, eax");                                        // report a successful host transport
    emitter.instruction("jmp __rt_dom_host_call_ret");                          // return the leased resource

    emitter.label("__rt_dom_host_call_loader_missing_invoker");
    emit_host_exception_boundary_pop(emitter);
    emitter.label("__rt_dom_host_call_loader_cleanup_fail");
    emit_host_loader_args_cleanup_x86_64(emitter, "failure");
    emitter.instruction("jmp __rt_dom_host_call_fail");                         // reject a descriptor without its uniform invoker
}

/// Validates, invokes, and publishes one custom XPath callback on x86_64.
fn emit_host_xpath_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_dom_host_call_xpath_validate");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the XPath callback request pointer
    emitter.instruction("mov r11, QWORD PTR [r10 + 24]");                       // load descriptor-plus-argument value count
    emitter.instruction("cmp r11, 1");                                          // every callback request starts with one descriptor
    emitter.instruction("jb __rt_dom_host_call_fail");                          // reject a request without its callable descriptor
    emitter.instruction("mov r8d, DWORD PTR [r10 + 12]");                       // load the root-count marker and callback request flags
    emitter.instruction("test r8d, 0x80000000");                                // does this callback tree declare its visible roots?
    emitter.instruction("jz __rt_dom_host_call_fail");                          // nested XPath values require an explicit root count
    emitter.instruction("and r8d, 0x7fffffff");                                 // isolate descriptor-plus-visible-argument roots
    emitter.instruction("cmp r8, 1");                                           // every callback tree retains one descriptor root
    emitter.instruction("jb __rt_dom_host_call_fail");                          // reject a missing descriptor root
    emitter.instruction("cmp r8, r11");                                         // must every root fit inside the complete flat value section?
    emitter.instruction("ja __rt_dom_host_call_fail");                          // reject a root count beyond the declared values
    emitter.instruction("mov QWORD PTR [rbp - 56], r8");                        // preserve the root bound across tree validation
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                        // descendants must begin immediately after all roots
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the complete request length
    emitter.instruction("cmp rax, 72");                                         // require the header and descriptor record
    emitter.instruction("jb __rt_dom_host_call_fail");                          // reject a truncated descriptor request
    emitter.instruction("sub rax, 48");                                         // isolate all bytes available after the fixed header
    emitter.instruction("xor edx, edx");                                        // prepare unsigned division by one value-record size
    emitter.instruction("mov r9, 24");                                           // each flat value occupies twenty-four bytes
    emitter.instruction("div r9");                                               // derive the maximum value count that can fit
    emitter.instruction("cmp r11, rax");                                        // does the declared value section fit the request?
    emitter.instruction("ja __rt_dom_host_call_fail");                          // reject multiplication overflow or truncation
    emitter.instruction("imul rax, r11, 24");                                   // compute the declared value-section size
    emitter.instruction("add rax, 48");                                         // derive the dynamic byte-section start
    emitter.instruction("mov r9, QWORD PTR [r10 + 32]");                        // load the declared callback byte count
    emitter.instruction("add rax, r9");                                         // derive the exact complete request size
    emitter.instruction("jc __rt_dom_host_call_fail");                          // reject an overflowing byte-section length
    emitter.instruction("cmp rax, QWORD PTR [rbp - 40]");                       // do header, values, and bytes consume the request?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject truncated or extended callback messages
    emitter.instruction("cmp DWORD PTR [r10 + 48], 9");                         // public tag nine identifies a callable descriptor
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject a callback request without a descriptor
    emitter.instruction("cmp DWORD PTR [r10 + 52], 0");                         // do descriptor flags use the ABI v1 zero form?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject flagged callable descriptors
    emitter.instruction("mov rax, QWORD PTR [r10 + 56]");                       // load the retained callable descriptor pointer
    emitter.instruction("test rax, rax");                                       // is the descriptor non-null?
    emitter.instruction("jz __rt_dom_host_call_fail");                          // reject a null callable descriptor
    emitter.instruction("cmp QWORD PTR [r10 + 64], 0");                         // is the unused descriptor high word zero?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject malformed descriptor payloads
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the descriptor across argument construction
    emitter.instruction("mov r8, 1");                                           // begin validation after the descriptor record
    emitter.label("__rt_dom_host_call_xpath_value_loop");
    emitter.instruction("cmp r8, QWORD PTR [rbp - 56]");                        // have all root argument records been validated?
    emitter.instruction("jae __rt_dom_host_call_xpath_values_valid");           // continue after the complete argument prefix
    emitter.instruction("imul rax, r8, 24");                                    // compute this value record's displacement
    emitter.instruction("add rax, 48");                                         // values begin after the padded request header
    emitter.instruction("add rax, r10");                                        // address the selected callback argument
    emitter.instruction("cmp DWORD PTR [rax + 4], 0");                          // do the scalar ABI flags use the canonical zero form?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject flagged callback scalar values
    emitter.instruction("mov ecx, DWORD PTR [rax]");                            // load the public callback argument tag
    emitter.instruction("test ecx, ecx");                                       // is this canonical PHP null?
    emitter.instruction("jz __rt_dom_host_call_xpath_value_null");              // validate both unused null payloads
    emitter.instruction("cmp ecx, 1");                                          // public tag one identifies a boolean
    emitter.instruction("je __rt_dom_host_call_xpath_value_bool");              // validate the zero-or-one payload
    emitter.instruction("cmp ecx, 3");                                          // public tag three identifies a double
    emitter.instruction("je __rt_dom_host_call_xpath_value_float");             // validate its unused high payload
    emitter.instruction("cmp ecx, 5");                                          // public tag five identifies a nested node-set array
    emitter.instruction("je __rt_dom_host_call_xpath_value_nodes");             // validate its contiguous bridge-handle descendants
    emitter.instruction("cmp ecx, 4");                                          // public tag four identifies request bytes
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject unsupported XPath callback argument shapes
    emitter.instruction("mov rcx, QWORD PTR [rax + 8]");                        // load this byte range's section offset
    emitter.instruction("add rcx, QWORD PTR [rax + 16]");                       // derive this byte range's end
    emitter.instruction("jc __rt_dom_host_call_fail");                          // reject an overflowing byte range
    emitter.instruction("cmp rcx, r9");                                         // must the range remain inside the byte section?
    emitter.instruction("ja __rt_dom_host_call_fail");                          // reject an out-of-bounds XPath string
    emitter.instruction("jmp __rt_dom_host_call_xpath_value_valid");            // continue with the next callback argument
    emitter.label("__rt_dom_host_call_xpath_value_null");
    emitter.instruction("mov rcx, QWORD PTR [rax + 8]");                        // load the null low payload
    emitter.instruction("or rcx, QWORD PTR [rax + 16]");                        // combine both unused null payload words
    emitter.instruction("jnz __rt_dom_host_call_fail");                         // canonical null carries two zero payloads
    emitter.instruction("jmp __rt_dom_host_call_xpath_value_valid");            // continue after validating null
    emitter.label("__rt_dom_host_call_xpath_value_bool");
    emitter.instruction("cmp QWORD PTR [rax + 8], 1");                          // is the boolean canonical zero or one?
    emitter.instruction("ja __rt_dom_host_call_fail");                          // reject non-canonical boolean payloads
    emitter.instruction("cmp QWORD PTR [rax + 16], 0");                         // is the unused boolean high payload zero?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject a malformed boolean value
    emitter.instruction("jmp __rt_dom_host_call_xpath_value_valid");            // continue after validating the boolean
    emitter.label("__rt_dom_host_call_xpath_value_float");
    emitter.instruction("cmp QWORD PTR [rax + 16], 0");                         // is the unused floating-point high payload zero?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // doubles use only their exact low-word bit pattern
    emitter.instruction("jmp __rt_dom_host_call_xpath_value_valid");            // continue after validating the double
    emitter.label("__rt_dom_host_call_xpath_value_nodes");
    emitter.instruction("mov rcx, QWORD PTR [rax + 8]");                        // load the nested node-value range start
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 48]");                       // must node arrays partition descendants without gaps?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject overlapping, orphaned, or reordered descendants
    emitter.instruction("mov rdx, rcx");                                        // begin deriving the exclusive descendant range end
    emitter.instruction("add rdx, QWORD PTR [rax + 16]");                       // add the nested node-value count
    emitter.instruction("jc __rt_dom_host_call_fail");                          // reject an overflowing node-value range
    emitter.instruction("cmp rdx, r11");                                        // must the complete range fit the declared flat values?
    emitter.instruction("ja __rt_dom_host_call_fail");                          // reject node handles outside the request
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // require the next array to follow this range
    emitter.label("__rt_dom_host_call_xpath_node_loop");
    emitter.instruction("cmp rcx, rdx");                                        // have all node handle records been validated?
    emitter.instruction("jae __rt_dom_host_call_xpath_value_valid");            // return to the root loop after this array
    emitter.instruction("imul rax, rcx, 24");                                   // compute this descendant record's displacement
    emitter.instruction("add rax, 48");                                         // the complete flat value section follows the header
    emitter.instruction("add rax, r10");                                        // address the selected bridge-handle record
    emitter.instruction("cmp DWORD PTR [rax], 8");                              // public tag eight identifies a native bridge handle
    emitter.instruction("jne __rt_dom_host_call_fail");                         // node arrays contain only canonical DOM wrapper handles
    emitter.instruction("cmp DWORD PTR [rax + 4], 0");                          // do descendant flags use the canonical ABI v1 form?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // bridge handles carry no flags
    emitter.instruction("cmp QWORD PTR [rax + 8], 0");                          // is the generation-checked native handle non-null?
    emitter.instruction("je __rt_dom_host_call_fail");                          // reject null node handles
    emitter.instruction("cmp QWORD PTR [rax + 16], 0");                         // is a stable concrete wrapper kind present?
    emitter.instruction("je __rt_dom_host_call_fail");                          // every node argument needs a concrete wrapper kind
    emitter.instruction("add rcx, 1");                                          // advance to the next node descendant
    emitter.instruction("jmp __rt_dom_host_call_xpath_node_loop");              // validate the remaining node handles
    emitter.label("__rt_dom_host_call_xpath_value_valid");
    emitter.instruction("add r8, 1");                                           // advance to the next argument record
    emitter.instruction("jmp __rt_dom_host_call_xpath_value_loop");             // validate the remaining callback arguments
    emitter.label("__rt_dom_host_call_xpath_values_valid");
    emitter.instruction("cmp QWORD PTR [rbp - 48], r11");                       // did root arrays account for every trailing value?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject orphaned flat descendants
    emitter.instruction("mov rdi, r10");                                        // pass the validated request to the argument builder
    emitter.instruction(&format!(
        "mov rsi, QWORD PTR [rbp - {}]",
        HOST_CALL_USER_DATA_OFFSET
    ));                                                                         // pass the owning DOM context ID for node wrappers
    abi::emit_call_label(emitter, "__rt_dom_host_xpath_build_args");             // return boxed args in rax and their raw array in rdi
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // preserve the boxed argument container
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // preserve its independently owned raw array
    emitter.instruction("test rax, rax");                                       // did argument construction return a box?
    emitter.instruction("jz __rt_dom_host_call_xpath_cleanup_fail");            // reject a missing boxed argument container
    emitter.instruction("test rdi, rdi");                                       // did argument construction return its raw array?
    emitter.instruction("jz __rt_dom_host_call_xpath_cleanup_fail");            // reject a missing raw callback argument array
    emit_host_exception_boundary_push(emitter);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // invoker arg0 is the retained callable descriptor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // invoker arg1 is the boxed source-order argument array
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", CALLABLE_DESC_INVOKER_OFFSET)); // load this descriptor's uniform invoker
    emitter.instruction("test r10, r10");                                       // does this descriptor expose an invoker?
    emitter.instruction("jz __rt_dom_host_call_xpath_missing_invoker");         // every host-visible descriptor must expose one
    emitter.instruction("call r10");                                            // invoke the registered custom XPath callback
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // preserve the boxed PHP callback result
    emitter.instruction("test rax, rax");                                       // did the callback return one boxed value?
    emitter.instruction("jz __rt_dom_host_call_xpath_missing_result");          // reject a missing callback result cell
    emitter.instruction("cmp QWORD PTR [rax], 3");                              // is the result a runtime PHP boolean?
    emitter.instruction("je __rt_dom_host_call_xpath_result_bool");             // preserve booleans as native XPath booleans
    emitter.instruction("cmp QWORD PTR [rax], 6");                              // is the result a runtime PHP object?
    emitter.instruction("je __rt_dom_host_call_xpath_result_object");           // accept only canonical DOM node wrappers
    abi::emit_call_label(emitter, "__rt_mixed_cast_string");                     // apply PHP scalar-to-string callback result semantics
    abi::emit_call_label(emitter, "__rt_str_persist");                           // own the possibly borrowed string while releasing its cell
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // preserve the independently owned string pointer
    emitter.instruction("mov rdi, rax");                                        // Mixed payload low word is the persisted string pointer
    emitter.instruction("mov rsi, rdx");                                        // Mixed payload high word is the exact string length
    emitter.instruction("mov eax, 1");                                          // runtime tag one boxes a PHP string
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // create the native result lease's boxed string
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the leased string box
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the extra persisted string owner
    abi::emit_call_label(emitter, "__rt_decref_any");                           // leave the new Mixed box as sole string owner
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload the original callback result cell
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the callback result after string persistence
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // prevent exceptional cleanup from seeing a released cell
    emit_host_exception_boundary_pop(emitter);
    emit_host_loader_args_cleanup_x86_64(emitter, "xpath_success");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the leased boxed callback string
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the caller-owned result header
    emit_host_result_init_x86_64(emitter, "r9");
    emitter.instruction("mov DWORD PTR [r9 + 12], 4");                          // public value tag four is a byte-string range
    emitter.instruction("mov QWORD PTR [r9 + 24], rax");                        // lease the boxed string as its host result ID
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load the persisted callback string pointer
    emitter.instruction("mov r11, QWORD PTR [rax + 16]");                       // load the persisted callback string length
    emitter.instruction("mov QWORD PTR [r9 + 40], r11");                        // mirror the byte length in payload1
    emitter.instruction("mov QWORD PTR [r9 + 48], r10");                        // expose borrowed string bytes while the lease is live
    emitter.instruction("mov QWORD PTR [r9 + 56], r11");                        // expose the exact borrowed byte count
    emitter.instruction("xor eax, eax");                                        // report a successful host transport
    emitter.instruction("jmp __rt_dom_host_call_ret");                          // return the leased XPath callback result
    emitter.label("__rt_dom_host_call_xpath_result_object");
    emitter.instruction("mov rdi, rax");                                        // validator arg0 is the boxed callback object
    emitter.instruction(&format!(
        "mov rsi, QWORD PTR [rbp - {}]",
        HOST_CALL_USER_DATA_OFFSET
    ));                                                                         // validator arg1 is the owning DOM bridge context
    abi::emit_call_label(emitter, "__rt_dom_host_xpath_result_handle");          // return the validated native bridge handle or zero
    emitter.instruction("test rax, rax");                                       // did the callback return one canonical DOM wrapper?
    emitter.instruction(
        "jz __rt_dom_host_call_xpath_result_object_type_error",
    );                                                                          // ordinary PHP objects cannot become XPath expressions
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the bridge handle across argument cleanup
    emit_host_exception_boundary_pop(emitter);
    emit_host_loader_args_cleanup_x86_64(emitter, "xpath_node");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload the leased boxed DOM callback result
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the caller-owned result header
    emit_host_result_init_x86_64(emitter, "r9");
    emitter.instruction("mov DWORD PTR [r9 + 12], 8");                          // public value tag eight identifies a native bridge handle
    emitter.instruction("mov QWORD PTR [r9 + 24], rax");                        // lease the original boxed DOM callback result
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the generation-checked bridge handle
    emitter.instruction("mov QWORD PTR [r9 + 32], r10");                        // expose the native bridge handle to Rust
    emitter.instruction("xor eax, eax");                                        // report a successful host transport
    emitter.instruction("jmp __rt_dom_host_call_ret");                          // return the leased XPath callback node
    emitter.label("__rt_dom_host_call_xpath_result_object_type_error");
    emit_xpath_object_type_error_x86_64(emitter);
    emitter.label("__rt_dom_host_call_xpath_result_bool");
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load the canonical callback boolean payload
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve it across result and argument cleanup
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the boxed boolean callback result
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // mark the callback result as released
    emit_host_exception_boundary_pop(emitter);
    emit_host_loader_args_cleanup_x86_64(emitter, "xpath_bool");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the caller-owned result header
    emit_host_result_init_x86_64(emitter, "r9");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the canonical boolean payload after result initialization
    emitter.instruction("mov DWORD PTR [r9 + 12], 1");                          // public value tag one is a PHP/XPath boolean
    emitter.instruction("mov QWORD PTR [r9 + 32], r10");                        // expose the canonical zero-or-one payload
    emitter.instruction("xor eax, eax");                                        // report a successful pointer-free host transport
    emitter.instruction("jmp __rt_dom_host_call_ret");                          // return the XPath boolean callback result
    emitter.label("__rt_dom_host_call_xpath_missing_result");
    emit_host_exception_boundary_pop(emitter);
    emitter.instruction("jmp __rt_dom_host_call_xpath_cleanup_fail");           // clean the argument container before ABI failure
    emitter.label("__rt_dom_host_call_xpath_missing_invoker");
    emit_host_exception_boundary_pop(emitter);
    emitter.label("__rt_dom_host_call_xpath_cleanup_fail");
    emit_host_loader_args_cleanup_x86_64(emitter, "xpath_failure");
    emitter.instruction("jmp __rt_dom_host_call_fail");                         // contain malformed callback state as an ABI failure
}

/// Validates and reads one bounded chunk from a leased PHP stream on x86_64.
fn emit_host_stream_read_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_dom_host_call_stream_read_validate");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the stream-read request pointer
    emitter.instruction("cmp QWORD PTR [rbp - 40], 96");                        // require header plus resource and length values
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject truncated or extended stream-read messages
    emitter.instruction("cmp QWORD PTR [r10 + 24], 2");                         // stream reads carry exactly two values
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject a shifted stream-read value section
    emitter.instruction("cmp QWORD PTR [r10 + 32], 0");                         // stream reads carry no byte section
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject unexpected request bytes
    emitter.instruction("cmp DWORD PTR [r10 + 48], 10");                        // public tag ten identifies a PHP resource
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject non-resource stream handles
    emitter.instruction("cmp DWORD PTR [r10 + 52], 0");                         // v1 resources carry no flags
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject flagged resource values
    emitter.instruction("mov r11, QWORD PTR [r10 + 56]");                       // load the opaque runtime stream descriptor
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // preserve the descriptor across fread
    emitter.instruction("mov r11, QWORD PTR [r10 + 64]");                       // load the resource-kind discriminator
    emitter.instruction("test r11, r11");                                       // is this a legacy kind-zero stream?
    emitter.instruction("jz __rt_dom_host_call_stream_kind_valid");             // accept legacy stream resources
    emitter.instruction("cmp r11, 1");                                          // kind one is an ordinary fd-backed stream
    emitter.instruction("je __rt_dom_host_call_stream_kind_valid");             // accept ordinary streams
    emitter.instruction("cmp r11, 3");                                          // kind three is a readable popen pipe fd
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject hash contexts and directory resources
    emitter.label("__rt_dom_host_call_stream_kind_valid");
    emitter.instruction("cmp DWORD PTR [r10 + 72], 2");                         // public tag two identifies an integer
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject non-integer read limits
    emitter.instruction("cmp DWORD PTR [r10 + 76], 0");                         // v1 integers carry no flags
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject flagged integer values
    emitter.instruction("mov r11, QWORD PTR [r10 + 80]");                       // load the requested maximum byte count
    emitter.instruction("test r11, r11");                                       // is the requested chunk non-empty?
    emitter.instruction("jz __rt_dom_host_call_fail");                          // native read callbacks always request positive lengths
    emitter.instruction("cmp r11, 0x7fffffff");                                 // does the request fit libxml2's signed-int capacity?
    emitter.instruction("ja __rt_dom_host_call_fail");                          // reject oversized reads
    emitter.instruction("cmp QWORD PTR [r10 + 88], 0");                         // canonical integers have no high payload
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject malformed integer values
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // preserve the maximum length across setjmp
    emit_host_exception_boundary_push(emitter);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // fread arg0 is the runtime stream descriptor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // fread arg1 is the bounded byte count
    abi::emit_call_label(emitter, "__rt_fread");                                // return one owned-or-borrowed chunk in rax/rdx
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // preserve the chunk pointer across persistence
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // preserve the exact returned chunk length
    emitter.instruction("test rdx, rdx");                                       // did fread reach EOF?
    emitter.instruction("jz __rt_dom_host_call_stream_read_empty");             // publish EOF without allocating a lease
    emitter.instruction("mov rdi, rax");                                        // string boxing low word is the chunk pointer
    emitter.instruction("mov rsi, rdx");                                        // string boxing high word is the chunk length
    emitter.instruction("mov eax, 1");                                          // runtime tag one boxes a persisted PHP string
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // copy and lease the stream chunk for Rust
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // preserve the leased result box
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the original fread chunk owner
    abi::emit_call_label(emitter, "__rt_decref_any");                           // release heap-backed wrapper/filter chunks
    emit_host_exception_boundary_pop(emitter);
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload the leased string result
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the caller-owned result header
    emit_host_result_init_x86_64(emitter, "r9");
    emitter.instruction("mov DWORD PTR [r9 + 12], 4");                          // public tag four identifies returned bytes
    emitter.instruction("mov QWORD PTR [r9 + 24], rax");                        // lease the boxed string as its result ID
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load the persistent result bytes
    emitter.instruction("mov r11, QWORD PTR [rax + 16]");                       // load the persistent result length
    emitter.instruction("mov QWORD PTR [r9 + 40], r11");                        // mirror the byte length in payload1
    emitter.instruction("mov QWORD PTR [r9 + 48], r10");                        // expose borrowed bytes while the lease is live
    emitter.instruction("mov QWORD PTR [r9 + 56], r11");                        // publish the exact byte count
    emitter.instruction("xor eax, eax");                                        // report successful host transport
    emitter.instruction("jmp __rt_dom_host_call_ret");                          // return the leased stream chunk
    emitter.label("__rt_dom_host_call_stream_read_empty");
    abi::emit_call_label(emitter, "__rt_decref_any");                           // release a heap-backed empty wrapper result
    emit_host_exception_boundary_pop(emitter);
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the caller-owned result header
    emit_host_result_init_x86_64(emitter, "r9");
    emitter.instruction("mov DWORD PTR [r9 + 12], 4");                          // EOF is an unleased empty byte string
    emitter.instruction("xor eax, eax");                                        // report successful EOF transport
    emitter.instruction("jmp __rt_dom_host_call_ret");                          // return the empty stream chunk
}

/// Validates and opens one PHP stream path for native document I/O on AArch64.
fn emit_host_stream_open_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_dom_host_call_stream_open_validate");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_REQUEST_OFFSET)); // reload the stream-open request
    emitter.instruction("ldr x10, [x9, #24]");                                  // load the declared value count
    emitter.instruction(&format!("cmp x10, #{}", HOST_STREAM_OPEN_VALUE_COUNT)); // require path, mode, and context values
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject a shifted stream-open value section
    emitter.instruction("ldr w10, [x9, #12]");                                  // load the stat-before-open flag
    emitter.instruction("cmp w10, #1");                                         // only zero and one are valid stream-open flags
    emitter.instruction("b.hi __rt_dom_host_call_fail");                        // reject unknown host-open behavior flags
    emitter.instruction("ldr x10, [x9, #32]");                                  // load the complete byte-section length
    emitter.instruction(&format!("add x11, x10, #{}", HOST_STREAM_OPEN_FIXED_SIZE)); // derive the exact request length
    emitter.instruction(&format!("ldr x12, [sp, #{}]", HOST_CALL_REQUEST_LENGTH_OFFSET)); // reload the caller-supplied request length
    emitter.instruction("cmp x11, x12");                                        // must values and bytes consume the whole request?
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject truncated or extended stream opens
    for index in 0..2 {
        let value_offset = 48 + index * 24;
        emitter.instruction(&format!("ldr w11, [x9, #{}]", value_offset));      // load the path or mode value tag
        emitter.instruction("cmp w11, #4");                                     // public tag four identifies byte strings
        emitter.instruction("b.ne __rt_dom_host_call_fail");                    // reject non-string path or mode values
        emitter.instruction(&format!("ldr w11, [x9, #{}]", value_offset + 4));  // load the byte-value flags
        emitter.instruction("cbnz w11, __rt_dom_host_call_fail");               // v1 stream-open strings carry no flags
        emitter.instruction(&format!("ldr x11, [x9, #{}]", value_offset + 8));  // load the byte-section offset
        emitter.instruction(&format!("ldr x12, [x9, #{}]", value_offset + 16)); // load the byte-string length
        emitter.instruction("adds x13, x11, x12");                              // compute the checked end offset
        emitter.instruction("b.cs __rt_dom_host_call_fail");                    // reject an overflowing byte range
        emitter.instruction("cmp x13, x10");                                    // must the bytes remain inside the request?
        emitter.instruction("b.hi __rt_dom_host_call_fail");                    // reject an out-of-bounds path or mode
        emitter.instruction(&format!("add x13, x9, #{}", HOST_STREAM_OPEN_FIXED_SIZE)); // locate the request byte section
        emitter.instruction("add x13, x13, x11");                               // point at this string's first byte
        let (pointer_offset, length_offset) = if index == 0 {
            (HOST_CALL_CONTEXT_OFFSET, HOST_CALL_ARGS_OFFSET)
        } else {
            (HOST_CALL_BOXED_ARGS_OFFSET, HOST_CALL_BOXED_RESULT_OFFSET)
        };
        emitter.instruction(&format!("str x13, [sp, #{}]", pointer_offset));    // preserve the path or mode pointer
        emitter.instruction(&format!("str x12, [sp, #{}]", length_offset));     // preserve its exact byte length
    }
    emitter.instruction("ldr w11, [x9, #96]");                                  // load the optional stream-context tag
    emitter.instruction("cbz w11, __rt_dom_host_call_stream_open_null_context"); // null selects the default runtime context
    emitter.instruction("cmp w11, #10");                                        // public tag ten identifies a resource
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject non-resource context values
    emitter.instruction("ldr w11, [x9, #100]");                                 // load the resource flags
    emitter.instruction("cbnz w11, __rt_dom_host_call_fail");                   // v1 resources carry no flags
    emitter.instruction("ldr x11, [x9, #112]");                                 // load the unused resource high word
    emitter.instruction("cbnz x11, __rt_dom_host_call_fail");                   // stream contexts use a canonical zero high word
    emitter.instruction("ldr x11, [x9, #104]");                                 // load the opaque stream-context resource ID
    emitter.instruction(&format!("str x11, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // preserve it across wrapper callbacks
    emitter.instruction("b __rt_dom_host_call_stream_open_context_ready");      // continue with the explicit context
    emitter.label("__rt_dom_host_call_stream_open_null_context");
    emitter.instruction("ldr x11, [x9, #100]");                                 // null flags and padding must be zero
    emitter.instruction("cbnz x11, __rt_dom_host_call_fail");                   // reject a flagged null context
    emitter.instruction("ldr x11, [x9, #104]");                                 // canonical null has no low payload
    emitter.instruction("cbnz x11, __rt_dom_host_call_fail");                   // reject a shifted null context
    emitter.instruction("ldr x11, [x9, #112]");                                 // canonical null has no high payload
    emitter.instruction("cbnz x11, __rt_dom_host_call_fail");                   // reject a malformed null context
    emitter.instruction(&format!("str xzr, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // default context uses resource ID zero
    emitter.label("__rt_dom_host_call_stream_open_context_ready");
    emit_host_exception_boundary_push(emitter);
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // select the libxml context before entering PHP wrapper code
    abi::emit_call_label(emitter, "__rt_stream_context_select");
    abi::emit_symbol_address(emitter, "x9", "_dom_stream_context_resource");
    emitter.instruction(&format!("ldr x10, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // reload the selected context resource
    emitter.instruction("str x10, [x9]");                                       // publish it for wrapper-object initialization
    abi::emit_symbol_address(emitter, "x9", "_dom_stream_context_active");
    emitter.instruction("mov x10, #1");                                         // every libxml open exposes a context resource
    emitter.instruction("str x10, [x9]");                                       // activate context injection for stat/open callbacks
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_REQUEST_OFFSET)); // reload the request flags
    emitter.instruction("ldr w10, [x9, #12]");                                  // should read-only libxml I/O run url_stat first?
    emitter.instruction("cbz w10, __rt_dom_host_call_stream_open_after_stat");  // write opens skip the read-side stat probe
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HOST_CALL_CONTEXT_OFFSET)); // url_stat path pointer
    emitter.instruction(&format!("ldr x1, [sp, #{}]", HOST_CALL_ARGS_OFFSET));  // url_stat path length
    emitter.instruction("mov x2, #2");                                          // PHP_STREAM_URL_STAT_QUIET
    abi::emit_call_label(emitter, "__rt_user_wrapper_url_stat");                // invoke a registered wrapper's stat hook when present
    emitter.instruction("cbz x0, __rt_dom_host_call_stream_open_after_stat");   // non-wrapper paths return no boxed stat result
    emitter.instruction("ldr x9, [x0]");                                        // inspect the boxed stat-result runtime tag
    emitter.instruction("cmp x9, #3");                                          // boxed tag three is PHP false
    emitter.instruction("b.eq __rt_dom_host_call_stream_open_stat_failed");     // false stat blocks stream_open like php-src
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the transient stat result
    emitter.instruction("b __rt_dom_host_call_stream_open_after_stat");         // continue only after a successful wrapper stat
    emitter.label("__rt_dom_host_call_stream_open_stat_failed");
    emitter.instruction(&format!("str x0, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // preserve the false box while collecting stable failure details
    abi::emit_symbol_address(emitter, "x9", "_user_wrapper_url_stat_failure_kind");
    emitter.instruction("ldr x10, [x9]");                                       // load zero for false returns or one for a missing method
    emitter.instruction("cmp x10, #1");                                         // did the wrapper omit url_stat?
    emitter.instruction("b.eq __rt_dom_host_call_stream_open_stat_missing");    // publish the class-specific PHP warning detail
    emitter.instruction("mov x11, #4");                                         // failure reason four is a silent false url_stat result
    emitter.instruction(&format!("str x11, [sp, #{}]", HOST_CALL_BOXED_ARGS_OFFSET)); // preserve the reason across box release
    emitter.instruction(&format!("str xzr, [sp, #{}]", HOST_CALL_CONTEXT_OFFSET)); // silent stat failures expose no class bytes
    emitter.instruction(&format!("str xzr, [sp, #{}]", HOST_CALL_ARGS_OFFSET)); // silent stat failures expose no class byte count
    emitter.instruction("b __rt_dom_host_call_stream_open_stat_detail_ready");  // share failure cleanup and result publication
    emitter.label("__rt_dom_host_call_stream_open_stat_missing");
    emitter.instruction("mov x11, #1");                                         // failure reason one means url_stat is absent
    emitter.instruction(&format!("str x11, [sp, #{}]", HOST_CALL_BOXED_ARGS_OFFSET)); // preserve the reason across box release
    abi::emit_symbol_address(emitter, "x9", "_user_wrapper_url_stat_class_ptr");
    emitter.instruction("ldr x10, [x9]");                                       // load the matched wrapper class-name pointer
    emitter.instruction(&format!("str x10, [sp, #{}]", HOST_CALL_CONTEXT_OFFSET)); // keep class bytes stable during cleanup
    abi::emit_symbol_address(emitter, "x9", "_user_wrapper_url_stat_class_len");
    emitter.instruction("ldr x10, [x9]");                                       // load the matched wrapper class-name length
    emitter.instruction(&format!("str x10, [sp, #{}]", HOST_CALL_ARGS_OFFSET)); // keep the class byte count stable during cleanup
    emitter.label("__rt_dom_host_call_stream_open_stat_detail_ready");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // reload the false stat box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the rejected stat result
    abi::emit_symbol_address(emitter, "x9", "_dom_stream_context_active");
    emitter.instruction("str xzr, [x9]");                                       // context injection ends on stat failure too
    emit_host_exception_boundary_pop(emitter);
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_RESULT_OFFSET)); // restore the result header
    emit_host_result_init_aarch64(emitter, "x9");
    emitter.instruction(&format!("ldr x10, [sp, #{}]", HOST_CALL_BOXED_ARGS_OFFSET)); // reload the stat failure reason
    emitter.instruction("str x10, [x9, #32]");                                  // expose the reason as null payload0
    emitter.instruction("cmp x10, #1");                                         // does this failure carry a wrapper class name?
    emitter.instruction("b.ne __rt_dom_host_call_stream_open_stat_published");  // silent false stat results carry no bytes
    emitter.instruction(&format!("ldr x11, [sp, #{}]", HOST_CALL_CONTEXT_OFFSET)); // reload the wrapper class-name pointer
    emitter.instruction("str x11, [x9, #48]");                                  // expose borrowed class-name bytes to Rust
    emitter.instruction(&format!("ldr x11, [sp, #{}]", HOST_CALL_ARGS_OFFSET)); // reload the wrapper class-name length
    emitter.instruction("str x11, [x9, #56]");                                  // publish the exact class byte count
    emitter.label("__rt_dom_host_call_stream_open_stat_published");
    emitter.instruction("mov w0, wzr");                                         // stat rejection is a successful null host result
    emitter.instruction("b __rt_dom_host_call_ret");                            // restore the native callback frame
    emitter.label("__rt_dom_host_call_stream_open_after_stat");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", HOST_CALL_CONTEXT_OFFSET)); // fopen path pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", HOST_CALL_ARGS_OFFSET));  // fopen path length
    emitter.instruction(&format!("ldr x3, [sp, #{}]", HOST_CALL_BOXED_ARGS_OFFSET)); // fopen mode pointer
    emitter.instruction(&format!("ldr x4, [sp, #{}]", HOST_CALL_BOXED_RESULT_OFFSET)); // fopen mode length
    abi::emit_call_label(emitter, "__rt_fopen");                                // open a native or registered PHP stream
    emitter.instruction(&format!("str x0, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // preserve the returned descriptor
    abi::emit_symbol_address(emitter, "x9", "_dom_stream_context_active");
    emitter.instruction("str xzr, [x9]");                                       // context injection ends immediately after stream_open()
    emit_host_exception_boundary_pop(emitter);
    emitter.instruction(&format!("ldr x10, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // reload the opened descriptor
    emitter.instruction("cmp x10, #0");                                         // did fopen report a negative failure sentinel?
    emitter.instruction("b.lt __rt_dom_host_call_stream_open_failed");          // publish a pointer-free null result on open failure
    emitter.instruction("mov x1, x10");                                         // box the runtime descriptor as the resource low word
    emitter.instruction("mov x2, #1");                                          // kind one closes native and synthetic wrapper streams
    emitter.instruction("mov x0, #9");                                          // runtime Mixed tag nine identifies a PHP resource
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // lease the opened resource until Rust drops it
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_RESULT_OFFSET)); // restore the result header
    emit_host_result_init_aarch64(emitter, "x9");
    emitter.instruction("mov w11, #10");                                        // public tag ten identifies a resource
    emitter.instruction("str w11, [x9, #12]");                                  // publish the opened stream value kind
    emitter.instruction("str x0, [x9, #24]");                                   // boxed resource is the host result lease ID
    emitter.instruction(&format!("ldr x10, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // reload the stream descriptor
    emitter.instruction("str x10, [x9, #32]");                                  // expose the descriptor as payload0
    emitter.instruction("mov x11, #1");                                         // ordinary stream resource-kind discriminator
    emitter.instruction("str x11, [x9, #40]");                                  // publish resource kind for read dispatch
    emitter.instruction("mov w11, #0x4000");                                    // load the high half of the wrapper descriptor base
    emitter.instruction("lsl x11, x11, #16");                                   // form USER_WRAPPER_FD_BASE
    emitter.instruction("cmp x10, x11");                                        // is the opened stream below the wrapper range?
    emitter.instruction("b.lt __rt_dom_host_call_stream_open_no_class");        // native streams have no PHP wrapper class name
    emitter.instruction("add x12, x11, #256");                                  // form the exclusive wrapper descriptor bound
    emitter.instruction("cmp x10, x12");                                        // is the opened stream inside that range?
    emitter.instruction("b.hs __rt_dom_host_call_stream_open_no_class");        // descriptors above it have no wrapper class
    emitter.instruction("sub x11, x10, x11");                                   // convert the descriptor to its handle-slot index
    abi::emit_symbol_address(emitter, "x12", "_user_wrapper_handles");
    emitter.instruction("ldr x12, [x12, x11, lsl #3]");                         // load the live wrapper object from its handle slot
    emitter.instruction("cbz x12, __rt_dom_host_call_stream_open_no_class");    // a stale slot cannot provide class metadata
    emitter.instruction("ldr x11, [x12]");                                      // load the wrapper object's runtime class id
    abi::emit_symbol_address(emitter, "x12", "_class_name_entries");
    emitter.instruction("add x12, x12, x11, lsl #4");                           // address the 16-byte class-name metadata row
    emitter.instruction("ldr x11, [x12]");                                      // load the wrapper class-name pointer
    emitter.instruction("str x11, [x9, #48]");                                  // expose borrowed class-name bytes to the bridge
    emitter.instruction("ldr x11, [x12, #8]");                                  // load the wrapper class-name byte length
    emitter.instruction("str x11, [x9, #56]");                                  // publish that borrowed byte count
    emitter.label("__rt_dom_host_call_stream_open_no_class");
    emitter.instruction("mov w0, wzr");                                         // report successful host transport
    emitter.instruction("b __rt_dom_host_call_ret");                            // restore the native callback frame
    emitter.label("__rt_dom_host_call_stream_open_failed");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_RESULT_OFFSET)); // restore the result header
    emit_host_result_init_aarch64(emitter, "x9");
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_open_failure_kind");
    emitter.instruction("ldr x11, [x10]");                                      // load the wrapper-open failure discriminator
    emitter.instruction("cbz x11, __rt_dom_host_call_stream_open_failed_published"); // native and generic failures carry no PHP method warning
    emitter.instruction("add x11, x11, #1");                                    // map fopen kinds one/two to host reasons two/three
    emitter.instruction("str x11, [x9, #32]");                                  // expose the exact method failure as null payload0
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_open_class_ptr");
    emitter.instruction("ldr x11, [x10]");                                      // load the failed wrapper class-name pointer
    emitter.instruction("str x11, [x9, #48]");                                  // expose borrowed class-name bytes to Rust
    abi::emit_symbol_address(emitter, "x10", "_user_wrapper_open_class_len");
    emitter.instruction("ldr x11, [x10]");                                      // load the failed wrapper class-name length
    emitter.instruction("str x11, [x9, #56]");                                  // publish the exact class byte count
    emitter.label("__rt_dom_host_call_stream_open_failed_published");
    emitter.instruction("mov w0, wzr");                                         // open failure is a successful null host result
    emitter.instruction("b __rt_dom_host_call_ret");                            // restore the native callback frame
}

/// Validates and opens one PHP stream path for native document I/O on x86_64.
fn emit_host_stream_open_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_dom_host_call_stream_open_validate");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the stream-open request
    emitter.instruction(&format!("cmp QWORD PTR [r10 + 24], {}", HOST_STREAM_OPEN_VALUE_COUNT)); // require path, mode, and context values
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject a shifted stream-open value section
    emitter.instruction("cmp DWORD PTR [r10 + 12], 1");                         // only zero and one are valid stream-open flags
    emitter.instruction("ja __rt_dom_host_call_fail");                          // reject unknown host-open behavior flags
    emitter.instruction("mov r11, QWORD PTR [r10 + 32]");                       // load the request byte-section length
    emitter.instruction(&format!("add r11, {}", HOST_STREAM_OPEN_FIXED_SIZE));  // derive the exact complete request length
    emitter.instruction("cmp r11, QWORD PTR [rbp - 40]");                       // must values and bytes consume the whole request?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject truncated or extended stream opens
    for index in 0..2 {
        let value_offset = 48 + index * 24;
        emitter.instruction(&format!("cmp DWORD PTR [r10 + {}], 4", value_offset)); // require a byte-string path or mode
        emitter.instruction("jne __rt_dom_host_call_fail");                     // reject non-string path or mode values
        emitter.instruction(&format!("cmp DWORD PTR [r10 + {}], 0", value_offset + 4)); // v1 byte values carry no flags
        emitter.instruction("jne __rt_dom_host_call_fail");                     // reject a flagged path or mode
        emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {}]", value_offset + 8)); // load the byte-section offset
        emitter.instruction(&format!("mov r12, QWORD PTR [r10 + {}]", value_offset + 16)); // load the byte-string length
        emitter.instruction("mov r13, r11");                                    // copy the offset for checked addition
        emitter.instruction("add r13, r12");                                    // compute the range end
        emitter.instruction("jc __rt_dom_host_call_fail");                      // reject an overflowing byte range
        emitter.instruction("cmp r13, QWORD PTR [r10 + 32]");                   // must the range remain inside request bytes?
        emitter.instruction("ja __rt_dom_host_call_fail");                      // reject an out-of-bounds path or mode
        emitter.instruction(&format!("lea r13, [r10 + {}]", HOST_STREAM_OPEN_FIXED_SIZE)); // locate the request byte section
        emitter.instruction("add r13, r11");                                    // point at this string's first byte
        let (pointer_offset, length_offset) = if index == 0 {
            (-56, -48)
        } else {
            (-64, -72)
        };
        emitter.instruction(&format!("mov QWORD PTR [rbp {}], r13", pointer_offset)); // preserve the path or mode pointer
        emitter.instruction(&format!("mov QWORD PTR [rbp {}], r12", length_offset)); // preserve its exact byte length
    }
    emitter.instruction("mov eax, DWORD PTR [r10 + 96]");                       // load the optional stream-context tag
    emitter.instruction("test eax, eax");                                       // does the request select the default context?
    emitter.instruction("jz __rt_dom_host_call_stream_open_null_context_x86");  // validate the canonical null form
    emitter.instruction("cmp eax, 10");                                         // public tag ten identifies a resource
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject non-resource contexts
    emitter.instruction("cmp DWORD PTR [r10 + 100], 0");                        // v1 resources carry no flags
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject a flagged stream context
    emitter.instruction("cmp QWORD PTR [r10 + 112], 0");                        // stream contexts use a zero high word
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject a malformed context resource
    emitter.instruction("mov r11, QWORD PTR [r10 + 104]");                      // load the opaque context resource ID
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // preserve it across wrapper callbacks
    emitter.instruction("jmp __rt_dom_host_call_stream_open_context_ready_x86"); // continue with the explicit context
    emitter.label("__rt_dom_host_call_stream_open_null_context_x86");
    emitter.instruction("cmp DWORD PTR [r10 + 100], 0");                        // null flags and padding must be zero
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject a flagged null context
    emitter.instruction("cmp QWORD PTR [r10 + 104], 0");                        // canonical null has no low payload
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject a shifted null context
    emitter.instruction("cmp QWORD PTR [r10 + 112], 0");                        // canonical null has no high payload
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject a malformed null context
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // default context uses resource ID zero
    emitter.label("__rt_dom_host_call_stream_open_context_ready_x86");
    emit_host_exception_boundary_push(emitter);
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // select the libxml context before entering PHP wrapper code
    abi::emit_call_label(emitter, "__rt_stream_context_select");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the selected context resource
    abi::emit_store_reg_to_symbol(emitter, "r11", "_dom_stream_context_resource", 0);
    emitter.instruction("mov r11, 1");                                          // every libxml open exposes a context resource
    abi::emit_store_reg_to_symbol(emitter, "r11", "_dom_stream_context_active", 0);
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the request flags
    emitter.instruction("cmp DWORD PTR [r10 + 12], 0");                         // should read-only I/O run url_stat first?
    emitter.instruction("je __rt_dom_host_call_stream_open_after_stat_x86");    // write opens skip the stat probe
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // url_stat path pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // url_stat path length
    emitter.instruction("mov edx, 2");                                          // PHP_STREAM_URL_STAT_QUIET
    abi::emit_call_label(emitter, "__rt_user_wrapper_url_stat");                // invoke a registered wrapper's stat hook when present
    emitter.instruction("test rax, rax");                                       // did a registered wrapper return a boxed stat value?
    emitter.instruction("jz __rt_dom_host_call_stream_open_after_stat_x86");    // non-wrapper paths have nothing to release
    emitter.instruction("cmp QWORD PTR [rax], 3");                              // boxed tag three is PHP false
    emitter.instruction("je __rt_dom_host_call_stream_open_stat_failed_x86");   // false stat blocks stream_open like php-src
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the transient stat result
    emitter.instruction("jmp __rt_dom_host_call_stream_open_after_stat_x86");   // continue only after a successful wrapper stat
    emitter.label("__rt_dom_host_call_stream_open_stat_failed_x86");
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // preserve the false box while collecting stable failure details
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_url_stat_failure_kind");
    emitter.instruction("cmp QWORD PTR [r10], 1");                              // did the wrapper omit url_stat?
    emitter.instruction("je __rt_dom_host_call_stream_open_stat_missing_x86");  // publish the class-specific PHP warning detail
    emitter.instruction("mov QWORD PTR [rbp - 16], 4");                         // failure reason four is a silent false url_stat result
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // silent stat failures expose no class bytes
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // silent stat failures expose no class byte count
    emitter.instruction("jmp __rt_dom_host_call_stream_open_stat_detail_ready_x86"); // share failure cleanup and result publication
    emitter.label("__rt_dom_host_call_stream_open_stat_missing_x86");
    emitter.instruction("mov QWORD PTR [rbp - 16], 1");                         // failure reason one means url_stat is absent
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_url_stat_class_ptr");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the matched wrapper class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // keep class bytes stable during cleanup
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_url_stat_class_len");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the matched wrapper class-name length
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // keep the class byte count stable during cleanup
    emitter.label("__rt_dom_host_call_stream_open_stat_detail_ready_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // reload the false stat box
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // release the rejected stat result
    abi::emit_store_zero_to_symbol(emitter, "_dom_stream_context_active", 0);
    emit_host_exception_boundary_pop(emitter);
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the result header
    emit_host_result_init_x86_64(emitter, "r9");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the stat failure reason
    emitter.instruction("mov QWORD PTR [r9 + 32], r10");                        // expose the reason as null payload0
    emitter.instruction("cmp r10, 1");                                          // does this failure carry a wrapper class name?
    emitter.instruction("jne __rt_dom_host_call_stream_open_stat_published_x86"); // silent false stat results carry no bytes
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the wrapper class-name pointer
    emitter.instruction("mov QWORD PTR [r9 + 48], r11");                        // expose borrowed class-name bytes to Rust
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the wrapper class-name length
    emitter.instruction("mov QWORD PTR [r9 + 56], r11");                        // publish the exact class byte count
    emitter.label("__rt_dom_host_call_stream_open_stat_published_x86");
    emitter.instruction("xor eax, eax");                                        // stat rejection is a successful null host result
    emitter.instruction("jmp __rt_dom_host_call_ret");                          // restore the native callback frame
    emitter.label("__rt_dom_host_call_stream_open_after_stat_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // fopen path pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // fopen path length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // fopen mode pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // fopen mode length
    abi::emit_call_label(emitter, "__rt_fopen");                                // open a native or registered PHP stream
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the returned descriptor
    abi::emit_store_zero_to_symbol(emitter, "_dom_stream_context_active", 0);
    emit_host_exception_boundary_pop(emitter);
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the opened descriptor
    emitter.instruction("test r10, r10");                                       // did fopen report a negative failure sentinel?
    emitter.instruction("js __rt_dom_host_call_stream_open_failed_x86");        // publish a pointer-free null result on failure
    emitter.instruction("mov rdi, r10");                                        // box the descriptor as the resource low word
    emitter.instruction("mov esi, 1");                                          // kind one closes native and synthetic wrapper streams
    emitter.instruction("mov eax, 9");                                          // runtime Mixed tag nine identifies a PHP resource
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // lease the opened stream until Rust drops it
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the result header
    emit_host_result_init_x86_64(emitter, "r9");
    emitter.instruction("mov DWORD PTR [r9 + 12], 10");                         // public tag ten identifies a resource
    emitter.instruction("mov QWORD PTR [r9 + 24], rax");                        // boxed resource is the host result lease ID
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the stream descriptor
    emitter.instruction("mov QWORD PTR [r9 + 32], r10");                        // expose the descriptor as payload0
    emitter.instruction("mov QWORD PTR [r9 + 40], 1");                          // publish ordinary stream resource kind
    emitter.instruction("mov r11, r10");                                        // copy the descriptor for wrapper-range normalization
    emitter.instruction("sub r11, 0x40000000");                                 // convert a wrapper descriptor to its handle-slot index
    emitter.instruction("cmp r11, 256");                                        // is this descriptor inside the wrapper handle range?
    emitter.instruction("jae __rt_dom_host_call_stream_open_no_class_x86");     // native and out-of-range streams have no PHP class
    abi::emit_symbol_address(emitter, "r8", "_user_wrapper_handles");
    emitter.instruction("mov r8, QWORD PTR [r8 + r11 * 8]");                    // load the live wrapper object from its handle slot
    emitter.instruction("test r8, r8");                                         // did the descriptor resolve to a live wrapper object?
    emitter.instruction("jz __rt_dom_host_call_stream_open_no_class_x86");      // stale slots cannot provide class metadata
    emitter.instruction("mov r11, QWORD PTR [r8]");                             // load the wrapper object's runtime class id
    abi::emit_symbol_address(emitter, "r8", "_class_name_entries");
    emitter.instruction("shl r11, 4");                                          // scale the class id to a 16-byte metadata row
    emitter.instruction("add r8, r11");                                         // address the wrapper class-name metadata
    emitter.instruction("mov r11, QWORD PTR [r8]");                             // load the wrapper class-name pointer
    emitter.instruction("mov QWORD PTR [r9 + 48], r11");                        // expose borrowed class-name bytes to the bridge
    emitter.instruction("mov r11, QWORD PTR [r8 + 8]");                         // load the wrapper class-name byte length
    emitter.instruction("mov QWORD PTR [r9 + 56], r11");                        // publish that borrowed byte count
    emitter.label("__rt_dom_host_call_stream_open_no_class_x86");
    emitter.instruction("xor eax, eax");                                        // report successful host transport
    emitter.instruction("jmp __rt_dom_host_call_ret");                          // restore the native callback frame
    emitter.label("__rt_dom_host_call_stream_open_failed_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the result header
    emit_host_result_init_x86_64(emitter, "r9");
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_open_failure_kind");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the wrapper-open failure discriminator
    emitter.instruction("test r11, r11");                                       // is this a class-specific wrapper failure?
    emitter.instruction("jz __rt_dom_host_call_stream_open_failed_published_x86"); // native and generic failures carry no PHP method warning
    emitter.instruction("inc r11");                                             // map fopen kinds one/two to host reasons two/three
    emitter.instruction("mov QWORD PTR [r9 + 32], r11");                        // expose the exact method failure as null payload0
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_open_class_ptr");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the failed wrapper class-name pointer
    emitter.instruction("mov QWORD PTR [r9 + 48], r11");                        // expose borrowed class-name bytes to Rust
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_open_class_len");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the failed wrapper class-name length
    emitter.instruction("mov QWORD PTR [r9 + 56], r11");                        // publish the exact class byte count
    emitter.label("__rt_dom_host_call_stream_open_failed_published_x86");
    emitter.instruction("xor eax, eax");                                        // open failure is a successful null host result
    emitter.instruction("jmp __rt_dom_host_call_ret");                          // restore the native callback frame
}

/// Validates and writes one bounded stream chunk for native document I/O on AArch64.
fn emit_host_stream_write_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_dom_host_call_stream_write_validate");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_REQUEST_OFFSET)); // reload the stream-write request
    emitter.instruction("ldr w10, [x9, #12]");                                  // load the request flags
    emitter.instruction("cbnz w10, __rt_dom_host_call_fail");                   // v1 stream writes carry no flags
    emitter.instruction("ldr x10, [x9, #24]");                                  // load the declared value count
    emitter.instruction("cmp x10, #2");                                         // stream writes carry resource and byte-string values
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject a shifted stream-write value section
    emitter.instruction("ldr x10, [x9, #32]");                                  // load the exact payload byte count
    emitter.instruction("cbz x10, __rt_dom_host_call_fail");                    // native write callbacks always send non-empty chunks
    emitter.instruction("adds x11, x10, #96");                                  // derive header plus two values plus bytes
    emitter.instruction("b.cs __rt_dom_host_call_fail");                        // reject an overflowing complete request length
    emitter.instruction(&format!("ldr x12, [sp, #{}]", HOST_CALL_REQUEST_LENGTH_OFFSET)); // reload the caller-supplied request length
    emitter.instruction("cmp x11, x12");                                        // must the byte section consume the complete request?
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject truncated or extended stream writes
    emitter.instruction("ldr w11, [x9, #48]");                                  // load the stream resource value tag
    emitter.instruction("cmp w11, #10");                                        // public tag ten identifies a PHP resource
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject non-resource stream handles
    emitter.instruction("ldr w11, [x9, #52]");                                  // load resource flags
    emitter.instruction("cbnz w11, __rt_dom_host_call_fail");                   // v1 resources carry no flags
    emitter.instruction("ldr x11, [x9, #56]");                                  // load the opaque runtime stream descriptor
    emitter.instruction(&format!("str x11, [sp, #{}]", HOST_CALL_CONTEXT_OFFSET)); // preserve the descriptor across fwrite
    emitter.instruction("ldr x12, [x9, #64]");                                  // load the resource-kind discriminator
    emitter.instruction("cbz x12, __rt_dom_host_call_stream_write_kind_valid"); // accept legacy kind-zero streams
    emitter.instruction("cmp x12, #1");                                         // kind one is an ordinary fd-backed stream
    emitter.instruction("b.eq __rt_dom_host_call_stream_write_kind_valid");     // accept ordinary streams
    emitter.instruction("cmp x12, #3");                                         // kind three is a writable popen pipe fd
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject non-stream resources
    emitter.label("__rt_dom_host_call_stream_write_kind_valid");
    emitter.instruction("ldr w11, [x9, #72]");                                  // load the payload byte-string tag
    emitter.instruction("cmp w11, #4");                                         // public tag four identifies request bytes
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject non-string write payloads
    emitter.instruction("ldr w11, [x9, #76]");                                  // load payload flags
    emitter.instruction("cbnz w11, __rt_dom_host_call_fail");                   // v1 byte values carry no flags
    emitter.instruction("ldr x11, [x9, #80]");                                  // load the byte-section offset
    emitter.instruction("cbnz x11, __rt_dom_host_call_fail");                   // the sole payload starts at byte-section offset zero
    emitter.instruction("ldr x11, [x9, #88]");                                  // load the described payload length
    emitter.instruction("cmp x11, x10");                                        // must the value cover every request byte?
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject partial or overlong byte ranges
    emitter.instruction("add x11, x9, #96");                                    // locate the first output payload byte
    emitter.instruction(&format!("str x11, [sp, #{}]", HOST_CALL_ARGS_OFFSET)); // preserve the payload pointer across setjmp
    emitter.instruction(&format!("str x10, [sp, #{}]", HOST_CALL_BOXED_ARGS_OFFSET)); // preserve the payload length across setjmp
    emit_host_exception_boundary_push(emitter);
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HOST_CALL_CONTEXT_OFFSET)); // fwrite arg0 is the runtime stream descriptor
    emitter.instruction(&format!("ldr x1, [sp, #{}]", HOST_CALL_ARGS_OFFSET));  // fwrite arg1 is the payload pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", HOST_CALL_BOXED_ARGS_OFFSET)); // fwrite arg2 is the remaining byte count
    abi::emit_call_label(emitter, "__rt_fwrite");                               // return bytes written or minus one for exact false
    emitter.instruction(&format!("str x0, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // preserve the signed write result
    emit_host_exception_boundary_pop(emitter);
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_RESULT_OFFSET)); // restore the caller-owned result header
    emit_host_result_init_aarch64(emitter, "x9");
    emitter.instruction("mov w11, #2");                                         // public tag two identifies a signed integer
    emitter.instruction("str w11, [x9, #12]");                                  // publish the stream-write result kind
    emitter.instruction(&format!("ldr x10, [sp, #{}]", HOST_CALL_DESCRIPTOR_OFFSET)); // reload bytes written or minus one
    emitter.instruction("str x10, [x9, #32]");                                  // publish the signed result bit pattern
    emitter.instruction("mov w0, wzr");                                         // report successful host transport
    emitter.instruction("b __rt_dom_host_call_ret");                            // restore the native callback frame
}

/// Validates and flushes one leased document stream on AArch64.
fn emit_host_stream_flush_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_dom_host_call_stream_flush_validate");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_REQUEST_OFFSET)); // reload the stream-flush request
    emitter.instruction(&format!("ldr x10, [sp, #{}]", HOST_CALL_REQUEST_LENGTH_OFFSET)); // reload the complete request length
    emitter.instruction("cmp x10, #72");                                        // require one padded resource value
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject truncated or extended flush requests
    emitter.instruction("ldr w10, [x9, #12]");                                  // load request flags
    emitter.instruction("cbnz w10, __rt_dom_host_call_fail");                   // v1 stream flushes carry no flags
    emitter.instruction("ldr x10, [x9, #24]");                                  // load the declared value count
    emitter.instruction("cmp x10, #1");                                         // stream flushes carry one resource
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject a shifted resource value
    emitter.instruction("ldr x10, [x9, #32]");                                  // load the byte-section length
    emitter.instruction("cbnz x10, __rt_dom_host_call_fail");                   // flush requests carry no bytes
    emitter.instruction("ldr w10, [x9, #48]");                                  // load the resource value tag
    emitter.instruction("cmp w10, #10");                                        // public tag ten identifies a PHP resource
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject non-resource flush targets
    emitter.instruction("ldr w10, [x9, #52]");                                  // load resource flags
    emitter.instruction("cbnz w10, __rt_dom_host_call_fail");                   // v1 resources carry no flags
    emitter.instruction("ldr x10, [x9, #56]");                                  // load the runtime stream descriptor
    emitter.instruction(&format!("str x10, [sp, #{}]", HOST_CALL_CONTEXT_OFFSET)); // preserve the descriptor across flush
    emitter.instruction("ldr x11, [x9, #64]");                                  // load the resource-kind discriminator
    emitter.instruction("cbz x11, __rt_dom_host_call_stream_flush_kind_valid"); // accept legacy kind-zero streams
    emitter.instruction("cmp x11, #1");                                         // kind one is an ordinary fd-backed stream
    emitter.instruction("b.eq __rt_dom_host_call_stream_flush_kind_valid");     // accept ordinary streams
    emitter.instruction("cmp x11, #3");                                         // kind three is a writable popen pipe fd
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject non-stream resources
    emitter.label("__rt_dom_host_call_stream_flush_kind_valid");
    emit_host_exception_boundary_push(emitter);
    emitter.instruction(&format!("ldr x0, [sp, #{}]", HOST_CALL_CONTEXT_OFFSET)); // pass the runtime stream descriptor
    emitter.instruction("mov w9, #0x4000");                                     // load the high half of the wrapper descriptor base
    emitter.instruction("lsl w9, w9, #16");                                     // form USER_WRAPPER_FD_BASE
    emitter.instruction("cmp x0, x9");                                          // is the stream below the wrapper range?
    emitter.instruction("b.lt __rt_dom_host_call_stream_flush_native");         // native streams use fsync-backed fflush
    emitter.instruction("add x10, x9, #256");                                   // form the exclusive wrapper descriptor bound
    emitter.instruction("cmp x0, x10");                                         // is the stream inside the wrapper range?
    emitter.instruction("b.ge __rt_dom_host_call_stream_flush_native");         // descriptors above it use native fflush
    abi::emit_call_label(emitter, "__rt_user_wrapper_fflush");                  // invoke stream_flush() when the wrapper provides it
    emitter.instruction("b __rt_dom_host_call_stream_flush_done");              // ignore the wrapper's boolean result like php-src
    emitter.label("__rt_dom_host_call_stream_flush_native");
    abi::emit_call_label(emitter, "__rt_fflush");                               // flush an ordinary native stream
    emitter.label("__rt_dom_host_call_stream_flush_done");
    emit_host_exception_boundary_pop(emitter);
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_RESULT_OFFSET)); // restore the caller-owned result header
    emit_host_result_init_aarch64(emitter, "x9");
    emitter.instruction("mov w0, wzr");                                         // report a pointer-free successful flush transport
    emitter.instruction("b __rt_dom_host_call_ret");                            // restore the native callback frame
}

/// Validates and writes one bounded stream chunk for native document I/O on x86_64.
fn emit_host_stream_write_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_dom_host_call_stream_write_validate");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the stream-write request
    emitter.instruction("cmp DWORD PTR [r10 + 12], 0");                         // do request flags use the v1 zero form?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject flagged stream writes
    emitter.instruction("cmp QWORD PTR [r10 + 24], 2");                         // stream writes carry resource and byte-string values
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject a shifted stream-write value section
    emitter.instruction("mov r11, QWORD PTR [r10 + 32]");                       // load the exact payload byte count
    emitter.instruction("test r11, r11");                                       // is the write payload non-empty?
    emitter.instruction("jz __rt_dom_host_call_fail");                          // native write callbacks always send non-empty chunks
    emitter.instruction("add r11, 96");                                         // derive header plus two values plus bytes
    emitter.instruction("jc __rt_dom_host_call_fail");                          // reject an overflowing complete request length
    emitter.instruction("cmp r11, QWORD PTR [rbp - 40]");                       // must bytes consume the complete request?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject truncated or extended stream writes
    emitter.instruction("cmp DWORD PTR [r10 + 48], 10");                        // public tag ten identifies a PHP resource
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject non-resource stream handles
    emitter.instruction("cmp DWORD PTR [r10 + 52], 0");                         // do resource flags use the v1 zero form?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject flagged resources
    emitter.instruction("mov r11, QWORD PTR [r10 + 56]");                       // load the opaque runtime stream descriptor
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // preserve the descriptor across fwrite
    emitter.instruction("mov r11, QWORD PTR [r10 + 64]");                       // load the resource-kind discriminator
    emitter.instruction("test r11, r11");                                       // is this a legacy kind-zero stream?
    emitter.instruction("jz __rt_dom_host_call_stream_write_kind_valid_x86");   // accept legacy stream resources
    emitter.instruction("cmp r11, 1");                                          // kind one is an ordinary fd-backed stream
    emitter.instruction("je __rt_dom_host_call_stream_write_kind_valid_x86");   // accept ordinary streams
    emitter.instruction("cmp r11, 3");                                          // kind three is a writable popen pipe fd
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject non-stream resources
    emitter.label("__rt_dom_host_call_stream_write_kind_valid_x86");
    emitter.instruction("cmp DWORD PTR [r10 + 72], 4");                         // public tag four identifies request bytes
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject non-string write payloads
    emitter.instruction("cmp DWORD PTR [r10 + 76], 0");                         // do payload flags use the v1 zero form?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject flagged byte values
    emitter.instruction("cmp QWORD PTR [r10 + 80], 0");                         // must the sole payload begin at offset zero?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject shifted output payloads
    emitter.instruction("mov r11, QWORD PTR [r10 + 88]");                       // load the described payload length
    emitter.instruction("cmp r11, QWORD PTR [r10 + 32]");                       // must the value cover every request byte?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject partial or overlong byte ranges
    emitter.instruction("lea r11, [r10 + 96]");                                 // locate the first output payload byte
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // preserve the payload pointer across setjmp
    emitter.instruction("mov r11, QWORD PTR [r10 + 32]");                       // reload the exact payload length
    emitter.instruction("mov QWORD PTR [rbp - 64], r11");                       // preserve it across setjmp
    emit_host_exception_boundary_push(emitter);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // fwrite arg0 is the runtime stream descriptor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // fwrite arg1 is the payload pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // fwrite arg2 is the remaining byte count
    abi::emit_call_label(emitter, "__rt_fwrite");                               // return bytes written or minus one for exact false
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the signed write result
    emit_host_exception_boundary_pop(emitter);
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the caller-owned result header
    emit_host_result_init_x86_64(emitter, "r9");
    emitter.instruction("mov DWORD PTR [r9 + 12], 2");                          // public tag two identifies a signed integer
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload bytes written or minus one
    emitter.instruction("mov QWORD PTR [r9 + 32], r10");                        // publish the signed result bit pattern
    emitter.instruction("xor eax, eax");                                        // report successful host transport
    emitter.instruction("jmp __rt_dom_host_call_ret");                          // restore the native callback frame
}

/// Validates and flushes one leased document stream on x86_64.
fn emit_host_stream_flush_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_dom_host_call_stream_flush_validate");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the stream-flush request
    emitter.instruction("cmp QWORD PTR [rbp - 40], 72");                        // require one padded resource value
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject truncated or extended flush requests
    emitter.instruction("cmp DWORD PTR [r10 + 12], 0");                         // do request flags use the v1 zero form?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject flagged stream flushes
    emitter.instruction("cmp QWORD PTR [r10 + 24], 1");                         // stream flushes carry one resource
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject a shifted resource value
    emitter.instruction("cmp QWORD PTR [r10 + 32], 0");                         // flush requests carry no bytes
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject unexpected request bytes
    emitter.instruction("cmp DWORD PTR [r10 + 48], 10");                        // public tag ten identifies a PHP resource
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject non-resource flush targets
    emitter.instruction("cmp DWORD PTR [r10 + 52], 0");                         // do resource flags use the v1 zero form?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject flagged resources
    emitter.instruction("mov r11, QWORD PTR [r10 + 56]");                       // load the runtime stream descriptor
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // preserve the descriptor across flush
    emitter.instruction("mov r11, QWORD PTR [r10 + 64]");                       // load the resource-kind discriminator
    emitter.instruction("test r11, r11");                                       // is this a legacy kind-zero stream?
    emitter.instruction("jz __rt_dom_host_call_stream_flush_kind_valid_x86");   // accept legacy stream resources
    emitter.instruction("cmp r11, 1");                                          // kind one is an ordinary fd-backed stream
    emitter.instruction("je __rt_dom_host_call_stream_flush_kind_valid_x86");   // accept ordinary streams
    emitter.instruction("cmp r11, 3");                                          // kind three is a writable popen pipe fd
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject non-stream resources
    emitter.label("__rt_dom_host_call_stream_flush_kind_valid_x86");
    emit_host_exception_boundary_push(emitter);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // pass the runtime stream descriptor
    emitter.instruction("cmp rdi, 0x40000000");                                 // is the descriptor below the wrapper range?
    emitter.instruction("jl __rt_dom_host_call_stream_flush_native_x86");       // native streams use fsync-backed fflush
    emitter.instruction("cmp rdi, 0x40000100");                                 // is the descriptor inside the wrapper range?
    emitter.instruction("jge __rt_dom_host_call_stream_flush_native_x86");      // descriptors above it use native fflush
    abi::emit_call_label(emitter, "__rt_user_wrapper_fflush");                  // invoke stream_flush() when the wrapper provides it
    emitter.instruction("jmp __rt_dom_host_call_stream_flush_done_x86");        // ignore the wrapper's boolean result like php-src
    emitter.label("__rt_dom_host_call_stream_flush_native_x86");
    emitter.instruction("mov rax, rdi");                                        // native fflush expects its descriptor in rax
    abi::emit_call_label(emitter, "__rt_fflush");                               // flush an ordinary native stream
    emitter.label("__rt_dom_host_call_stream_flush_done_x86");
    emit_host_exception_boundary_pop(emitter);
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the caller-owned result header
    emit_host_result_init_x86_64(emitter, "r9");
    emitter.instruction("xor eax, eax");                                        // report a pointer-free successful flush transport
    emitter.instruction("jmp __rt_dom_host_call_ret");                          // restore the native callback frame
}

/// Validates and emits one bridge-formatted PHP warning on AArch64.
fn emit_host_warning_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_dom_host_call_warning_validate");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_REQUEST_OFFSET)); // reload the warning request
    emitter.instruction("ldr w10, [x9, #12]");                                  // load request flags
    emitter.instruction("cbnz w10, __rt_dom_host_call_fail");                   // v1 warning requests carry no flags
    emitter.instruction("ldr x10, [x9, #24]");                                  // load the declared value count
    emitter.instruction("cmp x10, #1");                                         // warnings carry one byte-string value
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject a shifted warning value section
    emitter.instruction("ldr x10, [x9, #32]");                                  // load the formatted warning byte count
    emitter.instruction("cbz x10, __rt_dom_host_call_fail");                    // native warning messages must be non-empty
    emitter.instruction("adds x11, x10, #72");                                  // derive header plus value plus payload length
    emitter.instruction("b.cs __rt_dom_host_call_fail");                        // reject an overflowing complete request length
    emitter.instruction(&format!("ldr x12, [sp, #{}]", HOST_CALL_REQUEST_LENGTH_OFFSET)); // reload the caller-supplied request length
    emitter.instruction("cmp x11, x12");                                        // must payload bytes consume the complete request?
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject truncated or extended warning requests
    emitter.instruction("ldr w11, [x9, #48]");                                  // load the warning payload value tag
    emitter.instruction("cmp w11, #4");                                         // public tag four identifies byte strings
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject non-string warning values
    emitter.instruction("ldr w11, [x9, #52]");                                  // load warning payload flags
    emitter.instruction("cbnz w11, __rt_dom_host_call_fail");                   // v1 byte values carry no flags
    emitter.instruction("ldr x11, [x9, #56]");                                  // load the payload byte-section offset
    emitter.instruction("cbnz x11, __rt_dom_host_call_fail");                   // the sole warning payload starts at offset zero
    emitter.instruction("ldr x11, [x9, #64]");                                  // load the described warning byte length
    emitter.instruction("cmp x11, x10");                                        // must the value cover every request byte?
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject partial or overlong warning ranges
    emitter.instruction("add x1, x9, #72");                                     // pass the formatted warning pointer
    emitter.instruction("mov x2, x10");                                         // pass the exact warning byte count
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the warning through PHP diagnostics
    emitter.instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_RESULT_OFFSET)); // restore the caller-owned result header
    emit_host_result_init_aarch64(emitter, "x9");
    emitter.instruction("mov w0, wzr");                                         // report a pointer-free successful warning transport
    emitter.instruction("b __rt_dom_host_call_ret");                            // restore the native callback frame
}

/// Validates and emits one bridge-formatted PHP warning on x86_64.
fn emit_host_warning_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_dom_host_call_warning_validate");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the warning request
    emitter.instruction("cmp DWORD PTR [r10 + 12], 0");                         // do request flags use the v1 zero form?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject flagged warning requests
    emitter.instruction("cmp QWORD PTR [r10 + 24], 1");                         // warnings carry one byte-string value
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject a shifted warning value section
    emitter.instruction("mov r11, QWORD PTR [r10 + 32]");                       // load the formatted warning byte count
    emitter.instruction("test r11, r11");                                       // is the warning payload non-empty?
    emitter.instruction("jz __rt_dom_host_call_fail");                          // reject empty native warning messages
    emitter.instruction("add r11, 72");                                         // derive header plus value plus payload length
    emitter.instruction("jc __rt_dom_host_call_fail");                          // reject an overflowing complete request length
    emitter.instruction("cmp r11, QWORD PTR [rbp - 40]");                       // must bytes consume the complete request?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject truncated or extended warnings
    emitter.instruction("cmp DWORD PTR [r10 + 48], 4");                         // public tag four identifies byte strings
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject non-string warning values
    emitter.instruction("cmp DWORD PTR [r10 + 52], 0");                         // do payload flags use the v1 zero form?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject flagged warning byte strings
    emitter.instruction("cmp QWORD PTR [r10 + 56], 0");                         // must the sole payload begin at offset zero?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject shifted warning payloads
    emitter.instruction("mov r11, QWORD PTR [r10 + 64]");                       // load the described warning byte length
    emitter.instruction("cmp r11, QWORD PTR [r10 + 32]");                       // must the value cover every request byte?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject partial or overlong warning ranges
    emitter.instruction("lea rdi, [r10 + 72]");                                 // pass the formatted warning pointer
    emitter.instruction("mov rsi, r11");                                        // pass the exact warning byte count
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the warning through PHP diagnostics
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the caller-owned result header
    emit_host_result_init_x86_64(emitter, "r9");
    emitter.instruction("xor eax, eax");                                        // report a pointer-free successful warning transport
    emitter.instruction("jmp __rt_dom_host_call_ret");                          // restore the native callback frame
}

/// Validates and resolves one PHP callable name on AArch64.
fn emit_host_xpath_resolve_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_dom_host_call_xpath_resolve_validate");
    emitter
        .instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_REQUEST_OFFSET));  // reload the callable-name request
    emitter.instruction("ldr w10, [x9, #12]");                                  // load the encoded public argument count
    emitter.instruction("mov w11, #1");                                         // seed the single-root argument count
    emitter.instruction("movk w11, #0x8000, lsl #16");                          // add the v1 argument-count marker
    emitter.instruction("cmp w10, w11");                                        // must this request describe exactly one root?
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject incompatible request flags
    emitter.instruction("ldr x10, [x9, #24]");                                  // load the flat value count
    emitter.instruction("cmp x10, #1");                                         // callable resolution carries one byte value
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject shifted value sections
    emitter.instruction("ldr x10, [x9, #32]");                                  // load the callable-name byte count
    emitter.instruction("cbz x10, __rt_dom_host_call_fail");                    // PHP callable names cannot be empty
    emitter.instruction("adds x11, x10, #72");                                  // derive header plus value plus name length
    emitter.instruction("b.cs __rt_dom_host_call_fail");                        // reject a wrapped complete request length
    emitter.instruction(&format!(
        "ldr x12, [sp, #{}]",
        HOST_CALL_REQUEST_LENGTH_OFFSET
    ));                                                                         // reload the caller-supplied length
    emitter.instruction("cmp x11, x12");                                        // must the byte value consume the complete request?
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject truncated or extended names
    emitter.instruction("ldr w11, [x9, #48]");                                  // load the root value tag
    emitter.instruction("cmp w11, #4");                                         // public tag four identifies byte strings
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject non-string callable names
    emitter.instruction("ldr w11, [x9, #52]");                                  // load byte-value flags
    emitter.instruction("cbnz w11, __rt_dom_host_call_fail");                   // v1 byte values carry no flags
    emitter.instruction("ldr x11, [x9, #56]");                                  // load the byte-section offset
    emitter.instruction("cbnz x11, __rt_dom_host_call_fail");                   // the sole callable name begins at offset zero
    emitter.instruction("ldr x11, [x9, #64]");                                  // load the described callable-name length
    emitter.instruction("cmp x11, x10");                                        // must the value cover every payload byte?
    emitter.instruction("b.ne __rt_dom_host_call_fail");                        // reject partial or overlong name ranges
    emitter.instruction("add x0, x9, #72");                                     // resolver arg0 is the callable-name pointer
    emitter.instruction("mov x1, x10");                                         // resolver arg1 is the exact byte length
    abi::emit_call_label(emitter, "__rt_dom_xpath_resolve_callable");            // return a borrowed descriptor or zero
    emitter
        .instruction(&format!("ldr x9, [sp, #{}]", HOST_CALL_RESULT_OFFSET));   // restore the caller-owned result header
    emit_host_result_init_aarch64(emitter, "x9");
    emitter.instruction("cbz x0, __rt_dom_host_call_xpath_resolve_published");  // an unknown callable is canonical null
    emitter.instruction("mov w10, #9");                                         // public tag nine identifies a PHP callable
    emitter.instruction("str w10, [x9, #12]");                                  // publish the callable result kind
    emitter.instruction("str x0, [x9, #32]");                                   // expose the borrowed descriptor
    emitter.label("__rt_dom_host_call_xpath_resolve_published");
    emitter.instruction("mov w0, wzr");                                         // report successful pointer-free resolution
    emitter.instruction("b __rt_dom_host_call_ret");                            // restore the native callback frame
}

/// Validates and resolves one PHP callable name on x86_64.
fn emit_host_xpath_resolve_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_dom_host_call_xpath_resolve_validate");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the callable-name request
    emitter.instruction("cmp DWORD PTR [r10 + 12], -2147483647");               // require the v1 marker plus one public root
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject incompatible request flags
    emitter.instruction("cmp QWORD PTR [r10 + 24], 1");                         // callable resolution carries one byte value
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject shifted value sections
    emitter.instruction("mov r11, QWORD PTR [r10 + 32]");                       // load the callable-name byte count
    emitter.instruction("test r11, r11");                                       // is the callable name non-empty?
    emitter.instruction("jz __rt_dom_host_call_fail");                          // reject empty callable names
    emitter.instruction("add r11, 72");                                         // derive header plus value plus name length
    emitter.instruction("jc __rt_dom_host_call_fail");                          // reject a wrapped complete request length
    emitter.instruction("cmp r11, QWORD PTR [rbp - 40]");                       // must the byte value consume the whole request?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject truncated or extended names
    emitter.instruction("cmp DWORD PTR [r10 + 48], 4");                         // public tag four identifies byte strings
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject non-string callable names
    emitter.instruction("cmp DWORD PTR [r10 + 52], 0");                         // do byte-value flags use the v1 zero form?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject flagged callable names
    emitter.instruction("cmp QWORD PTR [r10 + 56], 0");                         // must the sole name begin at byte offset zero?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject shifted callable-name bytes
    emitter.instruction("mov r11, QWORD PTR [r10 + 64]");                       // load the described callable-name length
    emitter.instruction("cmp r11, QWORD PTR [r10 + 32]");                       // must the value cover every payload byte?
    emitter.instruction("jne __rt_dom_host_call_fail");                         // reject partial or overlong name ranges
    emitter.instruction("lea rdi, [r10 + 72]");                                 // resolver arg0 is the callable-name pointer
    emitter.instruction("mov rsi, r11");                                        // resolver arg1 is the exact byte length
    abi::emit_call_label(emitter, "__rt_dom_xpath_resolve_callable");            // return a borrowed descriptor or zero
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the caller-owned result header
    emit_host_result_init_x86_64(emitter, "r9");
    emitter.instruction("test rax, rax");                                       // did the runtime recognize this callable name?
    emitter.instruction("jz __rt_dom_host_call_xpath_resolve_published");       // an unknown callable is canonical null
    emitter.instruction("mov DWORD PTR [r9 + 12], 9");                          // public tag nine identifies a PHP callable
    emitter.instruction("mov QWORD PTR [r9 + 32], rax");                        // expose the borrowed descriptor
    emitter.label("__rt_dom_host_call_xpath_resolve_published");
    emitter.instruction("xor eax, eax");                                        // report successful pointer-free resolution
    emitter.instruction("jmp __rt_dom_host_call_ret");                          // restore the native callback frame
}

/// Releases the external-loader argument container if its slots were initialized on x86_64.
fn emit_host_loader_args_cleanup_x86_64(emitter: &mut Emitter, suffix: &str) {
    let skip_boxed =
        format!("__rt_dom_host_call_loader_cleanup_boxed_skip_{suffix}");
    let skip_raw =
        format!("__rt_dom_host_call_loader_cleanup_raw_skip_{suffix}");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // load the optional boxed argument container as ABI arg0
    emitter.instruction("test rdi, rdi");                                       // was the boxed argument container initialized?
    emitter.instruction(&format!("jz {}", skip_boxed));                         // skip an argument box that was never built
    abi::emit_call_label(emitter, "__rt_decref_mixed");                         // drop the box's ownership of the raw argument array
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // prevent repeated cleanup after a contained throw
    emitter.label(&skip_boxed);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // load the optional raw argument array as ABI arg0
    emitter.instruction("test rdi, rdi");                                       // was the raw argument array initialized?
    emitter.instruction(&format!("jz {}", skip_raw));                           // skip an array that was never built
    abi::emit_call_label(emitter, "__rt_decref_any");                           // release the raw array and its three boxed values
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // prevent repeated cleanup on shared failure paths
    emitter.label(&skip_raw);
}

/// Initializes one x86_64 host result header as a successful pointer-free null value.
fn emit_host_result_init_x86_64(emitter: &mut Emitter, result_reg: &str) {
    emitter.instruction(&format!("mov DWORD PTR [{}], 1", result_reg));         // publish host result ABI version one
    emitter.instruction(&format!("mov DWORD PTR [{} + 4], 96", result_reg));    // publish the exact result struct size
    for offset in (8..96).step_by(8) {
        emitter.instruction(&format!("mov QWORD PTR [{} + {}], 0", result_reg, offset)); // clear one host result word
    }
}

/// Emits the non-returning AArch64 TypeError for unsupported XPath callback objects.
fn emit_xpath_object_type_error_aarch64(emitter: &mut Emitter) {
    emitter.instruction("mov x0, #56");                                         // request standard Throwable payload storage
    abi::emit_call_label(emitter, "__rt_heap_alloc");                            // allocate the TypeError object payload
    emitter.instruction("mov x9, #6");                                          // heap kind six identifies a Throwable object
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the uniform heap header
    abi::emit_symbol_address(emitter, "x9", "_spl_type_error_class_id");
    emitter.instruction("ldr x9, [x9]");                                        // load TypeError's runtime class ID
    emitter.instruction("str x9, [x0]");                                        // publish the concrete Throwable class
    abi::emit_symbol_address(
        emitter,
        "x9",
        "_elephc_dom_xpath_object_type_error",
    );
    emitter.instruction("str x9, [x0, #8]");                                    // publish the exact php-src diagnostic bytes
    emitter.instruction(&format!(
        "mov x9, #{}",
        DOM_XPATH_OBJECT_TYPE_ERROR.len()
    ));                                                                         // load the exact TypeError message length
    emitter.instruction("str x9, [x0, #16]");                                   // publish the message byte count
    emitter.instruction("str xzr, [x0, #24]");                                  // exception code defaults to zero
    emitter.instruction("str xzr, [x0, #40]");                                  // previous Throwable defaults to null
    abi::emit_symbol_address(emitter, "x9", "_exc_value");
    emitter.instruction("str x0, [x9]");                                        // publish the active TypeError object
    emitter.instruction("b __rt_throw_current");                                // unwind into the host callback boundary
}

/// Emits the non-returning Linux x86_64 TypeError for unsupported XPath callback objects.
fn emit_xpath_object_type_error_x86_64(emitter: &mut Emitter) {
    emitter.instruction("mov rax, 56");                                         // request standard Throwable payload storage
    abi::emit_call_label(emitter, "__rt_heap_alloc");                            // allocate the TypeError object payload
    emitter.instruction(&format!(
        "mov r10, 0x{:x}",
        crate::codegen_support::sentinels::x86_64_heap_kind_word(6)
    ));                                                                         // stamp the canonical x86_64 Throwable heap kind
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // publish the uniform heap header
    abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_type_error_class_id", 0);
    emitter.instruction("mov QWORD PTR [rax], r10");                            // publish TypeError's runtime class ID
    abi::emit_symbol_address(
        emitter,
        "r10",
        "_elephc_dom_xpath_object_type_error",
    );
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // publish the exact php-src diagnostic bytes
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + 16], {}",
        DOM_XPATH_OBJECT_TYPE_ERROR.len()
    ));                                                                         // publish the exact TypeError message length
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // exception code defaults to zero
    emitter.instruction("mov QWORD PTR [rax + 40], 0");                         // previous Throwable defaults to null
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);
    emitter.instruction("jmp __rt_throw_current");                              // unwind into the host callback boundary
}

/// Emits an overlap-agnostic byte copier for constructing flat bridge request sections.
fn emit_copy_bytes(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: DOM flat-request byte copy ---");
    emitter.label_global("__rt_dom_copy_bytes");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x9, x0");                                  // preserve the destination start as the helper result
            emitter.label("__rt_dom_copy_bytes_loop");
            emitter.instruction("cbz x2, __rt_dom_copy_bytes_ret");             // finish after copying the complete length-delimited byte string
            emitter.instruction("ldrb w10, [x1], #1");                          // load one source byte and advance the source cursor
            emitter.instruction("strb w10, [x0], #1");                          // store one byte and advance the request destination cursor
            emitter.instruction("sub x2, x2, #1");                              // decrement the remaining request-byte count
            emitter.instruction("b __rt_dom_copy_bytes_loop");                  // continue without interpreting embedded NUL bytes
            emitter.label("__rt_dom_copy_bytes_ret");
            emitter.instruction("mov x0, x9");                                  // return the original destination pointer
            emitter.instruction("ret");                                         // return after the exact byte count has been copied
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, rdi");                                // preserve the destination start as the helper result
            emitter.instruction("mov rcx, rdx");                                // copy the byte length into the loop counter
            emitter.instruction("rep movsb");                                   // copy exactly rcx bytes and preserve embedded NUL bytes
            emitter.instruction("ret");                                         // return the original destination pointer in rax
        }
    }
}

/// Emits the lazy process-local DOM bridge context constructor.
fn emit_context_ensure(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_context_ensure_x86_64(emitter);
        return;
    }

    let context_new = emitter.target.extern_symbol("elephc_dom_context_new");
    let set_class_metadata = emitter
        .target
        .extern_symbol("elephc_dom_context_set_class_metadata");
    emitter.blank();
    emitter.comment("--- runtime: DOM bridge context ensure ---");
    emitter.label_global("__rt_dom_context_ensure");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_elephc_dom_context", 0);
    emitter.instruction("cbnz x0, __rt_dom_context_ensure_ret");                // return the already initialized process-local context
    emitter.instruction("sub sp, sp, #16");                                     // reserve an aligned frame for the native context constructor
    emitter.instruction("stp x29, x30, [sp]");                                  // preserve the caller frame across the native bridge call
    emitter.instruction("mov x29, sp");                                         // establish the runtime helper frame
    abi::emit_symbol_address(emitter, "x0", "_elephc_dom_host_vtable");
    abi::emit_symbol_address(emitter, "x1", "_elephc_dom_context");
    abi::emit_call_label(emitter, &context_new);
    emitter.instruction("cbnz w0, __rt_dom_context_ensure_fail");               // reject ABI, engine, or allocation failures from context creation
    abi::emit_load_symbol_to_reg(emitter, "x0", "_elephc_dom_context", 0);
    emitter.instruction("cbz x0, __rt_dom_context_ensure_fail");                // reject a success status that did not initialize a context ID
    abi::emit_symbol_address(emitter, "x1", "_elephc_dom_class_metadata");
    abi::emit_load_symbol_to_reg(
        emitter,
        "x2",
        "_elephc_dom_class_metadata_count",
        0,
    );
    abi::emit_call_label(emitter, &set_class_metadata);
    emitter.instruction("cbnz w0, __rt_dom_context_ensure_fail");               // reject invalid compiler-emitted class metadata
    abi::emit_load_symbol_to_reg(emitter, "x0", "_elephc_dom_context", 0);
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore the caller frame after successful initialization
    emitter.instruction("add sp, sp, #16");                                     // release the native constructor frame
    emitter.label("__rt_dom_context_ensure_ret");
    emitter.instruction("ret");                                                 // return the opaque DOM context ID in x0
    emitter.label("__rt_dom_context_ensure_fail");
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore the caller frame before entering the fatal path
    emitter.instruction("add sp, sp, #16");                                     // release the failed native constructor frame
    emitter.instruction("b __rt_dom_bridge_failure");                           // terminate on a native boundary contract failure
}

/// Emits the Linux x86_64 lazy DOM context constructor.
fn emit_context_ensure_x86_64(emitter: &mut Emitter) {
    let context_new = emitter.target.extern_symbol("elephc_dom_context_new");
    let set_class_metadata = emitter
        .target
        .extern_symbol("elephc_dom_context_set_class_metadata");
    emitter.blank();
    emitter.comment("--- runtime: DOM bridge context ensure ---");
    emitter.label_global("__rt_dom_context_ensure");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_elephc_dom_context", 0);
    emitter.instruction("test rax, rax");                                       // is the process-local DOM context already initialized?
    emitter.instruction("jnz __rt_dom_context_ensure_ret");                     // return the existing opaque context ID
    emitter.instruction("push rbp");                                            // preserve the caller frame and align the native call stack
    emitter.instruction("mov rbp, rsp");                                        // establish the runtime helper frame
    abi::emit_symbol_address(emitter, "rdi", "_elephc_dom_host_vtable");
    abi::emit_symbol_address(emitter, "rsi", "_elephc_dom_context");
    abi::emit_call_label(emitter, &context_new);
    emitter.instruction("test eax, eax");                                       // did native context construction report success?
    emitter.instruction("jnz __rt_dom_context_ensure_fail");                    // reject ABI, engine, or allocation failures
    abi::emit_load_symbol_to_reg(emitter, "rax", "_elephc_dom_context", 0);
    emitter.instruction("test rax, rax");                                       // did the successful constructor publish a context ID?
    emitter.instruction("jz __rt_dom_context_ensure_fail");                     // reject an invalid zero context ID
    emitter.instruction("mov rdi, rax");                                        // pass the initialized DOM context ID
    abi::emit_symbol_address(emitter, "rsi", "_elephc_dom_class_metadata");
    abi::emit_load_symbol_to_reg(
        emitter,
        "rdx",
        "_elephc_dom_class_metadata_count",
        0,
    );
    abi::emit_call_label(emitter, &set_class_metadata);
    emitter.instruction("test eax, eax");                                       // did class metadata installation succeed?
    emitter.instruction("jnz __rt_dom_context_ensure_fail");                    // reject malformed compiler-emitted metadata
    abi::emit_load_symbol_to_reg(emitter, "rax", "_elephc_dom_context", 0);
    emitter.instruction("pop rbp");                                             // restore the caller frame after successful initialization
    emitter.label("__rt_dom_context_ensure_ret");
    emitter.instruction("ret");                                                 // return the opaque DOM context ID in rax
    emitter.label("__rt_dom_context_ensure_fail");
    emitter.instruction("pop rbp");                                             // restore the caller frame before entering the fatal path
    emitter.instruction("jmp __rt_dom_bridge_failure");                         // terminate on a native boundary contract failure
}

/// Emits the ordinary-object destructor used for every native DOM wrapper class.
fn emit_wrapper_finalize(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_wrapper_finalize_x86_64(emitter);
        return;
    }

    let dom_call = emitter.target.extern_symbol("elephc_dom_call");
    let result_release = emitter.target.extern_symbol("elephc_dom_result_release");
    emitter.blank();
    emitter.comment("--- runtime: DOM wrapper finalization ---");
    emitter.label_global("__rt_dom_wrapper_finalize");
    emitter.instruction("sub sp, sp, #192");                                    // reserve wrapper, request, result, and saved-frame storage
    emitter.instruction("stp x29, x30, [sp, #176]");                            // preserve the caller frame across native bridge calls
    emitter.instruction("add x29, sp, #176");                                   // establish the runtime finalizer frame
    emitter.instruction("str x0, [sp]");                                        // retain the borrowed PHP wrapper pointer
    emitter.instruction("ldr x11, [x0]");                                       // load the wrapper's runtime class ID outside the symbol scratch register
    abi::emit_cmp_reg_to_symbol(
        emitter,
        "x11",
        "_class_internal_extension_hidden_offsets_count",
    );
    emitter.instruction("b.hs __rt_dom_wrapper_finalize_ret");                  // ignore malformed out-of-range class metadata
    abi::emit_symbol_address(emitter, "x10", "_class_internal_extension_hidden_offsets");
    emitter.instruction("ldr x10, [x10, x11, lsl #3]");                         // load the compiler-hidden metadata tail offset
    emitter.instruction("cbz x10, __rt_dom_wrapper_finalize_ret");              // ordinary PHP objects have no native handle to release
    emitter.instruction("add x11, x0, x10");                                    // point at the wrapper's compiler-hidden metadata
    emitter.instruction("ldr x12, [x11, #64]");                                 // load the exactly-once finalization state
    emitter.instruction("cbnz x12, __rt_dom_wrapper_finalize_ret");             // never release one wrapper-owned handle twice
    emitter.instruction("mov x12, #1");                                         // mark finalization before crossing the re-entrant native ABI
    emitter.instruction("str x12, [x11, #64]");                                 // persist the exactly-once guard
    emitter.instruction("ldr x12, [x11, #16]");                                 // load the wrapper's opaque execution-context ID
    emitter.instruction("str x12, [sp, #8]");                                   // preserve the context across native calls
    emitter.instruction("ldr x13, [x11, #32]");                                 // load the generation-checked native object handle
    emitter.instruction("str x13, [sp, #16]");                                  // preserve the receiver handle for request construction
    emitter.instruction(&format!(
        "ldr x14, [x11, #{}]",
        crate::internal_extensions::NATIVE_WRAPPER_AUX_OWNER_OFFSET
    ));                                                                         // load an eager XPath array or namespace-parent owner
    emitter.instruction(&format!(
        "str xzr, [x11, #{}]",
        crate::internal_extensions::NATIVE_WRAPPER_AUX_OWNER_OFFSET
    ));                                                                         // clear the strong owner before recursive wrapper destruction
    emitter.instruction(&format!(
        "ldr x15, [x11, #{}]",
        crate::internal_extensions::NATIVE_WRAPPER_ITERATOR_CURRENT_OFFSET
    ));                                                                         // stage the independent SimpleXML iterator owner before calls clobber metadata registers
    emitter.instruction(&format!(
        "str xzr, [x11, #{}]",
        crate::internal_extensions::NATIVE_WRAPPER_ITERATOR_CURRENT_OFFSET
    ));                                                                         // clear the iterator owner before its destructor can re-enter the parent
    emitter.instruction("str x15, [sp, #24]");                                 // preserve the staged iterator owner across auxiliary release
    emitter.instruction("cbz x14, __rt_dom_wrapper_finalize_no_aux_owner");    // skip wrappers without an XPath auxiliary owner
    emitter.instruction("mov x0, x14");                                        // pass the heap-backed auxiliary owner to generic release
    abi::emit_call_label(emitter, "__rt_decref_any");                          // release arrays or parent wrapper objects by heap kind
    emitter.label("__rt_dom_wrapper_finalize_no_aux_owner");
    emitter.instruction("ldr x14, [sp, #24]");                                 // restore php-src's private strong iterator-data wrapper
    emitter.instruction("cbz x14, __rt_dom_wrapper_finalize_no_iterator_current"); // skip absent SimpleXML iterator data
    emitter.instruction("mov x0, x14");                                         // pass the strongly held iterator wrapper to object release
    emitter.instruction("bl __rt_decref_object");                               // run subclass destruction and release the iterator wrapper
    emitter.label("__rt_dom_wrapper_finalize_no_iterator_current");
    emitter.instruction("ldr x12, [sp, #8]");                                   // restore the context after iterator-wrapper destruction
    emitter.instruction("ldr x13, [sp, #16]");                                  // restore the native handle after iterator-wrapper destruction
    emitter.instruction("cbz x12, __rt_dom_wrapper_finalize_ret");              // a missing context cannot own a native handle
    emitter.instruction("cbz x13, __rt_dom_wrapper_finalize_ret");              // an uninitialized wrapper has nothing to release
    emitter.instruction("ldr x2, [sp]");                                        // pass the exact PHP wrapper pointer to weak-cache removal
    emitter.instruction("mov x0, x12");                                         // pass the owning DOM context ID
    emitter.instruction("mov x1, x13");                                         // pass the generation-checked native handle
    emitter.instruction("bl __rt_dom_wrapper_cache_remove");                    // remove weak identity state before releasing native ownership
    emitter.instruction("ldr x12, [sp, #8]");                                   // restore the context after cache removal
    emitter.instruction("ldr x13, [sp, #16]");                                  // restore the handle after cache removal

    emitter.comment("-- build the fixed empty release request --");
    abi::emit_load_int_immediate(emitter, "x9", ABI_VERSION);
    emitter.instruction("str w9, [sp, #32]");                                   // request ABI version
    abi::emit_load_int_immediate(emitter, "x9", REQUEST_HEADER_SIZE);
    emitter.instruction("str w9, [sp, #36]");                                   // request header byte size
    abi::emit_load_int_immediate(emitter, "x9", WRAPPER_RELEASE_OPCODE);
    emitter.instruction("str w9, [sp, #40]");                                   // stable wrapper-release opcode
    emitter.instruction("str wzr, [sp, #44]");                                  // release requests carry no flags
    emitter.instruction("str x13, [sp, #48]");                                  // receiver is the native wrapper handle
    emitter.instruction("str xzr, [sp, #56]");                                  // release requests contain no values
    emitter.instruction("str xzr, [sp, #64]");                                  // release requests contain no byte section
    emitter.instruction("stp xzr, xzr, [sp, #80]");                             // initialize the result record before the native call
    emitter.instruction("stp xzr, xzr, [sp, #96]");                             // clear result status, ID, and payload prefix
    emitter.instruction("stp xzr, xzr, [sp, #112]");                            // clear result payload and byte pointer fields
    emitter.instruction("stp xzr, xzr, [sp, #128]");                            // clear result byte/value range fields
    emitter.instruction("stp xzr, xzr, [sp, #144]");                            // clear result values/diagnostics pointer fields
    emitter.instruction("stp xzr, xzr, [sp, #160]");                            // clear the remaining result diagnostic fields
    emitter.instruction("mov x0, x12");                                         // first ABI argument is the opaque bridge context
    emitter.instruction("add x1, sp, #32");                                     // second ABI argument points at the flat request
    emitter.instruction("mov x2, #48");                                         // third ABI argument is the exact request length
    emitter.instruction("add x3, sp, #80");                                     // fourth ABI argument points at the fixed result header
    abi::emit_call_label(emitter, &dom_call);
    emitter.instruction("cbnz w0, __rt_dom_wrapper_finalize_fail");             // reject a failed native call status
    emitter.instruction("ldr w9, [sp, #88]");                                   // load the primary status returned in the result header
    emitter.instruction("cbnz w9, __rt_dom_wrapper_finalize_fail");             // finalization must complete without PHP-visible failure
    emitter.instruction("ldr x1, [sp, #104]");                                  // load the independently retained result-frame ID
    emitter.instruction("cbz x1, __rt_dom_wrapper_finalize_ret");               // pointer-free results need no explicit release
    emitter.instruction("ldr x0, [sp, #8]");                                    // restore the owning DOM context ID
    abi::emit_call_label(emitter, &result_release);
    emitter.instruction("b __rt_dom_wrapper_finalize_ret");                     // finish after releasing the native result frame
    emitter.label("__rt_dom_wrapper_finalize_fail");
    emitter.instruction("ldp x29, x30, [sp, #176]");                            // restore the caller frame before the fatal path
    emitter.instruction("add sp, sp, #192");                                    // release finalizer storage before termination
    emitter.instruction("b __rt_dom_bridge_failure");                           // contain a corrupted native boundary as a stable fatal
    emitter.label("__rt_dom_wrapper_finalize_ret");
    emitter.instruction("ldp x29, x30, [sp, #176]");                            // restore the caller frame after wrapper finalization
    emitter.instruction("add sp, sp, #192");                                    // release request/result scratch storage
    emitter.instruction("ret");                                                 // return to ordinary object deep-free processing
}

/// Emits the Linux x86_64 native-wrapper destructor.
fn emit_wrapper_finalize_x86_64(emitter: &mut Emitter) {
    let dom_call = emitter.target.extern_symbol("elephc_dom_call");
    let result_release = emitter.target.extern_symbol("elephc_dom_result_release");
    emitter.blank();
    emitter.comment("--- runtime: DOM wrapper finalization ---");
    emitter.label_global("__rt_dom_wrapper_finalize");
    emitter.instruction("push rbp");                                            // preserve the caller frame and align native calls
    emitter.instruction("mov rbp, rsp");                                        // establish the runtime finalizer frame
    emitter.instruction("sub rsp, 176");                                        // reserve wrapper, request, and result storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // retain the borrowed PHP wrapper pointer
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the wrapper's runtime class ID
    abi::emit_cmp_reg_to_symbol(
        emitter,
        "rax",
        "_class_internal_extension_hidden_offsets_count",
    );
    emitter.instruction("jae __rt_dom_wrapper_finalize_ret");                   // ignore malformed out-of-range class metadata
    abi::emit_symbol_address(emitter, "r10", "_class_internal_extension_hidden_offsets");
    emitter.instruction("mov r10, QWORD PTR [r10 + rax * 8]");                  // load the compiler-hidden metadata tail offset
    emitter.instruction("test r10, r10");                                       // does this class own native wrapper metadata?
    emitter.instruction("jz __rt_dom_wrapper_finalize_ret");                    // ordinary PHP objects have no native handle to release
    emitter.instruction("lea r11, [rdi + r10]");                                // point at the wrapper's compiler-hidden metadata
    emitter.instruction("cmp QWORD PTR [r11 + 64], 0");                         // has this wrapper already been finalized?
    emitter.instruction("jne __rt_dom_wrapper_finalize_ret");                   // never release one wrapper-owned handle twice
    emitter.instruction("mov QWORD PTR [r11 + 64], 1");                         // mark finalization before crossing the re-entrant native ABI
    emitter.instruction("mov rax, QWORD PTR [r11 + 16]");                       // load the opaque bridge execution-context ID
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the context across native calls
    emitter.instruction("mov r10, QWORD PTR [r11 + 32]");                       // load the generation-checked native object handle
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // preserve the receiver handle for request construction
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [r11 + {}]",
        crate::internal_extensions::NATIVE_WRAPPER_AUX_OWNER_OFFSET
    ));                                                                         // load an eager XPath array or namespace-parent owner
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the auxiliary owner before staging independent iterator state
    emitter.instruction(&format!(
        "mov QWORD PTR [r11 + {}], 0",
        crate::internal_extensions::NATIVE_WRAPPER_AUX_OWNER_OFFSET
    ));                                                                         // clear the strong owner before recursive wrapper destruction
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [r11 + {}]",
        crate::internal_extensions::NATIVE_WRAPPER_ITERATOR_CURRENT_OFFSET
    ));                                                                         // stage the independent SimpleXML iterator owner before calls clobber metadata registers
    emitter.instruction(&format!(
        "mov QWORD PTR [r11 + {}], 0",
        crate::internal_extensions::NATIVE_WRAPPER_ITERATOR_CURRENT_OFFSET
    ));                                                                         // clear the iterator owner before its destructor can re-enter the parent
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the staged iterator owner across auxiliary release
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // restore the auxiliary owner after staging the iterator owner
    emitter.instruction("test rax, rax");                                      // does this wrapper retain an XPath auxiliary owner?
    emitter.instruction("jz __rt_dom_wrapper_finalize_no_aux_owner_x86");      // skip ordinary wrappers with no auxiliary owner
    emitter.instruction("mov rdi, rax");                                       // pass the heap-backed owner to generic release
    abi::emit_call_label(emitter, "__rt_decref_any");                          // release arrays or parent wrapper objects by heap kind
    emitter.label("__rt_dom_wrapper_finalize_no_aux_owner_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // restore php-src's private strong iterator-data wrapper
    emitter.instruction("test rax, rax");                                       // does this SimpleXML wrapper retain iterator data?
    emitter.instruction("jz __rt_dom_wrapper_finalize_no_iterator_current_x86"); // skip absent iterator data
    emitter.instruction("call __rt_decref_object");                             // run subclass destruction and release the iterator wrapper
    emitter.label("__rt_dom_wrapper_finalize_no_iterator_current_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // restore the context after iterator-wrapper destruction
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // restore the native handle after iterator-wrapper destruction
    emitter.instruction("test rax, rax");                                       // does the wrapper carry a valid context ID?
    emitter.instruction("jz __rt_dom_wrapper_finalize_ret");                    // an uninitialized wrapper has nothing to release
    emitter.instruction("test r10, r10");                                       // does the wrapper carry a native receiver handle?
    emitter.instruction("jz __rt_dom_wrapper_finalize_ret");                    // a missing handle requires no native release
    emitter.instruction("mov rdi, rax");                                        // pass the owning DOM context ID
    emitter.instruction("mov rsi, r10");                                        // pass the generation-checked native handle
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // pass the exact PHP wrapper pointer
    abi::emit_call_label(emitter, "__rt_dom_wrapper_cache_remove");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // restore the context after cache removal
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // restore the handle after cache removal

    emitter.comment("-- build the fixed empty release request --");
    emitter.instruction("mov DWORD PTR [rbp - 80], 1");                         // request ABI version
    emitter.instruction("mov DWORD PTR [rbp - 76], 48");                        // request header byte size
    emitter.instruction("mov DWORD PTR [rbp - 72], 4110");                      // stable wrapper-release opcode
    emitter.instruction("mov DWORD PTR [rbp - 68], 0");                         // release requests carry no flags
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");                       // receiver is the native wrapper handle
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // release requests contain no values
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // release requests contain no byte section
    for offset in (88..=176).step_by(8) {
        emitter.instruction(&format!("mov QWORD PTR [rbp - {offset}], 0"));     // clear one word of the fixed native result header
    }
    emitter.instruction("mov rdi, rax");                                        // first ABI argument is the opaque bridge context
    emitter.instruction("lea rsi, [rbp - 80]");                                 // second ABI argument points at the flat request
    emitter.instruction("mov rdx, 48");                                         // third ABI argument is the exact request length
    emitter.instruction("lea rcx, [rbp - 176]");                                // fourth ABI argument points at the fixed result header
    abi::emit_call_label(emitter, &dom_call);
    emitter.instruction("test eax, eax");                                       // did the native entry point report success?
    emitter.instruction("jnz __rt_dom_wrapper_finalize_fail");                  // reject a failed native call status
    emitter.instruction("cmp DWORD PTR [rbp - 168], 0");                        // did the result header report a PHP/native failure?
    emitter.instruction("jne __rt_dom_wrapper_finalize_fail");                  // finalization must complete without visible failure
    emitter.instruction("mov rsi, QWORD PTR [rbp - 152]");                      // load the independently retained result-frame ID
    emitter.instruction("test rsi, rsi");                                       // did the bridge retain a result frame?
    emitter.instruction("jz __rt_dom_wrapper_finalize_ret");                    // pointer-free results need no explicit release
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // restore the owning DOM context ID
    abi::emit_call_label(emitter, &result_release);
    emitter.instruction("jmp __rt_dom_wrapper_finalize_ret");                   // finish after releasing the native result frame
    emitter.label("__rt_dom_wrapper_finalize_fail");
    emitter.instruction("mov rsp, rbp");                                        // discard finalizer storage before the fatal path
    emitter.instruction("pop rbp");                                             // restore the caller frame before termination
    emitter.instruction("jmp __rt_dom_bridge_failure");                         // contain a corrupted native boundary as a stable fatal
    emitter.label("__rt_dom_wrapper_finalize_ret");
    emitter.instruction("mov rsp, rbp");                                        // discard request/result scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame after wrapper finalization
    emitter.instruction("ret");                                                 // return to ordinary object deep-free processing
}

/// Emits the stable process-fatal path for an invalid or panicking DOM native boundary.
fn emit_bridge_failure(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: DOM bridge fatal containment ---");
    emitter.label_global("__rt_dom_bridge_failure");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #70");                                 // default native-boundary failure category
            emitter.instruction("b __rt_dom_bridge_failure_code");              // share stable diagnostic emission
        }
        Arch::X86_64 => {
            emitter.instruction("mov eax, 70");                                 // default native-boundary failure category
            emitter.instruction("jmp __rt_dom_bridge_failure_code");            // share stable diagnostic emission
        }
    }
    emitter.label_global("__rt_dom_bridge_failure_code");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x19, x0");                                 // preserve the internal failure category across write()
            abi::emit_symbol_address(emitter, "x1", "_elephc_dom_bridge_failure_msg");
            emitter.instruction("mov x2, #43");                                 // byte length of the stable bridge-failure diagnostic
            emitter.instruction("mov x0, #2");                                  // write the contained native failure to stderr
            emitter.syscall(4);
            emitter.instruction("mov x0, x19");                                 // expose the precise internal containment category to tests
            emitter.syscall(1);
        }
        Arch::X86_64 => {
            emitter.instruction("mov ebx, eax");                                // preserve the internal failure category across write()
            emitter.instruction("mov edi, 2");                                  // write the contained native failure to Linux stderr
            abi::emit_symbol_address(emitter, "rsi", "_elephc_dom_bridge_failure_msg");
            emitter.instruction("mov edx, 43");                                 // byte length of the stable bridge-failure diagnostic
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal DOM bridge diagnostic
            emitter.instruction("mov edi, ebx");                                // expose the precise internal containment category to tests
            emitter.instruction("mov eax, 60");                                 // Linux x86_64 syscall 60 = exit
            emitter.instruction("syscall");                                     // terminate without unwinding across the C ABI
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Verifies fixed DOM ABI sizes used by hand-emitted stack layouts.
    #[test]
    fn test_dom_runtime_abi_sizes_match_bridge_records() {
        assert_eq!(REQUEST_HEADER_SIZE, 48);
        assert_eq!(RESULT_HEADER_SIZE, 96);
    }

    /// Verifies the AArch64 runtime calls platform-mangled bridge entry points.
    #[test]
    fn test_dom_runtime_aarch64_emits_lifecycle_calls() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_dom_runtime(&mut emitter);
        let output = emitter.output();
        assert!(output.contains("__rt_dom_host_call"));
        assert!(output.contains("__rt_dom_host_call_loader_validate"));
        assert!(output.contains("__rt_dom_host_call_stream_read_validate"));
        assert!(output.contains("__rt_dom_host_call_stream_write_validate"));
        assert!(output.contains("__rt_dom_host_call_stream_flush_validate"));
        assert!(output.contains("__rt_dom_host_call_warning_validate"));
        assert!(output.contains("__rt_dom_host_call_xpath_validate"));
        assert!(output.contains("__rt_dom_host_call_xpath_resolve_validate"));
        assert!(output.contains("__rt_dom_host_loader_build_args"));
        assert!(output.contains("__rt_dom_host_xpath_build_args"));
        assert!(output.contains("__rt_dom_host_xpath_build_nodes"));
        assert!(output.contains("__rt_dom_host_xpath_wrapper_from_kind"));
        assert!(output.contains("__rt_dom_host_xpath_result_handle"));
        assert!(output.contains("__rt_dom_host_call_xpath_result_object"));
        assert!(output.contains("_elephc_dom_xpath_object_type_error"));
        assert!(output.contains("bl _setjmp"));
        assert!(output.contains("cbnz x0, __rt_dom_host_call_throw"));
        assert!(output.contains("bl __rt_fread"));
        assert!(output.contains("bl __rt_fwrite"));
        assert!(output.contains("bl __rt_user_wrapper_fflush"));
        assert!(output.contains("bl __rt_diag_warning"));
        assert!(output.contains("bl __rt_dom_xpath_resolve_callable"));
        assert!(output.contains("bl __rt_mixed_from_value"));
        assert!(output.contains("__rt_dom_host_call_stream_open_stat_failed"));
        assert!(output.contains("_user_wrapper_url_stat_failure_kind"));
        assert!(output.contains("_user_wrapper_open_failure_kind"));
        assert!(output.contains("str x11, [x9, #32]"));
        assert!(output.contains("str w10, [x9, #16]"));
        assert!(output.contains("bl __rt_callable_descriptor_release"));
        assert!(output.contains("bl _elephc_dom_context_new"));
        assert!(output.contains("__rt_dom_copy_bytes"));
        assert!(output.contains("bl _elephc_dom_call"));
        assert!(output.contains("bl _elephc_dom_result_release"));
        assert!(output.contains("ldr x11, [x0]"));
        assert!(output.contains("cmp x11, x9"));
        assert!(output.contains("ldr x10, [x10, x11, lsl #3]"));
        assert!(output.contains("ldr x14, [x11, #24]"));
        assert!(output.contains("str xzr, [x11, #24]"));
        assert!(output.contains("bl __rt_decref_any"));
    }

    /// Verifies the Linux x86_64 runtime emits the same bridge lifecycle surface.
    #[test]
    fn test_dom_runtime_x86_64_emits_lifecycle_calls() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_dom_runtime(&mut emitter);
        let output = emitter.output();
        assert!(output.contains("__rt_dom_host_call"));
        assert!(output.contains("__rt_dom_host_call_loader_validate"));
        assert!(output.contains("__rt_dom_host_call_stream_read_validate"));
        assert!(output.contains("__rt_dom_host_call_stream_write_validate"));
        assert!(output.contains("__rt_dom_host_call_stream_flush_validate"));
        assert!(output.contains("__rt_dom_host_call_warning_validate"));
        assert!(output.contains("__rt_dom_host_call_xpath_validate"));
        assert!(output.contains("__rt_dom_host_call_xpath_resolve_validate"));
        assert!(output.contains("__rt_dom_host_loader_build_args"));
        assert!(output.contains("__rt_dom_host_xpath_build_args"));
        assert!(output.contains("__rt_dom_host_xpath_build_nodes"));
        assert!(output.contains("__rt_dom_host_xpath_wrapper_from_kind"));
        assert!(output.contains("__rt_dom_host_xpath_result_handle"));
        assert!(output.contains("__rt_dom_host_call_xpath_result_object"));
        assert!(output.contains("_elephc_dom_xpath_object_type_error"));
        assert!(output.contains("call setjmp"));
        assert!(output.contains("jne __rt_dom_host_call_throw"));
        assert!(output.contains("call __rt_fread"));
        assert!(output.contains("call __rt_fwrite"));
        assert!(output.contains("call __rt_user_wrapper_fflush"));
        assert!(output.contains("call __rt_diag_warning"));
        assert!(output.contains("call __rt_dom_xpath_resolve_callable"));
        assert!(output.contains("call __rt_mixed_from_value"));
        assert!(output.contains("__rt_dom_host_call_stream_open_stat_failed_x86"));
        assert!(output.contains("_user_wrapper_url_stat_failure_kind"));
        assert!(output.contains("_user_wrapper_open_failure_kind"));
        assert!(output.contains("mov QWORD PTR [r9 + 32], r11"));
        assert!(output.contains("mov DWORD PTR [r9 + 16], 6"));
        assert!(output.contains("call __rt_callable_descriptor_release"));
        assert!(output.contains("call elephc_dom_context_new"));
        assert!(output.contains("__rt_dom_copy_bytes"));
        assert!(output.contains("call elephc_dom_call"));
        assert!(output.contains("call elephc_dom_result_release"));
        assert!(output.contains("mov rdi, QWORD PTR [rbp - 64]"));
        assert!(output.contains("mov rdi, QWORD PTR [rbp - 48]"));
        assert!(output.contains("mov QWORD PTR [rbp - 40], rax"));
        assert!(output.contains("mov QWORD PTR [r11 + 24], 0"));
        assert!(output.contains("mov rax, QWORD PTR [rbp - 40]"));
        assert!(output.contains("call __rt_decref_any"));
    }
}
