//! Purpose:
//! Owns stack-frame setup, teardown, frame-slot addressing, and generic memory loads/stores.
//! Provides target-specific helpers for local slots, caller stack access, and cleanup callbacks.
//!
//! Called from:
//! - `crate::codegen`, ABI call helpers, and shared wrapper emitters.
//!
//! Key details:
//! - Frame offsets and stack alignment are shared contracts with local collection and call materialization.

use crate::codegen_support::{emit::Emitter, platform::Arch};
#[cfg(test)]
use crate::types::PhpType;

#[cfg(test)]
use super::registers::{float_result_reg, int_result_reg, string_result_regs};
use super::registers::{frame_pointer_reg, is_float_register};
use super::values::emit_load_int_immediate;

/// Sets up the stack frame for a function body.
/// On AArch64: allocates `frame_size` bytes, saves x29/x30 in the footer, and establishes x29 as the frame pointer.
/// On x86_64: pushes rbp, establishes rsp as the frame base, and reserves `frame_size - 16` bytes for locals.
pub fn emit_frame_prologue(emitter: &mut Emitter, frame_size: usize) {
    assert!(
        frame_size >= 16,
        "frame_size must reserve the 16-byte frame footer (x29/x30), got {frame_size}"
    );
    emitter.comment("prologue");
    // A new frame has no frame-base register until its owner establishes one, even when it is
    // emitted in the middle of another function whose body keeps x28 as its base.
    emitter.frame_slot_base = None;
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_adjust_sp(emitter, frame_size, true);
            let footer_offset = frame_size - 16;
            if footer_offset <= 504 {
                emitter.instruction(&format!("stp x29, x30, [sp, #{}]", footer_offset)); // save frame pointer and return address in the fixed frame footer
            } else {
                emit_sp_address(emitter, "x9", footer_offset);
                emitter.instruction("stp x29, x30, [x9]");                      // save frame pointer and return address through the computed footer pointer
            }
            if footer_offset == 0 {
                emitter.instruction("mov x29, sp");                             // use the current stack pointer directly when the frame footer starts at sp
            } else if footer_offset <= 4095 {
                emitter.instruction(&format!("add x29, sp, #{}", footer_offset)); // point the frame pointer at the nearby fixed frame footer
            } else {
                emit_sp_address(emitter, "x29", footer_offset);
            }
        }
        Arch::X86_64 => {
            let local_bytes = frame_size.saturating_sub(16);
            emitter.instruction("push rbp");                                    // save the caller frame pointer on the stack
            emitter.instruction("mov rbp, rsp");                                // establish the current stack pointer as the new frame base
            if local_bytes > 0 {
                emitter.instruction(&format!("sub rsp, {}", local_bytes));      // reserve aligned stack space for local slots below rbp
            }
        }
    }
}

/// Tears down the stack frame and restores the caller's frame state.
/// On AArch64: restores sp/x29/x30 from the frame pointer footer. On x86_64: restores
/// rsp from rbp and pops rbp (the `leave` idiom).
///
/// Both restores are anchored on the frame pointer (x29/rbp) rather than computed by
/// adding `frame_size`/`local_bytes` back onto sp/rsp. The frame pointer is established
/// once at function entry by `emit_frame_prologue` and is never repurposed mid-body (it
/// is excluded from the register allocator's pools), so it stays reliable even when sp
/// itself has drifted from a mid-body cross-block spill imbalance — e.g. a push whose
/// matching pop was skipped on some taken control-flow path. Restoring through the frame
/// pointer corrects that drift instead of reproducing it into the caller; for an already
/// balanced body it yields the identical sp/rsp as the old size-based arithmetic.
pub fn emit_frame_restore(emitter: &mut Emitter, frame_size: usize) {
    assert!(
        frame_size >= 16,
        "frame_size must reserve the 16-byte frame footer (x29/x30), got {frame_size}"
    );
    match emitter.target.arch {
        Arch::AArch64 => {
            // x29 == entry_sp - 16, and [x29] is the saved frame footer regardless
            // of any temporary-stack drift in the function body.
            emitter.instruction("mov x9, x29");                                 // preserve the footer address before restoring the caller frame pointer
            emitter.instruction("ldp x29, x30, [x9]");                          // reload the caller frame pointer and return address
            emitter.instruction("add sp, x9, #16");                             // release the frame only after reading its still-live footer
        }
        Arch::X86_64 => {
            emitter.instruction("leave");                                       // restore rsp from rbp and pop the caller frame pointer
        }
    }
}

