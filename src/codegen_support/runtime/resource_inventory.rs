//! Purpose:
//! Tracks every PHP resource incarnation and materializes `get_resources()` results.
//!
//! Called from:
//! - `__rt_mixed_from_value` when a genuine PHP resource is boxed.
//! - Explicit stream close lowering and the Core `get_resources()` lowering.
//!
//! Key details:
//! - Inventory nodes are append-only so descriptor reuse cannot erase closed resources.
//! - Nodes store PHP id, native payload, subtype, and close state independently.
//! - Returned hashes use integer PHP resource ids as keys and raw tag-9 values.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Synthetic payload reserved for PHP's implicit default stream context, resource id 4.
pub(super) const DEFAULT_CONTEXT_PAYLOAD: i64 = (1_i64 << 62) - 1;

/// Number of payload bytes in one append-only resource inventory node.
const RESOURCE_NODE_BYTES: i64 = 40;

/// Emits inventory registration, close tracking, filtering, and enumeration helpers.
pub(crate) fn emit_resource_inventory(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_resource_inventory_aarch64(emitter),
        Arch::X86_64 => emit_resource_inventory_x86_64(emitter),
    }
}

/// Emits the complete AArch64 inventory helper family.
fn emit_resource_inventory_aarch64(emitter: &mut Emitter) {
    emit_register_aarch64(emitter);
    emit_close_aarch64(emitter);
    emit_insert_aarch64(emitter);
    emit_type_selector_aarch64(emitter);
    emit_get_resources_aarch64(emitter);
}

/// Registers one resource id exactly once in creation order on AArch64.
fn emit_register_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_inventory_register ---");
    emitter.label_global("__rt_resource_inventory_register");
    emitter.instruction("sub sp, sp, #64");                                     // save the incoming tuple across allocation
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the inventory registration frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save PHP id and native payload
    emitter.instruction("str x2, [sp, #16]");                                   // save the resource subtype
    emitter.instruction("cmp x0, #4");                                          // standard streams and the implicit context are synthesized
    emitter.instruction("b.le __rt_resource_inventory_register_done");          // do not duplicate ids 1 through 4 in the linked list
    abi::emit_symbol_address(emitter, "x9", "_resource_inventory_head");
    emitter.instruction("ldr x10, [x9]");                                       // load the first recorded incarnation
    emitter.label("__rt_resource_inventory_register_find");
    emitter.instruction("cbz x10, __rt_resource_inventory_register_new");       // a missing id needs a fresh node
    emitter.instruction("ldr x11, [x10, #8]");                                  // load the node's PHP id
    emitter.instruction("cmp x11, x0");                                         // compare with the incoming PHP id
    emitter.instruction("b.eq __rt_resource_inventory_register_done");          // aliases of an existing resource add no new incarnation
    emitter.instruction("ldr x10, [x10, #0]");                                  // follow the creation-order next pointer
    emitter.instruction("b __rt_resource_inventory_register_find");             // continue until the id is found or the list ends

    emitter.label("__rt_resource_inventory_register_new");
    abi::emit_load_int_immediate(emitter, "x0", RESOURCE_NODE_BYTES);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction("mov x9, x0");                                          // keep the newly allocated node pointer
    emitter.instruction("str xzr, [x9, #0]");                                   // next = null on the new tail
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload PHP id
    emitter.instruction("str x10, [x9, #8]");                                   // store PHP id
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload native payload
    emitter.instruction("str x10, [x9, #16]");                                  // store native payload
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload resource subtype
    emitter.instruction("str x10, [x9, #24]");                                  // store resource subtype
    emitter.instruction("str xzr, [x9, #32]");                                  // closed = false
    abi::emit_symbol_address(emitter, "x10", "_resource_inventory_tail");
    emitter.instruction("ldr x11, [x10]");                                      // load the preceding tail
    emitter.instruction("cbz x11, __rt_resource_inventory_register_first");     // an empty list initializes its head
    emitter.instruction("str x9, [x11, #0]");                                   // append after the preceding tail
    emitter.instruction("b __rt_resource_inventory_register_tail");             // skip first-node head initialization
    emitter.label("__rt_resource_inventory_register_first");
    abi::emit_symbol_address(emitter, "x11", "_resource_inventory_head");
    emitter.instruction("str x9, [x11]");                                       // publish the first node as list head
    emitter.label("__rt_resource_inventory_register_tail");
    emitter.instruction("str x9, [x10]");                                       // publish the new tail

    emitter.label("__rt_resource_inventory_register_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame state
    emitter.instruction("add sp, sp, #64");                                     // release tuple spill storage
    emitter.instruction("ret");                                                 // return after registration or deduplication
}

