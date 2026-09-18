//! Purpose:
//! Emits the read-only C-ABI query that tells Magician whether one entry of a
//! natively built array still belongs to a live PHP reference set.
//!
//! Called from:
//! - `crate::codegen_support::runtime::eval_bridge::emit_eval_value_runtime()`, once per program.
//!
//! Key details:
//! - `clone($object, $withProperties)` on an eval-owned object is applied by Magician, whose
//!   own array-element alias table only knows arrays eval itself built. An override hash built
//!   in generated code carries its reference state in the hash entry instead, and this query is
//!   how Magician reads it. Without it a by-reference override entry would be applied silently.
//! - The predicate is the one the generated clone-override applicator already uses
//!   (`codegen/lower_inst/builtins/clone_with/overrides.rs`): runtime value tag 11 identifies a
//!   reference entry, and its `value_lo` managed reference cell answers "shared" when more than
//!   one owner holds it. The entry itself owns one count; a live local alias or a second entry in
//!   the same reference set owns another.
//! - The caller's borrowed entry value retains the boxed Mixed value INSIDE the cell, not the
//!   cell, so there is nothing to discount from the owner count.
//! - Every input is BORROWED. The query allocates nothing, retains nothing, releases nothing,
//!   writes no output slot, and cannot throw, so it needs no status/output split.
//! - Both supported architectures define the same symbol with the same argument order.

use super::*;

/// Runtime tag marking a hash entry whose `value_lo` is a managed reference cell.
const REFERENCE_CELL_VALUE_TAG: i64 = 11;

/// Runtime tag of an associative array payload inside a boxed Mixed cell.
const ASSOC_ARRAY_TAG: i64 = 5;