/// Aligns the stack for calls made after the frame has been torn down.
///
/// System V AMD64 requires `rsp` to be 16-byte aligned AT the call, so the
/// callee sees `rsp % 16 == 8` once the return address is pushed. After `leave`,
/// `rsp` holds the value it had on entry — already 8 past alignment — so every
/// call emitted after the frame restore is off by 8. Hand-written runtime
/// helpers survive that; compiled Rust does not, because an aligned SSE store to
/// a stack temporary faults.
///
/// That is what took a CI shard down on linux-x86_64 alone: `main` ran, printed
/// its output, and died in the profiler's exit dump — the last call before the
/// exit syscall, and the first one made of Rust. AArch64 keeps `sp` 16-aligned
/// by construction, which is why the same commit was green there.
///
/// Only valid where the path does not return: it discards the low bits of `rsp`.
/// Every caller is on the way to `exit`.
pub fn emit_teardown_call_alignment(emitter: &mut Emitter) {
    if matches!(emitter.target.arch, Arch::X86_64) {
        emitter.instruction("and rsp, -16");                                    // realign for the teardown calls below (this path never returns)
    }
}

/// Emits the function return sequence using the platform `ret` instruction.
pub fn emit_return(emitter: &mut Emitter) {
    emitter.instruction("ret");                                                 // return to the caller using the platform return instruction
}

/// Emits code that computes the address of a local frame slot and stores it in `dest`.
/// Uses the frame pointer (x29/rbp) as the base. A far offset on AArch64 is materialized into
/// `dest` and applied with one register-form `sub`, matching the single `lea` x86_64 uses.
///
/// That far path used to walk the frame pointer down in 4095-byte steps, one `sub` per step. The
/// offset is a compile-time constant, so the walk bought nothing and cost `offset / 4095`
/// instructions in a single dependent chain: in the Symfony `--web` build `sub x9, x9, #4095`
/// alone was 4,706,946 of 40,575,184 emitted lines, 11.6% of the artifact, at ~8.6 steps per
/// address. `movz`/`movk` reaches any 64-bit constant in at most four instructions and typically
/// one, so the whole sequence is now 2-3 instructions whatever the frame size.
///
/// The offset is staged in `dest` itself, which needs no scratch register because `dest` is a pure
/// output here -- every caller treats it as clobbered. The one register that cannot stage it is
/// the frame pointer itself, since `sub x29, x29, x29` would read the staged value as the base;
/// `dest == "x29"` therefore keeps the original walk.
pub fn emit_frame_slot_address(emitter: &mut Emitter, dest: &str, offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if offset == 0 {
                emitter.instruction(&format!("mov {}, x29", dest));             // copy the frame pointer when the requested slot is the frame base itself
            } else if offset <= 4095 {
                emitter.instruction(&format!("sub {}, x29, #{}", dest, offset)); // compute the local-slot address directly from the frame pointer
            } else if dest != "x29" {
                emit_load_int_immediate(emitter, dest, offset as i64);          // materialize the whole distance to the far local slot as one immediate
                emitter.instruction(&format!("sub {}, x29, {}", dest, dest));   // step the frame pointer down to the distant local slot in a single subtraction
            } else {
                emitter.instruction(&format!("mov {}, x29", dest));             // seed the destination register from the frame pointer for a far local-slot address
                let mut remaining = offset;
                while remaining > 0 {
                    let chunk = remaining.min(4095);
                    emitter.instruction(&format!("sub {}, {}, #{}", dest, dest, chunk)); // walk the frame pointer itself down toward the distant local-slot address
                    remaining -= chunk;
                }
            }
        }
        Arch::X86_64 => {
            if offset == 0 {
                emitter.instruction(&format!("mov {}, {}", dest, frame_pointer_reg(emitter))); // copy rbp when the requested slot is the frame base itself
            } else {
                emitter.instruction(&format!(
                    "lea {}, [{} - {}]",
                    dest,
                    frame_pointer_reg(emitter),
                    offset
                ));                                                             // materialize the local-slot address relative to rbp
            }
        }
    }
}

