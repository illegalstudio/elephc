//! Purpose:
//! Emits `__rt_mixed_reuse_int_cell`, which boxes an integer for a Mixed local by rewriting
//! the cell that local already owns instead of allocating a new one.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//! - Generated code, through `crate::codegen::lower_inst::iterators::key_reuse`, for the
//!   integer key of a by-value `foreach` whose key variable is a plain Mixed local.
//!
//! Key details:
//! - The cell is reused only when the slot holds a managed heap cell with a reference count of
//!   exactly one and a scalar tag (int, float, bool or null). A count of one means the slot is
//!   the sole owner, so no other value can observe the rewrite; a scalar tag means the cell
//!   owns no child that would need releasing first.
//! - On reuse the slot is cleared and the cell is returned, so ownership moves to the caller
//!   exactly as if the cell had been freshly allocated. The store that follows publishes it
//!   back to the slot and finds nothing left to release.
//! - Every other case (an empty slot, a frame-local or static cell outside the heap, a shared
//!   cell, a string, array or object payload) leaves the slot untouched and tail-calls
//!   `__rt_mixed_from_value`, which is what the caller did before.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_mixed_reuse_int_cell` for the active target.
///
/// Input: AArch64 `x0` = address of the Mixed local slot, `x1` = integer payload; x86_64
/// `rdi` = slot address, `rsi` = integer payload. Output: `x0` / `rax` = an owned Mixed cell
/// holding that integer.
pub fn emit_mixed_reuse_int_cell(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_reuse_int_cell_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_reuse_int_cell ---");
    emitter.label_global("__rt_mixed_reuse_int_cell");

    emitter.instruction("ldr x9, [x0]");                                        // load the cell the Mixed local currently owns
    emitter.instruction("cbz x9, __rt_mric_fresh");                             // an empty slot has no cell to reuse
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_heap_buf");
    emitter.instruction("cmp x9, x10");                                         // is the cell below the managed heap?
    emitter.instruction("b.lo __rt_mric_fresh");                                // frame-local and static cells are never rewritten
    crate::codegen_support::abi::emit_symbol_address(emitter, "x11", "_heap_off");
    emitter.instruction("ldr x11, [x11]");                                      // load the current heap extent
    emitter.instruction("add x11, x10, x11");                                   // compute the managed heap end
    emitter.instruction("cmp x9, x11");                                         // is the cell at or beyond the heap end?
    emitter.instruction("b.hs __rt_mric_fresh");                                // foreign pointers are never rewritten
    emitter.instruction("ldr w10, [x9, #-12]");                                 // load the cell's reference count from the uniform heap header
    emitter.instruction("cmp w10, #1");                                         // is the local the cell's only owner?
    emitter.instruction("b.ne __rt_mric_fresh");                                // a shared cell must keep its value for the other owners
    emitter.instruction("ldr x10, [x9]");                                       // load the runtime tag of the value being replaced
    emitter.instruction("cbz x10, __rt_mric_reuse");                            // an integer cell owns no child
    emitter.instruction("cmp x10, #2");                                         // does the cell hold a float?
    emitter.instruction("b.eq __rt_mric_reuse");                                // a float cell owns no child
    emitter.instruction("cmp x10, #3");                                         // does the cell hold a bool?
    emitter.instruction("b.eq __rt_mric_reuse");                                // a bool cell owns no child
    emitter.instruction("cmp x10, #8");                                         // does the cell hold null?
    emitter.instruction("b.ne __rt_mric_fresh");                                // strings, containers and objects need their ordinary release

    emitter.label("__rt_mric_reuse");
    emitter.instruction("str xzr, [x9]");                                       // tag the reused cell as an integer
    emitter.instruction("str x1, [x9, #8]");                                    // store the integer payload
    emitter.instruction("str xzr, [x9, #16]");                                  // integers do not use the high payload word
    emitter.instruction("str xzr, [x0]");                                       // the slot hands its cell to the caller
    emitter.instruction("mov x0, x9");                                          // return the reused cell as an owned value
    emitter.instruction("ret");                                                 // no allocation was needed

    emitter.label("__rt_mric_fresh");
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("mov x2, #0");                                          // integers do not use the high payload word
    emitter.instruction("b __rt_mixed_from_value");                             // box the integer into a fresh owned cell
}

/// Emits the Linux x86_64 implementation of `__rt_mixed_reuse_int_cell`.
fn emit_mixed_reuse_int_cell_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_reuse_int_cell ---");
    emitter.label_global("__rt_mixed_reuse_int_cell");

    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the cell the Mixed local currently owns
    emitter.instruction("test rax, rax");                                       // does the slot hold a cell at all?
    emitter.instruction("jz __rt_mric_fresh_x86");                              // an empty slot has no cell to reuse
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("lea r11, [r10 + 16]");                                 // first valid user payload begins after the initial heap header
    emitter.instruction("cmp rax, r11");                                        // is the cell below the managed heap?
    emitter.instruction("jb __rt_mric_fresh_x86");                              // frame-local and static cells are never rewritten
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_heap_off");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the current heap extent
    emitter.instruction("add r11, r10");                                        // compute the managed heap end
    emitter.instruction("cmp rax, r11");                                        // is the cell at or beyond the heap end?
    emitter.instruction("jae __rt_mric_fresh_x86");                             // foreign pointers are never rewritten
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the stamped heap kind word from the uniform header
    emitter.instruction("shr r10, 32");                                         // isolate the heap wrapper marker
    emitter.instruction(&format!("cmp r10d, 0x{:x}", crate::codegen_support::sentinels::X86_64_HEAP_MAGIC_HI32)); // does the cell carry an elephc heap header?
    emitter.instruction("jne __rt_mric_fresh_x86");                             // foreign pointers are never rewritten
    emitter.instruction("cmp DWORD PTR [rax - 12], 1");                         // is the local the cell's only owner?
    emitter.instruction("jne __rt_mric_fresh_x86");                             // a shared cell must keep its value for the other owners
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the runtime tag of the value being replaced
    emitter.instruction("test r10, r10");                                       // does the cell hold an integer?
    emitter.instruction("jz __rt_mric_reuse_x86");                              // an integer cell owns no child
    emitter.instruction("cmp r10, 2");                                          // does the cell hold a float?
    emitter.instruction("je __rt_mric_reuse_x86");                              // a float cell owns no child
    emitter.instruction("cmp r10, 3");                                          // does the cell hold a bool?
    emitter.instruction("je __rt_mric_reuse_x86");                              // a bool cell owns no child
    emitter.instruction("cmp r10, 8");                                          // does the cell hold null?
    emitter.instruction("jne __rt_mric_fresh_x86");                             // strings, containers and objects need their ordinary release

    emitter.label("__rt_mric_reuse_x86");
    emitter.instruction("mov QWORD PTR [rax], 0");                              // tag the reused cell as an integer
    emitter.instruction("mov QWORD PTR [rax + 8], rsi");                        // store the integer payload
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // integers do not use the high payload word
    emitter.instruction("mov QWORD PTR [rdi], 0");                              // the slot hands its cell to the caller
    emitter.instruction("ret");                                                 // return the reused cell as an owned value

    emitter.label("__rt_mric_fresh_x86");
    emitter.instruction("mov rdi, rsi");                                        // pass the integer payload as the low word
    emitter.instruction("xor esi, esi");                                        // integers do not use the high payload word
    emitter.instruction("mov eax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the integer into a fresh owned cell
}
