//! Purpose:
//! Tracks every PHP resource incarnation and materializes `get_resources()` results.
//!
//! Called from:
//! - `__rt_mixed_from_value` when a genuine PHP resource is boxed.
//! - Explicit stream close lowering and the Core `get_resources()` lowering.
//!
//! Key details:
//! - Inventory nodes are append-only so descriptor reuse cannot erase closed resources.
//! - Nodes weakly reference a canonical Mixed cell and record its incarnation state.
//! - Returned hashes retain that cell, sharing ownership and explicit close state.
//! - Reset closes still-open owned handles, but preserves fd 0, 1, and 2 aliases.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::RuntimeFeatures;
use elephc_memoir::values::resource_weak::{self, CELL_OFFSET, ID_OFFSET, PAYLOAD_OFFSET, RETIRED, STATE_OFFSET, SUBTYPE_OFFSET};

/// Synthetic payload reserved for PHP's implicit default stream context, resource id 4.
pub(crate) const DEFAULT_CONTEXT_PAYLOAD: i64 = (1_i64 << 62) - 1;

/// Emits inventory registration, close tracking, filtering, and enumeration helpers.
pub(crate) fn emit_resource_inventory(emitter: &mut Emitter, features: RuntimeFeatures) {
    emitter.label_global("__rt_web_close_stream");
    let target = if emitter.target.arch == crate::codegen_support::platform::Arch::AArch64 { "x9" } else { "r11" };
    crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, target, crate::codegen_support::runtime::data::WEB_STREAM_CLOSE_SLOT, 0);
    emitter.instruction(if emitter.target.arch == crate::codegen_support::platform::Arch::AArch64 { "br x9" } else { "jmp r11" }); // tail-call the installed native stream closer with its C descriptor argument
    resource_weak::emit_retire(emitter);
    match emitter.target.arch {
        Arch::AArch64 => emit_resource_inventory_aarch64(emitter, features),
        Arch::X86_64 => emit_resource_inventory_x86_64(emitter, features),
    }
}

/// Emits the complete AArch64 inventory helper family.
fn emit_resource_inventory_aarch64(emitter: &mut Emitter, features: RuntimeFeatures) {
    emit_register_aarch64(emitter);
    emit_close_aarch64(emitter);
    emit_reset_aarch64(emitter, features);
    emit_insert_aarch64(emitter);
    emit_type_selector_aarch64(emitter);
    emit_get_resources_aarch64(emitter);
}

/// Routes native handle cleanup during the Memoir-owned inventory reset.
fn emit_reset_aarch64(emitter: &mut Emitter, features: RuntimeFeatures) {
    resource_weak::emit_drain(emitter, 5, |emitter| {
        emit_close_open_aarch64(emitter, features);
    });
}