/// The `x28`-relative offset of the slot `offset` bytes below `x29`, when the current function
/// keeps a frame-base register (`Emitter::frame_slot_base`) and one scaled `ldr`/`str` reaches the
/// slot: 64-bit registers only (`x`/`d`), 8-aligned, within the 32 760-byte immediate.
fn based_slot_offset(emitter: &Emitter, reg: &str, offset: usize) -> Option<usize> {
    let base = emitter.frame_slot_base?;
    let wide = reg.starts_with('x') || reg.starts_with('d');
    let distance = base.checked_sub(offset)?;
    (wide && distance % 8 == 0 && distance <= 32_760).then_some(distance)
}

/// Stores `reg` into the local frame slot at `offset` from the frame pointer, using x9 as scratch.
/// On AArch64: uses `stur` for offsets ≤ 255, otherwise computes the address first.
/// On x86_64: stores via `[rbp - offset]` with a mov instruction; float registers use movsd.
pub fn store_at_offset(emitter: &mut Emitter, reg: &str, offset: usize) {
    store_at_offset_scratch(emitter, reg, offset, "x9");
}

/// Stores `reg` into the local frame slot at `offset` from the frame pointer, using `scratch` as scratch.
/// This variant accepts a caller-specified scratch register to avoid conflicts in multi-register sequences.
pub fn store_at_offset_scratch(emitter: &mut Emitter, reg: &str, offset: usize, scratch: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if offset <= 255 {
                emitter.instruction(&format!("stur {}, [x29, #-{}]", reg, offset)); // store via unscaled immediate offset
            } else if let Some(base_offset) = based_slot_offset(emitter, reg, offset) {
                emitter.instruction(&format!("str {}, [x28, #{}]", reg, base_offset)); // store through the frame-base register in one instruction
            } else {
                emit_frame_slot_address(emitter, scratch, offset);
                emitter.instruction(&format!("str {}, [{}]", reg, scratch));    // store via computed address
            }
        }
        Arch::X86_64 => {
            let slot = if offset == 0 {
                format!("[{}]", frame_pointer_reg(emitter))
            } else {
                format!("[{} - {}]", frame_pointer_reg(emitter), offset)
            };
            if is_float_register(reg) {
                emitter.instruction(&format!("movsd QWORD PTR {}, {}", slot, reg)); // store the floating-point payload into the local frame slot
            } else {
                emitter.instruction(&format!("mov QWORD PTR {}, {}", slot, reg)); // store the integer or pointer payload into the local frame slot
            }
        }
    }
}

/// Loads the local frame slot at `offset` from the frame pointer into `reg`.
/// On AArch64: uses `ldur` for offsets ≤ 255, otherwise computes the address first.
/// On x86_64: loads via `[rbp - offset]` with a mov instruction; float registers use movsd.
///
/// The address is computed INTO THE DESTINATION, which the load is about to overwrite anyway, so
/// a deep frame cannot disturb a register the caller is still holding. It used to borrow x9
/// unconditionally, and that made correct-looking code fail once a frame grew past the 255-byte
/// `ldur` immediate -- a caller holding a value in x9 across this call lost it, with no diagnostic
/// and no change to its own source.
///
/// MEASURED: `foreach ($a as &$v) { $p($v); }` printed the values, and adding the unused KEY
/// (`as $k => &$v`) pushed the slot past the immediate range and printed nothing, because
/// `lower_invoker_ref_arg` keeps the marker's SOURCE TAG in x9 across the address materialization.
/// php 8.5.10 prints `p(1);p(2);` for both spellings.
///
/// A float destination cannot hold an address, so it keeps a general-purpose scratch.
pub fn load_at_offset(emitter: &mut Emitter, reg: &str, offset: usize) {
    let scratch = if is_float_register(reg) { "x9" } else { reg };
    load_at_offset_scratch(emitter, reg, offset, scratch);
}

