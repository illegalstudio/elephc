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

use crate::{abi, Arch, Emitter, GuardLayout, RecordOffset, TargetAbi};

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
    emitter.emit_store_to_sp(scratch, record.bytes() + 8);
    emitter.emit_temporary_stack_address(emitter.int_arg_reg_name(0), record.bytes());
    emitter.emit_load_int_immediate(emitter.int_arg_reg_name(1), 0);
    emitter.emit_call_label("__rt_exception_guard_owned");
}

/// Detaches a preallocated guard and leaves its storage to the caller.
pub fn emit_detach_at_stack(emitter: &mut impl TargetAbi, record: RecordOffset) {
    emitter.emit_temporary_stack_address(emitter.int_arg_reg_name(0), record.bytes());
    emitter.emit_call_label("__rt_exception_unguard_owned");
}

/// Borrows the current owner of a pointer-word slot for a caller's detach decision.
pub fn emit_load_indexed_owner(emitter: &mut impl Emitter, table: &str, index: &str, owner: &str) {
    emitter.instruction(&match emitter.arch() {
        Arch::AArch64 => format!("ldr {owner}, [{table}, {index}, lsl #3]"),
        Arch::X86_64 => format!("mov {owner}, QWORD PTR [{table} + {index}*8]"),
    });                                                                       // read the current owner without retaining it
}

/// Clears an indexed slot whose ownership has been transferred to the caller.
pub fn emit_clear_indexed_owner(emitter: &mut impl Emitter, table: &str, index: &str) {
    emitter.instruction(&match emitter.arch() {
        Arch::AArch64 => format!("str xzr, [{table}, {index}, lsl #3]"),
        Arch::X86_64 => format!("mov QWORD PTR [{table} + {index}*8], 0"),
    });                                                                       // detach the table's owner before it can be released
}
