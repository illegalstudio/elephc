//! Purpose:
//! Emits property-store ownership completion and decoded-box reuse protocols.
//!
//! Called from:
//! - The compiler hydration-storage adapter through its explicit owner context.
//!
//! Key details:
//! - Copied strings transfer their owner while stored containers acquire a retain.
//! - Replaced boxes preserve registry back-references through the host scope.
//! - The host supplies its uninitialized marker and ownership callback entries.

use crate::{abi, Arch, AssemblyEmitter, TargetAbi};

/// Compiler facts that configure emission without importing parser or PHP type policy.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    /// Host marker in either word of an uninitialized property or reference slot.
    pub uninitialized: i64,
    /// Runtime entry consuming a box owner into the active host scope.
    pub defer_data: &'a str,
    /// Host symbol recording whether a decoder will drain the deferred owners.
    pub active_depth: &'a str,
    /// Runtime entry releasing the saved tagged payload after its old box is deferred.
    pub release_payload: &'a str,
}

/// Emits the target-specific post-store ownership finisher.
pub fn emit<E: AssemblyEmitter + TargetAbi>(
    emitter: &mut E, context: Context<'_>,
) {
    match emitter.arch() {
        Arch::AArch64 => emit_aarch64(emitter, context),
        Arch::X86_64 => emit_x86_64(emitter, context),
    }
    match emitter.arch() {
        Arch::AArch64 => emit_release_previous_aarch64(emitter, context),
        Arch::X86_64 => emit_release_previous_x86_64(emitter, context),
    }
    match emitter.arch() {
        Arch::AArch64 => emit_reuse_box_aarch64(emitter, context),
        Arch::X86_64 => emit_reuse_box_x86_64(emitter, context),
    }
}

/// Emits the AArch64 post-store ownership finisher.
///
/// Input: x0 = parsed value box. The property slot already holds its copy or payload pointer.
fn emit_aarch64<E: AssemblyEmitter + TargetAbi>(
    emitter: &mut E, context: Context<'_>,
) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize stored-property ownership ---");
    emitter.label_global("__rt_unser_finish_stored_prop");
    emitter.instruction("cbz x0, __rt_unser_finish_stored_prop_done");          // a missing box needs no cleanup
    emitter.instruction("ldr x9, [x0]");                                        // boxed value tag
    emitter.instruction("cmp x9, #1");                                          // is the stored payload an owned string?
    emitter.instruction("b.eq __rt_unser_finish_stored_prop_string");           // the slot now owns the persisted string
    emitter.instruction("cmp x9, #4");                                          // is the value below the refcounted container range?
    emitter.instruction("b.lo __rt_unser_finish_stored_prop_scalar");           // scalars copy inline and own no heap payload
    emitter.instruction("cmp x9, #6");                                          // does the value stay inside the container range?
    emitter.instruction("b.hi __rt_unser_finish_stored_prop_scalar");           // other wire values transfer no container owner
    emitter.instruction("ldr x9, [x0, #8]");                                    // container payload stored in the property slot
    emitter.instruction("stp x0, x30, [sp, #-16]!");                            // preserve the parsed box across the payload retain
    emitter.instruction("mov x0, x9");                                          // pass the property's payload to the retain helper
    emitter.instruction("bl __rt_incref");                                      // give the property slot its own container owner
    emitter.instruction("ldp x0, x30, [sp], #16");                              // restore the parsed box after the retain
    abi::emit_jump(emitter, context.defer_data);                     // keep the registry box alive until the context completes
    emitter.label("__rt_unser_finish_stored_prop_string");
    abi::emit_jump(emitter, "__rt_heap_free");                                  // free the shell; the property slot owns the string payload
    emitter.label("__rt_unser_finish_stored_prop_scalar");
    abi::emit_jump(emitter, "__rt_decref_mixed");                               // release the scalar box through the counted path
    emitter.label("__rt_unser_finish_stored_prop_done");
    abi::emit_return(emitter);
}