/// Loads the local frame slot at `offset` from the frame pointer into `reg`, using `scratch` as scratch.
/// This variant accepts a caller-specified scratch register to avoid conflicts in multi-register sequences.
pub fn load_at_offset_scratch(emitter: &mut Emitter, reg: &str, offset: usize, scratch: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if offset <= 255 {
                emitter.instruction(&format!("ldur {}, [x29, #-{}]", reg, offset)); // load via unscaled immediate offset
            } else if let Some(base_offset) = based_slot_offset(emitter, reg, offset) {
                emitter.instruction(&format!("ldr {}, [x28, #{}]", reg, base_offset)); // load through the frame-base register in one instruction
            } else {
                emit_frame_slot_address(emitter, scratch, offset);
                emitter.instruction(&format!("ldr {}, [{}]", reg, scratch));    // load via computed address
            }
        }
        Arch::X86_64 => {
            let slot = if offset == 0 {
                format!("[{}]", frame_pointer_reg(emitter))
            } else {
                format!("[{} - {}]", frame_pointer_reg(emitter), offset)
            };
            if is_float_register(reg) {
                emitter.instruction(&format!("movsd {}, QWORD PTR {}", reg, slot)); // load the floating-point payload from the local frame slot
            } else {
                emitter.instruction(&format!("mov {}, QWORD PTR {}", reg, slot)); // load the integer or pointer payload from the local frame slot
            }
        }
    }
}

/// Emits a register-to-register move, choosing the move form by the register
/// classes of both operands. Handles same-class moves and moves between a
/// general-purpose and a floating-point register (a raw 64-bit bit copy, used
/// when a float value's payload must reach an integer register). Emits nothing
/// when source and destination are identical.
pub fn emit_reg_move(emitter: &mut Emitter, dst: &str, src: &str) {
    if dst == src {
        return;
    }
    let dst_float = is_float_register(dst);
    let src_float = is_float_register(src);
    match emitter.target.arch {
        Arch::AArch64 => {
            if dst_float || src_float {
                emitter.instruction(&format!("fmov {}, {}", dst, src));         // move between FP and/or GP registers
            } else {
                emitter.instruction(&format!("mov {}, {}", dst, src));          // move integer/pointer register
            }
        }
        Arch::X86_64 => {
            if dst_float && src_float {
                emitter.instruction(&format!("movsd {}, {}", dst, src));        // move scalar double between XMM registers
            } else if dst_float || src_float {
                emitter.instruction(&format!("movq {}, {}", dst, src));         // move 64-bit payload between GP and XMM
            } else {
                emitter.instruction(&format!("mov {}, {}", dst, src));          // move integer/pointer register
            }
        }
    }
}

/// Loads a value from an arbitrary address in memory into `reg`.
/// `addr_reg` holds the base address; `byte_offset` is added (AArch64 scaled immediate, x86_64 additive).
/// On x86_64, float registers use movsd; integers use mov.
pub fn emit_load_from_address(
    emitter: &mut Emitter,
    reg: &str,
    addr_reg: &str,
    byte_offset: usize,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if byte_offset == 0 {
                emitter.instruction(&format!("ldr {}, [{}]", reg, addr_reg));   // load the requested value directly from the computed address register
            } else {
                emitter.instruction(&format!("ldr {}, [{}, #{}]", reg, addr_reg, byte_offset)); // load the requested value from the computed address register plus byte offset
            }
        }
        Arch::X86_64 => {
            let slot = if byte_offset == 0 {
                format!("[{}]", addr_reg)
            } else {
                format!("[{} + {}]", addr_reg, byte_offset)
            };
            if is_float_register(reg) {
                emitter.instruction(&format!("movsd {}, QWORD PTR {}", reg, slot)); // load the floating-point payload through the computed address register
            } else {
                emitter.instruction(&format!("mov {}, QWORD PTR {}", reg, slot)); // load the integer or pointer payload through the computed address register
            }
        }
    }
}

