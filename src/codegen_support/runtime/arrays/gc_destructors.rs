//! Purpose:
//! Pins unreachable graph nodes while cycle-collector destructors inspect or mutate them.
//!
//! Called from:
//! - The native collector after root marking and before its final sweep.
//!
//! Key details:
//! - Snapshot chunks live outside the PHP heap and own one temporary reference per node.
//! - Root recounting discounts pins, so destructor resurrection is observed without false roots.
//! - Finished destructors use kind bit 17; temporary collector pins use kind bit 18.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits destructor snapshots, reachable-node unpinning, and snapshot disposal for every target.
pub fn emit_gc_destructors(emitter: &mut Emitter) {
    super::gc_exceptions::emit_gc_exception_helpers(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_destructors_aarch64(emitter);
            emit_unpin_aarch64(emitter);
        }
        Arch::X86_64 => {
            emit_destructors_x86_64(emitter);
            emit_unpin_x86_64(emitter);
        }
    }
}

/// Counts or pins the same ARM64 candidate set as the collector, without running user code.
fn emit_scan_aarch64(emitter: &mut Emitter, fill: bool) {
    let phase = if fill { "fill" } else { "count" };
    let loop_label = format!("__rt_gc_destructors_{phase}");
    let next = format!("{loop_label}_next");
    let ready = format!("{loop_label}_ready");
    let done = format!("{loop_label}_done");
    abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    abi::emit_load_symbol_to_reg(emitter, "x10", "_heap_off", 0);
    // The symbol load may use x9 as address scratch.
    abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("add x10, x9, x10");                                    // bound this allocation-free scan by the current heap extent
    emitter.label(&loop_label);
    emitter.instruction("cmp x9, x10");                                         // have all heap headers been inspected?
    emitter.instruction(&format!("b.hs {done}"));                               // finish before reading outside the managed heap
    emitter.instruction("ldr w11, [x9]");                                       // keep the payload size for the next header
    emitter.instruction("ldr w14, [x9, #4]");                                   // inspect the live owner count
    emitter.instruction(&format!("cbz w14, {next}"));                           // free-list blocks cannot be pinned
    emitter.instruction("ldr x12, [x9, #8]");                                   // inspect kind, reachability, and existing pins together
    emitter.instruction(&format!("tbnz x12, #16, {next}"));                     // externally reachable nodes need no destructor snapshot
    emitter.instruction(&format!("tbnz x12, #18, {next}"));                     // earlier snapshots already own this candidate
    emitter.instruction("and x13, x12, #0xff");                                 // isolate the heap kind
    emitter.instruction("cmp x13, #2");                                         // candidates start at indexed-array storage
    emitter.instruction(&format!("b.lo {next}"));                               // raw allocations and strings are not cycle candidates
    emitter.instruction("cmp x13, #5");                                         // candidates end at boxed Mixed storage
    emitter.instruction(&format!("b.ls {loop_label}_known"));                   // preserve the existing container candidate range
    emitter.instruction("cmp x13, #7");                                         // owned reference cells need pins during user destructors
    emitter.instruction(&format!("b.eq {ready}"));                              // protect cell storage while callbacks inspect aliases
    emitter.instruction(&format!("b {next}"));                                  // ignore other heap kinds
    emitter.label(&format!("{loop_label}_known"));
    emitter.instruction(&format!("b.hi {next}"));                               // match the collector's supported graph-node kinds
    emitter.instruction("cmp x13, #2");                                         // indexed arrays require reference-bearing elements
    emitter.instruction(&format!("b.ne {ready}"));                              // hashes, objects, and Mixed cells are graph nodes
    emitter.instruction("ubfx x15, x12, #8, #7");                               // inspect the indexed element storage tag
    emitter.instruction("cmp x15, #4");                                         // scalar and string arrays cannot own graph cycles
    emitter.instruction(&format!("b.lo {next}"));                               // leave non-reference arrays to their actual owners
    emitter.instruction("cmp x15, #7");                                         // accept array, hash, object, and Mixed elements
    emitter.instruction(&format!("b.hi {next}"));                               // unknown element layouts are not collector candidates
    emitter.label(&ready);
    if fill {
        emitter.instruction("add w14, w14, #1");                                // acquire one snapshot owner before any destructor can run
        emitter.instruction("str w14, [x9, #4]");                               // publish the temporary owner count
        emitter.instruction("orr x12, x12, #0x40000");                          // mark the artificial pin for root-count discounting
        emitter.instruction("str x12, [x9, #8]");                               // preserve all other heap metadata
        emitter.instruction("ldr x0, [sp]");                                    // recover this snapshot chunk
        emitter.instruction("ldr x1, [sp, #24]");                               // recover the next node slot
        emitter.instruction("add x0, x0, #16");                                 // skip the chunk link and node count
        emitter.instruction("add x2, x9, #16");                                 // store a payload pointer rather than its heap header
        emitter.instruction("str x2, [x0, x1, lsl #3]");                        // retain a stable identity across destructor-side allocations
        emitter.instruction("add x1, x1, #1");                                  // advance to the next snapshot slot
        emitter.instruction("str x1, [sp, #24]");                               // persist the fill index
    } else {
        emitter.instruction("ldr x0, [sp, #8]");                                // recover the candidate count
        emitter.instruction("add x0, x0, #1");                                  // reserve one snapshot slot for this node
        emitter.instruction("str x0, [sp, #8]");                                // persist the candidate count
        emitter.instruction("cmp x13, #4");                                     // only object nodes can need a PHP destructor
        emitter.instruction(&format!("b.ne {next}"));                           // non-object nodes only protect the object graph
        emitter.instruction(&format!("tbnz x12, #17, {next}"));                 // completed destructors must not trigger another pass
        emitter.instruction("mov x0, #1");                                      // record that a destructor pass is needed
        emitter.instruction("str x0, [sp, #16]");                               // avoid allocation when no new object needs processing
    }
    emitter.label(&next);
    emitter.instruction("add x9, x9, x11");                                     // skip the current payload
    emitter.instruction("add x9, x9, #16");                                     // skip the uniform heap header
    emitter.instruction(&format!("b {loop_label}"));                            // continue the allocation-free candidate scan
    emitter.label(&done);
}

