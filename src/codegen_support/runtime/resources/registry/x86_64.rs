//! Purpose:
//! Emits the Linux x86_64 System V implementation of the dynamic resource registry.
//! Its observable handle and slot layout matches the AArch64 implementation.
//!
//! Called from:
//! - `crate::codegen_support::runtime::resources::registry::emit_resource_registry()`.
//!
//! Key details:
//! - Heap-backed growth copies initialized slots without changing their indices.
//! - Retain returns the original handle in `rax`; release may clobber caller-saved registers.

use super::super::layout::{
    HANDLE_INDEX_BITS, INITIAL_REGISTRY_CAPACITY, RESOURCE_FLAG_OWNS_STATE,
    RESOURCE_FLAG_PERSISTENT, RESOURCE_KIND_CONTEXT, RESOURCE_KIND_STREAM,
    RESOURCE_REFS_IMMORTAL, RESOURCE_SLOT_SIZE, RESOURCE_STATUS_CLOSED,
    RESOURCE_STATUS_CLOSING, RESOURCE_STATUS_LIVE, SLOT_FLAGS_OFFSET,
    SLOT_GENERATION_OFFSET, SLOT_KIND_OFFSET, SLOT_NEXT_FREE_OFFSET, SLOT_PHP_ID_OFFSET,
    SLOT_REFS_OFFSET, SLOT_REQUEST_EPOCH_OFFSET, SLOT_STATE_PTR_OFFSET, SLOT_STATUS_OFFSET,
    STANDARD_STREAM_COUNT, STREAM_BACKEND_FD, STREAM_OWNERSHIP_FLAGS_OFFSET, STREAM_STATE_SIZE,
    STREAM_URI_LEN_OFFSET, STREAM_URI_PTR_OFFSET, STREAM_WRAPPER_ID_OFFSET,
};
use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;

/// Emits every Linux x86_64 resource-registry entry point.
pub(super) fn emit_resource_registry_x86_64(emitter: &mut Emitter) {
    emit_registry_init(emitter);
    emit_registry_grow(emitter);
    emit_resource_alloc(emitter);
    emit_resource_lookup_any(emitter);
    emit_resource_retain(emitter);
    emit_resource_release(emitter);
    emit_registry_request_reset(emitter);
    emit_registry_teardown(emitter);
    emit_resource_mark_closing(emitter);
    emit_resource_mark_closed(emitter);
    emit_resource_id_of_registry(emitter);
    emit_resource_is_open(emitter);
    emit_resource_kind_if_open(emitter);
}

/// Emits lazy x86_64 registry initialization and standard-stream slot installation.
fn emit_registry_init(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: initialize the opaque resource registry ---");
    emitter.label_global("__rt_resource_registry_init");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable initialization frame
    emitter.instruction("sub rsp, 64");                                         // reserve aligned initialization scratch storage
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_ptr");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the current dynamic slot-array pointer
    emitter.instruction("test r11, r11");                                       // has the registry already been initialized?
    emitter.instruction("jnz __rt_resource_registry_init_ready");               // reuse the existing dynamic registry
    // The initial slot array is the STATIC reservation, not a heap block: a program
    // must not need runtime heap before its first statement (see `data::fixed`).
    abi::emit_symbol_address(emitter, "rax", "_resource_registry_static_slots");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the static slot-array base
    emitter.instruction("mov r10, rax");                                        // start zeroing at the first slot byte
    emitter.instruction(&format!(
        "mov ecx, {}",
        INITIAL_REGISTRY_CAPACITY * RESOURCE_SLOT_SIZE / 8
    ));                                                                         // count zeroed machine words
    emitter.label("__rt_resource_registry_init_zero");
    emitter.instruction("mov QWORD PTR [r10], 0");                              // clear one slot-array word
    emitter.instruction("add r10, 8");                                          // advance to the next slot-array word
    emitter.instruction("sub rcx, 1");                                          // consume one word from the zeroing count
    emitter.instruction("jnz __rt_resource_registry_init_zero");                // clear the complete initial allocation
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the initialized slot-array pointer
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_ptr");
    emitter.instruction("mov QWORD PTR [r10], r11");                            // publish the dynamic registry pointer
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_len");
    emitter.instruction(&format!(
        "mov QWORD PTR [r10], {}", STANDARD_STREAM_COUNT
    ));                                                                         // publish the initialized standard slots
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_cap");
    emitter.instruction(&format!(
        "mov QWORD PTR [r10], {}",
        INITIAL_REGISTRY_CAPACITY
    ));                                                                         // publish the initial slot capacity
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_free");
    emitter.instruction("mov QWORD PTR [r10], 0");                              // initialize the one-based free-list head as empty
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_live");
    emitter.instruction(&format!(
        "mov QWORD PTR [r10], {}", STANDARD_STREAM_COUNT
    ));                                                                         // count standard streams as live
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_epoch");
    emitter.instruction("cmp QWORD PTR [r10], 0");                              // has a request epoch already been published?
    emitter.instruction("jne __rt_resource_registry_init_epoch_ready");         // preserve a previously advanced request epoch
    emitter.instruction("mov QWORD PTR [r10], 1");                              // request epochs start at one
    emitter.label("__rt_resource_registry_init_epoch_ready");
    abi::emit_symbol_address(emitter, "r10", "_resource_std_stream_states");
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the standard-stream state base
    emitter.instruction("xor r8d, r8d");                                        // initialize standard-stream slot index zero
    emitter.label("__rt_resource_registry_init_std_loop");
    emitter.instruction("mov r9, r8");                                          // copy the standard-stream slot index
    emitter.instruction("shl r9, 6");                                           // convert the slot index to a 64-byte offset
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the dynamic slot-array pointer
    emitter.instruction("add r11, r9");                                         // address the standard-stream registry slot
    emitter.instruction("mov QWORD PTR [r11], 1");                              // standard opaque handles begin at generation one
    emitter.instruction(&format!(
        "mov QWORD PTR [r11 + 8], {}", RESOURCE_KIND_STREAM
    ));                                                                         // store the stream resource kind
    emitter.instruction(&format!(
        "mov QWORD PTR [r11 + 16], {}", RESOURCE_STATUS_LIVE
    ));                                                                         // mark the standard stream live
    emitter.instruction(&format!(
        "mov QWORD PTR [r11 + 24], {}", RESOURCE_REFS_IMMORTAL
    ));                                                                         // prevent standard-stream release
    emitter.instruction("lea r9, [r8 + 1]");                                    // PHP ids for standard streams are one through three
    emitter.instruction("mov QWORD PTR [r11 + 32], r9");                        // store the PHP-visible resource id
    emitter.instruction("mov r9, r8");                                          // copy the standard-stream state index
    emitter.instruction(&format!(
        "imul r9, {}", STREAM_STATE_SIZE
    ));                                                                         // compute the standard stream-state byte offset
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the static standard-stream state base
    emitter.instruction("add r10, r9");                                         // address this standard stream's stable state
    emitter.instruction("mov QWORD PTR [r11 + 40], r10");                       // bind the registry slot to its stream state
    emitter.instruction("mov QWORD PTR [r11 + 48], 0");                         // standard slots never enter the free list
    emitter.instruction(&format!(
        "mov QWORD PTR [r11 + 56], {}",
        RESOURCE_FLAG_PERSISTENT
    ));                                                                         // mark the registry slot process-persistent
    emitter.instruction(&format!(
        "mov QWORD PTR [r10], {}", STREAM_BACKEND_FD
    ));                                                                         // select the direct-descriptor backend
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], 6", STREAM_WRAPPER_ID_OFFSET
    ));                                                                         // standard streams use PHP's php:// wrapper
    emitter.instruction("mov QWORD PTR [r10 + 16], r8");                        // store descriptor zero, one, or two
    abi::emit_symbol_address(emitter, "r9", "_resource_std_stream_uri_ptrs");
    emitter.instruction("mov rax, QWORD PTR [r9 + r8 * 8]");                    // load this standard stream's static URI pointer
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], rax", STREAM_URI_PTR_OFFSET
    ));                                                                         // publish the standard stream URI pointer
    abi::emit_symbol_address(emitter, "r9", "_resource_std_stream_uri_lens");
    emitter.instruction("mov rax, QWORD PTR [r9 + r8 * 8]");                    // load this standard stream's static URI length
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], rax", STREAM_URI_LEN_OFFSET
    ));                                                                         // publish the standard stream URI length
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {}], {}",
        STREAM_OWNERSHIP_FLAGS_OFFSET, RESOURCE_FLAG_PERSISTENT
    ));                                                                         // mark the stream state process-persistent
    emitter.instruction("add r8, 1");                                           // advance to the next standard stream
    emitter.instruction(&format!(
        "cmp r8, {}", STANDARD_STREAM_COUNT
    ));                                                                         // have all standard streams been installed?
    emitter.instruction("jb __rt_resource_registry_init_std_loop");             // initialize the remaining standard slots
    emitter.label("__rt_resource_registry_init_ready");
    emitter.instruction("mov eax, 1");                                          // report successful initialization
    emitter.instruction("jmp __rt_resource_registry_init_done");                // join the common initialization epilogue
    emitter.label("__rt_resource_registry_init_fail");
    emitter.instruction("xor eax, eax");                                        // report allocation failure without registry publication
    emitter.label("__rt_resource_registry_init_done");
    emitter.instruction("add rsp, 64");                                         // release initialization scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the registry caller
}

