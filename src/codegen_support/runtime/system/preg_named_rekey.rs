//! Purpose:
//! Emits `__rt_preg_rekey_named`, which gives a `preg_match_all()` result its named-group keys
//! and its `MARK` entry, plus the two helpers that read a match's `(*MARK)` name.
//!
//! Called from:
//! - `crate::codegen_support::runtime::system::preg_match_all_capture`, once per SET_ORDER row and
//!   once for the PATTERN_ORDER group table, while the compiled pattern handle is still alive.
//!
//! Key details:
//! - php lists a named group under its name AND its number, the name first:
//!   `preg_match_all('/(?<y>\d+)/', 'a1', $m)` gives `[0 => ['1'], 'y' => ['1'], 1 => ['1']]`.
//!   The capture runtime builds numbered lists only, so a pattern with names is re-keyed here.
//! - A `(*MARK:name)` verb adds a trailing `MARK` key: a SET_ORDER row gets the name of the mark
//!   its match passed, the PATTERN_ORDER table gets `[match number => name]` for the matches that
//!   passed one. php adds it after every group key, so it is inserted last.
//! - Input: the compiled handle, an owned indexed list whose slots are boxed Mixed cells, and an
//!   owned boxed `MARK` value or 0. Output: the list itself when the pattern names no group and
//!   there is no mark, otherwise a NEW owned hash, the list having been released. Callers tell
//!   the two apart by the storage's heap kind.
//! - `__rt_hash_set` takes ownership of the value it stores, so each key gets its own reference:
//!   the numbered key retains the list's cell, the named key gets a detached clone, so a write
//!   through one key can never show through the other.
//! - `pcre2_get_mark()` reports the MOST RECENT match, so the mark helpers run before the next
//!   exec on the handle.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Runtime value tag of a boxed Mixed cell stored in a container slot.
const MIXED_TAG: i64 = 7;

/// Emits `__rt_preg_rekey_named`, `__rt_preg_mark_cell` and `__rt_preg_mark_note`.
pub(crate) fn emit_preg_rekey_named(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_preg_rekey_named_linux_x86_64(emitter);
        emit_preg_mark_helpers_linux_x86_64(emitter);
        return;
    }
    emit_preg_rekey_named_arm64(emitter);
    emit_preg_mark_helpers_arm64(emitter);
}

