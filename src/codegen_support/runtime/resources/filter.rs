//! Purpose:
//! Emits the stream-filter resource primitives: `__rt_filter_state` (opaque
//! handle → FilterState), `__rt_filter_create`, and the doubly linked chain
//! operations `__rt_stream_filter_link` / `__rt_stream_filter_unlink`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::resources::emit_resources()`.
//! - `stream_filter_append` / `stream_filter_prepend` / `stream_filter_remove`
//!   lowering, and stream teardown when closing still-attached filters.
//!
//! Key details:
//! - PHP hands `stream_filter_append()` back a resource whose lifetime is
//!   observable: `stream_filter_remove()` closes it, and closing the owning
//!   stream closes every filter still attached. Modelling filters as registry
//!   resources is what makes `is_resource()` report that invalidation.
//! - The chain is doubly linked so a middle node can be unlinked without
//!   disturbing its neighbours. The previous design stored two filter-id bytes
//!   per descriptor and could not represent a third filter at all.
//! - Read and write chains are separate lists rooted in StreamState. A filter
//!   attached with `STREAM_FILTER_ALL` is linked into both, which is why the
//!   direction bits are stored per node rather than inferred from the root.

use crate::codegen_support::runtime::resources::layout::{
    FILTER_BUILTIN_ID_OFFSET, FILTER_DIRECTION_OFFSET, FILTER_FLAGS_OFFSET, FILTER_NEXT_OFFSET,
    FILTER_OBJECT_OFFSET, FILTER_PARAMS_OFFSET, FILTER_PREV_OFFSET, FILTER_STATE_SIZE,
    FILTER_STREAM_HANDLE_OFFSET, RESOURCE_KIND_FILTER, RESOURCE_STATUS_LIVE,
    STREAM_READ_FILTER_HEAD_OFFSET,
    SLOT_KIND_OFFSET, SLOT_STATE_PTR_OFFSET, SLOT_STATUS_OFFSET,
    STREAM_WRITE_FILTER_HEAD_OFFSET,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits every filter-resource runtime helper for the active target.
pub(crate) fn emit_filter_resources(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_filter_state_aarch64(emitter);
            emit_filter_create_aarch64(emitter);
            emit_filter_link_aarch64(emitter);
            emit_filter_unlink_aarch64(emitter);
            emit_filter_apply_chain_aarch64(emitter);
            emit_fwrite_filtered_aarch64(emitter);
            emit_stream_close_filter_chains(emitter);
            emit_filter_node_close_obj(emitter);
            emit_filter_node_closing_flush(emitter);
        }
        Arch::X86_64 => {
            emit_filter_state_x86_64(emitter);
            emit_filter_create_x86_64(emitter);
            emit_filter_link_x86_64(emitter);
            emit_filter_unlink_x86_64(emitter);
            emit_filter_apply_chain_x86_64(emitter);
            emit_fwrite_filtered_x86_64(emitter);
            emit_stream_close_filter_chains(emitter);
            emit_filter_node_close_obj(emitter);
            emit_filter_node_closing_flush(emitter);
        }
    }
}

/// `__rt_filter_state(handle) -> FilterState*` (AArch64), null when not a live filter.
fn emit_filter_state_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve an opaque stream-filter state ---");
    emitter.label_global("__rt_filter_state");
    emitter.instruction("sub sp, sp, #16");                                     // preserve the link register around generic lookup
    emitter.instruction("str x30, [sp, #8]");                                   // save the caller link register
    emitter.instruction("bl __rt_resource_lookup_any");                         // validate and resolve the opaque handle
    emitter.instruction("cbz x0, __rt_filter_state_fail");                      // reject invalid or stale resources
    emitter.instruction(&format!("ldr x9, [x0, #{SLOT_KIND_OFFSET}]"));         // load the registry resource kind
    emitter.instruction(&format!("cmp x9, #{RESOURCE_KIND_FILTER}"));           // is the slot a stream filter?
    emitter.instruction("b.ne __rt_filter_state_fail");                         // reject streams, contexts, and other resources
    emitter.instruction(&format!("ldr x9, [x0, #{SLOT_STATUS_OFFSET}]"));       // load the filter lifecycle state
    emitter.instruction(&format!("cmp x9, #{RESOURCE_STATUS_LIVE}"));           // only Live filters expose their state
    emitter.instruction("b.ne __rt_filter_state_fail");                         // reject Closing and Closed filters
    emitter.instruction(&format!("ldr x0, [x0, #{SLOT_STATE_PTR_OFFSET}]"));    // return the stable filter-state pointer
    emitter.instruction("b __rt_filter_state_done");                            // join the helper epilogue
    emitter.label("__rt_filter_state_fail");
    emitter.instruction("mov x0, #0");                                          // return null for invalid filter resources
    emitter.label("__rt_filter_state_done");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller link register
    emitter.instruction("add sp, sp, #16");                                     // release the aligned link-register save
    emitter.instruction("ret");                                                 // return the filter-state pointer or null
}

