//! Purpose:
//! Resolves generation-safe scalar Buffer handles to their static descriptor.
//! The helpers centralize stale-alias validation for every buffer operation.
//!
//! Called from:
//! - `__rt_buffer_len`, `__rt_buffer_free`, and EIR buffer get/set lowering.
//! - EIR `buffer_resolve` lowering, for the non-faulting variant.
//!
//! Key details:
//! - A valid handle is `(generation:u32 << 32) | index:u32`, with index 1..=4096.
//! - The descriptor must be active and carry exactly the supplied non-zero generation.
//! - `__rt_buffer_try_resolve` applies the same checks but answers 0 instead of failing, so a
//!   loop preheader can resolve a handle ahead of the access that reports the use-after-free.

use crate::abi;
use crate::Emitter;
use crate::Arch;

use super::{BUFFER_DESCRIPTOR_SIZE, BUFFER_REGISTRY_CAPACITY};

/// Emits `__rt_buffer_resolve` for the active target.
///
/// Input and output use `x0` on ARM64 and `rax` on x86_64. The input is a scalar
/// generation/index handle; the output is the validated static descriptor address.
/// Invalid, inactive, stale, or null handles never return and use the common
/// use-after-free diagnostic.
pub fn emit_buffer_resolve(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_buffer_resolve_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: buffer_resolve ---");
    emitter.label_global("__rt_buffer_resolve");
    emitter.instruction("cbz x0, __rt_buffer_resolve_invalid");                 // handle zero is the invalid/null sentinel
    emitter.instruction("mov w9, w0");                                          // low u32 = descriptor index
    emitter.instruction("lsr x10, x0, #32");                                    // high u32 = requested generation
    emitter.instruction("cbz x10, __rt_buffer_resolve_invalid");                // generation zero is never issued
    emitter.instruction("cbz x9, __rt_buffer_resolve_invalid");                 // descriptor index zero is reserved
    emitter.instruction(&format!("mov x11, #{}", BUFFER_REGISTRY_CAPACITY));    // highest usable descriptor index
    emitter.instruction("cmp x9, x11");                                         // stay inside the static registry
    emitter.instruction("b.hi __rt_buffer_resolve_invalid");                    // out-of-range handles cannot resolve
    abi::emit_symbol_address(emitter, "x11", "_buffer_registry");
    emitter.instruction(&format!("mov x12, #{}", BUFFER_DESCRIPTOR_SIZE));      // descriptor byte stride
    emitter.instruction("madd x11, x9, x12, x11");                              // descriptor = registry + index * stride
    emitter.instruction("ldr x12, [x11, #32]");                                 // active marker
    emitter.instruction("cmp x12, #1");                                         // only live descriptors may be observed
    emitter.instruction("b.ne __rt_buffer_resolve_invalid");                    // aliases of freed slots fail closed
    emitter.instruction("ldr x12, [x11, #24]");                                 // descriptor generation
    emitter.instruction("cmp x12, x10");                                        // reject a handle from an earlier slot lifetime
    emitter.instruction("b.ne __rt_buffer_resolve_invalid");                    // stale aliases never revive after reuse
    emitter.instruction("mov x0, x11");                                         // return descriptor address
    emitter.instruction("ret");                                                 // hand the validated descriptor back to the buffer operation
    emitter.label("__rt_buffer_resolve_invalid");
    emitter.instruction("b __rt_buffer_use_after_free");                        // one cross-atom branch after local conditional checks
}

/// Emits the Linux x86_64 generation-safe Buffer handle resolver.
fn emit_buffer_resolve_linux_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: buffer_resolve ---");
    emitter.label_global("__rt_buffer_resolve");
    emitter.instruction("test rax, rax");                                       // handle zero is the invalid/null sentinel
    emitter.instruction("jz __rt_buffer_resolve_invalid_x");                    // null locals and invalid aliases fail closed
    emitter.instruction("mov r10d, eax");                                       // low u32 = descriptor index
    emitter.instruction("shr rax, 32");                                         // high u32 = requested generation
    emitter.instruction("test rax, rax");                                       // generation zero is never issued
    emitter.instruction("jz __rt_buffer_resolve_invalid_x");                    // reject an uninitialized/stale generation
    emitter.instruction("test r10, r10");                                       // descriptor index zero is reserved
    emitter.instruction("jz __rt_buffer_resolve_invalid_x");                    // reject zero index
    emitter.instruction(&format!("cmp r10, {}", BUFFER_REGISTRY_CAPACITY));     // highest usable descriptor index
    emitter.instruction("ja __rt_buffer_resolve_invalid_x");                    // out-of-range handles cannot resolve
    abi::emit_symbol_address(emitter, "r11", "_buffer_registry");
    emitter.instruction(&format!("imul r10, {}", BUFFER_DESCRIPTOR_SIZE));      // descriptor byte offset
    emitter.instruction("add r11, r10");                                        // descriptor = registry + index * stride
    emitter.instruction("cmp QWORD PTR [r11 + 32], 1");                         // active marker
    emitter.instruction("jne __rt_buffer_resolve_invalid_x");                   // aliases of freed slots fail closed
    emitter.instruction("cmp QWORD PTR [r11 + 24], rax");                       // descriptor generation
    emitter.instruction("jne __rt_buffer_resolve_invalid_x");                   // stale aliases never revive after reuse
    emitter.instruction("mov rax, r11");                                        // return descriptor address
    emitter.instruction("ret");                                                 // hand the validated descriptor back to the buffer operation
    emitter.label("__rt_buffer_resolve_invalid_x");
    emitter.instruction("jmp __rt_buffer_use_after_free");                      // one shared fatal branch after local conditional checks
}

