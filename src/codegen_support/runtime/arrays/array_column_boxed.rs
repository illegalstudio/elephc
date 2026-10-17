//! Purpose:
//! Implements the general `array_column()` runtime: any row layout, integer or string
//! column keys, a `null` column (whole rows), an optional index key, and object rows.
//!
//! Called from:
//! - The typed ArrayColumn backend (`crate::codegen::lower_inst::builtins::arrays::column`)
//!   whenever the concrete associative-row fast paths do not apply.
//!
//! Key details:
//! - `__rt_array_column_boxed(box, keys)` unboxes a declared PHP array and tail-calls
//!   `__rt_array_column_any(array, keys)`; a non-array box returns 0.
//! - `keys` points at a caller-owned 8-word block: column `(tag, lo, hi)`, static flags,
//!   index `(tag, lo, hi)`, padding. Tags use the boxed Mixed numbering; tag 8 is `null`.
//!   Flag bit 2 asks for a hash result even when the index key turns out to be `null`.
//! - Row values and index values are read as owned Mixed cells. Without an index key the
//!   result is a packed array of Mixed cells; with one it is a Mixed-valued hash whose keys
//!   follow PHP array-key conversion, and rows lacking the index key are appended with the
//!   hash's next integer index.
//! - Object rows expose public properties only (declared rows whose print_r key equals the
//!   bare name, plus dynamic properties). Integer keys never match an object property.
//! - A `null` column keeps every element, scalars included, exactly like PHP.
//! - Small return values are errors after the partial result has been released:
//!   1/2 = index value is an array/object, 3/5 = column key argument is an array/object,
//!   4/6 = index key argument is an array/object. Object codes also return the object's
//!   borrowed class name (AArch64 x1/x2, x86_64 rsi/rdx) for PHP's message.
//! - Stamping packed results preserves the x86_64 heap marker used by boxing and cleanup.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

const FRAME: usize = 176;
const SOURCE: usize = 0;
const KEYS: usize = 8;
const COL_LO: usize = 16;
const COL_HI: usize = 24;
const IDX_LO: usize = 32;
const IDX_HI: usize = 40;
const FLAGS: usize = 48;
const CURSOR: usize = 56;
const ROW: usize = 64;
const ROW_TAG: usize = 72;
const OUTPUT: usize = 80;
const CELL: usize = 88;
const VALUE: usize = 112;
const INDEX: usize = 120;
const KEY_LO: usize = 128;
const KEY_HI: usize = 136;
const ROW_HI: usize = 144;

/// Flag bit: the column key is `null`, so whole rows are collected.
pub const ARRAY_COLUMN_FLAG_WHOLE_ROW: i64 = 1;
/// Flag bit: a non-null index key re-keys the result.
pub const ARRAY_COLUMN_FLAG_INDEXED: i64 = 2;
/// Flag bit (static, set by lowering): build a hash result.
pub const ARRAY_COLUMN_FLAG_HASH_RESULT: i64 = 4;

/// Emits one equivalent instruction for the selected native ABI.
fn op(emitter: &mut Emitter, arm: &str, x86: &str) {
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { arm } else { x86 }); // keep the same operation on both native architectures
}

/// Emits the boxed entry, the general walker, and their private key/property helpers.
pub fn emit_array_column_boxed(emitter: &mut Emitter) {
    emit_boxed_entry(emitter);
    emit_any(emitter);
    emit_key_conversion(emitter);
    emit_public_property_lookup(emitter);
    emit_class_name(emitter);
}