/// Marks one inventory id closed while preserving its identity on AArch64.
fn emit_close_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_inventory_close ---");
    emitter.label_global("__rt_resource_inventory_close");
    emitter.instruction("sub sp, sp, #32");                                     // preserve caller scratch still holding box and native handle
    emitter.instruction("stp x9, x10, [sp, #0]");                               // save inventory search scratch pair
    emitter.instruction("str x11, [sp, #16]");                                  // save the close caller's native-handle register
    abi::emit_symbol_address(emitter, "x9", "_resource_inventory_head");
    emitter.instruction("ldr x9, [x9]");                                        // start at the oldest resource incarnation
    emitter.label("__rt_resource_inventory_close_find");
    emitter.instruction("cbz x9, __rt_resource_inventory_close_done");          // an untracked id needs no update
    emitter.instruction("ldr x10, [x9, #8]");                                   // load the node's PHP id
    emitter.instruction("cmp x10, x0");                                         // is this the resource being closed?
    emitter.instruction("b.eq __rt_resource_inventory_close_hit");              // update the matching node
    emitter.instruction("ldr x9, [x9, #0]");                                    // follow the next pointer
    emitter.instruction("b __rt_resource_inventory_close_find");                // continue the linear search
    emitter.label("__rt_resource_inventory_close_hit");
    emitter.instruction("mov x10, #1");                                         // materialize the closed marker
    emitter.instruction("str x10, [x9, #32]");                                  // retain the node but mark it Unknown
    emitter.instruction("neg x10, x0");                                         // closed resources carry their stable id as -id
    emitter.instruction("str x10, [x9, #16]");                                  // replace the now-invalid native payload
    emitter.label("__rt_resource_inventory_close_done");
    emitter.instruction("ldr x11, [sp, #16]");                                  // restore caller native-handle register
    emitter.instruction("ldp x9, x10, [sp, #0]");                               // restore caller box and scratch registers
    emitter.instruction("add sp, sp, #32");                                     // release preservation storage
    emitter.instruction("ret");                                                 // return with the caller's live close state intact
}

/// Tail-calls `__rt_hash_set` with one integer-keyed raw resource value on AArch64.
fn emit_insert_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_inventory_insert ---");
    emitter.label_global("__rt_resource_inventory_insert");
    emitter.instruction("mov x4, x3");                                          // value high word = resource subtype
    emitter.instruction("mov x3, x2");                                          // value low word = native payload or -id
    emitter.instruction("mov x2, #-1");                                         // key high word -1 marks an inline integer key
    emitter.instruction("mov x5, #9");                                          // runtime value tag 9 = PHP resource
    emitter.instruction("b __rt_hash_set");                                     // return the possibly-grown hash directly
}

/// Converts one exact PHP resource type name to an internal selector on AArch64.
fn emit_type_selector_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_type_selector ---");
    emitter.label_global("__rt_resource_type_selector");
    emitter.instruction("sub sp, sp, #32");                                     // preserve the input string across equality calls
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save filter pointer and byte length
    for (selector, symbol, len, next) in [
        (0, "_resource_type_stream", 6, "__rt_resource_type_selector_context"),
        (1, "_resource_type_stream_context", 14, "__rt_resource_type_selector_filter"),
        (2, "_resource_type_stream_filter", 13, "__rt_resource_type_selector_unknown"),
        (3, "_resource_type_unknown", 7, "__rt_resource_type_selector_invalid"),
    ] {
        emitter.instruction("ldp x1, x2, [sp, #0]");                            // restore the filter pair for this comparison
        abi::emit_symbol_address(emitter, "x3", symbol);
        abi::emit_load_int_immediate(emitter, "x4", len);
        abi::emit_call_label(emitter, "__rt_str_eq");
        emitter.instruction(&format!("cbz x0, {next}"));                        // try the next valid resource type after a mismatch
        abi::emit_load_int_immediate(emitter, "x0", selector);
        emitter.instruction("b __rt_resource_type_selector_done");              // return the matching selector
        emitter.label(next);
    }
    abi::emit_load_int_immediate(emitter, "x0", 4);                           // selector 4 means invalid resource type
    emitter.label("__rt_resource_type_selector_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame state after string comparisons
    emitter.instruction("add sp, sp, #32");                                     // release filter spill storage
    emitter.instruction("ret");                                                 // return selector in x0
}

