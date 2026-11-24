//! Purpose:
//! Links guard records and transfers their owners before reentrant release.
//!
//! Called from:
//! - Compiler ownership-guard and protected-callback adapters.
//!
//! Key details:
//! - The host supplies chain storage, owner offsets and callback argument spill offsets.
//! - Taking an owner clears its slot before any release can invoke user code.
//! - Constructor policy and exception-handler installation stay in the compiler.

use crate::{abi, Arch, Emitter, FrameOffset, GuardLayout, RecordOffset, TargetAbi};
use crate::activation_records::{CLEANUP, GUARD_BYTES, GUARD_OWNER};

/// Emits guard insertion/removal for the host activation-chain ABI.
pub fn emit_guard_chain(emitter: &mut impl Emitter, layout: GuardLayout, head_symbol: &str) {
    let owner_offset = layout.owner().bytes();
    if emitter.arch() == Arch::AArch64 { aarch64(emitter, owner_offset, head_symbol); }
    else { x86_64(emitter, owner_offset, head_symbol); }
}

/// Unlinks an AArch64 guard from the activation chain and transfers exceptional owners to the GC.
fn aarch64(emitter: &mut impl Emitter, owner_offset: usize, head_symbol: &str) {
    emitter.label_global("__rt_exception_guard_owned");
    abi::emit_symbol_address(emitter, "x9", head_symbol);
    emitter.instruction("cmp x1, #0");                                          // zero insertion tokens start a new parameter-ordered group at the chain head
    emitter.instruction("csel x9, x9, x1, eq");                                 // otherwise append after the preceding parameter's guard
    emitter.instruction("ldr x10, [x9]");                                       // preserve the activation previously reached through this insertion link
    emitter.instruction("str x10, [x0]");                                       // connect the new record to its predecessor in unwind order
    emitter.instruction("str x0, [x0, #16]");                                   // pass this guard record as callback data during unwinding
    emitter.instruction("str x0, [x9]");                                        // publish the completed guard without disturbing newer parameters
    emitter.instruction("ret");                                                 // return the guard address as its normal-path removal token
    emitter.label_global("__rt_exception_unguard_owned");
    abi::emit_symbol_address(emitter, "x9", head_symbol);
    emitter.label("__rt_exception_unguard_owned_loop");
    emitter.instruction("ldr x10, [x9]");                                       // inspect the activation referenced by the current link
    emitter.instruction("cbz x10, __rt_exception_unguard_owned_done");          // stop defensively when this guard has already been removed
    emitter.instruction("cmp x10, x0");                                         // recognize the guard whose owner returns to normal EIR cleanup
    emitter.instruction("b.eq __rt_exception_unguard_owned_found");             // detach exactly this guard while preserving later registrations
    emitter.instruction("mov x9, x10");                                         // continue through the preceding-activation link
    emitter.instruction("b __rt_exception_unguard_owned_loop");                 // search older activations without touching their ownership
    emitter.label("__rt_exception_unguard_owned_found");
    emitter.instruction("ldr x10, [x0]");                                       // recover the guarded value's previous activation
    emitter.instruction("str x10, [x9]");                                       // connect the newer activation directly to its predecessor
    emitter.instruction(&format!("str xzr, [x0, #{owner_offset}]"));            // clear exceptional ownership before ordinary EIR release
    emitter.label("__rt_exception_unguard_owned_done");
    emitter.instruction("ret");                                                 // leave value ownership unchanged on normal completion

}

