//! Purpose:
//! Emits storage preparation for boxed indexed and associative arrays.
//!
//! Called from:
//! - Compiler array mutation and fetch-for-write emitters.
//!
//! Key details:
//! - The emitted helpers build, separate and publish storage while callers select the write semantics.

use crate::{abi, Arch, Emitter};
use crate::layout::{emit_branch_if_null_container, x86_64_heap_kind_word, MIXED_HEAP_KIND};

/// Emits empty-cell construction and hash payload preparation in runtime order.
pub fn emit_storage_helpers(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emit_mixed_new_empty_array_cell_aarch64(emitter);
            emit_mixed_cell_promote_to_hash_aarch64(emitter);
        }
        Arch::X86_64 => {
            emit_mixed_new_empty_array_cell_x86_64(emitter);
            emit_mixed_cell_promote_to_hash_x86_64(emitter);
        }
    }
}

/// Accepts an array cell in the first C argument and consumes its old owner only when splitting.
pub fn emit_cell_ensure_unique(emitter: &mut impl Emitter) {
    let arg = abi::int_arg_reg_name(emitter, 0);
    let result = abi::int_result_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_array_cell_ensure_unique");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, arg, 8);
    abi::emit_reg_move(emitter, result, arg);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("cbz x0, __rt_array_cell_invalid");             // null is not a boxed PHP array
            emitter.instruction("ldr x9, [x0]");                                // validate the cell tag before reading allocation metadata
            emitter.instruction("sub x9, x9, #4");                              // array and hash tags become zero and one
            emitter.instruction("cmp x9, #1");                                  // only the two PHP array layouts can be mutated
            emitter.instruction("b.hi __rt_array_cell_invalid");                // reject scalar, object and resource cells
            emitter.instruction("ldr w9, [x0, #-12]");                          // inspect the outer cell's owner count
            emitter.instruction("cmp w9, #1");                                  // unique cells can retain their address
            emitter.instruction("b.ls __rt_array_cell_unique");                 // skip allocation when no value alias shares this cell
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // null is not a boxed PHP array
            emitter.instruction("jz __rt_array_cell_invalid");                  // reject null before dereferencing its tag
            emitter.instruction("mov r10, QWORD PTR [rax]");                    // validate the cell tag before reading allocation metadata
            emitter.instruction("sub r10, 4");                                  // array and hash tags become zero and one
            emitter.instruction("cmp r10, 1");                                  // only the two PHP array layouts can be mutated
            emitter.instruction("ja __rt_array_cell_invalid");                  // reject scalar, object and resource cells
            emitter.instruction("cmp DWORD PTR [rax - 12], 1");                 // inspect the outer cell's owner count
            emitter.instruction("jbe __rt_array_cell_unique");                  // skip allocation when no value alias shares this cell
        }
    }
    abi::emit_call_label(emitter, "__rt_mixed_clone");
    // A split can retire the final external owner of a cyclic old cell.
    abi::store_at_offset(emitter, result, 16);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    abi::load_at_offset(emitter, result, 16);
    abi::emit_jump(emitter, "__rt_array_cell_unique");
    emitter.label("__rt_array_cell_invalid");
    abi::emit_load_int_immediate(emitter, result, 0);
    emitter.label("__rt_array_cell_unique");
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

/// Emits `__rt_mixed_cell_autovivify_array` for the active target.
pub fn emit_mixed_cell_autovivify_array(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_mixed_cell_autovivify_array_x86_64(emitter);
        return;
    }
    emit_mixed_cell_autovivify_array_aarch64(emitter);
}

/// Emits the AArch64 helper that installs a fresh empty indexed array into a Mixed cell.
fn emit_mixed_cell_autovivify_array_aarch64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cell_autovivify_array ---");
    emitter.label_global("__rt_mixed_cell_autovivify_array");

    emitter.instruction("sub sp, sp, #32");                                     // reserve the receiver slot and saved frame registers
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the existing Mixed cell across array allocation
    emitter.instruction("mov x0, #0");                                          // request a zero-capacity indexed array (grown on demand)
    emitter.instruction("mov x1, #8");                                          // autovivified arrays store boxed Mixed pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate the fresh empty indexed array
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the existing Mixed cell
    emitter.instruction("mov x10, #4");                                         // runtime value tag 4 = indexed-array payload
    emitter.instruction("str x10, [x9]");                                       // retag the receiver cell as an indexed array
    emitter.instruction("str x0, [x9, #8]");                                    // transfer the fresh array reference into the cell
    emitter.instruction("str xzr, [x9, #16]");                                  // clear the unused high payload word
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the installed array pointer in x0
}

