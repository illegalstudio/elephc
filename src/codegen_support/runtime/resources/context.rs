//! Purpose:
//! Emits typed lookup for resource-registry-backed stream contexts.
//! The helper hides registry offsets from context API lowering.
//!
//! Called from:
//! - `crate::codegen_support::runtime::resources::emit_resource_runtime()`.
//!
//! Key details:
//! - Only Live resources of kind Context resolve to state storage.
//! - Owned states defensively release retained options, reserved params, and
//!   notifier children before freeing the 32-byte state allocation.

use super::layout::{
    CONTEXT_FLAGS_OFFSET, CONTEXT_NOTIFIER_OFFSET, CONTEXT_OPTIONS_OFFSET,
    CONTEXT_PARAMS_OFFSET, CONTEXT_STATE_SIZE, RESOURCE_KIND_CONTEXT, RESOURCE_STATUS_LIVE,
    SLOT_KIND_OFFSET, SLOT_STATE_PTR_OFFSET, SLOT_STATUS_OFFSET,
};
use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the target-specific `__rt_context_state` typed lookup helper.
pub(super) fn emit_context_resources(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_context_state_aarch64(emitter);
            emit_context_destroy_state_aarch64(emitter);
            emit_default_context_ensure_aarch64(emitter);
        }
        Arch::X86_64 => {
            emit_context_state_x86_64(emitter);
            emit_context_destroy_state_x86_64(emitter);
            emit_default_context_ensure_x86_64(emitter);
        }
    }
}

/// Emits AArch64 lazy creation of the request-default stream context.
///
/// # Inputs
/// - none.
///
/// # Outputs
/// - `x0` / `rax`: the request-global default context handle, or `0` if it could not
///   be allocated.
///
/// # Why this exists
/// PHP mints resource id 4 for the request's default stream context, created at the
/// FIRST stream open of any kind and retained for the rest of the request. The compiled
/// side does that inline in `fopen`/`opendir` lowering, but a stream opened INSIDE a
/// runtime-interpreted `eval()` never runs that lowering, so nothing consumed id 4 and
/// every eval resource reported an id one lower than PHP's — `eval('$a = fopen(…)')`
/// answered `4` where PHP 8.5.6 answers `5`. The eval bridge calls this before it boxes
/// a resource, which is the moment eval mints an id.
///
/// # ABI details
/// - Calls `__rt_heap_alloc`, `__rt_resource_alloc` and, on the unwind path,
///   `__rt_heap_free`, so it saves and restores `x30` around them.
/// - The state is allocated ZEROED. The inline lowering hands the newly created state
///   the options and notifier scratch its own call site staged; the request default has
///   neither, which is why the lowering clears both globals before creating it.
fn emit_default_context_ensure_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: lazily create the request-default stream context ---");
    emitter.label_global("__rt_stream_default_context_ensure");
    abi::emit_symbol_address(emitter, "x9", "_stream_default_context_handle");
    emitter.instruction("ldr x0, [x9]");                                        // load the request-global default context handle
    emitter.instruction("cbnz x0, __rt_stream_default_context_ensure_done");    // one context per request: reuse the existing one
    emitter.instruction("sub sp, sp, #16");                                     // reserve the state slot and a link-register save
    emitter.instruction("str x30, [sp, #8]");                                   // save the caller link register across the allocations
    emitter.instruction(&format!("mov x0, #{}", CONTEXT_STATE_SIZE));           // ContextState stores options, params, notifier and flags
    emitter.instruction("bl __rt_heap_alloc");
    emitter.instruction("cbz x0, __rt_stream_default_context_ensure_failed");   // report no context when the state cannot be allocated
    emitter.instruction("str x0, [sp, #0]");                                    // keep ContextState reachable across the registry call
    emitter.instruction(&format!("str xzr, [x0, #{}]", CONTEXT_OPTIONS_OFFSET)); // the request default carries no options
    emitter.instruction(&format!("str xzr, [x0, #{}]", CONTEXT_PARAMS_OFFSET));  // the request default carries no params
    emitter.instruction(&format!("str xzr, [x0, #{}]", CONTEXT_NOTIFIER_OFFSET)); // the request default carries no notifier
    emitter.instruction(&format!("str xzr, [x0, #{}]", CONTEXT_FLAGS_OFFSET));   // ContextState flags start clear
    emitter.instruction("mov x1, x0");                                          // pass ContextState as the registry slot state
    emitter.instruction(&format!("mov x0, #{}", RESOURCE_KIND_CONTEXT));        // registry resource kind 2 = Context
    emitter.instruction("mov x2, #1");                                          // the request default owns its state
    emitter.instruction("bl __rt_resource_alloc");
    emitter.instruction("cbz x0, __rt_stream_default_context_ensure_unwind");   // free ContextState when registry growth fails
    abi::emit_symbol_address(emitter, "x9", "_stream_default_context_handle");
    emitter.instruction("str x0, [x9]");                                        // transfer the creator reference to the request-global owner
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller link register
    emitter.instruction("add sp, sp, #16");                                     // release the creation frame
    emitter.instruction("ret");                                                 // return the freshly created default context handle

    emitter.label("__rt_stream_default_context_ensure_unwind");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the orphaned ContextState
    emitter.instruction("bl __rt_heap_free");
    emitter.instruction("mov x0, #0");                                          // report no context after unwinding the state
    emitter.label("__rt_stream_default_context_ensure_failed");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller link register
    emitter.instruction("add sp, sp, #16");                                     // release the creation frame
    emitter.label("__rt_stream_default_context_ensure_done");
    emitter.instruction("ret");                                                 // return the existing or absent default context handle
}

