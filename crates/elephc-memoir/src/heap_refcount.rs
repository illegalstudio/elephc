//! Purpose:
//! Owns inline native heap header operations used at compiler boundaries.
//!
//! Called from:
//! - Eval object boxing and in-place boxed integer updates.
//!
//! Key details:
//! - Post-boxing release requires another live owner.
//! - The in-place update keeps its previously loaded unique count across arithmetic.

use crate::{Arch, AssemblyEmitter, TargetAbi};

/// Drops the original allocation owner after a fresh boxed owner has retained it.
pub fn emit_drop_after_boxing(emitter: &mut (impl AssemblyEmitter + TargetAbi)) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr w9, [x0, #-12]");                          // load the raw object refcount after Mixed boxing retained it
            emitter.instruction("sub w9, w9, #1");                              // consume the allocator-owned object reference locally
            emitter.instruction("str w9, [x0, #-12]");                          // leave the boxed Mixed as the sole object owner
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10d, DWORD PTR [rax - 12]");              // load the raw object refcount after Mixed boxing retained it
            emitter.instruction("sub r10d, 1");                                 // consume the allocator-owned object reference locally
            emitter.instruction("mov DWORD PTR [rax - 12], r10d");              // leave the boxed Mixed as the sole object owner
        }
    }
}

/// Loads the candidate's refcount and branches unless it is uniquely owned.
///
/// The candidate must be a managed cell. Leaves the count in w12 or r10d for later retention.
pub fn emit_require_unique_cell(emitter: &mut (impl AssemblyEmitter + TargetAbi), fallback: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr w12, [x0, #-12]");                         // load the heap block's reference count
            emitter.instruction("cmp w12, #1");                                 // is this the sole owner of the cell?
            emitter.instruction(&format!("b.ne {fallback}"));                   // shared cells must not be mutated through one alias
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10d, DWORD PTR [rax - 12]");              // load the heap block's reference count
            emitter.instruction("cmp r10d, 1");                                 // is this the sole owner of the cell?
            emitter.instruction(&format!("jne {fallback}"));                    // shared cells must not be mutated through one alias
        }
    }
}

/// Writes the staged integer into a unique cell and publishes an additional result owner.
///
/// The caller supplies the new payload in x10 or r8 and the count from the uniqueness check.
pub fn emit_commit_unique_integer(emitter: &mut (impl AssemblyEmitter + TargetAbi)) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("str x10, [x0, #8]");                           // publish the integer payload
            emitter.instruction("add w12, w12, #1");                            // retain one reference for the owned result
            emitter.instruction("str w12, [x0, #-12]");                         // record the retained result owner
        }
        Arch::X86_64 => {
            emitter.instruction("mov QWORD PTR [rax + 8], r8");                 // publish the integer payload
            emitter.instruction("add r10d, 1");                                 // retain one reference for the owned result
            emitter.instruction("mov DWORD PTR [rax - 12], r10d");              // record the retained result owner
        }
    }
}