/// Emits x86_64 registry growth by allocate, zero, copy, publish, and free.
fn emit_registry_grow(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: grow the opaque resource registry ---");
    emitter.label_global("__rt_resource_registry_grow");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable growth frame
    emitter.instruction("sub rsp, 64");                                         // reserve aligned growth scratch storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the requested minimum capacity
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_ptr");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the old slot-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // preserve the old slot-array pointer
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_cap");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the old slot capacity
    emitter.label("__rt_resource_registry_grow_capacity");
    emitter.instruction("cmp r11, QWORD PTR [rbp - 8]");                        // does the proposed capacity satisfy the request?
    emitter.instruction("jae __rt_resource_registry_grow_alloc");               // allocate once the capacity is large enough
    emitter.instruction("add r11, r11");                                        // double the dynamic slot capacity
    emitter.instruction("jz __rt_resource_registry_grow_fail");                 // reject integer wrap
    emitter.instruction("jmp __rt_resource_registry_grow_capacity");            // continue growing to the requested minimum
    emitter.label("__rt_resource_registry_grow_alloc");
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // preserve the chosen replacement capacity
    emitter.instruction("mov rax, r11");                                        // copy the slot count into the heap-size argument
    emitter.instruction("shl rax, 6");                                          // convert the slot capacity to bytes
    emitter.instruction("call __rt_heap_alloc");                                // allocate the replacement slot array
    emitter.instruction("test rax, rax");                                       // did allocation return storage?
    emitter.instruction("jz __rt_resource_registry_grow_fail");                 // preserve the old registry after failure
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the replacement slot-array pointer
    emitter.instruction("mov r10, rax");                                        // start zeroing at the replacement allocation
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the replacement capacity
    emitter.instruction("shl rcx, 3");                                          // count 64-byte slots as eight-byte words
    emitter.label("__rt_resource_registry_grow_zero");
    emitter.instruction("mov QWORD PTR [r10], 0");                              // clear one replacement-array word
    emitter.instruction("add r10, 8");                                          // advance the zeroing cursor
    emitter.instruction("sub rcx, 1");                                          // consume one zeroed word
    emitter.instruction("jnz __rt_resource_registry_grow_zero");                // clear the complete replacement array
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the old slot-array pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the replacement slot-array pointer
    abi::emit_symbol_address(emitter, "r9", "_resource_registry_len");
    emitter.instruction("mov rcx, QWORD PTR [r9]");                             // load the number of initialized slots
    emitter.instruction("shl rcx, 3");                                          // convert 64-byte slots to eight-byte words
    emitter.label("__rt_resource_registry_grow_copy");
    emitter.instruction("test rcx, rcx");                                       // have all initialized words been copied?
    emitter.instruction("jz __rt_resource_registry_grow_publish");              // publish after copying every initialized slot
    emitter.instruction("mov r8, QWORD PTR [r10]");                             // load one old registry word
    emitter.instruction("mov QWORD PTR [r11], r8");                             // copy the word into the replacement
    emitter.instruction("add r10, 8");                                          // advance the old-array cursor
    emitter.instruction("add r11, 8");                                          // advance the replacement-array cursor
    emitter.instruction("sub rcx, 1");                                          // consume one copied word
    emitter.instruction("jmp __rt_resource_registry_grow_copy");                // copy the remaining initialized words
    emitter.label("__rt_resource_registry_grow_publish");
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_ptr");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the replacement slot-array pointer
    emitter.instruction("mov QWORD PTR [r10], r11");                            // publish the replacement registry base
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_cap");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the replacement capacity
    emitter.instruction("mov QWORD PTR [r10], r11");                            // publish the replacement capacity
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the superseded slot array
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_static_slots");
    emitter.instruction("cmp rax, r10");                                        // was the superseded array the STATIC reservation?
    emitter.instruction("je __rt_resource_registry_grow_static_x86");           // the static base is not heap storage and must never be freed
    emitter.instruction("call __rt_heap_free");                                 // release the old dynamic slot array
    emitter.label("__rt_resource_registry_grow_static_x86");
    emitter.instruction("mov eax, 1");                                          // report successful growth
    emitter.instruction("jmp __rt_resource_registry_grow_done");                // join the helper epilogue
    emitter.label("__rt_resource_registry_grow_fail");
    emitter.instruction("xor eax, eax");                                        // report growth failure
    emitter.label("__rt_resource_registry_grow_done");
    emitter.instruction("add rsp, 64");                                         // release growth scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the growth status
}

