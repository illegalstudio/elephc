//! Purpose:
//! Owns inline native heap header operations used at compiler boundaries.
//!
//! Called from:
//! - Eval object boxing and in-place boxed integer updates.
//!
//! Key details:
//! - Post-boxing release requires another live owner.
//! - The in-place update keeps its previously loaded unique count across arithmetic.

use crate::{Arch, AssemblyEmitter, Emitter, TargetAbi};

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

/// Compares physical box and array owners after discounting the compiler's pending leases.
pub fn emit_boxed_array_liveness(
    emitter: &mut impl Emitter, payload: &str, live: &str, done: &str,
) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {live}"));                    // non-array absent cells keep their existing update semantics
            emitter.instruction("ldr x9, [x0]");                                // inspect the canonical PHP cell tag
            emitter.instruction("cmp x9, #4");                                  // tag four owns an indexed array payload
            emitter.instruction(&format!("b.eq {payload}"));                    // indexed arrays use the shared payload owner check
            emitter.instruction("cmp x9, #5");                                  // tag five owns an associative array payload
            emitter.instruction(&format!("b.ne {live}"));                       // do not cancel object or scalar offset behavior here
            emitter.label(payload);
            emitter.instruction("ldr w10, [x0, #-12]");                         // one cell owner is the pending compiler lease
            emitter.instruction("ldr x9, [x0, #8]");                            // separately boxed copies can retain this array payload
            emitter.instruction("ldr w9, [x9, #-12]");                          // inspect the payload independent owner count
            emitter.instruction("add x9, x9, x10");                             // count every cell and independently boxed payload owner
            emitter.instruction("cmp x9, #3");                                  // discount two compiler-held references and require one PHP owner
            emitter.instruction("cset x0, eq");                                 // multiple payload copies must cancel rather than share the write
            emitter.instruction(&format!("b {done}"));                          // keep the complete liveness result
            emitter.label(live);
            emitter.instruction("mov x0, #1");                                  // preserve live arrays and non-array update semantics
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // recognize a non-array absent cell
            emitter.instruction(&format!("jz {live}"));                         // absent cells retain their existing update semantics
            emitter.instruction("cmp QWORD PTR [rax], 4");                      // tag four owns an indexed array payload
            emitter.instruction(&format!("je {payload}"));                      // indexed arrays share the payload owner check
            emitter.instruction("cmp QWORD PTR [rax], 5");                      // tag five owns an associative array payload
            emitter.instruction(&format!("jne {live}"));                        // do not cancel object or scalar offset behavior here
            emitter.label(payload);
            emitter.instruction("mov r11d, DWORD PTR [rax - 12]");              // one cell owner is the pending compiler lease
            emitter.instruction("mov r10, QWORD PTR [rax + 8]");                // a separately boxed copy can own the same payload
            emitter.instruction("mov r10d, DWORD PTR [r10 - 12]");              // count every independently boxed payload owner
            emitter.instruction("add r11, r10");                                // include both compiler-held references without 32-bit overflow
            emitter.instruction("cmp r11, 3");                                  // require exactly one surviving PHP owner after discounting leases
            emitter.instruction("sete al");                                     // multiple payload copies must cancel rather than share the write
            emitter.instruction("movzx rax, al");                               // return the complete boolean result
            emitter.instruction(&format!("jmp {done}"));                        // keep the complete liveness result
            emitter.label(live);
            emitter.instruction("mov eax, 1");                                  // preserve live arrays and non-array update semantics
        }
    }
    emitter.label(done);
}

