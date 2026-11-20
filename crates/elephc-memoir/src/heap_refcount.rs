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

/// Marks an owned string whose identity lease must be retired during heap release.
pub fn emit_mark_string_identity(emitter: &mut (impl AssemblyEmitter + TargetAbi), pointer: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr w9, [{pointer}, #-8]"));          // load the allocation kind word before registering identity metadata
            emitter.instruction(&format!("mov w10, #{}", crate::layout::MBSTRING_IDENTITY_TRACKED)); // materialize the tracked-identity header flag
            emitter.instruction("orr w9, w9, w10");                             // mark this allocation as carrying mbstring identity metadata
            emitter.instruction(&format!("str w9, [{pointer}, #-8]"));          // publish the flag so the free path can skip untracked strings
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("or QWORD PTR [{pointer} - 8], 0x{:x}", crate::layout::MBSTRING_IDENTITY_TRACKED)); // mark this allocation as carrying mbstring identity metadata
        }
    }
}

/// Returns whether the known cell has an owner in addition to the caller's retained pin.
pub fn emit_has_other_owner(emitter: &mut (impl AssemblyEmitter + TargetAbi), done: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {done}"));                    // an absent cell cannot have another owner
            emitter.instruction("ldr w9, [x0, #-12]");                          // read the uniform heap reference count
            emitter.instruction("cmp w9, #1");                                  // the pending write owns one retained cell reference
            emitter.instruction("cset x0, hi");                                 // another owner keeps the selected parent attached
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // an absent cell cannot have another owner
            emitter.instruction(&format!("jz {done}"));                         // retain the zero result for an absent cell
            emitter.instruction("cmp DWORD PTR [rax - 12], 1");                 // compare the uniform heap reference count with the pending pin
            emitter.instruction("seta al");                                     // another owner keeps the selected parent attached
            emitter.instruction("movzx eax, al");                               // materialize the complete PHP boolean result
        }
    }
}

/// Branches when only the current owner holds the known allocation.
pub fn emit_branch_if_at_most_one_owner(emitter: &mut (impl AssemblyEmitter + TargetAbi), done: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr w9, [x0, #-12]");                          // load the allocation refcount
            emitter.instruction("cmp w9, #1");                                  // a sole owner has no alias to preserve
            emitter.instruction(&format!("b.ls {done}"));                       // skip the shared-owner path
        }
        Arch::X86_64 => {
            emitter.instruction("cmp DWORD PTR [rax - 12], 1");                 // compare the allocation refcount against one
            emitter.instruction(&format!("jbe {done}"));                        // skip the shared-owner path
        }
    }
}