/// Emits x86_64 allocation of a live resource slot and opaque handle.
fn emit_resource_alloc(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: allocate an opaque resource handle ---");
    emitter.label_global("__rt_resource_alloc");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable allocation frame
    emitter.instruction("sub rsp, 80");                                         // reserve aligned allocation scratch storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the requested resource kind
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the stable state pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve resource ownership flags
    emitter.instruction("test rdi, rdi");                                       // resource kind zero is reserved for free slots
    emitter.instruction("jz __rt_resource_alloc_fail");                         // reject allocation of a free-kind resource
    emitter.instruction("call __rt_resource_registry_init");                    // lazily install registry storage and standard streams
    emitter.instruction("test rax, rax");                                       // did registry initialization succeed?
    emitter.instruction("jz __rt_resource_alloc_fail");                         // propagate initialization failure
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_free");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the one-based free-list head
    emitter.instruction("test r11, r11");                                       // is a recycled slot available?
    emitter.instruction("jz __rt_resource_alloc_append");                       // append when the free list is empty
    emitter.instruction("sub r11, 1");                                          // convert the free-list head to a zero-based index
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // preserve the selected slot index
    abi::emit_symbol_address(emitter, "r9", "_resource_registry_ptr");
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // load the dynamic slot-array pointer
    emitter.instruction("mov rcx, r11");                                        // copy the slot index for byte-offset calculation
    emitter.instruction("shl rcx, 6");                                          // convert the index to a 64-byte offset
    emitter.instruction("add r9, rcx");                                         // address the selected free slot
    emitter.instruction(&format!(
        "mov rcx, QWORD PTR [r9 + {}]",
        SLOT_NEXT_FREE_OFFSET
    ));                                                                         // load the next one-based free-list link
    emitter.instruction("mov QWORD PTR [r10], rcx");                            // pop the selected slot from the free list
    emitter.instruction("jmp __rt_resource_alloc_fill");                        // initialize the reused slot
    emitter.label("__rt_resource_alloc_append");
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_len");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the next never-initialized slot index
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // preserve the append slot index
    abi::emit_symbol_address(emitter, "r9", "_resource_registry_cap");
    emitter.instruction("cmp r11, QWORD PTR [r9]");                             // is the slot array already full?
    emitter.instruction("jb __rt_resource_alloc_append_ready");                 // append directly when capacity remains
    emitter.instruction("lea rdi, [r11 + 1]");                                  // request room for the new one-based slot
    emitter.instruction("call __rt_resource_registry_grow");                    // grow without changing existing slot indices
    emitter.instruction("test rax, rax");                                       // did registry growth succeed?
    emitter.instruction("jz __rt_resource_alloc_fail");                         // report allocation failure after growth failure
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_len");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // restore the append slot index
    emitter.label("__rt_resource_alloc_append_ready");
    emitter.instruction("lea r9, [r11 + 1]");                                   // advance the initialized slot count
    emitter.instruction("mov QWORD PTR [r10], r9");                             // publish the appended slot
    abi::emit_symbol_address(emitter, "r9", "_resource_registry_ptr");
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // reload the possibly grown slot-array pointer
    emitter.instruction("mov rcx, r11");                                        // copy the append slot index
    emitter.instruction("shl rcx, 6");                                          // convert the index to a 64-byte offset
    emitter.instruction("add r9, rcx");                                         // address the appended slot
    emitter.label("__rt_resource_alloc_fill");
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [r9 + {}]",
        SLOT_GENERATION_OFFSET
    ));                                                                         // load the current slot generation
    emitter.instruction("test r10, r10");                                       // has this slot been used before?
    emitter.instruction("jnz __rt_resource_alloc_generation_ready");            // preserve a generation advanced by release
    emitter.instruction("mov r10, 1");                                          // first-use generations start at one
    emitter.label("__rt_resource_alloc_generation_ready");
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {}], r10",
        SLOT_GENERATION_OFFSET
    ));                                                                         // publish the live slot generation
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the requested resource kind
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {}], r11", SLOT_KIND_OFFSET
    ));                                                                         // publish the resource kind
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {}], {}",
        SLOT_STATUS_OFFSET, RESOURCE_STATUS_LIVE
    ));                                                                         // publish the Live lifecycle state
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {}], 1", SLOT_REFS_OFFSET
    ));                                                                         // allocation returns one owned reference
    abi::emit_symbol_address(emitter, "r11", "_resource_id_next");
    emitter.instruction("mov rcx, QWORD PTR [r11]");                            // mint the next PHP-visible resource id
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {}], rcx", SLOT_PHP_ID_OFFSET
    ));                                                                         // bind the PHP id to this incarnation
    emitter.instruction("add rcx, 1");                                          // advance the never-reused PHP id cursor
    emitter.instruction("mov QWORD PTR [r11], rcx");                            // publish the next resource id
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the stable state pointer
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {}], rcx",
        SLOT_STATE_PTR_OFFSET
    ));                                                                         // bind the resource state
    abi::emit_symbol_address(emitter, "r11", "_resource_registry_epoch");
    emitter.instruction("mov rcx, QWORD PTR [r11]");                            // load the request epoch owning this allocation
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {}], rcx",
        SLOT_REQUEST_EPOCH_OFFSET
    ));                                                                         // replace stale free-list linkage with request ownership
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload resource ownership flags
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {}], rcx", SLOT_FLAGS_OFFSET
    ));                                                                         // publish ownership flags
    abi::emit_symbol_address(emitter, "r11", "_resource_registry_live");
    emitter.instruction("add QWORD PTR [r11], 1");                              // include the allocated resource in the live count
    emitter.instruction(&format!(
        "shl r10, {}", HANDLE_INDEX_BITS
    ));                                                                         // place the generation in the high handle word
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the zero-based slot index
    emitter.instruction("add r11, 1");                                          // encode a non-zero one-based slot
    emitter.instruction("or r10, r11");                                         // combine generation and slot index
    emitter.instruction("mov rax, r10");                                        // return the opaque handle
    emitter.instruction("jmp __rt_resource_alloc_done");                        // join the allocation epilogue
    emitter.label("__rt_resource_alloc_fail");
    emitter.instruction("xor eax, eax");                                        // return the invalid handle on failure
    emitter.label("__rt_resource_alloc_done");
    emitter.instruction("add rsp, 80");                                         // release allocation scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the opaque handle or zero
}