/// `__rt_filter_create(builtin_id, obj, direction, params) -> handle` (AArch64).
///
/// Allocates a zeroed FilterState, seeds it, and registers it as a filter
/// resource. Returns 0 when the heap or the registry is exhausted.
fn emit_filter_create_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: create a stream-filter resource ---");
    emitter.label_global("__rt_filter_create");
    // Frame: [0]=builtin id [8]=obj [16]=direction [24]=params [32]=state ptr
    emitter.instruction("sub sp, sp, #64");                                     // reserve the creation frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the built-in filter id
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the user-filter object
    emitter.instruction("str x2, [sp, #16]");                                   // preserve the direction bits
    emitter.instruction("str x3, [sp, #24]");                                   // preserve the retained params value

    emitter.instruction(&format!("mov x0, #{FILTER_STATE_SIZE}"));              // FilterState payload size
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the stable filter state
    emitter.instruction("cbz x0, __rt_filter_create_fail");                     // heap exhausted
    emitter.instruction("str x0, [sp, #32]");                                   // spill the state pointer across the registry call

    // -- zero the payload so both chain links start empty --
    emitter.instruction("mov x9, #0");                                          // byte cursor
    emitter.label("__rt_filter_create_zero");
    emitter.instruction(&format!("cmp x9, #{FILTER_STATE_SIZE}"));              // cleared every byte?
    emitter.instruction("b.ge __rt_filter_create_zeroed");
    emitter.instruction("str xzr, [x0, x9]");                                   // clear one word
    emitter.instruction("add x9, x9, #8");                                      // advance the cursor
    emitter.instruction("b __rt_filter_create_zero");
    emitter.label("__rt_filter_create_zeroed");

    // -- seed the descriptor fields --
    emitter.instruction("ldr x9, [sp, #0]");
    emitter.instruction(&format!("str x9, [x0, #{FILTER_BUILTIN_ID_OFFSET}]")); // built-in filter id (0 for user filters)
    emitter.instruction("ldr x9, [sp, #8]");
    emitter.instruction(&format!("str x9, [x0, #{FILTER_OBJECT_OFFSET}]"));     // php_user_filter instance
    emitter.instruction("ldr x9, [sp, #16]");
    emitter.instruction(&format!("str x9, [x0, #{FILTER_DIRECTION_OFFSET}]"));  // direction bits
    emitter.instruction("ldr x9, [sp, #24]");
    emitter.instruction(&format!("str x9, [x0, #{FILTER_PARAMS_OFFSET}]"));     // retained params value

    // -- read the built-in filter parameters out of `$params`, ONCE --
    //
    // php parses `$params` in each filter's `create` callback, not per buffer, and for the same
    // reason: `line-length`, `line-break-chars` and `binary` cannot change once the filter is
    // attached, so probing the hash on every buffer would put a lookup in the encoder's inner
    // path for a constant.
    emitter.instruction("bl __rt_filter_absorb_params");                        // x0 = the node it fills
    emitter.instruction("cbz x0, __rt_filter_create_bad_params");               // php refuses the attach outright
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the state the absorber consumed

    // -- register the filter as an owned resource --
    emitter.instruction("mov x1, x0");                                          // stable state pointer
    emitter.instruction(&format!("mov x0, #{RESOURCE_KIND_FILTER}"));           // resource kind
    emitter.instruction("mov x2, #1");                                          // RESOURCE_FLAG_OWNS_STATE
    emitter.instruction("bl __rt_resource_alloc");                              // x0 = opaque filter handle (0 on failure)
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the creation frame
    emitter.instruction("ret");                                                 // return the filter handle

    emitter.label("__rt_filter_create_bad_params");
    // The state was allocated before the parameters could be read, so it is released here rather
    // than leaked: the caller sees the same 0 an allocation failure produces and reports php's
    // wording for the refusal.
    emitter.instruction("ldr x0, [sp, #32]");                                   // the state nobody will own
    emitter.instruction("bl __rt_heap_free");
    emitter.label("__rt_filter_create_fail");
    emitter.instruction("mov x0, #0");                                          // report allocation failure
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the creation frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// `__rt_stream_filter_link(stream_handle, filter_handle, head_offset, prepend)` (AArch64).
///
/// Links a filter node into one direction's chain. `prepend` non-zero inserts at
/// the head; otherwise the node is appended at the tail, which is what
/// `stream_filter_append()` needs so the chain applies in attachment order.
fn emit_filter_link_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: link a filter into a stream chain ---");
    emitter.label_global("__rt_stream_filter_link");
    // Frame: [0]=stream handle [8]=filter handle [16]=head offset [24]=prepend
    //        [32]=filter state [40]=stream state
    emitter.instruction("sub sp, sp, #64");                                     // reserve the link frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the owning stream handle
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the filter handle
    emitter.instruction("str x2, [sp, #16]");                                   // preserve the chain-head byte offset
    emitter.instruction("str x3, [sp, #24]");                                   // preserve the prepend flag

    emitter.instruction("mov x0, x1");                                          // resolve the filter state
    emitter.instruction("bl __rt_filter_state");
    emitter.instruction("cbz x0, __rt_filter_link_fail");                       // reject a dead filter handle
    emitter.instruction("str x0, [sp, #32]");                                   // spill the filter state
    emitter.instruction("ldr x0, [sp, #0]");                                    // resolve the owning stream state
    emitter.instruction("bl __rt_stream_state");
    emitter.instruction("cbz x0, __rt_filter_link_fail");                       // reject a dead stream handle
    emitter.instruction("str x0, [sp, #40]");                                   // spill the stream state

    // -- record the owner so removal can find this chain again --
    emitter.instruction("ldr x9, [sp, #32]");                                   // filter state
    emitter.instruction("ldr x10, [sp, #0]");                                   // owning stream handle
    emitter.instruction(&format!(
        "str x10, [x9, #{FILTER_STREAM_HANDLE_OFFSET}]"
    ));                                                                         // remember the owning stream

    emitter.instruction("ldr x11, [sp, #16]");                                  // chain-head offset within StreamState
    emitter.instruction("ldr x12, [sp, #40]");                                  // stream state
    emitter.instruction("add x11, x12, x11");                                   // address of the chain head slot
    emitter.instruction("ldr x13, [x11]");                                      // current head filter handle
    emitter.instruction("ldr x14, [sp, #24]");                                  // prepend flag
    emitter.instruction("cbz x14, __rt_filter_link_append");                    // append is the stream_filter_append path

    // -- prepend: the new node becomes the head --
    emitter.instruction("ldr x15, [sp, #8]");                                   // the new filter handle
    emitter.instruction(&format!("str x13, [x9, #{FILTER_NEXT_OFFSET}]"));      // new->next = old head
    emitter.instruction(&format!("str xzr, [x9, #{FILTER_PREV_OFFSET}]"));      // new->prev = none
    emitter.instruction("str x15, [x11]");                                      // head = new node
    emitter.instruction("cbz x13, __rt_filter_link_ok");                        // empty chain: nothing to back-link
    emitter.instruction("mov x0, x13");                                         // resolve the previous head
    emitter.instruction("bl __rt_filter_state");
    emitter.instruction("cbz x0, __rt_filter_link_ok");                         // a stale head simply ends the chain
    emitter.instruction("ldr x15, [sp, #8]");                                   // reload the new filter handle
    emitter.instruction(&format!("str x15, [x0, #{FILTER_PREV_OFFSET}]"));      // old head->prev = new node
    emitter.instruction("b __rt_filter_link_ok");

    // -- append: walk to the tail and link there --
    emitter.label("__rt_filter_link_append");
    emitter.instruction("cbnz x13, __rt_filter_link_walk");                     // non-empty chain: find the tail
    emitter.instruction("ldr x15, [sp, #8]");                                   // the new filter handle
    emitter.instruction("str x15, [x11]");                                      // empty chain: the node is the head
    emitter.instruction("b __rt_filter_link_ok");

    emitter.label("__rt_filter_link_walk");
    emitter.instruction("mov x0, x13");                                         // resolve the current node
    emitter.instruction("bl __rt_filter_state");
    emitter.instruction("cbz x0, __rt_filter_link_fail");                       // a broken chain cannot be extended
    emitter.instruction(&format!("ldr x14, [x0, #{FILTER_NEXT_OFFSET}]"));      // next handle
    emitter.instruction("cbz x14, __rt_filter_link_tail");                      // reached the tail
    emitter.instruction("mov x13, x14");                                        // advance to the next node
    emitter.instruction("b __rt_filter_link_walk");

    emitter.label("__rt_filter_link_tail");
    emitter.instruction("ldr x15, [sp, #8]");                                   // the new filter handle
    emitter.instruction(&format!("str x15, [x0, #{FILTER_NEXT_OFFSET}]"));      // tail->next = new node
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the new filter state
    emitter.instruction(&format!("str x13, [x9, #{FILTER_PREV_OFFSET}]"));      // new->prev = old tail
    emitter.instruction(&format!("str xzr, [x9, #{FILTER_NEXT_OFFSET}]"));      // new->next = none

    emitter.label("__rt_filter_link_ok");
    emitter.instruction("mov x0, #1");                                          // report success
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the link frame
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_filter_link_fail");
    emitter.instruction("mov x0, #0");                                          // report failure
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the link frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// `__rt_stream_filter_unlink(filter_handle, head_offset)` (AArch64).
///
/// Detaches one node from a chain, repairing both neighbours. The head slot is
/// only rewritten when the removed node actually was the head, so removing a
/// middle node leaves its neighbours attached.
fn emit_filter_unlink_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unlink a filter from a stream chain ---");
    emitter.label_global("__rt_stream_filter_unlink");
    // Frame: [0]=filter handle [8]=head offset [16]=prev [24]=next [32]=filter state
    emitter.instruction("sub sp, sp, #64");                                     // reserve the unlink frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the filter handle
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the chain-head byte offset

    emitter.instruction("bl __rt_filter_state");                                // resolve the filter state
    emitter.instruction("cbz x0, __rt_filter_unlink_fail");                     // nothing to unlink
    emitter.instruction("str x0, [sp, #32]");                                   // spill the filter state
    emitter.instruction(&format!("ldr x9, [x0, #{FILTER_PREV_OFFSET}]"));       // previous node handle
    emitter.instruction("str x9, [sp, #16]");
    emitter.instruction(&format!("ldr x10, [x0, #{FILTER_NEXT_OFFSET}]"));      // next node handle
    emitter.instruction("str x10, [sp, #24]");

    // -- prev->next = next, or move the chain head when this node was first --
    emitter.instruction("ldr x9, [sp, #16]");
    emitter.instruction("cbz x9, __rt_filter_unlink_was_head");                 // no predecessor: this node was the head
    emitter.instruction("mov x0, x9");                                          // resolve the predecessor
    emitter.instruction("bl __rt_filter_state");
    emitter.instruction("cbz x0, __rt_filter_unlink_next");                     // a stale predecessor needs no repair
    emitter.instruction("ldr x10, [sp, #24]");                                  // successor handle
    emitter.instruction(&format!("str x10, [x0, #{FILTER_NEXT_OFFSET}]"));      // prev->next = next
    emitter.instruction("b __rt_filter_unlink_next");

    emitter.label("__rt_filter_unlink_was_head");
    emitter.instruction("ldr x0, [sp, #32]");                                   // the removed filter state
    emitter.instruction(&format!(
        "ldr x0, [x0, #{FILTER_STREAM_HANDLE_OFFSET}]"
    ));                                                                         // the owning stream handle
    emitter.instruction("cbz x0, __rt_filter_unlink_next");                     // detached node: no chain to repair
    emitter.instruction("bl __rt_stream_state");                                // resolve the owning stream state
    emitter.instruction("cbz x0, __rt_filter_unlink_next");                     // the stream is gone: nothing to repair
    emitter.instruction("ldr x11, [sp, #8]");                                   // chain-head offset
    emitter.instruction("add x11, x0, x11");                                    // address of the chain head slot
    emitter.instruction("ldr x10, [sp, #24]");                                  // successor handle
    emitter.instruction("str x10, [x11]");                                      // head = next

    // -- next->prev = prev --
    emitter.label("__rt_filter_unlink_next");
    emitter.instruction("ldr x10, [sp, #24]");
    emitter.instruction("cbz x10, __rt_filter_unlink_ok");                      // no successor to repair
    emitter.instruction("mov x0, x10");                                         // resolve the successor
    emitter.instruction("bl __rt_filter_state");
    emitter.instruction("cbz x0, __rt_filter_unlink_ok");                       // a stale successor needs no repair
    emitter.instruction("ldr x9, [sp, #16]");                                   // predecessor handle
    emitter.instruction(&format!("str x9, [x0, #{FILTER_PREV_OFFSET}]"));       // next->prev = prev

    emitter.label("__rt_filter_unlink_ok");
    // -- isolate the removed node so a double removal is inert --
    emitter.instruction("ldr x0, [sp, #32]");
    emitter.instruction(&format!("str xzr, [x0, #{FILTER_NEXT_OFFSET}]"));
    emitter.instruction(&format!("str xzr, [x0, #{FILTER_PREV_OFFSET}]"));
    emitter.instruction(&format!("str xzr, [x0, #{FILTER_STREAM_HANDLE_OFFSET}]"));
    emitter.instruction("mov x0, #1");                                          // report success
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the unlink frame
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_filter_unlink_fail");
    emitter.instruction("mov x0, #0");                                          // report failure
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the unlink frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// `__rt_stream_apply_filter_chain(stream, buf, len, head_offset) -> len` (AArch64).
///
/// Walks one direction's chain and applies every node to the buffer in order.
/// Built-in nodes reuse `__rt_apply_stream_filter`, which transforms in place and
/// returns the (possibly changed) length, so length-changing filters such as
/// `convert.base64-encode` thread through the chain correctly.
///
/// Input:  x0 = stream handle, x1 = buffer, x2 = length, x3 = chain-head offset.
/// Output: x2 = resulting length; the buffer pointer is preserved in x1.
fn emit_filter_apply_chain_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: apply a stream filter chain ---");
    emitter.label_global("__rt_stream_apply_filter_chain");
    // Frame: [0]=buffer [8]=length [16]=node handle
    emitter.instruction("sub sp, sp, #48");                                     // reserve the chain-walk frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // preserve the buffer pointer
    emitter.instruction("str x2, [sp, #8]");                                    // preserve the current length
    // The offset must live in the frame, not a register: x9 is caller-saved and
    // __rt_stream_state clobbers it, which silently turned the chain-head address
    // into garbage and made every attached filter look absent.
    emitter.instruction("str x3, [sp, #24]");                                   // preserve the chain-head offset

    emitter.instruction("bl __rt_stream_state");                                // resolve the owning stream state
    emitter.instruction("cbz x0, __rt_apply_chain_done");                       // no live stream: pass the buffer through
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the chain-head offset
    emitter.instruction("add x0, x0, x9");                                      // address of the chain head slot
    emitter.instruction("ldr x10, [x0]");                                       // head filter handle
    emitter.instruction("str x10, [sp, #16]");                                  // preserve the walk cursor

    emitter.label("__rt_apply_chain_loop");
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the current node handle
    emitter.instruction("cbz x10, __rt_apply_chain_done");                      // end of chain
    emitter.instruction("mov x0, x10");                                         // resolve the node state
    emitter.instruction("bl __rt_filter_state");
    emitter.instruction("cbz x0, __rt_apply_chain_done");                       // a stale node ends the chain

    // -- advance the cursor before dispatching: the filter call clobbers scratch --
    emitter.instruction(&format!("ldr x11, [x0, #{FILTER_NEXT_OFFSET}]"));      // next node handle
    emitter.instruction("str x11, [sp, #16]");                                  // store the updated cursor
    emitter.instruction(&format!("ldr x12, [x0, #{FILTER_BUILTIN_ID_OFFSET}]")); // built-in filter id
    emitter.instruction("cbz x12, __rt_apply_chain_user");                      // id 0 marks a user filter carried by this node

    emitter.instruction("mov x14, x0");                                         // the node stays reachable across the reloads
    emitter.instruction("ldr x1, [sp, #0]");                                    // buffer pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // current length
    emitter.instruction("mov x3, x12");                                         // built-in filter id
    abi::emit_push_reg_pair(emitter, "x1", "x2");                               // the buffer pair outlives the publish
    abi::emit_push_reg(emitter, "x3");
    emitter.instruction("mov x0, x14");                                         // the node whose parameters apply
    emitter.instruction("bl __rt_asf_params_load");                             // the encoders read them from globals
    abi::emit_pop_reg(emitter, "x3");
    abi::emit_pop_reg_pair(emitter, "x1", "x2");
    emitter.instruction("bl __rt_apply_stream_filter");                         // transform in place; x2 = new length
    emitter.instruction("str x2, [sp, #8]");                                    // carry the new length to the next node
    // The published parameters are cleared immediately, so they are non-zero for exactly the
    // duration of one application. The legacy per-descriptor path (which still serves `zlib.*` and
    // `bzip2.*`) calls the same applier without a node, and a `line-length` left behind by an
    // unrelated chain would silently wrap ITS output.
    emitter.instruction("mov x0, #0");
    emitter.instruction("bl __rt_asf_params_load");                             // clear them again
    emitter.instruction("b __rt_apply_chain_loop");                             // continue down the chain

    // A user filter runs from the node's own `php_user_filter`, through the same
    // instance-keyed dispatch the per-descriptor path uses. Unlike a built-in, it may
    // answer with a DIFFERENT buffer (its bucket brigade builds one), so both halves of
    // the pair are carried forward, not just the length.
    emitter.label("__rt_apply_chain_user");
    // php gives `filter()` the stream it is running for, and takes it away again afterwards.
    // The node knows which stream that is; the dispatch two calls down does not, so the handle
    // travels through a global for exactly the duration of the call.
    emitter.instruction(&format!("ldr x9, [x0, #{FILTER_STREAM_HANDLE_OFFSET}]")); // the stream this node filters
    abi::emit_symbol_address(emitter, "x10", "_user_filter_current_stream");
    emitter.instruction("str x9, [x10]");
    emitter.instruction(&format!("ldr x0, [x0, #{FILTER_OBJECT_OFFSET}]"));     // the php_user_filter instance this node owns
    emitter.instruction("cbz x0, __rt_apply_chain_user_clear");                 // a node without an instance is inert
    emitter.instruction("ldr x1, [sp, #0]");                                    // buffer pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // current length
    emitter.instruction("bl __rt_apply_user_filter_obj");                       // x1/x2 = the filtered pair
    emitter.instruction("str x1, [sp, #0]");                                    // carry the possibly-relocated buffer
    emitter.instruction("str x2, [sp, #8]");                                    // and its length
    emitter.label("__rt_apply_chain_user_clear");
    abi::emit_symbol_address(emitter, "x10", "_user_filter_current_stream");
    emitter.instruction("str xzr, [x10]");                                      // the stream is the call's, not the instance's
    emitter.instruction("b __rt_apply_chain_loop");                             // continue down the chain

    emitter.label("__rt_apply_chain_done");
    emitter.instruction("ldr x1, [sp, #0]");                                    // restore the buffer pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // resulting length
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the chain-walk frame
    emitter.instruction("ret");                                                 // return the filtered buffer/length pair
}