/// Emits the compiler-selected closer for the live record supplied by Memoir.
fn emit_close_open_aarch64(emitter: &mut Emitter, features: RuntimeFeatures) {
    emitter.instruction(&format!("ldr x10, [x9, #{SUBTYPE_OFFSET}]"));          // load the resource cleanup subtype
    emitter.instruction("cmp x10, #1");                                         // kind 1 owns a native stream descriptor
    emitter.instruction("b.eq __rt_resource_inventory_reset_stream");           // close a leaked stream descriptor directly
    if features.popen_resource {
        emitter.instruction("cmp x10, #3");                                     // kind 3 owns a popen pipe
        emitter.instruction("b.eq __rt_resource_inventory_reset_popen");        // close and reap the leaked pipe
    }
    if features.directory_resource {
        emitter.instruction("cmp x10, #4");                                     // kind 4 owns an open directory stream
        emitter.instruction("b.eq __rt_resource_inventory_reset_directory");    // release the leaked DIR pointer mapping
    }
    emitter.instruction("b __rt_resource_inventory_reset_free");                // other resource kinds have no inventory-owned destructor
    emitter.label("__rt_resource_inventory_reset_stream");
    emitter.instruction(&format!("ldr x0, [x9, #{PAYLOAD_OFFSET}]"));           // load the native descriptor payload
    emitter.instruction("cmp x0, #2");                                          // standard-stream aliases never own fd 0, 1, or 2
    emitter.instruction("b.ls __rt_resource_inventory_reset_free");             // preserve process stdio across web requests
    emitter.instruction("mov x10, #0x40000000");                                // synthetic and closed sentinels start at this unsigned value
    emitter.instruction("cmp x0, x10");                                         // skip synthetic or invalid descriptors
    emitter.instruction("b.hs __rt_resource_inventory_reset_free");             // no operating-system handle is owned
    emitter.syscall(6);                                                          // close the leaked descriptor on Darwin or Linux
    emitter.instruction("b __rt_resource_inventory_reset_free");                // continue with node reclamation
    if features.popen_resource {
        emitter.label("__rt_resource_inventory_reset_popen");
        emitter.instruction(&format!("ldr x0, [x9, #{PAYLOAD_OFFSET}]"));       // pass the leaked pipe descriptor
        emitter.instruction("mov x10, #0x40000000");                            // reject synthetic or invalid descriptors
        emitter.instruction("cmp x0, x10");                                     // reject synthetic pipe handles
        emitter.instruction("b.hs __rt_resource_inventory_reset_free");         // synthetic pipes own no descriptor
        abi::emit_call_label(emitter, "__rt_pclose");
        emitter.instruction("b __rt_resource_inventory_reset_free");            // reclaim the node after pipe cleanup
    }
    if features.directory_resource {
        emitter.label("__rt_resource_inventory_reset_directory");
        emitter.instruction(&format!("ldr x0, [x9, #{PAYLOAD_OFFSET}]"));       // pass the leaked directory descriptor
        emitter.instruction("mov x10, #0x40000000");                            // reject synthetic or invalid descriptors
        emitter.instruction("cmp x0, x10");                                     // reject synthetic directory handles
        emitter.instruction("b.hs __rt_resource_inventory_reset_free");         // synthetic directories own no descriptor
        abi::emit_call_label(emitter, "__rt_closedir");
    }
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
    emitter.instruction("b.le __rt_resource_inventory_register_standard");      // synthesized entries need no canonical inventory cell
    resource_weak::emit_find_registration_aarch64(emitter);
    emitter.label("__rt_resource_inventory_register_new");
    resource_weak::emit_append_node(emitter);
    emitter.label("__rt_resource_inventory_register_found");
    emitter.instruction("mov x0, x10");                                         // expose the canonical cell slot to the boxing caller
    emitter.instruction("b __rt_resource_inventory_register_done");             // preserve the node result
    emitter.label("__rt_resource_inventory_register_standard");
    emitter.instruction("mov x0, #0");                                          // synthesized resources have no node

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
    resource_weak::emit_mark_closed_aarch64(emitter);
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
    emitter.instruction(&format!("ldr x10, [x9, #{STATE_OFFSET}]"));            // load closed flag
    emitter.instruction(&format!("cmp x10, #{RETIRED}"));                       // destroyed resources no longer belong in PHP inventories
    emitter.instruction("b.eq __rt_get_resources_next");                        // skip dead incarnations even in the Unknown filter
    emitter.instruction("cbnz x10, __rt_get_resources_type_unknown");           // closed resources expose the Unknown type
    emitter.instruction(&format!("ldr x10, [x9, #{SUBTYPE_OFFSET}]"));          // load open resource subtype
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
    emitter.instruction(&format!("ldr x0, [x9, #{CELL_OFFSET}]"));              // retain the shared cell for the new hash owner
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("ldr x9, [sp, #16]");                                   // restore current node after retention
    emitter.instruction("ldr x0, [sp, #8]");                                    // destination hash
    emitter.instruction(&format!("ldr x1, [x9, #{ID_OFFSET}]"));                // integer key = PHP resource id
    emitter.instruction("mov x2, #-1");                                         // integer PHP id key
    emitter.instruction(&format!("ldr x3, [x9, #{CELL_OFFSET}]"));              // share the canonical Mixed resource cell
    emitter.instruction("mov x4, #0");                                          // boxed cells have no high payload word
    emitter.instruction("mov x5, #7");                                          // transfer the retained Mixed reference to the hash
    abi::emit_call_label(emitter, "__rt_hash_set");
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
fn emit_resource_inventory_x86_64(emitter: &mut Emitter, features: RuntimeFeatures) {
    emit_register_x86_64(emitter);
    emit_close_x86_64(emitter);
    emit_reset_x86_64(emitter, features);
    emit_insert_x86_64(emitter);
    emit_type_selector_x86_64(emitter);
    emit_get_resources_x86_64(emitter);
}

/// Routes native handle cleanup during the Memoir-owned inventory reset.
fn emit_reset_x86_64(emitter: &mut Emitter, features: RuntimeFeatures) {
    resource_weak::emit_drain(emitter, 5, |emitter| {
        emit_close_open_x86_64(emitter, features);
    });
}

/// Emits the compiler-selected closer for the live record supplied by Memoir.
fn emit_close_open_x86_64(emitter: &mut Emitter, features: RuntimeFeatures) {
    emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {SUBTYPE_OFFSET}]")); // load the resource cleanup subtype
    emitter.instruction("cmp r11, 1");                                          // kind 1 owns a native stream descriptor
    emitter.instruction("je __rt_resource_inventory_reset_stream_x86");         // close a leaked stream descriptor directly
    if features.popen_resource {
        emitter.instruction("cmp r11, 3");                                      // kind 3 owns a popen pipe
        emitter.instruction("je __rt_resource_inventory_reset_popen_x86");      // close and reap the leaked pipe
    }
    if features.directory_resource {
        emitter.instruction("cmp r11, 4");                                      // kind 4 owns an open directory stream
        emitter.instruction("je __rt_resource_inventory_reset_directory_x86");  // release the leaked DIR pointer mapping
    }
    emitter.instruction("jmp __rt_resource_inventory_reset_free_x86");          // other resource kinds have no inventory-owned destructor
    emitter.label("__rt_resource_inventory_reset_stream_x86");
    emitter.instruction(&format!("mov rdi, QWORD PTR [r10 + {PAYLOAD_OFFSET}]")); // load the native descriptor payload
    emitter.instruction("cmp rdi, 2");                                          // standard-stream aliases never own fd 0, 1, or 2
    emitter.instruction("jbe __rt_resource_inventory_reset_free_x86");          // preserve process stdio across web requests
    emitter.instruction("cmp rdi, 0x40000000");                                 // reject synthetic or invalid descriptors
    emitter.instruction("jae __rt_resource_inventory_reset_free_x86");          // no operating-system handle is owned
    abi::emit_call_label(emitter, "close");
    emitter.instruction("jmp __rt_resource_inventory_reset_free_x86");          // reclaim the node after stream cleanup
    if features.popen_resource {
        emitter.label("__rt_resource_inventory_reset_popen_x86");
        emitter.instruction(&format!("mov rdi, QWORD PTR [r10 + {PAYLOAD_OFFSET}]")); // pass the leaked pipe descriptor
        emitter.instruction("cmp rdi, 0x40000000");                             // reject synthetic or invalid descriptors
        emitter.instruction("jae __rt_resource_inventory_reset_free_x86");      // synthetic pipes own no descriptor
        abi::emit_call_label(emitter, "__rt_pclose");
        emitter.instruction("jmp __rt_resource_inventory_reset_free_x86");      // reclaim the node after pipe cleanup
    }
    if features.directory_resource {
        emitter.label("__rt_resource_inventory_reset_directory_x86");
        emitter.instruction(&format!("mov rdi, QWORD PTR [r10 + {PAYLOAD_OFFSET}]")); // pass the leaked directory descriptor
        emitter.instruction("cmp rdi, 0x40000000");                             // reject synthetic or invalid descriptors
        emitter.instruction("jae __rt_resource_inventory_reset_free_x86");      // synthetic directories own no descriptor
        abi::emit_call_label(emitter, "__rt_closedir");
    }
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
    emitter.instruction("jle __rt_resource_inventory_register_standard_x86");   // synthesized entries need no canonical inventory cell
    resource_weak::emit_find_registration_x86_64(emitter);
    emitter.label("__rt_resource_inventory_register_new_x86");
    resource_weak::emit_append_node(emitter);
    emitter.label("__rt_resource_inventory_register_found_x86");
    emitter.instruction("mov rax, r10");                                        // expose the canonical cell slot to the boxing caller
    emitter.instruction("jmp __rt_resource_inventory_register_done_x86");       // preserve the node result
    emitter.label("__rt_resource_inventory_register_standard_x86");
    emitter.instruction("xor eax, eax");                                        // synthesized resources have no node

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
    resource_weak::emit_mark_closed_x86_64(emitter);
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
    emitter.instruction(&format!("cmp QWORD PTR [r10 + {STATE_OFFSET}], {RETIRED}")); // destroyed incarnations are absent from every filter
    emitter.instruction("je __rt_get_resources_next_x86");                      // skip retired cells
    emitter.instruction(&format!("cmp QWORD PTR [r10 + {STATE_OFFSET}], 0"));   // inspect closed marker
    emitter.instruction("jne __rt_get_resources_type_unknown_x86");             // closed resources are Unknown
    emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {SUBTYPE_OFFSET}]")); // load open subtype
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
    emitter.instruction(&format!("mov rax, QWORD PTR [r10 + {CELL_OFFSET}]"));  // retain the shared cell for the new hash owner
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // restore current node after retention
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // destination hash
    emitter.instruction(&format!("mov rsi, QWORD PTR [r10 + {ID_OFFSET}]"));    // integer key = PHP id
    emitter.instruction("mov rdx, -1");                                         // integer PHP id key
    emitter.instruction(&format!("mov rcx, QWORD PTR [r10 + {CELL_OFFSET}]"));  // share the canonical Mixed resource cell
    emitter.instruction("xor r8d, r8d");                                        // boxed cells have no high payload word
    emitter.instruction("mov r9, 7");                                           // transfer the retained Mixed reference to the hash
    abi::emit_call_label(emitter, "__rt_hash_set");
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
    use crate::codegen_support::platform::{AppleVariant, Platform, Target};

    /// Verifies both supported architectures expose every inventory entry point.
    #[test]
    fn emits_complete_inventory_helper_family_on_both_architectures() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_inventory(&mut emitter, RuntimeFeatures::none());
            let asm = emitter.output();
            assert!(!asm.contains("__rt_stream_registry_cleanup"),
                "{target:?}: single-node application pruning must preserve stream registrations");
            for symbol in [
                "__rt_resource_inventory_register:",
                "__rt_resource_inventory_retire:",
                "__rt_resource_inventory_close:",
                "__rt_resource_inventory_reset:",
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
            emit_resource_inventory(&mut emitter, RuntimeFeatures::none());
            let asm = emitter.output();
            assert!(asm.contains("_resource_inventory_head"), "{target:?}");
            assert!(asm.contains("_resource_inventory_tail"), "{target:?}");
            assert!(asm.contains("__rt_get_resources_standard"), "{target:?}");
            assert!(asm.contains("__rt_get_resources_loop"), "{target:?}");
            assert!(asm.contains("__rt_resource_inventory_insert"), "{target:?}");
        }
    }

    /// Verifies reset emits live-handle cleanup for every inventory-owned resource kind.
    #[test]
    fn reset_closes_live_resource_handles_on_both_architectures() {
        let features = RuntimeFeatures {
            popen_resource: true,
            directory_resource: true,
            ..RuntimeFeatures::none()
        };
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_inventory(&mut emitter, features);
            let asm = emitter.output();
            for cleanup in ["close", "__rt_pclose", "__rt_closedir"] {
                assert!(
                    asm.contains(cleanup),
                    "missing reset cleanup {cleanup} for {target:?}"
                );
            }
            assert!(
                asm.contains("#32]") || asm.contains("+ 32]"),
                "missing closed-node guard for {target:?}"
            );
        }
    }

    /// Verifies leaked stream aliases cannot close process stdio during request reset.
    #[test]
    fn reset_preserves_standard_stream_aliases_on_both_architectures() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_inventory(&mut emitter, RuntimeFeatures::none());
            let asm = emitter.output();
            match target.arch {
                Arch::AArch64 => {
                    assert!(asm.contains("cmp x0, #2"), "{target:?}");
                    assert!(
                        asm.contains("b.ls __rt_resource_inventory_reset_free"),
                        "{target:?}"
                    );
                }
                Arch::X86_64 => {
                    assert!(asm.contains("cmp rdi, 2"), "{target:?}");
                    assert!(
                        asm.contains("jbe __rt_resource_inventory_reset_free_x86"),
                        "{target:?}"
                    );
                }
            }
        }
    }
}