/// Emits the x86_64 helper that installs a fresh empty indexed array into a Mixed cell.
fn emit_mixed_cell_autovivify_array_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cell_autovivify_array ---");
    emitter.label_global("__rt_mixed_cell_autovivify_array");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 16");                                         // reserve the receiver slot while keeping call alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the existing Mixed cell across array allocation
    emitter.instruction("xor edi, edi");                                        // request a zero-capacity indexed array (grown on demand)
    emitter.instruction("mov rsi, 8");                                          // autovivified arrays store boxed Mixed pointers
    emitter.instruction("call __rt_array_new");                                 // allocate the fresh empty indexed array
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the existing Mixed cell
    emitter.instruction("mov QWORD PTR [r10], 4");                              // retag the receiver cell as an indexed array
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // transfer the fresh array reference into the cell
    emitter.instruction("mov QWORD PTR [r10 + 16], 0");                         // clear the unused high payload word
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the installed array pointer in rax
}

/// Emits `__rt_mixed_new_empty_array_cell` for ARM64.
///
/// Output: `x0` = fresh boxed Mixed cell (tag 4) owning a fresh empty indexed
/// array. The cell starts with refcount 1; the array's initial reference is
/// transferred to the cell (no extra retain, unlike `__rt_mixed_from_value`).
fn emit_mixed_new_empty_array_cell_aarch64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_new_empty_array_cell ---");
    emitter.label_global("__rt_mixed_new_empty_array_cell");

    emitter.instruction("sub sp, sp, #32");                                     // reserve frame for the fresh array pointer and saved fp/lr
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer
    emitter.instruction("mov x0, #0");                                          // request a zero-capacity indexed array (grown on demand)
    emitter.instruction("mov x1, #8");                                          // autovivified arrays use 8-byte slots (boxed cells fit)
    emitter.instruction("bl __rt_array_new");                                   // allocate the empty indexed array (refcount 1)
    emitter.instruction("str x0, [sp, #0]");                                    // save the fresh array across the cell allocation
    emitter.instruction("mov x0, #24");                                         // mixed cells store tag plus two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the mixed cell storage (refcount 1)
    emitter.instruction("mov x9, #5");                                          // low byte 5 = mixed cell heap kind
    emitter.instruction("str x9, [x0, #-8]");                                   // install the mixed-cell heap kind in the uniform header
    emitter.instruction("mov x9, #4");                                          // runtime value tag 4 = indexed array payload
    emitter.instruction("str x9, [x0]");                                        // store the indexed-array tag at mixed[0]
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the fresh empty array
    emitter.instruction("str x9, [x0, #8]");                                    // transfer the array's initial reference into the cell payload
    emitter.instruction("str xzr, [x0, #16]");                                  // clear the unused high payload word
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the fresh boxed array cell in x0
}

