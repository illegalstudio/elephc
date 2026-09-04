//! Purpose:
//! Emits the AArch64 implementation of the dynamic opaque resource registry.
//! It is shared by macOS AArch64 and Linux AArch64.
//!
//! Called from:
//! - `crate::codegen_support::runtime::resources::registry::emit_resource_registry()`.
//!
//! Key details:
//! - Heap-backed slot growth preserves slot indices while invalidating no live handle.
//! - Lookup is a leaf helper; lifecycle helpers preserve `x30` around nested calls.

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

/// Emits every AArch64 resource-registry entry point.
pub(super) fn emit_resource_registry_aarch64(emitter: &mut Emitter) {
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

/// Emits lazy registry initialization, including the three persistent standard streams.
fn emit_registry_init(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: initialize the opaque resource registry ---");
    emitter.label_global("__rt_resource_registry_init");
    emitter.instruction("sub sp, sp, #64");                                     // reserve initialization scratch and a saved frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and link register
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_ptr");
    emitter.instruction("ldr x10, [x9]");                                       // load the current dynamic slot-array pointer
    emitter.instruction("cbnz x10, __rt_resource_registry_init_ready");         // reuse an already initialized registry

    // The initial slot array is the STATIC reservation, not a heap block: a program
    // must not need runtime heap before its first statement (see `data::fixed`).
    abi::emit_symbol_address(emitter, "x0", "_resource_registry_static_slots");
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the static slot-array base
    emitter.instruction("mov x10, x0");                                         // start zeroing at the first slot byte
    emitter.instruction(&format!(
        "mov x11, #{}",
        INITIAL_REGISTRY_CAPACITY * RESOURCE_SLOT_SIZE / 8
    ));                                                                         // count zeroed machine words
    emitter.label("__rt_resource_registry_init_zero");
    emitter.instruction("str xzr, [x10], #8");                                  // clear one slot-array word
    emitter.instruction("subs x11, x11, #1");                                   // consume one word from the zeroing count
    emitter.instruction("b.ne __rt_resource_registry_init_zero");               // clear the complete initial allocation

    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the initialized slot-array pointer
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_ptr");
    emitter.instruction("str x10, [x9]");                                       // publish the dynamic registry pointer
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_len");
    emitter.instruction(&format!(
        "mov x11, #{}", STANDARD_STREAM_COUNT
    ));                                                                         // reserve slots zero through two for standard streams
    emitter.instruction("str x11, [x9]");                                       // publish the initialized slot count
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_cap");
    emitter.instruction(&format!(
        "mov x11, #{}", INITIAL_REGISTRY_CAPACITY
    ));                                                                         // materialize the initial slot capacity
    emitter.instruction("str x11, [x9]");                                       // publish the initial slot capacity
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_free");
    emitter.instruction("str xzr, [x9]");                                       // initialize the one-based free-list head as empty
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_live");
    emitter.instruction(&format!(
        "mov x11, #{}", STANDARD_STREAM_COUNT
    ));                                                                         // count the persistent standard streams as live
    emitter.instruction("str x11, [x9]");                                       // publish the initial live-resource count
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_epoch");
    emitter.instruction("ldr x11, [x9]");                                       // preserve a previously advanced request epoch
    emitter.instruction("cbnz x11, __rt_resource_registry_init_epoch_ready");   // avoid resetting a non-zero request epoch
    emitter.instruction("mov x11, #1");                                         // request epochs start at one
    emitter.instruction("str x11, [x9]");                                       // publish the first request epoch
    emitter.label("__rt_resource_registry_init_epoch_ready");

    abi::emit_symbol_address(emitter, "x12", "_resource_std_stream_states");
    emitter.instruction("str x12, [sp, #8]");                                   // preserve the static standard-stream state base
    emitter.instruction("mov x13, #0");                                         // initialize standard-stream slot index zero
    emitter.label("__rt_resource_registry_init_std_loop");
    emitter.instruction("lsl x14, x13, #6");                                    // convert the slot index to a 64-byte offset
    emitter.instruction("add x15, x10, x14");                                   // address the standard-stream registry slot
    emitter.instruction("mov x16, #1");                                         // standard opaque handles begin at generation one
    emitter.instruction("str x16, [x15, #0]");                                  // keep raw 0/1/2 distinct during migration
    emitter.instruction(&format!(
        "mov x16, #{}", RESOURCE_KIND_STREAM
    ));                                                                         // select the stream resource kind
    emitter.instruction("str x16, [x15, #8]");                                  // store the standard-stream kind
    emitter.instruction(&format!(
        "mov x16, #{}", RESOURCE_STATUS_LIVE
    ));                                                                         // select the live lifecycle state
    emitter.instruction("str x16, [x15, #16]");                                 // mark the standard stream live
    emitter.instruction(&format!(
        "mov x16, #{}", RESOURCE_REFS_IMMORTAL
    ));                                                                         // materialize the immortal-reference sentinel
    emitter.instruction("str x16, [x15, #24]");                                 // prevent standard-stream release
    emitter.instruction("add x16, x13, #1");                                    // PHP ids for standard streams are one through three
    emitter.instruction("str x16, [x15, #32]");                                 // store the PHP-visible resource id
    emitter.instruction(&format!(
        "mov x17, #{}", STREAM_STATE_SIZE
    ));                                                                         // materialize the stream-state stride
    emitter.instruction("mul x17, x13, x17");                                   // compute the standard stream-state byte offset
    emitter.instruction("ldr x12, [sp, #8]");                                   // reload the static standard-stream state base
    emitter.instruction("add x17, x12, x17");                                   // address this standard stream's stable state
    emitter.instruction("str x17, [x15, #40]");                                 // bind the registry slot to its stream state
    emitter.instruction("str xzr, [x15, #48]");                                 // standard slots never participate in the free list
    emitter.instruction(&format!(
        "mov x16, #{}", RESOURCE_FLAG_PERSISTENT
    ));                                                                         // select persistent registry ownership
    emitter.instruction("str x16, [x15, #56]");                                 // mark the standard slot process-persistent
    emitter.instruction(&format!(
        "mov x16, #{}", STREAM_BACKEND_FD
    ));                                                                         // select the direct-descriptor backend
    emitter.instruction("str x16, [x17, #0]");                                  // record the standard stream backend kind
    emitter.instruction("mov x16, #6");                                         // standard streams use PHP's php:// wrapper
    emitter.instruction(&format!(
        "str x16, [x17, #{}]", STREAM_WRAPPER_ID_OFFSET
    ));                                                                         // publish the PHP wrapper metadata
    emitter.instruction("str x13, [x17, #16]");                                 // store descriptor zero, one, or two
    abi::emit_symbol_address(emitter, "x9", "_resource_std_stream_uri_ptrs");
    emitter.instruction("ldr x16, [x9, x13, lsl #3]");                          // load this standard stream's static URI pointer
    emitter.instruction(&format!(
        "str x16, [x17, #{}]", STREAM_URI_PTR_OFFSET
    ));                                                                         // publish the standard stream URI pointer
    abi::emit_symbol_address(emitter, "x9", "_resource_std_stream_uri_lens");
    emitter.instruction("ldr x16, [x9, x13, lsl #3]");                          // load this standard stream's static URI length
    emitter.instruction(&format!(
        "str x16, [x17, #{}]", STREAM_URI_LEN_OFFSET
    ));                                                                         // publish the standard stream URI length
    emitter.instruction(&format!(
        "mov x16, #{}", RESOURCE_FLAG_PERSISTENT
    ));                                                                         // mark the stream state as persistent
    emitter.instruction(&format!(
        "str x16, [x17, #{}]", STREAM_OWNERSHIP_FLAGS_OFFSET
    ));                                                                         // publish persistent stream ownership
    emitter.instruction("add x13, x13, #1");                                    // advance to the next standard stream
    emitter.instruction(&format!(
        "cmp x13, #{}", STANDARD_STREAM_COUNT
    ));                                                                         // have all standard streams been installed?
    emitter.instruction("b.lo __rt_resource_registry_init_std_loop");           // initialize the remaining standard slots

    emitter.label("__rt_resource_registry_init_ready");
    emitter.instruction("mov x0, #1");                                          // report successful initialization
    emitter.instruction("b __rt_resource_registry_init_done");                  // join the common initialization epilogue
    emitter.label("__rt_resource_registry_init_fail");
    emitter.instruction("mov x0, #0");                                          // report allocation failure without registry publication
    emitter.label("__rt_resource_registry_init_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #64");                                     // release initialization scratch storage
    emitter.instruction("ret");                                                 // return to the registry caller
}

/// Emits dynamic slot-array growth while preserving every live slot index.
fn emit_registry_grow(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: grow the opaque resource registry ---");
    emitter.label_global("__rt_resource_registry_grow");
    emitter.instruction("sub sp, sp, #96");                                     // reserve growth state and a saved frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the caller frame and link register
    emitter.instruction("add x29, sp, #80");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the requested minimum capacity
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_ptr");
    emitter.instruction("ldr x10, [x9]");                                       // load the old slot-array pointer
    emitter.instruction("str x10, [sp, #8]");                                   // preserve the old slot-array pointer
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_cap");
    emitter.instruction("ldr x11, [x9]");                                       // load the old slot capacity
    emitter.instruction("str x11, [sp, #16]");                                  // preserve the old slot capacity
    emitter.label("__rt_resource_registry_grow_capacity");
    emitter.instruction("ldr x12, [sp, #0]");                                   // reload the required minimum capacity
    emitter.instruction("cmp x11, x12");                                        // does the proposed capacity satisfy the request?
    emitter.instruction("b.hs __rt_resource_registry_grow_alloc");              // allocate once the capacity is large enough
    emitter.instruction("lsl x11, x11, #1");                                    // double the dynamic slot capacity
    emitter.instruction("cbz x11, __rt_resource_registry_grow_fail");           // reject integer wrap instead of publishing a zero capacity
    emitter.instruction("b __rt_resource_registry_grow_capacity");              // continue growing to the requested minimum
    emitter.label("__rt_resource_registry_grow_alloc");
    emitter.instruction("str x11, [sp, #24]");                                  // preserve the chosen new capacity
    emitter.instruction("lsl x0, x11, #6");                                     // convert the slot capacity to bytes
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the replacement slot array
    emitter.instruction("cbz x0, __rt_resource_registry_grow_fail");            // preserve the old registry if allocation failed
    emitter.instruction("str x0, [sp, #32]");                                   // preserve the replacement slot-array pointer
    emitter.instruction("mov x10, x0");                                         // start zeroing at the replacement allocation
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the replacement capacity
    emitter.instruction("lsl x11, x11, #3");                                    // count 64-byte slots as eight-byte words
    emitter.label("__rt_resource_registry_grow_zero");
    emitter.instruction("str xzr, [x10], #8");                                  // clear one replacement-array word
    emitter.instruction("subs x11, x11, #1");                                   // consume one zeroed word
    emitter.instruction("b.ne __rt_resource_registry_grow_zero");               // clear the complete replacement array

    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the old slot-array pointer
    emitter.instruction("ldr x12, [sp, #32]");                                  // reload the replacement slot-array pointer
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_len");
    emitter.instruction("ldr x11, [x9]");                                       // load the number of initialized slots
    emitter.instruction("lsl x11, x11, #3");                                    // convert 64-byte slots to eight-byte words
    emitter.label("__rt_resource_registry_grow_copy");
    emitter.instruction("cbz x11, __rt_resource_registry_grow_publish");        // publish after copying every initialized slot
    emitter.instruction("ldr x13, [x10], #8");                                  // load one old registry word
    emitter.instruction("str x13, [x12], #8");                                  // copy the registry word into the replacement
    emitter.instruction("sub x11, x11, #1");                                    // consume one copied word
    emitter.instruction("b __rt_resource_registry_grow_copy");                  // copy the remaining initialized slot words
    emitter.label("__rt_resource_registry_grow_publish");
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_ptr");
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the replacement slot-array pointer
    emitter.instruction("str x10, [x9]");                                       // atomically publish the replacement registry base
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_cap");
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the replacement capacity
    emitter.instruction("str x11, [x9]");                                       // publish the replacement capacity
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the superseded slot array
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_static_slots");
    emitter.instruction("cmp x0, x9");                                          // was the superseded array the STATIC reservation?
    emitter.instruction("b.eq __rt_resource_registry_grow_static");             // the static base is not heap storage and must never be freed
    emitter.instruction("bl __rt_heap_free");                                   // release the old dynamic slot array
    emitter.label("__rt_resource_registry_grow_static");
    emitter.instruction("mov x0, #1");                                          // report successful growth
    emitter.instruction("b __rt_resource_registry_grow_done");                  // join the helper epilogue
    emitter.label("__rt_resource_registry_grow_fail");
    emitter.instruction("mov x0, #0");                                          // report growth failure without modifying registry globals
    emitter.label("__rt_resource_registry_grow_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #96");                                     // release growth scratch storage
    emitter.instruction("ret");                                                 // return the growth status
}

/// Emits allocation of a live resource slot and its opaque generation/index handle.
fn emit_resource_alloc(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: allocate an opaque resource handle ---");
    emitter.label_global("__rt_resource_alloc");
    emitter.instruction("sub sp, sp, #96");                                     // reserve allocation state and a saved frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the caller frame and link register
    emitter.instruction("add x29, sp, #80");                                    // establish the helper frame pointer
    emitter.instruction("stp x0, x1, [sp, #0]");                                // preserve resource kind and stable state pointer
    emitter.instruction("str x2, [sp, #16]");                                   // preserve resource ownership flags
    emitter.instruction("cbz x0, __rt_resource_alloc_fail");                    // resource kind zero is reserved for free slots
    emitter.instruction("bl __rt_resource_registry_init");                      // lazily install registry storage and standard streams
    emitter.instruction("cbz x0, __rt_resource_alloc_fail");                    // propagate registry initialization failure

    abi::emit_symbol_address(emitter, "x9", "_resource_registry_free");
    emitter.instruction("ldr x10, [x9]");                                       // load the one-based free-list head
    emitter.instruction("cbz x10, __rt_resource_alloc_append");                 // append a slot when the free list is empty
    emitter.instruction("sub x11, x10, #1");                                    // convert the free-list head to a zero-based slot index
    emitter.instruction("str x11, [sp, #24]");                                  // preserve the selected slot index
    abi::emit_symbol_address(emitter, "x12", "_resource_registry_ptr");
    emitter.instruction("ldr x12, [x12]");                                      // load the dynamic slot-array pointer
    emitter.instruction("add x13, x12, x11, lsl #6");                           // address the selected free slot
    emitter.instruction(&format!(
        "ldr x14, [x13, #{}]", SLOT_NEXT_FREE_OFFSET
    ));                                                                         // load the next one-based free-list link
    emitter.instruction("str x14, [x9]");                                       // pop the selected slot from the free list
    emitter.instruction("b __rt_resource_alloc_fill");                          // initialize the reused slot

    emitter.label("__rt_resource_alloc_append");
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_len");
    emitter.instruction("ldr x11, [x9]");                                       // load the next never-initialized slot index
    emitter.instruction("str x11, [sp, #24]");                                  // preserve the append slot index
    abi::emit_symbol_address(emitter, "x10", "_resource_registry_cap");
    emitter.instruction("ldr x12, [x10]");                                      // load the current dynamic slot capacity
    emitter.instruction("cmp x11, x12");                                        // is the slot array already full?
    emitter.instruction("b.lo __rt_resource_alloc_append_ready");               // append directly when capacity remains
    emitter.instruction("add x0, x11, #1");                                     // request room for the new one-based slot
    emitter.instruction("bl __rt_resource_registry_grow");                      // grow without changing any existing slot index
    emitter.instruction("cbz x0, __rt_resource_alloc_fail");                    // report allocation failure if growth failed
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_len");
    emitter.instruction("ldr x11, [sp, #24]");                                  // restore the append slot index
    emitter.label("__rt_resource_alloc_append_ready");
    emitter.instruction("add x12, x11, #1");                                    // advance the initialized slot count
    emitter.instruction("str x12, [x9]");                                       // publish the appended slot
    abi::emit_symbol_address(emitter, "x12", "_resource_registry_ptr");
    emitter.instruction("ldr x12, [x12]");                                      // reload the possibly grown slot-array pointer
    emitter.instruction("add x13, x12, x11, lsl #6");                           // address the appended registry slot

    emitter.label("__rt_resource_alloc_fill");
    emitter.instruction(&format!(
        "ldr x14, [x13, #{}]", SLOT_GENERATION_OFFSET
    ));                                                                         // load the slot generation
    emitter.instruction("cbnz x14, __rt_resource_alloc_generation_ready");      // preserve a generation advanced by release
    emitter.instruction("mov x14, #1");                                         // first-use generations start at one
    emitter.label("__rt_resource_alloc_generation_ready");
    emitter.instruction(&format!(
        "str x14, [x13, #{}]", SLOT_GENERATION_OFFSET
    ));                                                                         // publish the live slot generation
    emitter.instruction("ldr x15, [sp, #0]");                                   // reload the requested resource kind
    emitter.instruction(&format!(
        "str x15, [x13, #{}]", SLOT_KIND_OFFSET
    ));                                                                         // publish the resource kind
    emitter.instruction(&format!(
        "mov x15, #{}", RESOURCE_STATUS_LIVE
    ));                                                                         // select the live lifecycle state
    emitter.instruction(&format!(
        "str x15, [x13, #{}]", SLOT_STATUS_OFFSET
    ));                                                                         // publish the live state
    emitter.instruction("mov x15, #1");                                         // allocation returns one owned reference
    emitter.instruction(&format!(
        "str x15, [x13, #{}]", SLOT_REFS_OFFSET
    ));                                                                         // initialize strong ownership
    abi::emit_symbol_address(emitter, "x9", "_resource_id_next");
    emitter.instruction("ldr x15, [x9]");                                       // mint the next PHP-visible resource id
    emitter.instruction(&format!(
        "str x15, [x13, #{}]", SLOT_PHP_ID_OFFSET
    ));                                                                         // bind the PHP id to this incarnation
    emitter.instruction("add x15, x15, #1");                                    // advance the never-reused PHP id cursor
    emitter.instruction("str x15, [x9]");                                       // publish the next resource id
    emitter.instruction("ldr x15, [sp, #8]");                                   // reload the stable state pointer
    emitter.instruction(&format!(
        "str x15, [x13, #{}]", SLOT_STATE_PTR_OFFSET
    ));                                                                         // bind the resource state
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_epoch");
    emitter.instruction("ldr x15, [x9]");                                       // load the request epoch owning this allocation
    emitter.instruction(&format!(
        "str x15, [x13, #{}]", SLOT_REQUEST_EPOCH_OFFSET
    ));                                                                         // replace stale free-list linkage with request ownership
    emitter.instruction("ldr x15, [sp, #16]");                                  // reload resource ownership flags
    emitter.instruction(&format!(
        "str x15, [x13, #{}]", SLOT_FLAGS_OFFSET
    ));                                                                         // publish resource ownership flags
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_live");
    emitter.instruction("ldr x15, [x9]");                                       // load the live-resource count
    emitter.instruction("add x15, x15, #1");                                    // include the allocated resource
    emitter.instruction("str x15, [x9]");                                       // publish the live-resource count
    emitter.instruction(&format!(
        "lsl x14, x14, #{}", HANDLE_INDEX_BITS
    ));                                                                         // place the generation in the high handle word
    emitter.instruction("ldr x15, [sp, #24]");                                  // reload the zero-based slot index
    emitter.instruction("add x15, x15, #1");                                    // encode a non-zero one-based slot
    emitter.instruction("orr x0, x14, x15");                                    // return generation plus slot as the opaque handle
    emitter.instruction("b __rt_resource_alloc_done");                          // join the allocation epilogue
    emitter.label("__rt_resource_alloc_fail");
    emitter.instruction("mov x0, #0");                                          // return the invalid handle on allocation failure
    emitter.label("__rt_resource_alloc_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #96");                                     // release allocation scratch storage
    emitter.instruction("ret");                                                 // return the opaque handle or zero
}

/// Emits leaf validation of an opaque handle and returns its registry slot.
fn emit_resource_lookup_any(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: validate and resolve an opaque resource handle ---");
    emitter.label_global("__rt_resource_lookup_any");
    emitter.instruction("cbz x0, __rt_resource_lookup_any_fail");               // handle zero is always invalid
    emitter.instruction("ubfx x9, x0, #0, #32");                                // extract the one-based slot component
    emitter.instruction("cbz x9, __rt_resource_lookup_any_fail");               // reject a zero one-based slot
    emitter.instruction("sub x9, x9, #1");                                      // convert the handle to a zero-based slot index
    abi::emit_symbol_address(emitter, "x10", "_resource_registry_len");
    emitter.instruction("ldr x10, [x10]");                                      // load the initialized slot count
    emitter.instruction("cmp x9, x10");                                         // is the requested slot initialized?
    emitter.instruction("b.hs __rt_resource_lookup_any_fail");                  // reject out-of-range slot indices
    abi::emit_symbol_address(emitter, "x10", "_resource_registry_ptr");
    emitter.instruction("ldr x10, [x10]");                                      // load the dynamic slot-array pointer
    emitter.instruction("cbz x10, __rt_resource_lookup_any_fail");              // reject lookup before initialization
    emitter.instruction("add x10, x10, x9, lsl #6");                            // address the selected 64-byte slot
    emitter.instruction(&format!(
        "lsr x11, x0, #{}", HANDLE_INDEX_BITS
    ));                                                                         // extract the handle generation
    emitter.instruction(&format!(
        "ldr x12, [x10, #{}]", SLOT_GENERATION_OFFSET
    ));                                                                         // load the slot generation
    emitter.instruction("cmp w11, w12");                                        // compare the generation low words
    emitter.instruction("b.ne __rt_resource_lookup_any_fail");                  // reject stale or recycled handles
    emitter.instruction(&format!(
        "ldr x12, [x10, #{}]", SLOT_KIND_OFFSET
    ));                                                                         // load the current resource kind
    emitter.instruction("cbz x12, __rt_resource_lookup_any_fail");              // reject free slots
    emitter.instruction("mov x0, x10");                                         // return the validated registry-slot pointer
    emitter.instruction("ret");                                                 // finish the successful leaf lookup
    emitter.label("__rt_resource_lookup_any_fail");
    emitter.instruction("mov x0, #0");                                          // return null for invalid or stale handles
    emitter.instruction("ret");                                                 // finish the failed leaf lookup
}

/// Emits strong-reference acquisition while preserving the original handle result.
fn emit_resource_retain(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: retain an opaque resource handle ---");
    emitter.label_global("__rt_resource_retain");
    emitter.instruction("sub sp, sp, #32");                                     // reserve the original handle and a saved frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the caller frame and link register
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the exact incoming opaque handle
    emitter.instruction("bl __rt_resource_lookup_any");                         // validate the handle generation and slot
    emitter.instruction("cbz x0, __rt_resource_retain_done");                   // invalid handles acquire no ownership
    emitter.instruction(&format!(
        "ldr x9, [x0, #{}]", SLOT_REFS_OFFSET
    ));                                                                         // load the strong-reference count
    emitter.instruction("cmn x9, #1");                                          // is this the immortal-reference sentinel?
    emitter.instruction("b.eq __rt_resource_retain_done");                      // persistent resources never change reference count
    emitter.instruction("add x9, x9, #1");                                      // acquire one resource reference
    emitter.instruction(&format!(
        "str x9, [x0, #{}]", SLOT_REFS_OFFSET
    ));                                                                         // publish the acquired reference
    emitter.label("__rt_resource_retain_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the exact original handle for Acquire lowering
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #32");                                     // release retain scratch storage
    emitter.instruction("ret");                                                 // return the preserved opaque handle
}

/// Emits reference release and slot recycling without backend-close dispatch.
fn emit_resource_release(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: release an opaque resource handle ---");
    emitter.label_global("__rt_resource_release");
    emitter.instruction("sub sp, sp, #64");                                     // reserve release state and a saved frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and link register
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the opaque handle for free-list recycling
    emitter.instruction("bl __rt_resource_lookup_any");                         // validate and resolve the resource slot
    emitter.instruction("cbz x0, __rt_resource_release_done");                  // stale handles release no ownership
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the resolved slot pointer
    emitter.instruction(&format!(
        "ldr x9, [x0, #{}]", SLOT_REFS_OFFSET
    ));                                                                         // load the strong-reference count
    emitter.instruction("cmn x9, #1");                                          // is this a persistent immortal resource?
    emitter.instruction("b.eq __rt_resource_release_done");                     // never release standard-stream ownership
    emitter.instruction("cbz x9, __rt_resource_release_done");                  // tolerate an already exhausted reference count
    emitter.instruction("sub x9, x9, #1");                                      // release one strong reference
    emitter.instruction(&format!(
        "str x9, [x0, #{}]", SLOT_REFS_OFFSET
    ));                                                                         // publish the decremented reference count
    emitter.instruction("cbnz x9, __rt_resource_release_done");                 // keep the live slot while another owner remains

    emitter.instruction(&format!(
        "ldr x10, [x0, #{}]", SLOT_STATUS_OFFSET
    ));                                                                         // load lifecycle state before final release
    emitter.instruction(&format!(
        "cmp x10, #{}", RESOURCE_STATUS_CLOSING
    ));                                                                         // is a re-entrant close still executing?
    emitter.instruction("b.eq __rt_resource_release_done");                     // defer recycling until the active close completes
    emitter.instruction(&format!(
        "ldr x11, [x0, #{}]", SLOT_KIND_OFFSET
    ));                                                                         // load the resource kind before state teardown
    emitter.instruction(&format!(
        "cmp x11, #{}", RESOURCE_KIND_STREAM
    ));                                                                         // does this slot own a stream backend?
    emitter.instruction("b.ne __rt_resource_release_after_close");              // non-stream resources have no Gate 1 backend hook
    emitter.instruction(&format!(
        "cmp x10, #{}", RESOURCE_STATUS_LIVE
    ));                                                                         // does the stream still require exact-once close?
    emitter.instruction("b.ne __rt_resource_release_after_close");              // already-closed streams skip backend dispatch
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the opaque stream handle
    emitter.instruction("bl __rt_stream_close_backend");                        // close the supported backend before recycling state
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the opaque handle after close dispatch
    emitter.instruction("bl __rt_resource_lookup_any");                         // re-resolve the slot after nested lifecycle helpers
    emitter.instruction("cbz x0, __rt_resource_release_done");                  // tolerate defensive invalidation during close
    emitter.instruction("str x0, [sp, #8]");                                    // refresh the resolved slot pointer
    emitter.label("__rt_resource_release_after_close");
    emitter.instruction(&format!(
        "ldr x11, [x0, #{}]", SLOT_KIND_OFFSET
    ));                                                                         // reload the kind before owned-state teardown
    emitter.instruction(&format!(
        "cmp x11, #{}", RESOURCE_KIND_STREAM
    ));                                                                         // does this slot own a StreamState aggregate?
    emitter.instruction("b.ne __rt_resource_release_context_state");            // non-stream resources follow their own teardown path
    emitter.instruction(&format!(
        "ldr x10, [x0, #{}]", SLOT_FLAGS_OFFSET
    ));                                                                         // load stream-state ownership flags
    emitter.instruction(&format!(
        "tst x10, #{}", RESOURCE_FLAG_OWNS_STATE
    ));                                                                         // does the registry own this StreamState?
    emitter.instruction("b.eq __rt_resource_release_recycle");                  // persistent or borrowed stream states stay externally owned
    emitter.instruction(&format!(
        "ldr x11, [x0, #{}]", SLOT_STATE_PTR_OFFSET
    ));                                                                         // load the owned StreamState pointer
    emitter.instruction("cbz x11, __rt_resource_release_recycle");              // absent stream state requires no destructor
    emitter.instruction(&format!(
        "str xzr, [x0, #{}]", SLOT_STATE_PTR_OFFSET
    ));                                                                         // detach state before potentially re-entrant child cleanup
    emitter.instruction("mov x0, x11");                                         // pass StreamState to its typed destructor
    emitter.instruction("bl __rt_stream_destroy_state");                        // release URI, host, then StreamState exactly once
    emitter.instruction("b __rt_resource_release_after_state_destroy");         // re-resolve after potentially re-entrant child teardown
    emitter.label("__rt_resource_release_context_state");
    emitter.instruction(&format!(
        "ldr x11, [x0, #{}]", SLOT_KIND_OFFSET
    ));                                                                         // reload the resource kind after the stream branch
    emitter.instruction(&format!(
        "cmp x11, #{}", RESOURCE_KIND_CONTEXT
    ));                                                                         // does this slot own a ContextState aggregate?
    emitter.instruction("b.ne __rt_resource_release_generic_state");            // other kinds use ordinary state allocation teardown
    emitter.instruction(&format!(
        "ldr x10, [x0, #{}]", SLOT_FLAGS_OFFSET
    ));                                                                         // load context-state ownership flags
    emitter.instruction(&format!(
        "tst x10, #{}", RESOURCE_FLAG_OWNS_STATE
    ));                                                                         // does the registry own this ContextState?
    emitter.instruction("b.eq __rt_resource_release_recycle");                  // borrowed context states remain externally owned
    emitter.instruction(&format!(
        "ldr x11, [x0, #{}]", SLOT_STATE_PTR_OFFSET
    ));                                                                         // load the owned ContextState pointer
    emitter.instruction("cbz x11, __rt_resource_release_recycle");              // absent context state requires no destructor
    emitter.instruction(&format!(
        "str xzr, [x0, #{}]", SLOT_STATE_PTR_OFFSET
    ));                                                                         // detach the state before potentially re-entrant child cleanup
    emitter.instruction("mov x0, x11");                                         // pass ContextState to its typed destructor
    emitter.instruction("bl __rt_context_destroy_state");                       // release options, notifier, then ContextState exactly once
    emitter.instruction("b __rt_resource_release_after_state_destroy");         // re-resolve after potentially re-entrant child teardown
    emitter.label("__rt_resource_release_generic_state");
    emitter.instruction(&format!(
        "ldr x10, [x0, #{}]", SLOT_FLAGS_OFFSET
    ));                                                                         // load state ownership flags
    emitter.instruction("str x10, [sp, #16]");                                  // preserve flags across an optional heap free
    emitter.instruction(&format!(
        "ldr x11, [x0, #{}]", SLOT_STATE_PTR_OFFSET
    ));                                                                         // load the stable state pointer
    emitter.instruction("str x11, [sp, #24]");                                  // preserve the state pointer across helper calls
    emitter.instruction(&format!(
        "tst x10, #{}", RESOURCE_FLAG_OWNS_STATE
    ));                                                                         // does the registry own the state allocation?
    emitter.instruction("b.eq __rt_resource_release_recycle");                  // leave externally owned state untouched
    emitter.instruction("cbz x11, __rt_resource_release_recycle");              // skip absent state storage
    emitter.instruction("mov x0, x11");                                         // pass the owned state pointer to heap_free
    emitter.instruction("bl __rt_heap_free");                                   // release registry-owned state storage

    emitter.label("__rt_resource_release_after_state_destroy");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the generation-safe handle after child teardown
    emitter.instruction("bl __rt_resource_lookup_any");                         // re-resolve because teardown may have grown the registry
    emitter.instruction("cbz x0, __rt_resource_release_done");                  // tolerate defensive invalidation during teardown
    emitter.instruction("str x0, [sp, #8]");                                    // refresh the slot pointer before recycling it
    emitter.label("__rt_resource_release_recycle");
    emitter.instruction("ldr x12, [sp, #8]");                                   // reload the registry slot pointer
    emitter.instruction(&format!(
        "ldr x9, [x12, #{}]", SLOT_GENERATION_OFFSET
    ));                                                                         // load the retiring generation
    emitter.instruction("add w9, w9, #1");                                      // advance the 32-bit generation before reuse
    emitter.instruction("cbnz w9, __rt_resource_release_generation_ready");     // keep every non-zero wrapped generation
    emitter.instruction("mov w9, #1");                                          // reserve generation zero for standard streams
    emitter.label("__rt_resource_release_generation_ready");
    emitter.instruction(&format!(
        "str x9, [x12, #{}]", SLOT_GENERATION_OFFSET
    ));                                                                         // publish the next slot generation
    emitter.instruction(&format!(
        "str xzr, [x12, #{}]", SLOT_KIND_OFFSET
    ));                                                                         // mark the slot free
    emitter.instruction(&format!(
        "str xzr, [x12, #{}]", SLOT_STATUS_OFFSET
    ));                                                                         // clear lifecycle state
    emitter.instruction(&format!(
        "str xzr, [x12, #{}]", SLOT_REFS_OFFSET
    ));                                                                         // clear strong ownership
    emitter.instruction(&format!(
        "str xzr, [x12, #{}]", SLOT_PHP_ID_OFFSET
    ));                                                                         // clear the retired PHP id
    emitter.instruction(&format!(
        "str xzr, [x12, #{}]", SLOT_STATE_PTR_OFFSET
    ));                                                                         // clear the retired state pointer
    emitter.instruction(&format!(
        "str xzr, [x12, #{}]", SLOT_FLAGS_OFFSET
    ));                                                                         // clear retired ownership flags
    abi::emit_symbol_address(emitter, "x10", "_resource_registry_free");
    emitter.instruction("ldr x11, [x10]");                                      // load the previous one-based free-list head
    emitter.instruction(&format!(
        "str x11, [x12, #{}]", SLOT_NEXT_FREE_OFFSET
    ));                                                                         // link the recycled slot to the free list
    emitter.instruction("ldr x13, [sp, #0]");                                   // reload the retiring opaque handle
    emitter.instruction("ubfx x13, x13, #0, #32");                              // recover its one-based slot component
    emitter.instruction("str x13, [x10]");                                      // publish the recycled slot as the free-list head
    abi::emit_symbol_address(emitter, "x10", "_resource_registry_live");
    emitter.instruction("ldr x11, [x10]");                                      // load the current live-resource count
    emitter.instruction(&format!(
        "cmp x11, #{}", STANDARD_STREAM_COUNT
    ));                                                                         // protect the persistent standard-resource floor
    emitter.instruction("b.ls __rt_resource_release_done");                     // avoid underflow after defensive duplicate release
    emitter.instruction("sub x11, x11, #1");                                    // remove the recycled resource from the live count
    emitter.instruction("str x11, [x10]");                                      // publish the reduced live-resource count

    emitter.label("__rt_resource_release_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #64");                                     // release lifecycle scratch storage
    emitter.instruction("ret");                                                 // return after releasing or ignoring the handle
}

/// Emits request shutdown that force-releases every non-persistent registry slot.
fn emit_registry_request_reset(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: release request-owned opaque resources ---");
    emitter.label_global("__rt_resource_registry_request_reset");
    emitter.instruction("sub sp, sp, #64");                                     // reserve scan state and a saved frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame and link register
    emitter.instruction("add x29, sp, #48");                                    // establish a stable request-reset frame
    abi::emit_symbol_address(emitter, "x9", "_stream_context_options");
    emitter.instruction("str xzr, [x9]");                                       // clear the borrowed options bridge before request-owned states are destroyed
    abi::emit_symbol_address(emitter, "x9", "_stream_notification_callback");
    emitter.instruction("str xzr, [x9]");                                       // clear the borrowed notifier bridge before descriptor teardown
    abi::emit_symbol_address(emitter, "x9", "_stream_current_context_handle");
    emitter.instruction("str xzr, [x9]");                                       // clear the borrowed wrapper-context handle before teardown
    emitter.instruction("str xzr, [sp, #8]");                                   // phase zero releases streams before their attached contexts
    emitter.instruction("bl __rt_resource_registry_init");                      // make standard persistent slots available
    emitter.instruction("cbz x0, __rt_resource_registry_request_reset_done");   // tolerate registry allocation failure

    emitter.label("__rt_resource_registry_request_reset_restart");
    emitter.instruction(&format!(
        "mov x9, #{}", STANDARD_STREAM_COUNT
    ));                                                                         // begin after STDIN, STDOUT, and STDERR
    emitter.instruction("str x9, [sp, #0]");                                    // preserve the scan index across release callbacks
    emitter.label("__rt_resource_registry_request_reset_scan");
    abi::emit_symbol_address(emitter, "x10", "_resource_registry_len");
    emitter.instruction("ldr x10, [x10]");                                      // reload length because teardown callbacks may allocate
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the current scan index
    emitter.instruction("cmp x9, x10");                                         // have all initialized slots been inspected?
    emitter.instruction("b.hs __rt_resource_registry_request_reset_advance");   // finish when no request-owned slot remains
    abi::emit_symbol_address(emitter, "x11", "_resource_registry_ptr");
    emitter.instruction("ldr x11, [x11]");                                      // reload storage because callbacks may grow the registry
    emitter.instruction("add x12, x11, x9, lsl #6");                            // address the current registry slot
    emitter.instruction(&format!(
        "ldr x13, [x12, #{}]", SLOT_KIND_OFFSET
    ));                                                                         // load the slot kind
    emitter.instruction("cbz x13, __rt_resource_registry_request_reset_next");  // skip free slots
    emitter.instruction("ldr x14, [sp, #8]");                                   // load the current reset phase
    emitter.instruction("cbnz x14, __rt_resource_registry_request_reset_kind_ready"); // phase one releases every remaining resource kind
    emitter.instruction(&format!("cmp x13, #{}", RESOURCE_KIND_STREAM));        // is this a stream during the stream-first phase?
    emitter.instruction("b.ne __rt_resource_registry_request_reset_next");      // preserve contexts until all streams are destroyed
    emitter.label("__rt_resource_registry_request_reset_kind_ready");
    emitter.instruction(&format!(
        "ldr x13, [x12, #{}]", SLOT_FLAGS_OFFSET
    ));                                                                         // load resource persistence flags
    emitter.instruction(&format!(
        "tst x13, #{}", RESOURCE_FLAG_PERSISTENT
    ));                                                                         // are process-lifetime resources excluded from request cleanup?
    emitter.instruction("b.ne __rt_resource_registry_request_reset_next");      // preserve persistent resources
    emitter.instruction(&format!(
        "ldr x13, [x12, #{}]", SLOT_STATUS_OFFSET
    ));                                                                         // load lifecycle status before forcing final release
    emitter.instruction(&format!(
        "cmp x13, #{}", RESOURCE_STATUS_CLOSING
    ));                                                                         // is a re-entrant close still on the stack?
    emitter.instruction("b.eq __rt_resource_registry_request_reset_next");      // never recycle a resource during its active destructor
    emitter.instruction("mov x13, #1");                                         // collapse abandoned aliases to one shutdown owner
    emitter.instruction(&format!(
        "str x13, [x12, #{}]", SLOT_REFS_OFFSET
    ));                                                                         // make release deterministically retire this incarnation
    emitter.instruction(&format!(
        "ldr x13, [x12, #{}]", SLOT_GENERATION_OFFSET
    ));                                                                         // load the live generation
    emitter.instruction(&format!(
        "lsl x13, x13, #{}", HANDLE_INDEX_BITS
    ));                                                                         // place the generation in the opaque handle high word
    emitter.instruction("add x9, x9, #1");                                      // convert the index to its one-based handle word
    emitter.instruction("orr x0, x13, x9");                                     // reconstruct the exact live opaque handle
    emitter.instruction("bl __rt_resource_release");                            // close, destroy state, invalidate, and recycle the slot
    emitter.instruction("b __rt_resource_registry_request_reset_restart");      // restart after callbacks may move storage or allocate resources

    emitter.label("__rt_resource_registry_request_reset_next");
    emitter.instruction("add x9, x9, #1");                                      // advance to the next initialized slot
    emitter.instruction("str x9, [sp, #0]");                                    // publish the next scan index
    emitter.instruction("b __rt_resource_registry_request_reset_scan");         // continue searching for request-owned resources

    emitter.label("__rt_resource_registry_request_reset_advance");
    emitter.instruction("ldr x10, [sp, #8]");                                   // load the completed reset phase
    emitter.instruction("cbnz x10, __rt_resource_registry_request_reset_epoch"); // phase one completed all remaining resources
    emitter.instruction("mov x10, #1");                                         // advance from stream teardown to context teardown
    emitter.instruction("str x10, [sp, #8]");                                   // publish the remaining-resource phase
    abi::emit_symbol_address(emitter, "x9", "_stream_default_context_handle");
    emitter.instruction("str xzr, [x9]");                                       // detach the default owner only after attached streams released it
    emitter.instruction("b __rt_resource_registry_request_reset_restart");      // rescan from the beginning for contexts and other resources
    emitter.label("__rt_resource_registry_request_reset_epoch");
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_epoch");
    emitter.instruction("ldr x10, [x9]");                                       // load the completed request epoch
    emitter.instruction("add x10, x10, #1");                                    // advance allocation ownership to the next request
    emitter.instruction("cbnz x10, __rt_resource_registry_request_reset_epoch_ready"); // keep every non-zero wrapped epoch
    emitter.instruction("mov x10, #1");                                         // reserve epoch zero for process-persistent resources
    emitter.label("__rt_resource_registry_request_reset_epoch_ready");
    emitter.instruction("str x10, [x9]");                                       // publish the next request epoch
    emitter.label("__rt_resource_registry_request_reset_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #64");                                     // release request-reset scratch storage
    emitter.instruction("ret");                                                 // return after deterministic request shutdown
}

/// Emits process-exit teardown: releases the slot array `__rt_resource_registry_init`
/// allocated and resets the globals so a later init starts from a clean registry.
///
/// This is the counterpart of init, NOT of the request reset. A `--web` worker reuses
/// one slot array across requests and must only run the request reset; the CLI epilogue
/// runs this afterwards so the array does not show up as a leaked block under
/// `--heap-debug` in every program, stream-using or not.
fn emit_registry_teardown(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: release the opaque resource registry at process exit ---");
    emitter.label_global("__rt_resource_registry_teardown");
    emitter.instruction("sub sp, sp, #16");                                     // reserve a frame for the heap-free call
    emitter.instruction("str x30, [sp, #8]");                                   // save the caller link register
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_ptr");
    emitter.instruction("ldr x0, [x9]");                                        // load the dynamic slot-array pointer
    emitter.instruction("cbz x0, __rt_resource_registry_teardown_done");        // an uninitialized registry owns no storage
    emitter.instruction("str xzr, [x9]");                                       // clear the pointer before releasing it
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_static_slots");
    emitter.instruction("cmp x0, x9");                                          // is the registry still on its STATIC reservation?
    emitter.instruction("b.eq __rt_resource_registry_teardown_reset");          // static storage is not heap storage: only the globals reset
    emitter.instruction("bl __rt_heap_free");                                   // release a grown, heap-allocated slot array
    emitter.label("__rt_resource_registry_teardown_reset");
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_len");
    emitter.instruction("str xzr, [x9]");                                       // no slots remain initialized
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_cap");
    emitter.instruction("str xzr, [x9]");                                       // no capacity remains
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_free");
    emitter.instruction("str xzr, [x9]");                                       // drop the free-list head with its storage
    abi::emit_symbol_address(emitter, "x9", "_resource_registry_live");
    emitter.instruction("str xzr, [x9]");                                       // no live resources remain
    emitter.label("__rt_resource_registry_teardown_done");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller link register
    emitter.instruction("add sp, sp, #16");                                     // release the teardown frame
    emitter.instruction("ret");
}

/// Emits the Live-to-Closing lifecycle transition used before re-entrant cleanup.
fn emit_resource_mark_closing(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mark an opaque resource as closing ---");
    emitter.label_global("__rt_resource_mark_closing");
    emitter.instruction("sub sp, sp, #16");                                     // preserve the link register around lookup
    emitter.instruction("str x30, [sp, #8]");                                   // save the caller link register
    emitter.instruction("bl __rt_resource_lookup_any");                         // validate and resolve the resource slot
    emitter.instruction("cbz x0, __rt_resource_mark_closing_fail");             // reject stale handles
    emitter.instruction(&format!(
        "ldr x9, [x0, #{}]", SLOT_STATUS_OFFSET
    ));                                                                         // load the lifecycle status
    emitter.instruction(&format!(
        "cmp x9, #{}", RESOURCE_STATUS_LIVE
    ));                                                                         // is the resource currently live?
    emitter.instruction("b.ne __rt_resource_mark_closing_fail");                // only Live may transition to Closing
    emitter.instruction(&format!(
        "mov x9, #{}", RESOURCE_STATUS_CLOSING
    ));                                                                         // select the re-entrant closing state
    emitter.instruction(&format!(
        "str x9, [x0, #{}]", SLOT_STATUS_OFFSET
    ));                                                                         // publish Closing before backend callbacks
    emitter.instruction("mov x0, #1");                                          // report a successful transition
    emitter.instruction("b __rt_resource_mark_closing_done");                   // join the helper epilogue
    emitter.label("__rt_resource_mark_closing_fail");
    emitter.instruction("mov x0, #0");                                          // report invalid or already-transitioned resources
    emitter.label("__rt_resource_mark_closing_done");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller link register
    emitter.instruction("add sp, sp, #16");                                     // release the aligned link-register save
    emitter.instruction("ret");                                                 // return the transition status
}

/// Emits the Live/Closing-to-Closed lifecycle transition after cleanup.
fn emit_resource_mark_closed(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mark an opaque resource as closed ---");
    emitter.label_global("__rt_resource_mark_closed");
    emitter.instruction("sub sp, sp, #32");                                     // preserve the handle and caller frame around nested release
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save the caller frame and link register
    emitter.instruction("add x29, sp, #16");                                    // establish a stable transition frame
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the exact opaque handle
    emitter.instruction("bl __rt_resource_lookup_any");                         // validate and resolve the resource slot
    emitter.instruction("cbz x0, __rt_resource_mark_closed_fail");              // reject stale handles
    emitter.instruction(&format!(
        "ldr x9, [x0, #{}]", SLOT_STATUS_OFFSET
    ));                                                                         // load the lifecycle status
    emitter.instruction(&format!(
        "cmp x9, #{}", RESOURCE_STATUS_CLOSED
    ));                                                                         // is the resource already closed?
    emitter.instruction("b.eq __rt_resource_mark_closed_fail");                 // preserve exactly-once close reporting
    emitter.instruction(&format!(
        "cmp x9, #{}", RESOURCE_STATUS_LIVE
    ));                                                                         // can Live transition directly to Closed?
    emitter.instruction("b.eq __rt_resource_mark_closed_store");                // accept close paths without callbacks
    emitter.instruction(&format!(
        "cmp x9, #{}", RESOURCE_STATUS_CLOSING
    ));                                                                         // did cleanup publish Closing first?
    emitter.instruction("b.ne __rt_resource_mark_closed_fail");                 // reject every other lifecycle state
    emitter.label("__rt_resource_mark_closed_store");
    emitter.instruction(&format!(
        "mov x9, #{}", RESOURCE_STATUS_CLOSED
    ));                                                                         // select the terminal closed state
    emitter.instruction(&format!(
        "str x9, [x0, #{}]", SLOT_STATUS_OFFSET
    ));                                                                         // publish Closed after cleanup
    emitter.instruction(&format!(
        "ldr x9, [x0, #{}]", SLOT_REFS_OFFSET
    ));                                                                         // inspect ownership deferred by a re-entrant close
    emitter.instruction("cbnz x9, __rt_resource_mark_closed_success");          // a remaining owner keeps the Closed slot addressable
    emitter.instruction("mov x9, #1");                                          // restore one synthetic owner for the common release path
    emitter.instruction(&format!(
        "str x9, [x0, #{}]", SLOT_REFS_OFFSET
    ));                                                                         // make deferred zero-ref cleanup releasable
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the original generation-safe handle
    emitter.instruction("bl __rt_resource_release");                            // destroy owned state and recycle the deferred slot
    emitter.label("__rt_resource_mark_closed_success");
    emitter.instruction("mov x0, #1");                                          // report a successful transition
    emitter.instruction("b __rt_resource_mark_closed_done");                    // join the helper epilogue
    emitter.label("__rt_resource_mark_closed_fail");
    emitter.instruction("mov x0, #0");                                          // report invalid or already-closed resources
    emitter.label("__rt_resource_mark_closed_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #32");                                     // release transition scratch storage
    emitter.instruction("ret");                                                 // return the transition status
}

/// Emits registry-backed PHP resource-id lookup for live or closed handles.
fn emit_resource_id_of_registry(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: read the PHP id of an opaque resource ---");
    emitter.label_global("__rt_resource_id_of_registry");
    emitter.instruction("sub sp, sp, #16");                                     // preserve the link register around lookup
    emitter.instruction("str x30, [sp, #8]");                                   // save the caller link register
    emitter.instruction("bl __rt_resource_lookup_any");                         // validate and resolve the resource slot
    emitter.instruction("cbz x0, __rt_resource_id_of_registry_done");           // invalid resources report id zero
    emitter.instruction(&format!(
        "ldr x0, [x0, #{}]", SLOT_PHP_ID_OFFSET
    ));                                                                         // return the slot's PHP-visible resource id
    emitter.label("__rt_resource_id_of_registry_done");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller link register
    emitter.instruction("add sp, sp, #16");                                     // release the aligned link-register save
    emitter.instruction("ret");                                                 // return the resource id or zero
}

/// Emits generation-aware testing for a resource whose lifecycle status is exactly Live.
fn emit_resource_is_open(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: test whether an opaque resource is open ---");
    emitter.label_global("__rt_resource_is_open");
    emitter.instruction("sub sp, sp, #16");                                     // preserve the link register around lookup
    emitter.instruction("str x30, [sp, #8]");                                   // save the caller link register
    emitter.instruction("bl __rt_resource_lookup_any");                         // validate and resolve the opaque handle
    emitter.instruction("cbz x0, __rt_resource_is_open_false");                 // invalid or stale handles are not open
    emitter.instruction(&format!(
        "ldr x9, [x0, #{}]", SLOT_STATUS_OFFSET
    ));                                                                         // load the lifecycle status
    emitter.instruction(&format!(
        "cmp x9, #{}", RESOURCE_STATUS_LIVE
    ));                                                                         // is the resource exactly Live?
    emitter.instruction("cset x0, eq");                                         // materialize the open-resource predicate
    emitter.instruction("b __rt_resource_is_open_done");                        // join the helper epilogue
    emitter.label("__rt_resource_is_open_false");
    emitter.instruction("mov x0, #0");                                          // report false for invalid handles
    emitter.label("__rt_resource_is_open_done");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller link register
    emitter.instruction("add sp, sp, #16");                                     // release the aligned link-register save
    emitter.instruction("ret");                                                 // return one for Live, otherwise zero
}

/// Emits live-resource kind lookup, returning zero for stale, closed, or invalid handles.
fn emit_resource_kind_if_open(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve the kind of a live opaque resource ---");
    emitter.label_global("__rt_resource_kind_if_open");
    emitter.instruction("sub sp, sp, #16");                                     // preserve the link register around generic registry lookup
    emitter.instruction("str x30, [sp, #8]");                                   // save the caller return address
    emitter.instruction("bl __rt_resource_lookup_any");                         // validate the opaque handle generation and slot
    emitter.instruction("cbz x0, __rt_resource_kind_if_open_false");            // invalid or stale handles have no live resource kind
    emitter.instruction(&format!(
        "ldr x9, [x0, #{}]", SLOT_STATUS_OFFSET
    ));                                                                         // load the resource lifecycle state
    emitter.instruction(&format!(
        "cmp x9, #{}", RESOURCE_STATUS_LIVE
    ));                                                                         // only Live entries remain PHP resources
    emitter.instruction("b.ne __rt_resource_kind_if_open_false");               // Closing and Closed entries report no kind
    emitter.instruction(&format!(
        "ldr x0, [x0, #{}]", SLOT_KIND_OFFSET
    ));                                                                         // return the live registry resource kind
    emitter.instruction("b __rt_resource_kind_if_open_done");                   // join the helper epilogue
    emitter.label("__rt_resource_kind_if_open_false");
    emitter.instruction("mov x0, #0");                                          // return kind zero for invalid or non-live resources
    emitter.label("__rt_resource_kind_if_open_done");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller return address
    emitter.instruction("add sp, sp, #16");                                     // release aligned lookup scratch storage
    emitter.instruction("ret");                                                 // return the live resource kind or zero
}