/// Materializes PHP's integer-keyed resource inventory on AArch64.
fn emit_get_resources_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: get_resources ---");
    emitter.label_global("__rt_get_resources");
    emitter.instruction("sub sp, sp, #64");                                     // reserve selector, result, node, and frame storage
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the enumeration frame
    emitter.instruction("str x0, [sp, #0]");                                    // save requested type selector, -1 means all
    abi::emit_symbol_address(emitter, "x9", "_resource_id_next");
    emitter.instruction("ldr x0, [x9]");                                        // size from the highest reserved resource id
    emitter.instruction("lsl x0, x0, #1");                                      // keep hash load below the growth threshold
    emitter.instruction("add x0, x0, #16");                                     // leave headroom for standard resources
    emitter.instruction("mov x1, #7");                                          // hash values carry independent runtime tags
    abi::emit_call_label(emitter, "__rt_hash_new");
    emitter.instruction("str x0, [sp, #8]");                                    // save the current result hash pointer

    emitter.instruction("ldr x9, [sp, #0]");                                    // reload selector for standard stream filtering
    emitter.instruction("cmn x9, #1");                                          // does the caller request all resource types?
    emitter.instruction("b.eq __rt_get_resources_standard");                    // all includes the three standard streams
    emitter.instruction("cbnz x9, __rt_get_resources_context");                 // selector 0 alone names stream resources
    emitter.label("__rt_get_resources_standard");
    for (id, payload) in [(1, 0), (2, 1), (3, 2)] {
        emitter.instruction("ldr x0, [sp, #8]");                                // reload the possibly-grown destination hash
        abi::emit_load_int_immediate(emitter, "x1", id);
        abi::emit_load_int_immediate(emitter, "x2", payload);
        abi::emit_load_int_immediate(emitter, "x3", 1);
        abi::emit_call_label(emitter, "__rt_resource_inventory_insert");
        emitter.instruction("str x0, [sp, #8]");                                // retain a grown hash pointer for later insertions
    }

    emitter.label("__rt_get_resources_context");
    abi::emit_symbol_address(emitter, "x9", "_resource_inventory_head");
    emitter.instruction("ldr x9, [x9]");                                        // any user resource implies PHP's default context exists
    emitter.instruction("cbz x9, __rt_get_resources_nodes");                    // startup-only inventories contain no context id 4
    emitter.instruction("ldr x10, [sp, #0]");                                   // inspect requested resource type
    emitter.instruction("cmn x10, #1");                                         // all types include the default context
    emitter.instruction("b.eq __rt_get_resources_add_context");                 // add it before user resources to preserve PHP order
    emitter.instruction("cmp x10, #1");                                         // selector 1 names stream-context
    emitter.instruction("b.ne __rt_get_resources_nodes");                       // other filters skip the context
    emitter.label("__rt_get_resources_add_context");
    emitter.instruction("ldr x0, [sp, #8]");                                    // destination hash
    emitter.instruction("mov x1, #4");                                          // PHP reserves resource id 4 for the default context
    abi::emit_load_int_immediate(emitter, "x2", DEFAULT_CONTEXT_PAYLOAD);
    emitter.instruction("mov x3, #10");                                         // subtype 10 = stream-context
    abi::emit_call_label(emitter, "__rt_resource_inventory_insert");
    emitter.instruction("str x0, [sp, #8]");                                    // save the possibly-grown hash

    emitter.label("__rt_get_resources_nodes");
    abi::emit_symbol_address(emitter, "x9", "_resource_inventory_head");
    emitter.instruction("ldr x9, [x9]");                                        // begin in creation order
    emitter.instruction("str x9, [sp, #16]");                                   // save current node across hash insertion
    emitter.label("__rt_get_resources_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload current node
    emitter.instruction("cbz x9, __rt_get_resources_done");                     // null ends the inventory walk
    emitter.instruction("ldr x10, [x9, #32]");                                  // load closed flag
    emitter.instruction("cbnz x10, __rt_get_resources_type_unknown");           // closed resources expose the Unknown type
    emitter.instruction("ldr x10, [x9, #24]");                                  // load open resource subtype
    emitter.instruction("cmp x10, #10");                                        // stream-context subtype?
    emitter.instruction("b.eq __rt_get_resources_type_context");                // select context filter code
    emitter.instruction("cmp x10, #9");                                         // stream filter subtype?
    emitter.instruction("b.eq __rt_get_resources_type_filter");                 // select filter filter code
    emitter.instruction("mov x10, #0");                                         // every other genuine resource is a stream
    emitter.instruction("b __rt_get_resources_type_ready");                     // continue with the resolved type code
    emitter.label("__rt_get_resources_type_context");
    emitter.instruction("mov x10, #1");                                         // selector 1 = stream-context
    emitter.instruction("b __rt_get_resources_type_ready");                     // continue to filter matching
    emitter.label("__rt_get_resources_type_filter");
    emitter.instruction("mov x10, #2");                                         // selector 2 = stream filter
    emitter.instruction("b __rt_get_resources_type_ready");                     // continue to filter matching
    emitter.label("__rt_get_resources_type_unknown");
    emitter.instruction("mov x10, #3");                                         // selector 3 = Unknown
    emitter.label("__rt_get_resources_type_ready");
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload requested selector
    emitter.instruction("cmn x11, #1");                                         // -1 accepts every type
    emitter.instruction("b.eq __rt_get_resources_insert_node");                 // include without comparison
    emitter.instruction("cmp x11, x10");                                        // otherwise require an exact type match
    emitter.instruction("b.ne __rt_get_resources_next");                        // skip nonmatching resources
    emitter.label("__rt_get_resources_insert_node");
    emitter.instruction("ldr x0, [sp, #8]");                                    // destination hash
    emitter.instruction("ldr x1, [x9, #8]");                                    // integer key = PHP resource id
    emitter.instruction("ldr x2, [x9, #16]");                                   // value payload, already -id after close
    emitter.instruction("ldr x3, [x9, #24]");                                   // preserve original resource subtype
    abi::emit_call_label(emitter, "__rt_resource_inventory_insert");
    emitter.instruction("str x0, [sp, #8]");                                    // save destination after possible growth
    emitter.label("__rt_get_resources_next");
    emitter.instruction("ldr x9, [sp, #16]");                                   // restore current node after helper calls
    emitter.instruction("ldr x9, [x9, #0]");                                    // follow creation-order next pointer
    emitter.instruction("str x9, [sp, #16]");                                   // save next node
    emitter.instruction("b __rt_get_resources_loop");                           // continue enumeration

    emitter.label("__rt_get_resources_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the final hash pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore caller frame state
    emitter.instruction("add sp, sp, #64");                                     // release enumeration storage
    emitter.instruction("ret");                                                 // return the owned associative array
}

/// Emits the complete x86_64 inventory helper family.
fn emit_resource_inventory_x86_64(emitter: &mut Emitter) {
    emit_register_x86_64(emitter);
    emit_close_x86_64(emitter);
    emit_insert_x86_64(emitter);
    emit_type_selector_x86_64(emitter);
    emit_get_resources_x86_64(emitter);
}

/// Registers one resource id exactly once in creation order on x86_64.
fn emit_register_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_inventory_register ---");
    emitter.label_global("__rt_resource_inventory_register");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish stable spill addressing
    emitter.instruction("sub rsp, 32");                                         // save PHP id, payload, and subtype
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save PHP id
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save native payload
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save resource subtype
    emitter.instruction("cmp rdi, 4");                                          // ids 1 through 4 are synthesized
    emitter.instruction("jle __rt_resource_inventory_register_done_x86");       // do not append standard entries
    abi::emit_symbol_address(emitter, "r10", "_resource_inventory_head");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load first node
    emitter.label("__rt_resource_inventory_register_find_x86");
    emitter.instruction("test r10, r10");                                       // did the list end?
    emitter.instruction("jz __rt_resource_inventory_register_new_x86");         // allocate a node for a new id
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // load stored PHP id
    emitter.instruction("cmp r11, QWORD PTR [rbp - 8]");                        // compare with incoming id
    emitter.instruction("je __rt_resource_inventory_register_done_x86");        // aliases do not append
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // follow next pointer
    emitter.instruction("jmp __rt_resource_inventory_register_find_x86");       // continue search

    emitter.label("__rt_resource_inventory_register_new_x86");
    abi::emit_load_int_immediate(emitter, "rax", RESOURCE_NODE_BYTES);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction("mov r9, rax");                                         // keep new node pointer
    emitter.instruction("mov QWORD PTR [r9], 0");                               // next = null
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload PHP id
    emitter.instruction("mov QWORD PTR [r9 + 8], r10");                         // store PHP id
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload native payload
    emitter.instruction("mov QWORD PTR [r9 + 16], r10");                        // store native payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload resource subtype
    emitter.instruction("mov QWORD PTR [r9 + 24], r10");                        // store resource subtype
    emitter.instruction("mov QWORD PTR [r9 + 32], 0");                          // closed = false
    abi::emit_symbol_address(emitter, "r10", "_resource_inventory_tail");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load preceding tail
    emitter.instruction("test r11, r11");                                       // is the list empty?
    emitter.instruction("jz __rt_resource_inventory_register_first_x86");       // initialize head for the first node
    emitter.instruction("mov QWORD PTR [r11], r9");                             // append after preceding tail
    emitter.instruction("jmp __rt_resource_inventory_register_tail_x86");       // skip head initialization
    emitter.label("__rt_resource_inventory_register_first_x86");
    abi::emit_symbol_address(emitter, "r11", "_resource_inventory_head");
    emitter.instruction("mov QWORD PTR [r11], r9");                             // publish first node as head
    emitter.label("__rt_resource_inventory_register_tail_x86");
    emitter.instruction("mov QWORD PTR [r10], r9");                             // publish new tail

    emitter.label("__rt_resource_inventory_register_done_x86");
    emitter.instruction("add rsp, 32");                                         // release tuple spills
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return after registration
}

