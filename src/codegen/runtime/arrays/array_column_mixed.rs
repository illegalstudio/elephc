//! Purpose:
//! Emits the `__rt_array_column_mixed` runtime helper for mixed column extraction.
//! It boxes each found hash payload into an owned Mixed cell before appending it.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Hash lookups return tag plus payload words; Mixed indexed-array slots store boxed cells.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_array_column_mixed` runtime helper.
/// Dispatches to the target-specific implementation based on `emitter.target`.
///
/// Inputs (ARM64 AAPCS64):
/// - x0: outer indexed array (Array of AssocArray)
/// - x1: column key string pointer
/// - x2: column key string length
///
/// Output:
/// - x0: new indexed array containing boxed Mixed values for each row that has the key
///
/// Behavior:
/// - Iterates every row in the outer indexed array.
/// - For each row, performs a hash lookup for the requested key.
/// - Skips rows that do not contain the key (cbz/jz check).
/// - Boxes the found hash payload into an owned Mixed cell via `__rt_mixed_from_value`.
/// - Appends the boxed Mixed pointer to the result array via `__rt_array_push_int`.
/// - Returns the result array stamped with value_type 7 (boxed Mixed).
pub fn emit_array_column_mixed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_column_mixed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_column_mixed ---");
    emitter.label_global("__rt_array_column_mixed");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame for inputs, result array, and loop cursor
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set a stable frame pointer for mixed column extraction

    // -- save inputs --
    emitter.instruction("str x0, [sp, #0]");                                    // save outer indexed-array pointer
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save requested column key pointer and length

    // -- load outer array length --
    emitter.instruction("ldr x9, [x0]");                                        // load outer indexed-array logical length
    emitter.instruction("str x9, [sp, #24]");                                   // save outer length for the extraction loop

    // -- create result array with boxed Mixed slots --
    emitter.instruction("mov x0, x9");                                          // result capacity matches the outer row count
    emitter.instruction("mov x1, #8");                                          // boxed Mixed slots store one pointer each
    emitter.instruction("bl __rt_array_new");                                   // allocate the result indexed array
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the result array metadata word
    emitter.instruction("mov x10, #0x80ff");                                    // preserve indexed-array kind and persistent COW metadata
    emitter.instruction("and x9, x9, x10");                                     // clear the stale value-type byte
    emitter.instruction("mov x10, #7");                                         // runtime value_type 7 = boxed Mixed
    emitter.instruction("lsl x10, x10, #8");                                    // move the Mixed tag into the packed metadata byte
    emitter.instruction("orr x9, x9, x10");                                     // combine stable metadata with the Mixed value type
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the result array as Mixed-valued
    emitter.instruction("str x0, [sp, #32]");                                   // save result array pointer across hash lookups

    // -- iterate outer array --
    emitter.instruction("str xzr, [sp, #40]");                                  // initialize outer loop index to zero

    emitter.label("__rt_acm_loop");
    emitter.instruction("ldr x9, [sp, #40]");                                   // load current outer loop index
    emitter.instruction("ldr x10, [sp, #24]");                                  // load saved outer row count
    emitter.instruction("cmp x9, x10");                                         // compare current index with outer row count
    emitter.instruction("b.ge __rt_acm_done");                                  // finish once every row has been examined

    // -- load inner associative row and look up the requested key --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload outer indexed-array pointer
    emitter.instruction("add x0, x0, #24");                                     // advance to the outer array payload base
    emitter.instruction("ldr x0, [x0, x9, lsl #3]");                            // load current inner associative-array hash pointer
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // reload requested column key pointer and length
    emitter.instruction("bl __rt_hash_get");                                    // look up row value, returning found plus tag and payload words
    emitter.instruction("cbz x0, __rt_acm_next");                               // skip rows that do not contain the requested column key

    // -- box found payload and append it as an owned Mixed slot --
    emitter.instruction("mov x0, x3");                                          // pass the runtime value tag to the Mixed boxing helper
    emitter.instruction("bl __rt_mixed_from_value");                            // box the borrowed hash payload into an owned Mixed cell
    emitter.instruction("mov x1, x0");                                          // use the boxed Mixed pointer as the appended slot payload
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload result array pointer for the append helper
    emitter.instruction("bl __rt_array_push_int");                              // append the owned Mixed box without taking an extra retain
    emitter.instruction("str x0, [sp, #32]");                                   // save the possibly grown result array pointer

    emitter.label("__rt_acm_next");
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload current outer loop index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next outer row
    emitter.instruction("str x9, [sp, #40]");                                   // store updated outer loop index
    emitter.instruction("b __rt_acm_loop");                                     // continue scanning rows for the requested column

    emitter.label("__rt_acm_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // return the Mixed-valued result array

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the extraction stack frame
    emitter.instruction("ret");                                                 // return to generated code
}