/// x86_64 variant of [`emit_filter_apply_chain_aarch64`].
///
/// Input:  rdi = stream handle, rax = buffer, rdx = length, rsi = chain-head offset.
/// Output: rdx = resulting length; rax = buffer pointer.
fn emit_filter_apply_chain_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: apply a stream filter chain ---");
    emitter.label_global("__rt_stream_apply_filter_chain");
    // Frame: [-8]=buffer [-16]=length [-24]=node handle [-32]=head offset [-40]=filter id
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the chain-walk frame
    emitter.instruction("sub rsp, 48");                                         // reserve spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the buffer pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the current length
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // preserve the chain-head offset

    emitter.instruction("call __rt_stream_state");                              // resolve the owning stream state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_apply_chain_done_x");                          // no live stream: pass the buffer through
    emitter.instruction("add rax, QWORD PTR [rbp - 32]");                       // address of the chain head slot
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // head filter handle
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // preserve the walk cursor

    emitter.label("__rt_apply_chain_loop_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the current node handle
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_apply_chain_done_x");                          // end of chain
    emitter.instruction("mov rdi, r10");                                        // resolve the node state
    emitter.instruction("call __rt_filter_state");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_apply_chain_done_x");                          // a stale node ends the chain

    // -- advance the cursor before dispatching: the filter call clobbers scratch --
    emitter.instruction(&format!("mov r11, QWORD PTR [rax + {FILTER_NEXT_OFFSET}]")); // next node handle
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // store the updated cursor
    emitter.instruction(&format!("mov r11, QWORD PTR [rax + {FILTER_BUILTIN_ID_OFFSET}]")); // built-in filter id
    emitter.instruction("test r11, r11");
    emitter.instruction("jz __rt_apply_chain_user_x");                          // id 0 marks a user filter carried by this node

    emitter.instruction("mov rdi, rax");                                        // the node whose parameters apply
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // the filter id outlives the publish
    emitter.instruction("call __rt_asf_params_load");                           // the encoders read them from globals
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // buffer pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // current length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // built-in filter id
    emitter.instruction("call __rt_apply_stream_filter");                       // transform in place; rdx = new length
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // the new length outlives the clear
    // The published parameters are cleared immediately, so they are non-zero for exactly the
    // duration of one application. The legacy per-descriptor path (which still serves `zlib.*` and
    // `bzip2.*`) calls the same applier without a node, and a `line-length` left behind by an
    // unrelated chain would silently wrap ITS output.
    emitter.instruction("xor edi, edi");
    emitter.instruction("call __rt_asf_params_load");                           // clear them again
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // and carry the length onward
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // carry the new length to the next node
    emitter.instruction("jmp __rt_apply_chain_loop_x");                         // continue down the chain

    // See the AArch64 counterpart: a user filter runs from the node's own instance and
    // may answer with a different buffer, so both halves of the pair are carried.
    emitter.label("__rt_apply_chain_user_x");
    // See the AArch64 counterpart: the node knows which stream this call filters.
    emitter.instruction(&format!(
        "mov r9, QWORD PTR [rax + {FILTER_STREAM_HANDLE_OFFSET}]"
    ));
    abi::emit_symbol_address(emitter, "r10", "_user_filter_current_stream");
    emitter.instruction("mov QWORD PTR [r10], r9");
    emitter.instruction(&format!("mov rdi, QWORD PTR [rax + {FILTER_OBJECT_OFFSET}]")); // the php_user_filter instance this node owns
    emitter.instruction("test rdi, rdi");
    emitter.instruction("jz __rt_apply_chain_user_clear_x");                    // a node without an instance is inert
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // buffer pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // current length
    emitter.instruction("call __rt_apply_user_filter_obj");                     // rax/rdx = the filtered pair
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // carry the possibly-relocated buffer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // and its length
    emitter.label("__rt_apply_chain_user_clear_x");
    abi::emit_symbol_address(emitter, "r10", "_user_filter_current_stream");
    emitter.instruction("mov QWORD PTR [r10], 0");                              // the stream is the call's, not the instance's
    emitter.instruction("jmp __rt_apply_chain_loop_x");                         // continue down the chain

    emitter.label("__rt_apply_chain_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the buffer pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // resulting length
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return the filtered buffer/length pair
}

/// Stores the "no user filter ran" sentinel into `_user_filter_last_consumed` (AArch64).
///
/// `i64::MIN` is the sentinel because it is not a byte count any filter could plausibly claim,
/// and a plain 0 would be indistinguishable from the very case that matters: a user filter that
/// never assigned `&$consumed`, which php reports as `fwrite()` returning 0.
fn emit_arm_user_filter_consumed_sentinel_aarch64(emitter: &mut Emitter) {
    emitter.instruction("mov x9, #1");
    emitter.instruction("lsl x9, x9, #63");                                     // i64::MIN
    abi::emit_symbol_address(emitter, "x10", "_user_filter_last_consumed");
    emitter.instruction("str x9, [x10]");                                       // no user filter has published yet
}

/// x86_64 variant of [`emit_arm_user_filter_consumed_sentinel_aarch64`].
fn emit_arm_user_filter_consumed_sentinel_x86_64(emitter: &mut Emitter) {
    emitter.instruction("mov r10, 1");
    emitter.instruction("shl r10, 63");                                         // i64::MIN
    abi::emit_symbol_address(emitter, "r11", "_user_filter_last_consumed");
    emitter.instruction("mov QWORD PTR [r11], r10");                            // no user filter has published yet
}

/// `__rt_fwrite_filtered(stream, ptr, len) -> written` (AArch64).
///
/// Applies the write chain to a private copy of the payload and hands the result
/// to `__rt_fwrite`. The copy matters: the payload is a PHP string that may be
/// shared, so the in-place filters must never touch it.
///
/// A stream with an empty write chain skips the copy entirely and tail-calls
/// `__rt_fwrite`, so unfiltered writes keep their original cost.
///
/// The scratch is sized `2 * len + 64` because the length-changing built-ins
/// (`convert.base64-encode`, `convert.quoted-printable-encode`) expand their
/// input; the old path instead capped the payload at a 64 KiB static buffer and
/// silently wrote oversized payloads unfiltered.
///
/// Returns the number of payload bytes consumed, which is what PHP's `fwrite()`
/// reports for a filtered stream — not the post-filter byte count.
fn emit_fwrite_filtered_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fwrite through the write filter chain ---");
    emitter.label_global("__rt_fwrite_filtered");
    // Frame: [0]=stream handle [8]=payload ptr [16]=payload len [24]=scratch
    //        [32]=filtered len [40]=filtered buffer
    emitter.instruction("sub sp, sp, #64");                                     // reserve the filtered-write frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the opaque stream handle
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the payload pointer
    emitter.instruction("str x2, [sp, #16]");                                   // preserve the payload length

    // -- fast path: no write filters attached --
    emitter.instruction("bl __rt_stream_state");                                // resolve the owning stream state
    emitter.instruction("cbz x0, __rt_fwrite_filtered_direct");                 // no live stream: let __rt_fwrite report the error
    emitter.instruction(&format!(
        "ldr x9, [x0, #{STREAM_WRITE_FILTER_HEAD_OFFSET}]"
    ));                                                                         // write-chain head
    emitter.instruction("cbz x9, __rt_fwrite_filtered_direct");                 // empty chain: write the payload untouched

    // -- allocate a private, growth-tolerant copy of the payload --
    emitter.instruction("ldr x2, [sp, #16]");                                   // payload length
    emitter.instruction("lsl x0, x2, #1");                                      // 2 * len leaves room for expanding filters
    emitter.instruction("add x0, x0, #64");                                     // plus a small fixed margin
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = scratch buffer
    emitter.instruction("cbz x0, __rt_fwrite_filtered_direct");                 // out of heap: fall back to an unfiltered write
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the scratch pointer

    emitter.instruction("ldr x1, [sp, #8]");                                    // payload pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // payload length
    emitter.instruction("mov x3, #0");                                          // copy cursor
    emitter.label("__rt_fwrite_filtered_copy");
    emitter.instruction("cmp x3, x2");                                          // copied every byte?
    emitter.instruction("b.ge __rt_fwrite_filtered_copied");
    emitter.instruction("ldrb w4, [x1, x3]");                                   // load one payload byte
    emitter.instruction("strb w4, [x0, x3]");                                   // store it into the scratch
    emitter.instruction("add x3, x3, #1");                                      // advance the cursor
    emitter.instruction("b __rt_fwrite_filtered_copy");
    emitter.label("__rt_fwrite_filtered_copied");

    // -- arm the "no user filter ran" sentinel before the chain --
    // A user filter's `filter()` publishes whatever it left in `&$consumed`; a chain of built-ins
    // publishes nothing, and php reports the payload length for those. The sentinel is what tells
    // the two apart on the way out.
    emit_arm_user_filter_consumed_sentinel_aarch64(emitter);

    // -- run the chain over the copy --
    emitter.instruction("ldr x0, [sp, #0]");                                    // stream handle
    emitter.instruction("ldr x1, [sp, #24]");                                   // scratch buffer
    emitter.instruction("ldr x2, [sp, #16]");                                   // payload length
    emitter.instruction(&format!("mov x3, #{STREAM_WRITE_FILTER_HEAD_OFFSET}")); // select the write chain
    emitter.instruction("bl __rt_stream_apply_filter_chain");                   // x1/x2 <- filtered buffer and length

    // -- write the filtered bytes through the regular descriptor path --
    // Both halves of the returned pair matter. A built-in node rewrites the scratch
    // in place, but a user filter answers with the string its `filter()` returned,
    // which lives elsewhere — reading the scratch back would write the raw bytes at
    // the filtered length.
    emitter.instruction("str x1, [sp, #40]");                                   // stash the buffer the chain settled on
    emitter.instruction("str x2, [sp, #32]");                                   // stash its length across the write
    emitter.instruction("ldr x0, [sp, #0]");                                    // stream handle: __rt_fwrite resolves the descriptor itself
    emitter.instruction("ldr x1, [sp, #40]");                                   // filtered buffer
    emitter.instruction("ldr x2, [sp, #32]");                                   // filtered length
    emitter.instruction("bl __rt_fwrite");                                      // perform the descriptor write

    // -- release the scratch and report the consumed payload length --
    emitter.instruction("ldr x0, [sp, #24]");                                   // scratch pointer
    emitter.instruction("bl __rt_heap_free");                                   // the copy never escapes this helper
    emitter.instruction("ldr x0, [sp, #16]");                                   // PHP reports payload bytes consumed
    // ...for a chain of built-ins. A USER filter reports whatever it left in `&$consumed`:
    // measured on php 8.5.6, a filter that never assigns the parameter makes `fwrite()` answer 0
    // even though the bytes reached the file, one that adds a wrong number answers that number,
    // and a negative leaves `fwrite()` false.
    abi::emit_symbol_address(emitter, "x9", "_user_filter_last_consumed");
    emitter.instruction("ldr x10, [x9]");                                       // what the chain published, if anything
    emitter.instruction("mov x11, #1");
    emitter.instruction("lsl x11, x11, #63");                                   // the untouched "no user filter ran" sentinel
    emitter.instruction("cmp x10, x11");
    emitter.instruction("b.eq __rt_fwrite_filtered_ret");                       // built-ins only: keep the payload length
    emitter.instruction("mov x0, x10");                                         // the count the user filter claimed
    emitter.instruction("cmp x0, #0");
    emitter.instruction("b.ge __rt_fwrite_filtered_ret");
    emitter.instruction("mov x0, #-1");                                         // a negative claim is a failure, like php
    emitter.label("__rt_fwrite_filtered_ret");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the filtered-write frame
    emitter.instruction("ret");                                                 // return the consumed byte count

    emitter.label("__rt_fwrite_filtered_direct");
    emitter.instruction("ldr x0, [sp, #0]");                                    // stream handle: __rt_fwrite resolves the descriptor itself
    emitter.instruction("ldr x1, [sp, #8]");                                    // original payload pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // original payload length
    emitter.instruction("bl __rt_fwrite");                                      // unfiltered descriptor write
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the filtered-write frame
    emitter.instruction("ret");                                                 // return __rt_fwrite's byte count
}