/// ARM64: x0 = compiled pattern handle, x1 = owned list of boxed cells, x2 = owned boxed `MARK`
/// value or 0; returns x0 = storage.
fn emit_preg_rekey_named_arm64(emitter: &mut Emitter) {
    let handle_off = 0;
    let list_off = 8;
    let hash_off = 16;
    let idx_off = 24;
    let len_off = 32;
    let name_ptr_off = 40;
    let name_len_off = 48;
    let key_lo_off = 56;
    let key_hi_off = 64;
    let cell_off = 72;
    let mark_off = 80;
    let mark_key_off = 88;
    let save_off = 96;
    let stack_size = 112;

    emitter.blank();
    emitter.comment("--- runtime: preg_rekey_named ---");
    emitter.label_global("__rt_preg_rekey_named");
    emitter.instruction(&format!("sub sp, sp, #{}", stack_size));               // reserve the re-keying frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_off));         // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // establish the re-keying frame pointer
    emitter.instruction(&format!("str x0, [sp, #{}]", handle_off));             // keep the compiled handle across helper calls
    emitter.instruction(&format!("str x1, [sp, #{}]", list_off));               // keep the owned numbered list
    emitter.instruction(&format!("str x2, [sp, #{}]", mark_off));               // keep the owned `MARK` value, or 0
    emitter.bl_c("elephc_pcre2_v1_name_count");                                 // does the pattern name any group?
    emitter.instruction("cbnz x0, __rt_preg_rekey_named_build");                // named groups need the keyed hash
    emitter.instruction(&format!("ldr x9, [sp, #{}]", mark_off));               // is there a `MARK` value?
    emitter.instruction("cbnz x9, __rt_preg_rekey_named_build");                // `MARK` is a string key: it needs the hash too
    emitter.instruction(&format!("ldr x0, [sp, #{}]", list_off));               // no names: the numbered list is already php's shape
    emitter.instruction("b __rt_preg_rekey_named_ret");                         // return it unchanged

    emitter.label("__rt_preg_rekey_named_build");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", list_off));               // reload the numbered list
    emitter.instruction("ldr x0, [x9]");                                        // load its length
    emitter.instruction(&format!("str x0, [sp, #{}]", len_off));                // keep the length for the loop bound
    emitter.instruction("lsl x0, x0, #1");                                      // one bucket per number and one per name
    emitter.instruction("mov x9, #16");                                         // minimum hash capacity
    emitter.instruction("cmp x0, x9");                                          // is the doubled length below the minimum?
    emitter.instruction("csel x0, x9, x0, lt");                                 // clamp the capacity to the minimum
    emitter.instruction(&format!("mov x1, #{}", MIXED_TAG));                    // the hash stores boxed Mixed cells
    emitter.instruction("bl __rt_hash_new");                                    // allocate the keyed result hash
    emitter.instruction(&format!("str x0, [sp, #{}]", hash_off));               // keep the hash across inserts
    emitter.instruction(&format!("str xzr, [sp, #{}]", idx_off));               // start at group 0

    emitter.label("__rt_preg_rekey_named_loop");
    emitter.instruction(&format!("ldr x12, [sp, #{}]", idx_off));               // reload the group index
    emitter.instruction(&format!("ldr x13, [sp, #{}]", len_off));               // reload the list length
    emitter.instruction("cmp x12, x13");                                        // has every group been re-keyed?
    emitter.instruction("b.ge __rt_preg_rekey_named_done");                     // finish and release the list
    emitter.instruction(&format!("ldr x9, [sp, #{}]", list_off));               // reload the numbered list
    emitter.instruction("add x9, x9, #24");                                     // skip the indexed-array header
    emitter.instruction("ldr x10, [x9, x12, lsl #3]");                          // load this group's boxed cell (borrowed)
    emitter.instruction(&format!("str x10, [sp, #{}]", cell_off));              // keep the cell across helper calls
    emitter.instruction(&format!("ldr x0, [sp, #{}]", handle_off));             // pass the handle holding the name table
    emitter.instruction("mov x1, x12");                                         // ask for this group's name
    emitter.instruction(&format!("add x2, sp, #{}", name_ptr_off));             // receive the name pointer
    emitter.instruction(&format!("add x3, sp, #{}", name_len_off));             // receive the name length
    emitter.bl_c("elephc_pcre2_v1_group_name");                                 // resolve the group's declared name
    emitter.instruction("cbnz w0, __rt_preg_rekey_named_number");               // an unnamed group only gets its number
    emitter.instruction(&format!("ldr x1, [sp, #{}]", name_ptr_off));           // load the name pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", name_len_off));           // load the name length
    emitter.instruction("bl __rt_hash_normalize_key");                          // apply php's numeric-string key rule
    emitter.instruction(&format!("str x1, [sp, #{}]", key_lo_off));             // keep the normalized key
    emitter.instruction(&format!("str x2, [sp, #{}]", key_hi_off));             // keep the key discriminant
    emitter.instruction(&format!("ldr x0, [sp, #{}]", cell_off));               // reload the borrowed cell
    emitter.instruction("bl __rt_mixed_clone");                                 // the named key owns a detached copy
    emitter.instruction("mov x3, x0");                                          // the clone is the stored value
    emitter.instruction("mov x4, #0");                                          // boxed cells have no high payload word
    emitter.instruction(&format!("ldr x1, [sp, #{}]", key_lo_off));             // reload the normalized key
    emitter.instruction(&format!("ldr x2, [sp, #{}]", key_hi_off));             // reload the key discriminant
    emitter.instruction(&format!("ldr x0, [sp, #{}]", hash_off));               // reload the result hash
    emitter.instruction(&format!("mov x5, #{}", MIXED_TAG));                    // the value is a boxed Mixed cell
    emitter.instruction("bl __rt_hash_set");                                    // insert the name ahead of its number
    emitter.instruction(&format!("str x0, [sp, #{}]", hash_off));               // keep the possibly-grown hash

    emitter.label("__rt_preg_rekey_named_number");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", cell_off));               // reload the borrowed cell
    emitter.instruction("bl __rt_incref");                                      // the numbered key keeps its own reference
    emitter.instruction(&format!("ldr x3, [sp, #{}]", cell_off));               // the cell is the stored value
    emitter.instruction("mov x4, #0");                                          // boxed cells have no high payload word
    emitter.instruction(&format!("ldr x1, [sp, #{}]", idx_off));                // the group number is the integer key
    emitter.instruction("mov x2, #-1");                                         // -1 marks an integer key
    emitter.instruction(&format!("ldr x0, [sp, #{}]", hash_off));               // reload the result hash
    emitter.instruction(&format!("mov x5, #{}", MIXED_TAG));                    // the value is a boxed Mixed cell
    emitter.instruction("bl __rt_hash_set");                                    // insert the group under its number
    emitter.instruction(&format!("str x0, [sp, #{}]", hash_off));               // keep the possibly-grown hash
    emitter.instruction(&format!("ldr x12, [sp, #{}]", idx_off));               // reload the group index
    emitter.instruction("add x12, x12, #1");                                    // advance to the next group
    emitter.instruction(&format!("str x12, [sp, #{}]", idx_off));               // save the next group index
    emitter.instruction("b __rt_preg_rekey_named_loop");                        // continue re-keying

    emitter.label("__rt_preg_rekey_named_done");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", list_off));               // reload the numbered list
    emitter.instruction("bl __rt_decref_array");                                // release it: the hash holds its own references
    emitter.instruction(&format!("ldr x9, [sp, #{}]", mark_off));               // is there a `MARK` value?
    emitter.instruction("cbz x9, __rt_preg_rekey_named_keyed");                 // no mark: the hash is complete
    emitter.instruction("movz w9, #0x414d");                                    // little-endian 'M','A'
    emitter.instruction("movk w9, #0x4b52, lsl #16");                           // little-endian 'R','K'
    emitter.instruction(&format!("str w9, [sp, #{}]", mark_key_off));           // materialize the literal key bytes
    emitter.instruction(&format!("add x1, sp, #{}", mark_key_off));             // pass the key bytes
    emitter.instruction("mov x2, #4");                                          // `MARK` is four bytes
    emitter.instruction("bl __rt_hash_normalize_key");                          // apply php's numeric-string key rule
    emitter.instruction(&format!("ldr x3, [sp, #{}]", mark_off));               // the owned `MARK` value is stored as is
    emitter.instruction("mov x4, #0");                                          // boxed cells have no high payload word
    emitter.instruction(&format!("ldr x0, [sp, #{}]", hash_off));               // reload the result hash
    emitter.instruction(&format!("mov x5, #{}", MIXED_TAG));                    // the value is a boxed Mixed cell
    emitter.instruction("bl __rt_hash_set");                                    // insert `MARK` after every group key
    emitter.instruction(&format!("str x0, [sp, #{}]", hash_off));               // keep the possibly-grown hash

    emitter.label("__rt_preg_rekey_named_keyed");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", hash_off));               // return the keyed hash

    emitter.label("__rt_preg_rekey_named_ret");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", stack_size));               // release the re-keying frame
    emitter.instruction("ret");                                                 // return the storage in x0
}