/// Pins an ARM64 snapshot, runs its object destructors, and requests a fresh root recount.
fn emit_destructors_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_gc_destructors");
    emitter.instruction("sub sp, sp, #64");                                     // reserve chunk, count, flag, index, receiver, and saved frame slots
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller across PHP and C callbacks
    emitter.instruction("add x29, sp, #48");                                    // establish a stable callback frame
    emitter.instruction("str xzr, [sp, #8]");                                   // begin with no candidate nodes
    emitter.instruction("str xzr, [sp, #16]");                                  // begin with no pending object destructors
    emit_scan_aarch64(emitter, false);
    emitter.instruction("ldr x0, [sp, #16]");                                   // inspect whether this graph contains a new object candidate
    emitter.instruction("cbz x0, __rt_gc_destructors_return");                  // no new destructor means the collector can sweep
    emitter.instruction("ldr x0, [sp, #8]");                                    // size the snapshot from the counted candidate nodes
    emitter.instruction("lsl x0, x0, #3");                                      // reserve one raw pointer per node
    emitter.instruction("add x0, x0, #16");                                     // include the chunk link and node count
    emitter.bl_c("malloc");                                                     // keep snapshot storage outside the PHP collector's graph
    emitter.instruction("cbnz x0, __rt_gc_destructors_allocated");              // keep the conditional branch within this helper's range
    emitter.instruction("b __rt_heap_allocation_failed");                       // reach the shared failure entry with a full-range branch
    emitter.label("__rt_gc_destructors_allocated");
    emitter.instruction("str x0, [sp]");                                        // keep the new chunk across all callbacks
    abi::emit_symbol_address(emitter, "x10", "_gc_pin_head");
    emitter.instruction("ldr x11, [x10]");                                      // preserve earlier chunks from destructor-induced recounts
    emitter.instruction("str x11, [x0]");                                       // link this chunk to the existing snapshot list
    emitter.instruction("ldr x11, [sp, #8]");                                   // recover the number of node slots
    emitter.instruction("str x11, [x0, #8]");                                   // record the chunk's initialized-node count
    emitter.instruction("str x0, [x10]");                                       // publish the chunk for later unpinning and disposal
    emitter.instruction("str xzr, [sp, #24]");                                  // fill node slots from index zero
    emit_scan_aarch64(emitter, true);
    emitter.instruction("str xzr, [sp, #24]");                                  // restart iteration only after every candidate is pinned
    emitter.label("__rt_gc_destructors_objects");
    emitter.instruction("ldr x9, [sp, #24]");                                   // recover the current node index
    emitter.instruction("ldr x10, [sp, #8]");                                   // recover the number of initialized node pointers
    emitter.instruction("cmp x9, x10");                                         // have all snapshot nodes been considered?
    emitter.instruction("b.hs __rt_gc_destructors_recount");                    // changes made by destructors require fresh root analysis
    emitter.instruction("ldr x10, [sp]");                                       // recover the current snapshot chunk
    emitter.instruction("add x10, x10, #16");                                   // skip the chunk header
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // borrow the pinned node's stable payload pointer
    emitter.instruction("ldr x10, [x0, #-8]");                                  // inspect the node kind without following properties
    emitter.instruction("and x11, x10, #0xff");                                 // isolate the graph-node kind
    emitter.instruction("cmp x11, #4");                                         // only objects run PHP destructors
    emitter.instruction("b.ne __rt_gc_destructors_objects_next");               // array, hash, and Mixed pins only preserve data
    emitter.instruction("tbnz x10, #17, __rt_gc_destructors_objects_next");     // a finished destructor is never invoked twice
    emitter.instruction("str x0, [sp, #32]");                                   // root the receiver identity across callbacks
    emitter.instruction("bl __rt_gc_destructor_begin");                         // measure the destructor phase independently of sweeping
    emitter.instruction("ldr x0, [sp, #32]");                                   // recover the raw object argument after timing
    emitter.instruction("bl __rt_gc_protected_destructor");                     // capture throws while every candidate still owns its properties
    emitter.instruction("bl __rt_gc_destructor_end");                           // finish the measured destructor interval
    emitter.instruction("ldr x9, [sp, #32]");                                   // recover the pinned receiver after user code
    emitter.instruction("ldr x10, [x9, #-8]");                                  // preserve collector marks and the snapshot pin
    emitter.instruction("orr x10, x10, #0x20000");                              // remember completion independently of the real owner count
    emitter.instruction("str x10, [x9, #-8]");                                  // prevent a later sweep or last-owner release from rerunning it
    emitter.instruction("ldr w10, [x9, #-12]");                                 // inspect owners accumulated or removed by the destructor
    emitter.instruction("and w10, w10, #0x7fffffff");                           // remove the temporary reentrancy guard but keep real owners
    emitter.instruction("str w10, [x9, #-12]");                                 // allow the next root pass to observe resurrection accurately
    emitter.label("__rt_gc_destructors_objects_next");
    emitter.instruction("ldr x9, [sp, #24]");                                   // recover the iteration position
    emitter.instruction("add x9, x9, #1");                                      // advance to the next stable node identity
    emitter.instruction("str x9, [sp, #24]");                                   // persist the iteration position
    emitter.instruction("b __rt_gc_destructors_objects");                       // continue without walking mutable heap headers
    emitter.label("__rt_gc_destructors_recount");
    emitter.instruction("mov x0, #1");                                          // request new reachability analysis before freeing any pinned graph
    emitter.label("__rt_gc_destructors_return");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the collector frame
    emitter.instruction("add sp, sp, #64");                                     // release local snapshot iteration state
    emitter.instruction("ret");                                                 // return zero for sweep or one for recount
}