/// Emits leaf x86_64 opaque-handle validation and slot lookup.
fn emit_resource_lookup_any(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: validate and resolve an opaque resource handle ---");
    emitter.label_global("__rt_resource_lookup_any");
    emitter.instruction("test rdi, rdi");                                       // handle zero is always invalid
    emitter.instruction("jz __rt_resource_lookup_any_fail");                    // reject the invalid handle
    emitter.instruction("mov ecx, edi");                                        // extract the one-based low handle word
    emitter.instruction("test ecx, ecx");                                       // is the one-based slot component zero?
    emitter.instruction("jz __rt_resource_lookup_any_fail");                    // reject an invalid zero slot component
    emitter.instruction("sub rcx, 1");                                          // convert to a zero-based slot index
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_len");
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // is the requested slot initialized?
    emitter.instruction("jae __rt_resource_lookup_any_fail");                   // reject out-of-range slot indices
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_ptr");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the dynamic slot-array pointer
    emitter.instruction("test r10, r10");                                       // has registry storage been initialized?
    emitter.instruction("jz __rt_resource_lookup_any_fail");                    // reject lookup before initialization
    emitter.instruction("shl rcx, 6");                                          // convert the slot index to a 64-byte offset
    emitter.instruction("add r10, rcx");                                        // address the selected registry slot
    emitter.instruction("mov r11, rdi");                                        // copy the opaque handle
    emitter.instruction(&format!(
        "shr r11, {}", HANDLE_INDEX_BITS
    ));                                                                         // extract the handle generation
    emitter.instruction(&format!(
        "cmp r11d, DWORD PTR [r10 + {}]",
        SLOT_GENERATION_OFFSET
    ));                                                                         // compare handle and slot generations
    emitter.instruction("jne __rt_resource_lookup_any_fail");                   // reject stale or recycled handles
    emitter.instruction(&format!(
        "cmp QWORD PTR [r10 + {}], 0", SLOT_KIND_OFFSET
    ));                                                                         // is the slot free?
    emitter.instruction("je __rt_resource_lookup_any_fail");                    // reject free slots
    emitter.instruction("mov rax, r10");                                        // return the validated registry slot pointer
    emitter.instruction("ret");                                                 // finish the successful leaf lookup
    emitter.label("__rt_resource_lookup_any_fail");
    emitter.instruction("xor eax, eax");                                        // return null for invalid or stale handles
    emitter.instruction("ret");                                                 // finish the failed leaf lookup
}

/// Emits x86_64 strong-reference acquisition while returning the original handle.
fn emit_resource_retain(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: retain an opaque resource handle ---");
    emitter.label_global("__rt_resource_retain");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable retain frame
    emitter.instruction("sub rsp, 16");                                         // reserve an aligned original-handle slot
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the exact incoming handle
    emitter.instruction("call __rt_resource_lookup_any");                       // validate the handle generation and slot
    emitter.instruction("test rax, rax");                                       // did lookup resolve a registry slot?
    emitter.instruction("jz __rt_resource_retain_done");                        // invalid handles acquire no ownership
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {}]", SLOT_REFS_OFFSET
    ));                                                                         // load the reference count
    emitter.instruction(&format!(
        "cmp r10, {}", RESOURCE_REFS_IMMORTAL
    ));                                                                         // is this a persistent immortal resource?
    emitter.instruction("je __rt_resource_retain_done");                        // persistent resources never change ownership
    emitter.instruction("add r10, 1");                                          // acquire one strong reference
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], r10", SLOT_REFS_OFFSET
    ));                                                                         // publish the acquired reference
    emitter.label("__rt_resource_retain_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the exact original opaque handle
    emitter.instruction("add rsp, 16");                                         // release retain scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the preserved handle
}

