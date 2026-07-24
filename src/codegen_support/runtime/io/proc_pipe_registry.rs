//! Purpose:
//! Emits the process-to-pipe ownership registry used by `proc_close()`.
//!
//! Called from:
//! - `__rt_proc_open`'s EIR lowerer after it has published `$pipes`, and from
//!   `__rt_proc_close` immediately before the platform wait primitive.
//!
//! Key details:
//! - A registry node retains the exact `$pipes` container, so close-time lookup
//!   reaches the original kind-1 resource boxes even after a PHP COW copy.
//! - Every selected pipe box receives the `-1` release sentinel before its fd is
//!   closed; later scope cleanup therefore cannot close a reused descriptor.
//! - Nodes are raw heap allocations and are removed before the process wait.
//!   `__rt_heap_alloc` terminates through `__rt_heap_exhausted` on failure, so a
//!   successful `proc_open` result can never escape without a registry node.

use crate::codegen_support::{abi, emit::Emitter, platform::{Arch, Platform}};

/// Emits the register and pre-wait pipe-close helpers for every supported ABI.
pub fn emit_proc_pipe_registry(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_proc_pipe_registry_aarch64(emitter),
        Arch::X86_64 => emit_proc_pipe_registry_x86_64(emitter),
    }
}

/// Emits AArch64 registry helpers shared by macOS and Linux.
///
/// Nodes are `[next, process, retained_pipes]`. The register helper takes
/// `(x0=process, x1=pipes)` and returns the same pair; the close helper takes
/// and returns `x0=process`, making it transparent to `__rt_proc_close`.
fn emit_proc_pipe_registry_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: proc pipe registry ---");
    emitter.label_global("__rt_proc_pipe_registry_register");

    // -- retain a process' published pipe container in a raw registry node --
    emitter.instruction("cmn x0, #1");                                          // proc_open failure sentinel has no pipes to register
    emitter.instruction("b.eq __rt_proc_pipe_registry_register_done");          // preserve the failure pair unchanged
    emitter.instruction("sub sp, sp, #48");                                     // reserve stable process, pipes, and node slots
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the registry helper frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // preserve process and pipes across ownership calls
    emitter.instruction("mov x0, x1");                                          // pass pipes to the generic incref helper
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("mov x0, #24");                                         // three machine words form one raw registry node
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction("str x0, [sp, #16]");                                   // retain the node pointer across global publication
    abi::emit_symbol_address(emitter, "x9", "_proc_pipe_registry_head");
    emitter.instruction("ldr x10, [x9]");                                       // load the old registry head
    emitter.instruction("str x10, [x0]");                                       // prepend this node to the linked registry
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the raw process descriptor
    emitter.instruction("str x10, [x0, #8]");                                   // node.process = raw pid or process HANDLE
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the published pipes container
    emitter.instruction("str x10, [x0, #16]");                                  // node owns one container reference
    emitter.instruction("str x0, [x9]");                                        // publish the new registry head
    emitter.instruction("ldp x0, x1, [sp, #0]");                                // return the unchanged proc_open result pair
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the registry frame
    emitter.instruction("ret");                                                 // return process and pipes to the lowerer
    emitter.label("__rt_proc_pipe_registry_register_done");
    emitter.instruction("ret");                                                 // leave proc_open failure untouched

    emitter.label_global("__rt_proc_pipe_registry_close");
    // -- find and unlink the node for this exact process resource --
    emitter.instruction("sub sp, sp, #112");                                    // reserve registry traversal and iterator state
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the close helper frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the raw process descriptor for return/wait
    abi::emit_symbol_address(emitter, "x9", "_proc_pipe_registry_head");
    emitter.instruction("ldr x10, [x9]");                                       // start at the registry head
    emitter.instruction("str xzr, [sp, #8]");                                   // previous node = null
    emitter.label("__rt_proc_pipe_registry_find");
    emitter.instruction("cbz x10, __rt_proc_pipe_registry_done");               // no node means no pipe ownership was recorded
    emitter.instruction("ldr x11, [x10, #8]");                                  // load this node's process descriptor
    emitter.instruction("ldr x12, [sp, #0]");                                   // reload the requested process descriptor
    emitter.instruction("cmp x11, x12");                                        // is this the matching process node?
    emitter.instruction("b.eq __rt_proc_pipe_registry_found");                  // unlink and drain this node
    emitter.instruction("str x10, [sp, #8]");                                   // advance previous node before walking onward
    emitter.instruction("ldr x10, [x10]");                                      // follow the next linked registry node
    emitter.instruction("b __rt_proc_pipe_registry_find");                      // continue the process lookup

    emitter.label("__rt_proc_pipe_registry_found");
    emitter.instruction("str x10, [sp, #16]");                                  // retain the matched raw node
    emitter.instruction("ldr x11, [x10]");                                      // load the matched node's successor
    emitter.instruction("ldr x12, [sp, #8]");                                   // reload the previous node pointer
    emitter.instruction("cbnz x12, __rt_proc_pipe_registry_link_prev");         // non-head nodes relink through their predecessor
    abi::emit_symbol_address(emitter, "x9", "_proc_pipe_registry_head");
    emitter.instruction("str x11, [x9]");                                       // remove the head node from the global list
    emitter.instruction("b __rt_proc_pipe_registry_linked");                    // continue with the retained container
    emitter.label("__rt_proc_pipe_registry_link_prev");
    emitter.instruction("str x11, [x12]");                                      // bypass the matched node in the linked list
    emitter.label("__rt_proc_pipe_registry_linked");
    emitter.instruction("ldr x9, [x10, #16]");                                  // load the registry-owned pipes container
    emitter.instruction("str x9, [sp, #24]");                                   // preserve pipes through iterator/accessor calls
    emitter.instruction("ldr x10, [x9, #-8]");                                  // inspect the indexed/hash storage kind
    emitter.instruction("and x10, x10, #0xff");                                 // isolate the low-byte container kind
    emitter.instruction("str x10, [sp, #48]");                                  // retain kind: 2 indexed, 3 associative hash
    emitter.instruction("ldr x10, [x9]");                                       // load the number of published pipe entries
    emitter.instruction("str x10, [sp, #40]");                                  // retain total entry count
    emitter.instruction("str xzr, [sp, #32]");                                  // processed entries = 0
    emitter.instruction("str xzr, [sp, #56]");                                  // hash insertion-order cursor = 0
    emitter.label("__rt_proc_pipe_registry_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload processed-entry count
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload container entry count
    emitter.instruction("cmp x9, x10");                                         // have all published pipe entries been visited?
    emitter.instruction("b.ge __rt_proc_pipe_registry_release");                // release the retained container once draining is complete
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload the storage kind
    emitter.instruction("cmp x10, #3");                                         // does the container use hash storage?
    emitter.instruction("b.eq __rt_proc_pipe_registry_hash_key");               // hashes require insertion-order key iteration
    emitter.instruction("str x9, [sp, #64]");                                   // indexed key is the current zero-based entry index
    emitter.instruction("mov x10, #-1");                                        // integer-key high-word sentinel
    emitter.instruction("str x10, [sp, #72]");                                  // preserve the indexed key representation
    emitter.instruction("add x9, x9, #1");                                      // advance indexed position before the accessor call
    emitter.instruction("str x9, [sp, #32]");                                   // persist processed-entry count
    emitter.instruction("b __rt_proc_pipe_registry_get");                       // fetch the exact resource box by its key
    emitter.label("__rt_proc_pipe_registry_hash_key");
    emitter.instruction("ldr x0, [sp, #24]");                                   // pass the retained hash container
    emitter.instruction("ldr x1, [sp, #56]");                                   // pass the prior insertion-order cursor
    abi::emit_call_label(emitter, "__rt_hash_iter_next");
    emitter.instruction("str x0, [sp, #56]");                                   // persist the next hash cursor
    emitter.instruction("str x1, [sp, #64]");                                   // retain integer/string key low payload
    emitter.instruction("str x2, [sp, #72]");                                   // retain key high payload or integer sentinel
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload processed-entry count
    emitter.instruction("add x9, x9, #1");                                      // consume this hash entry
    emitter.instruction("str x9, [sp, #32]");                                   // persist processed-entry count
    emitter.label("__rt_proc_pipe_registry_get");
    emitter.instruction("ldr x0, [sp, #24]");                                   // pass the retained pipes container to the generic accessor
    emitter.instruction("ldr x1, [sp, #64]");                                   // pass the original descriptor key low payload
    emitter.instruction("ldr x2, [sp, #72]");                                   // pass its key high payload/sentinel
    emitter.instruction("mov x3, #0");                                          // suppress missing-key diagnostics during teardown
    abi::emit_call_label(emitter, "__rt_array_get_mixed_key");
    emitter.instruction("str x0, [sp, #80]");                                   // retain the accessor-owned result box
    emitter.instruction("cbz x0, __rt_proc_pipe_registry_loop");                // a malformed/missing entry needs no close action
    emitter.instruction("ldr x9, [x0]");                                        // inspect the boxed runtime value tag
    emitter.instruction("cmp x9, #9");                                          // is this a resource value?
    emitter.instruction("b.ne __rt_proc_pipe_registry_drop_box");               // only resources can own process pipes
    emitter.instruction("ldr x9, [x0, #16]");                                   // inspect the resource subtype
    emitter.instruction("cmp x9, #1");                                          // kind 1 denotes a native stream descriptor
    emitter.instruction("b.ne __rt_proc_pipe_registry_drop_box");               // leave non-pipe resources untouched
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the raw pipe descriptor
    emitter.instruction("mov x10, #0x40000000");                                // sentinel/synthetic descriptor threshold
    emitter.instruction("cmp x9, x10");                                         // has user code already closed this exact resource?
    emitter.instruction("b.hs __rt_proc_pipe_registry_drop_box");               // skip sentinel and synthetic stream values
    emitter.instruction("mov x10, #-1");                                        // explicit-release sentinel prevents later double close
    emitter.instruction("str x10, [x0, #8]");                                   // mark the exact shared resource box closed
    emitter.instruction("mov x0, x9");                                          // pass the raw descriptor to the platform close shim
    emit_proc_pipe_close_aarch64(emitter);
    emitter.label("__rt_proc_pipe_registry_drop_box");
    emitter.instruction("ldr x0, [sp, #80]");                                   // reload the accessor-owned mixed result
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    emitter.instruction("b __rt_proc_pipe_registry_loop");                      // continue closing every registered pipe resource

    emitter.label("__rt_proc_pipe_registry_release");
    emitter.instruction("ldr x0, [sp, #24]");                                   // release the registry's retained pipes container
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.instruction("ldr x0, [sp, #16]");                                   // release the raw linked-list node
    abi::emit_call_label(emitter, "__rt_heap_free");
    emitter.label("__rt_proc_pipe_registry_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the unchanged process descriptor to proc_close
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the close helper frame
    emitter.instruction("ret");                                                 // continue into the platform process wait
}

/// Emits the x86_64 registry helpers shared by Linux and Windows.
///
/// Internal runtime helpers use the established SysV-style convention on both
/// x86_64 targets. Register input/output is `(rdi,rsi)` / `(rax,rdx)` and close
/// input/output is `rdi` / `rax`.
fn emit_proc_pipe_registry_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: proc pipe registry ---");
    emitter.label_global("__rt_proc_pipe_registry_register");
    emitter.instruction("cmp rdi, -1");                                         // proc_open failure sentinel has no pipes to register
    emitter.instruction("je __rt_proc_pipe_registry_register_done_x86");        // preserve the failure pair unchanged
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the registry helper frame
    emitter.instruction("sub rsp, 32");                                         // reserve process, pipes, and node spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the raw process descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the published pipes container
    emitter.instruction("mov rax, rsi");                                        // pass pipes to the generic incref helper
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.instruction("mov rax, 24");                                         // three machine words form one raw registry node
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // retain the allocated node pointer
    abi::emit_symbol_address(emitter, "r10", "_proc_pipe_registry_head");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the old registry head
    emitter.instruction("mov QWORD PTR [rax], r11");                            // prepend this node to the linked registry
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the raw process descriptor
    emitter.instruction("mov QWORD PTR [rax + 8], r11");                        // node.process = raw pid or process HANDLE
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the published pipes container
    emitter.instruction("mov QWORD PTR [rax + 16], r11");                       // node owns one container reference
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the new registry head
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return process in the normal result register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // return pipes in the paired result register
    emitter.instruction("add rsp, 32");                                         // release registry spill storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the unchanged proc_open result pair
    emitter.label("__rt_proc_pipe_registry_register_done_x86");
    emitter.instruction("mov rax, rdi");                                        // preserve failure descriptor in the normal result register
    emitter.instruction("mov rdx, rsi");                                        // preserve pipes in the paired result register
    emitter.instruction("ret");                                                 // leave proc_open failure untouched

    emitter.label_global("__rt_proc_pipe_registry_close");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the close helper frame
    emitter.instruction("sub rsp, 96");                                         // reserve registry traversal and iterator state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the raw process descriptor for return/wait
    abi::emit_symbol_address(emitter, "r10", "_proc_pipe_registry_head");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // start at the registry head
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // previous node = null
    emitter.label("__rt_proc_pipe_registry_find_x86");
    emitter.instruction("test r11, r11");                                       // did the registry walk reach its end?
    emitter.instruction("jz __rt_proc_pipe_registry_done_x86");                 // no node means no pipe ownership was recorded
    emitter.instruction("mov rax, QWORD PTR [r11 + 8]");                        // load this node's process descriptor
    emitter.instruction("cmp rax, QWORD PTR [rbp - 8]");                        // is this the matching process node?
    emitter.instruction("je __rt_proc_pipe_registry_found_x86");                // unlink and drain this node
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // advance previous node before walking onward
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // follow the next linked registry node
    emitter.instruction("jmp __rt_proc_pipe_registry_find_x86");                // continue the process lookup

    emitter.label("__rt_proc_pipe_registry_found_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // retain the matched raw node
    emitter.instruction("mov rax, QWORD PTR [r11]");                            // load the matched node's successor
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the previous node pointer
    emitter.instruction("test r10, r10");                                       // is the matched node the list head?
    emitter.instruction("jnz __rt_proc_pipe_registry_link_prev_x86");           // non-head nodes relink through their predecessor
    abi::emit_symbol_address(emitter, "r10", "_proc_pipe_registry_head");
    emitter.instruction("mov QWORD PTR [r10], rax");                            // remove the head node from the global list
    emitter.instruction("jmp __rt_proc_pipe_registry_linked_x86");              // continue with the retained container
    emitter.label("__rt_proc_pipe_registry_link_prev_x86");
    emitter.instruction("mov QWORD PTR [r10], rax");                            // bypass the matched node in the linked list
    emitter.label("__rt_proc_pipe_registry_linked_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the matched node
    emitter.instruction("mov rax, QWORD PTR [r11 + 16]");                       // load the registry-owned pipes container
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve pipes through iterator/accessor calls
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // inspect the indexed/hash storage kind
    emitter.instruction("and r10, 0xff");                                       // isolate the low-byte container kind
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // retain kind: 2 indexed, 3 associative hash
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the number of published pipe entries
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // retain total entry count
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // processed entries = 0
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // hash insertion-order cursor = 0
    emitter.label("__rt_proc_pipe_registry_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload processed-entry count
    emitter.instruction("cmp r10, QWORD PTR [rbp - 48]");                       // have all published pipe entries been visited?
    emitter.instruction("jae __rt_proc_pipe_registry_release_x86");             // release the retained container once draining is complete
    emitter.instruction("cmp QWORD PTR [rbp - 56], 3");                         // does the container use hash storage?
    emitter.instruction("je __rt_proc_pipe_registry_hash_key_x86");             // hashes require insertion-order key iteration
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // indexed key is the current zero-based entry index
    emitter.instruction("mov QWORD PTR [rbp - 80], -1");                        // integer-key high-word sentinel
    emitter.instruction("inc r10");                                             // advance indexed position before the accessor call
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // persist processed-entry count
    emitter.instruction("jmp __rt_proc_pipe_registry_get_x86");                 // fetch the exact resource box by its key
    emitter.label("__rt_proc_pipe_registry_hash_key_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the retained hash container
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // pass the prior insertion-order cursor
    abi::emit_call_label(emitter, "__rt_hash_iter_next");
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // persist the next hash cursor
    emitter.instruction("mov QWORD PTR [rbp - 72], rdi");                       // retain integer/string key low payload
    emitter.instruction("mov QWORD PTR [rbp - 80], rdx");                       // retain key high payload or integer sentinel
    emitter.instruction("inc QWORD PTR [rbp - 40]");                            // consume this hash entry
    emitter.label("__rt_proc_pipe_registry_get_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the retained pipes container to the generic accessor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // pass the original descriptor key low payload
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // pass its key high payload/sentinel
    emitter.instruction("xor ecx, ecx");                                        // suppress missing-key diagnostics during teardown
    abi::emit_call_label(emitter, "__rt_array_get_mixed_key");
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // retain the accessor-owned result box
    emitter.instruction("test rax, rax");                                       // did the accessor return a resource box?
    emitter.instruction("jz __rt_proc_pipe_registry_loop_x86");                 // malformed/missing entries need no close action
    emitter.instruction("cmp QWORD PTR [rax], 9");                              // is this a resource value?
    emitter.instruction("jne __rt_proc_pipe_registry_drop_box_x86");            // only resources can own process pipes
    emitter.instruction("cmp QWORD PTR [rax + 16], 1");                         // kind 1 denotes a native stream descriptor
    emitter.instruction("jne __rt_proc_pipe_registry_drop_box_x86");            // leave non-pipe resources untouched
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // load the raw pipe descriptor
    if emitter.target.platform == Platform::Windows {
        emitter.instruction("cmp rdi, -1");                                     // has user code already closed this exact resource?
        emitter.instruction("je __rt_proc_pipe_registry_drop_box_x86");         // avoid closing the release sentinel twice
    } else {
        emitter.instruction("cmp rdi, 0x40000000");                             // has user code already closed this exact resource?
        emitter.instruction("jae __rt_proc_pipe_registry_drop_box_x86");        // skip sentinel and synthetic stream values
    }
    emitter.instruction("mov QWORD PTR [rax + 8], -1");                         // mark the exact shared resource box closed
    if emitter.target.platform == Platform::Windows {
        abi::emit_call_label(emitter, "__rt_sys_close");
    } else {
        emitter.instruction("mov eax, 3");                                      // Linux x86_64 close syscall number
        emitter.instruction("syscall");                                         // close the descriptor before waiting for the child
    }
    emitter.label("__rt_proc_pipe_registry_drop_box_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // reload the accessor-owned mixed result
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    emitter.instruction("jmp __rt_proc_pipe_registry_loop_x86");                // continue closing every registered pipe resource

    emitter.label("__rt_proc_pipe_registry_release_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // release the registry's retained pipes container
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // release the raw linked-list node
    abi::emit_call_label(emitter, "__rt_heap_free");
    emitter.label("__rt_proc_pipe_registry_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the unchanged process descriptor to proc_close
    emitter.instruction("add rsp, 96");                                         // release the close helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // continue into the platform process wait
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Structural tests for the process-to-pipe registry runtime helpers.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - All targets must retain the pipe container, mark exact kind-1 boxes,
    //!   close them before wait, and release their registry ownership.

    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Verifies every supported target emits ownership-safe pipe teardown primitives.
    #[test]
    fn process_pipe_registry_closes_exact_boxes_on_every_target() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
            Target::new(Platform::Windows, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_proc_pipe_registry(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_proc_pipe_registry_register"));
            assert!(asm.contains("__rt_proc_pipe_registry_close"));
            assert!(asm.contains("_proc_pipe_registry_head"));
            assert!(asm.contains("__rt_array_get_mixed_key"));
            assert!(asm.contains("__rt_heap_alloc"));
            assert!(!asm.contains("__rt_proc_pipe_registry_register_oom"));
            if target.platform == Platform::Windows {
                assert!(asm.contains("call __rt_sys_close"));
                assert!(asm.contains("cmp rdi, -1"));
            } else {
                assert!(asm.contains("syscall") || asm.contains("svc"));
            }
            assert!(asm.contains("__rt_decref_any"));
        }
    }
}

/// Emits the direct close operation used by AArch64 registry teardown.
///
/// macOS and Linux use different syscall numbers, and no generic AArch64
/// `__rt_sys_close` shim exists. The process registry therefore follows the
/// same direct trap convention as the existing proc_open cleanup path.
fn emit_proc_pipe_close_aarch64(emitter: &mut Emitter) {
    if emitter.target.platform == Platform::MacOS {
        emitter.instruction("mov x16, #6");                                     // macOS close syscall number
        emitter.instruction("svc #0x80");                                       // close the descriptor through the macOS trap
    } else {
        emitter.instruction("mov x8, #57");                                     // Linux AArch64 close syscall number
        emitter.instruction("svc #0");                                          // close the descriptor through the Linux trap
    }
}
