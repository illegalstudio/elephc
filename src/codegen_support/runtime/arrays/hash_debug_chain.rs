//! Purpose:
//! Emits `__rt_hash_debug_validate_chain`, the `--heap-debug` check that a hash's insertion-order
//! chain still describes the table it belongs to.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Reads only; preserves the table pointer so a caller can validate before using it.
//! - Entries live in the separate storage `hash_layout` addresses, not after the header.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// The diagnostic a broken insertion-order chain reports.
pub(crate) const HASH_CHAIN_MSG: &str =
    "Fatal error: heap debug detected a hash whose insertion order left the table\n";

/// Emits `__rt_hash_debug_validate_chain`: walk a hash's insertion order and refuse a chain that
/// leaves the table.
///
/// A corrupted chain is invisible until something iterates it, and by then the walk is already
/// reading whatever sits in an unoccupied slot -- for the Symfony `--web` preload build that
/// surfaces as `__rt_hash_bytes` hashing a foreign heap header as a five-byte key, four requests
/// into a worker's life and nowhere near whatever broke it. Walking the chain where a hash is
/// about to be USED turns that into a report at the first operation that sees the damage.
///
/// Three things must hold: every visited slot index is inside the table, every visited slot is
/// occupied (1, not empty or a tombstone), and the walk visits exactly as many slots as the
/// header's live count. The capacity bound also terminates a cycle.
///
/// Input: `x0` / `rax` = hash table pointer, preserved. Clobbers the scratch registers only.
pub fn emit_hash_debug_validate_chain(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_debug_validate_chain_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_debug_validate_chain ---");
    emitter.label_global("__rt_hash_debug_validate_chain");

    emitter.instruction("cbz x0, __rt_hash_debug_validate_chain_done");         // a null table has no chain to check
    emitter.instruction("ldr x10, [x0]");                                       // x10 = live entry count from the header
    emitter.instruction("ldr x11, [x0, #8]");                                   // x11 = table capacity in slots
    emitter.instruction("ldr x12, [x0, #24]");                                  // x12 = head slot index
    emitter.instruction("mov x13, #0");                                         // x13 = slots visited so far

    emitter.label("__rt_hash_debug_validate_chain_loop");
    emitter.instruction("cmn x12, #1");                                         // has the chain reached its terminator?
    emitter.instruction("b.eq __rt_hash_debug_validate_chain_end");             // yes — compare the walk length with the count
    emitter.instruction("cmp x12, x11");                                        // is the slot index inside the table?
    emitter.instruction("b.hs __rt_hash_debug_validate_chain_fail");            // no — the chain left the table
    super::hash_layout::emit_entry_address(emitter, "x15", "x0", "x12");
    emitter.instruction("ldr x16, [x15]");                                      // load the slot's occupancy flag
    emitter.instruction("cmp x16, #1");                                         // is the visited slot actually occupied?
    emitter.instruction("b.ne __rt_hash_debug_validate_chain_fail");            // no — insertion order points at empty or tombstoned storage
    emitter.instruction("ldr x17, [x15, #16]");                                 // load the visited entry's key length
    emitter.instruction("cmn x17, #1");                                         // is this an inline integer key?
    emitter.instruction("b.eq __rt_hash_debug_validate_chain_counted");         // integer keys carry no pointer to check
    emitter.instruction("ldr x17, [x15, #8]");                                  // load the string key payload pointer
    crate::codegen_support::abi::emit_symbol_address(emitter, "x14", "_heap_buf");
    emitter.instruction("add x16, x14, #16");                                   // the first address a payload can start at
    emitter.instruction("cmp x17, x16");                                        // is the key below the first payload?
    emitter.instruction("b.lo __rt_hash_debug_validate_chain_fail");            // yes — that is not arena storage
    crate::codegen_support::abi::emit_symbol_address(emitter, "x16", "_heap_off");
    emitter.instruction("ldr x16, [x16]");                                      // load the current bump offset
    emitter.instruction("add x16, x14, x16");                                   // compute the live heap end
    emitter.instruction("cmp x17, x16");                                        // is the key at or past the live heap end?
    emitter.instruction("b.hs __rt_hash_debug_validate_chain_fail");            // yes — that is not arena storage either
    emitter.instruction("sub x17, x17, x14");                                   // offset of the key within the arena
    emitter.instruction("and x17, x17, #7");                                    // payload pointers are 8-byte aligned
    emitter.instruction("cbnz x17, __rt_hash_debug_validate_chain_fail");       // a misaligned key is a header word, not a pointer
    emitter.label("__rt_hash_debug_validate_chain_counted");
    emitter.instruction("add x13, x13, #1");                                    // count this slot as visited
    emitter.instruction("cmp x13, x11");                                        // has the walk exceeded the table's capacity?
    emitter.instruction("b.hi __rt_hash_debug_validate_chain_fail");            // yes — the chain contains a cycle
    emitter.instruction("ldr x12, [x15, #56]");                                 // follow the insertion-order successor
    emitter.instruction("b __rt_hash_debug_validate_chain_loop");               // keep walking

    emitter.label("__rt_hash_debug_validate_chain_end");
    emitter.instruction("cmp x13, x10");                                        // did the walk visit exactly the live entries?
    emitter.instruction("b.ne __rt_hash_debug_validate_chain_fail");            // no — the chain and the count disagree

    emitter.label("__rt_hash_debug_validate_chain_done");
    emitter.instruction("ret");                                                 // the chain describes this table

    emitter.label("__rt_hash_debug_validate_chain_fail");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_heap_dbg_chain_msg");
    emitter.instruction(&format!("mov x2, #{}", HASH_CHAIN_MSG.len()));         // pass the exact chain-diagnostic length
    emitter.instruction("b __rt_heap_debug_fail");                              // report the broken chain and terminate immediately
}