/// Marks one inventory id closed while preserving its identity on x86_64.
fn emit_close_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_inventory_close ---");
    emitter.label_global("__rt_resource_inventory_close");
    emitter.instruction("push r10");                                            // preserve native handle held by the close caller
    emitter.instruction("push r11");                                            // preserve resource Mixed box pointer held by the close caller
    abi::emit_symbol_address(emitter, "r10", "_resource_inventory_head");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // start at oldest incarnation
    emitter.label("__rt_resource_inventory_close_find_x86");
    emitter.instruction("test r10, r10");                                       // did the list end?
    emitter.instruction("jz __rt_resource_inventory_close_done_x86");           // an untracked id is a no-op
    emitter.instruction("cmp QWORD PTR [r10 + 8], rax");                        // compare node id with the closing id
    emitter.instruction("je __rt_resource_inventory_close_hit_x86");            // update matching node
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // follow next pointer
    emitter.instruction("jmp __rt_resource_inventory_close_find_x86");          // continue search
    emitter.label("__rt_resource_inventory_close_hit_x86");
    emitter.instruction("mov QWORD PTR [r10 + 32], 1");                         // mark resource Unknown
    emitter.instruction("mov r11, rax");                                        // copy stable PHP id
    emitter.instruction("neg r11");                                             // closed payload sentinel = -id
    emitter.instruction("mov QWORD PTR [r10 + 16], r11");                       // discard the invalid native payload
    emitter.label("__rt_resource_inventory_close_done_x86");
    emitter.instruction("pop r11");                                             // restore resource Mixed box pointer
    emitter.instruction("pop r10");                                             // restore native handle
    emitter.instruction("ret");                                                 // return with caller scratch intact
}