/// Borrows a boxed source and the key block; tail-calls the walker or returns zero.
fn emit_boxed_entry(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    let low = crate::codegen_support::mixed_unbox_payload_reg(emitter.target);
    emitter.blank();
    emitter.label_global("__rt_array_column_boxed");
    abi::emit_frame_prologue(emitter, 32);
    abi::emit_store_to_sp(emitter, arg1, 0);
    abi::emit_reg_move(emitter, result, arg0);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    op(emitter, "sub x9, x0, #4", "lea r10, [rax - 4]");                        // rebase the runtime tag so arrays become 0 and 1
    op(emitter, "cmp x9, #1", "cmp r10, 1");                                    // packed (4) and hash (5) payloads are the only arrays
    op(emitter, "b.hi __rt_array_column_boxed_invalid", "ja __rt_array_column_boxed_invalid"); // anything else is a TypeError at the call site
    abi::emit_reg_move(emitter, arg0, low);
    abi::emit_load_temporary_stack_slot(emitter, arg1, 0);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_jump(emitter, "__rt_array_column_any");
    emitter.label("__rt_array_column_boxed_invalid");
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

/// Emits `__rt_array_column_any(array, keys)`, which returns an owned array or an error code.
fn emit_any(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let result = abi::int_result_reg(emitter);
    let arg = |index: usize| abi::int_arg_reg_name(emitter.target, index);
    let (arg0, arg1, arg2, arg3) = (arg(0), arg(1), arg(2), arg(3));
    let (arg4, arg5) = (arg(4), arg(5));
    let low = crate::codegen_support::mixed_unbox_payload_reg(emitter.target);

    emitter.blank();
    emitter.label_global("__rt_array_column_any");
    abi::emit_frame_prologue(emitter, FRAME);
    abi::emit_store_to_sp(emitter, arg0, SOURCE);
    abi::emit_store_to_sp(emitter, arg1, KEYS);
    op(emitter, "ldr x9, [x1, #24]", "mov r10, QWORD PTR [rsi + 24]");          // read the static flags word from the key block
    abi::emit_store_to_sp(emitter, if arm { "x9" } else { "r10" }, FLAGS);

    // Column key: null selects whole rows, arrays and objects are TypeErrors.
    load_key_triple(emitter, 0);
    abi::emit_call_label(emitter, "__rt_array_column_key");
    op(emitter, "cmp x0, #1", "cmp rax, 1");                                    // status 1 = null column key
    op(emitter, "b.eq __rt_array_column_any_whole_row", "je __rt_array_column_any_whole_row"); // collect whole rows
    op(emitter, "cmp x0, #2", "cmp rax, 2");                                    // status 2 = array column key
    op(emitter, "b.eq __rt_array_column_any_bad_col_array", "je __rt_array_column_any_bad_col_array"); // report the column-key TypeError
    op(emitter, "cmp x0, #3", "cmp rax, 3");                                    // status 3 = object column key
    op(emitter, "b.eq __rt_array_column_any_bad_col_object", "je __rt_array_column_any_bad_col_object"); // report the column-key TypeError
    store_key_pair(emitter, COL_LO, COL_HI);
    abi::emit_jump(emitter, "__rt_array_column_any_index_key");
    emitter.label("__rt_array_column_any_whole_row");
    or_flag(emitter, ARRAY_COLUMN_FLAG_WHOLE_ROW);

    // Index key: null keeps automatic keys, arrays and objects are TypeErrors.
    emitter.label("__rt_array_column_any_index_key");
    load_key_triple(emitter, 32);
    abi::emit_call_label(emitter, "__rt_array_column_key");
    op(emitter, "cmp x0, #1", "cmp rax, 1");                                    // status 1 = null index key
    op(emitter, "b.eq __rt_array_column_any_alloc", "je __rt_array_column_any_alloc"); // keep automatic integer keys
    op(emitter, "cmp x0, #2", "cmp rax, 2");                                    // status 2 = array index key
    op(emitter, "b.eq __rt_array_column_any_bad_idx_array", "je __rt_array_column_any_bad_idx_array"); // report the index-key TypeError
    op(emitter, "cmp x0, #3", "cmp rax, 3");                                    // status 3 = object index key
    op(emitter, "b.eq __rt_array_column_any_bad_idx_object", "je __rt_array_column_any_bad_idx_object"); // report the index-key TypeError
    store_key_pair(emitter, IDX_LO, IDX_HI);
    or_flag(emitter, ARRAY_COLUMN_FLAG_INDEXED);

    // The fresh result owns pointer-sized Mixed cells, including when it stays empty.
    emitter.label("__rt_array_column_any_alloc");
    abi::emit_load_temporary_stack_slot(emitter, result, FLAGS);
    op(emitter, "tbnz x0, #2, __rt_array_column_any_alloc_hash", "test rax, 4"); // does the caller expect a hash result?
    if !arm {
        emitter.instruction("jnz __rt_array_column_any_alloc_hash");            // build a hash for the re-keyed result
    }
    abi::emit_load_int_immediate(emitter, arg0, 8);
    abi::emit_load_int_immediate(emitter, arg1, 8);
    abi::emit_call_label(emitter, "__rt_array_new");
    op(emitter, "ldr x9, [x0, #-8]", "mov r10, QWORD PTR [rax - 8]");           // load the fresh array's heap kind word
    op(emitter, "mov x10, #0x80ff", "mov r11, 0xffffffff000080ff");             // keep the kind byte, COW bit and x86 heap marker
    op(emitter, "and x9, x9, x10", "and r10, r11");                             // clear the previous element value_type
    op(emitter, "orr x9, x9, #0x700", "or r10, 0x700");                         // stamp value_type 7 (boxed Mixed cells)
    op(emitter, "str x9, [x0, #-8]", "mov QWORD PTR [rax - 8], r10");           // publish the Mixed element layout
    abi::emit_jump(emitter, "__rt_array_column_any_alloc_done");
    emitter.label("__rt_array_column_any_alloc_hash");
    abi::emit_load_int_immediate(emitter, arg0, 8);
    abi::emit_load_int_immediate(emitter, arg1, 7);
    abi::emit_call_label(emitter, "__rt_hash_new");
    emitter.label("__rt_array_column_any_alloc_done");
    abi::emit_store_to_sp(emitter, result, OUTPUT);
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_store_to_sp(emitter, result, CURSOR);

    // Walk every source entry; only array and object rows contribute.
    emitter.label("__rt_array_column_any_loop");
    abi::emit_load_temporary_stack_slot(emitter, arg0, SOURCE);
    abi::emit_load_temporary_stack_slot(emitter, arg1, CURSOR);
    abi::emit_call_label(emitter, "__rt_array_iter_next");
    op(emitter, "cmn x0, #1", "cmp rax, -1");                                   // has the iterator reported exhaustion?
    op(emitter, "b.eq __rt_array_column_any_done", "je __rt_array_column_any_done"); // return the finished result
    abi::emit_store_to_sp(emitter, result, CURSOR);
    let fields = if arm { ["x3", "x4", "x5"] } else { ["r8", "r9", "r10"] };
    for (reg, offset) in fields.into_iter().zip([CELL, CELL + 8, CELL + 16]) {
        abi::emit_store_to_sp(emitter, reg, offset);
    }
    abi::emit_temporary_stack_address(emitter, result, CELL);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    abi::emit_store_to_sp(emitter, low, ROW);
    abi::emit_store_to_sp(emitter, if arm { "x2" } else { "rdx" }, ROW_HI);
    abi::emit_store_to_sp(emitter, result, ROW_TAG);
    op(emitter, "cmp x0, #6", "cmp rax, 6");                                    // is this row an object?
    op(emitter, "b.eq __rt_array_column_any_row_ok", "je __rt_array_column_any_row_ok"); // objects expose public properties
    op(emitter, "sub x9, x0, #4", "lea r10, [rax - 4]");                        // rebase the row tag so arrays become 0 and 1
    op(emitter, "cmp x9, #1", "cmp r10, 1");                                    // packed (4) and hash (5) rows are arrays
    op(emitter, "b.ls __rt_array_column_any_row_ok", "jbe __rt_array_column_any_row_ok"); // array rows can hold the column
    abi::emit_load_temporary_stack_slot(emitter, result, FLAGS);
    op(emitter, "tbz x0, #0, __rt_array_column_any_loop", "test rax, 1");       // a null column keeps scalar rows too
    if !arm {
        emitter.instruction("jz __rt_array_column_any_loop");                   // otherwise scalar rows contribute nothing
    }
    emitter.label("__rt_array_column_any_row_ok");

    // Column value: the whole row, an array element, or a public property.
    abi::emit_load_temporary_stack_slot(emitter, result, FLAGS);
    op(emitter, "tbnz x0, #0, __rt_array_column_any_value_row", "test rax, 1"); // is the column key null?
    if !arm {
        emitter.instruction("jnz __rt_array_column_any_value_row");             // box the whole row as the value
    }
    emit_row_read(emitter, COL_LO, COL_HI, "value", "__rt_array_column_any_loop");
    abi::emit_jump(emitter, "__rt_array_column_any_value_ready");
    emitter.label("__rt_array_column_any_value_row");
    if arm {
        abi::emit_load_temporary_stack_slot(emitter, "x0", ROW_TAG);
        abi::emit_load_temporary_stack_slot(emitter, "x1", ROW);
        abi::emit_load_temporary_stack_slot(emitter, "x2", ROW_HI);
    } else {
        abi::emit_load_temporary_stack_slot(emitter, "rax", ROW_TAG);
        abi::emit_load_temporary_stack_slot(emitter, "rdi", ROW);
        abi::emit_load_temporary_stack_slot(emitter, "rsi", ROW_HI);
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    emitter.label("__rt_array_column_any_value_ready");
    abi::emit_store_to_sp(emitter, result, VALUE);

    // Index value: absent index keys and rows without it append automatically.
    abi::emit_load_temporary_stack_slot(emitter, result, FLAGS);
    op(emitter, "tbz x0, #1, __rt_array_column_any_append", "test rax, 2");     // is there a non-null index key?
    if !arm {
        emitter.instruction("jz __rt_array_column_any_append");                 // no index key: append the value
    }
    emit_row_read(emitter, IDX_LO, IDX_HI, "index", "__rt_array_column_any_append");
    abi::emit_store_to_sp(emitter, result, INDEX);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    if !arm {
        emitter.instruction("mov rsi, rdi");                                    // index payload low word into the key-conversion slot
        emitter.instruction("mov rdi, rax");                                    // index runtime tag into the key-conversion slot
    }
    abi::emit_call_label(emitter, "__rt_array_column_key");
    op(emitter, "cmp x0, #2", "cmp rax, 2");                                    // array index values cannot become keys
    op(emitter, "b.eq __rt_array_column_any_fail_array", "je __rt_array_column_any_fail_array"); // raise PHP's illegal-offset TypeError
    op(emitter, "cmp x0, #3", "cmp rax, 3");                                    // object index values cannot become keys
    op(emitter, "b.eq __rt_array_column_any_fail_object", "je __rt_array_column_any_fail_object"); // raise PHP's illegal-offset TypeError
    op(emitter, "cmp x0, #1", "cmp rax, 1");                                    // does a null index value select the empty-string key?
    op(emitter, "b.ne __rt_array_column_any_insert", "jne __rt_array_column_any_insert"); // keep the converted int or string key
    abi::emit_temporary_stack_address(emitter, if arm { "x1" } else { "rsi" }, INDEX);
    op(emitter, "mov x2, #0", "xor edx, edx");                                  // a zero-length string key ("")
    emitter.label("__rt_array_column_any_insert");
    store_key_pair(emitter, KEY_LO, KEY_HI);
    abi::emit_load_temporary_stack_slot(emitter, arg0, OUTPUT);
    abi::emit_load_temporary_stack_slot(emitter, arg1, KEY_LO);
    abi::emit_load_temporary_stack_slot(emitter, arg2, KEY_HI);
    abi::emit_load_temporary_stack_slot(emitter, arg3, VALUE);
    abi::emit_load_int_immediate(emitter, arg4, 0);
    abi::emit_load_int_immediate(emitter, arg5, 7);
    abi::emit_call_label(emitter, "__rt_hash_set");
    abi::emit_store_to_sp(emitter, result, OUTPUT);
    abi::emit_load_temporary_stack_slot(emitter, result, INDEX);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    abi::emit_jump(emitter, "__rt_array_column_any_loop");

    // Automatic keys: packed results push, hash results use the next integer index.
    emitter.label("__rt_array_column_any_append");
    abi::emit_load_temporary_stack_slot(emitter, result, FLAGS);
    op(emitter, "tbnz x0, #2, __rt_array_column_any_append_hash", "test rax, 4"); // is the result a hash?
    if !arm {
        emitter.instruction("jnz __rt_array_column_any_append_hash");           // append through the hash's next index
    }
    abi::emit_load_temporary_stack_slot(emitter, arg0, OUTPUT);
    abi::emit_load_temporary_stack_slot(emitter, arg1, VALUE);
    abi::emit_call_label(emitter, "__rt_array_push_int");
    abi::emit_store_to_sp(emitter, result, OUTPUT);
    abi::emit_jump(emitter, "__rt_array_column_any_loop");
    emitter.label("__rt_array_column_any_append_hash");
    abi::emit_load_temporary_stack_slot(emitter, arg0, OUTPUT);
    abi::emit_load_temporary_stack_slot(emitter, arg1, VALUE);
    abi::emit_load_int_immediate(emitter, arg2, 0);
    abi::emit_load_int_immediate(emitter, arg3, 7);
    abi::emit_call_label(emitter, "__rt_hash_append");
    abi::emit_store_to_sp(emitter, result, OUTPUT);
    abi::emit_jump(emitter, "__rt_array_column_any_loop");

    // Illegal index values release every owner before reporting the error code.
    emitter.label("__rt_array_column_any_fail_array");
    abi::emit_load_int_immediate(emitter, result, 1);
    abi::emit_jump(emitter, "__rt_array_column_any_fail");
    emitter.label("__rt_array_column_any_fail_object");
    abi::emit_load_temporary_stack_slot(emitter, result, INDEX);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    abi::emit_reg_move(emitter, arg0, low);
    abi::emit_call_label(emitter, "__rt_array_column_class_name");
    store_name(emitter);
    abi::emit_load_int_immediate(emitter, result, 2);
    emitter.label("__rt_array_column_any_fail");
    abi::emit_store_to_sp(emitter, result, KEY_LO);
    for owner in [INDEX, VALUE] {
        abi::emit_load_temporary_stack_slot(emitter, result, owner);
        abi::emit_call_label(emitter, "__rt_decref_mixed");
    }
    abi::emit_load_temporary_stack_slot(emitter, result, OUTPUT);
    abi::emit_call_label(emitter, "__rt_decref_any");
    load_name(emitter);
    abi::emit_load_temporary_stack_slot(emitter, result, KEY_LO);
    abi::emit_jump(emitter, "__rt_array_column_any_return");

    for (label, code) in [("__rt_array_column_any_bad_col_array", 3), ("__rt_array_column_any_bad_idx_array", 4)] {
        emitter.label(label);
        abi::emit_load_int_immediate(emitter, result, code);
        abi::emit_jump(emitter, "__rt_array_column_any_return");
    }
    // Object-typed key arguments report the class name, read back from the key block.
    for (label, code, block_offset) in [
        ("__rt_array_column_any_bad_col_object", 5, 8),
        ("__rt_array_column_any_bad_idx_object", 6, 40),
    ] {
        emitter.label(label);
        abi::emit_load_temporary_stack_slot(emitter, arg0, KEYS);
        abi::emit_load_from_address(emitter, arg0, arg0, block_offset);
        abi::emit_call_label(emitter, "__rt_array_column_class_name");
        abi::emit_load_int_immediate(emitter, result, code);
        abi::emit_jump(emitter, "__rt_array_column_any_return");
    }

    emitter.label("__rt_array_column_any_done");
    abi::emit_load_temporary_stack_slot(emitter, result, OUTPUT);
    emitter.label("__rt_array_column_any_return");
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Loads one `(tag, lo, hi)` triple from the key block into the key-conversion inputs.
fn load_key_triple(emitter: &mut Emitter, block_offset: usize) {
    let (base, regs) = if emitter.target.arch == Arch::AArch64 {
        ("x9", ["x0", "x1", "x2"])
    } else {
        ("r10", ["rdi", "rsi", "rdx"])
    };
    abi::emit_load_temporary_stack_slot(emitter, base, KEYS);
    for (index, reg) in regs.into_iter().enumerate() {
        abi::emit_load_from_address(emitter, reg, base, block_offset + index * 8);
    }
}

/// Stores the converted hash key pair returned by `__rt_array_column_key`.
fn store_key_pair(emitter: &mut Emitter, lo: usize, hi: usize) {
    let (key_lo, key_hi) = if emitter.target.arch == Arch::AArch64 { ("x1", "x2") } else { ("rsi", "rdx") };
    abi::emit_store_to_sp(emitter, key_lo, lo);
    abi::emit_store_to_sp(emitter, key_hi, hi);
}

/// Saves the class-name pair returned by `__rt_array_column_class_name` in the CELL slots.
fn store_name(emitter: &mut Emitter) {
    let (ptr, len) = name_regs(emitter);
    abi::emit_store_to_sp(emitter, ptr, CELL);
    abi::emit_store_to_sp(emitter, len, CELL + 8);
}

/// Reloads the saved class-name pair into the walker's error-return registers.
fn load_name(emitter: &mut Emitter) {
    let (ptr, len) = name_regs(emitter);
    abi::emit_load_temporary_stack_slot(emitter, ptr, CELL);
    abi::emit_load_temporary_stack_slot(emitter, len, CELL + 8);
}

/// Registers that carry a borrowed class name next to an object error code.
fn name_regs(emitter: &Emitter) -> (&'static str, &'static str) {
    if emitter.target.arch == Arch::AArch64 { ("x1", "x2") } else { ("rsi", "rdx") }
}

/// Sets one runtime flag bit in the walker's flags slot.
fn or_flag(emitter: &mut Emitter, bit: i64) {
    let reg = abi::int_result_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, reg, FLAGS);
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction(&format!("orr x0, x0, #{bit}"));                    // record the runtime key-shape flag
    } else {
        emitter.instruction(&format!("or rax, {bit}"));                         // record the runtime key-shape flag
    }
    abi::emit_store_to_sp(emitter, reg, FLAGS);
}

/// Reads the keyed value of the current row as an owned Mixed cell, or jumps to `missing`.
///
/// Array rows probe presence first so a stored `null` still counts; object rows resolve a
/// public property index and box a fresh copy of its value.
fn emit_row_read(emitter: &mut Emitter, lo: usize, hi: usize, kind: &str, missing: &str) {
    let result = abi::int_result_reg(emitter);
    let arg = |index: usize| abi::int_arg_reg_name(emitter.target, index);
    let (arg0, arg1, arg2, arg3) = (arg(0), arg(1), arg(2), arg(3));
    let object = format!("__rt_array_column_any_{kind}_object");
    let ready = format!("__rt_array_column_any_{kind}_read");
    abi::emit_load_temporary_stack_slot(emitter, result, ROW_TAG);
    op(emitter, "cmp x0, #6", "cmp rax, 6");                                    // object rows read public properties
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction(&format!("b.eq {object}"));                         // resolve the property by name
        emitter.instruction("sub x9, x0, #4");                                  // rebase the row tag so arrays become 0 and 1
        emitter.instruction("cmp x9, #1");                                      // only packed (4) and hash (5) rows have keys
        emitter.instruction(&format!("b.hi {missing}"));                        // scalar rows lack every key
    } else {
        emitter.instruction(&format!("je {object}"));                           // resolve the property by name
        emitter.instruction("lea r10, [rax - 4]");                              // rebase the row tag so arrays become 0 and 1
        emitter.instruction("cmp r10, 1");                                      // only packed (4) and hash (5) rows have keys
        emitter.instruction(&format!("ja {missing}"));                          // scalar rows lack every key
    }
    for (reg, offset) in [(arg0, ROW), (arg1, lo), (arg2, hi)] {
        abi::emit_load_temporary_stack_slot(emitter, reg, offset);
    }
    abi::emit_call_label(emitter, "__rt_array_key_exists_mixed_key");
    abi::emit_branch_if_int_result_zero(emitter, missing);
    for (reg, offset) in [(arg0, ROW), (arg1, lo), (arg2, hi)] {
        abi::emit_load_temporary_stack_slot(emitter, reg, offset);
    }
    abi::emit_load_int_immediate(emitter, arg3, 0);
    abi::emit_call_label(emitter, "__rt_array_get_mixed_key");
    abi::emit_jump(emitter, &ready);
    emitter.label(&object);
    for (reg, offset) in [(arg0, ROW), (arg1, lo), (arg2, hi)] {
        abi::emit_load_temporary_stack_slot(emitter, reg, offset);
    }
    abi::emit_call_label(emitter, "__rt_array_column_prop");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("cmn x0, #1");                                      // was no public property found?
        emitter.instruction(&format!("b.eq {missing}"));                        // treat the row as lacking the key
        emitter.instruction("mov x1, x0");                                      // property index for the boxed read
    } else {
        emitter.instruction("cmp rax, -1");                                     // was no public property found?
        emitter.instruction(&format!("je {missing}"));                          // treat the row as lacking the key
        emitter.instruction("mov rsi, rax");                                    // property index for the boxed read
    }
    abi::emit_load_temporary_stack_slot(emitter, arg0, ROW);
    abi::emit_call_label(emitter, "__rt_obj_prop_value");
    emitter.label(&ready);
}