/// Emits the x86_64 post-store ownership finisher.
///
/// Input: rax = parsed value box. The property slot already holds its copy or payload pointer.
fn emit_x86_64<E: AssemblyEmitter + TargetAbi>(
    emitter: &mut E, context: Context<'_>,
) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize stored-property ownership ---");
    emitter.label_global("__rt_unser_finish_stored_prop");
    emitter.instruction("test rax, rax");                                       // a missing box needs no cleanup
    emitter.instruction("jz __rt_unser_finish_stored_prop_done_x");             // nothing to release
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // boxed value tag
    emitter.instruction("cmp r10, 1");                                          // is the stored payload an owned string?
    emitter.instruction("je __rt_unser_finish_stored_prop_string_x");           // the slot now owns the persisted string
    emitter.instruction("cmp r10, 4");                                          // is the value below the refcounted container range?
    emitter.instruction("jb __rt_unser_finish_stored_prop_scalar_x");           // scalars copy inline and own no heap payload
    emitter.instruction("cmp r10, 6");                                          // does the value stay inside the container range?
    emitter.instruction("ja __rt_unser_finish_stored_prop_scalar_x");           // other wire values transfer no container owner
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // container payload stored in the property slot
    emitter.instruction("push rax");                                            // preserve the parsed box across the payload retain
    emitter.instruction("mov rax, rdi");                                        // __rt_incref consumes its argument in rax
    emitter.instruction("call __rt_incref");                                    // give the property slot its own container owner
    emitter.instruction("pop rax");                                             // restore the parsed box for the context deferral
    abi::emit_jump(emitter, context.defer_data);                     // keep the registry box alive until the context completes
    emitter.label("__rt_unser_finish_stored_prop_string_x");
    abi::emit_jump(emitter, "__rt_heap_free");                                  // free the shell; the property slot owns the string payload
    emitter.label("__rt_unser_finish_stored_prop_scalar_x");
    abi::emit_jump(emitter, "__rt_decref_mixed");                               // release the scalar box through the counted path
    emitter.label("__rt_unser_finish_stored_prop_done_x");
    abi::emit_return(emitter);
}

/// Emits the AArch64 release of the property value a hydration store overwrites.
///
/// Input: x0=slot address, x1=compile-time property tag. Uninitialized typed slots are skipped.
fn emit_release_previous_aarch64<E: AssemblyEmitter + TargetAbi>(
    emitter: &mut E, context: Context<'_>,
) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize previous-property release ---");
    emitter.label_global("__rt_unser_release_prev_prop");
    emitter.instruction("cbz x0, __rt_unser_release_prev_prop_done");           // a missing slot owns no previous value
    emitter.instruction("ldr x9, [x0, #8]");                                    // slot metadata word (string length or typed marker)
    abi::emit_load_int_immediate(emitter, "x10", context.uninitialized);
    emitter.instruction("cmp x9, x10");                                         // is this typed property still uninitialized?
    emitter.instruction("b.eq __rt_unser_release_prev_prop_done");              // no previous owner exists for an uninitialized slot
    emitter.instruction("ldr x9, [x0]");                                        // previous payload low word
    emitter.instruction("cmp x9, x10");                                         // native reference cells put their uninitialized marker in the low word
    emitter.instruction("b.eq __rt_unser_release_prev_prop_done");              // a fresh reference cell has no previous owner
    emitter.instruction("cmp x1, #1");                                          // is the previous value an owned string?
    emitter.instruction("b.eq __rt_unser_release_prev_prop_any");               // strings release through the uniform dispatcher
    emitter.instruction("cmp x1, #4");                                          // is the previous value below the container range?
    emitter.instruction("b.lo __rt_unser_release_prev_prop_done");              // scalar slots own no heap payload
    emitter.instruction("cmp x1, #6");                                          // does the previous value stay inside the container range?
    emitter.instruction("b.ls __rt_unser_release_prev_prop_any");               // arrays, hashes, and objects release uniformly
    emitter.instruction("cmp x1, #7");                                          // is the previous value a boxed Mixed cell?
    emitter.instruction("b.eq __rt_unser_release_prev_prop_mixed");             // Mixed cells stay alive for their reference slot
    emitter.instruction("cmp x1, #10");                                         // is the previous value a callable descriptor?
    emitter.instruction("b.eq __rt_unser_release_prev_prop_callable");          // callable descriptors own capture state
    emitter.instruction("cmp x1, #11");                                         // is the previous value an owned reference cell?
    emitter.instruction("b.eq __rt_unser_release_prev_prop_any");               // reference cells release through uniform dispatch
    emitter.instruction("b __rt_unser_release_prev_prop_done");                 // other tags hold no release obligation here
    emitter.label("__rt_unser_release_prev_prop_any");
    emitter.instruction("mov x0, x9");                                          // pass the previous payload to the uniform release helper
    abi::emit_jump(emitter, "__rt_decref_any");
    emitter.label("__rt_unser_release_prev_prop_mixed");
    abi::emit_return(emitter);                                                  // the Mixed store reuses real boxes and skips sentinels
    emitter.label("__rt_unser_release_prev_prop_callable");
    emitter.instruction("mov x0, x9");                                          // pass the previous callable descriptor to its release helper
    abi::emit_jump(emitter, "__rt_callable_descriptor_release");
    emitter.label("__rt_unser_release_prev_prop_done");
    abi::emit_return(emitter);
}