/// Emits `__rt_buffer_try_resolve` for the active target.
///
/// Same register contract and validation as `__rt_buffer_resolve`, but an invalid, inactive,
/// stale, or null handle returns 0 instead of reaching the use-after-free diagnostic. The
/// caller owns the fault: `buffer_get_resolved` and `buffer_set_resolved` test the descriptor
/// at the access site, so the diagnostic still fires exactly where the unresolved access would
/// have raised it.
pub fn emit_buffer_try_resolve(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_buffer_try_resolve_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: buffer_try_resolve ---");
    emitter.label_global("__rt_buffer_try_resolve");
    emitter.instruction("cbz x0, __rt_buffer_try_resolve_invalid");             // handle zero is the invalid/null sentinel
    emitter.instruction("mov w9, w0");                                          // low u32 = descriptor index
    emitter.instruction("lsr x10, x0, #32");                                    // high u32 = requested generation
    emitter.instruction("cbz x10, __rt_buffer_try_resolve_invalid");            // generation zero is never issued
    emitter.instruction("cbz x9, __rt_buffer_try_resolve_invalid");             // descriptor index zero is reserved
    emitter.instruction(&format!("mov x11, #{}", BUFFER_REGISTRY_CAPACITY));    // highest usable descriptor index
    emitter.instruction("cmp x9, x11");                                         // stay inside the static registry
    emitter.instruction("b.hi __rt_buffer_try_resolve_invalid");                // out-of-range handles cannot resolve
    abi::emit_symbol_address(emitter, "x11", "_buffer_registry");
    emitter.instruction(&format!("mov x12, #{}", BUFFER_DESCRIPTOR_SIZE));      // descriptor byte stride
    emitter.instruction("madd x11, x9, x12, x11");                              // descriptor = registry + index * stride
    emitter.instruction("ldr x12, [x11, #32]");                                 // active marker
    emitter.instruction("cmp x12, #1");                                         // only live descriptors may be observed
    emitter.instruction("b.ne __rt_buffer_try_resolve_invalid");                // aliases of freed slots resolve to nothing
    emitter.instruction("ldr x12, [x11, #24]");                                 // descriptor generation
    emitter.instruction("cmp x12, x10");                                        // reject a handle from an earlier slot lifetime
    emitter.instruction("b.ne __rt_buffer_try_resolve_invalid");                // stale aliases never revive after reuse
    emitter.instruction("mov x0, x11");                                         // return descriptor address
    emitter.instruction("ret");                                                 // hand the validated descriptor back to the preheader
    emitter.label("__rt_buffer_try_resolve_invalid");
    emitter.instruction("mov x0, #0");                                          // answer the null descriptor and let the access report the fault
    emitter.instruction("ret");                                                 // return without diagnosing: the resolution may be speculative
}