/// Emits `__rt_array_column_key(tag, lo, hi)` → `(status, key_lo, key_hi)`.
///
/// Status 0 is a normalized hash key (key_hi = -1 for integers), 1 is `null`, 2 an array and
/// 3 an object. Strings normalize numeric spellings; floats truncate with PHP's diagnostics.
/// AArch64 uses x0..x2 in and out; x86_64 takes rdi/rsi/rdx and returns rax/rsi/rdx.
fn emit_key_conversion(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_array_column_key");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("cmp x0, #1");                                      // string keys need numeric normalization
        emitter.instruction("b.eq __rt_array_column_key_string");               // normalize the string key
        emitter.instruction("cmp x0, #8");                                      // null is reported to the caller
        emitter.instruction("b.eq __rt_array_column_key_null");                 // the caller decides what null means
        emitter.instruction("cmp x0, #2");                                      // floats truncate toward zero
        emitter.instruction("b.eq __rt_array_column_key_float");                // convert with PHP diagnostics
        emitter.instruction("cmp x0, #4");                                      // packed arrays are illegal keys
        emitter.instruction("b.eq __rt_array_column_key_array");                // report an array key
        emitter.instruction("cmp x0, #5");                                      // hashes are illegal keys
        emitter.instruction("b.eq __rt_array_column_key_array");                // report an array key
        emitter.instruction("cmp x0, #6");                                      // objects are illegal keys
        emitter.instruction("b.eq __rt_array_column_key_object");               // report an object key
        emitter.instruction("cmp x0, #0");                                      // integers are keys as-is
        emitter.instruction("b.eq __rt_array_column_key_int");                  // keep the integer payload
        emitter.instruction("cmp x0, #3");                                      // booleans become integer keys 0/1
        emitter.instruction("b.eq __rt_array_column_key_int");                  // keep the boolean payload
        emitter.instruction("mov x1, #0");                                      // other tags fall back to integer key zero
        emitter.label("__rt_array_column_key_int");
        emitter.instruction("mov x2, #-1");                                     // key_hi = -1 marks an integer key
        emitter.instruction("mov x0, #0");                                      // status 0 = usable key
        emitter.instruction("ret");                                             // return the integer key
        emitter.label("__rt_array_column_key_string");
        emitter.instruction("stp x29, x30, [sp, #-16]!");                       // save linkage across normalization
        emitter.instruction("mov x29, sp");                                     // establish a frame for the call
        emitter.instruction("bl __rt_hash_normalize_key");                      // x1/x2 = normalized string or integer key
        emitter.instruction("ldp x29, x30, [sp], #16");                         // restore linkage
        emitter.instruction("mov x0, #0");                                      // status 0 = usable key
        emitter.instruction("ret");                                             // return the normalized key
        emitter.label("__rt_array_column_key_float");
        emitter.instruction("stp x29, x30, [sp, #-16]!");                       // save linkage across the conversion
        emitter.instruction("mov x29, sp");                                     // establish a frame for the call
        emitter.instruction("fmov d0, x1");                                     // move the IEEE-754 payload into d0
        emitter.instruction("bl __rt_float_key_to_int");                        // x0 = PHP integer key (with diagnostics)
        emitter.instruction("ldp x29, x30, [sp], #16");                         // restore linkage
        emitter.instruction("mov x1, x0");                                      // the converted integer is the key
        emitter.instruction("b __rt_array_column_key_int");                     // finish as an integer key
        emitter.label("__rt_array_column_key_null");
        emitter.instruction("mov x0, #1");                                      // status 1 = null
        emitter.instruction("ret");                                             // return the null status
        emitter.label("__rt_array_column_key_array");
        emitter.instruction("mov x0, #2");                                      // status 2 = array
        emitter.instruction("ret");                                             // return the array status
        emitter.label("__rt_array_column_key_object");
        emitter.instruction("mov x0, #3");                                      // status 3 = object
        emitter.instruction("ret");                                             // return the object status
    } else {
        emitter.instruction("cmp rdi, 1");                                      // string keys need numeric normalization
        emitter.instruction("je __rt_array_column_key_string");                 // normalize the string key
        emitter.instruction("cmp rdi, 8");                                      // null is reported to the caller
        emitter.instruction("je __rt_array_column_key_null");                   // the caller decides what null means
        emitter.instruction("cmp rdi, 2");                                      // floats truncate toward zero
        emitter.instruction("je __rt_array_column_key_float");                  // convert with PHP diagnostics
        emitter.instruction("cmp rdi, 4");                                      // packed arrays are illegal keys
        emitter.instruction("je __rt_array_column_key_array");                  // report an array key
        emitter.instruction("cmp rdi, 5");                                      // hashes are illegal keys
        emitter.instruction("je __rt_array_column_key_array");                  // report an array key
        emitter.instruction("cmp rdi, 6");                                      // objects are illegal keys
        emitter.instruction("je __rt_array_column_key_object");                 // report an object key
        emitter.instruction("cmp rdi, 0");                                      // integers are keys as-is
        emitter.instruction("je __rt_array_column_key_int");                    // keep the integer payload
        emitter.instruction("cmp rdi, 3");                                      // booleans become integer keys 0/1
        emitter.instruction("je __rt_array_column_key_int");                    // keep the boolean payload
        emitter.instruction("xor esi, esi");                                    // other tags fall back to integer key zero
        emitter.label("__rt_array_column_key_int");
        emitter.instruction("mov rdx, -1");                                     // key_hi = -1 marks an integer key
        emitter.instruction("xor eax, eax");                                    // status 0 = usable key
        emitter.instruction("ret");                                             // return the integer key
        emitter.label("__rt_array_column_key_string");
        emitter.instruction("push rbp");                                        // save linkage and align the nested call
        emitter.instruction("mov rbp, rsp");                                    // establish a frame for the call
        emitter.instruction("mov rax, rsi");                                    // string pointer into the normalizer input
        emitter.instruction("call __rt_hash_normalize_key");                    // rax/rdx = normalized string or integer key
        emitter.instruction("mov rsi, rax");                                    // normalized key low word
        emitter.instruction("pop rbp");                                         // restore linkage
        emitter.instruction("xor eax, eax");                                    // status 0 = usable key
        emitter.instruction("ret");                                             // return the normalized key
        emitter.label("__rt_array_column_key_float");
        emitter.instruction("push rbp");                                        // save linkage and align the nested call
        emitter.instruction("mov rbp, rsp");                                    // establish a frame for the call
        emitter.instruction("movq xmm0, rsi");                                  // move the IEEE-754 payload into xmm0
        emitter.instruction("call __rt_float_key_to_int");                      // rax = PHP integer key (with diagnostics)
        emitter.instruction("mov rsi, rax");                                    // the converted integer is the key
        emitter.instruction("pop rbp");                                         // restore linkage
        emitter.instruction("jmp __rt_array_column_key_int");                   // finish as an integer key
        emitter.label("__rt_array_column_key_null");
        emitter.instruction("mov eax, 1");                                      // status 1 = null
        emitter.instruction("ret");                                             // return the null status
        emitter.label("__rt_array_column_key_array");
        emitter.instruction("mov eax, 2");                                      // status 2 = array
        emitter.instruction("ret");                                             // return the array status
        emitter.label("__rt_array_column_key_object");
        emitter.instruction("mov eax, 3");                                      // status 3 = object
        emitter.instruction("ret");                                             // return the object status
    }
}