/// Applies the identical guard-chain operations with SysV arguments and the native GC convention.
fn x86_64(emitter: &mut impl Emitter, owner_offset: usize, head_symbol: &str) {
    emitter.label_global("__rt_exception_guard_owned");
    abi::emit_symbol_address(emitter, "r10", head_symbol);
    emitter.instruction("test rsi, rsi");                                       // distinguish a new call group from insertion after an earlier parameter
    emitter.instruction("cmovnz r10, rsi");                                     // preserve parameter-order cleanup independently of source evaluation order
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // retain the activation previously referenced by the insertion link
    emitter.instruction("mov QWORD PTR [rdi], r11");                            // connect the new guard to its next cleanup activation
    emitter.instruction("mov QWORD PTR [rdi + 16], rdi");                       // provide this record as the callback's opaque data
    emitter.instruction("mov QWORD PTR [r10], rdi");                            // publish the completed guard in PHP parameter order
    emitter.instruction("mov rax, rdi");                                        // return the stack-local guard token through the C result register
    emitter.instruction("ret");                                                 // retain transferred exceptional ownership until unguarding or unwinding
    emitter.label_global("__rt_exception_unguard_owned");
    abi::emit_symbol_address(emitter, "r10", head_symbol);
    emitter.label("__rt_exception_unguard_owned_loop");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // inspect the next activation without changing the guard token
    emitter.instruction("test r11, r11");                                       // recognize the end of the activation chain
    emitter.instruction("jz __rt_exception_unguard_owned_done");                // permit an already-removed guard without touching unrelated records
    emitter.instruction("cmp r11, rdi");                                        // identify the exact guard consumed by normal EIR cleanup
    emitter.instruction("je __rt_exception_unguard_owned_found");               // preserve newer registrations when arguments were reordered
    emitter.instruction("mov r10, r11");                                        // select this activation's previous-record link
    emitter.instruction("jmp __rt_exception_unguard_owned_loop");               // continue searching older activations
    emitter.label("__rt_exception_unguard_owned_found");
    emitter.instruction("mov r11, QWORD PTR [rdi]");                            // recover the activation that preceded the removed guard
    emitter.instruction("mov QWORD PTR [r10], r11");                            // splice out this guard without consuming its PHP value
    emitter.instruction(&format!("mov QWORD PTR [rdi + {owner_offset}], 0"));   // return ownership exclusively to normal argument cleanup
    emitter.label("__rt_exception_unguard_owned_done");
    emitter.instruction("ret");                                                 // complete guard removal without retaining or releasing a heap value

}

/// Loads the callback record and consumes its owner slot without releasing the value yet.
pub fn emit_take_guard_owner(emitter: &mut impl Emitter, layout: GuardLayout) {
    let owner_offset = layout.owner().bytes();
    let argument_offset = layout.argument().bytes();
    if emitter.arch() == Arch::AArch64 {
        emitter.instruction(&format!("ldr x9, [sp, #{argument_offset}]"));      // recover the cleanup record passed as the first C argument
        emitter.instruction(&format!("ldr x0, [x9, #{owner_offset}]"));         // transfer the guarded owner to the native GC convention
        emitter.instruction(&format!("str xzr, [x9, #{owner_offset}]"));        // clear ownership before a destructor reenters PHP
    } else {
        emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {argument_offset}]")); // recover the callback's guard-record argument
        emitter.instruction(&format!("mov rax, QWORD PTR [r10 + {owner_offset}]")); // pass the owned heap value through the native GC convention
        emitter.instruction(&format!("mov QWORD PTR [r10 + {owner_offset}], 0")); // prevent duplicate release during nested exception propagation
    }
}

/// Transfers the guard owner into the selected native release protocol.
pub fn emit_release_body(emitter: &mut impl Emitter, symbol: &str, layout: GuardLayout) {
    emit_take_guard_owner(emitter, layout);
    abi::emit_call_label(emitter, symbol);
}

/// Publishes a preallocated stack guard with the caller's owner and cleanup entry.
pub fn emit_publish_at_stack(
    emitter: &mut impl TargetAbi,
    owner: &str,
    cleanup: &str,
    scratch: &str,
    record: RecordOffset,
    owner_field: RecordOffset,
) {
    emitter.emit_store_to_sp(owner, record.bytes() + owner_field.bytes());
    emitter.emit_symbol_address(scratch, cleanup);
    emitter.emit_store_to_sp(scratch, record.bytes() + CLEANUP);
    emitter.emit_temporary_stack_address(emitter.int_arg_reg_name(0), record.bytes());
    emitter.emit_load_int_immediate(emitter.int_arg_reg_name(1), 0);
    emitter.emit_call_label("__rt_exception_guard_owned");
}