/// Tail-calls `__rt_hash_set` with one integer-keyed raw resource value on x86_64.
fn emit_insert_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_inventory_insert ---");
    emitter.label_global("__rt_resource_inventory_insert");
    emitter.instruction("mov r10, rdi");                                        // preserve integer resource-id key
    emitter.instruction("mov r11, rsi");                                        // preserve resource payload
    emitter.instruction("mov r8, rdx");                                         // value high word = resource subtype
    emitter.instruction("mov rdi, rax");                                        // destination hash pointer
    emitter.instruction("mov rsi, r10");                                        // key low word = resource id
    emitter.instruction("mov rdx, -1");                                         // key high word -1 marks an integer key
    emitter.instruction("mov rcx, r11");                                        // value low word = native payload or -id
    emitter.instruction("mov r9, 9");                                           // runtime value tag 9 = PHP resource
    emitter.instruction("jmp __rt_hash_set");                                   // return the possibly-grown hash directly
}

/// Converts one exact PHP resource type name to an internal selector on x86_64.
fn emit_type_selector_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_type_selector ---");
    emitter.label_global("__rt_resource_type_selector");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish stable filter spills
    emitter.instruction("sub rsp, 16");                                         // save filter pointer and length
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save filter pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save filter length
    for (selector, symbol, len, next) in [
        (0, "_resource_type_stream", 6, "__rt_resource_type_selector_context_x86"),
        (1, "_resource_type_stream_context", 14, "__rt_resource_type_selector_filter_x86"),
        (2, "_resource_type_stream_filter", 13, "__rt_resource_type_selector_unknown_x86"),
        (3, "_resource_type_unknown", 7, "__rt_resource_type_selector_invalid_x86"),
    ] {
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // restore filter pointer for comparison
        emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                   // restore filter length for comparison
        abi::emit_symbol_address(emitter, "rdx", symbol);
        abi::emit_load_int_immediate(emitter, "rcx", len);
        abi::emit_call_label(emitter, "__rt_str_eq");
        emitter.instruction("test rax, rax");                                   // did this valid type name match?
        emitter.instruction(&format!("jz {next}"));                             // try the next type name after a mismatch
        abi::emit_load_int_immediate(emitter, "rax", selector);
        emitter.instruction("jmp __rt_resource_type_selector_done_x86");        // return matching selector
        emitter.label(next);
    }
    abi::emit_load_int_immediate(emitter, "rax", 4);                           // selector 4 means invalid resource type
    emitter.label("__rt_resource_type_selector_done_x86");
    emitter.instruction("add rsp, 16");                                         // release filter spills
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return selector in rax
}