/// Emits the x86_64 release of the property value a hydration store overwrites.
///
/// Input: rdi=slot address, rsi=compile-time property tag. Uninitialized typed slots are skipped.
fn emit_release_previous_x86_64<E: AssemblyEmitter + TargetAbi>(
    emitter: &mut E, context: Context<'_>,
) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize previous-property release ---");
    emitter.label_global("__rt_unser_release_prev_prop");
    emitter.instruction("test rdi, rdi");                                       // a missing slot owns no previous value
    emitter.instruction("jz __rt_unser_release_prev_prop_done_x");              // nothing to release
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // slot metadata word (string length or typed marker)
    abi::emit_load_int_immediate(emitter, "r11", context.uninitialized);
    emitter.instruction("cmp r10, r11");                                        // is this typed property still uninitialized?
    emitter.instruction("je __rt_unser_release_prev_prop_done_x");              // no previous owner exists for an uninitialized slot
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // previous payload low word
    emitter.instruction("cmp r10, r11");                                        // native reference cells put their uninitialized marker in the low word
    emitter.instruction("je __rt_unser_release_prev_prop_done_x");              // a fresh reference cell has no previous owner
    emitter.instruction("cmp rsi, 1");                                          // is the previous value an owned string?
    emitter.instruction("je __rt_unser_release_prev_prop_any_x");               // strings release through the uniform dispatcher
    emitter.instruction("cmp rsi, 4");                                          // is the previous value below the container range?
    emitter.instruction("jb __rt_unser_release_prev_prop_done_x");              // scalar slots own no heap payload
    emitter.instruction("cmp rsi, 6");                                          // does the previous value stay inside the container range?
    emitter.instruction("jbe __rt_unser_release_prev_prop_any_x");              // arrays, hashes, and objects release uniformly
    emitter.instruction("cmp rsi, 7");                                          // is the previous value a boxed Mixed cell?
    emitter.instruction("je __rt_unser_release_prev_prop_mixed_x");             // Mixed cells stay alive for their reference slot
    emitter.instruction("cmp rsi, 10");                                         // is the previous value a callable descriptor?
    emitter.instruction("je __rt_unser_release_prev_prop_callable_x");          // callable descriptors own capture state
    emitter.instruction("cmp rsi, 11");                                         // is the previous value an owned reference cell?
    emitter.instruction("je __rt_unser_release_prev_prop_any_x");               // reference cells release through uniform dispatch
    emitter.instruction("jmp __rt_unser_release_prev_prop_done_x");             // other tags hold no release obligation here
    emitter.label("__rt_unser_release_prev_prop_any_x");
    emitter.instruction("mov rax, r10");                                        // pass the previous payload to the uniform release helper
    abi::emit_jump(emitter, "__rt_decref_any");
    emitter.label("__rt_unser_release_prev_prop_mixed_x");
    abi::emit_return(emitter);                                                  // the Mixed store reuses real boxes and skips sentinels
    emitter.label("__rt_unser_release_prev_prop_callable_x");
    emitter.instruction("mov rax, r10");                                        // pass the previous callable descriptor to its release helper
    abi::emit_jump(emitter, "__rt_callable_descriptor_release");
    emitter.label("__rt_unser_release_prev_prop_done_x");
    abi::emit_return(emitter);
}

