//! Purpose:
//! Emits target dispatch and shared helpers for structural JSON decoding into Mixed values.
//! Provides the runtime assembly used by JSON builtins on the selected target.
//!
//! Called from:
//! - `crate::codegen_support::runtime::system` during runtime emission.
//!
//! Key details:
//! - The module coordinates target-specific entry points and recursive object/array decoding helpers.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

mod aarch64;
mod arrays;
mod integer_prefix;
mod integers;
mod objects;
mod ownership;
mod whitespace;
mod x86_64;

#[cfg(test)]
mod ownership_tests;
#[cfg(test)]
mod integer_tests;

/// __rt_json_decode_mixed: checked structural JSON decoder that returns a boxed
/// Mixed cell pointer, or 0 after recording a JSON error. The outer boundary
/// defers JSON exceptions until recursive frames have released partial values.
///
/// Trims the source slice, uses the existing string-returning
/// `__rt_json_decode` helper for string unescaping, and classifies the result
/// based on the first non-whitespace byte of the original input:
///
///   * `"`            → Mixed(tag=1, ptr, len)
///   * `t`            → Mixed(tag=3, lo=1)         (true)
///   * `f`            → Mixed(tag=3, lo=0)         (false)
///   * `n`            → Mixed(tag=8)               (null)
///   * `-` / digit    → parse as int or float (presence of `.`/`e`/`E`
///                     toggles the float path); ints use `__rt_atoi`
///                     and floats use libc `atof` on a null-terminated
///                     stack copy of the decoded slice.
///   * `[` / `{`      → recursive structural decode via checked helpers under
///                     `arrays`/`objects`.
///   * empty input    → JSON_ERROR_SYNTAX and 0
///
/// Input ABI:
///   ARM64: x1 = json ptr, x2 = json len
///   x86_64: rax = json ptr, rdx = json len
/// Output: x0/rax = boxed Mixed pointer or 0 on error
pub(crate) fn emit_json_decode_mixed(emitter: &mut Emitter) {
    ownership::emit(emitter);
    integers::emit(emitter);
    if emitter.target.arch == Arch::X86_64 {
        whitespace::emit(emitter);
        x86_64::emit(emitter);
        arrays::emit_x86_64(emitter);
        objects::emit_x86_64(emitter);
        emit_box_owned_x86_64(emitter);
        return;
    }

    whitespace::emit(emitter);
    aarch64::emit(emitter);
    arrays::emit_aarch64(emitter);
    objects::emit_aarch64(emitter);
    emit_box_owned_aarch64(emitter);
}

/// __rt_json_box_owned: boxes a container the decoder just created, handing the box its ONLY
/// reference.
///
/// `__rt_mixed_from_value` retains a refcounted payload for the box, which is right for a payload
/// someone else still owns and wrong for one the decoder created a moment ago: every decoded
/// array, hash and stdClass kept its creation reference forever, so `json_decode('[1,2]', true)`
/// leaked the array and everything in it on every call. Same ABI as `__rt_mixed_from_value`
/// (ARM64: x0 tag, x1 payload, x2 high word; x86_64: rax tag, rdi payload, rsi high word);
/// returns the box in x0/rax.
fn emit_box_owned_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_box_owned ---");
    emitter.label_global("__rt_json_box_owned");
    emitter.instruction("sub sp, sp, #32");                                     // reserve slots for the container and its box
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame
    emitter.instruction("str x1, [sp, #0]");                                    // keep the container: the box retains its own reference
    emitter.instruction("bl __rt_mixed_from_value");                            // box the container (retains it for the box)
    emitter.instruction("str x0, [sp, #8]");                                    // keep the box across the release
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the container
    emitter.instruction("bl __rt_decref_any");                                  // drop the decoder's creation reference; the box owns it now
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the box
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the owning box
}

/// x86_64 form of [`emit_box_owned_aarch64`].
fn emit_box_owned_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_box_owned ---");
    emitter.label_global("__rt_json_box_owned");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 16");                                         // reserve slots for the container and its box
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // keep the container: the box retains its own reference
    emitter.instruction("call __rt_mixed_from_value");                          // box the container (retains it for the box)
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // keep the box across the release
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the container
    emitter.instruction("call __rt_decref_any");                                // drop the decoder's creation reference; the box owns it now
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the box
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the owning box
}
