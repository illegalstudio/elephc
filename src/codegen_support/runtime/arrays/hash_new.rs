//! Purpose:
//! Allocates stable associative-array headers and independently owned entry storage.
//!
//! Called from:
//! - Shared hash constructors and growth helpers on every supported target.
//!
//! Key details:
//! - The header owns a raw heap allocation at offset 40; entries are 64 bytes each.
//! - Capacity is nonnegative and checked before multiplication or allocation.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::runtime::{arrays::hash_layout, data::ARRAY_ALLOC_SIZE_MSG};

/// Allocates an empty hash from capacity and value type, returning its stable header.
/// AArch64 uses x0/x1 and returns x0; x86_64 uses rdi/rsi and returns rax.
pub fn emit_hash_new(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }
    emitter.blank();
    emitter.comment("--- runtime: hash_new ---");
    emitter.label_global("__rt_hash_new");
    emitter.instruction("cmp x0, #0");                                          // normalize negative capacities before saving metadata
    emitter.instruction("csel x0, x0, xzr, ge");                                // use zero for an empty entry allocation
    emitter.instruction("lsr x9, x0, #57");                                     // check that capacity times 64 fits a signed word
    emitter.instruction("cbnz x9, __rt_hash_cap_overflow");                     // reject unrepresentable entry storage
    emitter.instruction("sub sp, sp, #48");                                     // reserve constructor state and linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve caller linkage
    emitter.instruction("add x29, sp, #32");                                    // establish the constructor frame
    emitter.instruction("str x0, [sp]");                                        // preserve normalized capacity
    emitter.instruction("str x1, [sp, #8]");                                    // preserve table-wide value metadata
    emitter.instruction("lsl x0, x0, #6");                                      // request 64 bytes per entry
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate raw entry storage with no independent child ownership
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the entry allocation across header allocation
    emitter.instruction(&format!("mov x0, #{}", hash_layout::HEADER_SIZE));     // request the stable header size
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the stable associative-array identity
    emitter.instruction("mov x9, #0x8003");                                     // mark a copy-on-write associative-array owner
    emitter.instruction("str x9, [x0, #-8]");                                   // publish the typed heap kind
    emitter.instruction("str xzr, [x0]");                                       // initialize the live-entry count
    emitter.instruction("ldr x9, [sp]");                                        // recover normalized capacity
    emitter.instruction("str x9, [x0, #8]");                                    // publish the entry capacity
    emitter.instruction("ldr x10, [sp, #8]");                                   // recover the table-wide value type
    emitter.instruction("str x10, [x0, #16]");                                  // publish value metadata
    emitter.instruction("mov x10, #-1");                                        // represent an empty insertion-order chain
    emitter.instruction("str x10, [x0, #24]");                                  // initialize the head index
    emitter.instruction("str x10, [x0, #32]");                                  // initialize the tail index
    emitter.instruction("ldr x11, [sp, #16]");                                  // recover the independently owned entry allocation
    emitter.instruction("str x11, [x0, #40]");                                  // publish entry ownership in the stable header
    emitter.instruction(&format!("str xzr, [x0, #{}]", hash_layout::PINS_OFFSET)); // new arrays have no internal lifetime pins
    abi::emit_load_int_immediate(emitter, "x10", i64::MIN);
    emitter.instruction(&format!("str x10, [x0, #{}]", hash_layout::NEXT_INDEX_OFFSET)); // no integer key has advanced the append counter
    emitter.label("__rt_hash_new_zero");
    emitter.instruction("cbz x9, __rt_hash_new_done");                          // finish after clearing every occupied marker
    emitter.instruction("str xzr, [x11]");                                      // initialize an empty entry
    emitter.instruction("add x11, x11, #64");                                   // advance to the next entry
    emitter.instruction("sub x9, x9, #1");                                      // count the remaining entries
    emitter.instruction("b __rt_hash_new_zero");                                // continue entry initialization
    emitter.label("__rt_hash_new_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore caller linkage
    emitter.instruction("add sp, sp, #48");                                     // release constructor state
    emitter.instruction("ret");                                                 // return the stable hash header
    emitter.label("__rt_hash_cap_overflow");
    emitter.instruction("mov x0, #2");                                          // send the allocation diagnostic to stderr
    abi::emit_symbol_address(emitter, "x1", "_arr_cap_err_msg");
    emitter.instruction(&format!("mov x2, #{}", ARRAY_ALLOC_SIZE_MSG.len()));   // provide the exact diagnostic length
    emitter.syscall(4);
    abi::emit_cdylib_exit_escape(emitter);
    emitter.instruction("mov x0, #1");                                          // report allocation failure
    emitter.syscall(1);
}