/// Emits the AArch64 in-place Mixed box reuse used by duplicate hash keys and properties.
///
/// Input: x0=destination box, x1=parsed replacement box. Output: x0=destination box.
fn emit_reuse_box_aarch64<E: AssemblyEmitter + TargetAbi>(
    emitter: &mut E, context: Context<'_>,
) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize in-place box reuse ---");
    emitter.label_global("__rt_unser_reuse_box");
    abi::emit_frame_prologue(emitter, 96);
    abi::store_at_offset(emitter, "x0", 40);
    abi::store_at_offset(emitter, "x1", 32);
    emit_replacement_aarch64(emitter, context, ReuseSite::PropertySlot);
    abi::load_at_offset(emitter, "x0", 40);
    abi::emit_frame_restore(emitter, 96);
    abi::emit_return(emitter);
}

/// Emits AArch64 replacement with the old and new boxes in frame slots 40 and 32.
fn emit_replacement_aarch64(
    emitter: &mut (impl AssemblyEmitter + TargetAbi), context: Context<'_>, site: ReuseSite,
) {
    let prefix = site.label_prefix();
    abi::load_at_offset(emitter, "x9", 40);
    site.arm_instruction(emitter, "ldr x10, [x9]", "previous value tag");
    abi::store_at_offset(emitter, "x10", 48);
    site.arm_instruction(emitter, "ldr x10, [x9, #8]", "previous payload low word");
    abi::store_at_offset(emitter, "x10", 56);
    site.arm_instruction(emitter, "ldr x10, [x9, #16]", "previous payload high word");
    abi::store_at_offset(emitter, "x10", 64);
    abi::load_at_offset(emitter, "x0", 48);
    abi::load_at_offset(emitter, "x1", 56);
    abi::load_at_offset(emitter, "x2", 64);
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::store_at_offset(emitter, "x0", 72);
    if matches!(site, ReuseSite::PropertySlot) {
        // Direct hydration keeps this lease until the replacement is visible.
        abi::emit_load_symbol_to_reg(emitter, "x9", context.active_depth, 0);
        emitter.instruction("cbnz x9, __rt_unser_reuse_defer_old");             // an active decoder owns the deferred copy
        emitter.instruction("b __rt_unser_reuse_old_ready");                    // direct calls defer the release until after the store
        emitter.label("__rt_unser_reuse_defer_old");
    }
    abi::load_at_offset(emitter, "x0", 72);
    abi::emit_call_label(emitter, context.defer_data);
    if matches!(site, ReuseSite::PropertySlot) {
        emitter.label("__rt_unser_reuse_old_ready");
    }
    abi::load_at_offset(emitter, "x0", 48);
    abi::load_at_offset(emitter, "x1", 56);
    abi::emit_call_label(emitter, context.release_payload);

    abi::load_at_offset(emitter, "x9", 32);
    site.arm_instruction(emitter, "ldr x10, [x9]", "replacement value tag");
    abi::store_at_offset(emitter, "x10", 48);
    site.arm_instruction(emitter, "ldr x10, [x9, #8]", "replacement payload low word");
    abi::store_at_offset(emitter, "x10", 56);
    site.arm_instruction(emitter, "ldr x10, [x9, #16]", "replacement payload high word");
    abi::store_at_offset(emitter, "x10", 64);
    abi::load_at_offset(emitter, "x9", 40);
    abi::load_at_offset(emitter, "x10", 48);
    site.arm_instruction(emitter, "str x10, [x9]", "publish the replacement tag in the existing box");
    abi::load_at_offset(emitter, "x10", 56);
    site.arm_instruction(emitter, "str x10, [x9, #8]", "publish the replacement low payload word");
    abi::load_at_offset(emitter, "x10", 64);
    site.arm_instruction(emitter, "str x10, [x9, #16]", "publish the replacement high payload word");

    abi::load_at_offset(emitter, "x10", 48);
    site.arm_instruction(emitter, "cmp x10, #4", "is the replacement an indexed array or hash?");
    site.arm_instruction(emitter, &format!("b.lo {prefix}_scalar"), "scalars have no reference slot to preserve");
    site.arm_instruction(emitter, "cmp x10, #5", "does the replacement use container storage?");
    site.arm_instruction(emitter, &format!("b.ls {prefix}_container"), "arrays and hashes own retained payload storage");
    site.arm_instruction(emitter, "cmp x10, #6", "is the replacement an object?");
    site.arm_instruction(emitter, &format!("b.eq {prefix}_object"), "objects need a retained nested wrapper");
    site.arm_instruction(emitter, &format!("b {prefix}_scalar"), "other wire values cannot be published references");

    emitter.label(&format!("{prefix}_container"));
    abi::load_at_offset(emitter, "x9", 32);
    site.arm_instruction(emitter, "mov x10, #7", "nested Mixed wrapper tag");
    site.arm_instruction(emitter, "str x10, [x9]", "repurpose the parsed box as a wrapper");
    abi::load_at_offset(emitter, "x10", 40);
    site.arm_instruction(emitter, "str x10, [x9, #8]", "point the wrapper at the reused box");
    site.arm_instruction(emitter, "str xzr, [x9, #16]", "nested wrappers carry no high payload word");
    abi::load_at_offset(emitter, "x0", 40);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, "x0", 32);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    site.arm_instruction(emitter, &format!("b {prefix}_return"), "preserve the wrapper published by the array parser");

    emitter.label(&format!("{prefix}_object"));
    abi::load_at_offset(emitter, "x9", 32);
    site.arm_instruction(emitter, "mov x10, #7", "nested Mixed wrapper tag");
    site.arm_instruction(emitter, "str x10, [x9]", "repurpose the parsed box as a wrapper");
    abi::load_at_offset(emitter, "x10", 40);
    site.arm_instruction(emitter, "str x10, [x9, #8]", "point the wrapper at the reused box");
    site.arm_instruction(emitter, "str xzr, [x9, #16]", "nested wrappers carry no high payload word");
    abi::load_at_offset(emitter, "x0", 40);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, "x0", 32);
    abi::emit_call_label(emitter, context.defer_data);
    site.arm_instruction(emitter, &format!("b {prefix}_return"), "keep the duplicate object's reference slot valid");

    emitter.label(&format!("{prefix}_scalar"));
    abi::load_at_offset(emitter, "x9", 32);
    site.arm_instruction(emitter, "mov x10, #8", "canonical null tag");
    site.arm_instruction(emitter, "str x10, [x9]", "clear the discarded box payload");
    site.arm_instruction(emitter, "str xzr, [x9, #8]", "null has no low payload word");
    site.arm_instruction(emitter, "str xzr, [x9, #16]", "null has no high payload word");
    abi::load_at_offset(emitter, "x0", 32);
    abi::emit_call_label(emitter, "__rt_decref_mixed");

    emitter.label(&format!("{prefix}_return"));
    if matches!(site, ReuseSite::PropertySlot) {
        abi::emit_load_symbol_to_reg(emitter, "x9", context.active_depth, 0);
        abi::emit_branch_if_zero_wide(emitter, "x9", "__rt_unser_reuse_release_direct");
        abi::emit_jump(emitter, "__rt_unser_reuse_return_box");
        emitter.label("__rt_unser_reuse_release_direct");
        abi::load_at_offset(emitter, "x0", 72);
        abi::emit_call_label(emitter, "__rt_decref_any");
        emitter.label("__rt_unser_reuse_return_box");
    }
}