/// Emits x86_64 reference release and slot recycling without backend dispatch.
fn emit_resource_release(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: release an opaque resource handle ---");
    emitter.label_global("__rt_resource_release");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable release frame
    emitter.instruction("sub rsp, 64");                                         // reserve aligned release scratch storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the retiring opaque handle
    emitter.instruction("call __rt_resource_lookup_any");                       // validate and resolve the resource slot
    emitter.instruction("test rax, rax");                                       // did lookup resolve a registry slot?
    emitter.instruction("jz __rt_resource_release_done");                       // stale handles release no ownership
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the resolved slot pointer
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {}]", SLOT_REFS_OFFSET
    ));                                                                         // load the strong-reference count
    emitter.instruction(&format!(
        "cmp r10, {}", RESOURCE_REFS_IMMORTAL
    ));                                                                         // is this a persistent immortal resource?
    emitter.instruction("je __rt_resource_release_done");                       // never release standard-stream ownership
    emitter.instruction("test r10, r10");                                       // is ownership already exhausted?
    emitter.instruction("jz __rt_resource_release_done");                       // tolerate a defensive duplicate release
    emitter.instruction("sub r10, 1");                                          // release one strong reference
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], r10", SLOT_REFS_OFFSET
    ));                                                                         // publish the decremented reference count
    emitter.instruction("jnz __rt_resource_release_done");                      // keep the slot while another owner remains
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {}]", SLOT_STATUS_OFFSET
    ));                                                                         // load lifecycle state before final release
    emitter.instruction(&format!(
        "cmp r10, {}", RESOURCE_STATUS_CLOSING
    ));                                                                         // is a re-entrant close still executing?
    emitter.instruction("je __rt_resource_release_done");                       // defer recycling until the active close completes
    emitter.instruction(&format!(
        "cmp QWORD PTR [rax + {}], {}",
        SLOT_KIND_OFFSET, RESOURCE_KIND_STREAM
    ));                                                                         // does this slot own a stream backend?
    emitter.instruction("jne __rt_resource_release_after_close");               // non-stream resources have no Gate 1 backend hook
    emitter.instruction(&format!(
        "cmp r10, {}", RESOURCE_STATUS_LIVE
    ));                                                                         // does the stream still require exact-once close?
    emitter.instruction("jne __rt_resource_release_after_close");               // already-closed streams skip backend dispatch
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle
    emitter.instruction("call __rt_stream_close_backend");                      // close the supported backend before recycling state
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the handle after close dispatch
    emitter.instruction("call __rt_resource_lookup_any");                       // re-resolve the slot after nested lifecycle helpers
    emitter.instruction("test rax, rax");                                       // is the slot still valid?
    emitter.instruction("jz __rt_resource_release_done");                       // tolerate defensive invalidation during close
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // refresh the resolved slot pointer
    emitter.label("__rt_resource_release_after_close");
    emitter.instruction(&format!(
        "cmp QWORD PTR [rax + {}], {}",
        SLOT_KIND_OFFSET, RESOURCE_KIND_STREAM
    ));                                                                         // does this slot own a StreamState aggregate?
    emitter.instruction("jne __rt_resource_release_context_state");             // non-stream resources follow their own teardown path
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [rax + {}]", SLOT_FLAGS_OFFSET
    ));                                                                         // load stream-state ownership flags
    emitter.instruction(&format!(
        "test r11, {}", RESOURCE_FLAG_OWNS_STATE
    ));                                                                         // does the registry own this StreamState?
    emitter.instruction("jz __rt_resource_release_recycle");                    // persistent or borrowed stream states stay externally owned
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [rax + {}]", SLOT_STATE_PTR_OFFSET
    ));                                                                         // load the owned StreamState pointer
    emitter.instruction("test r11, r11");                                       // is a StreamState allocation attached?
    emitter.instruction("jz __rt_resource_release_recycle");                    // absent stream state requires no destructor
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], 0",
        SLOT_STATE_PTR_OFFSET
    ));                                                                         // detach state before potentially re-entrant child cleanup
    emitter.instruction("mov rax, r11");                                        // pass StreamState to its typed destructor
    emitter.instruction("call __rt_stream_destroy_state");                      // release URI, host, then StreamState exactly once
    emitter.instruction("jmp __rt_resource_release_after_state_destroy");       // re-resolve after potentially re-entrant child teardown
    emitter.label("__rt_resource_release_context_state");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the resolved registry slot
    emitter.instruction(&format!(
        "cmp QWORD PTR [rax + {}], {}",
        SLOT_KIND_OFFSET, RESOURCE_KIND_CONTEXT
    ));                                                                         // does this slot own a ContextState aggregate?
    emitter.instruction("jne __rt_resource_release_generic_state");             // other kinds use ordinary state allocation teardown
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [rax + {}]", SLOT_FLAGS_OFFSET
    ));                                                                         // load context-state ownership flags
    emitter.instruction(&format!(
        "test r11, {}", RESOURCE_FLAG_OWNS_STATE
    ));                                                                         // does the registry own this ContextState?
    emitter.instruction("jz __rt_resource_release_recycle");                    // borrowed context states remain externally owned
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [rax + {}]", SLOT_STATE_PTR_OFFSET
    ));                                                                         // load the owned ContextState pointer
    emitter.instruction("test r11, r11");                                       // is a ContextState allocation attached?
    emitter.instruction("jz __rt_resource_release_recycle");                    // absent context state requires no destructor
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], 0",
        SLOT_STATE_PTR_OFFSET
    ));                                                                         // detach state before potentially re-entrant child cleanup
    emitter.instruction("mov rax, r11");                                        // pass ContextState to its typed destructor
    emitter.instruction("call __rt_context_destroy_state");                     // release options, notifier, then ContextState exactly once
    emitter.instruction("jmp __rt_resource_release_after_state_destroy");       // re-resolve after potentially re-entrant child teardown
    emitter.label("__rt_resource_release_generic_state");
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [rax + {}]", SLOT_FLAGS_OFFSET
    ));                                                                         // load state ownership flags
    emitter.instruction(&format!(
        "test r11, {}", RESOURCE_FLAG_OWNS_STATE
    ));                                                                         // does the registry own the stable state storage?
    emitter.instruction("jz __rt_resource_release_recycle");                    // leave externally owned state untouched
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [rax + {}]", SLOT_STATE_PTR_OFFSET
    ));                                                                         // load owned state storage
    emitter.instruction("test rax, rax");                                       // is state storage present?
    emitter.instruction("jz __rt_resource_release_recycle");                    // skip absent state storage
    emitter.instruction("call __rt_heap_free");                                 // release registry-owned state storage
    emitter.label("__rt_resource_release_after_state_destroy");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the generation-safe handle after child teardown
    emitter.instruction("call __rt_resource_lookup_any");                       // re-resolve because teardown may have grown the registry
    emitter.instruction("test rax, rax");                                       // is the retiring slot still valid?
    emitter.instruction("jz __rt_resource_release_done");                       // tolerate defensive invalidation during teardown
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // refresh the slot pointer before recycling it
    emitter.label("__rt_resource_release_recycle");
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the registry slot pointer
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [r9 + {}]", SLOT_GENERATION_OFFSET
    ));                                                                         // load the retiring generation
    emitter.instruction("add r10d, 1");                                         // advance the 32-bit generation before reuse
    emitter.instruction("jnz __rt_resource_release_generation_ready");          // preserve every non-zero wrapped generation
    emitter.instruction("mov r10d, 1");                                         // reserve generation zero
    emitter.label("__rt_resource_release_generation_ready");
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {}], r10",
        SLOT_GENERATION_OFFSET
    ));                                                                         // publish the next slot generation
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {}], 0", SLOT_KIND_OFFSET
    ));                                                                         // mark the slot free
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {}], 0", SLOT_STATUS_OFFSET
    ));                                                                         // clear lifecycle state
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {}], 0", SLOT_REFS_OFFSET
    ));                                                                         // clear strong ownership
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {}], 0", SLOT_PHP_ID_OFFSET
    ));                                                                         // clear the retired PHP id
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {}], 0",
        SLOT_STATE_PTR_OFFSET
    ));                                                                         // clear the retired state pointer
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {}], 0", SLOT_FLAGS_OFFSET
    ));                                                                         // clear retired ownership flags
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_free");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the previous free-list head
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {}], r11",
        SLOT_NEXT_FREE_OFFSET
    ));                                                                         // link the recycled slot to the free list
    emitter.instruction("mov r11d, DWORD PTR [rbp - 8]");                       // recover the one-based low handle word
    emitter.instruction("mov QWORD PTR [r10], r11");                            // publish the recycled slot as free-list head
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_live");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the current live-resource count
    emitter.instruction(&format!(
        "cmp r11, {}", STANDARD_STREAM_COUNT
    ));                                                                         // protect the persistent standard-resource floor
    emitter.instruction("jbe __rt_resource_release_done");                      // avoid underflow after duplicate release
    emitter.instruction("sub r11, 1");                                          // remove the recycled resource from the live count
    emitter.instruction("mov QWORD PTR [r10], r11");                            // publish the reduced live-resource count
    emitter.label("__rt_resource_release_done");
    emitter.instruction("add rsp, 64");                                         // release lifecycle scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return after releasing or ignoring the handle
}

