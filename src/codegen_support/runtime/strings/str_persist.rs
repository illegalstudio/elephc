//! Purpose:
//! Emits the `__rt_str_persist`, `__rt_str_persist_done` and `__rt_str_claim` runtime helper assembly for heap-backed string persistence.
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
//! - `__rt_str_claim` retags such a temporary as owned without copying it, so a value that a
//!   user call may hand back has exactly one releasing owner before any persist can adopt it.
//! - The allocator's live-base bitmap rejects interior pointers in constant time.
//! - Optional INI metadata records fresh lazy origins and preserves complete copies without native heap references.

use crate::codegen_support::sentinels::CONCAT_TEMP_HEAP_KIND;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits a probe that accepts only the exact payload base of a live transient concat block.
///
/// A range check plus `[pointer - 8]` kind load is insufficient for string slices because their
/// interior pointer can be preceded by user-controlled bytes that resemble a heap header. The
/// allocator's live-base bitmap rejects interior pointers without scanning unrelated blocks.
fn emit_transient_base_probe(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: str_transient_base ---");
        emitter.label("__rt_str_transient_base");
        emitter.instruction("test rax, rax");                                   // null pointers cannot name a managed payload base
        emitter.instruction("jz __rt_str_transient_base_zero_x86");             // reject null before reading allocator state
        crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_heap_buf");
        emitter.instruction("lea rcx, [r8 + 16]");                              // compute the first possible payload base
        emitter.instruction("cmp rax, rcx");                                    // is the candidate below the first payload?
        emitter.instruction("jb __rt_str_transient_base_zero_x86");             // static and foreign pointers are not transient blocks
        crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_heap_off");
        emitter.instruction("mov r9, QWORD PTR [r9]");                          // load the live bump extent
        emitter.instruction("add r9, r8");                                      // derive the current heap end
        emitter.instruction("cmp rax, r9");                                     // is the candidate outside the live heap window?
        emitter.instruction("jae __rt_str_transient_base_zero_x86");            // reject pointers at or beyond the heap end
        emitter.instruction("test al, 7");                                      // every real payload base has eight-byte alignment
        emitter.instruction("jnz __rt_str_transient_base_zero_x86");            // reject unaligned interior byte pointers
        emitter.instruction("mov r10, QWORD PTR [rax - 8]");                    // cheaply reject ordinary strings before probing the side table
        emitter.instruction(&format!("mov r11, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(CONCAT_TEMP_HEAP_KIND))); // materialize the complete transient kind word
        emitter.instruction("cmp r10, r11");                                    // do the candidate's preceding bytes even carry the full marker?
        emitter.instruction("jne __rt_str_transient_base_zero_x86");            // ordinary strings avoid all side-table work
        emitter.instruction("mov r9, rax");                                     // copy the candidate before deriving its heap-relative offset
        emitter.instruction("sub r9, r8");                                      // derive the candidate's heap-relative byte offset
        emitter.instruction("mov rcx, r9");                                     // preserve the offset for the granule-bit calculation
        emitter.instruction("shr r9, 6");                                       // select one bitmap byte for each 64 heap bytes
        emitter.instruction("shr rcx, 3");                                      // convert the byte offset to its 8-byte granule index
        emitter.instruction("and ecx, 7");                                      // select the candidate bit within that bitmap byte
        emitter.instruction("mov r11d, 1");                                     // begin with the low bit set
        emitter.instruction("shl r11d, cl");                                    // move the mask to the candidate payload-base bit
        crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_heap_live_bases");
        emitter.instruction("test BYTE PTR [r10 + r9], r11b");                  // did the allocator publish this exact live payload base?
        emitter.instruction("jz __rt_str_transient_base_zero_x86");             // forged markers inside a payload have no live-base bit
        emitter.instruction("cmp DWORD PTR [rax - 12], 0");                     // is the exact matching block still live?
        emitter.instruction("je __rt_str_transient_base_zero_x86");             // free blocks cannot be adopted
        emitter.instruction("mov eax, 1");                                      // report the exact live transient payload base
        emitter.instruction("ret");                                             // return the exact-base classification
        emitter.label("__rt_str_transient_base_zero_x86");
        emitter.instruction("xor eax, eax");                                    // report a nonmatching or interior pointer
        emitter.instruction("ret");                                             // return false to the caller
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_transient_base ---");
    emitter.label_shared("__rt_str_transient_base");
    emitter.instruction("cbz x0, __rt_str_transient_base_zero");                // null pointers cannot name a managed payload base
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("add x13, x9, #16");                                    // compute the first possible payload base
    emitter.instruction("cmp x0, x13");                                         // is the candidate below the first payload?
    emitter.instruction("b.lo __rt_str_transient_base_zero");                   // static and foreign pointers are not transient blocks
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // load the live bump extent
    emitter.instruction("add x10, x9, x10");                                    // derive the current heap end
    emitter.instruction("cmp x0, x10");                                         // is the candidate outside the live heap window?
    emitter.instruction("b.hs __rt_str_transient_base_zero");                   // reject pointers at or beyond the heap end
    emitter.instruction("tst x0, #7");                                          // every real payload base has eight-byte alignment
    emitter.instruction("b.ne __rt_str_transient_base_zero");                   // reject unaligned interior byte pointers
    emitter.instruction("ldr x11, [x0, #-8]");                                  // cheaply reject ordinary strings before probing the side table
    emitter.instruction(&format!("cmp x11, #{}", CONCAT_TEMP_HEAP_KIND));       // do the candidate's preceding bytes carry the complete transient kind?
    emitter.instruction("b.ne __rt_str_transient_base_zero");                   // ordinary strings avoid all side-table work
    emitter.instruction("sub x11, x0, x9");                                     // derive the candidate's heap-relative byte offset
    emitter.instruction("lsr x12, x11, #6");                                    // select one bitmap byte for each 64 heap bytes
    emitter.instruction("lsr x11, x11, #3");                                    // convert the byte offset to its 8-byte granule index
    emitter.instruction("and x11, x11, #7");                                    // select the candidate bit within that bitmap byte
    emitter.instruction("mov w13, #1");                                         // begin with the low bit set
    emitter.instruction("lsl w13, w13, w11");                                   // move the mask to the candidate payload-base bit
    crate::codegen_support::abi::emit_symbol_address(emitter, "x14", "_heap_live_bases");
    emitter.instruction("ldrb w14, [x14, x12]");                                // load the side-table byte for this candidate granule
    emitter.instruction("tst w14, w13");                                        // did the allocator publish this exact live payload base?
    emitter.instruction("b.eq __rt_str_transient_base_zero");                   // forged markers inside a payload have no live-base bit
    emitter.instruction("ldr w14, [x0, #-12]");                                 // load the exact matching block's live refcount
    emitter.instruction("cbz w14, __rt_str_transient_base_zero");               // free blocks cannot be adopted
    emitter.instruction("mov x0, #1");                                          // report the exact live transient payload base
    emitter.instruction("ret");                                                 // return the exact-base classification
    emitter.label("__rt_str_transient_base_zero");
    emitter.instruction("mov x0, #0");                                          // report a nonmatching or interior pointer
    emitter.instruction("ret");                                                 // return false to the caller
}


/// str_claim: give a possibly transient string one owner without copying it.
/// Input:  x1=ptr, x2=len (AArch64) or rax=ptr, rdx=len (x86_64)
/// Output: the same pointer and length
///
/// A payload stamped `CONCAT_TEMP_HEAP_KIND` is retagged as an owned string (heap kind 1) in
/// place; every other payload (concat scratch, literals, owned strings, the null sentinel) is
/// returned untouched. Codegen emits it for `EnsureOwned`, before a transient string reaches a
/// user call that may hand it back: from then on `__rt_str_persist` duplicates the block instead
/// of adopting it, so the caller's eventual release is the only one that frees it.
pub fn emit_str_claim(emitter: &mut Emitter, mbstring: bool) {
    emit_transient_base_probe(emitter);
    if emitter.target.arch == Arch::X86_64 {
        emit_str_claim_linux_x86_64(emitter, mbstring);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_claim ---");
    emitter.label_global("__rt_str_claim");

    crate::codegen_support::abi::emit_load_int_immediate(
        emitter,
        "x9",
        crate::codegen_support::sentinels::NULL_SENTINEL,
    );
    emitter.instruction("cmp x1, x9");                                          // a missing string has no storage to claim
    emitter.instruction("b.eq __rt_str_claim_done");                            // leave the dedicated null-string sentinel untouched
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the frame pointer and return address across the heap-kind probe
    emitter.instruction("mov x29, sp");                                         // establish the claim-helper probe frame pointer
    emitter.instruction("mov x0, x1");                                          // pass the payload pointer to the uniform heap-kind probe
    emitter.instruction("bl __rt_str_transient_base");                          // require the exact base of a live transient block
    emitter.instruction("cbz x0, __rt_str_claim_keep");                         // scratch, slices, literals and owned strings are already safe to share
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = owned elephc string
    emitter.instruction("str x9, [x1, #-8]");                                   // retag the transient block as owned so later persists duplicate it
    if mbstring {
        emitter.instruction("bl __rt_mbstring_ini_fresh");                      // assign a fresh logical origin to the newly owned block
    }
    emitter.label("__rt_str_claim_keep");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the frame pointer and return address after the probe
    emitter.label("__rt_str_claim_done");
    emitter.instruction("ret");                                                 // return the same pointer and length in x1/x2
}

/// Emits the x86_64 Linux implementation of the `__rt_str_claim` runtime helper.
///
/// Mirrors the AArch64 helper with the `rax`/`rdx` string pair: the heap-kind probe clobbers the
/// caller-saved registers, so the pair is spilled and reloaded around it.
fn emit_str_claim_linux_x86_64(emitter: &mut Emitter, mbstring: bool) {
    emitter.blank();
    emitter.comment("--- runtime: str_claim ---");
    emitter.label_global("__rt_str_claim");

    crate::codegen_support::abi::emit_load_int_immediate(
        emitter,
        "r10",
        crate::codegen_support::sentinels::NULL_SENTINEL,
    );
    emitter.instruction("cmp rax, r10");                                        // a missing string has no storage to claim
    emitter.instruction("je __rt_str_claim_done_x86");                          // leave the dedicated null-string sentinel untouched
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame pointer for the saved string pair
    emitter.instruction("sub rsp, 16");                                         // reserve aligned slots for the payload pointer and length
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // keep the payload pointer across the heap-kind probe
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // keep the payload length across the heap-kind probe
    emitter.instruction("call __rt_str_transient_base");                        // require the exact base of a live transient block
    emitter.instruction("test eax, eax");                                       // did the exact-base probe accept this payload?
    emitter.instruction("jz __rt_str_claim_keep_x86");                          // scratch, slices, literals and owned strings are already safe to share
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the transient block pointer for the in-place retag
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // materialize the owned-string heap kind word with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // retag the transient block as owned so later persists duplicate it
    if mbstring {
        emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                   // restore the length the metadata hook expects in the result pair
        emitter.instruction("call __rt_mbstring_ini_fresh");                    // assign a fresh logical origin to the newly owned block
    }
    emitter.label("__rt_str_claim_keep_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the unchanged payload pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // return the unchanged payload length
    emitter.instruction("add rsp, 16");                                         // release the spill slots used by the claim helper
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.label("__rt_str_claim_done_x86");
    emitter.instruction("ret");                                                 // return the same pointer and length in rax/rdx
}

/// str_persist: copy a string to heap for permanent storage.
/// Used to persist strings that would otherwise outlive their current owner.
/// Input:  x1=ptr, x2=len
/// Output: x1=new_ptr (on heap), x2=len (unchanged)
///
/// A source stamped with `CONCAT_TEMP_HEAP_KIND` is an unowned `.` operator temporary with at
/// most one consumer, so ownership is transferred by retagging the existing block as heap kind 1
/// instead of allocating and copying a second one. Every other source (rodata literals, concat
/// scratch slices, already-owned strings) still gets a fresh owned duplicate.
pub fn emit_str_persist(emitter: &mut Emitter, mbstring: bool) {
    emit_str_claim(emitter, mbstring);
    if emitter.target.arch == Arch::X86_64 {
        emit_str_persist_linux_x86_64(emitter, mbstring);
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
    emitter.instruction("bl __rt_str_transient_base");                          // require the exact base of a live transient block
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the frame pointer and return address after the probe
    emitter.instruction("cbz x0, __rt_str_persist_duplicate");                  // every other source, including slices, gets a fresh owned duplicate
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x1, #-8]");                                   // retag the concat temporary as an owned string in place
    if mbstring {
        emitter.instruction("stp x29, x30, [sp, #-16]!");                       // preserve linkage while recording the newly stabilized concat result
        emitter.instruction("bl __rt_mbstring_ini_fresh");                      // assign a fresh logical origin without reading or retaining its bytes
        emitter.instruction("ldp x29, x30, [sp], #16");                         // restore the original return address after the metadata hook
    }
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

    emitter.raw(".p2align 6");
    emitter.label("__rt_str_persist_copy");
    emitter.instruction("cbz x4, __rt_str_persist_ret");                        // all bytes copied
    emitter.instruction("ldrb w5, [x1], #1");                                   // load byte from source, advance
    emitter.instruction("strb w5, [x3], #1");                                   // store byte to heap, advance
    emitter.instruction("sub x4, x4, #1");                                      // decrement remaining count
    emitter.instruction("b __rt_str_persist_copy");                             // continue copying

    // -- return heap pointer and original length --
    emitter.label("__rt_str_persist_ret");
    if mbstring {
        emitter.instruction("ldr x1, [sp]");                                    // recover the original source allocation for identity-preserving copies
        emitter.instruction("ldr x2, [sp, #8]");                                // preserve identity only for a complete source byte range
        emitter.instruction("bl __rt_mbstring_ini_persist");                    // share a complete known origin or create a fresh logical destination
    }
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
/// Input:  rax=ptr, rdx=len — the x86_64 string result pair, not the SysV
///         argument registers; the body reads `rax` and every call site loads it.
/// Output: rax=heap_ptr (owned), rdx=len (unchanged)
fn emit_str_persist_linux_x86_64(emitter: &mut Emitter, mbstring: bool) {
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
    emitter.instruction("call __rt_str_transient_base");                        // require the exact base of a live transient block
    emitter.instruction("test eax, eax");                                       // did the exact-base probe accept this payload?
    emitter.instruction("jz __rt_str_persist_duplicate");                       // every other source, including slices, gets a fresh owned duplicate
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the concat temporary payload pointer for the in-place retag
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // materialize the owned-string heap kind word with the x86_64 heap magic marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // retag the concat temporary as an owned string in place
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore the original string length for the x86_64 string result pair
    if mbstring {
        emitter.instruction("call __rt_mbstring_ini_fresh");                    // record the newly stabilized concat result before any subsequent alias copy
    }
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
    emitter.raw(".p2align 6");
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
    if mbstring {
        emitter.instruction("mov rcx, rdx");                                    // pass the complete byte count to the metadata copy hook
        emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                    // recover the original source allocation before replacing the result pair
        emitter.instruction("call __rt_mbstring_ini_persist");                  // share the complete known origin or create a fresh logical destination
        emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                   // restore the ordinary native pointer and length return convention
    }
    emitter.instruction("add rsp, 16");                                         // release the temporary spill slots used by the persist helper
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning

    emitter.label("__rt_str_persist_done");
    emitter.instruction("ret");                                                 // return to the caller with rax=owned_ptr and rdx=length
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// The quadratic concat-assignment copy loop keeps stable instruction-cache alignment.
    #[test]
    fn persist_copy_loop_is_cache_line_aligned_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_str_persist(&mut emitter, false);
            assert!(
                emitter.output().contains(".p2align 6\n__rt_str_persist_copy:"),
                "{name}"
            );
        }
    }

    /// Every target emits the non-copying claim helper next to persist: it requires an exact
    /// transient block base, retags that block as an owned string, and never allocates.
    #[test]
    fn str_claim_retags_only_transient_blocks_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_str_persist(&mut emitter, false);
            let asm = emitter.output();
            let start = asm.find("__rt_str_claim:\n").unwrap_or_else(|| panic!("{name}: no claim helper"));
            let end = asm[start..].find("__rt_str_persist:\n").map(|end| start + end).unwrap_or(asm.len());
            let claim = &asm[start..end];
            assert!(claim.contains("__rt_str_transient_base"), "{name}: {claim}");
            assert!(!claim.contains("__rt_heap_alloc"), "{name}: {claim}");
            if target.arch == Arch::X86_64 {
                assert!(claim.contains("mov QWORD PTR [rax - 8], r10"), "{name}: {claim}");
            } else {
                assert!(claim.contains("str x9, [x1, #-8]"), "{name}: {claim}");
            }
        }
    }

    /// Exact bases come from the bitmap without a heap walk, with live and complete headers.
    #[test]
    fn transient_base_probe_rejects_interior_pointers_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_str_persist(&mut emitter, false);
            let asm = emitter.output();
            let start = asm.find("__rt_str_transient_base:\n")
                .unwrap_or_else(|| panic!("{name}: no exact transient-base probe"));
            let end = asm[start..].find("__rt_str_claim:\n")
                .map(|end| start + end)
                .unwrap_or(asm.len());
            let probe = &asm[start..end];
            assert!(probe.contains("_heap_buf"), "{name}: {probe}");
            assert!(probe.contains("_heap_off"), "{name}: {probe}");
            assert!(probe.contains("_heap_live_bases"), "{name}: {probe}");
            assert!(!probe.contains("transient_base_scan"), "{name}: {probe}");
            if target.arch == Arch::X86_64 {
                assert!(probe.contains("test BYTE PTR [r10 + r9], r11b"), "{name}: {probe}");
                assert!(probe.contains("cmp DWORD PTR [rax - 12], 0"), "{name}: {probe}");
                assert!(probe.contains(&format!(
                    "mov r11, 0x{:x}",
                    crate::codegen_support::sentinels::x86_64_heap_kind_word(CONCAT_TEMP_HEAP_KIND)
                )), "{name}: {probe}");
            } else {
                assert!(probe.contains("tst w14, w13"), "{name}: {probe}");
                assert!(probe.contains("ldr w14, [x0, #-12]"), "{name}: {probe}");
                assert!(probe.contains(&format!("cmp x11, #{}", CONCAT_TEMP_HEAP_KIND)), "{name}: {probe}");
            }
        }
    }

    /// Verifies emit str persist linux x86_64 uses heap helper.
    #[test]
    fn test_emit_str_persist_linux_x86_64_uses_heap_helper() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_str_persist(&mut emitter, false);
        let asm = emitter.output();

        assert!(asm.contains("__rt_str_persist:\n"));
        assert!(asm.contains("call __rt_heap_alloc\n"));
        assert!(asm.contains("mov QWORD PTR [rax - 8], r10\n"));
    }
}