/// x86_64 variant of [`emit_fwrite_filtered_aarch64`].
///
/// Input:  rdi = stream handle, rsi = payload pointer, rdx = payload length.
/// Output: rax = payload bytes consumed.
fn emit_fwrite_filtered_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fwrite through the write filter chain ---");
    emitter.label_global("__rt_fwrite_filtered");
    // Frame: [-8]=stream handle [-16]=payload ptr [-24]=payload len
    //        [-32]=scratch [-40]=filtered len [-48]=filtered buffer
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the filtered-write frame
    emitter.instruction("sub rsp, 48");                                         // reserve spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the opaque stream handle
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the payload pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the payload length

    // -- fast path: no write filters attached --
    emitter.instruction("call __rt_stream_state");                              // resolve the owning stream state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fwrite_filtered_direct_x");                    // no live stream: let __rt_fwrite report the error
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {STREAM_WRITE_FILTER_HEAD_OFFSET}]"
    ));                                                                         // write-chain head
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_fwrite_filtered_direct_x");                    // empty chain: write the payload untouched

    // -- allocate a private, growth-tolerant copy of the payload --
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // payload length
    emitter.instruction("shl rax, 1");                                          // 2 * len leaves room for expanding filters
    emitter.instruction("add rax, 64");                                         // plus a small fixed margin
    emitter.instruction("call __rt_heap_alloc");                                // rax = scratch buffer
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fwrite_filtered_direct_x");                    // out of heap: fall back to an unfiltered write
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the scratch pointer

    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // payload pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // payload length
    emitter.instruction("xor rcx, rcx");                                        // copy cursor
    emitter.label("__rt_fwrite_filtered_copy_x");
    emitter.instruction("cmp rcx, rdx");                                        // copied every byte?
    emitter.instruction("jge __rt_fwrite_filtered_copied_x");
    emitter.instruction("mov r8b, BYTE PTR [rsi + rcx]");                       // load one payload byte
    emitter.instruction("mov BYTE PTR [rax + rcx], r8b");                       // store it into the scratch
    emitter.instruction("add rcx, 1");                                          // advance the cursor
    emitter.instruction("jmp __rt_fwrite_filtered_copy_x");
    emitter.label("__rt_fwrite_filtered_copied_x");

    // -- arm the "no user filter ran" sentinel before the chain --
    // See the AArch64 counterpart: it is what tells a user filter's published `&$consumed` apart
    // from a chain of built-ins, which publishes nothing.
    emit_arm_user_filter_consumed_sentinel_x86_64(emitter);

    // -- run the chain over the copy --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // stream handle
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // scratch buffer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // payload length
    emitter.instruction(&format!("mov rsi, {STREAM_WRITE_FILTER_HEAD_OFFSET}")); // select the write chain
    emitter.instruction("call __rt_stream_apply_filter_chain");                 // rax/rdx <- filtered buffer and length
    // See the AArch64 counterpart: the pointer matters as much as the length, because
    // a user filter answers with its own string rather than the rewritten scratch.
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // stash the buffer the chain settled on
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // stash the filtered length

    // -- write the filtered bytes through the regular descriptor path --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // stream handle: __rt_fwrite resolves the descriptor itself
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // filtered buffer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // filtered length
    emitter.instruction("call __rt_fwrite");                                    // perform the descriptor write

    // -- release the scratch and report the consumed payload length --
    // __rt_heap_free takes its pointer in rax on x86_64, not rdi — same convention as
    // __rt_heap_alloc a few lines above. Passing rdi left rax holding __rt_fwrite's return
    // value, so this freed the BYTE COUNT as if it were an address.
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // scratch pointer
    emitter.instruction("call __rt_heap_free");                                 // the copy never escapes this helper
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // PHP reports payload bytes consumed
    // ...for a chain of built-ins; a USER filter reports its `&$consumed` instead. See the
    // AArch64 counterpart for the measurement.
    abi::emit_symbol_address(emitter, "r10", "_user_filter_last_consumed");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // what the chain published, if anything
    emitter.instruction("mov r10, 1");
    emitter.instruction("shl r10, 63");                                         // the untouched "no user filter ran" sentinel
    emitter.instruction("cmp r11, r10");
    emitter.instruction("je __rt_fwrite_filtered_ret_x");                       // built-ins only: keep the payload length
    emitter.instruction("mov rax, r11");                                        // the count the user filter claimed
    emitter.instruction("cmp rax, 0");
    emitter.instruction("jge __rt_fwrite_filtered_ret_x");
    emitter.instruction("mov rax, -1");                                         // a negative claim is a failure, like php
    emitter.label("__rt_fwrite_filtered_ret_x");
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return the consumed byte count

    emitter.label("__rt_fwrite_filtered_direct_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // stream handle: __rt_fwrite resolves the descriptor itself
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // original payload pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // original payload length
    emitter.instruction("call __rt_fwrite");                                    // unfiltered descriptor write
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return __rt_fwrite's byte count
}