/// x86_64 Linux variant of [`emit_hash_debug_validate_chain`].
///
/// Input: `rax` = hash table pointer, preserved, matching the other internal heap helpers.
fn emit_hash_debug_validate_chain_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_debug_validate_chain ---");
    emitter.label_global("__rt_hash_debug_validate_chain");

    emitter.instruction("test rax, rax");                                       // a null table has no chain to check
    emitter.instruction("jz __rt_hash_debug_validate_chain_done");              // nothing to walk
    emitter.instruction("mov r8, QWORD PTR [rax]");                             // r8 = live entry count from the header
    emitter.instruction("mov r9, QWORD PTR [rax + 8]");                         // r9 = table capacity in slots
    emitter.instruction("mov r10, QWORD PTR [rax + 24]");                       // r10 = head slot index
    emitter.instruction("xor r11d, r11d");                                      // r11 = slots visited so far

    emitter.label("__rt_hash_debug_validate_chain_loop");
    emitter.instruction("cmp r10, -1");                                         // has the chain reached its terminator?
    emitter.instruction("je __rt_hash_debug_validate_chain_end");               // yes — compare the walk length with the count
    emitter.instruction("cmp r10, r9");                                         // is the slot index inside the table?
    emitter.instruction("jae __rt_hash_debug_validate_chain_fail");             // no — the chain left the table
    super::hash_layout::emit_entry_address(emitter, "rcx", "rax", "r10");
    emitter.instruction("cmp QWORD PTR [rcx], 1");                              // is the visited slot actually occupied?
    emitter.instruction("jne __rt_hash_debug_validate_chain_fail");             // no — insertion order points at empty or tombstoned storage
    emitter.instruction("cmp QWORD PTR [rcx + 16], -1");                        // is this an inline integer key?
    emitter.instruction("je __rt_hash_debug_validate_chain_counted");           // integer keys carry no pointer to check
    emitter.instruction("mov rdi, QWORD PTR [rcx + 8]");                        // load the string key payload pointer
    crate::codegen_support::abi::emit_symbol_address(emitter, "rsi", "_heap_buf");
    emitter.instruction("lea rdx, [rsi + 16]");                                 // the first address a payload can start at
    emitter.instruction("cmp rdi, rdx");                                        // is the key below the first payload?
    emitter.instruction("jb __rt_hash_debug_validate_chain_fail");              // yes — that is not arena storage
    crate::codegen_support::abi::emit_symbol_address(emitter, "rdx", "_heap_off");
    emitter.instruction("mov rdx, QWORD PTR [rdx]");                            // load the current bump offset
    emitter.instruction("add rdx, rsi");                                        // compute the live heap end
    emitter.instruction("cmp rdi, rdx");                                        // is the key at or past the live heap end?
    emitter.instruction("jae __rt_hash_debug_validate_chain_fail");             // yes — that is not arena storage either
    emitter.instruction("sub rdi, rsi");                                        // offset of the key within the arena
    emitter.instruction("and rdi, 7");                                          // payload pointers are 8-byte aligned
    emitter.instruction("jnz __rt_hash_debug_validate_chain_fail");             // a misaligned key is a header word, not a pointer
    emitter.label("__rt_hash_debug_validate_chain_counted");
    emitter.instruction("inc r11");                                             // count this slot as visited
    emitter.instruction("cmp r11, r9");                                         // has the walk exceeded the table's capacity?
    emitter.instruction("ja __rt_hash_debug_validate_chain_fail");              // yes — the chain contains a cycle
    emitter.instruction("mov r10, QWORD PTR [rcx + 56]");                       // follow the insertion-order successor
    emitter.instruction("jmp __rt_hash_debug_validate_chain_loop");             // keep walking

    emitter.label("__rt_hash_debug_validate_chain_end");
    emitter.instruction("cmp r11, r8");                                         // did the walk visit exactly the live entries?
    emitter.instruction("jne __rt_hash_debug_validate_chain_fail");             // no — the chain and the count disagree

    emitter.label("__rt_hash_debug_validate_chain_done");
    emitter.instruction("ret");                                                 // the chain describes this table

    emitter.label("__rt_hash_debug_validate_chain_fail");
    crate::codegen_support::abi::emit_symbol_address(emitter, "rsi", "_heap_dbg_chain_msg");
    emitter.instruction(&format!("mov rdx, {}", HASH_CHAIN_MSG.len()));         // pass the exact chain-diagnostic length
    emitter.instruction("jmp __rt_heap_debug_fail");                            // report the broken chain and terminate immediately
}
