//! Purpose:
//! Emits runtime release support for heap-backed callable descriptors.
//! Frees runtime descriptor copies, by-value captures and managed reference capture leases.
//!
//! Called from:
//! - `crate::codegen_support::runtime::callables`
//!
//! Key details:
//! - Static descriptors live in `.data` and are ignored by heap range checks.
//! - Dynamic descriptors use the uniform heap header refcount, then deep-release
//!   owned capture slots before returning the descriptor block to the allocator.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;


/// Emits the `__rt_callable_descriptor_release` runtime helper for the active target.
///
/// Input: `x0`/`rax` = callable descriptor pointer. Static and null pointers are no-ops.
/// Dynamic descriptors are refcounted with the uniform heap header. On the final release,
/// by-value string captures are freed, heap-backed captures are decref'd through
/// `__rt_decref_any`, nested callable captures recurse, and managed reference captures retire
/// their cell lease. Borrowed addresses stay untouched before the descriptor block is freed.
pub(crate) fn emit_callable_descriptor_release(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_callable_descriptor_release_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: callable descriptor release ---");
    emitter.label_global("__rt_callable_descriptor_release");

    // -- null and heap-range checks --
    emitter.instruction("cbz x0, __rt_callable_descriptor_release_done");       // static null descriptors have nothing to release
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is the descriptor below the managed heap?
    emitter.instruction("b.lo __rt_callable_descriptor_release_done");          // yes, it is static or foreign metadata
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // load the current heap bump offset
    emitter.instruction("add x10, x9, x10");                                    // compute the current managed heap end
    emitter.instruction("cmp x0, x10");                                         // is the descriptor outside the live heap window?
    emitter.instruction("b.hs __rt_callable_descriptor_release_done");          // yes, do not touch it

    // -- decrement descriptor refcount --
    emitter.instruction("ldr w9, [x0, #-12]");                                  // load descriptor refcount from the uniform heap header
    emitter.instruction("cbz w9, __rt_callable_descriptor_release_done");       // already-freed raw blocks are ignored defensively
    emitter.instruction("subs w9, w9, #1");                                     // drop one descriptor owner
    emitter.instruction("str w9, [x0, #-12]");                                  // store the decremented descriptor refcount
    emitter.instruction("b.ne __rt_callable_descriptor_release_done");          // other owners still keep the descriptor alive

    // -- set up cleanup frame --
    crate::codegen_support::abi::emit_frame_prologue(emitter, 64);
    emitter.instruction("str x0, [sp, #0]");                                    // save descriptor pointer for capture release and final free
    super::super::arrays::deep_cleanup::begin(emitter);
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize capture index to zero

    // -- load environment metadata --
    emitter.instruction("ldr x9, [x0, #40]");                                   // x9 = descriptor environment record pointer
    emitter.instruction("cbz x9, __rt_callable_descriptor_release_free");       // descriptors without captures skip straight to block free
    emitter.instruction("ldr x10, [x9]");                                       // x10 = capture count
    emitter.instruction("str x10, [sp, #8]");                                   // save capture count for the cleanup loop
    emitter.instruction("cbz x10, __rt_callable_descriptor_release_free");      // no capture slots need release work
    emitter.instruction("ldr x11, [x9, #16]");                                  // x11 = capture binding metadata table
    emitter.instruction("str x11, [sp, #16]");                                  // save capture table pointer for the cleanup loop
    emitter.instruction("cbz x11, __rt_callable_descriptor_release_free");      // missing metadata means there are no typed owned captures to release

    // -- walk capture metadata and release owned slots --
    emitter.label("__rt_callable_descriptor_release_loop");
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload current capture index
    emitter.instruction("ldr x13, [sp, #8]");                                   // reload total capture count
    emitter.instruction("cmp x12, x13");                                        // have all capture slots been processed?
    emitter.instruction("b.hs __rt_callable_descriptor_release_free");          // yes, free the descriptor allocation
    emitter.instruction("ldr x14, [sp, #16]");                                  // reload capture binding table pointer
    emitter.instruction("mov x15, #32");                                        // each capture binding entry is four 8-byte words
    emitter.instruction("mul x15, x12, x15");                                   // compute byte offset for this capture metadata entry
    emitter.instruction("add x14, x14, x15");                                   // x14 = capture metadata entry pointer
    emitter.instruction("ldr x16, [x14, #24]");                                 // preserve the reference flag while loading the capture payload
    emitter.instruction("ldr x15, [x14, #16]");                                 // load descriptor type tag for the by-value capture
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload descriptor pointer before reading the capture slot
    emitter.instruction("mov x10, #16");                                        // each runtime capture slot is 16 bytes
    emitter.instruction("mul x10, x12, x10");                                   // compute capture slot offset after the descriptor header
    emitter.instruction("add x10, x10, #64");                                   // skip the 64-byte static descriptor header
    emitter.instruction("ldr x0, [x9, x10]");                                   // x0 = capture slot low word, usually a heap pointer
    emitter.instruction("cbnz x16, __rt_callable_descriptor_release_reference"); // managed reference captures own their cell instead of its payload
    emitter.instruction("cmp x15, #1");                                         // is this a string capture?
    emitter.instruction("b.eq __rt_callable_descriptor_release_string");        // strings release their owned copied payload directly
    emitter.instruction("cmp x15, #4");                                         // is this an indexed-array capture?
    emitter.instruction("b.eq __rt_callable_descriptor_release_any");           // arrays release through the uniform heap dispatcher
    emitter.instruction("cmp x15, #5");                                         // is this an associative-array capture?
    emitter.instruction("b.eq __rt_callable_descriptor_release_any");           // hashes release through the uniform heap dispatcher
    emitter.instruction("cmp x15, #6");                                         // is this an object capture?
    emitter.instruction("b.eq __rt_callable_descriptor_release_any");           // objects release through the uniform heap dispatcher
    emitter.instruction("cmp x15, #7");                                         // is this a mixed or union capture?
    emitter.instruction("b.eq __rt_callable_descriptor_release_any");           // mixed boxes release through the uniform heap dispatcher
    emitter.instruction("cmp x15, #10");                                        // is this a nested callable descriptor capture?
    emitter.instruction("b.eq __rt_callable_descriptor_release_callable");      // nested callable descriptors recurse through this helper
    emitter.instruction("cmp x15, #12");                                        // is this an iterable capture?
    emitter.instruction("b.eq __rt_callable_descriptor_release_any");           // erased iterables release by inspecting their runtime heap kind
    emitter.instruction("b __rt_callable_descriptor_release_next");             // scalar captures have no heap ownership to release

    emitter.label("__rt_callable_descriptor_release_reference");
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_reference_cell_owner");
    super::super::arrays::deep_cleanup::invoke(emitter, "__rt_reference_cell_release", "x0");
    emitter.instruction("b __rt_callable_descriptor_release_next");             // continue after releasing only an independently owned reference cell

    emitter.label("__rt_callable_descriptor_release_string");
    emitter.instruction("bl __rt_heap_free_safe");                              // release the descriptor-owned string capture copy
    emitter.instruction("b __rt_callable_descriptor_release_next");             // continue with the next capture slot

    emitter.label("__rt_callable_descriptor_release_any");
    super::super::arrays::deep_cleanup::invoke(emitter, "__rt_decref_any", "x0");
    emitter.instruction("b __rt_callable_descriptor_release_next");             // continue with the next capture slot

    emitter.label("__rt_callable_descriptor_release_callable");
    super::super::arrays::deep_cleanup::invoke(emitter, "__rt_callable_descriptor_release", "x0");

    emitter.label("__rt_callable_descriptor_release_next");
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload current capture index after any nested release
    emitter.instruction("add x12, x12, #1");                                    // advance to the next capture slot
    emitter.instruction("str x12, [sp, #24]");                                  // persist the updated capture index
    emitter.instruction("b __rt_callable_descriptor_release_loop");             // continue walking capture metadata

    // -- free descriptor block and return --
    emitter.label("__rt_callable_descriptor_release_free");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload descriptor pointer for final heap free
    emitter.instruction("bl __rt_heap_free");                                   // return the runtime descriptor block to the heap allocator
    super::super::arrays::deep_cleanup::finish(emitter, "__rt_callable_descriptor_release_return");

    emitter.label("__rt_callable_descriptor_release_done");
    emitter.instruction("ret");                                                 // return after releasing or ignoring the descriptor
}