/// Emits x86_64 request shutdown for every non-persistent registry slot.
fn emit_registry_request_reset(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: release request-owned opaque resources ---");
    emitter.label_global("__rt_resource_registry_request_reset");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable request-reset frame
    emitter.instruction("sub rsp, 64");                                         // reserve aligned scan scratch storage
    abi::emit_symbol_address(emitter, "r9", "_stream_context_options");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // clear the borrowed options bridge before request-owned states are destroyed
    abi::emit_symbol_address(emitter, "r9", "_stream_notification_callback");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // clear the borrowed notifier bridge before descriptor teardown
    abi::emit_symbol_address(emitter, "r9", "_stream_current_context_handle");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // clear the borrowed wrapper-context handle before teardown
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // phase zero releases streams before their attached contexts
    emitter.instruction("call __rt_resource_registry_init");                    // make standard persistent slots available
    emitter.instruction("test rax, rax");                                       // did lazy registry initialization succeed?
    emitter.instruction("jz __rt_resource_registry_request_reset_done");        // tolerate registry allocation failure

    emitter.label("__rt_resource_registry_request_reset_restart");
    emitter.instruction(&format!(
        "mov r9, {}", STANDARD_STREAM_COUNT
    ));                                                                         // begin after STDIN, STDOUT, and STDERR
    emitter.instruction("mov QWORD PTR [rbp - 8], r9");                         // preserve the scan index across release callbacks
    emitter.label("__rt_resource_registry_request_reset_scan");
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_len");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // reload length because teardown callbacks may allocate
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the current scan index
    emitter.instruction("cmp r9, r10");                                         // have all initialized slots been inspected?
    emitter.instruction("jae __rt_resource_registry_request_reset_advance");    // finish when no request-owned slot remains
    abi::emit_symbol_address(emitter, "r11", "_resource_registry_ptr");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // reload storage because callbacks may grow the registry
    emitter.instruction("mov rcx, r9");                                         // copy the zero-based slot index
    emitter.instruction("shl rcx, 6");                                          // convert the slot index to a byte offset
    emitter.instruction("add r11, rcx");                                        // address the current registry slot
    emitter.instruction(&format!(
        "cmp QWORD PTR [r11 + {}], 0", SLOT_KIND_OFFSET
    ));                                                                         // is the current slot free?
    emitter.instruction("je __rt_resource_registry_request_reset_next");        // skip free slots
    emitter.instruction("cmp QWORD PTR [rbp - 16], 0");                         // is this the stream-first reset phase?
    emitter.instruction("jne __rt_resource_registry_request_reset_kind_ready"); // phase one releases every remaining resource kind
    emitter.instruction(&format!(
        "cmp QWORD PTR [r11 + {}], {}", SLOT_KIND_OFFSET, RESOURCE_KIND_STREAM
    ));                                                                         // is this a stream during the stream-first phase?
    emitter.instruction("jne __rt_resource_registry_request_reset_next");       // preserve contexts until all streams are destroyed
    emitter.label("__rt_resource_registry_request_reset_kind_ready");
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [r11 + {}]", SLOT_FLAGS_OFFSET
    ));                                                                         // load resource persistence flags
    emitter.instruction(&format!(
        "test r10, {}", RESOURCE_FLAG_PERSISTENT
    ));                                                                         // are process-lifetime resources excluded from request cleanup?
    emitter.instruction("jnz __rt_resource_registry_request_reset_next");       // preserve persistent resources
    emitter.instruction(&format!(
        "cmp QWORD PTR [r11 + {}], {}",
        SLOT_STATUS_OFFSET, RESOURCE_STATUS_CLOSING
    ));                                                                         // is a re-entrant close still on the stack?
    emitter.instruction("je __rt_resource_registry_request_reset_next");        // never recycle a resource during its active destructor
    emitter.instruction(&format!(
        "mov QWORD PTR [r11 + {}], 1", SLOT_REFS_OFFSET
    ));                                                                         // collapse abandoned aliases to one shutdown owner
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [r11 + {}]",
        SLOT_GENERATION_OFFSET
    ));                                                                         // load the live generation
    emitter.instruction(&format!(
        "shl r10, {}", HANDLE_INDEX_BITS
    ));                                                                         // place the generation in the opaque handle high word
    emitter.instruction("add r9, 1");                                           // convert the index to its one-based handle word
    emitter.instruction("or r10, r9");                                          // reconstruct the exact live opaque handle
    emitter.instruction("mov rdi, r10");                                        // pass the handle to the uniform lifecycle release path
    emitter.instruction("call __rt_resource_release");                          // close, destroy state, invalidate, and recycle the slot
    emitter.instruction("jmp __rt_resource_registry_request_reset_restart");    // restart after callbacks may move storage or allocate resources

    emitter.label("__rt_resource_registry_request_reset_next");
    emitter.instruction("add r9, 1");                                           // advance to the next initialized slot
    emitter.instruction("mov QWORD PTR [rbp - 8], r9");                         // publish the next scan index
    emitter.instruction("jmp __rt_resource_registry_request_reset_scan");       // continue searching for request-owned resources

    emitter.label("__rt_resource_registry_request_reset_advance");
    emitter.instruction("cmp QWORD PTR [rbp - 16], 0");                         // did the stream-first phase just complete?
    emitter.instruction("jne __rt_resource_registry_request_reset_epoch");      // phase one completed all remaining resources
    emitter.instruction("mov QWORD PTR [rbp - 16], 1");                         // advance to context and remaining-resource teardown
    abi::emit_symbol_address(emitter, "r9", "_stream_default_context_handle");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // detach the default owner only after attached streams released it
    emitter.instruction("jmp __rt_resource_registry_request_reset_restart");    // rescan from the beginning for contexts and other resources
    emitter.label("__rt_resource_registry_request_reset_epoch");
    abi::emit_symbol_address(emitter, "r9", "_resource_registry_epoch");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the completed request epoch
    emitter.instruction("add r10, 1");                                          // advance allocation ownership to the next request
    emitter.instruction("jnz __rt_resource_registry_request_reset_epoch_ready"); // keep every non-zero wrapped epoch
    emitter.instruction("mov r10, 1");                                          // reserve epoch zero for process-persistent resources
    emitter.label("__rt_resource_registry_request_reset_epoch_ready");
    emitter.instruction("mov QWORD PTR [r9], r10");                             // publish the next request epoch
    emitter.label("__rt_resource_registry_request_reset_done");
    emitter.instruction("add rsp, 64");                                         // release request-reset scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return after deterministic request shutdown
}