/// x86_64 counterpart of `emit_default_context_ensure_aarch64`.
fn emit_default_context_ensure_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: lazily create the request-default stream context ---");
    emitter.label_global("__rt_stream_default_context_ensure");
    abi::emit_symbol_address(emitter, "r9", "_stream_default_context_handle");
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // load the request-global default context handle
    emitter.instruction("test rax, rax");                                       // has this request already created its default context?
    emitter.instruction("jnz __rt_stream_default_context_ensure_done_x86");     // one context per request: reuse the existing one
    emitter.instruction("sub rsp, 24");                                         // reserve the state slot and realign for the calls
    emitter.instruction(&format!("mov eax, {}", CONTEXT_STATE_SIZE));           // ContextState stores options, params, notifier and flags
    emitter.instruction("call __rt_heap_alloc");
    emitter.instruction("test rax, rax");                                       // did libc allocate ContextState?
    emitter.instruction("jz __rt_stream_default_context_ensure_failed_x86");    // report no context when the state cannot be allocated
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // keep ContextState reachable across the registry call
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", CONTEXT_OPTIONS_OFFSET)); // the request default carries no options
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", CONTEXT_PARAMS_OFFSET));  // the request default carries no params
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", CONTEXT_NOTIFIER_OFFSET)); // the request default carries no notifier
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", CONTEXT_FLAGS_OFFSET));   // ContextState flags start clear
    emitter.instruction("mov rsi, rax");                                        // pass ContextState as the registry slot state
    emitter.instruction(&format!("mov edi, {}", RESOURCE_KIND_CONTEXT));        // registry resource kind 2 = Context
    emitter.instruction("mov edx, 1");                                          // the request default owns its state
    emitter.instruction("call __rt_resource_alloc");
    emitter.instruction("test rax, rax");                                       // did the registry allocate a generation handle?
    emitter.instruction("jz __rt_stream_default_context_ensure_unwind_x86");    // free ContextState when registry growth fails
    abi::emit_symbol_address(emitter, "r9", "_stream_default_context_handle");
    emitter.instruction("mov QWORD PTR [r9], rax");                             // transfer the creator reference to the request-global owner
    emitter.instruction("add rsp, 24");                                         // release the creation frame
    emitter.instruction("ret");                                                 // return the freshly created default context handle

    emitter.label("__rt_stream_default_context_ensure_unwind_x86");
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // reload the orphaned ContextState
    emitter.instruction("call __rt_heap_free");
    emitter.instruction("xor eax, eax");                                        // report no context after unwinding the state
    emitter.label("__rt_stream_default_context_ensure_failed_x86");
    emitter.instruction("add rsp, 24");                                         // release the creation frame
    emitter.label("__rt_stream_default_context_ensure_done_x86");
    emitter.instruction("ret");                                                 // return the existing or absent default context handle
}

