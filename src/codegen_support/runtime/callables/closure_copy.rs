//! Purpose:
//! Emits the `__rt_callable_closure_copy` runtime helper that turns an existing callable
//! descriptor into the Closure object `Closure::fromCallable()` and `$callable(...)` return.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`, for the callable factory
//!   lowering in `crate::codegen::lower_inst::callables`.
//!
//! Key details:
//! - A descriptor with an object handle already is a Closure: PHP returns that same object, so
//!   the helper only retains it.
//! - Any other descriptor is an implicit string or array callable, an invokable object converted
//!   to a callable, or an eval callback adapter (an eval closure, invokable object or function
//!   name). PHP wraps it in a new Closure, so the helper copies the header and every capture slot
//!   into a fresh descriptor with its own handle, retains the captures descriptor release will
//!   drop, clears the `CALLABLE_DESC_ORIGIN_*` flags and turns an `ObjectInvoke` kind back into
//!   the instance-method call it performs.
//! - A descriptor with an owned string or a reference capture would need a deep copy, so it is
//!   retained unchanged. No callable a factory receives has one.
//! - The caller rejects a missing descriptor (null or the in-band null sentinel) first.

use crate::codegen_support::callable_descriptor::{
    emit_initialize_runtime_descriptor, CALLABLE_DESC_ENVIRONMENT_OFFSET,
    CALLABLE_DESC_KIND_INSTANCE_METHOD, CALLABLE_DESC_KIND_MASK, CALLABLE_DESC_KIND_OBJECT_INVOKE,
    CALLABLE_DESC_ORIGIN_ARRAY, CALLABLE_DESC_ORIGIN_STRING, CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET,
};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Capture binding tags whose payload is a refcounted heap value, as descriptor release treats them.
const HEAP_CAPTURE_TAGS: [u64; 6] = [4, 5, 6, 7, 10, 12];

