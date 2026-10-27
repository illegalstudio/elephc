//! Purpose:
//! Emits `__rt_heap_consolidate` and `__rt_heap_bins_clear`, the allocator helpers that fold
//! every exact size-class bin back into one address-ordered, fully coalesced free list.
//! Owns the bin geometry the allocator, the free path, the validator and the runtime data share.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - `__rt_heap_alloc` when the bump pointer would pass `_heap_max`, `__rt_gc_mem_caches`, and the
//!   `--web` per-request arena reset.
//!
//! Key details:
//! - Freed blocks up to `HEAP_BIN_MAX` payload bytes go to one bin per 8-byte size, so the hot
//!   alloc/free paths never walk a list. Binned blocks are not coalesced when freed; this pass
//!   merges them, which is why the allocator runs it before reporting heap exhaustion.
//! - Free blocks are recognised by marking: every block reachable from a bin or the ordered list
//!   gets `HEAP_CONSOLIDATE_MARK` in its kind word, then one physical walk over the tiled heap
//!   merges each maximal run of marked blocks. A live block whose refcount happens to be zero
//!   mid-release is never marked, so it is never merged.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Largest payload, in bytes, that a freed block parks in an exact size-class bin. Larger blocks
/// join the ordered coalescing free list.
pub(crate) const HEAP_BIN_MAX: u64 = 2048;

/// Bytes of `_heap_small_bins`: one 8-byte chain head per 8-byte payload class from 8 to
/// `HEAP_BIN_MAX`, so the head of the class for payload `n` lives at offset `n - 8`.
pub(crate) const HEAP_BIN_TABLE_BYTES: u64 = HEAP_BIN_MAX;

/// Bytes of `_heap_bin_marks` for a heap of `heap_size` bytes: one byte per 16-byte granule of
/// `_heap_buf`, indexed by a block header's granule. A non-zero byte means the block was parked
/// in a bin by a guarded free (`--heap-debug` or the `--web` heap guard), so a second free of it
/// is caught without walking the bin. Blocks are at least 24 bytes, so two headers never share a
/// granule. Only guarded programs set marks, so other programs never touch the table's pages.
pub(crate) fn heap_bin_mark_bytes(heap_size: usize) -> usize {
    heap_size / 16 + 2
}

/// Kind word stamped on free blocks while a consolidation pass runs. Live headers carry a small
/// heap kind (plus the x86_64 marker in the high half), so they never hold this value.
const HEAP_CONSOLIDATE_MARK: u64 = 0xF4EE_B10C_F4EE_B10C;

/// `__rt_heap_consolidate` mode: merged runs that fit a bin go back to their exact bin.
pub(crate) const HEAP_CONSOLIDATE_REBIN: u64 = 0;

/// `__rt_heap_consolidate` mode: every merged run joins the ordered free list.
pub(crate) const HEAP_CONSOLIDATE_LIST_ALL: u64 = 1;