/// Emits AArch64 Live Context state lookup.
fn emit_context_state_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve an opaque stream-context state ---");
    emitter.label_global("__rt_context_state");
    emitter.instruction("sub sp, sp, #16");                                     // preserve the link register around generic lookup
    emitter.instruction("str x30, [sp, #8]");                                   // save the caller link register
    emitter.instruction("bl __rt_resource_lookup_any");                         // validate and resolve the opaque handle
    emitter.instruction("cbz x0, __rt_context_state_fail");                     // reject invalid or stale resources
    emitter.instruction(&format!(
        "ldr x9, [x0, #{}]", SLOT_KIND_OFFSET
    ));                                                                         // load the registry resource kind
    emitter.instruction(&format!(
        "cmp x9, #{}", RESOURCE_KIND_CONTEXT
    ));                                                                         // is the slot a stream context?
    emitter.instruction("b.ne __rt_context_state_fail");                        // reject streams, filters, and other resources
    emitter.instruction(&format!(
        "ldr x9, [x0, #{}]", SLOT_STATUS_OFFSET
    ));                                                                         // load the context lifecycle state
    emitter.instruction(&format!(
        "cmp x9, #{}", RESOURCE_STATUS_LIVE
    ));                                                                         // only Live contexts expose their state
    emitter.instruction("b.ne __rt_context_state_fail");                        // reject Closing and Closed contexts
    emitter.instruction(&format!(
        "ldr x0, [x0, #{}]", SLOT_STATE_PTR_OFFSET
    ));                                                                         // return the stable context-state pointer
    emitter.instruction("b __rt_context_state_done");                           // join the helper epilogue
    emitter.label("__rt_context_state_fail");
    emitter.instruction("mov x0, #0");                                          // return null for invalid context resources
    emitter.label("__rt_context_state_done");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller link register
    emitter.instruction("add sp, sp, #16");                                     // release the aligned link-register save
    emitter.instruction("ret");                                                 // return the context-state pointer or null
}

/// Emits Linux x86_64 Live Context state lookup.
fn emit_context_state_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve an opaque stream-context state ---");
    emitter.label_global("__rt_context_state");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable context lookup frame
    emitter.instruction("call __rt_resource_lookup_any");                       // validate and resolve the opaque handle
    emitter.instruction("test rax, rax");                                       // did lookup resolve a registry slot?
    emitter.instruction("jz __rt_context_state_fail");                          // reject invalid or stale resources
    emitter.instruction(&format!(
        "cmp QWORD PTR [rax + {}], {}",
        SLOT_KIND_OFFSET, RESOURCE_KIND_CONTEXT
    ));                                                                         // is the slot a stream context?
    emitter.instruction("jne __rt_context_state_fail");                         // reject streams, filters, and other resources
    emitter.instruction(&format!(
        "cmp QWORD PTR [rax + {}], {}",
        SLOT_STATUS_OFFSET, RESOURCE_STATUS_LIVE
    ));                                                                         // only Live contexts expose their state
    emitter.instruction("jne __rt_context_state_fail");                         // reject Closing and Closed contexts
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [rax + {}]",
        SLOT_STATE_PTR_OFFSET
    ));                                                                         // return the stable context-state pointer
    emitter.instruction("jmp __rt_context_state_done");                         // join the helper epilogue
    emitter.label("__rt_context_state_fail");
    emitter.instruction("xor eax, eax");                                        // return null for invalid context resources
    emitter.label("__rt_context_state_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the context-state pointer or null
}