/// Materializes PHP's integer-keyed resource inventory on x86_64.
fn emit_get_resources_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: get_resources ---");
    emitter.label_global("__rt_get_resources");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish stable enumeration spills
    emitter.instruction("sub rsp, 32");                                         // selector, result, and node slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save requested selector
    abi::emit_symbol_address(emitter, "r10", "_resource_id_next");
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // derive capacity from highest reserved id
    emitter.instruction("shl rdi, 1");                                          // keep hash load below growth threshold
    emitter.instruction("add rdi, 16");                                         // headroom for standard entries
    emitter.instruction("mov rsi, 7");                                          // hash entries carry independent runtime tags
    abi::emit_call_label(emitter, "__rt_hash_new");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save destination hash

    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // inspect selector for standard streams
    emitter.instruction("cmp r10, -1");                                         // all resource types?
    emitter.instruction("je __rt_get_resources_standard_x86");                  // include standard streams
    emitter.instruction("test r10, r10");                                       // selector 0 = stream
    emitter.instruction("jne __rt_get_resources_context_x86");                  // other filters skip standard streams
    emitter.label("__rt_get_resources_standard_x86");
    for (id, payload) in [(1, 0), (2, 1), (3, 2)] {
        emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                   // reload destination hash
        abi::emit_load_int_immediate(emitter, "rdi", id);
        abi::emit_load_int_immediate(emitter, "rsi", payload);
        abi::emit_load_int_immediate(emitter, "rdx", 1);
        abi::emit_call_label(emitter, "__rt_resource_inventory_insert");
        emitter.instruction("mov QWORD PTR [rbp - 16], rax");                   // save possibly-grown hash
    }

    emitter.label("__rt_get_resources_context_x86");
    abi::emit_symbol_address(emitter, "r10", "_resource_inventory_head");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // any user resource implies default context id 4
    emitter.instruction("test r10, r10");                                       // is the inventory still startup-only?
    emitter.instruction("jz __rt_get_resources_nodes_x86");                     // no context before a user resource exists
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload selector
    emitter.instruction("cmp r11, -1");                                         // all types include context
    emitter.instruction("je __rt_get_resources_add_context_x86");               // insert it before user resources
    emitter.instruction("cmp r11, 1");                                          // selector 1 = stream-context
    emitter.instruction("jne __rt_get_resources_nodes_x86");                    // other filters skip context
    emitter.label("__rt_get_resources_add_context_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // destination hash
    emitter.instruction("mov rdi, 4");                                          // reserved default-context id
    abi::emit_load_int_immediate(emitter, "rsi", DEFAULT_CONTEXT_PAYLOAD);
    emitter.instruction("mov rdx, 10");                                         // subtype 10 = stream-context
    abi::emit_call_label(emitter, "__rt_resource_inventory_insert");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save possibly-grown hash

    emitter.label("__rt_get_resources_nodes_x86");
    abi::emit_symbol_address(emitter, "r10", "_resource_inventory_head");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // begin in creation order
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save current node across insertions
    emitter.label("__rt_get_resources_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload current node
    emitter.instruction("test r10, r10");                                       // did the linked list end?
    emitter.instruction("jz __rt_get_resources_done_x86");                      // return completed hash
    emitter.instruction("cmp QWORD PTR [r10 + 32], 0");                         // inspect closed marker
    emitter.instruction("jne __rt_get_resources_type_unknown_x86");             // closed resources are Unknown
    emitter.instruction("mov r11, QWORD PTR [r10 + 24]");                       // load open subtype
    emitter.instruction("cmp r11, 10");                                         // stream-context subtype?
    emitter.instruction("je __rt_get_resources_type_context_x86");              // select context code
    emitter.instruction("cmp r11, 9");                                          // stream filter subtype?
    emitter.instruction("je __rt_get_resources_type_filter_x86");               // select filter code
    emitter.instruction("xor r11d, r11d");                                      // selector 0 = stream
    emitter.instruction("jmp __rt_get_resources_type_ready_x86");               // continue to filter matching
    emitter.label("__rt_get_resources_type_context_x86");
    emitter.instruction("mov r11, 1");                                          // selector 1 = stream-context
    emitter.instruction("jmp __rt_get_resources_type_ready_x86");               // continue to filter matching
    emitter.label("__rt_get_resources_type_filter_x86");
    emitter.instruction("mov r11, 2");                                          // selector 2 = stream filter
    emitter.instruction("jmp __rt_get_resources_type_ready_x86");               // continue to filter matching
    emitter.label("__rt_get_resources_type_unknown_x86");
    emitter.instruction("mov r11, 3");                                          // selector 3 = Unknown
    emitter.label("__rt_get_resources_type_ready_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload requested selector
    emitter.instruction("cmp r10, -1");                                         // -1 accepts all types
    emitter.instruction("je __rt_get_resources_insert_node_x86");               // include without exact comparison
    emitter.instruction("cmp r10, r11");                                        // otherwise require exact type match
    emitter.instruction("jne __rt_get_resources_next_x86");                     // skip nonmatching resource
    emitter.label("__rt_get_resources_insert_node_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // restore current node
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // destination hash
    emitter.instruction("mov rdi, QWORD PTR [r10 + 8]");                        // integer key = PHP id
    emitter.instruction("mov rsi, QWORD PTR [r10 + 16]");                       // native payload or -id
    emitter.instruction("mov rdx, QWORD PTR [r10 + 24]");                       // preserve original subtype
    abi::emit_call_label(emitter, "__rt_resource_inventory_insert");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save destination after possible growth
    emitter.label("__rt_get_resources_next_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // restore current node
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // follow next pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save next node
    emitter.instruction("jmp __rt_get_resources_loop_x86");                     // continue enumeration

    emitter.label("__rt_get_resources_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return final hash pointer
    emitter.instruction("add rsp, 32");                                         // release enumeration spills
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return owned associative array
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Verifies both supported architectures expose every inventory entry point.
    #[test]
    fn emits_complete_inventory_helper_family_on_both_architectures() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_inventory(&mut emitter);
            let asm = emitter.output();
            for symbol in [
                "__rt_resource_inventory_register:",
                "__rt_resource_inventory_close:",
                "__rt_resource_inventory_insert:",
                "__rt_resource_type_selector:",
                "__rt_get_resources:",
            ] {
                assert!(asm.contains(symbol), "missing {symbol} for {target:?}");
            }
        }
    }

    /// Pins that enumeration includes PHP's standard ids and append-only node walk.
    #[test]
    fn enumeration_includes_standard_resources_and_walks_inventory() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_inventory(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("_resource_inventory_head"), "{target:?}");
            assert!(asm.contains("_resource_inventory_tail"), "{target:?}");
            assert!(asm.contains("__rt_get_resources_standard"), "{target:?}");
            assert!(asm.contains("__rt_get_resources_loop"), "{target:?}");
            assert!(asm.contains("__rt_resource_inventory_insert"), "{target:?}");
        }
    }
}