/// Releases ARM64 pins on reachable nodes before sweeping, then disposes chunks after sweeping.
fn emit_unpin_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_gc_unpin_reachable");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_gc_pin_head", 0);
    emitter.label("__rt_gc_unpin_chunk");
    emitter.instruction("cbz x0, __rt_gc_unpin_done");                          // finish after the last snapshot chunk
    emitter.instruction("ldr x1, [x0, #8]");                                    // inspect this chunk's node count
    emitter.instruction("mov x2, #0");                                          // visit every initialized node pointer
    emitter.instruction("add x3, x0, #16");                                     // locate the node pointer vector
    emitter.label("__rt_gc_unpin_node");
    emitter.instruction("cmp x2, x1");                                          // have all nodes in this chunk been inspected?
    emitter.instruction("b.hs __rt_gc_unpin_next_chunk");                       // move to the previous snapshot chunk
    emitter.instruction("ldr x4, [x3, x2, lsl #3]");                            // every pin still guarantees a live header before sweep
    emitter.instruction("ldr x5, [x4, #-8]");                                   // inspect refreshed reachability
    emitter.instruction("tbz x5, #16, __rt_gc_unpin_next_node");                // doomed nodes keep their pins until direct reclamation
    emitter.instruction("ldr w6, [x4, #-12]");                                  // reachable nodes have real owners in addition to the pin
    emitter.instruction("sub w6, w6, #1");                                      // remove only the artificial snapshot owner
    emitter.instruction("str w6, [x4, #-12]");                                  // no reachable node can reach zero here
    emitter.instruction("bic x5, x5, #0x40000");                                // remove the temporary pin marker from surviving storage
    emitter.instruction("str x5, [x4, #-8]");                                   // retain destructor completion and all ordinary metadata
    emitter.label("__rt_gc_unpin_next_node");
    emitter.instruction("add x2, x2, #1");                                      // advance to the next snapshot identity
    emitter.instruction("b __rt_gc_unpin_node");                                // continue without releasing any real graph edge
    emitter.label("__rt_gc_unpin_next_chunk");
    emitter.instruction("ldr x0, [x0]");                                        // follow the previous snapshot chunk
    emitter.instruction("b __rt_gc_unpin_chunk");                               // visit nodes pinned by earlier destructor passes
    emitter.label("__rt_gc_unpin_done");
    emitter.instruction("ret");                                                 // unreachable pins will disappear with their reclaimed headers

    emitter.label_global("__rt_gc_drop_pins");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the collector across C allocator calls
    emitter.instruction("mov x29, sp");                                         // establish a valid C-call frame
    emitter.label("__rt_gc_drop_pin_chunk");
    abi::emit_symbol_address(emitter, "x9", "_gc_pin_head");
    emitter.instruction("ldr x0, [x9]");                                        // load the next chunk without touching reclaimed PHP nodes
    emitter.instruction("cbz x0, __rt_gc_drop_pins_done");                      // every chunk has been disposed
    emitter.instruction("ldr x10, [x0]");                                       // retain the list successor before freeing this chunk
    emitter.instruction("str x10, [x9]");                                       // remove the chunk from collector state
    emitter.bl_c("free");                                                       // release only C-allocated snapshot memory
    emitter.instruction("b __rt_gc_drop_pin_chunk");                            // continue until collector pin state is empty
    emitter.label("__rt_gc_drop_pins_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the collector's frame and return address
    emitter.instruction("ret");                                                 // the next collection starts with no snapshot owners
}