/// Emits the x86_64 in-place Mixed box reuse used by duplicate hash keys and properties.
///
/// Input: rdi=destination box, rsi=parsed replacement box. Output: rax=destination box.
fn emit_reuse_box_x86_64<E: AssemblyEmitter + TargetAbi>(
    emitter: &mut E, context: Context<'_>,
) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize in-place box reuse ---");
    emitter.label_global("__rt_unser_reuse_box");
    abi::emit_frame_prologue(emitter, 96);
    abi::store_at_offset(emitter, "rdi", 40);
    abi::store_at_offset(emitter, "rsi", 32);
    emit_replacement_x86_64(emitter, context, ReuseSite::PropertySlot);
    abi::load_at_offset(emitter, "rax", 40);
    abi::emit_frame_restore(emitter, 96);
    abi::emit_return(emitter);
}

/// Emits x86_64 replacement with the old and new boxes in frame slots 40 and 32.
fn emit_replacement_x86_64(
    emitter: &mut (impl AssemblyEmitter + TargetAbi), context: Context<'_>, site: ReuseSite,
) {
    let prefix = site.label_prefix();
    abi::load_at_offset(emitter, "r9", 40);
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // previous value tag
    abi::store_at_offset(emitter, "r10", 48);
    emitter.instruction("mov r10, QWORD PTR [r9 + 8]");                         // previous payload low word
    abi::store_at_offset(emitter, "r10", 56);
    emitter.instruction("mov r10, QWORD PTR [r9 + 16]");                        // previous payload high word
    abi::store_at_offset(emitter, "r10", 64);
    abi::load_at_offset(emitter, "rax", 48);
    abi::load_at_offset(emitter, "rdi", 56);
    abi::load_at_offset(emitter, "rsi", 64);
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::store_at_offset(emitter, "rax", 72);
    if matches!(site, ReuseSite::PropertySlot) {
        // Direct hydration keeps this lease until the replacement is visible.
        abi::emit_load_symbol_to_reg(emitter, "r9", context.active_depth, 0);
        emitter.instruction("test r9, r9");                                     // check whether a decoder owns the deferred list
        emitter.instruction("jnz __rt_unser_reuse_defer_old_x");                // an active decoder owns the deferred copy
        emitter.instruction("jmp __rt_unser_reuse_old_ready_x");                // direct calls defer the release until after the store
        emitter.label("__rt_unser_reuse_defer_old_x");
    }
    // Both sites still have the old owner in rax from boxing. Preserve the
    // hash store's additional reload into its original argument register.
    let old_owner_reload = match site {
        ReuseSite::PropertySlot => "rax",
        ReuseSite::HashEntry => "rdi",
    };
    abi::load_at_offset(emitter, old_owner_reload, 72);
    abi::emit_call_label(emitter, context.defer_data);
    if matches!(site, ReuseSite::PropertySlot) {
        emitter.label("__rt_unser_reuse_old_ready_x");
    }
    abi::load_at_offset(emitter, "rdi", 48);
    abi::load_at_offset(emitter, "rsi", 56);
    abi::emit_call_label(emitter, context.release_payload);

    abi::load_at_offset(emitter, "r9", 32);
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // replacement value tag
    abi::store_at_offset(emitter, "r10", 48);
    emitter.instruction("mov r10, QWORD PTR [r9 + 8]");                         // replacement payload low word
    abi::store_at_offset(emitter, "r10", 56);
    emitter.instruction("mov r10, QWORD PTR [r9 + 16]");                        // replacement payload high word
    abi::store_at_offset(emitter, "r10", 64);
    abi::load_at_offset(emitter, "r9", 40);
    abi::load_at_offset(emitter, "r10", 48);
    emitter.instruction("mov QWORD PTR [r9], r10");                             // publish the replacement tag in the destination box
    abi::load_at_offset(emitter, "r10", 56);
    emitter.instruction("mov QWORD PTR [r9 + 8], r10");                         // publish the replacement low payload word
    abi::load_at_offset(emitter, "r10", 64);
    emitter.instruction("mov QWORD PTR [r9 + 16], r10");                        // publish the replacement high payload word

    abi::load_at_offset(emitter, "r10", 48);
    emitter.instruction("cmp r10, 4");                                          // is the replacement an array or hash?
    emitter.instruction(&format!("jl {prefix}_scalar_x"));                      // scalars discard their parsed box
    emitter.instruction("cmp r10, 5");                                          // does the replacement use container storage?
    emitter.instruction(&format!("jle {prefix}_container_x"));                  // arrays and hashes keep a wrapper slot
    emitter.instruction("cmp r10, 6");                                          // is the replacement an object?
    emitter.instruction(&format!("je {prefix}_object_x"));                      // objects keep a retained wrapper slot
    emitter.instruction(&format!("jmp {prefix}_scalar_x"));                     // other wire values discard their box

    emitter.label(&format!("{prefix}_container_x"));
    abi::load_at_offset(emitter, "r9", 32);
    emitter.instruction("mov QWORD PTR [r9], 7");                               // repurpose the parsed box as a wrapper
    abi::load_at_offset(emitter, "r10", 40);
    emitter.instruction("mov QWORD PTR [r9 + 8], r10");                         // point the wrapper at the reused box
    emitter.instruction("mov QWORD PTR [r9 + 16], 0");                          // nested wrappers carry no high payload word
    abi::load_at_offset(emitter, "rax", 40);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, "rax", 32);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    emitter.instruction(&format!("jmp {prefix}_return_x"));                     // preserve the container's wrapper slot

    emitter.label(&format!("{prefix}_object_x"));
    abi::load_at_offset(emitter, "r9", 32);
    emitter.instruction("mov QWORD PTR [r9], 7");                               // repurpose the parsed box as a wrapper
    abi::load_at_offset(emitter, "r10", 40);
    emitter.instruction("mov QWORD PTR [r9 + 8], r10");                         // point the wrapper at the reused box
    emitter.instruction("mov QWORD PTR [r9 + 16], 0");                          // nested wrappers carry no high payload word
    abi::load_at_offset(emitter, "rax", 40);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, "rax", 32);
    abi::emit_call_label(emitter, context.defer_data);
    emitter.instruction(&format!("jmp {prefix}_return_x"));                     // keep the duplicate object's reference slot valid

    emitter.label(&format!("{prefix}_scalar_x"));
    abi::load_at_offset(emitter, "r9", 32);
    emitter.instruction("mov QWORD PTR [r9], 8");                               // canonical null tag
    emitter.instruction("mov QWORD PTR [r9 + 8], 0");                           // null has no low payload word
    emitter.instruction("mov QWORD PTR [r9 + 16], 0");                          // null has no high payload word
    abi::load_at_offset(emitter, "rax", 32);
    abi::emit_call_label(emitter, "__rt_decref_mixed");

    emitter.label(&format!("{prefix}_return_x"));
    if matches!(site, ReuseSite::PropertySlot) {
        abi::emit_load_symbol_to_reg(emitter, "r9", context.active_depth, 0);
        emitter.instruction("test r9, r9");                                     // inspect the active decoder depth after publication
        emitter.instruction("jz __rt_unser_reuse_release_direct_x");            // direct calls still own the old-value copy
        emitter.instruction("jmp __rt_unser_reuse_return_box_x");               // a decoder context owns the old-value copy
        emitter.label("__rt_unser_reuse_release_direct_x");
        abi::load_at_offset(emitter, "rax", 72);
        abi::emit_call_label(emitter, "__rt_decref_any");
        emitter.label("__rt_unser_reuse_return_box_x");
    }
}