/// Emits `__elephc_eval_array_entry_is_shared_reference_v1` for the active target.
///
/// Arguments, in C order: the borrowed boxed override array, the entry key bytes and their
/// length, and the borrowed boxed entry value the caller already holds (or null). Returns one
/// when that entry still belongs to a live PHP reference set and zero otherwise.
pub(super) fn emit_array_entry_reference_query(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: eval array entry reference query ---");
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the ARM64 query body.
fn emit_aarch64(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_array_entry_is_shared_reference_v1");
    emitter.instruction("sub sp, sp, #48");                                     // frame for the saved inputs and the hash pointer
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address before nested calls
    emitter.instruction("add x29, sp, #32");                                    // establish the query frame
    emitter.instruction("str x3, [sp, #0]");                                    // save the caller's borrowed boxed entry value
    emitter.instruction("str x1, [sp, #8]");                                    // save the entry key bytes pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save the entry key byte length
    emitter.instruction("cbz x0, __elephc_eval_array_entry_ref_no");            // a null array box carries no entry state
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0 = payload tag, x1 = payload low word
    emitter.instruction(&format!("cmp x0, #{ASSOC_ARRAY_TAG}"));                // only associative arrays own hash entries
    emitter.instruction("b.ne __elephc_eval_array_entry_ref_no");               // every other shape is an ordinary by-value override
    emitter.instruction("str x1, [sp, #24]");                                   // save the hash table pointer across key normalization
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = key bytes pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = key byte length
    emitter.instruction("bl __rt_hash_normalize_key");                          // x1/x2 = normalized key low/high words
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = the hash table to probe
    emitter.instruction("bl __rt_hash_get");                                    // x4 = matching entry address, zero on a miss
    emitter.instruction("cbz x4, __elephc_eval_array_entry_ref_no");            // a key with no entry of its own carries no reference metadata
    emitter.instruction("mov x6, x4");                                          // hold the entry address across the payload loads
    emitter.instruction("ldr x3, [x6, #24]");                                   // x3 = value_lo, the entry's managed reference cell when tagged 11
    emitter.instruction("ldr x5, [x6, #40]");                                   // x5 = the entry's value tag
    emitter.instruction(&format!("cmp x5, #{REFERENCE_CELL_VALUE_TAG}"));       // is this entry a member of a PHP reference set?
    emitter.instruction("b.ne __elephc_eval_array_entry_ref_no");               // ordinary entry values are by-value overrides
    emitter.instruction("ldr w10, [x3, #-12]");                                 // load the shared reference cell's owner count
    emitter.instruction("cmp w10, #1");                                         // does anything besides this entry still own the cell?
    emitter.instruction("b.hi __elephc_eval_array_entry_ref_yes");              // shared cell ownership preserves PHP reference identity
    emitter.label("__elephc_eval_array_entry_ref_no");
    emitter.instruction("mov x0, xzr");                                         // report an ordinary by-value override entry
    emitter.instruction("b __elephc_eval_array_entry_ref_ret");                 // fall into the shared epilogue
    emitter.label("__elephc_eval_array_entry_ref_yes");
    emitter.instruction("mov x0, #1");                                          // report a live PHP reference set
    emitter.label("__elephc_eval_array_entry_ref_ret");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the query frame
    emitter.instruction("ret");                                                 // return the borrowed-only answer to Rust
}

/// Emits the x86_64 query body.
fn emit_x86_64(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_array_entry_is_shared_reference_v1");
    emitter.instruction("push rbp");                                            // save the caller frame pointer and realign the stack
    emitter.instruction("mov rbp, rsp");                                        // establish the query frame
    emitter.instruction("sub rsp, 48");                                         // reserve spill slots for the saved inputs
    emitter.instruction("mov QWORD PTR [rbp - 8], rcx");                        // save the caller's borrowed boxed entry value
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the entry key bytes pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the entry key byte length
    emitter.instruction("test rdi, rdi");                                       // a null array box carries no entry state
    emitter.instruction("jz __elephc_eval_array_entry_ref_no_x86");             // report an ordinary override for a missing container
    emitter.instruction("mov rax, rdi");                                        // the unbox helper takes its boxed input in rax
    emitter.instruction("call __rt_mixed_unbox");                               // rax = payload tag, rdi = payload low word
    emitter.instruction(&format!("cmp rax, {ASSOC_ARRAY_TAG}"));                // only associative arrays own hash entries
    emitter.instruction("jne __elephc_eval_array_entry_ref_no_x86");            // every other shape is an ordinary by-value override
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the hash table pointer across key normalization
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // rax = key bytes pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // rdx = key byte length
    emitter.instruction("call __rt_hash_normalize_key");                        // rax/rdx = normalized key low/high words
    emitter.instruction("mov rsi, rax");                                        // move the normalized key low word into the lookup ABI register
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // rdi = the hash table to probe
    emitter.instruction("call __rt_hash_get");                                  // r8 = matching entry address, zero on a miss
    emitter.instruction("test r8, r8");                                         // a key with no entry of its own carries no reference metadata
    emitter.instruction("jz __elephc_eval_array_entry_ref_no_x86");             // absent keys are ordinary by-value overrides
    emitter.instruction("mov r10, r8");                                         // hold the entry address across the payload loads
    emitter.instruction("mov rcx, QWORD PTR [r10 + 24]");                       // rcx = value_lo, the entry's managed reference cell when tagged 11
    emitter.instruction("mov r9, QWORD PTR [r10 + 40]");                        // r9 = the entry's value tag
    emitter.instruction(&format!("cmp r9, {REFERENCE_CELL_VALUE_TAG}"));        // is this entry a member of a PHP reference set?
    emitter.instruction("jne __elephc_eval_array_entry_ref_no_x86");            // ordinary entry values are by-value overrides
    emitter.instruction("mov r10d, DWORD PTR [rcx - 12]");                      // load the shared reference cell's owner count
    emitter.instruction("cmp r10d, 1");                                         // does anything besides this entry still own the cell?
    emitter.instruction("ja __elephc_eval_array_entry_ref_yes_x86");            // shared cell ownership preserves PHP reference identity
    emitter.label("__elephc_eval_array_entry_ref_no_x86");
    emitter.instruction("xor eax, eax");                                        // report an ordinary by-value override entry
    emitter.instruction("jmp __elephc_eval_array_entry_ref_ret_x86");           // fall into the shared epilogue
    emitter.label("__elephc_eval_array_entry_ref_yes_x86");
    emitter.instruction("mov eax, 1");                                          // report a live PHP reference set
    emitter.label("__elephc_eval_array_entry_ref_ret_x86");
    emitter.instruction("mov rsp, rbp");                                        // discard the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the borrowed-only answer to Rust
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The query keeps one C symbol, both refusal sources, and its hash probe on every target.
    #[test]
    fn eval_array_entry_reference_query_is_symmetric_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = crate::codegen_support::platform::Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_array_entry_reference_query(&mut emitter);
            let output = emitter.output();
            let symbol = target.extern_symbol("__elephc_eval_array_entry_is_shared_reference_v1");
            assert_eq!(output.matches(&format!("{symbol}:")).count(), 1, "{name}");
            // The query must reach the real hash entry rather than guess from the container.
            assert!(output.contains("__rt_mixed_unbox"), "{name}");
            assert!(output.contains("__rt_hash_normalize_key"), "{name}");
            assert!(output.contains("__rt_hash_get"), "{name}");
            // Both halves of the refusal predicate have to survive on both architectures:
            // the tag-11 reference-entry test and the shared-cell owner-count test.
            let (reference_tag_branch, shared_owner_branch) = match target.arch {
                Arch::AArch64 => (
                    "cmp x5, #11",
                    "b.hi __elephc_eval_array_entry_ref_yes",
                ),
                Arch::X86_64 => ("cmp r9, 11", "ja __elephc_eval_array_entry_ref_yes_x86"),
            };
            assert!(output.contains(reference_tag_branch), "{name}");
            assert!(output.contains(shared_owner_branch), "{name}");
            // A pure read must not allocate, retain, release, or throw.
            for forbidden in [
                "__rt_heap_alloc",
                "__rt_incref",
                "__rt_mixed_release",
                "__rt_throw_current",
            ] {
                assert!(!output.contains(forbidden), "{name}: {forbidden}");
            }
        }
    }
}
