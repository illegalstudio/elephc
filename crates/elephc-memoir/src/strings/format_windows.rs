//! Purpose:
//! Reserves and publishes the string windows used by numeric formatters.
//!
//! Called from:
//! - Integer string formatting and fixed runtime data emission.
//!
//! Key details:
//! - Overflow uses a finite spill ring so diagnostics can format without allocating.

use crate::{abi, Emitter};

/// Number of 32-byte integer spill slots. The power of two permits cursor masking.
const ITOA_SPILL_SLOTS: usize = 8;

/// Reserves a scratch or spill window for reverse decimal digits.
pub fn emit_itoa_window_aarch64(emitter: &mut impl Emitter) {
    // -- get concat_buf write position --
    abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset into concat_buf
    emitter.instruction("add x12, x8, #21");                                    // offset after reserving the 21-byte scratch window
    abi::emit_load_int_immediate(emitter, "x13", emitter.concat_capacity() as i64); // concat scratch capacity in bytes
    emitter.instruction("cmp x12, x13");                                        // would the window cross the end of _concat_buf?
    emitter.instruction("b.hi __rt_itoa_spill");                                // a full scratch formats into the spill ring instead
    abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // compute write position: buf + offset
    emitter.instruction("b __rt_itoa_window_ready");                            // the scratch window is reserved
    emitter.label("__rt_itoa_spill");
    abi::emit_symbol_address(emitter, "x7", "_itoa_spill_index");
    emitter.instruction("ldr x12, [x7]");                                       // load the next spill slot index
    emitter.instruction("add x13, x12, #1");                                    // advance the ring cursor
    emitter.instruction(&format!("and x13, x13, #{}", ITOA_SPILL_SLOTS - 1));   // wrap the cursor inside the ring
    emitter.instruction("str x13, [x7]");                                       // publish the cursor for the next spill
    abi::emit_symbol_address(emitter, "x7", "_itoa_spill_ring");
    emitter.instruction("add x9, x7, x12, lsl #5");                             // each spill slot is 32 bytes
    emitter.instruction("sub x8, x8, #21");                                     // cancel the scratch advance below: the live scratch keeps its offset
    emitter.label("__rt_itoa_window_ready");
    emitter.instruction("add x9, x9, #20");                                     // advance to end of 21-byte scratch area (digits written right-to-left)
}

/// Publishes the scratch cursor after formatting.
pub fn emit_itoa_finish_aarch64(emitter: &mut impl Emitter) {
    emitter.instruction("add x8, x8, #21");                                     // advance concat_off by scratch area size
    emitter.instruction("str x8, [x6]");                                        // store updated offset back to _concat_off
}

/// Reserves a scratch or spill window for reverse decimal digits.
pub fn emit_itoa_window_x86_64(emitter: &mut impl Emitter) {
    // -- get concat_buf write position --
    abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat buffer offset
    emitter.instruction("lea rsi, [r9 + 21]");                                  // offset after reserving the 21-byte scratch window
    emitter.instruction(&format!("cmp rsi, {}", emitter.concat_capacity()));    // would the window cross the end of _concat_buf?
    emitter.instruction("ja __rt_itoa_spill");                                  // a full scratch formats into the spill ring instead
    abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("add r10, r9");                                         // compute the current concat buffer write position
    emitter.instruction("jmp __rt_itoa_window_ready");                          // the scratch window is reserved
    emitter.label("__rt_itoa_spill");
    abi::emit_symbol_address(emitter, "r10", "_itoa_spill_index");
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // load the next spill slot index
    emitter.instruction("lea rdx, [rsi + 1]");                                  // advance the ring cursor
    emitter.instruction(&format!("and rdx, {}", ITOA_SPILL_SLOTS - 1));         // wrap the cursor inside the ring
    emitter.instruction("mov QWORD PTR [r10], rdx");                            // publish the cursor for the next spill
    abi::emit_symbol_address(emitter, "r10", "_itoa_spill_ring");
    emitter.instruction("shl rsi, 5");                                          // each spill slot is 32 bytes
    emitter.instruction("add r10, rsi");                                        // address the claimed spill slot
    emitter.instruction("sub r9, 21");                                          // cancel the scratch advance below: the live scratch keeps its offset
    emitter.label("__rt_itoa_window_ready");
    emitter.instruction("add r10, 20");                                         // advance to the end of the 21-byte scratch area for right-to-left digit writes
}

/// Publishes the scratch cursor after formatting.
pub fn emit_itoa_finish_x86_64(emitter: &mut impl Emitter) {
    emitter.instruction("add r9, 21");                                          // advance concat_off by the fixed scratch area size
    emitter.instruction("mov QWORD PTR [r8], r9");                              // store the updated concat buffer offset back to global storage
}

/// Reserves the integer and float spill rings, keeping each cursor with its storage.
pub fn append_spill_rings(out: &mut String, target: &impl crate::data::DataTarget) {
    use super::concat_scratch::{FLOAT_SPILL_SLOTS, FLOAT_WINDOW_BYTES};
    out.push_str(&target.common("_itoa_spill_ring", 32 * ITOA_SPILL_SLOTS));
    out.push_str(&target.common("_itoa_spill_index", 8));
    out.push_str(&target.common("_float_spill_ring", FLOAT_WINDOW_BYTES * FLOAT_SPILL_SLOTS));
    out.push_str(&target.common("_float_spill_index", 8));
}
