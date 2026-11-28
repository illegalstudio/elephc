//! Purpose:
//! Transfers guarded native callback values into and out of opaque result records.
//!
//! Called from:
//! - Compiler emission of the fileinfo stream-cast callback adapter.
//!
//! Key details:
//! - A temporary unwind guard covers the result during allocation.
//! - Release frees the record before invoking the captured value's cleanup.

use crate::{Arch, Emitter, ValueTag, abi};

/// Runtime value tags selecting the cleanup of a captured callback result.
#[derive(Clone, Copy)]
pub struct OwnerTags {
    /// Tag for a raw owned string.
    pub string: ValueTag,
    /// Tag for a callable descriptor.
    pub callable: ValueTag,
    /// First tag in the contiguous range of refcounted value owners.
    pub first_refcounted: ValueTag,
    /// Last tag in the contiguous range of refcounted value owners.
    pub last_refcounted: ValueTag,
}

/// Reads the status word through the C receiver without taking a value owner.
pub fn emit_read_status(emitter: &mut impl Emitter) {
    let ret = abi::int_result_reg(emitter);
    let arg = abi::int_arg_reg_name(emitter, 0);
    abi::emit_load_from_address(emitter, ret, arg, 0);
    emitter.instruction("ret");                                                 // return the status while preserving the captured value
}

/// Protects a staged callback owner until the opaque result record has adopted it.
pub fn emit_pack(
    emitter: &mut impl Emitter, guard_owner_offset: usize, tags: OwnerTags,
) {
    let arm = emitter.arch() == Arch::AArch64;
    let (ret, frame, scratch, callback) = if arm { ("x0", "sp", "x9", "x14") } else { ("rax", "rsp", "r9", "r11") };
    // -- guard the transferred callback owner before allocating its opaque record --
    abi::emit_store_zero_to_address(emitter, frame, 64 + guard_owner_offset);
    abi::emit_symbol_address(emitter, callback, "__rt_exception_release_owned");
    abi::emit_store_to_address(emitter, callback, frame, 72);
    abi::emit_load_from_address(emitter, scratch, frame, 8);
    emitter.instruction(&if arm { format!("cmp x9, #{}", tags.callable) } else { format!("cmp r9, {}", tags.callable) });        // callable descriptors have a dedicated exceptional release path
    emitter.instruction(if arm { "b.ne __rt_finfo_guard_heap" } else { "jne __rt_finfo_guard_heap" }); // use ordinary typed heap cleanup for other values
    abi::emit_symbol_address(emitter, callback, "__rt_exception_release_callable");
    abi::emit_store_to_address(emitter, callback, frame, 72);
    emitter.instruction(if arm { "b __rt_finfo_guard_owner" } else { "jmp __rt_finfo_guard_owner" }); // guard the returned callable and its captures
    emitter.label("__rt_finfo_guard_heap");
    emitter.instruction(&if arm { format!("cmp x9, #{}", tags.string) } else { format!("cmp r9, {}", tags.string) });          // returned strings also transfer an allocation owner
    emitter.instruction(if arm { "b.eq __rt_finfo_guard_owner" } else { "je __rt_finfo_guard_owner" }); // protect an owned string during record allocation
    emitter.instruction(&if arm { format!("cmp x9, #{}", tags.first_refcounted) } else { format!("cmp r9, {}", tags.first_refcounted) });          // scalar return registers must never be treated as heap pointers
    emitter.instruction(if arm { "b.lo __rt_finfo_guard_publish" } else { "jb __rt_finfo_guard_publish" }); // leave scalar guards empty
    emitter.instruction(&if arm { format!("cmp x9, #{}", tags.last_refcounted) } else { format!("cmp r9, {}", tags.last_refcounted) });          // arrays, objects and Mixed returns are refcounted owners
    emitter.instruction(if arm { "b.hi __rt_finfo_guard_publish" } else { "ja __rt_finfo_guard_publish" }); // raw resources and null carry no heap owner
    emitter.label("__rt_finfo_guard_owner");
    abi::emit_load_from_address(emitter, scratch, frame, 16);
    abi::emit_store_to_address(emitter, scratch, frame, 64 + guard_owner_offset);
    emitter.label("__rt_finfo_guard_publish");
    emitter.instruction(if arm { "add x0, sp, #64" } else { "lea rdi, [rsp + 64]" }); // point at the stack-local activation prefix
    emitter.instruction(if arm { "mov x1, #0" } else { "xor esi, esi" });       // publish this owner as a new unwind group
    emitter.instruction(if arm { "bl __rt_exception_guard_owned" } else { "call __rt_exception_guard_owned" }); // protect the return if allocation invokes throwing GC cleanup
    abi::emit_load_int_immediate(emitter, ret, 32);
    // -- move the owned result into a record and consume its temporary unwind guard --
    emitter.instruction(if arm { "bl __rt_heap_alloc" } else { "call __rt_heap_alloc" }); // allocate the opaque owner record using ordinary heap failure handling
    for (source, destination) in [(56, 0), (8, 8), (16, 16), (24, 24)] {
        abi::emit_load_from_address(emitter, scratch, frame, source);
        abi::emit_store_to_address(emitter, scratch, ret, destination);
    }
    abi::emit_store_to_address(emitter, ret, frame, 104);
    emitter.instruction(if arm { "add x0, sp, #64" } else { "lea rdi, [rsp + 64]" }); // consume exactly the guard whose owner now lives in the record
    emitter.instruction(if arm { "bl __rt_exception_unguard_owned" } else { "call __rt_exception_unguard_owned" }); // transfer ownership to the caller's PHP finally without releasing it
    abi::emit_load_from_address(emitter, ret, frame, 104);
}