/// Detaches a preallocated guard and leaves its storage to the caller.
pub fn emit_detach_at_stack(emitter: &mut impl TargetAbi, record: RecordOffset) {
    emitter.emit_temporary_stack_address(emitter.int_arg_reg_name(0), record.bytes());
    emitter.emit_call_label("__rt_exception_unguard_owned");
}

/// Writes a fixed frame guard's owner and cleanup while the caller loads its anchor.
pub fn emit_prepare_anchored(
    emitter: &mut impl TargetAbi,
    record: FrameOffset,
    owner: &str,
    cleanup: &str,
    scratch: &str,
) {
    emitter.store_at_offset(owner, record.bytes() - GUARD_OWNER);
    emitter.emit_symbol_address(scratch, cleanup);
    emitter.store_at_offset(scratch, record.bytes() - CLEANUP);
}

/// Installs a prepared guard after the caller has loaded the anchor into argument one.
pub fn emit_link_anchored(emitter: &mut impl TargetAbi, record: FrameOffset) {
    emitter.emit_frame_slot_address(emitter.int_arg_reg_name(0), record.bytes());
    emitter.emit_call_label("__rt_exception_guard_owned");
}

/// Unlinks the guard addressed by native argument zero.
pub fn emit_detach_addressed(emitter: &mut impl TargetAbi) {
    emitter.emit_call_label("__rt_exception_unguard_owned");
}

/// Takes a resource payload whose empty value differs from a null heap owner.
pub fn emit_take_resource(
    emitter: &mut impl TargetAbi,
    record: &str,
    result: &str,
    scratch: &str,
    empty: i64,
) {
    emitter.emit_load_from_address(result, record, GUARD_OWNER);
    emitter.emit_load_int_immediate(scratch, empty);
    emitter.emit_store_to_address(scratch, record, GUARD_OWNER);
}

/// Borrows the current owner of a pointer-word slot for a caller's detach decision.
pub fn emit_load_indexed_owner(emitter: &mut impl Emitter, table: &str, index: &str, owner: &str) {
    emitter.instruction(&match emitter.arch() {
        Arch::AArch64 => format!("ldr {owner}, [{table}, {index}, lsl #3]"),
        Arch::X86_64 => format!("mov {owner}, QWORD PTR [{table} + {index}*8]"),
    });                                                                         // read the current owner without retaining it
}

/// Clears an indexed slot whose ownership has been transferred to the caller.
pub fn emit_clear_indexed_owner(emitter: &mut impl Emitter, table: &str, index: &str) {
    emitter.instruction(&match emitter.arch() {
        Arch::AArch64 => format!("str xzr, [{table}, {index}, lsl #3]"),
        Arch::X86_64 => format!("mov QWORD PTR [{table} + {index}*8], 0"),
    });                                                                         // detach the table's owner before it can be released
}

/// Registers the native result-register owner in a 32-byte stack record at the supplied offsets.
pub fn emit_publish_in_frame(emitter: &mut impl Emitter, arm: usize, x86: usize) {
    emit_publish_frame_owner(emitter, "x0", arm, x86);
}

/// Publishes a frame guard for an ARM64 owner register or the x86-64 result register.
pub fn emit_publish_frame_owner(emitter: &mut impl Emitter, arm_owner: &str, arm: usize, x86: usize) {
    if emitter.arch() == Arch::AArch64 {
        emitter.instruction(&format!("str {arm_owner}, [sp, #{}]", arm + GUARD_OWNER)); // publish exceptional ownership before running PHP
        abi::emit_symbol_address(emitter, "x9", "__rt_exception_release_owned");
        emitter.instruction(&format!("str x9, [sp, #{}]", arm + CLEANUP));      // use the protected heap-owner release callback
        emitter.instruction(&format!("add x0, sp, #{arm}"));                    // provide the aligned activation record
        emitter.instruction("mov x1, #0");                                      // insert this owner at the current activation head
    } else {
        emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", x86 - GUARD_OWNER));// retain the owner in exceptional cleanup storage
        abi::emit_symbol_address(emitter, "r10", "__rt_exception_release_owned");
        emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", x86 - CLEANUP));// install protected release for the staged owner
        emitter.instruction(&format!("lea rdi, [rbp - {x86}]"));                // identify this stack-local activation
        emitter.instruction("xor esi, esi");                                    // insert ownership at the activation chain head
    }
    abi::emit_call_label(emitter, "__rt_exception_guard_owned");
}

