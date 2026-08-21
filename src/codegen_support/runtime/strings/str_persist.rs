//! Purpose:
//! Emits the `__rt_str_persist`, `__rt_str_persist_done` runtime helper assembly for heap-backed string persistence.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - String helpers scan or transform byte ranges and return target ABI pointer/length pairs for generated call sites.
//! - A source already carrying `CONCAT_TEMP_HEAP_KIND` is a heap-backed `.` operator temporary:
//!   it is taken over in place (retagged as an owned string) instead of being duplicated, so a
//!   `$s .= ...` accumulation loop does not leave one oversized block behind per append.

use crate::codegen_support::runtime::strings::concat_scratch::CONCAT_TEMP_HEAP_KIND;
use crate::codegen_support::{emit::Emitter, platform::Arch};


/// str_persist: copy a string to heap for permanent storage.
/// Used to persist strings that would otherwise outlive their current owner.
/// Input:  x1=ptr, x2=len
/// Output: x1=new_ptr (on heap), x2=len (unchanged)
///
/// A source stamped with `CONCAT_TEMP_HEAP_KIND` is an unowned `.` operator temporary with at
/// most one consumer, so ownership is transferred by retagging the existing block as heap kind 1
/// instead of allocating and copying a second one. Every other source (rodata literals, concat
/// scratch slices, already-owned strings) still gets a fresh owned duplicate.
pub fn emit_str_persist(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_persist_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_persist ---");
    emitter.label_global("__rt_str_persist");

    crate::codegen_support::abi::emit_load_int_immediate(
        emitter,
        "x9",
        crate::codegen_support::sentinels::NULL_SENTINEL,
    );
    emitter.instruction("cmp x1, x9");                                          // preserve the dedicated null-string sentinel across ownership stabilization
    emitter.instruction("b.eq __rt_str_persist_done");                          // a missing string has no payload to allocate or copy

    // -- take over a transient `.` operator temporary instead of duplicating it --
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the frame pointer and return address across the heap-kind probe
    emitter.instruction("mov x29, sp");                                         // establish the persist-helper probe frame pointer
    emitter.instruction("mov x0, x1");                                          // pass the source payload pointer to the uniform heap-kind probe
    emitter.instruction("bl __rt_heap_kind");                                   // classify the source storage without disturbing the x1/x2 string pair
    emitter.instruction(&format!("cmp x0, #{}", CONCAT_TEMP_HEAP_KIND));        // is the source an unowned heap-backed concat temporary?
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the frame pointer and return address after the probe
    emitter.instruction("b.ne __rt_str_persist_duplicate");                     // every other source still gets a fresh owned duplicate
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x1, #-8]");                                   // retag the concat temporary as an owned string in place
    emitter.instruction("ret");                                                 // return the taken-over block with its length unchanged
    emitter.label("__rt_str_persist_duplicate");

    // -- zero-length strings still get an owned heap block so callers never alias a borrowed source pointer --
    // (the old early-return let explode()'s empty segment alias the subject string and double-free it on release)

    // -- set up stack frame (we call heap_alloc which may clobber regs) --
    emitter.instruction("sub sp, sp, #32");                                     // allocate 32 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish new frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save source pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save string length

    // -- round up allocation to next power of 2 (min 32) for free-list reuse --
    // This reduces fragmentation for string .= loops: freed blocks can be
    // reused by subsequent allocations of similar or smaller size.
    emitter.instruction("mov x0, #32");                                         // minimum allocation size
    emitter.instruction("cmp x2, #32");                                         // is length <= 32?
    emitter.instruction("b.le __rt_str_persist_alloc");                         // yes, use 32
    // -- round up to next power of 2 --
    emitter.instruction("sub x0, x2, #1");                                      // x0 = len - 1
    emitter.instruction("clz x3, x0");                                          // count leading zeros
    emitter.instruction("mov x0, #1");                                          // start with 1
    emitter.instruction("mov x4, #64");                                         // 64 - clz = bit position
    emitter.instruction("sub x4, x4, x3");                                      // x4 = 64 - leading_zeros = ceil(log2)
    emitter.instruction("lsl x0, x0, x4");                                      // x0 = 1 << ceil(log2(len)) = next power of 2
    emitter.label("__rt_str_persist_alloc");
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate on heap, x0 = heap pointer
    emitter.instruction("mov x6, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x6, [x0, #-8]");                                   // store string kind in the uniform heap header

    // -- copy bytes from source to heap --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source pointer (restored)
    emitter.instruction("ldr x2, [sp, #8]");                                    // x2 = length (restored)
    emitter.instruction("mov x3, x0");                                          // x3 = destination (heap pointer)
    emitter.instruction("mov x4, x2");                                          // x4 = byte count for loop

    emitter.label("__rt_str_persist_copy");
    emitter.instruction("cbz x4, __rt_str_persist_ret");                        // all bytes copied
    emitter.instruction("ldrb w5, [x1], #1");                                   // load byte from source, advance
    emitter.instruction("strb w5, [x3], #1");                                   // store byte to heap, advance
    emitter.instruction("sub x4, x4, #1");                                      // decrement remaining count
    emitter.instruction("b __rt_str_persist_copy");                             // continue copying

    // -- return heap pointer and original length --
    emitter.label("__rt_str_persist_ret");
    emitter.instruction("mov x1, x0");                                          // x1 = heap pointer (new string location)
    emitter.instruction("ldr x2, [sp, #8]");                                    // x2 = original length (unchanged)

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame

    emitter.label("__rt_str_persist_done");
    emitter.instruction("ret");                                                 // return to caller
}