/// Consumes the record before releasing its captured typed value.
pub fn emit_release(emitter: &mut impl Emitter, tags: OwnerTags) {
    let arm = emitter.arch() == Arch::AArch64;
    let (arg, ret, frame, scratch) = if arm { ("x0", "x0", "sp", "x9") } else { ("rdi", "rax", "rsp", "r9") };
    // -- retain the typed value while retiring its opaque record before callbacks --
    if arm {
        emitter.instruction("sub sp, sp, #48");                                 // reserve the transferred value and caller linkage
        emitter.instruction("stp x29, x30, [sp, #32]");                         // preserve the caller frame and return address
    } else {
        emitter.instruction("push rbp");                                        // preserve the SysV caller frame pointer
        emitter.instruction("mov rbp, rsp");                                    // establish the release frame pointer
        emitter.instruction("sub rsp, 32");                                     // retain the value across record release and destructor calls
    }
    for (source, destination) in [(8, 0), (16, 8), (24, 16)] {
        abi::emit_load_from_address(emitter, scratch, arg, source);
        abi::emit_store_to_address(emitter, scratch, frame, destination);
    }
    abi::emit_store_zero_to_address(emitter, arg, 16);
    if !arm { emitter.instruction("mov rax, rdi"); }                            // pass the consumed record through the native heap ABI
    emitter.instruction(if arm { "bl __rt_heap_free_safe" } else { "call __rt_heap_free_safe" }); // retire the record before any PHP destructor can reenter
    // -- dispatch the transferred owner to its declared representation's release path --
    abi::emit_load_from_address(emitter, scratch, frame, 0);
    emitter.instruction(&if arm { format!("cmp x9, #{}", tags.string) } else { format!("cmp r9, {}", tags.string) });          // raw strings use the validated allocation release helper
    emitter.instruction(if arm { "b.eq __rt_finfo_result_string" } else { "je __rt_finfo_result_string" }); // release the transferred string allocation
    emitter.instruction(&if arm { format!("cmp x9, #{}", tags.callable) } else { format!("cmp r9, {}", tags.callable) });        // callable captures require descriptor cleanup
    emitter.instruction(if arm { "b.eq __rt_finfo_result_callable" } else { "je __rt_finfo_result_callable" }); // preserve the callable ownership protocol
    emitter.instruction(&if arm { format!("cmp x9, #{}", tags.first_refcounted) } else { format!("cmp r9, {}", tags.first_refcounted) });          // lower tags are non-owning scalars
    emitter.instruction(if arm { "b.lo __rt_finfo_result_done" } else { "jb __rt_finfo_result_done" }); // scalar results carry no transferred heap owner
    emitter.instruction(&if arm { format!("cmp x9, #{}", tags.last_refcounted) } else { format!("cmp r9, {}", tags.last_refcounted) });          // arrays, objects and Mixed results are refcounted
    emitter.instruction(if arm { "b.hi __rt_finfo_result_done" } else { "ja __rt_finfo_result_done" }); // null and raw descriptors have no heap owner
    abi::emit_load_from_address(emitter, ret, frame, 8);
    emitter.instruction(if arm { "bl __rt_decref_any" } else { "call __rt_decref_any" }); // release the result under PHP's active finally exception state
    emitter.instruction(if arm { "b __rt_finfo_result_done" } else { "jmp __rt_finfo_result_done" }); // finish after the complete typed release
    emitter.label("__rt_finfo_result_string");
    abi::emit_load_from_address(emitter, ret, frame, 8);
    emitter.instruction(if arm { "bl __rt_heap_free_safe" } else { "call __rt_heap_free_safe" }); // release a returned string without touching static data
    emitter.instruction(if arm { "b __rt_finfo_result_done" } else { "jmp __rt_finfo_result_done" }); // retain the normal finally control flow
    emitter.label("__rt_finfo_result_callable");
    abi::emit_load_from_address(emitter, ret, frame, 8);
    emitter.instruction(if arm { "bl __rt_callable_descriptor_release" } else { "call __rt_callable_descriptor_release" }); // release captured callable owners through their descriptor
    emitter.label("__rt_finfo_result_done");
    // -- restore the caller after the complete typed result release --
    if arm {
        emitter.instruction("ldp x29, x30, [sp, #32]");                         // restore the caller linkage after result destruction
        emitter.instruction("add sp, sp, #48");                                 // release the complete aligned release frame
    } else {
        emitter.instruction("add rsp, 32");                                     // release the aligned transferred-value storage
        emitter.instruction("pop rbp");                                         // restore the SysV caller frame pointer
    }
    emitter.instruction("ret");                                                 // leave exception chaining to the enclosing PHP finally
}
