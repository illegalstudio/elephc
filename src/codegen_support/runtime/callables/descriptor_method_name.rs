//! Purpose:
//! Emits `__rt_callable_descriptor_method_name`, which reads index 1 of a callable-array
//! descriptor: the bare method name PHP's `[$object, 'method']` array holds in its second slot.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::callables`.
//! - The `$mixed[1]` lowering in `crate::codegen::lower_inst::mixed_array_runtime`.
//!
//! Key details:
//! - A `callable` slot holds a descriptor, not the array the caller wrote, so the two-element
//!   array has to be reconstructed slot by slot. Index 0 (the bound receiver) already comes from
//!   the first runtime capture; this helper supplies the other half.
//! - The descriptor records a method as its QUALIFIED name (`"Foo::bar"`), while PHP's second
//!   slot holds only `"bar"`. The scan walks BACKWARDS to the last `:` because a method name
//!   cannot contain one, so the suffix after it is the bare name whatever the class name is.
//! - A name with no `:` is returned whole: that is the shape a plain function callable has, and
//!   it is already the bare name.

use crate::codegen_support::callable_descriptor::{
    CALLABLE_DESC_NAME_LEN_OFFSET, CALLABLE_DESC_NAME_OFFSET,
};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_callable_descriptor_method_name` runtime helper for the active target.
///
/// ## ARM64 ABI
/// - **Input**: `x0` = callable descriptor pointer
/// - **Output**: `x0` = an owned boxed `Mixed` string holding the bare method name
///
/// ## x86_64 ABI
/// - **Input**: `rax` = callable descriptor pointer
/// - **Output**: `rax` = the same owned boxed `Mixed` string
pub(crate) fn emit_callable_descriptor_method_name(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_callable_descriptor_method_name_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: callable descriptor method name ---");
    emitter.label_global("__rt_callable_descriptor_method_name");

    emitter.instruction("sub sp, sp, #16");                                     // reserve a minimal frame for the boxing call
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish a frame pointer for this helper
    emitter.instruction(&format!("ldr x1, [x0, #{}]", CALLABLE_DESC_NAME_OFFSET)); // x1 = descriptor PHP name pointer
    emitter.instruction(&format!(
        "ldr x2, [x0, #{}]",
        CALLABLE_DESC_NAME_LEN_OFFSET
    ));                                                                          // x2 = descriptor PHP name byte length
    emitter.instruction("cbz x1, __rt_callable_descriptor_method_name_empty");  // a descriptor without a name yields an empty string
    emitter.instruction("cbz x2, __rt_callable_descriptor_method_name_box");    // an empty name is already its own bare form
    emitter.instruction("mov x3, x2");                                          // start the backward scan just past the last byte

    emitter.label("__rt_callable_descriptor_method_name_scan");
    emitter.instruction("sub x3, x3, #1");                                      // step back one byte
    emitter.instruction("ldrb w4, [x1, x3]");                                   // load the candidate separator byte
    emitter.instruction("cmp w4, #58");                                         // is it ':' (the tail of a "Class::method" separator)?
    emitter.instruction("b.eq __rt_callable_descriptor_method_name_found");     // the bare method name starts right after it
    emitter.instruction("cbnz x3, __rt_callable_descriptor_method_name_scan");  // keep scanning while bytes remain
    emitter.instruction("b __rt_callable_descriptor_method_name_box");          // an unqualified name is already bare

    emitter.label("__rt_callable_descriptor_method_name_found");
    emitter.instruction("add x1, x1, x3");                                      // advance the pointer to the separator
    emitter.instruction("add x1, x1, #1");                                      // then past it, onto the bare method name
    emitter.instruction("sub x2, x2, x3");                                      // shorten the length by the qualified prefix
    emitter.instruction("sub x2, x2, #1");                                      // and by the separator byte itself
    emitter.instruction("b __rt_callable_descriptor_method_name_box");          // box the isolated method name

    emitter.label("__rt_callable_descriptor_method_name_empty");
    emitter.instruction("mov x1, x0");                                          // keep a valid non-null source pointer for the empty string
    emitter.instruction("mov x2, #0");                                          // a nameless descriptor stringifies to empty

    emitter.label("__rt_callable_descriptor_method_name_box");
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 identifies a string payload
    emitter.instruction("bl __rt_mixed_from_value");                            // persist and box the bare method name for the caller
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the local frame
    emitter.instruction("ret");                                                 // return the owned boxed string in x0
}

/// Emits the x86_64 variant of `__rt_callable_descriptor_method_name`.
fn emit_callable_descriptor_method_name_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: callable descriptor method name ---");
    emitter.label_global("__rt_callable_descriptor_method_name");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base and realign for the call
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {}]",
        CALLABLE_DESC_NAME_OFFSET
    ));                                                                          // r10 = descriptor PHP name pointer
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [rax + {}]",
        CALLABLE_DESC_NAME_LEN_OFFSET
    ));                                                                          // r11 = descriptor PHP name byte length
    emitter.instruction("test r10, r10");                                       // does the descriptor carry a name at all?
    emitter.instruction("jz __rt_callable_descriptor_method_name_empty");       // a descriptor without a name yields an empty string
    emitter.instruction("test r11, r11");                                       // is the recorded name empty?
    emitter.instruction("jz __rt_callable_descriptor_method_name_box");         // an empty name is already its own bare form
    emitter.instruction("mov r8, r11");                                         // start the backward scan just past the last byte

    emitter.label("__rt_callable_descriptor_method_name_scan");
    emitter.instruction("sub r8, 1");                                           // step back one byte
    emitter.instruction("cmp BYTE PTR [r10 + r8], 58");                         // is it ':' (the tail of a "Class::method" separator)?
    emitter.instruction("je __rt_callable_descriptor_method_name_found");       // the bare method name starts right after it
    emitter.instruction("test r8, r8");                                         // are there bytes left to scan?
    emitter.instruction("jnz __rt_callable_descriptor_method_name_scan");       // keep walking backwards
    emitter.instruction("jmp __rt_callable_descriptor_method_name_box");        // an unqualified name is already bare

    emitter.label("__rt_callable_descriptor_method_name_found");
    emitter.instruction("lea r10, [r10 + r8 + 1]");                             // point past the separator, onto the bare method name
    emitter.instruction("sub r11, r8");                                         // shorten the length by the qualified prefix
    emitter.instruction("sub r11, 1");                                          // and by the separator byte itself
    emitter.instruction("jmp __rt_callable_descriptor_method_name_box");        // box the isolated method name

    emitter.label("__rt_callable_descriptor_method_name_empty");
    emitter.instruction("mov r10, rax");                                        // keep a valid non-null source pointer for the empty string
    emitter.instruction("xor r11d, r11d");                                      // a nameless descriptor stringifies to empty

    emitter.label("__rt_callable_descriptor_method_name_box");
    emitter.instruction("mov rdi, r10");                                        // pass the bare method name pointer to the boxing helper
    emitter.instruction("mov rsi, r11");                                        // pass its byte length
    emitter.instruction("mov rax, 1");                                          // runtime tag 1 identifies a string payload
    emitter.instruction("call __rt_mixed_from_value");                          // persist and box the bare method name for the caller
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the owned boxed string in rax
}
