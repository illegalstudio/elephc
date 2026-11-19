//! Purpose:
//! Maintains the native object header state shared by destructor dispatch and GC.
//!
//! Called from:
//! - Compiler dispatch, failed-construction cleanup, and Memoir reclamation.
//!
//! Key details:
//! - Running state lives in the refcount word and persistent state in the kind word.
//! - Callers select the destructor and own PHP control flow.

use crate::{Arch, Emitter};

const RUNNING_BIT: u32 = 31;
pub(crate) const RUNNING_MASK: u32 = 1 << RUNNING_BIT;
const FINISHED_BIT: u32 = 14;
pub(crate) const FINISHED_MASK: u32 = 1 << FINISHED_BIT;

/// Marks destructor invocation once; failed construction uses the same bit to suppress invocation.
pub fn emit_dispatch_guard(emitter: &mut impl Emitter) {
    let arm = emitter.arch() == Arch::AArch64;
    emitter.label_global("__rt_call_object_destructor");
    if arm {
        emitter.instruction("cbz x0, __rt_object_destructor_boundary_ret");     // reject a missing receiver before probing its header
        emitter.instruction("ldr w10, [x0, #-12]");                             // inspect a destructor already running on this receiver
        emitter.instruction(&format!("tbnz w10, #{RUNNING_BIT}, __rt_object_destructor_boundary_ret"));// leave a reentrant call unmarked for its active owner
        emitter.instruction("ldr x9, [x0, #-8]");                               // inspect persistent object destructor state
        emitter.instruction(&format!("tbnz x9, #{FINISHED_BIT}, __rt_object_destructor_boundary_ret"));// skip a completed destructor or failed construction permanently
        emitter.instruction(&format!("orr x9, x9, #{FINISHED_MASK:#x}"));       // record invocation before any user callback or throwable
        emitter.instruction("str x9, [x0, #-8]");                               // preserve called state in the low sixteen kind-word bits across GC
        emitter.instruction("b __rt_call_object_destructor_body");              // let the protected caller finish temporary release state
    } else {
        emitter.instruction("test rdi, rdi");                                   // reject a missing receiver before reading object metadata
        emitter.instruction("jz __rt_object_destructor_boundary_ret");          // return immediately for a null receiver
        emitter.instruction(&format!("test DWORD PTR [rdi - 12], {RUNNING_MASK:#x}")); // inspect a destructor already running on this receiver
        emitter.instruction("jnz __rt_object_destructor_boundary_ret");         // leave a reentrant call unmarked for its active owner
        emitter.instruction(&format!("test QWORD PTR [rdi - 8], {FINISHED_MASK:#x}")); // inspect persistent destructor-called or suppression state
        emitter.instruction("jnz __rt_object_destructor_boundary_ret");         // completed and failed construction must not invoke PHP destruction
        emitter.instruction(&format!("or QWORD PTR [rdi - 8], {FINISHED_MASK:#x}")); // record invocation before a callback can escape
        emitter.instruction("jmp __rt_call_object_destructor_body");            // reuse the existing protected caller and native/eval dispatch
    }
    emitter.label("__rt_object_destructor_boundary_ret");
    emitter.instruction("ret");                                                 // leave reclamation to current owners and the collector's fresh root scan
}

/// Loads the receiver refcount and branches when the destructor is already running.
pub fn emit_skip_running(emitter: &mut impl Emitter, running: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr w9, [x0, #-12]");                          // read the receiver refcount before dispatch
            emitter.instruction(&format!("tbnz w9, #{RUNNING_BIT}, {running}")); // skip a reentrant invocation
        }
        Arch::X86_64 => {
            emitter.instruction("mov eax, DWORD PTR [rdi - 12]");               // read the receiver refcount before dispatch
            emitter.instruction(&format!("test eax, {RUNNING_MASK:#x}"));       // check the refcount's running bit
            emitter.instruction(&format!("jnz {running}"));                     // skip a reentrant invocation
        }
    }
}

/// Sets the running bit, reusing the refcount from emit_skip_running when still live.
pub fn emit_mark_running(emitter: &mut impl Emitter, refcount_loaded: bool) {
    emit_update_running(emitter, refcount_loaded, true);
}

/// Reloads and clears the running bit after an unhandled eval dispatch.
pub fn emit_clear_running(emitter: &mut impl Emitter) {
    emit_update_running(emitter, false, false);
}

/// Changes only the running bit of the current receiver.
fn emit_update_running(emitter: &mut impl Emitter, refcount_loaded: bool, running: bool) {
    match emitter.arch() {
        Arch::AArch64 => {
            if !refcount_loaded {
                emitter.instruction("ldr w9, [x0, #-12]");                      // reload the current receiver refcount
            }
            emitter.instruction(&format!("movz w12, #{:#x}, lsl #16", RUNNING_MASK >> 16)); // materialize the running mask
            emitter.instruction(if running { "orr w9, w9, w12" } else { "bic w9, w9, w12" }); // preserve the ordinary owner count
            emitter.instruction("str w9, [x0, #-12]");                          // publish the updated refcount word
        }
        Arch::X86_64 => {
            if !refcount_loaded {
                emitter.instruction("mov eax, DWORD PTR [rdi - 12]");           // reload the current receiver refcount
            }
            if running {
                emitter.instruction(&format!("or eax, {RUNNING_MASK:#x}"));     // mark the destructor running
            } else {
                emitter.instruction(&format!("and eax, {:#x}", !RUNNING_MASK)); // clear only the running bit
            }
            emitter.instruction("mov DWORD PTR [rdi - 12], eax");               // publish the updated refcount word
        }
    }
}

/// Sets the persistent suppression flag on a present failed-constructor owner.
pub fn emit_suppress_unconstructed(emitter: &mut impl Emitter, done: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {done}"));                    // avoid accessing a null owner
            emitter.instruction("ldr x10, [x0, #-8]");                          // preserve the heap kind and other flags
            emitter.instruction(&format!("orr x10, x10, #{FINISHED_MASK:#x}")); // suppress future destructor dispatch
            emitter.instruction("str x10, [x0, #-8]");                          // publish the persistent suppression flag
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // check whether the guard still owns an object
            emitter.instruction(&format!("jz {done}"));                         // avoid accessing a null owner
            emitter.instruction(&format!("or QWORD PTR [rax - 8], {FINISHED_MASK:#x}")); // suppress future destructor dispatch
        }
    }
    emitter.label(done);
}