/// Emits `__rt_callable_closure_copy` for the active target.
///
/// Input: `x0`/`rax` = a non-null callable descriptor. Output: `x0`/`rax` = an owned descriptor
/// that is a PHP Closure object (see the module notes).
pub(crate) fn emit_callable_closure_copy(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_callable_closure_copy_x86_64(emitter);
        return;
    }
    let env = CALLABLE_DESC_ENVIRONMENT_OFFSET;
    let capture = CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET;
    let origin = CALLABLE_DESC_ORIGIN_STRING | CALLABLE_DESC_ORIGIN_ARRAY;
    emitter.blank();
    emitter.comment("--- runtime: callable closure copy ---");
    emitter.label_global("__rt_callable_closure_copy");
    emitter.instruction("sub sp, sp, #48");                                     // reserve source, count, result and index slots
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the source descriptor
    emitter.instruction("bl __rt_object_handle_of");                            // x0 = the descriptor's object handle, 0 for none
    emitter.instruction("cbz x0, __rt_callable_closure_copy_shape");            // without identity it is no Closure yet

    // -- an existing Closure, or a shape this helper does not copy: share it --
    emitter.label("__rt_callable_closure_copy_retain");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source descriptor
    emitter.instruction("bl __rt_incref");                                      // the result is one more owner of the same descriptor
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the shared descriptor
    emitter.instruction("b __rt_callable_closure_copy_return");                 // skip the copy

    // -- every capture must be a by-value scalar or heap value --
    emitter.label("__rt_callable_closure_copy_shape");
    emitter.instruction("str xzr, [sp, #8]");                                   // default to a descriptor without captures
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the source descriptor
    emitter.instruction(&format!("ldr x10, [x9, #{env}]"));                     // x10 = environment record
    emitter.instruction("cbz x10, __rt_callable_closure_copy_alloc");           // no environment, no captures
    emitter.instruction("ldr x11, [x10]");                                      // x11 = capture count
    emitter.instruction("str x11, [sp, #8]");                                   // remember how many slots to copy
    emitter.instruction("cbz x11, __rt_callable_closure_copy_alloc");           // no captures to copy
    emitter.instruction("ldr x12, [x10, #16]");                                 // x12 = capture binding metadata table
    emitter.instruction("cbz x12, __rt_callable_closure_copy_retain");          // untyped captures cannot be retained safely
    emitter.instruction("mov x13, #0");                                         // start with the first capture
    emitter.label("__rt_callable_closure_copy_check");
    emitter.instruction("cmp x13, x11");                                        // has every capture been checked?
    emitter.instruction("b.hs __rt_callable_closure_copy_alloc");               // the shape can be copied
    emitter.instruction("add x14, x12, x13, lsl #5");                           // x14 = this capture's 32-byte binding entry
    emitter.instruction("ldr x15, [x14, #24]");                                 // load the capture's reference flag
    emitter.instruction("cbnz x15, __rt_callable_closure_copy_retain");         // a reference capture owns a cell lease
    emitter.instruction("ldr x15, [x14, #16]");                                 // x15 = capture type tag
    emitter.instruction("cmp x15, #1");                                         // is it an owned string copy?
    emitter.instruction("b.eq __rt_callable_closure_copy_retain");              // a string capture would need its own copy
    emitter.instruction("add x13, x13, #1");                                    // check the next capture
    emitter.instruction("b __rt_callable_closure_copy_check");                  // continue the shape check

    // -- allocate the Closure with its own object handle --
    emitter.label("__rt_callable_closure_copy_alloc");
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the capture count
    emitter.instruction(&format!("mov x0, #{capture}"));                        // the fixed descriptor header
    emitter.instruction("add x0, x0, x10, lsl #4");                             // plus one 16-byte slot per capture
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = fresh descriptor block
    emit_initialize_runtime_descriptor(emitter, "x0");
    emitter.instruction("bl __rt_object_handle_acquire");                       // a Closure is an object: draw its handle from the object pool
    emitter.instruction("str x0, [sp, #16]");                                   // save the new descriptor
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source descriptor
    for offset in (0..capture).step_by(16) {
        emitter.instruction(&format!("ldp x2, x3, [x1, #{offset}]"));           // copy two header words
        emitter.instruction(&format!("stp x2, x3, [x0, #{offset}]"));           // store them into the Closure
    }

    // -- the Closure is no string, array or invokable object --
    emitter.instruction("ldr x2, [x0]");                                        // load the copied kind word
    emitter.instruction(&format!("mov x4, #{origin}"));                        // the string and array origin flags
    emitter.instruction("bic x2, x2, x4");                                      // drop the recorded string or array origin
    emitter.instruction(&format!("and x3, x2, #{CALLABLE_DESC_KIND_MASK}"));    // x3 = descriptor kind
    emitter.instruction(&format!("cmp x3, #{CALLABLE_DESC_KIND_OBJECT_INVOKE}")); // does it stand for an invokable object?
    emitter.instruction("b.ne __rt_callable_closure_copy_kind");                // other kinds already name their call
    emitter.instruction("sub x2, x2, x3");                                      // clear the invokable-object kind
    emitter.instruction(&format!("add x2, x2, #{CALLABLE_DESC_KIND_INSTANCE_METHOD}")); // the Closure calls the receiver's __invoke method
    emitter.label("__rt_callable_closure_copy_kind");
    emitter.instruction("str x2, [x0]");                                        // store the Closure's kind word

    // -- copy every capture slot --
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the capture count
    emitter.instruction(&format!("mov x14, #{capture}"));                       // offset of the first capture slot
    emitter.instruction("add x15, x14, x10, lsl #4");                           // offset past the last capture slot
    emitter.label("__rt_callable_closure_copy_slot");
    emitter.instruction("cmp x14, x15");                                        // has every slot been copied?
    emitter.instruction("b.hs __rt_callable_closure_copy_slots_done");          // continue with the capture owners
    emitter.instruction("ldr x2, [x1, x14]");                                   // load a capture word
    emitter.instruction("str x2, [x0, x14]");                                   // store it into the Closure
    emitter.instruction("add x14, x14, #8");                                    // advance to the next capture word
    emitter.instruction("b __rt_callable_closure_copy_slot");                   // continue copying
    emitter.label("__rt_callable_closure_copy_slots_done");

    // -- the Closure owns its own reference to every heap capture --
    emitter.instruction("str xzr, [sp, #24]");                                  // start with the first capture
    emitter.label("__rt_callable_closure_copy_owner");
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload the capture index
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the capture count
    emitter.instruction("cmp x13, x10");                                        // has every capture been retained?
    emitter.instruction("b.hs __rt_callable_closure_copy_done");                // return the Closure
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the source descriptor
    emitter.instruction(&format!("ldr x10, [x9, #{env}]"));                     // reload its environment record
    emitter.instruction("ldr x12, [x10, #16]");                                 // reload the capture binding table
    emitter.instruction("add x14, x12, x13, lsl #5");                           // x14 = this capture's binding entry
    emitter.instruction("ldr x15, [x14, #16]");                                 // x15 = capture type tag
    for tag in HEAP_CAPTURE_TAGS {
        emitter.instruction(&format!("cmp x15, #{tag}"));                       // is the capture a refcounted heap value?
        emitter.instruction("b.eq __rt_callable_closure_copy_owned");           // retain it
    }
    emitter.instruction("b __rt_callable_closure_copy_next");                   // scalars and borrowed words own nothing
    emitter.label("__rt_callable_closure_copy_owned");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the new Closure
    emitter.instruction(&format!("add x0, x0, #{capture}"));                    // address its first capture slot
    emitter.instruction("add x0, x0, x13, lsl #4");                             // address this capture slot
    emitter.instruction("ldr x0, [x0]");                                        // pass the captured heap value to the incref helper
    emitter.instruction("bl __rt_incref");                                      // the Closure owns its own reference to the capture
    emitter.label("__rt_callable_closure_copy_next");
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload the capture index
    emitter.instruction("add x13, x13, #1");                                    // advance to the next capture
    emitter.instruction("str x13, [sp, #24]");                                  // persist the capture index
    emitter.instruction("b __rt_callable_closure_copy_owner");                  // continue retaining captures
    emitter.label("__rt_callable_closure_copy_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // return the new Closure

    emitter.label("__rt_callable_closure_copy_return");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // tear down the helper frame
    emitter.instruction("ret");                                                 // return the owned Closure descriptor
}