/// x86_64 variant of [`emit_filter_state_aarch64`].
fn emit_filter_state_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve an opaque stream-filter state ---");
    emitter.label_global("__rt_filter_state");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable lookup frame
    emitter.instruction("call __rt_resource_lookup_any");                       // validate and resolve the opaque handle
    emitter.instruction("test rax, rax");                                       // did the handle resolve?
    emitter.instruction("jz __rt_filter_state_fail");                           // reject invalid or stale resources
    emitter.instruction(&format!("mov r10, QWORD PTR [rax + {SLOT_KIND_OFFSET}]")); // registry resource kind
    emitter.instruction(&format!("cmp r10, {RESOURCE_KIND_FILTER}"));           // is the slot a stream filter?
    emitter.instruction("jne __rt_filter_state_fail");                          // reject streams, contexts, and other resources
    emitter.instruction(&format!("mov r10, QWORD PTR [rax + {SLOT_STATUS_OFFSET}]")); // lifecycle state
    emitter.instruction(&format!("cmp r10, {RESOURCE_STATUS_LIVE}"));           // only Live filters expose their state
    emitter.instruction("jne __rt_filter_state_fail");                          // reject Closing and Closed filters
    emitter.instruction(&format!("mov rax, QWORD PTR [rax + {SLOT_STATE_PTR_OFFSET}]")); // stable filter-state pointer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the filter-state pointer
    emitter.label("__rt_filter_state_fail");
    emitter.instruction("xor eax, eax");                                        // return null for invalid filter resources
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}

/// x86_64 variant of [`emit_filter_create_aarch64`].
fn emit_filter_create_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: create a stream-filter resource ---");
    emitter.label_global("__rt_filter_create");
    // Frame: [-8]=builtin id [-16]=obj [-24]=direction [-32]=params [-40]=state ptr
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the creation frame
    emitter.instruction("sub rsp, 48");                                         // reserve spill slots (keeps rsp 16-byte aligned)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the built-in filter id
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the user-filter object
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the direction bits
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // preserve the retained params value

    emitter.instruction(&format!("mov rax, {FILTER_STATE_SIZE}"));              // FilterState payload size
    emitter.instruction("call __rt_heap_alloc");                                // allocate the stable filter state
    emitter.instruction("test rax, rax");                                       // heap exhausted?
    emitter.instruction("jz __rt_filter_create_fail");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // spill the state pointer

    // -- zero the payload so both chain links start empty --
    emitter.instruction("xor rcx, rcx");                                        // byte cursor
    emitter.label("__rt_filter_create_zero");
    emitter.instruction(&format!("cmp rcx, {FILTER_STATE_SIZE}"));              // cleared every byte?
    emitter.instruction("jge __rt_filter_create_zeroed");
    emitter.instruction("mov QWORD PTR [rax + rcx], 0");                        // clear one word
    emitter.instruction("add rcx, 8");                                          // advance the cursor
    emitter.instruction("jmp __rt_filter_create_zero");
    emitter.label("__rt_filter_create_zeroed");

    // -- seed the descriptor fields --
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
    emitter.instruction(&format!("mov QWORD PTR [rax + {FILTER_BUILTIN_ID_OFFSET}], r10")); // built-in filter id
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");
    emitter.instruction(&format!("mov QWORD PTR [rax + {FILTER_OBJECT_OFFSET}], r10")); // php_user_filter instance
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");
    emitter.instruction(&format!("mov QWORD PTR [rax + {FILTER_DIRECTION_OFFSET}], r10")); // direction bits
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");
    emitter.instruction(&format!("mov QWORD PTR [rax + {FILTER_PARAMS_OFFSET}], r10")); // retained params value

    // -- read the built-in filter parameters out of `$params`, ONCE --
    //
    // php parses `$params` in each filter's `create` callback, not per buffer, and for the same
    // reason: `line-length`, `line-break-chars` and `binary` cannot change once the filter is
    // attached, so probing the hash on every buffer would put a lookup in the encoder's inner
    // path for a constant.
    emitter.instruction("mov rdi, rax");                                        // the node it fills
    emitter.instruction("call __rt_filter_absorb_params");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_filter_create_bad_params_x86");                // php refuses the attach outright
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the state the absorber consumed

    // -- register the filter as an owned resource --
    emitter.instruction("mov rsi, rax");                                        // stable state pointer
    emitter.instruction(&format!("mov rdi, {RESOURCE_KIND_FILTER}"));           // resource kind
    emitter.instruction("mov rdx, 1");                                          // RESOURCE_FLAG_OWNS_STATE
    emitter.instruction("call __rt_resource_alloc");                            // rax = opaque filter handle (0 on failure)
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return the filter handle

    emitter.label("__rt_filter_create_bad_params_x86");
    // See the AArch64 counterpart: the state was allocated before the parameters could be read.
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // the state nobody will own
    emitter.instruction("call __rt_heap_free");                                 // takes its pointer in rax, not rdi
    emitter.label("__rt_filter_create_fail");
    emitter.instruction("xor eax, eax");                                        // report allocation failure
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return to the caller
}