/// Emits process-exit teardown: releases the slot array `__rt_resource_registry_init`
/// allocated and resets the globals so a later init starts from a clean registry.
///
/// This is the counterpart of init, NOT of the request reset. A `--web` worker reuses
/// one slot array across requests and must only run the request reset; the CLI epilogue
/// runs this afterwards so the array does not show up as a leaked block under
/// `--heap-debug` in every program, stream-using or not.
///
/// THE POINTER GOES IN `rax`. `__rt_heap_free` reads its operand from the x86_64
/// integer RESULT register (`heap_free.rs` opens with `test rax, rax`), not from the
/// SysV first-argument register. Passing it in `rdi` handed the free path whatever
/// `rax` happened to hold, which walked the heap free list off a garbage pointer and
/// segfaulted EVERY CLI program on linux-x86_64 — after it had already printed its
/// output, since the CLI epilogue runs this last. macOS and Linux AArch64 were
/// unaffected because their operand register and their first-argument register are the
/// same `x0`, which is exactly why a local suite could not see it.
fn emit_registry_teardown(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: release the opaque resource registry at process exit ---");
    emitter.label_global("__rt_resource_registry_teardown");
    emitter.instruction("sub rsp, 8");                                          // realign the stack for the heap-free call
    abi::emit_symbol_address(emitter, "r9", "_resource_registry_ptr");
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // load the dynamic slot-array pointer into heap_free's operand register
    emitter.instruction("test rax, rax");                                       // has the registry ever been initialized?
    emitter.instruction("jz __rt_resource_registry_teardown_done");             // an uninitialized registry owns no storage
    emitter.instruction("mov QWORD PTR [r9], 0");                               // clear the pointer before releasing it
    abi::emit_symbol_address(emitter, "r10", "_resource_registry_static_slots");
    emitter.instruction("cmp rax, r10");                                        // is the registry still on its STATIC reservation?
    emitter.instruction("je __rt_resource_registry_teardown_reset_x86");        // static storage is not heap storage: only the globals reset
    emitter.instruction("call __rt_heap_free");                                 // release a grown, heap-allocated slot array
    emitter.label("__rt_resource_registry_teardown_reset_x86");
    abi::emit_symbol_address(emitter, "r9", "_resource_registry_len");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // no slots remain initialized
    abi::emit_symbol_address(emitter, "r9", "_resource_registry_cap");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // no capacity remains
    abi::emit_symbol_address(emitter, "r9", "_resource_registry_free");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // drop the free-list head with its storage
    abi::emit_symbol_address(emitter, "r9", "_resource_registry_live");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // no live resources remain
    emitter.label("__rt_resource_registry_teardown_done");
    emitter.instruction("add rsp, 8");                                          // release the alignment padding
    emitter.instruction("ret");
}

/// Emits the x86_64 Live-to-Closing lifecycle transition.
fn emit_resource_mark_closing(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mark an opaque resource as closing ---");
    emitter.label_global("__rt_resource_mark_closing");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable transition frame
    emitter.instruction("call __rt_resource_lookup_any");                       // validate and resolve the resource slot
    emitter.instruction("test rax, rax");                                       // did lookup resolve a slot?
    emitter.instruction("jz __rt_resource_mark_closing_fail");                  // reject stale handles
    emitter.instruction(&format!(
        "cmp QWORD PTR [rax + {}], {}",
        SLOT_STATUS_OFFSET, RESOURCE_STATUS_LIVE
    ));                                                                         // is the resource currently live?
    emitter.instruction("jne __rt_resource_mark_closing_fail");                 // only Live may transition to Closing
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], {}",
        SLOT_STATUS_OFFSET, RESOURCE_STATUS_CLOSING
    ));                                                                         // publish Closing before callbacks
    emitter.instruction("mov eax, 1");                                          // report a successful transition
    emitter.instruction("jmp __rt_resource_mark_closing_done");                 // join the helper epilogue
    emitter.label("__rt_resource_mark_closing_fail");
    emitter.instruction("xor eax, eax");                                        // report invalid or already-transitioned resources
    emitter.label("__rt_resource_mark_closing_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the transition status
}

