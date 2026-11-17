//! Purpose:
//! Exposes trusted internal SPL attached-storage writes to opaque eval without public dispatch.
//!
//! Called from:
//! - Magician after its class metadata proves ArrayObject or ArrayIterator owns offsetGet.
//!
//! Key details:
//! - One versioned boundary contains both native fetch and set throws below Rust.
//! - Fetch checks the private for-write selector and refuses eval-only classes or overrides.
//! - Both operations normalize the supplied key exactly once inside the protected boundary.

use super::*;
use crate::codegen_support::runtime::exceptions::emit_protected_status;
use crate::codegen_support::try_handlers::TRY_HANDLER_SLOT_SIZE;

/// Emits the protected C ABI used for attached internal-SPL fetch and set operations.
pub(super) fn emit(emitter: &mut Emitter) {
    let symbol = emitter
        .target
        .extern_symbol("__elephc_eval_spl_storage_write_v1");
    emit_protected_status(emitter, &symbol, body);
}

/// Selects the requested operation and returns success, unsupported, or pending-throwable status.
fn body(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => body_aarch64(emitter),
        Arch::X86_64 => body_x86_64(emitter),
    }
}

/// Emits the protected AAPCS64 operation body.
fn body_aarch64(emitter: &mut Emitter) {
    let fetch = "__rt_eval_spl_storage_write_v1_fetch";
    let set = "__rt_eval_spl_storage_write_v1_set";
    let set_value = "__rt_eval_spl_storage_write_v1_set_value";
    let unsupported = "__rt_eval_spl_storage_write_v1_unsupported";
    let done = "__rt_eval_spl_storage_write_v1_done";
    emitter.instruction(&format!("ldr x9, [sp, #{}]", TRY_HANDLER_SLOT_SIZE));  // recover the requested bridge operation
    emitter.instruction(&format!("cbz x9, {fetch}"));                           // operation zero fetches an attached child cell
    emitter.instruction("cmp x9, #1");                                          // operation one mutates that attached child
    emitter.instruction(&format!("b.eq {set}"));                                // keep replacement and destructor throws protected
    emitter.instruction(&format!("b {unsupported}"));                           // reject unknown ABI operations

    emitter.label(fetch);
    emitter.instruction(&format!("ldr x10, [sp, #{}]", TRY_HANDLER_SLOT_SIZE + 24));  // recover the caller-owned result slot
    emitter.instruction("str xzr, [x10]");                                      // failed or interrupted fetches publish no owner
    emitter.instruction(&format!("ldr x0, [sp, #{}]", TRY_HANDLER_SLOT_SIZE + 8));  // borrow the boxed receiver for validation
    emitter.instruction("bl __rt_mixed_deref");                                 // inspect the current cell behind any PHP reference
    emitter.instruction(&format!("cbz x0, {unsupported}"));                     // absent cells cannot expose attached SPL storage
    emitter.instruction("ldr x9, [x0]");                                        // load the concrete receiver tag
    emitter.instruction("cmp x9, #6");                                          // runtime tag 6 denotes an object payload
    emitter.instruction(&format!("b.ne {unsupported}"));                        // only object receivers can use the private selector
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the native object payload pointer
    emit_branch_if_null_container(emitter, "x9", "x10", unsupported);
    emitter.instruction("ldr x11, [x9]");                                       // load the concrete native class id
    abi::emit_symbol_address(emitter, "x10", "_class_gc_desc_count");
    emitter.instruction("ldr x10, [x10]");                                      // load the dense class metadata extent
    emitter.instruction("cmp x11, x10");                                        // is this class represented by the private table?
    emitter.instruction(&format!("b.hs {unsupported}"));                        // eval-only class ids must never use native selectors
    abi::emit_symbol_address(emitter, "x10", "_class_offsetget_forwrite_ptrs");
    emitter.instruction("ldr x10, [x10, x11, lsl #3]");                         // select only inherited internal SPL write fetches
    emitter.instruction(&format!("cbz x10, {unsupported}"));                    // a getter override has no trusted attached selector
    emitter.instruction(&format!("ldr x0, [sp, #{}]", TRY_HANDLER_SLOT_SIZE + 16));  // pass the boxed key to the normalizer once
    emitter.instruction("bl __elephc_eval_key_normalize");                      // return normalized key_lo/key_hi in x1/x2
    emitter.instruction(&format!("ldr x0, [sp, #{}]", TRY_HANDLER_SLOT_SIZE + 8));  // reload the validated boxed SPL receiver
    emitter.instruction("mov x3, xzr");                                         // ordinary attached writes retain standard COW behavior
    emitter.instruction("bl __rt_mixed_array_get_for_write");                   // fetch and retain the cell installed in internal SPL storage
    emitter.instruction(&format!("ldr x10, [sp, #{}]", TRY_HANDLER_SLOT_SIZE + 24));  // recover the caller's result slot after native work
    emitter.instruction("str x0, [x10]");                                       // transfer the owned attached cell to Rust
    emitter.instruction(&format!("b {done}"));                                  // report successful protected completion

    emitter.label(set);
    emitter.instruction(&format!("ldr x0, [sp, #{}]", TRY_HANDLER_SLOT_SIZE + 16));  // normalize the boxed set key once
    emitter.instruction("bl __elephc_eval_key_normalize");                      // return normalized key_lo/key_hi in x1/x2
    emitter.instruction(&format!("str x1, [sp, #{}]", TRY_HANDLER_SLOT_SIZE));  // retain key_lo in the consumed operation slot
    emitter.instruction(&format!("str x2, [sp, #{}]", TRY_HANDLER_SLOT_SIZE + 16));  // retain key_hi in the consumed key slot
    emitter.instruction(&format!("ldr x0, [sp, #{}]", TRY_HANDLER_SLOT_SIZE + 8));  // inspect the concrete attached cell after key normalization
    emitter.instruction("bl __rt_mixed_deref");                                 // follow any PHP reference before false autovivification
    emitter.instruction(&format!("cbz x0, {set_value}"));                       // absent cells retain the setter's ordinary drop behavior
    emitter.instruction(&format!("str x0, [sp, #{}]", TRY_HANDLER_SLOT_SIZE + 8));  // retain the concrete writable cell for the setter
    emitter.instruction("ldr x9, [x0]");                                        // inspect the attached cell's runtime tag
    emitter.instruction("cmp x9, #3");                                          // runtime tag 3 denotes boolean
    emitter.instruction(&format!("b.ne {set_value}"));                          // arrays, null, and strings need no bridge conversion
    emitter.instruction("ldr x9, [x0, #8]");                                    // distinguish deprecated false from invalid true
    emitter.instruction(&format!("cbnz x9, {set_value}"));                      // Rust rejects true before reaching this bridge
    emitter.instruction("mov x9, #8");                                          // canonical runtime tag 8 denotes null
    emitter.instruction("str x9, [x0]");                                        // make the existing attached cell autovivifiable in place
    emitter.instruction("stp xzr, xzr, [x0, #8]");                              // clear the non-owning boolean payload words
    emitter.label(set_value);
    emitter.instruction(&format!("ldr x0, [sp, #{}]", TRY_HANDLER_SLOT_SIZE + 24));  // recover the boxed value for its storage retain
    emitter.instruction("bl __rt_incref");                                      // give the Mixed setter the owner it consumes
    emitter.instruction(&format!("ldr x0, [sp, #{}]", TRY_HANDLER_SLOT_SIZE + 8));  // recover the attached child array cell
    emitter.instruction(&format!("ldr x1, [sp, #{}]", TRY_HANDLER_SLOT_SIZE));  // recover normalized key_lo
    emitter.instruction(&format!("ldr x2, [sp, #{}]", TRY_HANDLER_SLOT_SIZE + 16));  // recover normalized key_hi
    emitter.instruction(&format!("ldr x3, [sp, #{}]", TRY_HANDLER_SLOT_SIZE + 24));  // transfer the retained value to array storage
    emitter.instruction("bl __rt_mixed_array_set");                             // keep replacement destructor throws below Rust
    emitter.instruction(&format!("b {done}"));                                  // report successful protected completion

    emitter.label(unsupported);
    emitter.instruction("mov w0, #1");                                          // refuse unsupported operations or receivers
    emitter.instruction(&format!("b {done}_status"));                           // preserve the explicit unsupported status
    emitter.label(done);
    emitter.instruction("mov w0, #0");                                          // report successful protected completion
    emitter.label(&format!("{done}_status"));
}