/// Emits AArch64 owned ContextState teardown in child-before-parent order.
fn emit_context_destroy_state_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: destroy an owned stream-context state ---");
    emitter.label_global("__rt_context_destroy_state");
    emitter.instruction("cbz x0, __rt_context_destroy_state_done");             // null context states own no children or storage
    emitter.instruction("sub sp, sp, #32");                                     // reserve stable state storage and a saved frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the caller frame and link register
    emitter.instruction("add x29, sp, #16");                                    // establish a stable teardown frame
    emitter.instruction("str x0, [sp, #0]");                                    // preserve ContextState across nested releases
    emitter.instruction(&format!(
        "ldr x0, [x0, #{}]",
        CONTEXT_OPTIONS_OFFSET
    ));                                                                         // load the retained options hash
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload ContextState before clearing ownership
    emitter.instruction(&format!(
        "str xzr, [x9, #{}]",
        CONTEXT_OPTIONS_OFFSET
    ));                                                                         // detach options before potentially re-entrant release
    emitter.instruction("cbz x0, __rt_context_destroy_state_params");           // skip an absent options value
    emitter.instruction("bl __rt_decref_any");                                  // release the retained options hash
    emitter.label("__rt_context_destroy_state_params");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload ContextState for reserved params teardown
    emitter.instruction(&format!(
        "ldr x0, [x9, #{}]",
        CONTEXT_PARAMS_OFFSET
    ));                                                                         // load the defensively retained params payload
    emitter.instruction(&format!(
        "str xzr, [x9, #{}]",
        CONTEXT_PARAMS_OFFSET
    ));                                                                         // detach params before its potentially re-entrant release
    emitter.instruction("cbz x0, __rt_context_destroy_state_notifier");         // skip the normally empty reserved params slot
    emitter.instruction("bl __rt_decref_any");                                  // release a future retained params payload exactly once
    emitter.label("__rt_context_destroy_state_notifier");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload ContextState for notifier teardown
    emitter.instruction(&format!(
        "ldr x0, [x9, #{}]",
        CONTEXT_NOTIFIER_OFFSET
    ));                                                                         // load the retained notification descriptor
    emitter.instruction(&format!(
        "str xzr, [x9, #{}]",
        CONTEXT_NOTIFIER_OFFSET
    ));                                                                         // detach notifier before its potentially re-entrant release
    emitter.instruction("cbz x0, __rt_context_destroy_state_storage");          // skip an absent notification descriptor
    emitter.instruction("bl __rt_callable_descriptor_release");                 // release the retained callable descriptor
    emitter.label("__rt_context_destroy_state_storage");
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass ContextState itself to the heap allocator
    emitter.instruction("bl __rt_heap_free");                                   // release the owned 32-byte state allocation
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #32");                                     // release teardown scratch storage
    emitter.label("__rt_context_destroy_state_done");
    emitter.instruction("ret");                                                 // return after exact-once child and state teardown
}

/// Emits Linux x86_64 owned ContextState teardown in child-before-parent order.
fn emit_context_destroy_state_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: destroy an owned stream-context state ---");
    emitter.label_global("__rt_context_destroy_state");
    emitter.instruction("test rax, rax");                                       // do null context states own any storage?
    emitter.instruction("jz __rt_context_destroy_state_done");                  // no, return without entering a teardown frame
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable teardown frame
    emitter.instruction("sub rsp, 16");                                         // reserve aligned ContextState storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve ContextState across nested releases
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [rax + {}]",
        CONTEXT_OPTIONS_OFFSET
    ));                                                                         // load the retained options hash
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload ContextState before clearing ownership
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], 0",
        CONTEXT_OPTIONS_OFFSET
    ));                                                                         // detach options before potentially re-entrant release
    emitter.instruction("test rax, rax");                                       // was an options hash retained?
    emitter.instruction("jz __rt_context_destroy_state_params");                // skip an absent options value
    emitter.instruction("call __rt_decref_any");                                // release the retained options hash
    emitter.label("__rt_context_destroy_state_params");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload ContextState for reserved params teardown
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [r10 + {}]",
        CONTEXT_PARAMS_OFFSET
    ));                                                                         // load the defensively retained params payload
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], 0",
        CONTEXT_PARAMS_OFFSET
    ));                                                                         // detach params before its potentially re-entrant release
    emitter.instruction("test rax, rax");                                       // does the reserved params slot own a payload?
    emitter.instruction("jz __rt_context_destroy_state_notifier");              // skip the normally empty reserved params slot
    emitter.instruction("call __rt_decref_any");                                // release a future retained params payload exactly once
    emitter.label("__rt_context_destroy_state_notifier");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload ContextState for notifier teardown
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [r10 + {}]",
        CONTEXT_NOTIFIER_OFFSET
    ));                                                                         // load the retained notification descriptor
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], 0",
        CONTEXT_NOTIFIER_OFFSET
    ));                                                                         // detach notifier before its potentially re-entrant release
    emitter.instruction("test rax, rax");                                       // was a notification descriptor retained?
    emitter.instruction("jz __rt_context_destroy_state_storage");               // skip an absent notification descriptor
    emitter.instruction("call __rt_callable_descriptor_release");               // release the retained callable descriptor
    emitter.label("__rt_context_destroy_state_storage");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // pass ContextState itself to the heap allocator
    emitter.instruction("call __rt_heap_free");                                 // release the owned 32-byte state allocation
    emitter.instruction("add rsp, 16");                                         // release teardown scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.label("__rt_context_destroy_state_done");
    emitter.instruction("ret");                                                 // return after exact-once child and state teardown
}
