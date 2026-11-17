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
//! - Any other descriptor is an implicit string or array callable, or an invokable object
//!   converted to a callable. PHP wraps it in a new Closure, so the helper copies the header and
//!   its single heap capture (a bound receiver) into a fresh descriptor with its own handle,
//!   clears the `CALLABLE_DESC_ORIGIN_*` flags and turns an `ObjectInvoke` kind back into the
//!   instance-method call it performs.
//! - A shape it cannot copy safely (several captures, an owned string or a reference capture)
//!   is retained unchanged, as before this helper existed.

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
    emitter.instruction("sub sp, sp, #48");                                     // reserve source, capture shape and result slots
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

    // -- accept no capture, or one by-value heap capture --
    emitter.label("__rt_callable_closure_copy_shape");
    emitter.instruction("str xzr, [sp, #8]");                                   // default to a descriptor without captures
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the source descriptor
    emitter.instruction(&format!("ldr x10, [x9, #{env}]"));                     // x10 = environment record
    emitter.instruction("cbz x10, __rt_callable_closure_copy_alloc");           // no environment, no captures
    emitter.instruction("ldr x11, [x10]");                                      // x11 = capture count
    emitter.instruction("cbz x11, __rt_callable_closure_copy_alloc");           // no captures to copy
    emitter.instruction("cmp x11, #1");                                         // only a single bound receiver is copied
    emitter.instruction("b.ne __rt_callable_closure_copy_retain");              // keep a wider capture shape unchanged
    emitter.instruction("ldr x12, [x10, #16]");                                 // x12 = capture binding metadata table
    emitter.instruction("cbz x12, __rt_callable_closure_copy_retain");          // untyped captures cannot be retained safely
    emitter.instruction("ldr x13, [x12, #24]");                                 // load the capture's reference flag
    emitter.instruction("cbnz x13, __rt_callable_closure_copy_retain");         // a reference capture owns a cell lease
    emitter.instruction("ldr x13, [x12, #16]");                                 // x13 = capture type tag
    for tag in HEAP_CAPTURE_TAGS {
        emitter.instruction(&format!("cmp x13, #{tag}"));                       // is the capture a refcounted heap value?
        emitter.instruction("b.eq __rt_callable_closure_copy_heap_capture");    // copy and retain it
    }
    emitter.instruction("b __rt_callable_closure_copy_retain");                 // strings and scalars keep the original descriptor
    emitter.label("__rt_callable_closure_copy_heap_capture");
    emitter.instruction("str x11, [sp, #8]");                                   // record the single capture to copy

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

    // -- copy and retain the bound receiver --
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the capture count
    emitter.instruction("cbz x10, __rt_callable_closure_copy_done");            // nothing captured
    emitter.instruction(&format!("ldp x2, x3, [x1, #{capture}]"));              // load the capture slot
    emitter.instruction(&format!("stp x2, x3, [x0, #{capture}]"));              // store it into the Closure
    emitter.instruction("mov x0, x2");                                          // pass the captured heap value to the incref helper
    emitter.instruction("bl __rt_incref");                                      // the Closure owns its own reference to the capture
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
    emitter.instruction("sub rsp, 32");                                         // reserve source, capture shape and result slots
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

    // -- accept no capture, or one by-value heap capture --
    emitter.label("__rt_callable_closure_copy_shape");
    emitter.instruction("mov QWORD PTR [rsp+8], 0");                            // default to a descriptor without captures
    emitter.instruction("mov r9, [rsp]");                                       // reload the source descriptor
    emitter.instruction(&format!("mov r10, [r9+{env}]"));                       // r10 = environment record
    emitter.instruction("test r10, r10");                                       // is there an environment?
    emitter.instruction("jz __rt_callable_closure_copy_alloc");                 // no environment, no captures
    emitter.instruction("mov r11, [r10]");                                      // r11 = capture count
    emitter.instruction("test r11, r11");                                       // are there captures?
    emitter.instruction("jz __rt_callable_closure_copy_alloc");                 // no captures to copy
    emitter.instruction("cmp r11, 1");                                          // only a single bound receiver is copied
    emitter.instruction("jne __rt_callable_closure_copy_retain");               // keep a wider capture shape unchanged
    emitter.instruction("mov r10, [r10+16]");                                   // r10 = capture binding metadata table
    emitter.instruction("test r10, r10");                                       // is the capture typed?
    emitter.instruction("jz __rt_callable_closure_copy_retain");                // untyped captures cannot be retained safely
    emitter.instruction("cmp QWORD PTR [r10+24], 0");                           // is it a reference capture?
    emitter.instruction("jne __rt_callable_closure_copy_retain");               // a reference capture owns a cell lease
    emitter.instruction("mov rcx, [r10+16]");                                   // rcx = capture type tag
    for tag in HEAP_CAPTURE_TAGS {
        emitter.instruction(&format!("cmp rcx, {tag}"));                        // is the capture a refcounted heap value?
        emitter.instruction("je __rt_callable_closure_copy_heap_capture");      // copy and retain it
    }
    emitter.instruction("jmp __rt_callable_closure_copy_retain");               // strings and scalars keep the original descriptor
    emitter.label("__rt_callable_closure_copy_heap_capture");
    emitter.instruction("mov [rsp+8], r11");                                    // record the single capture to copy

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

    // -- copy and retain the bound receiver --
    emitter.instruction("cmp QWORD PTR [rsp+8], 0");                            // is anything captured?
    emitter.instruction("je __rt_callable_closure_copy_done");                  // nothing to copy
    emitter.instruction(&format!("mov rdx, [r9+{capture}]"));                   // load the capture payload
    emitter.instruction(&format!("mov [rax+{capture}], rdx"));                  // store it into the Closure
    emitter.instruction(&format!("mov rcx, [r9+{}]", capture + 8));             // load the capture's second word
    emitter.instruction(&format!("mov [rax+{}], rcx", capture + 8));            // store it into the Closure
    emitter.instruction("mov rax, rdx");                                        // pass the captured heap value to the incref helper
    emitter.instruction("call __rt_incref");                                    // the Closure owns its own reference to the capture
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