/// ARM64 `__rt_preg_mark_cell` (x0 = handle → x0 = owned boxed mark string, or 0) and
/// `__rt_preg_mark_note` (x0 = handle, x1 = owned mark table or 0, x2 = match number → x0 = table).
fn emit_preg_mark_helpers_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_mark_cell ---");
    emitter.label_global("__rt_preg_mark_cell");
    emitter.instruction("sub sp, sp, #32");                                     // reserve the mark query frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the query frame pointer
    emitter.instruction("str xzr, [sp]");                                       // clear the mark pointer before the query
    emitter.instruction("str xzr, [sp, #8]");                                   // clear the mark length before the query
    emitter.instruction("mov x1, sp");                                          // receive the mark name pointer
    emitter.instruction("add x2, sp, #8");                                      // receive the mark name length
    emitter.bl_c("elephc_pcre2_v1_last_mark");                                  // ask PCRE2 which MARK the last match passed
    emitter.instruction("ldr x1, [sp]");                                        // load the mark name pointer
    emitter.instruction("cbz x1, __rt_preg_mark_cell_none");                    // the match passed no mark
    emitter.instruction("mov x0, #1");                                          // runtime value tag 1 = string
    emitter.instruction("ldr x2, [sp, #8]");                                    // load the mark name length
    emitter.instruction("bl __rt_mixed_from_value");                            // persist and box the mark name
    emitter.instruction("b __rt_preg_mark_cell_ret");                           // return the boxed name
    emitter.label("__rt_preg_mark_cell_none");
    emitter.instruction("mov x0, #0");                                          // no mark
    emitter.label("__rt_preg_mark_cell_ret");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the query frame
    emitter.instruction("ret");                                                 // return the cell in x0

    emitter.blank();
    emitter.comment("--- runtime: preg_mark_note ---");
    emitter.label_global("__rt_preg_mark_note");
    emitter.instruction("sub sp, sp, #48");                                     // reserve the mark note frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the note frame pointer
    emitter.instruction("str x1, [sp]");                                        // keep the mark table, or 0
    emitter.instruction("str x2, [sp, #8]");                                    // keep the match number
    emitter.instruction("bl __rt_preg_mark_cell");                              // box the match's mark name, or 0
    emitter.instruction("cbz x0, __rt_preg_mark_note_ret");                     // no mark: the table is unchanged
    emitter.instruction("str x0, [sp, #16]");                                   // keep the boxed name
    emitter.instruction("ldr x9, [sp]");                                        // is there a table yet?
    emitter.instruction("cbnz x9, __rt_preg_mark_note_insert");                 // insert into the existing table
    emitter.instruction("mov x0, #16");                                         // minimum hash capacity
    emitter.instruction(&format!("mov x1, #{}", MIXED_TAG));                    // the table stores boxed Mixed cells
    emitter.instruction("bl __rt_hash_new");                                    // allocate the mark table on the first mark
    emitter.instruction("str x0, [sp]");                                        // keep the new table
    emitter.label("__rt_preg_mark_note_insert");
    emitter.instruction("ldr x0, [sp]");                                        // reload the mark table
    emitter.instruction("ldr x1, [sp, #8]");                                    // the match number is the integer key
    emitter.instruction("mov x2, #-1");                                         // -1 marks an integer key
    emitter.instruction("ldr x3, [sp, #16]");                                   // the boxed name is the stored value
    emitter.instruction("mov x4, #0");                                          // boxed cells have no high payload word
    emitter.instruction(&format!("mov x5, #{}", MIXED_TAG));                    // the value is a boxed Mixed cell
    emitter.instruction("bl __rt_hash_set");                                    // record the match's mark
    emitter.instruction("str x0, [sp]");                                        // keep the possibly-grown table
    emitter.label("__rt_preg_mark_note_ret");
    emitter.instruction("ldr x0, [sp]");                                        // return the table, or 0
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the note frame
    emitter.instruction("ret");                                                 // return the table in x0
}