/// Reuses a duplicate hash entry's box in place.
///
/// The caller stages the replacement at frame offset 32 and the existing box at 40.
pub fn emit_replace_hash_entry(emitter: &mut (impl AssemblyEmitter + TargetAbi), context: Context<'_>) {
    match emitter.arch() {
        Arch::AArch64 => emit_replacement_aarch64(emitter, context, ReuseSite::HashEntry),
        Arch::X86_64 => emit_replacement_x86_64(emitter, context, ReuseSite::HashEntry),
    }
}

/// Emits the shared saved-payload release helper after the decoder's duplicate store.
pub fn emit_release_payload(emitter: &mut (impl AssemblyEmitter + TargetAbi)) {
    match emitter.arch() {
        Arch::AArch64 => emit_release_payload_aarch64(emitter),
        Arch::X86_64 => emit_release_payload_x86_64(emitter),
    }
}

/// Identifies the two callers of the in-place box replacement.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReuseSite {
    PropertySlot,
    HashEntry,
}

impl ReuseSite {
    /// Keeps the assembler labels stable at both call sites.
    fn label_prefix(self) -> &'static str {
        match self {
            Self::PropertySlot => "__rt_unser_reuse",
            Self::HashEntry => "__rt_unser_store_duplicate",
        }
    }

    /// Preserves the annotated ARM listing at the hash site and the plain property listing.
    fn arm_instruction(self, emitter: &mut impl AssemblyEmitter, instruction: &str, comment: &str) {
        if self == Self::HashEntry {
            let padding = 76usize.saturating_sub(instruction.len()).max(1);
            let annotation = "                                      //";
            emitter.instruction(&format!("{instruction}{:padding$}{annotation} {comment}", "")); // emit the existing annotated listing
        } else {
            emitter.instruction(instruction);                                   // emit the existing plain listing
        }
    }
}

