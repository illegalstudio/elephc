//! Purpose:
//! Invokes PHP bucket-brigade filters with balanced native temporary ownership.
//!
//! Called from:
//! - `__rt_apply_user_stream_filter` for four-argument userspace filters.
//!
//! Key details:
//! - Every native owner is visible to the exception activation chain before PHP can run.
//! - Property and array insertion transfer their boxed owners; boxing retains its child.
//! - Callback parameter metadata selects boxed objects, scalar references and closing flags.
//! - Output grows in owned storage instead of overflowing the shared 64 KiB filter buffer.

use elephc_memoir::buffers::stream_tables;
use elephc_memoir::strings::str_new_owned;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use elephc_memoir::{FrameOffset, guarded_owners::GuardedFrameOwners};
use crate::codegen_support::sentinels::emit_branch_if_null_container;

const FRAME_BYTES: usize = 624;
const OWNERS: GuardedFrameOwners = GuardedFrameOwners::new(FrameOffset::new(128), 14, FRAME_BYTES);
const IN: usize = 0;
const BUCKET: usize = 1;
const VALUE: usize = 2;
const BOXED_BUCKET: usize = 3;
const ARRAY: usize = 4;
const BOXED_ARRAY: usize = 5;
const OUT: usize = 6;
const CONSUMED: usize = 7;
const CLOSING: usize = 8;
const OUTPUT_ARRAY: usize = 9;
const DATA: usize = 10;
const RESULT: usize = 11;
const BOXED_IN: usize = 12;
const BOXED_OUT: usize = 13;

/// Returns the native result register for pointers and boxed cells.
fn result(emitter: &Emitter) -> &'static str {
    if emitter.target.arch == Arch::AArch64 { "x0" } else { "rax" }
}

/// Returns the stack base shared by the fixed locals and owner records.
fn frame(emitter: &Emitter) -> &'static str {
    if emitter.target.arch == Arch::AArch64 { "sp" } else { "rsp" }
}

/// Loads one fixed native frame word into the requested register.
fn load(emitter: &mut Emitter, offset: usize, reg: &str) {
    abi::emit_load_from_address(emitter, reg, frame(emitter), offset);
}

/// Saves a native register without changing any other live argument.
fn save(emitter: &mut Emitter, offset: usize, reg: &str) {
    abi::emit_store_to_address(emitter, reg, frame(emitter), offset);
}

/// Boxes a value supplied in the target's native payload registers.
fn boxed(emitter: &mut Emitter, tag: i64, slot: usize) {
    abi::emit_load_int_immediate(emitter, result(emitter), tag);
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    OWNERS.emit_save(emitter, slot);
}

/// Transfers a boxed owner into a newly populated brigade or bucket property.
fn property(emitter: &mut Emitter, object: usize, value: usize, name: &str, len: i64) {
    let arm = emitter.target.arch == Arch::AArch64;
    load(emitter, OWNERS.owner_offset(value), if arm { "x3" } else { "rcx" });
    load(emitter, OWNERS.owner_offset(object), if arm { "x0" } else { "rdi" });
    abi::emit_symbol_address(emitter, if arm { "x1" } else { "rsi" }, name);
    abi::emit_load_int_immediate(emitter, if arm { "x2" } else { "rdx" }, len);
    abi::emit_call_label(emitter, "__rt_stdclass_set");
    OWNERS.emit_clear(emitter, value);
}