/// Counts or pins x86_64 graph candidates without calling allocators or user code mid-scan.
fn emit_scan_x86_64(emitter: &mut Emitter, fill: bool) {
    let phase = if fill { "fill" } else { "count" };
    let loop_label = format!("__rt_gc_destructors_{phase}");
    let next = format!("{loop_label}_next");
    let ready = format!("{loop_label}_ready");
    let done = format!("{loop_label}_done");
    abi::emit_symbol_address(emitter, "r8", "_heap_buf");
    abi::emit_load_symbol_to_reg(emitter, "r9", "_heap_off", 0);
    emitter.instruction("add r9, r8");                                          // capture the allocation-free scan's heap limit
    emitter.label(&loop_label);
    emitter.instruction("cmp r8, r9");                                          // have all headers in the current heap been inspected?
    emitter.instruction(&format!("jae {done}"));                                // never read beyond the managed heap
    emitter.instruction("mov r10d, DWORD PTR [r8]");                            // preserve the payload size for advancement
    emitter.instruction("cmp DWORD PTR [r8 + 4], 0");                           // distinguish live graph nodes from free-list entries
    emitter.instruction(&format!("je {next}"));                                 // free storage owns no destructor data
    emitter.instruction("mov r11, QWORD PTR [r8 + 8]");                         // retain the full kind word including collector flags
    emitter.instruction("test r11, 0x50000");                                   // inspect reachability and an existing snapshot pin
    emitter.instruction(&format!("jnz {next}"));                                // root-reachable and already pinned nodes need no new pin
    emitter.instruction("mov rcx, r11");                                        // preserve the full kind while inspecting its low byte
    emitter.instruction("and ecx, 0xff");                                       // isolate the uniform heap kind
    emitter.instruction("cmp ecx, 2");                                          // graph candidates start at indexed arrays
    emitter.instruction(&format!("jb {next}"));                                 // raw storage and strings are not cycle candidates
    emitter.instruction("cmp ecx, 5");                                          // graph candidates end at boxed Mixed cells
    emitter.instruction(&format!("jbe {loop_label}_known"));                    // preserve the existing container candidate range
    emitter.instruction("cmp ecx, 7");                                          // owned reference cells need pins during user destructors
    emitter.instruction(&format!("je {ready}"));                                // protect cell storage while callbacks inspect aliases
    emitter.instruction(&format!("jmp {next}"));                                // ignore other heap kinds
    emitter.label(&format!("{loop_label}_known"));
    emitter.instruction(&format!("ja {next}"));                                 // mirror the collector's candidate-kind range
    emitter.instruction("cmp ecx, 2");                                          // indexed arrays need reference-bearing element storage
    emitter.instruction(&format!("jne {ready}"));                               // other supported kinds already own graph edges
    emitter.instruction("mov rdx, r11");                                        // retain the full kind while inspecting array element storage
    emitter.instruction("shr rdx, 8");                                          // move the element tag into the low bits
    emitter.instruction("and edx, 0x7f");                                       // discard persistent-array flags
    emitter.instruction("cmp edx, 4");                                          // scalar and string elements do not form cycles
    emitter.instruction(&format!("jb {next}"));                                 // leave non-reference arrays to their ordinary owners
    emitter.instruction("cmp edx, 7");                                          // accept array, hash, object, and Mixed elements
    emitter.instruction(&format!("ja {next}"));                                 // reject unknown indexed storage tags
    emitter.label(&ready);
    if fill {
        emitter.instruction("add DWORD PTR [r8 + 4], 1");                       // acquire a temporary owner before running any destructor
        emitter.instruction("or QWORD PTR [r8 + 8], 0x40000");                  // mark the owner for root-count discounting
        emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                    // recover the allocated snapshot chunk
        emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                   // recover the next node slot
        emitter.instruction("lea rdx, [r8 + 16]");                              // capture the stable payload identity
        emitter.instruction("mov QWORD PTR [rcx + rax * 8 + 16], rdx");         // store the pin independently of heap iteration order
        emitter.instruction("add QWORD PTR [rbp - 32], 1");                     // advance the snapshot fill position
    } else {
        emitter.instruction("add QWORD PTR [rbp - 16], 1");                     // reserve one pointer slot for this graph candidate
        emitter.instruction("cmp ecx, 4");                                      // only objects need a PHP destructor pass
        emitter.instruction(&format!("jne {next}"));                            // other pins solely preserve reachable destructor data
        emitter.instruction("test r11, 0x20000");                               // inspect persistent destructor completion
        emitter.instruction(&format!("jnz {next}"));                            // completed destructors never request another pass
        emitter.instruction("mov QWORD PTR [rbp - 24], 1");                     // allocate a snapshot only when a new object needs processing
    }
    emitter.label(&next);
    emitter.instruction("lea r8, [r8 + r10 + 16]");                             // advance past the payload and its uniform header
    emitter.instruction(&format!("jmp {loop_label}"));                          // continue the allocation-free graph scan
    emitter.label(&done);
}