/// Emits the x86_64 Live/Closing-to-Closed lifecycle transition.
fn emit_resource_mark_closed(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mark an opaque resource as closed ---");
    emitter.label_global("__rt_resource_mark_closed");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable transition frame
    emitter.instruction("sub rsp, 16");                                         // reserve aligned storage for the opaque handle
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the exact generation-safe handle
    emitter.instruction("call __rt_resource_lookup_any");                       // validate and resolve the resource slot
    emitter.instruction("test rax, rax");                                       // did lookup resolve a slot?
    emitter.instruction("jz __rt_resource_mark_closed_fail");                   // reject stale handles
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {}]", SLOT_STATUS_OFFSET
    ));                                                                         // load the lifecycle state
    emitter.instruction(&format!(
        "cmp r10, {}", RESOURCE_STATUS_CLOSED
    ));                                                                         // is the resource already closed?
    emitter.instruction("je __rt_resource_mark_closed_fail");                   // preserve exactly-once close reporting
    emitter.instruction(&format!(
        "cmp r10, {}", RESOURCE_STATUS_LIVE
    ));                                                                         // can Live transition directly to Closed?
    emitter.instruction("je __rt_resource_mark_closed_store");                  // accept close paths without callbacks
    emitter.instruction(&format!(
        "cmp r10, {}", RESOURCE_STATUS_CLOSING
    ));                                                                         // did cleanup publish Closing first?
    emitter.instruction("jne __rt_resource_mark_closed_fail");                  // reject every other lifecycle state
    emitter.label("__rt_resource_mark_closed_store");
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], {}",
        SLOT_STATUS_OFFSET, RESOURCE_STATUS_CLOSED
    ));                                                                         // publish the terminal Closed state
    emitter.instruction(&format!(
        "cmp QWORD PTR [rax + {}], 0", SLOT_REFS_OFFSET
    ));                                                                         // was final release deferred during Closing?
    emitter.instruction("jne __rt_resource_mark_closed_success");               // remaining owners keep the Closed slot addressable
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {}], 1", SLOT_REFS_OFFSET
    ));                                                                         // restore one synthetic owner for common release
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the original opaque handle
    emitter.instruction("call __rt_resource_release");                          // destroy owned state and recycle the deferred slot
    emitter.label("__rt_resource_mark_closed_success");
    emitter.instruction("mov eax, 1");                                          // report a successful transition
    emitter.instruction("jmp __rt_resource_mark_closed_done");                  // join the helper epilogue
    emitter.label("__rt_resource_mark_closed_fail");
    emitter.instruction("xor eax, eax");                                        // report invalid or already-closed resources
    emitter.label("__rt_resource_mark_closed_done");
    emitter.instruction("add rsp, 16");                                         // release transition scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the transition status
}

/// Emits registry-backed PHP resource-id lookup on x86_64.
fn emit_resource_id_of_registry(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: read the PHP id of an opaque resource ---");
    emitter.label_global("__rt_resource_id_of_registry");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable lookup frame
    emitter.instruction("call __rt_resource_lookup_any");                       // validate and resolve the opaque handle
    emitter.instruction("test rax, rax");                                       // did lookup resolve a slot?
    emitter.instruction("jz __rt_resource_id_of_registry_done");                // invalid resources report id zero
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [rax + {}]", SLOT_PHP_ID_OFFSET
    ));                                                                         // return the PHP-visible resource id
    emitter.label("__rt_resource_id_of_registry_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the resource id or zero
}

/// Emits generation-aware testing for a resource whose lifecycle status is exactly Live.
fn emit_resource_is_open(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: test whether an opaque resource is open ---");
    emitter.label_global("__rt_resource_is_open");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable open-test frame
    emitter.instruction("call __rt_resource_lookup_any");                       // validate and resolve the opaque handle
    emitter.instruction("test rax, rax");                                       // did lookup resolve a registry slot?
    emitter.instruction("jz __rt_resource_is_open_false");                      // invalid or stale handles are not open
    emitter.instruction(&format!(
        "cmp QWORD PTR [rax + {}], {}",
        SLOT_STATUS_OFFSET, RESOURCE_STATUS_LIVE
    ));                                                                         // is the resource exactly Live?
    emitter.instruction("sete al");                                             // materialize the open-resource predicate byte
    emitter.instruction("movzx eax, al");                                       // return a normalized zero-or-one integer
    emitter.instruction("jmp __rt_resource_is_open_done");                      // join the helper epilogue
    emitter.label("__rt_resource_is_open_false");
    emitter.instruction("xor eax, eax");                                        // report false for invalid handles
    emitter.label("__rt_resource_is_open_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return one for Live, otherwise zero
}

/// Emits x86_64 live-resource kind lookup with zero for invalid or non-live handles.
fn emit_resource_kind_if_open(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve the kind of a live opaque resource ---");
    emitter.label_global("__rt_resource_kind_if_open");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable lookup frame
    emitter.instruction("call __rt_resource_lookup_any");                       // validate the opaque handle generation and slot
    emitter.instruction("test rax, rax");                                       // did lookup resolve a registry entry?
    emitter.instruction("jz __rt_resource_kind_if_open_false");                 // invalid or stale handles have no live resource kind
    emitter.instruction(&format!(
        "cmp QWORD PTR [rax + {}], {}",
        SLOT_STATUS_OFFSET, RESOURCE_STATUS_LIVE
    ));                                                                         // only Live entries remain PHP resources
    emitter.instruction("jne __rt_resource_kind_if_open_false");                // Closing and Closed entries report no kind
    emitter.instruction(&format!(
        "mov rax, QWORD PTR [rax + {}]", SLOT_KIND_OFFSET
    ));                                                                         // return the live registry resource kind
    emitter.instruction("jmp __rt_resource_kind_if_open_done");                 // join the helper epilogue
    emitter.label("__rt_resource_kind_if_open_false");
    emitter.instruction("xor eax, eax");                                        // return kind zero for invalid or non-live resources
    emitter.label("__rt_resource_kind_if_open_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the live resource kind or zero
}