/// Detaches one exceptional owner before ordinary release or transfer to the caller.
pub fn emit_detach_in_frame(emitter: &mut impl Emitter, arm: usize, x86: usize) {
    if emitter.arch() == Arch::AArch64 {
        emitter.instruction(&format!("add x0, sp, #{arm}"));                    // identify the completed exceptional ownership scope
    } else {
        emitter.instruction(&format!("lea rdi, [rbp - {x86}]"));                // identify the exact activation to retire
    }
    abi::emit_call_label(emitter, "__rt_exception_unguard_owned");
}

/// A contiguous group of exceptional owner records within a fixed native frame.
pub struct GuardedFrameOwners {
    start: FrameOffset,
    count: usize,
    frame_bytes: usize,
}

impl GuardedFrameOwners {
    /// Reserves a consecutive guard area, including the native frame-link convention.
    pub const fn new(start: FrameOffset, count: usize, frame_bytes: usize) -> Self {
        // The SysV frame prologue describes its two-word frame link in frame_bytes.
        assert!(start.bytes() + count * GUARD_BYTES <= frame_bytes - 16);
        Self { start, count, frame_bytes }
    }

    /// Computes a guard record's stack-relative address.
    const fn record_offset(&self, slot: usize) -> usize {
        assert!(slot < self.count);
        self.start.bytes() + slot * GUARD_BYTES
    }

    /// Returns the offset of the mutable owner word for callers passing it by address.
    pub const fn owner_offset(&self, slot: usize) -> usize {
        self.record_offset(slot) + GUARD_OWNER
    }

    /// Publishes every initially vacant owner record in slot order.
    pub fn emit_publish_empty(&self, emitter: &mut impl Emitter) {
        let result = emitter.int_result_reg();
        for slot in 0..self.count {
            emitter.emit_load_int_immediate(result, 0);
            let offset = self.record_offset(slot);
            emit_publish_in_frame(emitter, offset, self.frame_bytes - 16 - offset);
        }
    }

    /// Stores the latest native result in a live guard before the next operation can reenter.
    pub fn emit_save(&self, emitter: &mut impl Emitter, slot: usize) {
        let stack = if emitter.arch() == Arch::AArch64 { "sp" } else { "rsp" };
        emitter.emit_store_to_address(emitter.int_result_reg(), stack, self.owner_offset(slot));
    }

    /// Transfers the owner without detaching the now-vacant guard.
    pub fn emit_clear(&self, emitter: &mut impl Emitter, slot: usize) {
        let stack = if emitter.arch() == Arch::AArch64 { "sp" } else { "rsp" };
        emitter.emit_store_zero_to_address(stack, self.owner_offset(slot));
    }

    /// Takes a slot owner before calling its potentially reentrant native release.
    pub fn emit_release(&self, emitter: &mut impl Emitter, slot: usize) {
        let stack = if emitter.arch() == Arch::AArch64 { "sp" } else { "rsp" };
        emitter.emit_load_from_address(emitter.int_result_reg(), stack, self.owner_offset(slot));
        self.emit_clear(emitter, slot);
        emitter.emit_call_label("__rt_decref_any");
    }

    /// Releases all temporary owners in reverse order while the selected result stays guarded.
    pub fn emit_release_except(&self, emitter: &mut impl Emitter, result: usize) {
        for slot in (0..self.count).rev().filter(|slot| *slot != result) {
            self.emit_release(emitter, slot);
        }
    }

    /// Detaches the completed guard area in reverse publication order.
    pub fn emit_detach(&self, emitter: &mut impl Emitter) {
        for slot in (0..self.count).rev() {
            let offset = self.record_offset(slot);
            emit_detach_in_frame(emitter, offset, self.frame_bytes - 16 - offset);
        }
    }
}
