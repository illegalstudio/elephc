//! Purpose:
//! Keeps the live concatenation window available while joining strings and formatted values.
//!
//! Called from:
//! - Implode lowering and runtime emission on both native architectures.
//!
//! Key details:
//! - Growth transfers the initialized prefix and relocates the cursor.
//! - Cursor publication isolates the active prefix from nested casts and warning handlers.

use crate::{Emitter, abi};

/// Emits the AArch64 "make room for `x15` more destination bytes" sequence.
///
/// Compares the bytes already written plus the pending write against the destination's
/// capacity and, when they no longer fit, moves the whole accumulation into a larger owned
/// block through `__rt_concat_grow`. The doubling keeps a long join from reallocating per
/// element; taking the exact requirement instead when it is larger keeps one oversized element
/// from needing a second growth of its own.
///
/// Reads `x15` (pending byte count) and `x9` (live cursor). Preserves `x1`, `x2`, `x9`, `x10`
/// and `x11` across the growth call, and relocates `x9` into the grown buffer. Every other
/// caller-saved register is clobbered, so callers reload what they still need, `x3` in
/// particular.
pub fn emit_implode_ensure_room_aarch64(emitter: &mut impl Emitter, site: &str) {
    let skip = format!("__rt_implode_room_ok_{}", site);
    emitter.instruction("ldr x13, [sp, #24]");                                  // destination base
    emitter.instruction("sub x14, x9, x13");                                    // bytes already written into it
    emitter.instruction("add x15, x14, x15");                                   // capacity this write would need
    emitter.instruction("ldr x12, [sp, #80]");                                  // current destination capacity
    emitter.instruction("cmp x15, x12");                                        // does the pending write still fit?
    emitter.instruction(&format!("b.ls {}", skip));                             // room already reserved: copy straight in
    emitter.instruction("lsl x12, x12, #1");                                    // double the destination capacity
    emitter.instruction("cmp x12, x15");                                        // is the doubled capacity already enough?
    emitter.instruction("csel x12, x12, x15, hi");                              // otherwise take exactly what this write needs
    emitter.instruction("str x12, [sp, #80]");                                  // save the grown capacity
    emitter.instruction("str x14, [sp, #56]");                                  // bytes to preserve, and the cursor offset to restore
    emitter.instruction("str x10, [sp, #64]");                                  // preserve array length across the growth call
    emitter.instruction("str x11, [sp, #72]");                                  // preserve loop index across the growth call
    emitter.instruction("str x1, [sp, #96]");                                   // preserve the pending source pointer
    emitter.instruction("str x2, [sp, #104]");                                  // preserve the pending source length
    emitter.instruction("mov x0, x13");                                         // current buffer
    emitter.instruction("mov x1, x14");                                         // bytes to preserve
    emitter.instruction("mov x2, x12");                                         // new capacity
    emitter.instruction("bl __rt_concat_grow");                                 // move the join into a larger owned block
    emitter.instruction("str x0, [sp, #24]");                                   // record the grown destination base
    emitter.instruction("ldr x14, [sp, #56]");                                  // bytes preserved
    emitter.instruction("add x9, x0, x14");                                     // cursor at the same offset inside the grown buffer
    emitter.instruction("ldr x10, [sp, #64]");                                  // restore array length
    emitter.instruction("ldr x11, [sp, #72]");                                  // restore loop index
    emitter.instruction("ldr x1, [sp, #96]");                                   // restore the pending source pointer
    emitter.instruction("ldr x2, [sp, #104]");                                  // restore the pending source length
    emitter.label(&skip);
}

/// Emits the AArch64 sequence that publishes `_concat_off` for whichever storage the join
/// currently occupies.
///
/// A SCRATCH destination reserves everything written so far against the shared buffer, which
/// is what stops a nested `__rt_ftoa` from formatting over bytes already joined. A GROWN
/// destination occupies no scratch at all, so it restores the offset the call started from and
/// leaves the whole buffer to the cast.
///
/// `x9` must hold the live cursor. Clobbers `x12`-`x15`.
pub fn emit_implode_publish_offset_aarch64(emitter: &mut impl Emitter, site: &str) {
    let cap = emitter.concat_capacity();
    let grown = format!("__rt_implode_off_grown_{}", site);
    let store = format!("__rt_implode_off_store_{}", site);
    emitter.instruction("ldr x13, [sp, #24]");                                  // destination base
    abi::emit_symbol_address(emitter, "x14", "_concat_buf");
    emitter.instruction("sub x15, x13, x14");                                   // candidate scratch offset of the destination
    abi::emit_load_int_immediate(emitter, "x12", cap as i64); // concat scratch capacity in bytes
    emitter.instruction("cmp x15, x12");                                        // is the destination outside the scratch window (unsigned: heap wraps high)?
    emitter.instruction(&format!("b.hs {}", grown));                            // a grown join holds no scratch
    emitter.instruction("sub x14, x9, x14");                                    // absolute offset of the live destination cursor
    emitter.instruction(&format!("b {}", store));                               // publish the reservation the nested cast must not cross
    emitter.label(&grown);
    emitter.instruction("ldr x14, [sp, #88]");                                  // restore the offset this call started from
    emitter.label(&store);
    emitter.instruction("ldr x13, [sp, #32]");                                  // concat offset variable address
    emitter.instruction("str x14, [x13]");                                      // publish it
}

