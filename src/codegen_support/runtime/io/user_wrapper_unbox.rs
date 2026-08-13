//! Purpose:
//! Emits `__rt_wrapper_unbox_int`, the scalar counterpart of the boxed-result
//! conversion the userspace stream-wrapper helpers already perform for strings.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - Every wrapper helper whose vtable slot returns a raw integer or boolean,
//!   on the branch its per-class boxed-result mask bit selects.
//!
//! Key details:
//! - A wrapper method with NO declared return type has codegen representation
//!   `Mixed`, so it hands back a boxed cell where the helper reads a raw
//!   scalar. `stream_tell()` written the ordinary way — `public function
//!   stream_tell() { return $this->pos; }` — therefore made `ftell()` answer a
//!   pointer (measured: 4329450168 where PHP answers 5). The undeclared form is
//!   the one real wrapper code uses, so the broken case is the common one.
//! - The box is released with `__rt_decref_any`, NOT `__rt_mixed_free_deep`:
//!   `return $this->pos;` hands back the property's own cell rather than a copy,
//!   so freeing it outright destroys the property. Measured — with `free_deep`,
//!   `ftell()` answered the right value once and 0 for every later call, and
//!   `feof()` then never became true, hanging a read loop.
//! - A null box reads as 0, which is what every consuming helper treats as
//!   false/zero.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits `__rt_wrapper_unbox_int(box) -> int`.
///
/// Takes the boxed Mixed a wrapper method returned, converts it to an integer
/// through `__rt_mixed_cast_int` (which maps false/true to 0/1), releases the
/// box, and returns the integer. Dispatches by target.
pub fn emit_wrapper_unbox_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_wrapper_unbox_int_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: wrapper_unbox_int ---");
    emitter.label_global("__rt_wrapper_unbox_int");

    // Frame: [sp,#0..16] x29/x30, [sp,#16] the boxed cell, [sp,#24] the scalar.
    emitter.instruction("sub sp, sp, #32");                                     // helper frame for the cast and release
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("cbz x0, __rt_wui_null");                               // a null box carries nothing to unbox or release
    emitter.instruction("str x0, [sp, #16]");                                   // keep the box for release after the cast
    emitter.instruction("bl __rt_mixed_cast_int");                              // x0 = the scalar the wrapper meant to return
    emitter.instruction("str x0, [sp, #24]");                                   // stash the scalar across the box release
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the boxed cell
    emitter.instruction("bl __rt_decref_any");                                  // release our reference; a property's own cell survives
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the scalar result
    emitter.instruction("b __rt_wui_ret");                                      // share the common epilogue

    emitter.label("__rt_wui_null");
    emitter.instruction("mov x0, #0");                                          // a null box reads as 0, i.e. false / zero bytes

    emitter.label("__rt_wui_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the scalar
}

/// x86_64 implementation of `__rt_wrapper_unbox_int`.
///
/// Input: rax = boxed Mixed. Output: rax = the scalar.
fn emit_wrapper_unbox_int_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: wrapper_unbox_int ---");
    emitter.label_global("__rt_wrapper_unbox_int");

    // Frame: [rbp-8] the boxed cell, [rbp-16] the scalar. push rbp then
    //   sub rsp,32 keeps rsp 16-aligned for the nested helper calls.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // spill slots for the box and the scalar
    emitter.instruction("test rax, rax");                                       // is the box null?
    emitter.instruction("jz __rt_wui_null_x86");                                // a null box carries nothing to unbox or release
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // keep the box for release after the cast
    emitter.instruction("call __rt_mixed_cast_int");                            // rax = the scalar the wrapper meant to return
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // stash the scalar across the box release
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the boxed cell
    emitter.instruction("call __rt_decref_any");                                // release our reference; a property's own cell survives
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the scalar result
    emitter.instruction("jmp __rt_wui_ret_x86");                                // share the common epilogue

    emitter.label("__rt_wui_null_x86");
    emitter.instruction("xor eax, eax");                                        // a null box reads as 0, i.e. false / zero bytes

    emitter.label("__rt_wui_ret_x86");
    emitter.instruction("add rsp, 32");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the scalar
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};
    use crate::codegen_support::runtime::emit_runtime;
    use crate::codegen_support::RuntimeFeatures;

    /// Every helper that reads a wrapper result as a raw scalar must route through the
    /// conversion, on BOTH architectures.
    ///
    /// Two of those families have no behavioural reproducer: no wrapper body written so
    /// far makes `stream_open` or the path ops infer a `Mixed` return, so their mask bit
    /// is never set and removing their conversion changes no observable output. That is
    /// exactly why they need a structural check — a behavioural test cannot tell whether
    /// their wiring is present or has rotted away.
    #[test]
    fn every_scalar_reading_helper_routes_through_the_unbox_conversion() {
        for (platform, arch) in [
            (Platform::MacOS, Arch::AArch64),
            (Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(Target::new(platform, arch));
            emitter.emit_text_prelude();
            emit_runtime(&mut emitter, RuntimeFeatures::all());
            let asm = emitter.output();
            let call = if arch == Arch::X86_64 {
                "call __rt_wrapper_unbox_int"
            } else {
                "bl __rt_wrapper_unbox_int"
            };
            let sites = asm.matches(call).count();
            assert!(
                sites >= 11,
                "{arch:?}: only {sites} call sites convert a boxed scalar result; the seven \
                 stream slots plus stream_open, dir_opendir, the path ops and rename should \
                 all be wired"
            );
            assert!(
                asm.contains("__rt_wrapper_unbox_int:"),
                "{arch:?}: the conversion helper itself must be emitted"
            );
        }
    }
}