/// Emits the AArch64 payload-only release helper used when a duplicate key replaces a value.
///
/// Input: x0=boxed value tag, x1=payload low word. Scalar tags own nothing.
fn emit_release_payload_aarch64(emitter: &mut (impl AssemblyEmitter + TargetAbi)) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize replaced-payload release ---");
    emitter.label_global("__rt_unser_release_payload");
    emitter.instruction("cmp x0, #1");                                          // is the replaced payload an owned string?
    emitter.instruction("b.eq __rt_unser_release_payload_string");              // strings release through the safe heap helper
    emitter.instruction("cmp x0, #4");                                          // is the replaced payload below the container tag range?
    emitter.instruction("b.lo __rt_unser_release_payload_done");                // scalars, bools, floats, and null own no payload
    emitter.instruction("cmp x0, #7");                                          // does the replaced tag stay inside the refcounted range?
    emitter.instruction("b.hi __rt_unser_release_payload_done");                // other tags do not come from the wire decoder
    emitter.instruction("mov x0, x1");                                          // pass the replaced container payload to the uniform release dispatcher
    abi::emit_jump(emitter, "__rt_decref_any");
    emitter.label("__rt_unser_release_payload_string");
    emitter.instruction("mov x0, x1");                                          // pass the replaced string payload to the safe free helper
    abi::emit_jump(emitter, "__rt_heap_free_safe");
    emitter.label("__rt_unser_release_payload_done");
    abi::emit_return(emitter);
}