/// Emits the x86_64 Linux variant of `__rt_callable_descriptor_release`.
fn emit_callable_descriptor_release_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: callable descriptor release ---");
    emitter.label_global("__rt_callable_descriptor_release");

    emitter.instruction("test rax, rax");                                       // static null descriptors have nothing to release
    emitter.instruction("jz __rt_callable_descriptor_release_done");            // skip null descriptor pointers
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("cmp rax, r10");                                        // reject descriptors below the managed heap
    emitter.instruction("jb __rt_callable_descriptor_release_done");            // static descriptors live outside the heap and are ignored
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_heap_off");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the x86_64 heap bump offset
    emitter.instruction("add r11, r10");                                        // compute the current managed heap end
    emitter.instruction("cmp rax, r11");                                        // is the descriptor outside the live heap window?
    emitter.instruction("jae __rt_callable_descriptor_release_done");           // outside-heap descriptors are not runtime-owned
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the x86_64 heap marker/kind word
    emitter.instruction("shr r10, 32");                                         // isolate the high-word ownership marker
    emitter.instruction(&format!("cmp r10d, 0x{:x}", crate::codegen_support::sentinels::X86_64_HEAP_MAGIC_HI32)); // verify that this block belongs to the x86_64 heap wrapper
    emitter.instruction("jne __rt_callable_descriptor_release_done");           // foreign/static pointers are ignored
    emitter.instruction("mov r10d, DWORD PTR [rax - 12]");                      // load descriptor refcount from the uniform heap header
    emitter.instruction("test r10d, r10d");                                     // has this raw heap block already been released?
    emitter.instruction("jz __rt_callable_descriptor_release_done");            // yes, ignore the defensive duplicate release
    emitter.instruction("sub r10d, 1");                                         // drop one descriptor owner
    emitter.instruction("mov DWORD PTR [rax - 12], r10d");                      // store the decremented descriptor refcount
    emitter.instruction("jnz __rt_callable_descriptor_release_done");           // other owners still keep the descriptor alive

    crate::codegen_support::abi::emit_frame_prologue(emitter, 64);
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save descriptor pointer for capture release and final free
    super::super::arrays::deep_cleanup::begin(emitter);
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize capture index to zero

    emitter.instruction("mov r10, QWORD PTR [rax + 40]");                       // r10 = descriptor environment record pointer
    emitter.instruction("test r10, r10");                                       // does the descriptor carry capture metadata?
    emitter.instruction("jz __rt_callable_descriptor_release_free");            // descriptors without captures skip straight to block free
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // r11 = capture count
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save capture count for the cleanup loop
    emitter.instruction("test r11, r11");                                       // are there any capture slots to release?
    emitter.instruction("jz __rt_callable_descriptor_release_free");            // no capture slots need release work
    emitter.instruction("mov r11, QWORD PTR [r10 + 16]");                       // r11 = capture binding metadata table
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save capture table pointer for the cleanup loop
    emitter.instruction("test r11, r11");                                       // is typed capture metadata available?
    emitter.instruction("jz __rt_callable_descriptor_release_free");            // missing metadata means there are no owned typed captures

    emitter.label("__rt_callable_descriptor_release_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload current capture index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 16]");                       // have all capture slots been processed?
    emitter.instruction("jae __rt_callable_descriptor_release_free");           // yes, free the descriptor allocation
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload capture binding table pointer
    emitter.instruction("mov rcx, r10");                                        // copy index before scaling metadata offset
    emitter.instruction("shl rcx, 5");                                          // each capture binding entry is 32 bytes
    emitter.instruction("add r11, rcx");                                        // r11 = capture metadata entry pointer
    emitter.instruction("mov r8, QWORD PTR [r11 + 24]");                        // preserve the reference flag while loading the capture payload
    emitter.instruction("mov rdx, QWORD PTR [r11 + 16]");                       // load descriptor type tag for the by-value capture
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload descriptor pointer before reading the capture slot
    emitter.instruction("mov rcx, r10");                                        // copy index before scaling capture slot offset
    emitter.instruction("shl rcx, 4");                                          // each runtime capture slot is 16 bytes
    emitter.instruction("mov rax, QWORD PTR [rax + rcx + 64]");                 // rax = capture slot low word, usually a heap pointer
    emitter.instruction("test r8, r8");                                         // distinguish a cell address from an ordinary captured value
    emitter.instruction("jnz __rt_callable_descriptor_release_reference");      // managed reference captures own their cell instead of its payload
    emitter.instruction("cmp rdx, 1");                                          // is this a string capture?
    emitter.instruction("je __rt_callable_descriptor_release_string");          // strings release their owned copied payload directly
    emitter.instruction("cmp rdx, 4");                                          // is this an indexed-array capture?
    emitter.instruction("je __rt_callable_descriptor_release_any");             // arrays release through the uniform heap dispatcher
    emitter.instruction("cmp rdx, 5");                                          // is this an associative-array capture?
    emitter.instruction("je __rt_callable_descriptor_release_any");             // hashes release through the uniform heap dispatcher
    emitter.instruction("cmp rdx, 6");                                          // is this an object capture?
    emitter.instruction("je __rt_callable_descriptor_release_any");             // objects release through the uniform heap dispatcher
    emitter.instruction("cmp rdx, 7");                                          // is this a mixed or union capture?
    emitter.instruction("je __rt_callable_descriptor_release_any");             // mixed boxes release through the uniform heap dispatcher
    emitter.instruction("cmp rdx, 10");                                         // is this a nested callable descriptor capture?
    emitter.instruction("je __rt_callable_descriptor_release_callable");        // nested callable descriptors recurse through this helper
    emitter.instruction("cmp rdx, 12");                                         // is this an iterable capture?
    emitter.instruction("je __rt_callable_descriptor_release_any");             // erased iterables release by inspecting their runtime heap kind
    emitter.instruction("jmp __rt_callable_descriptor_release_next");           // scalar captures have no heap ownership to release

    emitter.label("__rt_callable_descriptor_release_reference");
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_reference_cell_owner");
    super::super::arrays::deep_cleanup::invoke(emitter, "__rt_reference_cell_release", "rax");
    emitter.instruction("jmp __rt_callable_descriptor_release_next");           // continue after releasing only an independently owned reference cell

    emitter.label("__rt_callable_descriptor_release_string");
    emitter.instruction("call __rt_heap_free_safe");                            // release the descriptor-owned string capture copy
    emitter.instruction("jmp __rt_callable_descriptor_release_next");           // continue with the next capture slot

    emitter.label("__rt_callable_descriptor_release_any");
    super::super::arrays::deep_cleanup::invoke(emitter, "__rt_decref_any", "rax");
    emitter.instruction("jmp __rt_callable_descriptor_release_next");           // continue with the next capture slot

    emitter.label("__rt_callable_descriptor_release_callable");
    super::super::arrays::deep_cleanup::invoke(emitter, "__rt_callable_descriptor_release", "rax");

    emitter.label("__rt_callable_descriptor_release_next");
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance to the next capture slot
    emitter.instruction("jmp __rt_callable_descriptor_release_loop");           // continue walking capture metadata

    emitter.label("__rt_callable_descriptor_release_free");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload descriptor pointer for final heap free
    emitter.instruction("call __rt_heap_free");                                 // return the runtime descriptor block to the heap allocator
    super::super::arrays::deep_cleanup::finish(emitter, "__rt_callable_descriptor_release_return");

    emitter.label("__rt_callable_descriptor_release_done");
    emitter.instruction("ret");                                                 // return after releasing or ignoring the descriptor
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Reference captures release only validated cell owners through guarded cleanup on every ABI.
    #[test]
    fn descriptor_reference_capture_retirement_is_guarded_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_callable_descriptor_release(&mut emitter);
            let asm = emitter.output();
            let branch = asm.split_once("__rt_callable_descriptor_release_reference:\n").unwrap().1;
            let branch = branch.split("__rt_callable_descriptor_release_next").next().unwrap();
            let lookup = branch.find("__rt_reference_cell_owner").unwrap();
            let release = branch.find("__rt_reference_cell_release").unwrap();
            let guard = branch.find("__rt_cleanup_invoke").unwrap();
            assert!(lookup < release && release < guard, "{target}: {branch}");
        }
    }
}