/// Stores `reg` to an arbitrary address in memory.
/// `addr_reg` holds the base address; `byte_offset` is added (AArch64 scaled immediate, x86_64 additive).
/// On x86_64, float registers use movsd; integers use mov.
pub fn emit_store_to_address(emitter: &mut Emitter, reg: &str, addr_reg: &str, byte_offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if byte_offset == 0 {
                emitter.instruction(&format!("str {}, [{}]", reg, addr_reg));   // store the requested value directly through the computed address register
            } else {
                emitter.instruction(&format!("str {}, [{}, #{}]", reg, addr_reg, byte_offset)); // store the requested value through the computed address register plus byte offset
            }
        }
        Arch::X86_64 => {
            let slot = if byte_offset == 0 {
                format!("[{}]", addr_reg)
            } else {
                format!("[{} + {}]", addr_reg, byte_offset)
            };
            if is_float_register(reg) {
                emitter.instruction(&format!("movsd QWORD PTR {}, {}", slot, reg)); // store the floating-point payload through the computed address register
            } else {
                emitter.instruction(&format!("mov QWORD PTR {}, {}", slot, reg)); // store the integer or pointer payload through the computed address register
            }
        }
    }
}

/// Stores zero to an arbitrary address in memory using the architectural zero register.
/// On AArch64 uses xzr; on x86_64 stores an explicit 0. `byte_offset` is added to `addr_reg`.
pub fn emit_store_zero_to_address(emitter: &mut Emitter, addr_reg: &str, byte_offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if byte_offset == 0 {
                emitter.instruction(&format!("str xzr, [{}]", addr_reg));       // store architectural zero directly through the computed address register
            } else {
                emitter.instruction(&format!("str xzr, [{}, #{}]", addr_reg, byte_offset)); // store architectural zero through the computed address register plus byte offset
            }
        }
        Arch::X86_64 => {
            let slot = if byte_offset == 0 {
                format!("[{}]", addr_reg)
            } else {
                format!("[{} + {}]", addr_reg, byte_offset)
            };
            emitter.instruction(&format!("mov QWORD PTR {}, 0", slot));         // store an integer zero through the computed address register
        }
    }
}

/// Loads a spilled incoming call argument from the caller stack into `reg`.
/// On AArch64 uses the frame pointer (x29) as base with positive offset; a far offset is
/// materialized into the x9 scratch and applied in one `add`. On x86_64 uses rbp with positive
/// offset. See [`emit_frame_slot_address`] for why the constant needs no extra register.
pub fn load_from_caller_stack(emitter: &mut Emitter, reg: &str, offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if offset <= 4095 {
                emitter.instruction(&format!("ldr {}, [x29, #{}]", reg, offset)); // load a spilled incoming argument from the caller stack
            } else {
                emit_load_int_immediate(emitter, "x9", offset as i64);          // materialize the whole distance to the far caller-stack slot as one immediate
                emitter.instruction("add x9, x29, x9");                         // advance the frame base to the distant caller-stack slot in a single addition
                emitter.instruction(&format!("ldr {}, [x9]", reg));             // load the spilled incoming argument through the computed caller-stack pointer
            }
        }
        Arch::X86_64 => {
            let slot = if offset == 0 {
                format!("[{}]", frame_pointer_reg(emitter))
            } else {
                format!("[{} + {}]", frame_pointer_reg(emitter), offset)
            };
            if is_float_register(reg) {
                emitter.instruction(&format!("movsd {}, QWORD PTR {}", reg, slot)); // load a spilled floating-point argument from the caller stack area
            } else {
                emitter.instruction(&format!("mov {}, QWORD PTR {}", reg, slot)); // load a spilled integer or pointer argument from the caller stack area
            }
        }
    }
}

/// Zero-initializes the local frame slot at `offset` from the frame pointer.
/// On AArch64 uses the xzr register via `store_at_offset`; on x86_64 emits a mov with immediate 0.
pub fn emit_store_zero_to_local_slot(emitter: &mut Emitter, offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            store_at_offset(emitter, "xzr", offset); // zero-initialize the local slot with the architectural zero register
        }
        Arch::X86_64 => {
            if offset == 0 {
                emitter.instruction(&format!(
                    "mov QWORD PTR [{}], 0",
                    frame_pointer_reg(emitter)
                ));                                                             // zero-initialize the frame-base slot directly through rbp
            } else {
                emitter.instruction(&format!(
                    "mov QWORD PTR [{} - {}], 0",
                    frame_pointer_reg(emitter),
                    offset
                ));                                                             // zero-initialize the requested local slot relative to rbp
            }
        }
    }
}