/// x86_64 Linux variant of `emit_array_column_mixed`.
///
/// Uses the System V AMD64 ABI:
/// - rdi: outer indexed array (Array of AssocArray)
/// - rsi: column key string pointer
/// - rdx: column key string length
///
/// Output:
/// - rax: new indexed array containing boxed Mixed values for each row that has the key
///
/// Behavior mirrors the ARM64 variant but uses AMD64 register conventions and
/// metadata manipulation (0x700 mask for value_type 7 = boxed Mixed).
fn emit_array_column_mixed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_column_mixed ---");
    emitter.label_global("__rt_array_column_mixed");

    emitter.instruction("push rbp");                                            // preserve caller frame pointer before mixed column extraction
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for saved inputs and loop state
    emitter.instruction("sub rsp, 48");                                         // reserve spill slots for outer array, key string, length, result, and index
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save outer indexed-array pointer across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save requested column key pointer across the row loop
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save requested column key length across the row loop
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load outer indexed-array logical length
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save outer row count for loop termination
    emitter.instruction("mov rdi, r10");                                        // pass result capacity equal to the outer row count
    emitter.instruction("mov rsi, 8");                                          // boxed Mixed slots store one pointer each
    emitter.instruction("call __rt_array_new");                                 // allocate the result indexed array
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load result array metadata word
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // materialize the x86_64 result-array metadata preservation mask
    emitter.instruction("and r10, r11");                                        // preserve heap marker, indexed-array kind, and persistent COW metadata
    emitter.instruction("or r10, 0x700");                                       // stamp runtime value_type 7 = boxed Mixed
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // persist the Mixed-valued result array metadata
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save result array pointer across hash lookups
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize outer loop index to zero

    emitter.label("__rt_acm_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // load current outer loop index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // compare index against saved outer row count
    emitter.instruction("jae __rt_acm_done");                                   // finish once all rows have been examined
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload outer indexed-array pointer
    emitter.instruction("mov rdi, QWORD PTR [r11 + r10 * 8 + 24]");             // load current inner associative-array with tag and payload
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload requested column key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload requested column key length
    emitter.instruction("call __rt_hash_get");                                  // look up row value, returning found plus tag and payload words
    emitter.instruction("test rax, rax");                                       // check whether the requested key exists in this row
    emitter.instruction("jz __rt_acm_next");                                    // skip rows that do not contain the requested column key
    emitter.instruction("mov rax, rcx");                                        // pass the runtime value tag to the Mixed boxing helper
    emitter.instruction("call __rt_mixed_from_value");                          // box the borrowed hash payload into an owned Mixed cell
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload result array pointer for the append helper
    emitter.instruction("mov rsi, rax");                                        // pass the owned Mixed box as the appended slot payload
    emitter.instruction("call __rt_array_push_int");                            // append the owned Mixed box without taking an extra retain
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the possibly grown result array pointer

    emitter.label("__rt_acm_next");
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // advance to the next outer row
    emitter.instruction("jmp __rt_acm_loop");                                   // continue scanning rows for the requested column

    emitter.label("__rt_acm_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return the Mixed-valued result array
    emitter.instruction("add rsp, 48");                                         // release mixed column extraction spill slots
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to generated code
}