/// x86_64 Linux variant of `emit_str_persist`.
/// Allocates heap storage, stamps the header with the owned-string heap kind, and copies
/// the source bytes into the owned allocation.
/// Input:  rax=ptr, rdx=len
/// Output: rax=heap_ptr (owned), rdx=len (unchanged)
///
/// NOTE: the source pointer is consumed from `rax`, not `rdi` — this mirrors the
/// x86_64 string result-pair convention used by the helpers that feed it.
fn emit_str_persist_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_persist ---");
    emitter.label_global("__rt_str_persist");

    crate::codegen_support::abi::emit_load_int_immediate(
        emitter,
        "r10",
        crate::codegen_support::sentinels::NULL_SENTINEL,
    );
    emitter.instruction("cmp rax, r10");                                        // preserve the dedicated null-string sentinel across ownership stabilization
    emitter.instruction("je __rt_str_persist_done");                            // a missing string has no payload to allocate or copy

    // -- zero-length strings still get an owned heap block so callers never alias a borrowed source pointer --

    // -- preserve the source payload across the heap allocation helper call --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame pointer for the saved source pointer and length
    emitter.instruction("sub rsp, 16");                                         // reserve local slots for the source pointer and source length across the allocator call
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source pointer across the heap allocation helper call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the source length across the heap allocation helper call

    // -- take over a transient `.` operator temporary instead of duplicating it --
    emitter.instruction("call __rt_heap_kind");                                 // classify the source storage (rax already holds the source pointer)
    emitter.instruction(&format!("cmp eax, {}", CONCAT_TEMP_HEAP_KIND));        // is the source an unowned heap-backed concat temporary?
    emitter.instruction("jne __rt_str_persist_duplicate");                      // every other source still gets a fresh owned duplicate
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the concat temporary payload pointer for the in-place retag
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // materialize the owned-string heap kind word with the x86_64 heap magic marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // retag the concat temporary as an owned string in place
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore the original string length for the x86_64 string result pair
    emitter.instruction("add rsp, 16");                                         // release the temporary spill slots used by the persist helper
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the taken-over block with its length unchanged

    emitter.label("__rt_str_persist_duplicate");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the byte length into the x86_64 heap helper input register
    emitter.instruction("call __rt_heap_alloc");                                // allocate owned string storage and return the payload pointer in rax
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // materialize the owned-string heap kind word with the x86_64 heap magic marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocated payload as a persisted elephc string in the uniform heap header
    emitter.instruction("mov r8, rax");                                         // preserve the destination heap pointer for the byte-copy loop and final return value
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the source pointer after the allocator helper returns
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the source byte length after the allocator helper returns

    // -- copy the source bytes into the owned heap allocation --
    emitter.label("__rt_str_persist_copy");
    emitter.instruction("test rcx, rcx");                                       // stop copying once every source byte has been moved into owned storage
    emitter.instruction("jz __rt_str_persist_ret");                             // the destination payload is fully initialized once no bytes remain
    emitter.instruction("mov r10b, BYTE PTR [r9]");                             // load the next source byte from the original transient string payload
    emitter.instruction("mov BYTE PTR [r8], r10b");                             // store the copied byte into the owned destination payload
    emitter.instruction("add r9, 1");                                           // advance the source cursor after copying one byte
    emitter.instruction("add r8, 1");                                           // advance the destination cursor after copying one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the number of remaining bytes left to duplicate
    emitter.instruction("jmp __rt_str_persist_copy");                           // continue the byte-copy loop until the entire payload is duplicated

    emitter.label("__rt_str_persist_ret");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore the original string length for the x86_64 string result pair
    emitter.instruction("sub r8, rdx");                                         // recover the base pointer of the owned payload after the post-increment copy loop
    emitter.instruction("mov rax, r8");                                         // return the owned string pointer in the x86_64 string result register
    emitter.instruction("add rsp, 16");                                         // release the temporary spill slots used by the persist helper
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning

    emitter.label("__rt_str_persist_done");
    emitter.instruction("ret");                                                 // return to the caller with rax=owned_ptr and rdx=length
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies emit str persist linux x86_64 uses heap helper.
    #[test]
    fn test_emit_str_persist_linux_x86_64_uses_heap_helper() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_str_persist(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("__rt_str_persist:\n"));
        assert!(asm.contains("call __rt_heap_alloc\n"));
        assert!(asm.contains("mov QWORD PTR [rax - 8], r10\n"));
    }
}