/// Returns whether a C ABI persistent reference remains shared after internal pins are discounted.
pub fn emit_logical_reference_sharing(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::AArch64 {
        emitter.instruction("cbz x0, __elephc_eval_reference_unshared");        // null cannot own a PHP reference
        emitter.instruction("ldr x9, [x0]");                                    // inspect the boxed runtime tag
        emitter.instruction("cmp x9, #7");                                      // persistent references use nested Mixed storage
        emitter.instruction("b.ne __elephc_eval_reference_unshared");           // ordinary boxes have no writable reference
        emitter.instruction("ldr x9, [x0, #16]");                               // inspect the persistent reference flag
        emitter.instruction("cmp x9, #1");                                      // flag one identifies a PHP reference
        emitter.instruction("b.ne __elephc_eval_reference_unshared");           // unrelated nested boxes remain ordinary
        emitter.instruction("ldr w9, [x0, #-12]");                              // read the wrapper's physical owner count
        emitter.instruction("ldr x10, [x0, #-8]");                              // inspect an artificial collector pin
        emitter.instruction("ubfx x10, x10, #18, #1");                          // isolate the collector-only owner bit
        emitter.instruction("sub w9, w9, w10");                                 // PHP sharing excludes collector pins
        crate::reference_cells::validation_pins::subtract(
            emitter, "x0", "x9", "x10", "x11", "__elephc_eval_reference_shared",
        );
        emitter.instruction("cmp w9, #1");                                      // one owner separates during array COW
        emitter.instruction("cset x0, hi");                                     // another owner keeps the reference shared
        emitter.instruction("ret");                                             // return the sharing predicate
        emitter.label("__elephc_eval_reference_unshared");
        emitter.instruction("mov x0, #0");                                      // report a singleton or ordinary value
        emitter.instruction("ret");                                             // leave the borrowed wrapper untouched
    } else {
        emitter.instruction("test rdi, rdi");                                   // null cannot own a PHP reference
        emitter.instruction("jz __elephc_eval_reference_unshared");             // skip an absent boxed value
        emitter.instruction("cmp QWORD PTR [rdi], 7");                          // persistent references use nested Mixed storage
        emitter.instruction("jne __elephc_eval_reference_unshared");            // ordinary boxes have no writable reference
        emitter.instruction("cmp QWORD PTR [rdi + 16], 1");                     // inspect the persistent reference flag
        emitter.instruction("jne __elephc_eval_reference_unshared");            // unrelated nested boxes remain ordinary
        emitter.instruction("mov eax, DWORD PTR [rdi - 12]");                   // read the wrapper's physical owner count
        emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                    // inspect an artificial collector pin
        emitter.instruction("shr r10, 18");                                     // position the collector-only owner bit
        emitter.instruction("and r10d, 1");                                     // isolate the collector pin
        emitter.instruction("sub eax, r10d");                                   // PHP sharing excludes collector pins
        crate::reference_cells::validation_pins::subtract(
            emitter, "rdi", "rax", "r10", "r11", "__elephc_eval_reference_shared",
        );
        emitter.instruction("cmp eax, 1");                                      // one owner separates during array COW
        emitter.instruction("seta al");                                         // another owner keeps the reference shared
        emitter.instruction("movzx eax, al");                                   // normalize the sharing predicate
        emitter.instruction("ret");                                             // leave the borrowed wrapper untouched
        emitter.label("__elephc_eval_reference_unshared");
        emitter.instruction("xor eax, eax");                                    // report a singleton or ordinary value
        emitter.instruction("ret");                                             // return the sharing predicate
    }
}

/// Returns the raw or flag-masked native owner count for a live C pointer.
pub fn emit_c_owner_count(emitter: &mut impl Emitter, name: &str, mask_high_bit: bool) {
    emitter.label_global(&emitter.extern_symbol(name));
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr w0, [x0, #-12]");                          // read the compact native refcount
            if mask_high_bit {
                emitter.instruction("and w0, w0, #0x7fffffff");                 // ignore the header's top flag bit
            }
        }
        Arch::X86_64 => {
            emitter.instruction("mov eax, DWORD PTR [rdi - 12]");               // read the compact native refcount
            if mask_high_bit {
                emitter.instruction("and eax, 0x7fffffff");                     // ignore the header's top flag bit
            }
        }
    }
    emitter.emit_return();
}