/// Emits the x86_64 "make room for `r9` more destination bytes" sequence.
///
/// The x86_64 mirror of [`emit_implode_ensure_room_aarch64`], with the same contract: reads the
/// pending byte count from `r9` and the live cursor from `r10`, preserves `r8`/`r9`/`r10`
/// across the growth call, and relocates `r10` into the grown buffer. The array length and loop
/// cursor already live in spill slots on this target, so they need no saving.
pub fn emit_implode_ensure_room_x86_64(emitter: &mut impl Emitter, site: &str) {
    let skip = format!("__rt_implode_room_ok_{}", site);
    let have_cap = format!("__rt_implode_room_cap_{}", site);
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // destination base
    emitter.instruction("mov rdx, r10");                                        // live cursor
    emitter.instruction("sub rdx, rcx");                                        // bytes already written into it
    emitter.instruction("mov r11, rdx");                                        // start from the bytes already written
    emitter.instruction("add r11, r9");                                         // capacity this write would need
    emitter.instruction("cmp r11, QWORD PTR [rbp - 80]");                       // does the pending write still fit?
    emitter.instruction(&format!("jbe {}", skip));                              // room already reserved: copy straight in
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // current destination capacity
    emitter.instruction("shl rax, 1");                                          // double it
    emitter.instruction("cmp rax, r11");                                        // is the doubled capacity already enough?
    emitter.instruction(&format!("ja {}", have_cap));                           // keep the doubled capacity
    emitter.instruction("mov rax, r11");                                        // otherwise take exactly what this write needs
    emitter.label(&have_cap);
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save the grown capacity
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // stash the preserved byte count in the cursor slot
    emitter.instruction("mov QWORD PTR [rbp - 96], r8");                        // preserve the pending source pointer
    emitter.instruction("mov QWORD PTR [rbp - 104], r9");                       // preserve the pending source length
    emitter.instruction("mov rsi, rax");                                        // new capacity
    emitter.instruction("mov rdi, rdx");                                        // bytes to preserve
    emitter.instruction("mov rax, rcx");                                        // current buffer
    emitter.instruction("call __rt_concat_grow");                               // move the join into a larger owned block
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // record the grown destination base
    emitter.instruction("mov r10, rax");                                        // rebuild the cursor from the grown base
    emitter.instruction("add r10, QWORD PTR [rbp - 40]");                       // ... at the same offset the preserved bytes reach
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // restore the cursor slot
    emitter.instruction("mov r8, QWORD PTR [rbp - 96]");                        // restore the pending source pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 104]");                       // restore the pending source length
    emitter.label(&skip);
}

/// Emits the x86_64 sequence that publishes `_concat_off` for whichever storage the join
/// currently occupies. The x86_64 mirror of [`emit_implode_publish_offset_aarch64`].
///
/// `r10` must hold the live cursor. Clobbers `rcx`, `rdx`, `r8` and `r9`.
pub fn emit_implode_publish_offset_x86_64(emitter: &mut impl Emitter, site: &str) {
    let cap = emitter.concat_capacity();
    let grown = format!("__rt_implode_off_grown_{}", site);
    let store = format!("__rt_implode_off_store_{}", site);
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // destination base
    abi::emit_symbol_address(emitter, "r8", "_concat_buf");
    emitter.instruction("mov rdx, rcx");                                        // copy the base before converting it to a scratch offset
    emitter.instruction("sub rdx, r8");                                         // candidate scratch offset of the destination
    abi::emit_load_int_immediate(emitter, "r9", cap as i64); // load the shared scratch capacity
    emitter.instruction("cmp rdx, r9");                                         // is the destination outside the scratch window (unsigned: heap wraps high)?
    emitter.instruction(&format!("jae {}", grown));                             // a grown join holds no scratch
    emitter.instruction("mov r9, r10");                                         // copy the live cursor before converting it to an absolute offset
    emitter.instruction("sub r9, r8");                                          // absolute offset of the live destination cursor
    emitter.instruction(&format!("jmp {}", store));                             // publish the reservation the nested cast must not cross
    emitter.label(&grown);
    emitter.instruction("mov r9, QWORD PTR [rbp - 88]");                        // restore the offset this call started from
    emitter.label(&store);
    abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov QWORD PTR [r8], r9");                              // publish it
}