/// x86_64 variant of [`emit_filter_link_aarch64`].
fn emit_filter_link_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: link a filter into a stream chain ---");
    emitter.label_global("__rt_stream_filter_link");
    // Frame: [-8]=stream handle [-16]=filter handle [-24]=head offset
    //        [-32]=prepend [-40]=filter state [-48]=stream state
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the link frame
    emitter.instruction("sub rsp, 64");                                         // reserve spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the owning stream handle
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the filter handle
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the chain-head byte offset
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // preserve the prepend flag

    emitter.instruction("mov rdi, rsi");                                        // resolve the filter state
    emitter.instruction("call __rt_filter_state");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_filter_link_fail");                            // reject a dead filter handle
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // spill the filter state
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // resolve the owning stream state
    emitter.instruction("call __rt_stream_state");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_filter_link_fail");                            // reject a dead stream handle
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // spill the stream state

    // -- record the owner so removal can find this chain again --
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // filter state
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // owning stream handle
    emitter.instruction(&format!("mov QWORD PTR [r9 + {FILTER_STREAM_HANDLE_OFFSET}], r10"));

    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // chain-head offset
    emitter.instruction("add r11, QWORD PTR [rbp - 48]");                       // address of the chain head slot
    emitter.instruction("mov r8, QWORD PTR [r11]");                             // current head filter handle
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // prepend flag
    emitter.instruction("test rcx, rcx");
    emitter.instruction("jz __rt_filter_link_append_x");                        // append is the stream_filter_append path

    // -- prepend: the new node becomes the head --
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // the new filter handle
    emitter.instruction(&format!("mov QWORD PTR [r9 + {FILTER_NEXT_OFFSET}], r8")); // new->next = old head
    emitter.instruction(&format!("mov QWORD PTR [r9 + {FILTER_PREV_OFFSET}], 0")); // new->prev = none
    emitter.instruction("mov QWORD PTR [r11], rdx");                            // head = new node
    emitter.instruction("test r8, r8");
    emitter.instruction("jz __rt_filter_link_ok_x");                            // empty chain: nothing to back-link
    emitter.instruction("mov rdi, r8");                                         // resolve the previous head
    emitter.instruction("call __rt_filter_state");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_filter_link_ok_x");                            // a stale head simply ends the chain
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the new filter handle
    emitter.instruction(&format!("mov QWORD PTR [rax + {FILTER_PREV_OFFSET}], rdx")); // old head->prev = new node
    emitter.instruction("jmp __rt_filter_link_ok_x");

    // -- append: walk to the tail and link there --
    emitter.label("__rt_filter_link_append_x");
    emitter.instruction("test r8, r8");
    emitter.instruction("jnz __rt_filter_link_walk_x");                         // non-empty chain: find the tail
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // the new filter handle
    emitter.instruction("mov QWORD PTR [r11], rdx");                            // empty chain: the node is the head
    emitter.instruction("jmp __rt_filter_link_ok_x");

    emitter.label("__rt_filter_link_walk_x");
    emitter.instruction("mov QWORD PTR [rbp - 56], r8");                        // spill the current node handle
    emitter.instruction("mov rdi, r8");                                         // resolve the current node
    emitter.instruction("call __rt_filter_state");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_filter_link_fail");                            // a broken chain cannot be extended
    emitter.instruction(&format!("mov r8, QWORD PTR [rax + {FILTER_NEXT_OFFSET}]")); // next handle
    emitter.instruction("test r8, r8");
    emitter.instruction("jz __rt_filter_link_tail_x");                          // reached the tail
    emitter.instruction("jmp __rt_filter_link_walk_x");                         // advance to the next node

    emitter.label("__rt_filter_link_tail_x");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // the new filter handle
    emitter.instruction(&format!("mov QWORD PTR [rax + {FILTER_NEXT_OFFSET}], rdx")); // tail->next = new node
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the new filter state
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // the old tail handle
    emitter.instruction(&format!("mov QWORD PTR [r9 + {FILTER_PREV_OFFSET}], r10")); // new->prev = old tail
    emitter.instruction(&format!("mov QWORD PTR [r9 + {FILTER_NEXT_OFFSET}], 0")); // new->next = none

    emitter.label("__rt_filter_link_ok_x");
    emitter.instruction("mov eax, 1");                                          // report success
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_filter_link_fail");
    emitter.instruction("xor eax, eax");                                        // report failure
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return to the caller
}

/// x86_64 variant of [`emit_filter_unlink_aarch64`].
fn emit_filter_unlink_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unlink a filter from a stream chain ---");
    emitter.label_global("__rt_stream_filter_unlink");
    // Frame: [-8]=filter handle [-16]=head offset [-24]=prev [-32]=next [-40]=filter state
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the unlink frame
    emitter.instruction("sub rsp, 48");                                         // reserve spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the filter handle
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the chain-head byte offset

    emitter.instruction("call __rt_filter_state");                              // resolve the filter state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_filter_unlink_fail");                          // nothing to unlink
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // spill the filter state
    emitter.instruction(&format!("mov r10, QWORD PTR [rax + {FILTER_PREV_OFFSET}]")); // previous node handle
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");
    emitter.instruction(&format!("mov r11, QWORD PTR [rax + {FILTER_NEXT_OFFSET}]")); // next node handle
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");

    // -- prev->next = next, or move the chain head when this node was first --
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_filter_unlink_was_head_x");                    // no predecessor: this node was the head
    emitter.instruction("mov rdi, r10");                                        // resolve the predecessor
    emitter.instruction("call __rt_filter_state");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_filter_unlink_next_x");                        // a stale predecessor needs no repair
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // successor handle
    emitter.instruction(&format!("mov QWORD PTR [rax + {FILTER_NEXT_OFFSET}], r11")); // prev->next = next
    emitter.instruction("jmp __rt_filter_unlink_next_x");

    emitter.label("__rt_filter_unlink_was_head_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // the removed filter state
    emitter.instruction(&format!("mov rdi, QWORD PTR [rax + {FILTER_STREAM_HANDLE_OFFSET}]")); // owning stream handle
    emitter.instruction("test rdi, rdi");
    emitter.instruction("jz __rt_filter_unlink_next_x");                        // detached node: no chain to repair
    emitter.instruction("call __rt_stream_state");                              // resolve the owning stream state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_filter_unlink_next_x");                        // the stream is gone: nothing to repair
    emitter.instruction("add rax, QWORD PTR [rbp - 16]");                       // address of the chain head slot
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // successor handle
    emitter.instruction("mov QWORD PTR [rax], r11");                            // head = next

    // -- next->prev = prev --
    emitter.label("__rt_filter_unlink_next_x");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");
    emitter.instruction("test r11, r11");
    emitter.instruction("jz __rt_filter_unlink_ok_x");                          // no successor to repair
    emitter.instruction("mov rdi, r11");                                        // resolve the successor
    emitter.instruction("call __rt_filter_state");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_filter_unlink_ok_x");                          // a stale successor needs no repair
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // predecessor handle
    emitter.instruction(&format!("mov QWORD PTR [rax + {FILTER_PREV_OFFSET}], r10")); // next->prev = prev

    emitter.label("__rt_filter_unlink_ok_x");
    // -- isolate the removed node so a double removal is inert --
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");
    emitter.instruction(&format!("mov QWORD PTR [rax + {FILTER_NEXT_OFFSET}], 0"));
    emitter.instruction(&format!("mov QWORD PTR [rax + {FILTER_PREV_OFFSET}], 0"));
    emitter.instruction(&format!("mov QWORD PTR [rax + {FILTER_STREAM_HANDLE_OFFSET}], 0"));
    emitter.instruction("mov eax, 1");                                          // report success
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_filter_unlink_fail");
    emitter.instruction("xor eax, eax");                                        // report failure
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return to the caller
}

/// Silences unused-constant warnings for the lifecycle fields consumed by the
/// `onClose` step, which lands with the chain-application rewiring.
const _: () = {
    let _ = FILTER_FLAGS_OFFSET;
    let _ = crate::codegen_support::runtime::resources::layout::FILTER_FLAG_ONCLOSE_CALLED;
};