/// Emits `__rt_heap_consolidate` and `__rt_heap_bins_clear`.
///
/// `__rt_heap_consolidate` empties every bin and the ordered list, then rebuilds them from the
/// heap itself: each maximal run of free blocks becomes one block, and a run reaching the bump
/// end lowers `_heap_off` instead. It is a leaf: it calls nothing and allocates nothing.
///
/// Input: `x0`/`rax` = placement mode. `HEAP_CONSOLIDATE_REBIN` parks merged runs up to
/// `HEAP_BIN_MAX` back in their exact bins (the allocator's retry, which keeps the ordered list
/// short); `HEAP_CONSOLIDATE_LIST_ALL` lists every run (`gc_mem_caches()`, which empties the
/// caches).
///
/// Output: `x0`/`rax` = bytes (payload plus header) taken out of the bins, the figure
/// `gc_mem_caches()` reports. `x1`/`rdx` = non-zero when the pass merged two blocks or lowered
/// the bump end, so an allocation retry can tell progress apart from a heap that is simply
/// full. Moving blocks between the bins and the list is not progress: a block that did not fit
/// before still does not fit, and counting it would retry forever.
///
/// `__rt_heap_bins_clear` empties every bin for the `--web` arena reset that discards the whole
/// heap at once. It walks each chain to clear the blocks' bin marks, so the next request's guarded
/// frees do not mistake a reused granule for a binned block and fall back to scanning.
///
/// Both helpers keep `_heap_bin_marks` in step: a block leaving a bin loses its mark, a run parked
/// back in a bin gains one.
pub fn emit_heap_consolidate(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_heap_consolidate_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: heap_consolidate ---");
    emitter.label_global("__rt_heap_consolidate");
    emitter.instruction("mov x8, x0");                                          // x8 = placement mode for merged runs
    emitter.instruction("mov x0, #0");                                          // no bin bytes drained yet
    emitter.instruction("mov x1, #0");                                          // nothing changed yet
    emit_mark_constant_aarch64(emitter, "x2");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x3", "_heap_buf");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x4", "_heap_off");
    emitter.instruction("ldr x4, [x4]");                                        // load the bump offset
    emitter.instruction("add x4, x3, x4");                                      // x4 = current heap end
    crate::codegen_support::abi::emit_symbol_address(emitter, "x14", "_heap_bin_marks");

    // -- mark every binned block free and empty its bin --
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_heap_small_bins");
    emitter.instruction(&format!("add x10, x9, #{}", HEAP_BIN_TABLE_BYTES));    // x10 = end of the bin-head table
    emitter.label("__rt_heap_consolidate_bin");
    emitter.instruction("cmp x9, x10");                                         // have all bin heads been visited?
    emitter.instruction("b.hs __rt_heap_consolidate_list");                     // yes — mark the ordered list next
    emitter.instruction("ldr x11, [x9]");                                       // x11 = first block of this bin
    emitter.instruction("str xzr, [x9], #8");                                   // empty the bin and advance to the next head
    emitter.label("__rt_heap_consolidate_bin_chain");
    emitter.instruction("cbz x11, __rt_heap_consolidate_bin");                  // chain done, next bin
    emitter.instruction("cmp x11, x3");                                         // does the link point below the heap?
    emitter.instruction("b.lo __rt_heap_consolidate_bin");                      // a wild link: drop the rest of this chain
    emitter.instruction("cmp x11, x4");                                         // does the link point past the heap end?
    emitter.instruction("b.hs __rt_heap_consolidate_bin");                      // a wild link: drop the rest of this chain
    emitter.instruction("ldr x12, [x11, #16]");                                 // the block is in the heap, so its next link is a safe load
    emitter.instruction("ldr w13, [x11, #4]");                                  // a parked block must have refcount zero
    emitter.instruction("cbnz w13, __rt_heap_consolidate_bin_skip");            // a live refcount is poison: leave the block alone
    emitter.instruction("ldr x13, [x11, #8]");                                  // a parked block must have no heap kind
    emitter.instruction("cbnz x13, __rt_heap_consolidate_bin_skip");            // a retained kind is poison: leave the block alone
    emitter.instruction("ldr w13, [x11]");                                      // load the parked payload size
    emitter.instruction("add x0, x0, x13");                                     // count its payload as drained from the bins
    emitter.instruction("add x0, x0, #16");                                     // and its header
    emitter.instruction("str x2, [x11, #8]");                                   // mark the block free for the physical walk
    emitter.instruction("sub x15, x11, x3");                                    // x15 = header offset from the heap base
    emitter.instruction("lsr x15, x15, #4");                                    // x15 = 16-byte granule index of the header
    emitter.instruction("strb wzr, [x14, x15]");                                // the block leaves its bin, so clear its bin mark
    emitter.label("__rt_heap_consolidate_bin_skip");
    emitter.instruction("mov x11, x12");                                        // follow the chain
    emitter.instruction("b __rt_heap_consolidate_bin_chain");                   // keep marking this bin

    // -- mark every ordered-list block free and empty the list --
    emitter.label("__rt_heap_consolidate_list");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_heap_free_list");
    emitter.instruction("ldr x11, [x9]");                                       // x11 = first ordered-list block
    emitter.instruction("str xzr, [x9]");                                       // the list is rebuilt from the walk below
    emitter.label("__rt_heap_consolidate_list_chain");
    emitter.instruction("cbz x11, __rt_heap_consolidate_walk_start");           // list done, walk the heap
    emitter.instruction("cmp x11, x3");                                         // does the link point below the heap?
    emitter.instruction("b.lo __rt_heap_consolidate_walk_start");               // a wild link: drop the rest of the list
    emitter.instruction("cmp x11, x4");                                         // does the link point past the heap end?
    emitter.instruction("b.hs __rt_heap_consolidate_walk_start");               // a wild link: drop the rest of the list
    emitter.instruction("ldr x12, [x11, #16]");                                 // the block is in the heap, so its next link is a safe load
    emitter.instruction("ldr w13, [x11, #4]");                                  // a listed block must have refcount zero
    emitter.instruction("cbnz w13, __rt_heap_consolidate_list_skip");           // a live refcount is poison: leave the block alone
    emitter.instruction("ldr x13, [x11, #8]");                                  // a listed block must have no heap kind
    emitter.instruction("cbnz x13, __rt_heap_consolidate_list_skip");           // a retained kind is poison: leave the block alone
    emitter.instruction("str x2, [x11, #8]");                                   // mark the block free for the physical walk
    emitter.label("__rt_heap_consolidate_list_skip");
    emitter.instruction("mov x11, x12");                                        // follow the list
    emitter.instruction("b __rt_heap_consolidate_list_chain");                  // keep marking the list

    // -- walk the tiled heap, merging each run of marked blocks into one listed block --
    emitter.label("__rt_heap_consolidate_walk_start");
    emitter.instruction("mov x5, x9");                                          // x5 = slot the next listed block is linked into
    emitter.instruction("mov x6, x3");                                          // x6 = cursor at the first physical header
    emitter.label("__rt_heap_consolidate_walk");
    emitter.instruction("cmp x6, x4");                                          // has the cursor reached the bump end?
    emitter.instruction("b.hs __rt_heap_consolidate_done");                     // yes — every block was visited
    emitter.instruction("ldr w12, [x6]");                                       // load this block's payload size
    emitter.instruction("cmp x12, #8");                                         // a header smaller than the minimum payload is corrupt
    emitter.instruction("b.lo __rt_heap_consolidate_done");                     // stop rather than follow a corrupt header
    emitter.instruction("add x7, x6, x12");                                     // x7 = header + payload
    emitter.instruction("add x7, x7, #16");                                     // x7 = the next physical header
    emitter.instruction("cmp x7, x4");                                          // does the block overrun the bump end?
    emitter.instruction("b.hi __rt_heap_consolidate_done");                     // stop rather than follow a corrupt header
    emitter.instruction("ldr x13, [x6, #8]");                                   // load the kind word
    emitter.instruction("cmp x13, x2");                                         // is this block marked free?
    emitter.instruction("b.eq __rt_heap_consolidate_run");                      // yes — start a run here
    emitter.instruction("mov x6, x7");                                          // a live block: step over it
    emitter.instruction("b __rt_heap_consolidate_walk");                        // continue the walk

    emitter.label("__rt_heap_consolidate_run");
    emitter.instruction("cmp x7, x4");                                          // does the run reach the bump end?
    emitter.instruction("b.hs __rt_heap_consolidate_run_end");                  // yes — the run is complete
    emitter.instruction("ldr x13, [x7, #8]");                                   // load the next block's kind word
    emitter.instruction("cmp x13, x2");                                         // is the next block marked free too?
    emitter.instruction("b.ne __rt_heap_consolidate_run_end");                  // no — the run stops before it
    emitter.instruction("ldr w12, [x7]");                                       // load the next block's payload size
    emitter.instruction("cmp x12, #8");                                         // a corrupt header ends the run
    emitter.instruction("b.lo __rt_heap_consolidate_run_end");                  // keep what was merged so far
    emitter.instruction("add x13, x7, x12");                                    // x13 = header + payload
    emitter.instruction("add x13, x13, #16");                                   // x13 = the block after it
    emitter.instruction("cmp x13, x4");                                         // does it overrun the bump end?
    emitter.instruction("b.hi __rt_heap_consolidate_run_end");                  // a corrupt header ends the run
    emitter.instruction("mov x7, x13");                                         // absorb the block into the run
    emitter.instruction("mov x1, #1");                                          // merging changes the allocator state
    emitter.instruction("b __rt_heap_consolidate_run");                         // keep extending the run

    emitter.label("__rt_heap_consolidate_run_end");
    emitter.instruction("cmp x7, x4");                                          // does the run end at the bump end?
    emitter.instruction("b.ne __rt_heap_consolidate_publish");                  // no — it becomes a listed free block
    emitter.instruction("sub x12, x6, x3");                                     // x12 = run start as a heap offset
    crate::codegen_support::abi::emit_symbol_address(emitter, "x13", "_heap_off");
    emitter.instruction("str x12, [x13]");                                      // give the trailing run back to the bump pointer
    emitter.instruction("mov x1, #1");                                          // lowering the bump end changes the allocator state
    emitter.instruction("b __rt_heap_consolidate_done");                        // nothing lies past the bump end
    emitter.label("__rt_heap_consolidate_publish");
    emitter.instruction("sub x12, x7, x6");                                     // x12 = run footprint
    emitter.instruction("sub x12, x12, #16");                                   // x12 = run payload under one header
    emitter.instruction("str w12, [x6]");                                       // write the merged payload size
    emitter.instruction("str wzr, [x6, #4]");                                   // free blocks keep refcount zero
    emitter.instruction("str xzr, [x6, #8]");                                   // and no heap kind
    emitter.instruction(&format!("cmp x8, #{}", HEAP_CONSOLIDATE_REBIN));       // are binnable runs parked back in their bins?
    emitter.instruction("b.ne __rt_heap_consolidate_append");                   // no — every run is listed
    emitter.instruction(&format!("cmp x12, #{}", HEAP_BIN_MAX));                // does the run fit an exact size-class bin?
    emitter.instruction("b.hi __rt_heap_consolidate_append");                   // no — larger runs are listed
    crate::codegen_support::abi::emit_symbol_address(emitter, "x13", "_heap_small_bins");
    emitter.instruction("add x13, x13, x12");                                   // the head for payload n lives at offset n - 8
    emitter.instruction("sub x13, x13, #8");                                    // x13 = this run's bin head
    emitter.instruction("ldr x12, [x13]");                                      // x12 = the bin's current chain
    emitter.instruction("str x12, [x6, #16]");                                  // chain the run in front of it
    emitter.instruction("str x6, [x13]");                                       // publish the run as the bin head
    emitter.instruction("sub x15, x6, x3");                                     // x15 = run header offset from the heap base
    emitter.instruction("lsr x15, x15, #4");                                    // x15 = 16-byte granule index of the run header
    emitter.instruction("mov w16, #1");                                         // a rebinned run is binned
    emitter.instruction("strb w16, [x14, x15]");                                // so its header carries the bin mark
    emitter.instruction("mov x6, x7");                                          // resume the walk after the run
    emitter.instruction("b __rt_heap_consolidate_walk");                        // continue the walk
    emitter.label("__rt_heap_consolidate_append");
    emitter.instruction("str xzr, [x6, #16]");                                  // the newest listed block ends the list
    emitter.instruction("str x6, [x5]");                                        // append it in address order
    emitter.instruction("add x5, x6, #16");                                     // the next block links through this one
    emitter.instruction("mov x6, x7");                                          // resume the walk after the run
    emitter.instruction("b __rt_heap_consolidate_walk");                        // continue the walk

    emitter.label("__rt_heap_consolidate_done");
    emitter.instruction("ret");                                                 // x0 = drained bin bytes, x1 = changed flag

    emitter.blank();
    emitter.comment("--- runtime: heap_bins_clear ---");
    emitter.label_global("__rt_heap_bins_clear");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x11", "_heap_buf");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x12", "_heap_max");
    emitter.instruction("ldr x12, [x12]");                                      // load the heap capacity
    emitter.instruction("add x12, x11, x12");                                   // x12 = end of the heap buffer (the bump end may already be reset)
    crate::codegen_support::abi::emit_symbol_address(emitter, "x13", "_heap_bin_marks");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_heap_small_bins");
    emitter.instruction(&format!("add x10, x9, #{}", HEAP_BIN_TABLE_BYTES));    // x10 = end of the bin-head table
    emitter.label("__rt_heap_bins_clear_loop");
    emitter.instruction("cmp x9, x10");                                         // have all bins been emptied?
    emitter.instruction("b.hs __rt_heap_bins_clear_done");                      // yes — done
    emitter.instruction("ldr x14, [x9]");                                       // x14 = first block of this bin
    emitter.instruction("str xzr, [x9], #8");                                   // empty the bin and advance to the next head
    emitter.label("__rt_heap_bins_clear_chain");
    emitter.instruction("cbz x14, __rt_heap_bins_clear_loop");                  // chain done, next bin
    emitter.instruction("cmp x14, x11");                                        // does the link point below the heap?
    emitter.instruction("b.lo __rt_heap_bins_clear_loop");                      // a wild link: drop the rest of this chain
    emitter.instruction("cmp x14, x12");                                        // does the link point past the heap buffer?
    emitter.instruction("b.hs __rt_heap_bins_clear_loop");                      // a wild link: drop the rest of this chain
    emitter.instruction("sub x15, x14, x11");                                   // x15 = header offset from the heap base
    emitter.instruction("lsr x15, x15, #4");                                    // x15 = 16-byte granule index of the header
    emitter.instruction("strb wzr, [x13, x15]");                                // the discarded block is no longer binned
    emitter.instruction("ldr x14, [x14, #16]");                                 // follow the chain
    emitter.instruction("b __rt_heap_bins_clear_chain");                        // keep clearing this bin's marks
    emitter.label("__rt_heap_bins_clear_done");
    emitter.instruction("ret");                                                 // every bin is empty and no block keeps a bin mark
}