/// Emits `__rt_mixed_cell_promote_to_hash` for ARM64.
///
/// Input:  `x0` = boxed Mixed cell with an indexed (tag 4) or hash (tag 5) payload.
/// Output: `x0` = the installed/borrowed hash payload, or zero for an invalid receiver.
///
/// Tag 4 builds a fresh hash from the indexed entries (`__rt_array_to_hash`
/// retains payloads, `__rt_hash_to_mixed` boxes raw scalar entries), installs
/// it, and releases the replaced indexed payload once. Tag 5 ensures its hash
/// is unique, normalizes raw typed entries to Mixed cells, and republishes that
/// result in the cell before returning its borrow.
fn emit_mixed_cell_promote_to_hash_aarch64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cell_promote_to_hash ---");
    emitter.label_global("__rt_mixed_cell_promote_to_hash");

    emitter.instruction("sub sp, sp, #48");                                     // reserve frame for cell, source array, hash, and saved fp/lr
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("cbz x0, __rt_mixed_cell_promote_to_hash_invalid");     // reject a missing boxed cell before reading its tag
    emitter.instruction("str x0, [sp, #0]");                                    // save the boxed Mixed cell being promoted
    emitter.instruction("ldr x9, [x0]");                                        // load the runtime payload tag before selecting its ownership path
    emitter.instruction("cmp x9, #5");                                          // is the cell already an associative array?
    emitter.instruction("b.eq __rt_mixed_cell_promote_to_hash_hash");           // ensure and republish an installed hash before returning its borrow
    emitter.instruction("cmp x9, #4");                                          // is the cell an indexed array eligible for promotion?
    emitter.instruction("b.ne __rt_mixed_cell_promote_to_hash_invalid");        // scalars and null cells are invalid key-sort receivers
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the indexed-array payload from the tag-4 cell
    emit_branch_if_null_container(
        emitter,
        "x0",
        "x9",
        "__rt_mixed_cell_promote_to_hash_invalid",
    );
    emitter.instruction("str x0, [sp, #8]");                                    // save the source array for the release after the copy
    emitter.instruction("bl __rt_array_to_hash");                               // build an owned hash from the indexed entries (payloads retained)
    emitter.instruction("bl __rt_hash_to_mixed");                               // box any raw scalar entries so the hash holds uniform Mixed cells
    emitter.instruction("str x0, [sp, #16]");                                   // save the promoted hash across the source release
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the boxed Mixed cell
    emitter.instruction("mov x10, #5");                                         // runtime value tag 5 = associative array payload
    emitter.instruction("str x10, [x9]");                                       // retag the cell as an associative payload
    emitter.instruction("str x0, [x9, #8]");                                    // install the promoted hash into the cell payload
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the replaced indexed payload
    emitter.instruction("bl __rt_decref_array");                                // release the cell's reference to the replaced indexed array
    emitter.instruction("ldr x0, [sp, #16]");                                   // return the promoted hash pointer
    emitter.instruction("b __rt_mixed_cell_promote_to_hash_done");              // skip the idempotent and invalid return paths after promotion

    emitter.label("__rt_mixed_cell_promote_to_hash_hash");
    emitter.instruction("ldr x0, [x0, #8]");                                    // borrow the already-installed hash payload from the tag-5 cell
    emit_branch_if_null_container(
        emitter,
        "x0",
        "x9",
        "__rt_mixed_cell_promote_to_hash_invalid",
    );
    emitter.instruction("bl __rt_hash_to_mixed");                               // COW-split and normalize entries while consuming the cell's old owner
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the Mixed cell clobbered by the COW helper call
    emitter.instruction("str x0, [x9, #8]");                                    // republish the unique normalized hash into the child cell
    emitter.instruction("b __rt_mixed_cell_promote_to_hash_done");              // return the borrow from the cell's current hash owner

    emitter.label("__rt_mixed_cell_promote_to_hash_invalid");
    emitter.instruction("mov x0, xzr");                                         // signal an invalid/missing Mixed array receiver to the EIR guard

    emitter.label("__rt_mixed_cell_promote_to_hash_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the installed hash in x0
}

/// Emits `__rt_mixed_new_empty_array_cell` for x86_64 (same contract as ARM64).
fn emit_mixed_new_empty_array_cell_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_new_empty_array_cell ---");
    emitter.label_global("__rt_mixed_new_empty_array_cell");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 16");                                         // reserve one slot for the fresh array pointer
    emitter.instruction("mov rdi, 0");                                          // request a zero-capacity indexed array (grown on demand)
    emitter.instruction("mov rsi, 8");                                          // autovivified arrays use 8-byte slots (boxed cells fit)
    emitter.instruction("call __rt_array_new");                                 // allocate the empty indexed array (refcount 1)
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the fresh array across the cell allocation
    emitter.instruction("mov rax, 24");                                         // mixed cells store tag plus two payload words
    emitter.instruction("call __rt_heap_alloc");                                // allocate the mixed cell storage (refcount 1)
    emitter.instruction(&format!("mov r10, 0x{:x}", x86_64_heap_kind_word(MIXED_HEAP_KIND))); // materialize the mixed-cell heap kind word with the x86_64 marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // install the mixed-cell heap kind in the uniform header
    emitter.instruction("mov QWORD PTR [rax], 4");                              // store the indexed-array tag at mixed[0]
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the fresh empty array
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // transfer the array's initial reference into the cell payload
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // clear the unused high payload word
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the fresh boxed array cell in rax
}