/// Pins an x86_64 snapshot and executes object destructors before asking the collector to recount.
fn emit_destructors_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_gc_destructors");
    emitter.instruction("push rbp");                                            // preserve and align the caller's frame
    emitter.instruction("mov rbp, rsp");                                        // establish stable slots across C and PHP calls
    emitter.instruction("sub rsp, 48");                                         // reserve chunk, count, flag, index, and receiver slots
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // initialize the candidate count
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the pending-destructor flag
    emit_scan_x86_64(emitter, false);
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // inspect whether new object candidates need processing
    emitter.instruction("test rax, rax");                                       // no pending object means no snapshot allocation
    emitter.instruction("jz __rt_gc_destructors_return");                       // allow sweeping after the final root analysis
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // count one pointer slot per unpinned candidate
    emitter.instruction("lea rdi, [rdi * 8 + 16]");                             // include the chunk link and node count
    emitter.bl_c("malloc");                                                     // allocate outside the managed PHP heap
    emitter.instruction("test rax, rax");                                       // verify destructor data can be protected
    emitter.instruction("jz __rt_heap_allocation_failed");                      // use the shared allocation-failure entry across emitter scopes
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the chunk across callbacks
    abi::emit_symbol_address(emitter, "r10", "_gc_pin_head");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // retain snapshots from earlier destructor passes
    emitter.instruction("mov QWORD PTR [rax], r11");                            // link this chunk to the preceding chunk
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // recover the number of initialized node pointers
    emitter.instruction("mov QWORD PTR [rax + 8], r11");                        // record the node count for later unpinning
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the chunk before any user callback
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // fill snapshot entries from zero
    emit_scan_x86_64(emitter, true);
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // restart only after all candidate owners have been acquired
    emitter.label("__rt_gc_destructors_objects");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // recover the current node index
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // have all snapshot identities been inspected?
    emitter.instruction("jae __rt_gc_destructors_recount");                     // destructor changes require fresh root analysis
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // recover the chunk containing stable payload pointers
    emitter.instruction("mov rdi, QWORD PTR [rcx + rax * 8 + 16]");             // borrow the next pinned node
    emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                        // inspect kind and destructor completion together
    emitter.instruction("mov r11, r10");                                        // preserve the full flags while inspecting the kind
    emitter.instruction("and r11d, 0xff");                                      // isolate the graph-node kind
    emitter.instruction("cmp r11d, 4");                                         // only objects execute PHP destructors
    emitter.instruction("jne __rt_gc_destructors_objects_next");                // other graph nodes only preserve data
    emitter.instruction("test r10, 0x20000");                                   // inspect persistent destructor completion
    emitter.instruction("jnz __rt_gc_destructors_objects_next");                // never run a completed destructor twice
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // preserve the receiver identity across timing and user code
    emitter.instruction("call __rt_gc_destructor_begin");                       // measure destructors separately from heap sweeping
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // restore the raw object argument after timing
    emitter.instruction("call __rt_gc_protected_destructor");                   // capture throws while every candidate has a snapshot owner
    emitter.instruction("call __rt_gc_destructor_end");                         // finish the measured destructor interval
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // recover the protected object after callback clobbers
    emitter.instruction("or QWORD PTR [rax - 8], 0x20000");                     // persist completion independently of live owner counts
    emitter.instruction("and DWORD PTR [rax - 12], 0x7fffffff");                // clear only the temporary destructor reentrancy guard
    emitter.label("__rt_gc_destructors_objects_next");
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance to the next stable snapshot identity
    emitter.instruction("jmp __rt_gc_destructors_objects");                     // never walk mutable heap headers during user callbacks
    emitter.label("__rt_gc_destructors_recount");
    emitter.instruction("mov eax, 1");                                          // request a fresh mark pass before freeing candidate storage
    emitter.label("__rt_gc_destructors_return");
    emitter.instruction("leave");                                               // release helper slots and restore the collector frame
    emitter.instruction("ret");                                                 // return zero for sweep or one for recount
}