/// `__rt_stream_close_filter_chains(state)` closes every filter still attached.
///
/// PHP invalidates a filter resource when its stream closes, so this runs from
/// the stream-state destructor and therefore covers both `fclose()` and a
/// release driven by scope exit.
///
/// The successor is read before the node is closed: closing may free the state.
/// A filter attached with `STREAM_FILTER_ALL` sits in both chains, so it is
/// visited twice; `__rt_resource_mark_closed` reports already-closed resources
/// as a no-op, which makes the second visit inert.
/// `__rt_filter_node_closing_flush(handle) -> PSFS code`: run PHP's closing flush.
///
/// Removing a filter gives it one last `filter(..., $closing = true)` call. Answering
/// `PSFS_ERR_FATAL` there means the filter refuses to be flushed, and PHP then reports
/// `stream_filter_remove()` as false and LEAVES THE FILTER ATTACHED — which is why the
/// code has to reach the caller rather than being swallowed like a read/write result.
///
/// Nodes with nothing to flush — a built-in, a class without `filter()`, or the simple
/// `filter(string)` form, which has no closing dispatch — answer `PSFS_PASS_ON`.
///
/// The bytes the flush produces are WRITTEN BACK to the node's own stream when the node sits on
/// the write chain. php's `php_stream_filter_remove(…, call_dtor)` hands the flushed buckets to
/// the stream rather than dropping them, and a filter that accumulated until `$closing` has all
/// of its payload in exactly that call: `stream_filter_remove()` on a buffering write filter
/// leaves php's file holding `<xy>` where elephc's stayed EMPTY — measured with `php -n` (8.5.6).
fn emit_filter_node_closing_flush(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: closing flush for one filter node ---");
    emitter.label_global("__rt_filter_node_closing_flush");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("push rbp");                                        // preserve the caller frame pointer
        emitter.instruction("mov rbp, rsp");                                    // establish the helper frame
        emitter.instruction("sub rsp, 48");                                     // obj, method, node handle, direction, flushed pair
        emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                   // preserve the node handle for the write-back
        emitter.instruction("call __rt_filter_state");                          // rax = FilterState or 0
        emitter.instruction("test rax, rax");
        emitter.instruction("jz __rt_fncf_pass_x");                             // a stale node has nothing to flush
        emitter.instruction(&format!(
            "mov r10, QWORD PTR [rax + {FILTER_DIRECTION_OFFSET}]"
        ));                                                                     // which chain(s) this node sits on
        emitter.instruction("mov QWORD PTR [rbp - 32], r10");                   // only a write-side node writes its flush back
        emitter.instruction(&format!(
            "mov rdi, QWORD PTR [rax + {FILTER_OBJECT_OFFSET}]"
        ));                                                                     // the php_user_filter instance
        emitter.instruction("test rdi, rdi");
        emitter.instruction("jz __rt_fncf_pass_x");                             // a built-in node has nothing to flush
        emitter.instruction("mov r11, QWORD PTR [rdi]");                        // class_id at the obj head
        abi::emit_symbol_address(emitter, "r10", "_user_filter_vtable_ptrs");
        emitter.instruction("mov r10, QWORD PTR [r10 + r11 * 8]");              // per-class user-filter vtable
        emitter.instruction("mov r11, QWORD PTR [r10]");                        // slot 0 = filter() method pointer
        emitter.instruction("test r11, r11");
        emitter.instruction("jz __rt_fncf_pass_x");                             // class never implemented filter()
        emitter.instruction("mov rax, QWORD PTR [r10 + 24]");                   // slot 3 = brigade-arity flag
        emitter.instruction("test rax, rax");
        emitter.instruction("jz __rt_fncf_pass_x");                             // the simple string form takes no $closing
        emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                    // preserve the instance
        emitter.instruction("mov QWORD PTR [rbp - 16], r11");                   // preserve the method pointer
        abi::emit_symbol_address(emitter, "r10", "_user_filter_closing");
        emitter.instruction("mov QWORD PTR [r10], 1");                          // this dispatch is the closing one
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // $this
        abi::emit_symbol_address(emitter, "rsi", "_stream_filter_buf");         // an empty input: the flush feeds no bytes
        emitter.instruction("xor edx, edx");                                    // length 0
        emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                   // method pointer
        emitter.instruction("call __rt_user_filter_brigade_invoke");
        emitter.instruction("mov QWORD PTR [rbp - 40], rax");                   // preserve the flushed payload pointer
        emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                   // preserve the flushed payload length
        abi::emit_symbol_address(emitter, "r10", "_user_filter_closing");
        emitter.instruction("mov QWORD PTR [r10], 0");                          // lower the flag again immediately
        emit_filter_node_flush_write_back_x86_64(emitter);
        abi::emit_symbol_address(emitter, "r10", "_user_filter_last_psfs");
        emitter.instruction("mov rax, QWORD PTR [r10]");                        // the code filter() answered with
        emitter.instruction("jmp __rt_fncf_done_x");
        emitter.label("__rt_fncf_pass_x");
        emitter.instruction("mov rax, 2");                                      // PSFS_PASS_ON: nothing refused the flush
        emitter.label("__rt_fncf_done_x");
        emitter.instruction("leave");                                           // restore rbp + rsp
        emitter.instruction("ret");                                             // return the PSFS code
        return;
    }
    emitter.instruction("sub sp, sp, #48");                                     // helper frame: node handle, direction, flushed pair
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the node handle for the write-back
    emitter.instruction("bl __rt_filter_state");                                // x0 = FilterState or 0
    emitter.instruction("cbz x0, __rt_fncf_pass");                              // a stale node has nothing to flush
    emitter.instruction(&format!("ldr x9, [x0, #{FILTER_DIRECTION_OFFSET}]"));  // which chain(s) this node sits on
    emitter.instruction("str x9, [sp, #8]");                                    // only a write-side node writes its flush back
    emitter.instruction(&format!("ldr x0, [x0, #{FILTER_OBJECT_OFFSET}]"));     // the php_user_filter instance
    emitter.instruction("cbz x0, __rt_fncf_pass");                              // a built-in node has nothing to flush
    emitter.instruction("ldr x6, [x0]");                                        // class_id at the obj head
    abi::emit_symbol_address(emitter, "x7", "_user_filter_vtable_ptrs");
    emitter.instruction("ldr x7, [x7, x6, lsl #3]");                            // per-class user-filter vtable
    emitter.instruction("ldr x8, [x7]");                                        // slot 0 = filter() method pointer
    emitter.instruction("cbz x8, __rt_fncf_pass");                              // class never implemented filter()
    emitter.instruction("ldr x9, [x7, #24]");                                   // slot 3 = brigade-arity flag
    emitter.instruction("cbz x9, __rt_fncf_pass");                              // the simple string form takes no $closing
    abi::emit_symbol_address(emitter, "x10", "_user_filter_closing");
    emitter.instruction("mov x11, #1");
    emitter.instruction("str x11, [x10]");                                      // this dispatch is the closing one
    abi::emit_symbol_address(emitter, "x1", "_stream_filter_buf");              // an empty input: the flush feeds no bytes
    emitter.instruction("mov x2, #0");                                          // length 0
    emitter.instruction("mov x3, x8");                                          // method pointer
    emitter.instruction("bl __rt_user_filter_brigade_invoke");                  // x0 still holds $this
    emitter.instruction("str x1, [sp, #16]");                                   // preserve the flushed payload pointer
    emitter.instruction("str x2, [sp, #24]");                                   // preserve the flushed payload length
    abi::emit_symbol_address(emitter, "x10", "_user_filter_closing");
    emitter.instruction("str xzr, [x10]");                                      // lower the flag again immediately
    emit_filter_node_flush_write_back_aarch64(emitter);
    abi::emit_symbol_address(emitter, "x9", "_user_filter_last_psfs");
    emitter.instruction("ldr x0, [x9]");                                        // the code filter() answered with
    emitter.instruction("b __rt_fncf_done");
    emitter.label("__rt_fncf_pass");
    emitter.instruction("mov x0, #2");                                          // PSFS_PASS_ON: nothing refused the flush
    emitter.label("__rt_fncf_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the PSFS code
}

/// Writes a node's closing-flush bytes to its own stream (AArch64).
///
/// Reads the node handle from `[sp, #0]`, the direction bits from `[sp, #8]` and the flushed pair
/// from `[sp, #16]`/`[sp, #24]`. Only a node on the WRITE chain writes: a read-side flush feeds
/// php's read buffer, never the file. Synthetic descriptors (user wrappers, phar writes) are
/// skipped — they are not descriptors the write syscall can reach.
fn emit_filter_node_flush_write_back_aarch64(emitter: &mut Emitter) {
    emitter.instruction("ldr x2, [sp, #24]");                                   // the flushed payload length
    emitter.instruction("cbz x2, __rt_fncf_no_writeback");                      // PSFS_FEED_ME and an empty flush write nothing
    emitter.instruction("ldr x9, [sp, #8]");                                    // the node's direction bits
    emitter.instruction("tst x9, #2");                                          // STREAM_FILTER_WRITE
    emitter.instruction("b.eq __rt_fncf_no_writeback");                         // a read-side flush never reaches the file
    emitter.instruction("ldr x0, [sp, #0]");                                    // the node handle
    emitter.instruction("bl __rt_filter_state");                                // re-resolve: the dispatch clobbered everything
    emitter.instruction("cbz x0, __rt_fncf_no_writeback");                      // the node died during its own flush
    emitter.instruction(&format!("ldr x0, [x0, #{FILTER_STREAM_HANDLE_OFFSET}]")); // the stream this node is attached to
    emitter.instruction("cbz x0, __rt_fncf_no_writeback");                      // a detached node has nowhere to write
    emitter.instruction("bl __rt_stream_fd");                                   // x0 = the backend descriptor
    emitter.instruction("cmp x0, #0");
    emitter.instruction("b.lt __rt_fncf_no_writeback");                         // a failed open left no descriptor
    emitter.instruction("mov w9, #0x4000");                                     // high half of the first synthetic descriptor
    emitter.instruction("lsl w9, w9, #16");                                     // form the synthetic descriptor base
    emitter.instruction("cmp x0, x9");
    emitter.instruction("b.ge __rt_fncf_no_writeback");                         // synthetic descriptors own their own write path
    emitter.instruction("ldr x1, [sp, #16]");                                   // the flushed payload pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // the flushed payload length
    emitter.syscall(4);                                                         // write(fd, ptr, len)
    emitter.label("__rt_fncf_no_writeback");
}

/// x86_64 counterpart of [`emit_filter_node_flush_write_back_aarch64`].
///
/// Reads the node handle from `[rbp - 24]`, the direction bits from `[rbp - 32]` and the flushed
/// pair from `[rbp - 40]`/`[rbp - 48]`.
fn emit_filter_node_flush_write_back_x86_64(emitter: &mut Emitter) {
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // the flushed payload length
    emitter.instruction("test rdx, rdx");
    emitter.instruction("jz __rt_fncf_no_writeback_x");                         // PSFS_FEED_ME and an empty flush write nothing
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // the node's direction bits
    emitter.instruction("test r10, 2");                                         // STREAM_FILTER_WRITE
    emitter.instruction("jz __rt_fncf_no_writeback_x");                         // a read-side flush never reaches the file
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // the node handle
    emitter.instruction("call __rt_filter_state");                              // re-resolve: the dispatch clobbered everything
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fncf_no_writeback_x");                         // the node died during its own flush
    emitter.instruction(&format!(
        "mov rdi, QWORD PTR [rax + {FILTER_STREAM_HANDLE_OFFSET}]"
    ));                                                                         // the stream this node is attached to
    emitter.instruction("test rdi, rdi");
    emitter.instruction("jz __rt_fncf_no_writeback_x");                         // a detached node has nowhere to write
    emitter.instruction("call __rt_stream_fd");                                 // rax = the backend descriptor
    emitter.instruction("cmp rax, 0");
    emitter.instruction("jl __rt_fncf_no_writeback_x");                         // a failed open left no descriptor
    emitter.instruction("mov r10, 0x40000000");                                 // the first synthetic descriptor value
    emitter.instruction("cmp rax, r10");
    emitter.instruction("jge __rt_fncf_no_writeback_x");                        // synthetic descriptors own their own write path
    emitter.instruction("mov rdi, rax");                                        // the descriptor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // the flushed payload pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // the flushed payload length
    emitter.instruction("call write");                                          // write(fd, ptr, len) through libc
    emitter.label("__rt_fncf_no_writeback_x");
}