/// Emits the x86_64 payload-only release helper used when a duplicate key replaces a value.
///
/// Input: rdi=boxed value tag, rsi=payload low word. Scalar tags own nothing.
fn emit_release_payload_x86_64(emitter: &mut (impl AssemblyEmitter + TargetAbi)) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize replaced-payload release ---");
    emitter.label_global("__rt_unser_release_payload");
    emitter.instruction("cmp rdi, 1");                                          // is the replaced payload an owned string?
    emitter.instruction("je __rt_unser_release_payload_string_x");              // strings release through the safe heap helper
    emitter.instruction("cmp rdi, 4");                                          // is the replaced payload below the container tag range?
    emitter.instruction("jb __rt_unser_release_payload_done_x");                // scalars, bools, floats, and null own no payload
    emitter.instruction("cmp rdi, 7");                                          // does the replaced tag stay inside the refcounted range?
    emitter.instruction("ja __rt_unser_release_payload_done_x");                // other tags do not come from the wire decoder
    emitter.instruction("mov rax, rsi");                                        // pass the replaced container payload to the uniform release dispatcher
    abi::emit_jump(emitter, "__rt_decref_any");
    emitter.label("__rt_unser_release_payload_string_x");
    emitter.instruction("mov rax, rsi");                                        // pass the replaced string payload to the safe free helper
    abi::emit_jump(emitter, "__rt_heap_free_safe");
    emitter.label("__rt_unser_release_payload_done_x");
    abi::emit_return(emitter);
}