/// Emits the Linux x86_64 non-faulting Buffer handle resolver.
fn emit_buffer_try_resolve_linux_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: buffer_try_resolve ---");
    emitter.label_global("__rt_buffer_try_resolve");
    emitter.instruction("test rax, rax");                                       // handle zero is the invalid/null sentinel
    emitter.instruction("jz __rt_buffer_try_resolve_invalid_x");                // null locals and invalid aliases resolve to nothing
    emitter.instruction("mov r10d, eax");                                       // low u32 = descriptor index
    emitter.instruction("shr rax, 32");                                         // high u32 = requested generation
    emitter.instruction("test rax, rax");                                       // generation zero is never issued
    emitter.instruction("jz __rt_buffer_try_resolve_invalid_x");                // reject an uninitialized/stale generation
    emitter.instruction("test r10, r10");                                       // descriptor index zero is reserved
    emitter.instruction("jz __rt_buffer_try_resolve_invalid_x");                // reject zero index
    emitter.instruction(&format!("cmp r10, {}", BUFFER_REGISTRY_CAPACITY));     // highest usable descriptor index
    emitter.instruction("ja __rt_buffer_try_resolve_invalid_x");                // out-of-range handles cannot resolve
    abi::emit_symbol_address(emitter, "r11", "_buffer_registry");
    emitter.instruction(&format!("imul r10, {}", BUFFER_DESCRIPTOR_SIZE));      // descriptor byte offset
    emitter.instruction("add r11, r10");                                        // descriptor = registry + index * stride
    emitter.instruction("cmp QWORD PTR [r11 + 32], 1");                         // active marker
    emitter.instruction("jne __rt_buffer_try_resolve_invalid_x");               // aliases of freed slots resolve to nothing
    emitter.instruction("cmp QWORD PTR [r11 + 24], rax");                       // descriptor generation
    emitter.instruction("jne __rt_buffer_try_resolve_invalid_x");               // stale aliases never revive after reuse
    emitter.instruction("mov rax, r11");                                        // return descriptor address
    emitter.instruction("ret");                                                 // hand the validated descriptor back to the preheader
    emitter.label("__rt_buffer_try_resolve_invalid_x");
    emitter.instruction("xor eax, eax");                                        // answer the null descriptor and let the access report the fault
    emitter.instruction("ret");                                                 // return without diagnosing: the resolution may be speculative
}

/// Computes an element address from a resolved descriptor and the index in x10 or r10.
/// The optional labels select the stale-descriptor trap and an unproven index check.
pub fn emit_element_address(
    emitter: &mut impl crate::Emitter, descriptor: &str, live: Option<&str>, bounds: Option<&str>,
) -> &'static str {
    let arm = emitter.arch() == Arch::AArch64;
    let hoisted = live.is_some();
    let index = if arm { "x10" } else { "r10" };
    let scratch = if arm && hoisted { "x9" } else if arm { "x12" } else { "rcx" };
    if let Some(label) = live {
        if arm {
            emitter.instruction(&format!("cbnz {descriptor}, {label}"));      // zero means the hoisted handle was stale
        } else {
            emitter.instruction(&format!("test {descriptor}, {descriptor}")); // reject an unresolved hoisted descriptor
            emitter.instruction(&format!("jnz {label}"));                     // continue only for a live descriptor
        }
        emitter.emit_jump("__rt_buffer_use_after_free");
        emitter.label(label);
    }
    if let Some(label) = bounds {
        if !arm && hoisted {
            emitter.instruction(&format!("cmp {index}, QWORD PTR [{descriptor} + 8]")); // compare against the descriptor's logical length
        } else {
            emitter.emit_load_from_address(scratch, descriptor, 8);
            emitter.instruction(&format!("cmp {index}, {scratch}"));          // unsigned comparison also rejects negative indices
        }
        emitter.instruction(&format!("{} {label}", if arm { "b.lo" } else { "jb" })); // proceed on an in-bounds index
        emitter.emit_jump("__rt_buffer_bounds_fail");
        emitter.label(label);
    }
    if arm {
        if hoisted {
            emitter.emit_load_from_address(scratch, descriptor, 16);
            emitter.emit_load_from_address("x11", descriptor, 0);
        } else {
            emitter.emit_load_from_address("x11", descriptor, 0);
            emitter.emit_load_from_address(scratch, descriptor, 16);
        }
        emitter.instruction(&format!("madd x11, x10, {scratch}, x11"));        // compute base plus index times stride
        "x11"
    } else if hoisted {
        emitter.instruction(&format!("imul r10, QWORD PTR [{descriptor} + 16]")); // scale the checked index
        emitter.instruction(&format!("add r10, QWORD PTR [{descriptor}]"));    // add the payload base without overwriting it
        "r10"
    } else {
        emitter.emit_load_from_address("r11", descriptor, 0);
        emitter.emit_load_from_address(scratch, descriptor, 16);
        emitter.instruction("imul r10, rcx");                                 // scale the checked index
        emitter.instruction("add r11, r10");                                  // add the scaled index to the payload base
        "r11"
    }
}

/// Loads the payload base from a descriptor returned by the checked resolver.
pub fn emit_load_payload(emitter: &mut impl crate::Emitter, result: &str, descriptor: &str) {
    emitter.emit_load_from_address(result, descriptor, 0);
}