/// `__rt_filter_node_close_obj(handle)`: fire `onClose()` for one chain node.
///
/// `stream_filter_remove()` reaches a node by handle rather than by walking a chain,
/// so it needs the same "claim the instance, then close it exactly once" step the
/// chain teardown performs inline. Clearing the slot first is what makes it once-only:
/// removing a filter and later closing its stream must not fire `onClose()` twice.
pub fn emit_filter_node_close_obj(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: close one filter node's user instance ---");
    emitter.label_global("__rt_filter_node_close_obj");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("push rbp");                                        // preserve the caller frame pointer
        emitter.instruction("mov rbp, rsp");                                    // establish the helper frame
        emitter.instruction("call __rt_filter_state");                          // rax = FilterState or 0
        emitter.instruction("test rax, rax");
        emitter.instruction("jz __rt_fnco_done_x");                             // a stale handle owns no instance
        emitter.instruction(&format!(
            "mov rdi, QWORD PTR [rax + {FILTER_OBJECT_OFFSET}]"
        ));                                                                     // the php_user_filter instance, if any
        emitter.instruction("test rdi, rdi");
        emitter.instruction("jz __rt_fnco_done_x");                             // a built-in node carries none
        emitter.instruction(&format!(
            "mov QWORD PTR [rax + {FILTER_OBJECT_OFFSET}], 0"
        ));                                                                     // claim it before calling out
        emitter.instruction("call __rt_user_filter_release_obj");               // onClose()
        emitter.label("__rt_fnco_done_x");
        emitter.instruction("leave");                                           // restore rbp + rsp
        emitter.instruction("ret");                                             // return to the caller
        return;
    }
    emitter.instruction("sub sp, sp, #16");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("bl __rt_filter_state");                                // x0 = FilterState or 0
    emitter.instruction("cbz x0, __rt_fnco_done");                              // a stale handle owns no instance
    emitter.instruction(&format!("ldr x1, [x0, #{FILTER_OBJECT_OFFSET}]"));     // the php_user_filter instance, if any
    emitter.instruction("cbz x1, __rt_fnco_done");                              // a built-in node carries none
    emitter.instruction(&format!("str xzr, [x0, #{FILTER_OBJECT_OFFSET}]"));    // claim it before calling out
    emitter.instruction("mov x0, x1");
    emitter.instruction("bl __rt_user_filter_release_obj");                     // onClose()
    emitter.label("__rt_fnco_done");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the caller
}

pub fn emit_stream_close_filter_chains(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_close_filter_chains_x86_64(emitter);
        return;
    }
    emitter.blank();
    emitter.comment("--- runtime: close a stream's attached filters ---");
    emitter.label_global("__rt_stream_close_filter_chains");
    // Frame: [0]=state [8]=current head offset [16]=node handle [24]=next handle
    emitter.instruction("sub sp, sp, #48");                                     // reserve the teardown frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("cbz x0, __rt_scfc_done");                              // a null state owns no chains
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the stream state
    emitter.instruction(&format!("mov x9, #{STREAM_READ_FILTER_HEAD_OFFSET}")); // start with the read chain
    emitter.instruction("str x9, [sp, #8]");

    emitter.label("__rt_scfc_chain");
    emitter.instruction("ldr x0, [sp, #0]");                                    // stream state
    emitter.instruction("ldr x9, [sp, #8]");                                    // current chain-head offset
    emitter.instruction("add x10, x0, x9");                                     // chain head slot address
    emitter.instruction("ldr x11, [x10]");                                      // first node handle
    emitter.instruction("str xzr, [x10]");                                      // detach the chain before closing it
    emitter.instruction("str x11, [sp, #16]");                                  // walk cursor

    emitter.label("__rt_scfc_node");
    emitter.instruction("ldr x11, [sp, #16]");
    emitter.instruction("cbz x11, __rt_scfc_chain_done");                       // end of chain
    emitter.instruction("mov x0, x11");                                         // resolve the node to read its successor
    emitter.instruction("bl __rt_filter_state");
    emitter.instruction("cbz x0, __rt_scfc_close");                             // already dead: nothing to read ahead
    emitter.instruction(&format!("ldr x12, [x0, #{FILTER_NEXT_OFFSET}]"));      // successor handle
    emitter.instruction("str x12, [sp, #24]");
    // A node carrying a user filter owes it an `onClose()`. The per-descriptor sweep
    // cannot reach it, so the chain fires it here. Clearing the slot first keeps it to
    // one call: a STREAM_FILTER_ALL node sits in both chains and is visited twice.
    emitter.instruction(&format!("ldr x1, [x0, #{FILTER_OBJECT_OFFSET}]"));     // php_user_filter instance, if any
    emitter.instruction("cbz x1, __rt_scfc_close_ready");                       // a built-in node carries none
    emitter.instruction(&format!("str xzr, [x0, #{FILTER_OBJECT_OFFSET}]"));    // claim the instance before calling out
    emitter.instruction("mov x0, x1");
    emitter.instruction("bl __rt_user_filter_release_obj");                     // onClose()
    emitter.instruction("b __rt_scfc_close_ready");
    emitter.label("__rt_scfc_close");
    emitter.instruction("str xzr, [sp, #24]");                                  // a stale node ends the walk
    emitter.label("__rt_scfc_close_ready");
    emitter.instruction("ldr x0, [sp, #16]");                                   // node handle
    emitter.instruction("bl __rt_resource_mark_closed");                        // publish Closed exactly once
    emitter.instruction("ldr x0, [sp, #16]");                                   // node handle
    emitter.instruction("bl __rt_resource_release");                            // drop the attach-time reference
    emitter.instruction("ldr x12, [sp, #24]");                                  // advance to the successor
    emitter.instruction("str x12, [sp, #16]");
    emitter.instruction("b __rt_scfc_node");

    emitter.label("__rt_scfc_chain_done");
    emitter.instruction("ldr x9, [sp, #8]");
    emitter.instruction(&format!("cmp x9, #{STREAM_WRITE_FILTER_HEAD_OFFSET}"));
    emitter.instruction("b.eq __rt_scfc_done");                                 // both chains handled
    emitter.instruction(&format!("mov x9, #{STREAM_WRITE_FILTER_HEAD_OFFSET}")); // continue with the write chain
    emitter.instruction("str x9, [sp, #8]");
    emitter.instruction("b __rt_scfc_chain");

    emitter.label("__rt_scfc_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the teardown frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// x86_64 variant of [`emit_stream_close_filter_chains`].
fn emit_stream_close_filter_chains_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: close a stream's attached filters ---");
    emitter.label_global("__rt_stream_close_filter_chains");
    // Frame: [-8]=state [-16]=head offset [-24]=node handle [-32]=next handle
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the teardown frame
    emitter.instruction("sub rsp, 48");                                         // reserve spill slots
    emitter.instruction("test rdi, rdi");
    emitter.instruction("jz __rt_scfc_done_x");                                 // a null state owns no chains
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the stream state
    emitter.instruction(&format!("mov QWORD PTR [rbp - 16], {STREAM_READ_FILTER_HEAD_OFFSET}"));

    emitter.label("__rt_scfc_chain_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // stream state
    emitter.instruction("add rax, QWORD PTR [rbp - 16]");                       // chain head slot address
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // first node handle
    emitter.instruction("mov QWORD PTR [rax], 0");                              // detach the chain before closing it
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // walk cursor

    emitter.label("__rt_scfc_node_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_scfc_chain_done_x");                           // end of chain
    emitter.instruction("mov rdi, r10");                                        // resolve the node to read its successor
    emitter.instruction("call __rt_filter_state");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_scfc_close_x");                                // already dead: nothing to read ahead
    emitter.instruction(&format!("mov r11, QWORD PTR [rax + {FILTER_NEXT_OFFSET}]"));
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");
    // See the AArch64 counterpart: the chain owes a node-carried user filter its
    // `onClose()`, and the cleared slot keeps a STREAM_FILTER_ALL node to one call.
    emitter.instruction(&format!("mov rdi, QWORD PTR [rax + {FILTER_OBJECT_OFFSET}]")); // php_user_filter instance, if any
    emitter.instruction("test rdi, rdi");
    emitter.instruction("jz __rt_scfc_close_ready_x");                          // a built-in node carries none
    emitter.instruction(&format!("mov QWORD PTR [rax + {FILTER_OBJECT_OFFSET}], 0")); // claim the instance before calling out
    emitter.instruction("call __rt_user_filter_release_obj");                   // onClose()
    emitter.instruction("jmp __rt_scfc_close_ready_x");
    emitter.label("__rt_scfc_close_x");
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // a stale node ends the walk
    emitter.label("__rt_scfc_close_ready_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // node handle
    emitter.instruction("call __rt_resource_mark_closed");                      // publish Closed exactly once
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // node handle
    emitter.instruction("call __rt_resource_release");                          // drop the attach-time reference
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // advance to the successor
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");
    emitter.instruction("jmp __rt_scfc_node_x");

    emitter.label("__rt_scfc_chain_done_x");
    emitter.instruction(&format!("cmp QWORD PTR [rbp - 16], {STREAM_WRITE_FILTER_HEAD_OFFSET}"));
    emitter.instruction("je __rt_scfc_done_x");                                 // both chains handled
    emitter.instruction(&format!("mov QWORD PTR [rbp - 16], {STREAM_WRITE_FILTER_HEAD_OFFSET}"));
    emitter.instruction("jmp __rt_scfc_chain_x");

    emitter.label("__rt_scfc_done_x");
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return to the caller
}