/// Zero-initializes the 8-byte local frame words at the given offsets from the frame pointer.
///
/// On AArch64 adjacent words are cleared two at a time with `stp xzr, xzr`, and a run beyond the
/// pair-store range reaches its words from one computed base instead of forming an address per
/// word: a function with a dozen cleanup slots used to spend two instructions on each. x86_64
/// keeps one `mov` per word. Offsets may arrive in any order and duplicates are cleared once.
pub fn emit_zero_local_slot_words(emitter: &mut Emitter, offsets: &[usize]) {
    let mut words: Vec<usize> = offsets.to_vec();
    words.sort_unstable_by(|a, b| b.cmp(a));
    words.dedup();
    if !matches!(emitter.target.arch, Arch::AArch64) {
        for offset in words {
            emit_store_zero_to_local_slot(emitter, offset);
        }
        return;
    }
    // `words` is in ascending ADDRESS order (a larger offset sits lower in the frame).
    let mut index = 0;
    while index < words.len() {
        let mut run_end = index + 1;
        while run_end < words.len() && words[run_end - 1] == words[run_end] + 8 {
            run_end += 1;
        }
        let run = &words[index..run_end];
        index = run_end;
        if run.len() == 1 {
            emit_store_zero_to_local_slot(emitter, run[0]);
            continue;
        }
        let lowest = run[0];
        // `stp` takes a signed, 8-scaled immediate in -512..=504 from its base.
        let (base, base_offset) = if lowest <= 512 {
            ("x29", lowest as i64)
        } else {
            emit_frame_slot_address(emitter, "x9", lowest);
            ("x9", 0)
        };
        let mut word = 0usize;
        while word < run.len() {
            let imm = if base == "x29" {
                -base_offset + (word as i64) * 8
            } else {
                (word as i64) * 8
            };
            if imm > 504 {
                // Past the pair-store range: re-base at this word.
                emit_frame_slot_address(emitter, "x9", run[word]);
                let rebased = &run[word..];
                emit_zero_run_from_x9(emitter, rebased.len());
                break;
            }
            if word + 1 < run.len() {
                emitter.instruction(&format!("stp xzr, xzr, [{}, #{}]", base, imm)); // zero two adjacent frame words with one pair store
                word += 2;
            } else if base == "x29" {
                // `str` takes only a positive scaled immediate from x29; `stur` reaches -256..255.
                if imm >= -256 {
                    emitter.instruction(&format!("stur xzr, [x29, #{}]", imm)); // zero the last frame word of the run
                } else {
                    emit_store_zero_to_local_slot(emitter, run[word]);
                }
                word += 1;
            } else {
                emitter.instruction(&format!("str xzr, [x9, #{}]", imm));       // zero the last frame word of the run
                word += 1;
            }
        }
    }
}

/// Clears `count` consecutive words upward from the address already in x9.
fn emit_zero_run_from_x9(emitter: &mut Emitter, count: usize) {
    let mut word = 0usize;
    let mut imm = 0usize;
    while word < count {
        if imm > 504 {
            emitter.instruction(&format!("add x9, x9, #{}", imm));              // advance the run base past the pair-store range
            imm = 0;
        }
        if word + 1 < count {
            emitter.instruction(&format!("stp xzr, xzr, [x9, #{}]", imm));      // zero two adjacent frame words with one pair store
            word += 2;
            imm += 16;
        } else {
            emitter.instruction(&format!("str xzr, [x9, #{}]", imm));           // zero the last frame word of the run
            word += 1;
        }
    }
}