/// Emits the protected SysV x86_64 operation body.
fn body_x86_64(emitter: &mut Emitter) {
    let fetch = "__rt_eval_spl_storage_write_v1_fetch";
    let set = "__rt_eval_spl_storage_write_v1_set";
    let set_value = "__rt_eval_spl_storage_write_v1_set_value";
    let unsupported = "__rt_eval_spl_storage_write_v1_unsupported";
    let done = "__rt_eval_spl_storage_write_v1_done";
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", TRY_HANDLER_SLOT_SIZE));  // recover the requested bridge operation
    emitter.instruction("test r9, r9");                                         // operation zero fetches an attached child cell
    emitter.instruction(&format!("jz {fetch}"));                                // enter the trusted internal-selector path
    emitter.instruction("cmp r9, 1");                                           // operation one mutates that attached child
    emitter.instruction(&format!("je {set}"));                                  // keep replacement and destructor throws protected
    emitter.instruction(&format!("jmp {unsupported}"));                         // reject unknown ABI operations

    emitter.label(fetch);
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", TRY_HANDLER_SLOT_SIZE + 24));  // recover the caller-owned result slot
    emitter.instruction("mov QWORD PTR [r10], 0");                              // failed or interrupted fetches publish no owner
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", TRY_HANDLER_SLOT_SIZE + 8));  // borrow the boxed receiver for validation
    emitter.instruction("call __rt_mixed_deref");                               // inspect the current cell behind any PHP reference
    emitter.instruction("test rax, rax");                                       // absent cells cannot expose attached SPL storage
    emitter.instruction(&format!("jz {unsupported}"));                          // reject the absent receiver before metadata reads
    emitter.instruction("cmp QWORD PTR [rax], 6");                              // runtime tag 6 denotes an object payload
    emitter.instruction(&format!("jne {unsupported}"));                         // only object receivers can use the private selector
    emitter.instruction("mov r9, QWORD PTR [rax + 8]");                         // load the native object payload pointer
    emit_branch_if_null_container(emitter, "r9", "r10", unsupported);
    emitter.instruction("mov r11, QWORD PTR [r9]");                             // load the concrete native class id
    abi::emit_load_symbol_to_reg(emitter, "r10", "_class_gc_desc_count", 0);
    emitter.instruction("cmp r11, r10");                                        // is this class represented by the private table?
    emitter.instruction(&format!("jae {unsupported}"));                         // eval-only class ids must never use native selectors
    abi::emit_symbol_address(emitter, "r10", "_class_offsetget_forwrite_ptrs");
    emitter.instruction("mov r10, QWORD PTR [r10 + r11 * 8]");                  // select only inherited internal SPL write fetches
    emitter.instruction("test r10, r10");                                       // a getter override has no trusted attached selector
    emitter.instruction(&format!("jz {unsupported}"));                          // refuse it instead of entering public dispatch
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", TRY_HANDLER_SLOT_SIZE + 16));  // pass the boxed key to the normalizer once
    emitter.instruction("call __elephc_eval_key_normalize");                    // return normalized key_lo/key_hi in rax/rdx
    emitter.instruction("mov rsi, rax");                                        // forward normalized key_lo to the write fetch
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", TRY_HANDLER_SLOT_SIZE + 8));  // reload the validated boxed SPL receiver
    emitter.instruction("xor ecx, ecx");                                        // ordinary attached writes retain standard COW behavior
    emitter.instruction("call __rt_mixed_array_get_for_write");                 // fetch and retain the cell installed in internal SPL storage
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", TRY_HANDLER_SLOT_SIZE + 24));  // recover the caller's result slot after native work
    emitter.instruction("mov QWORD PTR [r10], rax");                            // transfer the owned attached cell to Rust
    emitter.instruction(&format!("jmp {done}"));                                // report successful protected completion

    emitter.label(set);
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", TRY_HANDLER_SLOT_SIZE + 16));  // normalize the boxed set key once
    emitter.instruction("call __elephc_eval_key_normalize");                    // return normalized key_lo/key_hi in rax/rdx
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", TRY_HANDLER_SLOT_SIZE));  // retain key_lo in the consumed operation slot
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", TRY_HANDLER_SLOT_SIZE + 16));  // retain key_hi in the consumed key slot
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", TRY_HANDLER_SLOT_SIZE + 8));  // inspect the concrete attached cell after key normalization
    emitter.instruction("call __rt_mixed_deref");                               // follow any PHP reference before false autovivification
    emitter.instruction("test rax, rax");                                       // absent cells retain the setter's ordinary drop behavior
    emitter.instruction(&format!("jz {set_value}"));                            // leave the original nullable receiver slot unchanged
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", TRY_HANDLER_SLOT_SIZE + 8));  // retain the concrete writable cell for the setter
    emitter.instruction("cmp QWORD PTR [rax], 3");                              // runtime tag 3 denotes boolean
    emitter.instruction(&format!("jne {set_value}"));                           // arrays, null, and strings need no bridge conversion
    emitter.instruction("cmp QWORD PTR [rax + 8], 0");                          // distinguish deprecated false from invalid true
    emitter.instruction(&format!("jne {set_value}"));                           // Rust rejects true before reaching this bridge
    emitter.instruction("mov QWORD PTR [rax], 8");                              // canonical runtime tag 8 denotes null
    emitter.instruction("mov QWORD PTR [rax + 8], 0");                          // clear the non-owning boolean low payload
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // clear the non-owning boolean high payload
    emitter.label(set_value);
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", TRY_HANDLER_SLOT_SIZE + 24));  // recover the boxed value for its storage retain
    emitter.instruction("call __rt_incref");                                    // give the Mixed setter the owner it consumes
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", TRY_HANDLER_SLOT_SIZE + 8));  // recover the attached child array cell
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", TRY_HANDLER_SLOT_SIZE));  // recover normalized key_lo
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", TRY_HANDLER_SLOT_SIZE + 16));  // recover normalized key_hi
    emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {}]", TRY_HANDLER_SLOT_SIZE + 24));  // transfer the retained value to array storage
    emitter.instruction("call __rt_mixed_array_set");                           // keep replacement destructor throws below Rust
    emitter.instruction(&format!("jmp {done}"));                                // report successful protected completion

    emitter.label(unsupported);
    emitter.instruction("mov eax, 1");                                          // refuse unsupported operations or receivers
    emitter.instruction(&format!("jmp {done}_status"));                         // preserve the explicit unsupported status
    emitter.label(done);
    emitter.instruction("xor eax, eax");                                        // report successful protected completion
    emitter.label(&format!("{done}_status"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target protects the private fetch and attached set without public dispatch.
    #[test]
    fn spl_eval_attached_writes_are_protected_and_never_use_public_dispatch() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target.clone());
            emit(&mut emitter);
            let assembly = emitter.output();
            assert!(
                assembly.contains(&target.extern_symbol("__elephc_eval_spl_storage_write_v1")),
                "{name}"
            );
            let table = assembly.find("_class_offsetget_forwrite_ptrs").unwrap();
            let fetch = assembly.find("__rt_mixed_array_get_for_write").unwrap();
            assert!(table < fetch, "{name}: {assembly}");
            assert!(assembly.contains("__rt_mixed_array_set"), "{name}: missing protected set");
            assert!(!assembly.contains("_class_offsetget_ptrs"), "{name}: public getter fallback");
            assert!(assembly.contains(&target.extern_symbol("_setjmp")), "{name}: missing protected boundary");
        }
    }
}