/// Emits a four-argument filter call with independent guarded brigades and an owned result.
pub fn emit_user_filter_brigade_invoke(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let ret = result(emitter);
    let (arg0, arg1, arg2, arg3) = if arm { ("x0", "x1", "x2", "x3") }
        else { ("rdi", "rsi", "rdx", "rcx") };
    emitter.blank();
    emitter.comment("--- runtime: user_filter_brigade_invoke ---");
    emitter.label_global("__rt_user_filter_brigade_invoke");
    // -- save borrowed invocation inputs before publishing empty native owner guards --
    abi::emit_frame_prologue(emitter, FRAME_BYTES);
    for (offset, reg) in [(0, arg0), (8, arg1), (16, arg2), (24, arg3)] {
        save(emitter, offset, reg);
    }
    OWNERS.emit_publish_empty(emitter);
    // -- build the input bucket and transfer its property cells into its hash --
    abi::emit_call_label(emitter, "__rt_stdclass_new");
    OWNERS.emit_save(emitter, IN);
    abi::emit_call_label(emitter, "__rt_stdclass_new");
    OWNERS.emit_save(emitter, BUCKET);
    load(emitter, 8, if arm { "x1" } else { "rdi" });
    load(emitter, 16, if arm { "x2" } else { "rsi" });
    boxed(emitter, 1, VALUE);
    property(emitter, BUCKET, VALUE, "_brigade_data_key", 4);
    load(emitter, 16, if arm { "x1" } else { "rdi" });
    abi::emit_load_int_immediate(emitter, if arm { "x2" } else { "rsi" }, 0);
    boxed(emitter, 0, VALUE);
    property(emitter, BUCKET, VALUE, "_brigade_datalen_key", 7);
    // -- boxing retains each child, so retire its original independent owner --
    load(emitter, OWNERS.owner_offset(BUCKET), if arm { "x1" } else { "rdi" });
    abi::emit_load_int_immediate(emitter, if arm { "x2" } else { "rsi" }, 0);
    boxed(emitter, 6, BOXED_BUCKET);
    OWNERS.emit_release(emitter, BUCKET);
    abi::emit_load_int_immediate(emitter, arg0, 4);
    abi::emit_load_int_immediate(emitter, arg1, 8);
    abi::emit_call_label(emitter, "__rt_array_new");
    OWNERS.emit_save(emitter, ARRAY);
    // The array consumes Mixed pointers and must retain that element shape during GC.
    elephc_memoir::values::array_header::emit_result_value_type_stamp(emitter, elephc_memoir::ValueTag::new(7));
    load(emitter, OWNERS.owner_offset(ARRAY), arg0);
    load(emitter, OWNERS.owner_offset(BOXED_BUCKET), arg1);
    abi::emit_call_label(emitter, "__rt_array_push_int");
    OWNERS.emit_save(emitter, ARRAY);
    OWNERS.emit_clear(emitter, BOXED_BUCKET);
    load(emitter, OWNERS.owner_offset(ARRAY), if arm { "x1" } else { "rdi" });
    abi::emit_load_int_immediate(emitter, if arm { "x2" } else { "rsi" }, 0);
    boxed(emitter, 4, BOXED_ARRAY);
    OWNERS.emit_release(emitter, ARRAY);
    property(emitter, IN, BOXED_ARRAY, "_brigade_buckets_key", 8);
    abi::emit_call_label(emitter, "__rt_stdclass_new");
    OWNERS.emit_save(emitter, OUT);
    // -- select parameter representations from the source method signature --
    load(emitter, 0, arg0);
    emitter.instruction(if arm { "ldr x9, [x0]" } else { "mov r9, QWORD PTR [rdi]" });// recover the receiver class ID
    abi::emit_symbol_address(emitter, if arm { "x10" } else { "r10" }, "_user_filter_vtable_ptrs");
    // The symbol register differs on SysV, so the common lookup below is target-selected.
    emitter.instruction(if arm { "ldr x10, [x10, x9, lsl #3]" } else { "mov r10, QWORD PTR [r10 + r9 * 8]" });// select the class's filter table
    stream_tables::emit_load_user_filter_vtable_field(
        emitter,
        if arm { "x9" } else { "r9" },
        if arm { "x10" } else { "r10" },
        stream_tables::UserFilterVtableField::BoxedParams,
    );
    save(emitter, 120, if arm { "x9" } else { "r9" });
    for (bit, object, cell) in [(1, IN, BOXED_IN), (2, OUT, BOXED_OUT)] {
        load(emitter, 120, if arm { "x9" } else { "r9" });
        emitter.instruction(&if arm { format!("tst x9, #{bit}") } else { format!("test r9, {bit}") });// only Mixed parameters need an object box
        emitter.instruction(&format!("{} __rt_ufbi_boxed_{cell}", if arm { "b.eq" } else { "jz" }));// raw object parameters borrow the brigade directly
        load(emitter, OWNERS.owner_offset(object), if arm { "x1" } else { "rdi" });
        abi::emit_load_int_immediate(emitter, if arm { "x2" } else { "rsi" }, 0);
        boxed(emitter, 6, cell);
        emitter.label(&format!("__rt_ufbi_boxed_{cell}"));
    }
    abi::emit_store_zero_to_address(emitter, frame(emitter), 112);
    for (bit, tag, slot) in [(4, 0, CONSUMED), (8, 3, CLOSING)] {
        load(emitter, 120, if arm { "x9" } else { "r9" });
        emitter.instruction(&if arm { format!("tst x9, #{bit}") } else { format!("test r9, {bit}") });// scalar parameters use their native representation
        emitter.instruction(&format!("{} __rt_ufbi_scalar_{slot}", if arm { "b.eq" } else { "jz" }));// omit unused boxed scalar owners
        abi::emit_load_int_immediate(emitter, if arm { "x1" } else { "rdi" }, 0);
        abi::emit_load_int_immediate(emitter, if arm { "x2" } else { "rsi" }, 0);
        boxed(emitter, tag, slot);
        emitter.label(&format!("__rt_ufbi_scalar_{slot}"));
    }
    // -- borrow the prepared owners through the user's declared method ABI --
    for (object, cell, reg) in [(IN, BOXED_IN, arg1), (OUT, BOXED_OUT, arg2)] {
        load(emitter, OWNERS.owner_offset(cell), reg);
        emitter.instruction(&if arm { format!("cbnz {reg}, __rt_ufbi_arg_{cell}") } else { format!("test {reg}, {reg}") });// prefer the boxed object when the signature requires it
        if !arm { emitter.instruction(&format!("jnz __rt_ufbi_arg_{cell}")); }  // retain the boxed parameter representation
        load(emitter, OWNERS.owner_offset(object), reg);
        emitter.label(&format!("__rt_ufbi_arg_{cell}"));
    }
    load(emitter, OWNERS.owner_offset(CONSUMED), arg3);
    emitter.instruction(if arm { "cbz x3, __rt_ufbi_consumed_scalar" } else { "test rcx, rcx" });// Mixed references point to the owner's pointer word
    if !arm { emitter.instruction("jz __rt_ufbi_consumed_scalar"); }            // native integer references use their scalar word instead
    emitter.instruction(&if arm { format!("add x3, sp, #{}", OWNERS.owner_offset(CONSUMED)) }
        else { format!("lea rcx, [rsp + {}]", OWNERS.owner_offset(CONSUMED)) });              // writes replace the cell tracked by the existing exception guard
    abi::emit_jump(emitter, "__rt_ufbi_consumed_ready");
    emitter.label("__rt_ufbi_consumed_scalar");
    emitter.instruction(if arm { "add x3, sp, #112" } else { "lea rcx, [rsp + 112]" });// native integer references borrow a scalar word
    emitter.label("__rt_ufbi_consumed_ready");
    load(emitter, OWNERS.owner_offset(CLOSING), if arm { "x4" } else { "r8" });
    load(emitter, 0, arg0);
    load(emitter, 24, if arm { "x11" } else { "r11" });
    abi::emit_call_reg(emitter, if arm { "x11" } else { "r11" });
    // -- retain the output array while property getters and result growth can allocate --
    load(emitter, OWNERS.owner_offset(OUT), arg0);
    abi::emit_symbol_address(emitter, arg1, "_brigade_buckets_key");
    abi::emit_load_int_immediate(emitter, arg2, 8);
    abi::emit_call_label(emitter, "__rt_stdclass_get");
    OWNERS.emit_save(emitter, OUTPUT_ARRAY);
    abi::emit_store_zero_to_address(emitter, frame(emitter), 56);
    abi::emit_store_zero_to_address(emitter, frame(emitter), 72);
    emitter.instruction(if arm { "cbz x0, __rt_ufbi_done" } else { "test rax, rax" });// absent output leaves an empty result
    if !arm { emitter.instruction("jz __rt_ufbi_done"); }                       // skip an absent output cell
    emitter.instruction(if arm { "ldr x9, [x0]" } else { "mov r9, QWORD PTR [rax]" });// inspect the output property's PHP tag
    emitter.instruction(if arm { "cmp x9, #4" } else { "cmp r9, 4" });          // bucket brigades carry indexed arrays
    emitter.instruction(if arm { "b.ne __rt_ufbi_done" } else { "jne __rt_ufbi_done" });// ignore malformed output properties
    emitter.instruction(if arm { "ldr x9, [x0, #8]" } else { "mov r9, QWORD PTR [rax + 8]" });// borrow the retained bucket array
    emit_branch_if_null_container(emitter, if arm { "x9" } else { "r9" }, if arm { "x10" } else { "r10" }, "__rt_ufbi_done");
    save(emitter, 32, if arm { "x9" } else { "r9" });
    let (length, array) = if arm { ("x10", "x9") } else { ("r10", "r9") };
    elephc_memoir::values::packed_slots::emit_load_length(emitter, length, array);
    save(emitter, 40, if arm { "x10" } else { "r10" });
    abi::emit_store_zero_to_address(emitter, frame(emitter), 48);
    emitter.label("__rt_ufbi_walk");
    load(emitter, 48, if arm { "x10" } else { "r10" });
    load(emitter, 40, if arm { "x11" } else { "r11" });
    emitter.instruction(if arm { "cmp x10, x11" } else { "cmp r10, r11" });     // stop after the final output bucket
    emitter.instruction(if arm { "b.hs __rt_ufbi_done" } else { "jae __rt_ufbi_done" });// all selected bytes now belong to the result
    load(emitter, 32, if arm { "x9" } else { "r9" });
    elephc_memoir::values::packed_slots::emit_packed_payload(emitter, if arm { "x9" } else { "r9" }, if arm { "x9" } else { "r9" });
    emitter.instruction(if arm { "ldr x0, [x9, x10, lsl #3]" } else { "mov rax, QWORD PTR [r9 + r10 * 8]" });// borrow the next boxed bucket
    emitter.instruction(if arm { "cbz x0, __rt_ufbi_next" } else { "test rax, rax" });// empty array slots carry no bucket
    if !arm { emitter.instruction("jz __rt_ufbi_next"); }                       // skip an empty bucket slot
    emitter.instruction(if arm { "ldr x9, [x0]" } else { "mov r9, QWORD PTR [rax]" });// inspect the boxed bucket tag
    emitter.instruction(if arm { "cmp x9, #6" } else { "cmp r9, 6" });          // every usable bucket is an object
    emitter.instruction(if arm { "b.ne __rt_ufbi_next" } else { "jne __rt_ufbi_next" });// ignore non-object slots
    emitter.instruction(if arm { "ldr x0, [x0, #8]" } else { "mov rdi, QWORD PTR [rax + 8]" });// borrow the bucket receiver
    emit_branch_if_null_container(emitter, arg0, if arm { "x9" } else { "r9" }, "__rt_ufbi_next");
    abi::emit_symbol_address(emitter, arg1, "_brigade_data_key");
    abi::emit_load_int_immediate(emitter, arg2, 4);
    abi::emit_call_label(emitter, "__rt_stdclass_get");
    OWNERS.emit_save(emitter, DATA);
    emitter.instruction(if arm { "ldr x9, [x0]" } else { "mov r9, QWORD PTR [rax]" });// getters return an owned Mixed cell, including null
    emitter.instruction(if arm { "cmp x9, #1" } else { "cmp r9, 1" });          // concatenate string payloads only
    emitter.instruction(if arm { "b.ne __rt_ufbi_data_done" } else { "jne __rt_ufbi_data_done" });// malformed values still retire their getter owner
    for (field, offset) in [(8, 80), (16, 88)] {
        abi::emit_load_from_address(emitter, if arm { "x9" } else { "r9" }, ret, field);
        save(emitter, offset, if arm { "x9" } else { "r9" });
    }
    load(emitter, 56, if arm { "x9" } else { "r9" });
    load(emitter, 88, if arm { "x10" } else { "r10" });
    emitter.instruction(if arm { "add x9, x9, x10" } else { "add r9, r10" });   // compute the total bytes needed after this bucket
    save(emitter, 96, if arm { "x9" } else { "r9" });
    load(emitter, 72, if arm { "x10" } else { "r10" });
    emitter.instruction(if arm { "cmp x9, x10" } else { "cmp r9, r10" });       // reuse the current owned capacity when it is sufficient
    emitter.instruction(if arm { "b.ls __rt_ufbi_copy" } else { "jbe __rt_ufbi_copy" });// avoid reallocating between small buckets
    emitter.instruction(if arm { "add x9, x9, x10" } else { "add r9, r10" });   // grow geometrically as well as covering this bucket
    save(emitter, 72, if arm { "x9" } else { "r9" });
    load(emitter, OWNERS.owner_offset(RESULT), if arm { "x0" } else { "rax" });
    load(emitter, 56, if arm { "x1" } else { "rdi" });
    load(emitter, 72, if arm { "x2" } else { "rsi" });
    // Republish an independent string owner before any subsequent destructor can throw.
    str_new_owned::emit_grow_owned_string(emitter, "r9");
    OWNERS.emit_save(emitter, RESULT);
    emitter.label("__rt_ufbi_copy");
    load(emitter, 80, if arm { "x9" } else { "r9" });
    load(emitter, OWNERS.owner_offset(RESULT), if arm { "x10" } else { "r10" });
    load(emitter, 88, if arm { "x11" } else { "r11" });
    load(emitter, 56, if arm { "x12" } else { "r8" });
    abi::emit_load_int_immediate(emitter, if arm { "x14" } else { "rcx" }, 0);
    emitter.label("__rt_ufbi_copy_byte");
    emitter.instruction(if arm { "cmp x14, x11" } else { "cmp rcx, r11" });     // bound the byte copy by the retained string length
    emitter.instruction(if arm { "b.hs __rt_ufbi_copy_done" } else { "jae __rt_ufbi_copy_done" });// finish copying this bucket
    emitter.instruction(if arm { "ldrb w13, [x9, x14]" } else { "movzx eax, BYTE PTR [r9 + rcx]" });// read one byte without using any callee-saved register
    emitter.instruction(if arm { "strb w13, [x10, x12]" } else { "mov BYTE PTR [r10 + r8], al" });// append within the checked owned capacity
    emitter.instruction(if arm { "add x14, x14, #1" } else { "inc rcx" });      // advance the source offset
    emitter.instruction(if arm { "add x12, x12, #1" } else { "inc r8" });       // advance the result offset
    abi::emit_jump(emitter, "__rt_ufbi_copy_byte");
    emitter.label("__rt_ufbi_copy_done");
    load(emitter, 96, if arm { "x9" } else { "r9" });
    save(emitter, 56, if arm { "x9" } else { "r9" });
    emitter.label("__rt_ufbi_data_done");
    OWNERS.emit_release(emitter, DATA);
    emitter.label("__rt_ufbi_next");
    load(emitter, 48, if arm { "x9" } else { "r9" });
    emitter.instruction(if arm { "add x9, x9, #1" } else { "inc r9" });         // advance the output bucket index after retiring its getter
    save(emitter, 48, if arm { "x9" } else { "r9" });
    abi::emit_jump(emitter, "__rt_ufbi_walk");
    // -- cleanup can invoke destructors while the independent output remains guarded --
    emitter.label("__rt_ufbi_done");
    OWNERS.emit_release_except(emitter, RESULT);
    load(emitter, OWNERS.owner_offset(RESULT), ret);
    save(emitter, 64, ret);
    OWNERS.emit_clear(emitter, RESULT);
    OWNERS.emit_detach(emitter);
    load(emitter, 64, if arm { "x1" } else { "rax" });
    load(emitter, 56, if arm { "x2" } else { "rdx" });
    abi::emit_frame_restore(emitter, FRAME_BYTES);
    abi::emit_return(emitter);
}
