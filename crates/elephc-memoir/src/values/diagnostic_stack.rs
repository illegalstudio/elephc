//! Purpose:
//! Tracks borrowed object addresses during diagnostic traversal.
//!
//! Called from:
//! - The runtime emitter for var_dump and print_r.
//!
//! Key details:
//! - A full stack reports a hit so callers stop descending.

use crate::{Arch, Emitter, abi, data::{DataTarget, StaticStorage}};

const VD_SEEN_CAPACITY: usize = 256;
const SEEN: StaticStorage = StaticStorage { symbol: "_vd_seen", bytes: VD_SEEN_CAPACITY * 8 };
/// Symbol holding the active diagnostic traversal depth.
pub const DEPTH: &str = "_vd_seen_n";

/// Reserves the borrowed pointer stack and its depth.
pub fn append_data(out: &mut String, target: &impl DataTarget) {
    SEEN.append(out, target);
    out.push_str(&target.common(DEPTH, 8));
}

/// `__rt_vd_seen_find`: report whether an object pointer is already being walked.
///
/// Input: AArch64 x0 / x86_64 rdi = object pointer.
/// Output: AArch64 x0 / x86_64 rax = 1 when the pointer is on the visited stack
/// OR the stack is full, 0 otherwise. Treating a full stack as a hit is what
/// bounds the walk: the caller renders `*RECURSION*` and stops descending.
pub fn emit_vd_seen_find(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_vd_seen_find_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: vd_seen_find ---");
    emitter.label_global("__rt_vd_seen_find");

    abi::emit_symbol_address(emitter, "x9", DEPTH);                      // resolve the visited-stack depth counter
    emitter.instruction("ldr x10, [x9]");                                       // load the current visited-stack depth
    emitter.instruction(&format!("cmp x10, #{}", VD_SEEN_CAPACITY));            // is the visited stack already full?
    emitter.instruction("b.ge __rt_vd_seen_find_hit");                          // a full stack reports a hit so the walk terminates
    abi::emit_symbol_address(emitter, "x11", SEEN.symbol);                       // resolve the visited-pointer stack base
    emitter.instruction("mov x12, #0");                                         // start the scan at the bottom of the stack

    emitter.label("__rt_vd_seen_find_loop");
    emitter.instruction("cmp x12, x10");                                        // scanned every live entry?
    emitter.instruction("b.ge __rt_vd_seen_find_miss");                         // the pointer is not currently being walked
    emitter.instruction("ldr x13, [x11, x12, lsl #3]");                         // load the visited pointer at this depth
    emitter.instruction("cmp x13, x0");                                         // does it match the candidate object?
    emitter.instruction("b.eq __rt_vd_seen_find_hit");                          // already on the walk stack → recursion
    emitter.instruction("add x12, x12, #1");                                    // advance to the next visited entry
    emitter.instruction("b __rt_vd_seen_find_loop");                            // continue scanning

    emitter.label("__rt_vd_seen_find_hit");
    emitter.instruction("mov x0, #1");                                          // report recursion (or exhausted guard capacity)
    emitter.instruction("ret");                                                 // return to caller

    emitter.label("__rt_vd_seen_find_miss");
    emitter.instruction("mov x0, #0");                                          // the object is safe to descend into
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 visited-object lookup helper.
fn emit_vd_seen_find_linux_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: vd_seen_find ---");
    emitter.label_global("__rt_vd_seen_find");

    abi::emit_symbol_address(emitter, "r9", DEPTH);                      // resolve the visited-stack depth counter
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current visited-stack depth
    emitter.instruction(&format!("cmp r10, {}", VD_SEEN_CAPACITY));             // is the visited stack already full?
    emitter.instruction("jge __rt_vd_seen_find_hit_x86");                       // a full stack reports a hit so the walk terminates
    abi::emit_symbol_address(emitter, "r11", SEEN.symbol);                       // resolve the visited-pointer stack base
    emitter.instruction("xor rcx, rcx");                                        // start the scan at the bottom of the stack

    emitter.label("__rt_vd_seen_find_loop_x86");
    emitter.instruction("cmp rcx, r10");                                        // scanned every live entry?
    emitter.instruction("jge __rt_vd_seen_find_miss_x86");                      // the pointer is not currently being walked
    emitter.instruction("mov rax, QWORD PTR [r11 + rcx * 8]");                  // load the visited pointer at this depth
    emitter.instruction("cmp rax, rdi");                                        // does it match the candidate object?
    emitter.instruction("je __rt_vd_seen_find_hit_x86");                        // already on the walk stack → recursion
    emitter.instruction("add rcx, 1");                                          // advance to the next visited entry
    emitter.instruction("jmp __rt_vd_seen_find_loop_x86");                      // continue scanning

    emitter.label("__rt_vd_seen_find_hit_x86");
    emitter.instruction("mov rax, 1");                                          // report recursion (or exhausted guard capacity)
    emitter.instruction("ret");                                                 // return to caller

    emitter.label("__rt_vd_seen_find_miss_x86");
    emitter.instruction("xor rax, rax");                                        // the object is safe to descend into
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_vd_seen_push`: record an object pointer as being walked.
///
/// Silently drops the entry when the stack is full, `__rt_vd_seen_find` already
/// refuses to descend at that depth, so the pop below can never underflow.
/// Input: AArch64 x0 / x86_64 rdi = object pointer.
pub fn emit_vd_seen_push(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: vd_seen_push ---");
        emitter.label_global("__rt_vd_seen_push");
        abi::emit_symbol_address(emitter, "r9", DEPTH);                  // resolve the visited-stack depth counter
        emitter.instruction("mov r10, QWORD PTR [r9]");                         // load the current visited-stack depth
        emitter.instruction(&format!("cmp r10, {}", VD_SEEN_CAPACITY));         // is there room for another entry?
        emitter.instruction("jge __rt_vd_seen_push_done_x86");                  // a full stack is already refusing to descend
        abi::emit_symbol_address(emitter, "r11", SEEN.symbol);                   // resolve the visited-pointer stack base
        emitter.instruction("mov QWORD PTR [r11 + r10 * 8], rdi");              // store the object pointer at the top of the stack
        emitter.instruction("add r10, 1");                                      // grow the visited-stack depth
        emitter.instruction("mov QWORD PTR [r9], r10");                         // publish the new depth
        emitter.label("__rt_vd_seen_push_done_x86");
        emitter.instruction("ret");                                             // return to caller
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: vd_seen_push ---");
    emitter.label_global("__rt_vd_seen_push");
    abi::emit_symbol_address(emitter, "x9", DEPTH);                      // resolve the visited-stack depth counter
    emitter.instruction("ldr x10, [x9]");                                       // load the current visited-stack depth
    emitter.instruction(&format!("cmp x10, #{}", VD_SEEN_CAPACITY));            // is there room for another entry?
    emitter.instruction("b.ge __rt_vd_seen_push_done");                         // a full stack is already refusing to descend
    abi::emit_symbol_address(emitter, "x11", SEEN.symbol);                       // resolve the visited-pointer stack base
    emitter.instruction("str x0, [x11, x10, lsl #3]");                          // store the object pointer at the top of the stack
    emitter.instruction("add x10, x10, #1");                                    // grow the visited-stack depth
    emitter.instruction("str x10, [x9]");                                       // publish the new depth
    emitter.label("__rt_vd_seen_push_done");
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_vd_seen_pop`: drop the innermost visited-object entry. Takes no arguments.
///
/// Clamps at zero so an unbalanced pop can never make the depth negative and turn
/// the next `__rt_vd_seen_find` scan into an out-of-bounds read.
pub fn emit_vd_seen_pop(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: vd_seen_pop ---");
        emitter.label_global("__rt_vd_seen_pop");
        abi::emit_symbol_address(emitter, "r9", DEPTH);                  // resolve the visited-stack depth counter
        emitter.instruction("mov r10, QWORD PTR [r9]");                         // load the current visited-stack depth
        emitter.instruction("cmp r10, 0");                                      // is the stack already empty?
        emitter.instruction("jle __rt_vd_seen_pop_done_x86");                   // clamp at zero rather than underflow
        emitter.instruction("sub r10, 1");                                      // leave the innermost object
        emitter.instruction("mov QWORD PTR [r9], r10");                         // publish the new depth
        emitter.label("__rt_vd_seen_pop_done_x86");
        emitter.instruction("ret");                                             // return to caller
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: vd_seen_pop ---");
    emitter.label_global("__rt_vd_seen_pop");
    abi::emit_symbol_address(emitter, "x9", DEPTH);                      // resolve the visited-stack depth counter
    emitter.instruction("ldr x10, [x9]");                                       // load the current visited-stack depth
    emitter.instruction("cmp x10, #0");                                         // is the stack already empty?
    emitter.instruction("b.le __rt_vd_seen_pop_done");                          // clamp at zero rather than underflow
    emitter.instruction("sub x10, x10, #1");                                    // leave the innermost object
    emitter.instruction("str x10, [x9]");                                       // publish the new depth
    emitter.label("__rt_vd_seen_pop_done");
    emitter.instruction("ret");                                                 // return to caller
}