/// Emits `__rt_array_column_prop(object, key_lo, key_hi)` → public property index or -1.
///
/// Walks the renderable property list (`__rt_obj_prop_count` / `__rt_obj_prop_name`) and accepts
/// a declared row only when its print_r key has no visibility suffix; dynamic properties are
/// always public. Integer keys and the empty name never match.
fn emit_public_property_lookup(emitter: &mut Emitter) {
    const OBJ: usize = 0;
    const NAME: usize = 8;
    const LEN: usize = 16;
    const COUNT: usize = 24;
    const POS: usize = 32;
    const LOOKUP_FRAME: usize = 64;
    let arm = emitter.target.arch == Arch::AArch64;
    let result = abi::int_result_reg(emitter);
    let arg = |index: usize| abi::int_arg_reg_name(emitter.target, index);
    let (arg0, arg1, arg2, arg3) = (arg(0), arg(1), arg(2), arg(3));
    emitter.blank();
    emitter.label_global("__rt_array_column_prop");
    op(emitter, "cmn x2, #1", "cmp rdx, -1");                                   // integer keys never name an object property here
    op(emitter, "b.eq __rt_array_column_prop_none", "je __rt_array_column_prop_none"); // report no property
    op(emitter, "cbz x2, __rt_array_column_prop_none", "test rdx, rdx");        // the empty name never matches
    if !arm {
        emitter.instruction("jz __rt_array_column_prop_none");                  // report no property
    }
    abi::emit_frame_prologue(emitter, LOOKUP_FRAME);
    for (reg, offset) in [(arg0, OBJ), (arg1, NAME), (arg2, LEN)] {
        abi::emit_store_to_sp(emitter, reg, offset);
    }
    abi::emit_call_label(emitter, "__rt_obj_prop_count");
    abi::emit_store_to_sp(emitter, result, COUNT);
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_store_to_sp(emitter, result, POS);

    emitter.label("__rt_array_column_prop_loop");
    abi::emit_load_temporary_stack_slot(emitter, arg1, POS);
    abi::emit_load_temporary_stack_slot(emitter, result, COUNT);
    op(emitter, "cmp x1, x0", "cmp rsi, rax");                                  // has every property been inspected?
    op(emitter, "b.hs __rt_array_column_prop_missing", "jae __rt_array_column_prop_missing"); // no property carries this name
    abi::emit_load_temporary_stack_slot(emitter, arg0, OBJ);
    abi::emit_call_label(emitter, "__rt_obj_prop_name");
    if arm {
        abi::emit_load_temporary_stack_slot(emitter, "x3", NAME);
        abi::emit_load_temporary_stack_slot(emitter, "x4", LEN);
    } else {
        emitter.instruction("mov rdi, rax");                                    // property name pointer as the first string
        emitter.instruction("mov rsi, rdx");                                    // property name length as the first length
        abi::emit_load_temporary_stack_slot(emitter, arg2, NAME);
        abi::emit_load_temporary_stack_slot(emitter, arg3, LEN);
    }
    abi::emit_call_label(emitter, "__rt_str_eq");
    abi::emit_branch_if_int_result_zero(emitter, "__rt_array_column_prop_next");

    // A matching declared row is public only when print_r renders it without a suffix.
    let (obj, table, pos) = if arm { ("x10", "x11", "x14") } else { ("r8", "r9", "rcx") };
    abi::emit_load_temporary_stack_slot(emitter, obj, OBJ);
    abi::emit_load_temporary_stack_slot(emitter, pos, POS);
    abi::emit_symbol_address(emitter, table, "_class_prop_desc_ptrs");
    if arm {
        emitter.instruction("ldr x10, [x10]");                                  // load the runtime class id
        emitter.instruction("ldr x11, [x11, x10, lsl #3]");                     // load the class property descriptor
        emitter.instruction("ldr x12, [x11]");                                  // load the declared row count
        emitter.instruction("cmp x14, x12");                                    // is this a dynamic property?
        emitter.instruction("b.hs __rt_array_column_prop_found");               // dynamic properties are public
        emitter.instruction("mov x13, #48");                                    // descriptor rows are 48 bytes
        emitter.instruction("mul x13, x14, x13");                               // byte offset of the matching row
        emitter.instruction("add x13, x11, x13");                               // advance into the descriptor
        emitter.instruction("ldr x12, [x13, #16]");                             // print_r key length (after the count word)
        emitter.instruction("ldr x13, [x13, #48]");                             // bare property-name length
        emitter.instruction("cmp x12, x13");                                    // a visibility suffix makes print_r longer
        emitter.instruction("b.ne __rt_array_column_prop_next");                // protected/private rows stay hidden
    } else {
        emitter.instruction("mov r8, QWORD PTR [r8]");                          // load the runtime class id
        emitter.instruction("mov r9, QWORD PTR [r9 + r8 * 8]");                 // load the class property descriptor
        emitter.instruction("mov r10, QWORD PTR [r9]");                         // load the declared row count
        emitter.instruction("cmp rcx, r10");                                    // is this a dynamic property?
        emitter.instruction("jae __rt_array_column_prop_found");                // dynamic properties are public
        emitter.instruction("imul r11, rcx, 48");                               // byte offset of the matching row
        emitter.instruction("add r11, r9");                                     // advance into the descriptor
        emitter.instruction("mov r10, QWORD PTR [r11 + 16]");                   // print_r key length (after the count word)
        emitter.instruction("cmp r10, QWORD PTR [r11 + 48]");                   // a visibility suffix makes print_r longer
        emitter.instruction("jne __rt_array_column_prop_next");                 // protected/private rows stay hidden
    }
    emitter.label("__rt_array_column_prop_found");
    abi::emit_load_temporary_stack_slot(emitter, result, POS);
    abi::emit_frame_restore(emitter, LOOKUP_FRAME);
    abi::emit_return(emitter);

    emitter.label("__rt_array_column_prop_next");
    abi::emit_load_temporary_stack_slot(emitter, result, POS);
    op(emitter, "add x0, x0, #1", "add rax, 1");                                // advance to the next property
    abi::emit_store_to_sp(emitter, result, POS);
    abi::emit_jump(emitter, "__rt_array_column_prop_loop");

    emitter.label("__rt_array_column_prop_missing");
    abi::emit_load_int_immediate(emitter, result, -1);
    abi::emit_frame_restore(emitter, LOOKUP_FRAME);
    abi::emit_return(emitter);
    emitter.label("__rt_array_column_prop_none");
    abi::emit_load_int_immediate(emitter, result, -1);
    abi::emit_return(emitter);
}