/// Saves the return value into a hidden frame slot so it survives a tail-call or callback frame switch.
/// Float values use the float result register; strings use string_result_regs (pointer + length);
/// scalars use the integer result register. `return_offset` is the slot for the primary value; string
/// length is stored 8 bytes before it.
#[cfg(test)]
pub fn emit_preserve_return_value(
    emitter: &mut Emitter,
    return_ty: &PhpType,
    return_offset: usize,
) {
    match return_ty.codegen_repr() {
        PhpType::Float => {
            store_at_offset(emitter, float_result_reg(emitter), return_offset); // preserve the float return value in the hidden frame slot
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            store_at_offset(emitter, ptr_reg, return_offset); // preserve the string return pointer in the hidden frame slot
            store_at_offset(emitter, len_reg, return_offset - 8); // preserve the string return length in the hidden frame slot
        }
        _ => {
            store_at_offset(emitter, int_result_reg(emitter), return_offset); // preserve the scalar or pointer-like return value in the hidden frame slot
        }
    }
}

/// Restores the return value from a hidden frame slot after a tail-call or callback frame switch.
/// Reverse of `emit_preserve_return_value`: loads based on `return_ty` codegen repr into the
/// appropriate result registers.
#[cfg(test)]
pub fn emit_restore_return_value(emitter: &mut Emitter, return_ty: &PhpType, return_offset: usize) {
    match return_ty.codegen_repr() {
        PhpType::Float => {
            load_at_offset(emitter, float_result_reg(emitter), return_offset); // restore the preserved float return value from the hidden frame slot
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            load_at_offset(emitter, ptr_reg, return_offset); // restore the preserved string return pointer from the hidden frame slot
            load_at_offset(emitter, len_reg, return_offset - 8); // restore the preserved string return length from the hidden frame slot
        }
        _ => {
            load_at_offset(emitter, int_result_reg(emitter), return_offset); // restore the preserved scalar or pointer-like return value from the hidden frame slot
        }
    }
}

/// Allocates or releases `amount` bytes from the stack pointer.
/// On AArch64 emits at most 4080-byte chunks to stay within sub/add immediate limits.
/// On x86_64 emits a single sub or add. `subtract=true` reserves space; `subtract=false` releases.
pub(crate) fn emit_adjust_sp(emitter: &mut Emitter, amount: usize, subtract: bool) {
    match emitter.target.arch {
        Arch::AArch64 => {
            let mut remaining = amount;
            while remaining > 0 {
                let chunk = remaining.min(4080);
                if subtract {
                    emitter.instruction(&format!("sub sp, sp, #{}", chunk));    // reserve stack space for spilled outgoing call arguments
                } else {
                    emitter.instruction(&format!("add sp, sp, #{}", chunk));    // release temporary outgoing call-argument stack space
                }
                remaining -= chunk;
            }
        }
        Arch::X86_64 => {
            if amount == 0 {
                return;
            }
            if subtract {
                emitter.instruction(&format!("sub rsp, {}", amount));           // reserve stack space for spilled outgoing call arguments
            } else {
                emitter.instruction(&format!("add rsp, {}", amount));           // release temporary outgoing call-argument stack space
            }
        }
    }
}

/// Computes the address of a temporary stack slot relative to the current stack pointer and
/// stores it in `scratch`. Used for stack positions that are not part of the fixed frame layout.
/// On AArch64 a far offset is materialized and applied in one `add`; on x86_64 uses lea with rsp
/// base. See [`emit_frame_slot_address`] for why the constant is staged in the output register.
pub(crate) fn emit_sp_address(emitter: &mut Emitter, scratch: &str, offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if offset == 0 {
                emitter.instruction(&format!("mov {}, sp", scratch));           // copy the current stack pointer when the requested slot is at sp
            } else if offset <= 4095 {
                emitter.instruction(&format!("add {}, sp, #{}", scratch, offset)); // compute a nearby temporary stack-slot address directly from the stack pointer
            } else {
                emit_load_int_immediate(emitter, scratch, offset as i64);       // materialize the whole distance to the far stack slot as one immediate
                emitter.instruction(&format!("add {}, sp, {}", scratch, scratch)); // advance the stack pointer to the distant stack slot in a single addition
            }
        }
        Arch::X86_64 => {
            if offset == 0 {
                emitter.instruction(&format!("mov {}, rsp", scratch));          // copy the current stack pointer when the requested stack slot is at rsp
            } else {
                emitter.instruction(&format!("lea {}, [rsp + {}]", scratch, offset)); // materialize the temporary stack-slot address relative to rsp
            }
        }
    }
}