/// x86_64: rdi = compiled pattern handle, rsi = owned list of boxed cells, rdx = owned boxed
/// `MARK` value or 0; returns rax = storage.
fn emit_preg_rekey_named_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_rekey_named ---");
    emitter.label_global("__rt_preg_rekey_named");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the re-keying frame base
    emitter.instruction("sub rsp, 80");                                         // reserve aligned slots for the loop state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // keep the compiled handle across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // keep the owned numbered list
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // keep the owned `MARK` value, or 0
    emitter.bl_c("elephc_pcre2_v1_name_count");                                 // does the pattern name any group? (rdi = handle)
    emitter.instruction("test rax, rax");                                       // a zero count needs no keyed hash
    emitter.instruction("jnz __rt_preg_rekey_named_build_x86");                 // named groups need the keyed hash
    emitter.instruction("cmp QWORD PTR [rbp - 72], 0");                         // is there a `MARK` value?
    emitter.instruction("jne __rt_preg_rekey_named_build_x86");                 // `MARK` is a string key: it needs the hash too
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // no names: the numbered list is already php's shape
    emitter.instruction("jmp __rt_preg_rekey_named_ret_x86");                   // return it unchanged

    emitter.label("__rt_preg_rekey_named_build_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the numbered list
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // load its length
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // keep the length for the loop bound
    emitter.instruction("shl rdi, 1");                                          // one bucket per number and one per name
    emitter.instruction("cmp rdi, 16");                                         // is the doubled length below the minimum?
    emitter.instruction("jge __rt_preg_rekey_named_capacity_x86");              // keep the doubled capacity
    emitter.instruction("mov rdi, 16");                                         // clamp the capacity to the minimum
    emitter.label("__rt_preg_rekey_named_capacity_x86");
    emitter.instruction(&format!("mov rsi, {}", MIXED_TAG));                    // the hash stores boxed Mixed cells
    emitter.instruction("call __rt_hash_new");                                  // allocate the keyed result hash
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // keep the hash across inserts
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // start at group 0

    emitter.label("__rt_preg_rekey_named_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the group index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 32]");                       // has every group been re-keyed?
    emitter.instruction("jge __rt_preg_rekey_named_done_x86");                  // finish and release the list
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the numbered list
    emitter.instruction("mov r11, QWORD PTR [r10 + 24 + rcx * 8]");             // load this group's boxed cell (borrowed)
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // keep the cell across helper calls
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the handle holding the name table
    emitter.instruction("mov rsi, rcx");                                        // ask for this group's name
    emitter.instruction("lea rdx, [rbp - 56]");                                 // receive the name pointer
    emitter.instruction("lea rcx, [rbp - 64]");                                 // receive the name length
    emitter.bl_c("elephc_pcre2_v1_group_name");                                 // resolve the group's declared name
    emitter.instruction("test eax, eax");                                       // did the group declare a name?
    emitter.instruction("jnz __rt_preg_rekey_named_number_x86");                // an unnamed group only gets its number
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // load the name pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // load the name length
    emitter.instruction("call __rt_hash_normalize_key");                        // apply php's numeric-string key rule
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // keep the normalized key low word
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // keep the key discriminant
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the borrowed cell
    emitter.instruction("call __rt_mixed_clone");                               // the named key owns a detached copy
    emitter.instruction("mov rcx, rax");                                        // the clone is the stored value
    emitter.instruction("xor r8d, r8d");                                        // boxed cells have no high payload word
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload the normalized key
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // reload the key discriminant
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the result hash
    emitter.instruction(&format!("mov r9, {}", MIXED_TAG));                     // the value is a boxed Mixed cell
    emitter.instruction("call __rt_hash_set");                                  // insert the name ahead of its number
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // keep the possibly-grown hash

    emitter.label("__rt_preg_rekey_named_number_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the borrowed cell
    emitter.instruction("call __rt_incref");                                    // the numbered key keeps its own reference
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // the cell is the stored value
    emitter.instruction("xor r8d, r8d");                                        // boxed cells have no high payload word
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // the group number is the integer key
    emitter.instruction("mov rdx, -1");                                         // -1 marks an integer key
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the result hash
    emitter.instruction(&format!("mov r9, {}", MIXED_TAG));                     // the value is a boxed Mixed cell
    emitter.instruction("call __rt_hash_set");                                  // insert the group under its number
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // keep the possibly-grown hash
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the group index
    emitter.instruction("add rcx, 1");                                          // advance to the next group
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // save the next group index
    emitter.instruction("jmp __rt_preg_rekey_named_loop_x86");                  // continue re-keying

    emitter.label("__rt_preg_rekey_named_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the numbered list
    emitter.instruction("call __rt_decref_array");                              // release it: the hash holds its own references
    emitter.instruction("cmp QWORD PTR [rbp - 72], 0");                         // is there a `MARK` value?
    emitter.instruction("je __rt_preg_rekey_named_keyed_x86");                  // no mark: the hash is complete
    emitter.instruction("mov DWORD PTR [rbp - 80], 0x4b52414d");                // materialize the literal `MARK` key bytes
    emitter.instruction("lea rax, [rbp - 80]");                                 // pass the key bytes
    emitter.instruction("mov rdx, 4");                                          // `MARK` is four bytes
    emitter.instruction("call __rt_hash_normalize_key");                        // apply php's numeric-string key rule
    emitter.instruction("mov rsi, rax");                                        // the normalized key low word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 72]");                       // the owned `MARK` value is stored as is
    emitter.instruction("xor r8d, r8d");                                        // boxed cells have no high payload word
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the result hash
    emitter.instruction(&format!("mov r9, {}", MIXED_TAG));                     // the value is a boxed Mixed cell
    emitter.instruction("call __rt_hash_set");                                  // insert `MARK` after every group key
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // keep the possibly-grown hash

    emitter.label("__rt_preg_rekey_named_keyed_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the keyed hash

    emitter.label("__rt_preg_rekey_named_ret_x86");
    emitter.instruction("mov rsp, rbp");                                        // release the re-keying frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the storage in rax
}

/// x86_64 `__rt_preg_mark_cell` (rdi = handle → rax = owned boxed mark string, or 0) and
/// `__rt_preg_mark_note` (rdi = handle, rsi = owned mark table or 0, rdx = match number →
/// rax = table).
fn emit_preg_mark_helpers_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_mark_cell ---");
    emitter.label_global("__rt_preg_mark_cell");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the query frame base
    emitter.instruction("sub rsp, 16");                                         // reserve the mark pointer and length slots
    emitter.instruction("mov QWORD PTR [rbp - 8], 0");                          // clear the mark pointer before the query
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // clear the mark length before the query
    emitter.instruction("lea rsi, [rbp - 8]");                                  // receive the mark name pointer
    emitter.instruction("lea rdx, [rbp - 16]");                                 // receive the mark name length
    emitter.bl_c("elephc_pcre2_v1_last_mark");                                  // ask PCRE2 which MARK the last match passed (rdi = handle)
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // load the mark name pointer
    emitter.instruction("test rdi, rdi");                                       // did the match pass a mark?
    emitter.instruction("jz __rt_preg_mark_cell_none_x86");                     // no mark
    emitter.instruction("mov rax, 1");                                          // runtime value tag 1 = string
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // load the mark name length
    emitter.instruction("call __rt_mixed_from_value");                          // persist and box the mark name
    emitter.instruction("jmp __rt_preg_mark_cell_ret_x86");                     // return the boxed name
    emitter.label("__rt_preg_mark_cell_none_x86");
    emitter.instruction("xor eax, eax");                                        // no mark
    emitter.label("__rt_preg_mark_cell_ret_x86");
    emitter.instruction("mov rsp, rbp");                                        // release the query frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the cell in rax

    emitter.blank();
    emitter.comment("--- runtime: preg_mark_note ---");
    emitter.label_global("__rt_preg_mark_note");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the note frame base
    emitter.instruction("sub rsp, 32");                                         // reserve the table, number and cell slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // keep the mark table, or 0
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // keep the match number
    emitter.instruction("call __rt_preg_mark_cell");                            // box the match's mark name, or 0 (rdi = handle)
    emitter.instruction("test rax, rax");                                       // did the match pass a mark?
    emitter.instruction("jz __rt_preg_mark_note_ret_x86");                      // no mark: the table is unchanged
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // keep the boxed name
    emitter.instruction("cmp QWORD PTR [rbp - 8], 0");                          // is there a table yet?
    emitter.instruction("jne __rt_preg_mark_note_insert_x86");                  // insert into the existing table
    emitter.instruction("mov rdi, 16");                                         // minimum hash capacity
    emitter.instruction(&format!("mov rsi, {}", MIXED_TAG));                    // the table stores boxed Mixed cells
    emitter.instruction("call __rt_hash_new");                                  // allocate the mark table on the first mark
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // keep the new table
    emitter.label("__rt_preg_mark_note_insert_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the mark table
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // the match number is the integer key
    emitter.instruction("mov rdx, -1");                                         // -1 marks an integer key
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // the boxed name is the stored value
    emitter.instruction("xor r8d, r8d");                                        // boxed cells have no high payload word
    emitter.instruction(&format!("mov r9, {}", MIXED_TAG));                     // the value is a boxed Mixed cell
    emitter.instruction("call __rt_hash_set");                                  // record the match's mark
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // keep the possibly-grown table
    emitter.label("__rt_preg_mark_note_ret_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the table, or 0
    emitter.instruction("mov rsp, rbp");                                        // release the note frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the table in rax
}