/// Emits `__rt_mixed_cell_promote_to_hash` for x86_64.
///
/// Input: `rdi` = boxed Mixed cell with an indexed (tag 4) or hash (tag 5) payload.
/// Output: `rax` = the installed/borrowed hash payload, or zero for an invalid receiver.
fn emit_mixed_cell_promote_to_hash_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cell_promote_to_hash ---");
    emitter.label_global("__rt_mixed_cell_promote_to_hash");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve slots for cell, source array, and promoted hash
    emitter.instruction("test rdi, rdi");                                       // reject a missing boxed cell before reading its tag
    emitter.instruction("jz __rt_mixed_cell_promote_to_hash_invalid");          // route null cells to the controlled EIR type-error guard
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the boxed Mixed cell being promoted
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the runtime payload tag before selecting its ownership path
    emitter.instruction("cmp r10, 5");                                          // is the cell already an associative array?
    emitter.instruction("je __rt_mixed_cell_promote_to_hash_hash");             // ensure and republish an installed hash before returning its borrow
    emitter.instruction("cmp r10, 4");                                          // is the cell an indexed array eligible for promotion?
    emitter.instruction("jne __rt_mixed_cell_promote_to_hash_invalid");         // scalars and null cells are invalid key-sort receivers
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // load the indexed-array payload from the tag-4 cell
    emit_branch_if_null_container(
        emitter,
        "rdi",
        "r10",
        "__rt_mixed_cell_promote_to_hash_invalid",
    );
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the source array for the release after the copy
    emitter.instruction("call __rt_array_to_hash");                             // build an owned hash from the indexed entries (payloads retained)
    emitter.instruction("mov rdi, rax");                                        // pass the fresh hash to the Mixed-entry conversion helper
    emitter.instruction("call __rt_hash_to_mixed");                             // box any raw scalar entries so the hash holds uniform Mixed cells
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the promoted hash across the source release
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the boxed Mixed cell
    emitter.instruction("mov QWORD PTR [r10], 5");                              // retag the cell as an associative payload
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // install the promoted hash into the cell payload
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the replaced indexed payload
    emitter.instruction("call __rt_decref_array");                              // release the cell's reference to the replaced indexed array
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the promoted hash pointer
    emitter.instruction("jmp __rt_mixed_cell_promote_to_hash_done");            // skip the idempotent and invalid return paths after promotion

    emitter.label("__rt_mixed_cell_promote_to_hash_hash");
    emitter.instruction("mov rax, QWORD PTR [rdi + 8]");                        // borrow the already-installed hash payload from the tag-5 cell
    emit_branch_if_null_container(
        emitter,
        "rax",
        "r10",
        "__rt_mixed_cell_promote_to_hash_invalid",
    );
    emitter.instruction("mov rdi, rax");                                        // pass the installed child hash to the Mixed-entry conversion helper
    emitter.instruction("call __rt_hash_to_mixed");                             // COW-split and normalize entries while consuming the cell's old owner
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the Mixed cell clobbered by the COW helper call
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // republish the unique normalized hash into the child cell
    emitter.instruction("jmp __rt_mixed_cell_promote_to_hash_done");            // return the borrow from the cell's current hash owner

    emitter.label("__rt_mixed_cell_promote_to_hash_invalid");
    emitter.instruction("xor rax, rax");                                        // signal an invalid/missing Mixed array receiver to the EIR guard

    emitter.label("__rt_mixed_cell_promote_to_hash_done");
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the installed hash in rax
}