/// Allocates and initializes the stable hash representation using the x86_64 heap ABI.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_new ---");
    emitter.label_global("__rt_hash_new");
    emitter.instruction("push rbp");                                            // preserve caller linkage
    emitter.instruction("mov rbp, rsp");                                        // establish the constructor frame
    emitter.instruction("sub rsp, 32");                                         // reserve capacity, value type, and entry allocation
    emitter.instruction("xor eax, eax");                                        // default to an empty entry region
    emitter.instruction("test rdi, rdi");                                       // classify the requested capacity
    emitter.instruction("cmovg rax, rdi");                                      // normalize negative capacities to zero
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve normalized capacity
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve table-wide value metadata
    emitter.instruction("imul rax, 64");                                        // request 64 bytes per entry
    emitter.instruction("jo __rt_hash_cap_overflow");                           // reject an unrepresentable entry allocation
    emitter.instruction("call __rt_heap_alloc");                                // allocate raw entry storage
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve its sole owner before header allocation
    emitter.instruction(&format!("mov eax, {}", hash_layout::HEADER_SIZE));     // request the stable header size
    emitter.instruction("call __rt_heap_alloc");                                // allocate the stable associative-array identity
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(0x8003))); // identify a copy-on-write hash owner
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // publish the typed heap kind
    emitter.instruction("mov QWORD PTR [rax], 0");                              // initialize the live-entry count
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // recover normalized capacity
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // publish the entry capacity
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // recover the table-wide value type
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // publish value metadata
    emitter.instruction("mov QWORD PTR [rax + 24], -1");                        // initialize the empty head index
    emitter.instruction("mov QWORD PTR [rax + 32], -1");                        // initialize the empty tail index
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // recover the independently owned entry allocation
    emitter.instruction("mov QWORD PTR [rax + 40], r10");                       // publish entry ownership in the stable header
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", hash_layout::PINS_OFFSET)); // new arrays have no internal lifetime pins
    abi::emit_load_int_immediate(emitter, "r11", i64::MIN);
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], r11", hash_layout::NEXT_INDEX_OFFSET)); // no integer key has advanced the append counter
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // recover the number of occupied markers to clear
    emitter.label("__rt_hash_new_zero");
    emitter.instruction("test r11, r11");                                       // check the remaining entry count
    emitter.instruction("jz __rt_hash_new_done");                               // finish after clearing every occupied marker
    emitter.instruction("mov QWORD PTR [r10], 0");                              // initialize an empty entry
    emitter.instruction("add r10, 64");                                         // advance to the next entry
    emitter.instruction("sub r11, 1");                                          // count the remaining entries
    emitter.instruction("jmp __rt_hash_new_zero");                              // continue entry initialization
    emitter.label("__rt_hash_new_done");
    emitter.instruction("add rsp, 32");                                         // release constructor state
    emitter.instruction("pop rbp");                                             // restore caller linkage
    emitter.instruction("ret");                                                 // return the stable hash header
    emitter.label("__rt_hash_cap_overflow");
    emitter.instruction("mov edi, 2");                                          // send the allocation diagnostic to stderr
    abi::emit_symbol_address(emitter, "rsi", "_arr_cap_err_msg");
    emitter.instruction(&format!("mov edx, {}", ARRAY_ALLOC_SIZE_MSG.len()));   // provide the exact diagnostic length
    emitter.instruction("mov eax, 1");                                          // select the Linux write syscall
    emitter.instruction("syscall");                                             // write the allocation diagnostic
    abi::emit_cdylib_exit_escape(emitter);
    emitter.instruction("mov edi, 1");                                          // report allocation failure
    emitter.instruction("mov eax, 60");                                         // select the Linux exit syscall
    emitter.instruction("syscall");                                             // terminate after reporting allocation failure
}