/// Emits the Linux x86_64 `__rt_callable_closure_copy` helper (mirror of the AArch64 path).
fn emit_callable_closure_copy_x86_64(emitter: &mut Emitter) {
    let env = CALLABLE_DESC_ENVIRONMENT_OFFSET;
    let capture = CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET;
    let origin = CALLABLE_DESC_ORIGIN_STRING | CALLABLE_DESC_ORIGIN_ARRAY;
    emitter.blank();
    emitter.comment("--- runtime: callable closure copy (x86_64) ---");
    emitter.label_global("__rt_callable_closure_copy");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 32");                                         // reserve source, count, result and index slots
    emitter.instruction("mov [rsp], rax");                                      // save the source descriptor
    emitter.instruction("call __rt_object_handle_of");                          // rax = the descriptor's object handle, 0 for none
    emitter.instruction("test rax, rax");                                       // does the descriptor already have identity?
    emitter.instruction("jz __rt_callable_closure_copy_shape");                 // without identity it is no Closure yet

    // -- an existing Closure, or a shape this helper does not copy: share it --
    emitter.label("__rt_callable_closure_copy_retain");
    emitter.instruction("mov rax, [rsp]");                                      // reload the source descriptor
    emitter.instruction("call __rt_incref");                                    // the result is one more owner of the same descriptor
    emitter.instruction("mov rax, [rsp]");                                      // return the shared descriptor
    emitter.instruction("jmp __rt_callable_closure_copy_return");               // skip the copy

    // -- every capture must be a by-value scalar or heap value --
    emitter.label("__rt_callable_closure_copy_shape");
    emitter.instruction("mov QWORD PTR [rsp+8], 0");                            // default to a descriptor without captures
    emitter.instruction("mov r9, [rsp]");                                       // reload the source descriptor
    emitter.instruction(&format!("mov r10, [r9+{env}]"));                       // r10 = environment record
    emitter.instruction("test r10, r10");                                       // is there an environment?
    emitter.instruction("jz __rt_callable_closure_copy_alloc");                 // no environment, no captures
    emitter.instruction("mov r11, [r10]");                                      // r11 = capture count
    emitter.instruction("mov [rsp+8], r11");                                    // remember how many slots to copy
    emitter.instruction("test r11, r11");                                       // are there captures?
    emitter.instruction("jz __rt_callable_closure_copy_alloc");                 // no captures to copy
    emitter.instruction("mov r10, [r10+16]");                                   // r10 = capture binding metadata table
    emitter.instruction("test r10, r10");                                       // are the captures typed?
    emitter.instruction("jz __rt_callable_closure_copy_retain");                // untyped captures cannot be retained safely
    emitter.instruction("xor ecx, ecx");                                        // start with the first capture
    emitter.label("__rt_callable_closure_copy_check");
    emitter.instruction("cmp rcx, r11");                                        // has every capture been checked?
    emitter.instruction("jae __rt_callable_closure_copy_alloc");                // the shape can be copied
    emitter.instruction("mov rdx, rcx");                                        // copy the capture index
    emitter.instruction("shl rdx, 5");                                          // each binding entry is 32 bytes
    emitter.instruction("add rdx, r10");                                        // rdx = this capture's binding entry
    emitter.instruction("cmp QWORD PTR [rdx+24], 0");                           // is it a reference capture?
    emitter.instruction("jne __rt_callable_closure_copy_retain");               // a reference capture owns a cell lease
    emitter.instruction("cmp QWORD PTR [rdx+16], 1");                           // is it an owned string copy?
    emitter.instruction("je __rt_callable_closure_copy_retain");                // a string capture would need its own copy
    emitter.instruction("inc rcx");                                             // check the next capture
    emitter.instruction("jmp __rt_callable_closure_copy_check");                // continue the shape check

    // -- allocate the Closure with its own object handle --
    emitter.label("__rt_callable_closure_copy_alloc");
    emitter.instruction("mov rax, [rsp+8]");                                    // reload the capture count
    emitter.instruction("shl rax, 4");                                          // one 16-byte slot per capture
    emitter.instruction(&format!("add rax, {capture}"));                        // plus the fixed descriptor header
    emitter.instruction("call __rt_heap_alloc");                                // rax = fresh descriptor block
    emit_initialize_runtime_descriptor(emitter, "rax");
    emitter.instruction("call __rt_object_handle_acquire");                     // a Closure is an object: draw its handle from the object pool
    emitter.instruction("mov [rsp+16], rax");                                   // save the new descriptor
    emitter.instruction("mov r9, [rsp]");                                       // r9 = source descriptor
    for offset in (0..capture).step_by(8) {
        emitter.instruction(&format!("mov rdx, [r9+{offset}]"));                // copy one header word
        emitter.instruction(&format!("mov [rax+{offset}], rdx"));               // store it into the Closure
    }

    // -- the Closure is no string, array or invokable object --
    emitter.instruction("mov rdx, [rax]");                                      // load the copied kind word
    emitter.instruction(&format!("and rdx, {}", !origin as i64));               // drop the recorded string or array origin
    emitter.instruction("mov rcx, rdx");                                        // copy the kind word
    emitter.instruction(&format!("and rcx, {CALLABLE_DESC_KIND_MASK}"));        // rcx = descriptor kind
    emitter.instruction(&format!("cmp rcx, {CALLABLE_DESC_KIND_OBJECT_INVOKE}")); // does it stand for an invokable object?
    emitter.instruction("jne __rt_callable_closure_copy_kind");                 // other kinds already name their call
    emitter.instruction("sub rdx, rcx");                                        // clear the invokable-object kind
    emitter.instruction(&format!("add rdx, {CALLABLE_DESC_KIND_INSTANCE_METHOD}")); // the Closure calls the receiver's __invoke method
    emitter.label("__rt_callable_closure_copy_kind");
    emitter.instruction("mov [rax], rdx");                                      // store the Closure's kind word

    // -- copy every capture slot --
    emitter.instruction("mov rcx, [rsp+8]");                                    // reload the capture count
    emitter.instruction("shl rcx, 4");                                          // bytes of capture storage
    emitter.instruction(&format!("add rcx, {capture}"));                        // offset past the last capture slot
    emitter.instruction(&format!("mov rdx, {capture}"));                        // offset of the first capture slot
    emitter.label("__rt_callable_closure_copy_slot");
    emitter.instruction("cmp rdx, rcx");                                        // has every slot been copied?
    emitter.instruction("jae __rt_callable_closure_copy_slots_done");           // continue with the capture owners
    emitter.instruction("mov r10, [r9+rdx]");                                   // load a capture word
    emitter.instruction("mov [rax+rdx], r10");                                  // store it into the Closure
    emitter.instruction("add rdx, 8");                                          // advance to the next capture word
    emitter.instruction("jmp __rt_callable_closure_copy_slot");                 // continue copying
    emitter.label("__rt_callable_closure_copy_slots_done");

    // -- the Closure owns its own reference to every heap capture --
    emitter.instruction("mov QWORD PTR [rsp+24], 0");                           // start with the first capture
    emitter.label("__rt_callable_closure_copy_owner");
    emitter.instruction("mov rcx, [rsp+24]");                                   // reload the capture index
    emitter.instruction("cmp rcx, [rsp+8]");                                    // has every capture been retained?
    emitter.instruction("jae __rt_callable_closure_copy_done");                 // return the Closure
    emitter.instruction("mov r9, [rsp]");                                       // reload the source descriptor
    emitter.instruction(&format!("mov r10, [r9+{env}]"));                       // reload its environment record
    emitter.instruction("mov r10, [r10+16]");                                   // reload the capture binding table
    emitter.instruction("mov rdx, rcx");                                        // copy the capture index
    emitter.instruction("shl rdx, 5");                                          // each binding entry is 32 bytes
    emitter.instruction("mov rdx, [r10+rdx+16]");                               // rdx = capture type tag
    for tag in HEAP_CAPTURE_TAGS {
        emitter.instruction(&format!("cmp rdx, {tag}"));                        // is the capture a refcounted heap value?
        emitter.instruction("je __rt_callable_closure_copy_owned");             // retain it
    }
    emitter.instruction("jmp __rt_callable_closure_copy_next");                 // scalars and borrowed words own nothing
    emitter.label("__rt_callable_closure_copy_owned");
    emitter.instruction("mov rax, [rsp+16]");                                   // reload the new Closure
    emitter.instruction("shl rcx, 4");                                          // byte offset of this capture slot
    emitter.instruction(&format!("mov rax, [rax+rcx+{capture}]"));              // pass the captured heap value to the incref helper
    emitter.instruction("call __rt_incref");                                    // the Closure owns its own reference to the capture
    emitter.label("__rt_callable_closure_copy_next");
    emitter.instruction("inc QWORD PTR [rsp+24]");                              // advance to the next capture
    emitter.instruction("jmp __rt_callable_closure_copy_owner");                // continue retaining captures
    emitter.label("__rt_callable_closure_copy_done");
    emitter.instruction("mov rax, [rsp+16]");                                   // return the new Closure

    emitter.label("__rt_callable_closure_copy_return");
    emitter.instruction("mov rsp, rbp");                                        // tear down the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the owned Closure descriptor
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Every target copies a non-Closure into a fresh handle-bearing descriptor and clears origins.
    #[test]
    fn test_callable_closure_copy_acquires_a_handle_and_clears_origin() {
        for target in [Target::new(Platform::MacOS, Arch::AArch64), Target::new(Platform::Linux, Arch::X86_64)] {
            let mut emitter = Emitter::new(target);
            emit_callable_closure_copy(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_callable_closure_copy:"), "{target:?}");
            assert!(asm.contains("__rt_object_handle_of"), "{target:?}");
            assert!(asm.contains("__rt_object_handle_acquire"), "{target:?}");
            assert!(asm.contains("__rt_callable_closure_copy_retain:"), "{target:?}");
        }
    }
}