/// Materializes `HEAP_CONSOLIDATE_MARK` into an AArch64 register.
fn emit_mark_constant_aarch64(emitter: &mut Emitter, register: &str) {
    for shift in [0u32, 16, 32, 48] {
        let half = (HEAP_CONSOLIDATE_MARK >> shift) & 0xffff;
        let op = if shift == 0 { "movz" } else { "movk" };
        emitter.instruction(&format!("{op} {register}, #0x{half:x}, lsl #{shift}")); // build the free-block mark 16 bits at a time
    }
}

/// Emits the x86_64 `__rt_heap_consolidate` and `__rt_heap_bins_clear` (same contract as AArch64).
fn emit_heap_consolidate_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: heap_consolidate ---");
    emitter.label_global("__rt_heap_consolidate");
    emitter.instruction("push rax");                                            // keep the placement mode at [rsp + 8] for the publish step
    emitter.instruction("push r12");                                            // r12 is callee-saved and holds the bin-mark table for the whole pass
    crate::codegen_support::abi::emit_symbol_address(emitter, "r12", "_heap_bin_marks");
    emitter.instruction("xor eax, eax");                                        // no bin bytes drained yet
    emitter.instruction("xor edx, edx");                                        // nothing changed yet
    emitter.instruction(&format!("mov rdi, 0x{:x}", HEAP_CONSOLIDATE_MARK));   // rdi = the free-block mark
    crate::codegen_support::abi::emit_symbol_address(emitter, "rsi", "_heap_buf");
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_heap_off");
    emitter.instruction("mov r8, QWORD PTR [r8]");                              // load the bump offset
    emitter.instruction("add r8, rsi");                                         // r8 = current heap end

    // -- mark every binned block free and empty its bin --
    crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_heap_small_bins");
    emitter.instruction(&format!("lea r10, [r9 + {}]", HEAP_BIN_TABLE_BYTES));  // r10 = end of the bin-head table
    emitter.label("__rt_heap_consolidate_bin");
    emitter.instruction("cmp r9, r10");                                         // have all bin heads been visited?
    emitter.instruction("jae __rt_heap_consolidate_list");                      // yes — mark the ordered list next
    emitter.instruction("mov r11, QWORD PTR [r9]");                             // r11 = first block of this bin
    emitter.instruction("mov QWORD PTR [r9], 0");                               // empty the bin
    emitter.instruction("add r9, 8");                                           // advance to the next head
    emitter.label("__rt_heap_consolidate_bin_chain");
    emitter.instruction("test r11, r11");                                       // is the chain done?
    emitter.instruction("jz __rt_heap_consolidate_bin");                        // yes — next bin
    emitter.instruction("cmp r11, rsi");                                        // does the link point below the heap?
    emitter.instruction("jb __rt_heap_consolidate_bin");                        // a wild link: drop the rest of this chain
    emitter.instruction("cmp r11, r8");                                         // does the link point past the heap end?
    emitter.instruction("jae __rt_heap_consolidate_bin");                       // a wild link: drop the rest of this chain
    emitter.instruction("mov rcx, QWORD PTR [r11 + 16]");                       // the block is in the heap, so its next link is a safe load
    emitter.instruction("cmp DWORD PTR [r11 + 4], 0");                          // a parked block must have refcount zero
    emitter.instruction("jne __rt_heap_consolidate_bin_skip");                  // a live refcount is poison: leave the block alone
    emitter.instruction("cmp QWORD PTR [r11 + 8], 0");                          // a parked block must have no heap kind
    emitter.instruction("jne __rt_heap_consolidate_bin_skip");                  // a retained kind is poison: leave the block alone
    emitter.instruction("mov ecx, DWORD PTR [r11]");                            // load the parked payload size (the link is reloaded below)
    emitter.instruction("lea rax, [rax + rcx + 16]");                           // count its payload and header as drained
    emitter.instruction("mov rcx, r11");                                        // rcx = header address
    emitter.instruction("sub rcx, rsi");                                        // rcx = header offset from the heap base
    emitter.instruction("shr rcx, 4");                                          // rcx = 16-byte granule index of the header
    emitter.instruction("mov BYTE PTR [r12 + rcx], 0");                         // the block leaves its bin, so clear its bin mark
    emitter.instruction("mov rcx, QWORD PTR [r11 + 16]");                       // reload the next link
    emitter.instruction("mov QWORD PTR [r11 + 8], rdi");                        // mark the block free for the physical walk
    emitter.label("__rt_heap_consolidate_bin_skip");
    emitter.instruction("mov r11, rcx");                                        // follow the chain
    emitter.instruction("jmp __rt_heap_consolidate_bin_chain");                 // keep marking this bin

    // -- mark every ordered-list block free and empty the list --
    emitter.label("__rt_heap_consolidate_list");
    crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_heap_free_list");
    emitter.instruction("mov r11, QWORD PTR [r9]");                             // r11 = first ordered-list block
    emitter.instruction("mov QWORD PTR [r9], 0");                               // the list is rebuilt from the walk below
    emitter.label("__rt_heap_consolidate_list_chain");
    emitter.instruction("test r11, r11");                                       // is the list done?
    emitter.instruction("jz __rt_heap_consolidate_walk_start");                 // yes — walk the heap
    emitter.instruction("cmp r11, rsi");                                        // does the link point below the heap?
    emitter.instruction("jb __rt_heap_consolidate_walk_start");                 // a wild link: drop the rest of the list
    emitter.instruction("cmp r11, r8");                                         // does the link point past the heap end?
    emitter.instruction("jae __rt_heap_consolidate_walk_start");                // a wild link: drop the rest of the list
    emitter.instruction("mov rcx, QWORD PTR [r11 + 16]");                       // the block is in the heap, so its next link is a safe load
    emitter.instruction("cmp DWORD PTR [r11 + 4], 0");                          // a listed block must have refcount zero
    emitter.instruction("jne __rt_heap_consolidate_list_skip");                 // a live refcount is poison: leave the block alone
    emitter.instruction("cmp QWORD PTR [r11 + 8], 0");                          // a listed block must have no heap kind
    emitter.instruction("jne __rt_heap_consolidate_list_skip");                 // a retained kind is poison: leave the block alone
    emitter.instruction("mov QWORD PTR [r11 + 8], rdi");                        // mark the block free for the physical walk
    emitter.label("__rt_heap_consolidate_list_skip");
    emitter.instruction("mov r11, rcx");                                        // follow the list
    emitter.instruction("jmp __rt_heap_consolidate_list_chain");                // keep marking the list

    // -- walk the tiled heap, merging each run of marked blocks into one listed block --
    emitter.label("__rt_heap_consolidate_walk_start");
    emitter.instruction("mov r10, r9");                                         // r10 = slot the next listed block is linked into
    emitter.instruction("mov r9, rsi");                                         // r9 = cursor at the first physical header
    emitter.label("__rt_heap_consolidate_walk");
    emitter.instruction("cmp r9, r8");                                          // has the cursor reached the bump end?
    emitter.instruction("jae __rt_heap_consolidate_done");                      // yes — every block was visited
    emitter.instruction("mov ecx, DWORD PTR [r9]");                             // load this block's payload size
    emitter.instruction("cmp rcx, 8");                                          // a header smaller than the minimum payload is corrupt
    emitter.instruction("jb __rt_heap_consolidate_done");                       // stop rather than follow a corrupt header
    emitter.instruction("lea r11, [r9 + rcx + 16]");                            // r11 = the next physical header
    emitter.instruction("cmp r11, r8");                                         // does the block overrun the bump end?
    emitter.instruction("ja __rt_heap_consolidate_done");                       // stop rather than follow a corrupt header
    emitter.instruction("cmp QWORD PTR [r9 + 8], rdi");                         // is this block marked free?
    emitter.instruction("je __rt_heap_consolidate_run");                        // yes — start a run here
    emitter.instruction("mov r9, r11");                                         // a live block: step over it
    emitter.instruction("jmp __rt_heap_consolidate_walk");                      // continue the walk

    emitter.label("__rt_heap_consolidate_run");
    emitter.instruction("cmp r11, r8");                                         // does the run reach the bump end?
    emitter.instruction("jae __rt_heap_consolidate_run_end");                   // yes — the run is complete
    emitter.instruction("cmp QWORD PTR [r11 + 8], rdi");                        // is the next block marked free too?
    emitter.instruction("jne __rt_heap_consolidate_run_end");                   // no — the run stops before it
    emitter.instruction("mov ecx, DWORD PTR [r11]");                            // load the next block's payload size
    emitter.instruction("cmp rcx, 8");                                          // a corrupt header ends the run
    emitter.instruction("jb __rt_heap_consolidate_run_end");                    // keep what was merged so far
    emitter.instruction("lea rcx, [r11 + rcx + 16]");                           // rcx = the block after it
    emitter.instruction("cmp rcx, r8");                                         // does it overrun the bump end?
    emitter.instruction("ja __rt_heap_consolidate_run_end");                    // a corrupt header ends the run
    emitter.instruction("mov r11, rcx");                                        // absorb the block into the run
    emitter.instruction("mov edx, 1");                                          // merging changes the allocator state
    emitter.instruction("jmp __rt_heap_consolidate_run");                       // keep extending the run

    emitter.label("__rt_heap_consolidate_run_end");
    emitter.instruction("cmp r11, r8");                                         // does the run end at the bump end?
    emitter.instruction("jne __rt_heap_consolidate_publish");                   // no — it becomes a listed free block
    emitter.instruction("mov rcx, r9");                                         // rcx = run start
    emitter.instruction("sub rcx, rsi");                                        // as a heap offset
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_heap_off");
    emitter.instruction("mov QWORD PTR [r11], rcx");                            // give the trailing run back to the bump pointer
    emitter.instruction("mov edx, 1");                                          // lowering the bump end changes the allocator state
    emitter.instruction("jmp __rt_heap_consolidate_done");                      // nothing lies past the bump end
    emitter.label("__rt_heap_consolidate_publish");
    emitter.instruction("mov rcx, r11");                                        // rcx = run end
    emitter.instruction("sub rcx, r9");                                         // rcx = run footprint
    emitter.instruction("sub rcx, 16");                                         // rcx = run payload under one header
    emitter.instruction("mov DWORD PTR [r9], ecx");                             // write the merged payload size
    emitter.instruction("mov DWORD PTR [r9 + 4], 0");                           // free blocks keep refcount zero
    emitter.instruction("mov QWORD PTR [r9 + 8], 0");                           // and no heap kind
    emitter.instruction(&format!("cmp QWORD PTR [rsp + 8], {}", HEAP_CONSOLIDATE_REBIN)); // are binnable runs parked back in their bins?
    emitter.instruction("jne __rt_heap_consolidate_append");                    // no — every run is listed
    emitter.instruction(&format!("cmp rcx, {}", HEAP_BIN_MAX));                 // does the run fit an exact size-class bin?
    emitter.instruction("ja __rt_heap_consolidate_append");                     // no — larger runs are listed
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_heap_small_bins");
    emitter.instruction("lea r8, [r8 + rcx - 8]");                              // r8 = this run's bin head (payload n at offset n - 8)
    emitter.instruction("mov rcx, QWORD PTR [r8]");                             // rcx = the bin's current chain
    emitter.instruction("mov QWORD PTR [r9 + 16], rcx");                        // chain the run in front of it
    emitter.instruction("mov QWORD PTR [r8], r9");                              // publish the run as the bin head
    emitter.instruction("mov rcx, r9");                                         // rcx = run header address
    emitter.instruction("sub rcx, rsi");                                        // rcx = run header offset from the heap base
    emitter.instruction("shr rcx, 4");                                          // rcx = 16-byte granule index of the run header
    emitter.instruction("mov BYTE PTR [r12 + rcx], 1");                         // a rebinned run carries the bin mark
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_heap_off");
    emitter.instruction("mov r8, QWORD PTR [r8]");                              // reload the bump offset the bin address displaced
    emitter.instruction("add r8, rsi");                                         // r8 = current heap end again
    emitter.instruction("mov r9, r11");                                         // resume the walk after the run
    emitter.instruction("jmp __rt_heap_consolidate_walk");                      // continue the walk
    emitter.label("__rt_heap_consolidate_append");
    emitter.instruction("mov QWORD PTR [r9 + 16], 0");                          // the newest listed block ends the list
    emitter.instruction("mov QWORD PTR [r10], r9");                             // append it in address order
    emitter.instruction("lea r10, [r9 + 16]");                                  // the next block links through this one
    emitter.instruction("mov r9, r11");                                         // resume the walk after the run
    emitter.instruction("jmp __rt_heap_consolidate_walk");                      // continue the walk

    emitter.label("__rt_heap_consolidate_done");
    emitter.instruction("pop r12");                                             // restore the callee-saved register
    emitter.instruction("add rsp, 8");                                          // drop the saved placement mode
    emitter.instruction("ret");                                                 // rax = drained bin bytes, rdx = changed flag

    emitter.blank();
    emitter.comment("--- runtime: heap_bins_clear ---");
    emitter.label_global("__rt_heap_bins_clear");
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_heap_buf");
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_heap_max");
    emitter.instruction("mov r8, QWORD PTR [r8]");                              // load the heap capacity
    emitter.instruction("add r8, r11");                                         // r8 = end of the heap buffer (the bump end may already be reset)
    crate::codegen_support::abi::emit_symbol_address(emitter, "rdx", "_heap_bin_marks");
    crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_heap_small_bins");
    emitter.instruction(&format!("lea r10, [r9 + {}]", HEAP_BIN_TABLE_BYTES));  // r10 = end of the bin-head table
    emitter.label("__rt_heap_bins_clear_loop");
    emitter.instruction("cmp r9, r10");                                         // have all bins been emptied?
    emitter.instruction("jae __rt_heap_bins_clear_done");                       // yes — done
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // rax = first block of this bin
    emitter.instruction("mov QWORD PTR [r9], 0");                               // empty this bin
    emitter.instruction("add r9, 8");                                           // advance to the next head
    emitter.label("__rt_heap_bins_clear_chain");
    emitter.instruction("test rax, rax");                                       // is the chain done?
    emitter.instruction("jz __rt_heap_bins_clear_loop");                        // yes — next bin
    emitter.instruction("cmp rax, r11");                                        // does the link point below the heap?
    emitter.instruction("jb __rt_heap_bins_clear_loop");                        // a wild link: drop the rest of this chain
    emitter.instruction("cmp rax, r8");                                         // does the link point past the heap buffer?
    emitter.instruction("jae __rt_heap_bins_clear_loop");                       // a wild link: drop the rest of this chain
    emitter.instruction("mov rcx, rax");                                        // rcx = header address
    emitter.instruction("sub rcx, r11");                                        // rcx = header offset from the heap base
    emitter.instruction("shr rcx, 4");                                          // rcx = 16-byte granule index of the header
    emitter.instruction("mov BYTE PTR [rdx + rcx], 0");                         // the discarded block is no longer binned
    emitter.instruction("mov rax, QWORD PTR [rax + 16]");                       // follow the chain
    emitter.instruction("jmp __rt_heap_bins_clear_chain");                      // keep clearing this bin's marks
    emitter.label("__rt_heap_bins_clear_done");
    emitter.instruction("ret");                                                 // every bin is empty and no block keeps a bin mark
}