/// Emits `__rt_array_column_class_name(object)` → borrowed class-name pair.
///
/// Reads the dense `_class_name_entries` table; unknown or unnamed class ids fall back to
/// `object`. AArch64 takes x0 and returns x1/x2; x86_64 takes rdi and returns rsi/rdx.
fn emit_class_name(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_array_column_class_name");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("ldr x13, [x0]");                                   // load the object's dense class id (symbol loads clobber x9)
        abi::emit_load_symbol_to_reg(emitter, "x10", "_class_name_count", 0);
        emitter.instruction("cmp x13, x10");                                    // is the class id inside the name table?
        emitter.instruction("b.hs __rt_array_column_class_name_fallback");      // unknown ids use the generic spelling
        abi::emit_symbol_address(emitter, "x10", "_class_name_entries");
        emitter.instruction("add x10, x10, x13, lsl #4");                       // select the pointer/length row
        emitter.instruction("ldp x1, x2, [x10]");                               // borrow the class-name pointer and length
        emitter.instruction("cbz x2, __rt_array_column_class_name_fallback");   // unnamed ids use the generic spelling
        emitter.instruction("ret");                                             // return the borrowed class name
        emitter.label("__rt_array_column_class_name_fallback");
        abi::emit_symbol_address(emitter, "x1", "_unser_type_object");
        emitter.instruction("mov x2, #6");                                      // byte length of "object"
        emitter.instruction("ret");                                             // return the generic spelling
    } else {
        emitter.instruction("mov r8, QWORD PTR [rdi]");                         // load the object's dense class id
        abi::emit_load_symbol_to_reg(emitter, "r9", "_class_name_count", 0);
        emitter.instruction("cmp r8, r9");                                      // is the class id inside the name table?
        emitter.instruction("jae __rt_array_column_class_name_fallback");       // unknown ids use the generic spelling
        abi::emit_symbol_address(emitter, "r9", "_class_name_entries");
        emitter.instruction("shl r8, 4");                                       // scale the id by the row width
        emitter.instruction("add r9, r8");                                      // select the pointer/length row
        emitter.instruction("mov rsi, QWORD PTR [r9]");                         // borrow the class-name pointer
        emitter.instruction("mov rdx, QWORD PTR [r9 + 8]");                     // borrow the class-name length
        emitter.instruction("test rdx, rdx");                                   // is the generated name empty?
        emitter.instruction("jz __rt_array_column_class_name_fallback");        // unnamed ids use the generic spelling
        emitter.instruction("ret");                                             // return the borrowed class name
        emitter.label("__rt_array_column_class_name_fallback");
        abi::emit_symbol_address(emitter, "rsi", "_unser_type_object");
        emitter.instruction("mov edx, 6");                                      // byte length of "object"
        emitter.instruction("ret");                                             // return the generic spelling
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target probes presence before reading and transfers each owned read cell once.
    #[test]
    fn boxed_column_uses_presence_and_owned_reads_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_array_column_boxed(&mut emitter);
            let asm = emitter.output();
            let probe = asm.find("__rt_array_key_exists_mixed_key").unwrap();
            let read = asm.find("__rt_array_get_mixed_key").unwrap();
            let append = asm.find("__rt_array_push_int").unwrap();
            assert!(probe < read && read < append, "{target}");
            assert!(asm.contains("__rt_array_iter_next"), "{target}");
            assert!(asm.contains("__rt_hash_normalize_key"), "{target}");
            if target == "linux-x86_64" {
                assert!(asm.contains("mov r11, 0xffffffff000080ff"),
                    "the result must remain recognizable to heap_kind and typed decref");
                assert!(!asm.contains("mov r11, 0x80ff"), "do not erase the heap identity marker");
            }
            assert!(!asm.contains("__rt_incref"), "{target}: the read already owns its cell");
        }
    }

    /// The general walker handles index keys, null columns and object rows on every target.
    #[test]
    fn general_column_walker_covers_index_keys_and_objects_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_array_column_boxed(&mut emitter);
            let asm = emitter.output();
            for symbol in [
                "__rt_array_column_any:", "__rt_array_column_key:", "__rt_array_column_prop:",
                "__rt_hash_new", "__rt_hash_set", "__rt_hash_append", "__rt_float_key_to_int",
                "__rt_obj_prop_count", "__rt_obj_prop_name", "__rt_obj_prop_value",
                "_class_prop_desc_ptrs", "__rt_mixed_from_value", "__rt_decref_any",
            ] {
                assert!(asm.contains(symbol), "{target}: {symbol}");
            }
            // The index cell is released only after the hash persisted its key.
            let insert = asm.find("__rt_array_column_any_insert:").unwrap();
            let set = insert + asm[insert..].find("__rt_hash_set").unwrap();
            let release = insert + asm[insert..].find("__rt_decref_mixed").unwrap();
            assert!(set < release, "{target}");
            let boxed = asm.find("__rt_array_column_boxed:").unwrap();
            let tail = boxed + asm[boxed..].find("__rt_array_column_any").unwrap();
            assert!(tail < asm.find("__rt_array_column_any:").unwrap(), "{target}");
        }
    }
}