/// Removes x86_64 pins from newly reachable nodes and separately frees snapshot chunks after sweep.
fn emit_unpin_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_gc_unpin_reachable");
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_gc_pin_head", 0);
    emitter.label("__rt_gc_unpin_chunk");
    emitter.instruction("test rdi, rdi");                                       // inspect the next snapshot chunk
    emitter.instruction("jz __rt_gc_unpin_done");                               // finish after all chunks have been visited
    emitter.instruction("mov rsi, QWORD PTR [rdi + 8]");                        // read the initialized-node count
    emitter.instruction("xor edx, edx");                                        // start from the first node pointer
    emitter.label("__rt_gc_unpin_node");
    emitter.instruction("cmp rdx, rsi");                                        // have all nodes in the current chunk been inspected?
    emitter.instruction("jae __rt_gc_unpin_next_chunk");                        // advance through the snapshot list
    emitter.instruction("mov rax, QWORD PTR [rdi + rdx * 8 + 16]");             // every snapshot owner still guarantees a live PHP header
    emitter.instruction("test QWORD PTR [rax - 8], 0x10000");                   // inspect the refreshed reachable mark
    emitter.instruction("jz __rt_gc_unpin_next_node");                          // unreachable nodes keep their pins until direct sweep
    emitter.instruction("sub DWORD PTR [rax - 12], 1");                         // remove the artificial owner while real reachable owners remain
    emitter.instruction("and QWORD PTR [rax - 8], -262145");                    // clear only the collector-pin bit while retaining the heap marker
    emitter.label("__rt_gc_unpin_next_node");
    emitter.instruction("add rdx, 1");                                          // advance to the next initialized snapshot entry
    emitter.instruction("jmp __rt_gc_unpin_node");                              // preserve all real graph references during unpinning
    emitter.label("__rt_gc_unpin_next_chunk");
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // follow the previous destructor-pass snapshot
    emitter.instruction("jmp __rt_gc_unpin_chunk");                             // inspect all pinned candidates before sweeping
    emitter.label("__rt_gc_unpin_done");
    emitter.instruction("ret");                                                 // doomed pins disappear with reclaimed heap headers

    emitter.label_global("__rt_gc_drop_pins");
    emitter.instruction("push rbp");                                            // preserve the collector and align C allocator calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable C-call frame
    emitter.label("__rt_gc_drop_pin_chunk");
    abi::emit_symbol_address(emitter, "r10", "_gc_pin_head");
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // read only C-allocated chunks after PHP nodes have been reclaimed
    emitter.instruction("test rdi, rdi");                                       // inspect whether snapshot storage remains
    emitter.instruction("jz __rt_gc_drop_pins_done");                           // finish with empty collector pin state
    emitter.instruction("mov r11, QWORD PTR [rdi]");                            // retain the next chunk before freeing this allocation
    emitter.instruction("mov QWORD PTR [r10], r11");                            // detach the chunk from collector state
    emitter.bl_c("free");                                                       // release the independent snapshot allocation
    emitter.instruction("jmp __rt_gc_drop_pin_chunk");                          // dispose all chunks without dereferencing reclaimed PHP pointers
    emitter.label("__rt_gc_drop_pins_done");
    emitter.instruction("pop rbp");                                             // restore the collector's frame
    emitter.instruction("ret");                                                 // later collections start without stale snapshot entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target pins before callbacks and recounts before unpinning or freeing graph nodes.
    #[test]
    fn gc_destructor_snapshots_cover_every_supported_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_gc_destructors(&mut emitter);
            let assembly = emitter.output();
            for symbol in ["__rt_gc_destructors", "__rt_gc_unpin_reachable", "__rt_gc_drop_pins"] {
                assert!(assembly.contains(&format!("{symbol}:")), "{name}: {symbol}");
            }
            let destructor_loop = assembly.split("__rt_gc_destructors:\n").nth(1).unwrap();
            assert!(destructor_loop.find("__rt_gc_destructors_fill_done:").unwrap()
                < destructor_loop.find("__rt_gc_protected_destructor").unwrap(), "{name}");
            assert!(assembly.contains(&target.extern_symbol("malloc")), "{name}");
            assert!(assembly.contains(&target.extern_symbol("free")), "{name}");
            assert!(assembly.contains("__rt_heap_allocation_failed"), "{name}");
            let dispose = assembly.split("__rt_gc_drop_pins:").nth(1).unwrap();
            assert!(!dispose.contains("__rt_decref"), "{name}");

            let mut emitter = Emitter::new(target);
            super::super::emit_gc_collect_cycles(&mut emitter);
            let assembly = emitter.output();
            let destroy = assembly.find("__rt_gc_destructors").unwrap();
            let after_destroy = &assembly[destroy..];
            assert!(after_destroy.find("__rt_gc_collect_cycles_recount").unwrap()
                < after_destroy.find("__rt_gc_unpin_reachable").unwrap(), "{name}");
            assert!(after_destroy.find("__rt_gc_unpin_reachable").unwrap()
                < after_destroy.find("__rt_gc_collect_cycles_free_loop:").unwrap(), "{name}");
            assert!(assembly.contains("_gc_freeing_unreachable"), "{name}");
            assert!(assembly.rfind("_gc_collecting").unwrap()
                < assembly.find("__rt_gc_rethrow_pending").unwrap(), "{name}");
            assert!(assembly.find("__rt_gc_drop_pins").unwrap()
                < assembly.find("__rt_gc_rethrow_pending").unwrap(), "{name}");
        }
    }
}
