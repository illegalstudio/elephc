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

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::runtime::exceptions::guards;
use crate::codegen_support::sentinels::emit_branch_if_null_container;

const FRAME_BYTES: usize = 624;
const OWNER_BASE: usize = 128;
const OWNER_COUNT: usize = 14;
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

/// Returns the word where a guard owns its current heap value.
fn owner(slot: usize) -> usize {
    OWNER_BASE + slot * 32 + 24
}

/// Saves a newly allocated or replaced owner before any subsequent allocation or PHP call.
fn own(emitter: &mut Emitter, slot: usize) {
    save(emitter, owner(slot), result(emitter));
}

/// Clears a transferred owner while retaining its empty exception record until normal return.
fn transfer(emitter: &mut Emitter, slot: usize) {
    abi::emit_store_zero_to_address(emitter, frame(emitter), owner(slot));
}

/// Retires one independent owner before calling its potentially throwing destructor.
fn release(emitter: &mut Emitter, slot: usize) {
    load(emitter, owner(slot), result(emitter));
    transfer(emitter, slot);
    abi::emit_call_label(emitter, "__rt_decref_any");
}

/// Boxes a value supplied in the target's native payload registers.
fn boxed(emitter: &mut Emitter, tag: i64, slot: usize) {
    abi::emit_load_int_immediate(emitter, result(emitter), tag);
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    own(emitter, slot);
}

/// Transfers a boxed owner into a newly populated brigade or bucket property.
fn property(emitter: &mut Emitter, object: usize, value: usize, name: &str, len: i64) {
    let arm = emitter.target.arch == Arch::AArch64;
    load(emitter, owner(value), if arm { "x3" } else { "rcx" });
    load(emitter, owner(object), if arm { "x0" } else { "rdi" });
    abi::emit_symbol_address(emitter, if arm { "x1" } else { "rsi" }, name);
    abi::emit_load_int_immediate(emitter, if arm { "x2" } else { "rdx" }, len);
    abi::emit_call_label(emitter, "__rt_stdclass_set");
    transfer(emitter, value);
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
    for slot in 0..OWNER_COUNT {
        abi::emit_load_int_immediate(emitter, ret, 0);
        let offset = OWNER_BASE + slot * 32;
        guards::guard(emitter, offset, FRAME_BYTES - 16 - offset);
    }
    // -- build the input bucket and transfer its property cells into its hash --
    abi::emit_call_label(emitter, "__rt_stdclass_new");
    own(emitter, IN);
    abi::emit_call_label(emitter, "__rt_stdclass_new");
    own(emitter, BUCKET);
    load(emitter, 8, if arm { "x1" } else { "rdi" });
    load(emitter, 16, if arm { "x2" } else { "rsi" });
    boxed(emitter, 1, VALUE);
    property(emitter, BUCKET, VALUE, "_brigade_data_key", 4);
    load(emitter, 16, if arm { "x1" } else { "rdi" });
    abi::emit_load_int_immediate(emitter, if arm { "x2" } else { "rsi" }, 0);
    boxed(emitter, 0, VALUE);
    property(emitter, BUCKET, VALUE, "_brigade_datalen_key", 7);
    // -- boxing retains each child, so retire its original independent owner --
    load(emitter, owner(BUCKET), if arm { "x1" } else { "rdi" });
    abi::emit_load_int_immediate(emitter, if arm { "x2" } else { "rsi" }, 0);
    boxed(emitter, 6, BOXED_BUCKET);
    release(emitter, BUCKET);
    abi::emit_load_int_immediate(emitter, arg0, 4);
    abi::emit_load_int_immediate(emitter, arg1, 8);
    abi::emit_call_label(emitter, "__rt_array_new");
    own(emitter, ARRAY);
    // The array consumes Mixed pointers and must retain that element shape during GC.
    if arm {
        emitter.instruction("ldr x9, [x0, #-8]");                               // preserve the array's allocation metadata
        emitter.instruction("mov x10, #0x80ff");                                // retain the kind and persistent COW bits
        emitter.instruction("and x9, x9, x10");                                 // clear the previous element-type metadata
        emitter.instruction("orr x9, x9, #0x700");                              // stamp boxed Mixed element ownership
        emitter.instruction("str x9, [x0, #-8]");                               // publish the shape before inserting the bucket
    } else {
        emitter.instruction("mov r9, QWORD PTR [rax - 8]");                     // preserve the array's allocation metadata
        emitter.instruction("mov r10, 0xffffffff000080ff");                     // retain the heap marker, kind and COW bits
        emitter.instruction("and r9, r10");                                     // clear the previous element-type metadata
        emitter.instruction("or r9, 0x700");                                    // stamp boxed Mixed element ownership
        emitter.instruction("mov QWORD PTR [rax - 8], r9");                     // publish the shape before inserting the bucket
    }
    load(emitter, owner(ARRAY), arg0);
    load(emitter, owner(BOXED_BUCKET), arg1);
    abi::emit_call_label(emitter, "__rt_array_push_int");
    own(emitter, ARRAY);
    transfer(emitter, BOXED_BUCKET);
    load(emitter, owner(ARRAY), if arm { "x1" } else { "rdi" });
    abi::emit_load_int_immediate(emitter, if arm { "x2" } else { "rsi" }, 0);
    boxed(emitter, 4, BOXED_ARRAY);
    release(emitter, ARRAY);
    property(emitter, IN, BOXED_ARRAY, "_brigade_buckets_key", 8);
    abi::emit_call_label(emitter, "__rt_stdclass_new");
    own(emitter, OUT);
    // -- select parameter representations from the source method signature --
    load(emitter, 0, arg0);
    emitter.instruction(if arm { "ldr x9, [x0]" } else { "mov r9, QWORD PTR [rdi]" });// recover the receiver class ID
    abi::emit_symbol_address(emitter, if arm { "x10" } else { "r10" }, "_user_filter_vtable_ptrs");
    // The symbol register differs on SysV, so the common lookup below is target-selected.
    emitter.instruction(if arm { "ldr x10, [x10, x9, lsl #3]" } else { "mov r10, QWORD PTR [r10 + r9 * 8]" });// select the class's filter table
    emitter.instruction(if arm { "ldr x9, [x10, #40]" } else { "mov r9, QWORD PTR [r10 + 40]" });// load the four visible parameter representation bits
    save(emitter, 120, if arm { "x9" } else { "r9" });
    for (bit, object, cell) in [(1, IN, BOXED_IN), (2, OUT, BOXED_OUT)] {
        load(emitter, 120, if arm { "x9" } else { "r9" });
        emitter.instruction(&if arm { format!("tst x9, #{bit}") } else { format!("test r9, {bit}") });// only Mixed parameters need an object box
        emitter.instruction(&format!("{} __rt_ufbi_boxed_{cell}", if arm { "b.eq" } else { "jz" }));// raw object parameters borrow the brigade directly
        load(emitter, owner(object), if arm { "x1" } else { "rdi" });
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
        load(emitter, owner(cell), reg);
        emitter.instruction(&if arm { format!("cbnz {reg}, __rt_ufbi_arg_{cell}") } else { format!("test {reg}, {reg}") });// prefer the boxed object when the signature requires it
        if !arm { emitter.instruction(&format!("jnz __rt_ufbi_arg_{cell}")); }  // retain the boxed parameter representation
        load(emitter, owner(object), reg);
        emitter.label(&format!("__rt_ufbi_arg_{cell}"));
    }
    load(emitter, owner(CONSUMED), arg3);
    emitter.instruction(if arm { "cbz x3, __rt_ufbi_consumed_scalar" } else { "test rcx, rcx" });// Mixed references point to the owner's pointer word
    if !arm { emitter.instruction("jz __rt_ufbi_consumed_scalar"); }            // native integer references use their scalar word instead
    emitter.instruction(&if arm { format!("add x3, sp, #{}", owner(CONSUMED)) }
        else { format!("lea rcx, [rsp + {}]", owner(CONSUMED)) });              // writes replace the cell tracked by the existing exception guard
    abi::emit_jump(emitter, "__rt_ufbi_consumed_ready");
    emitter.label("__rt_ufbi_consumed_scalar");
    emitter.instruction(if arm { "add x3, sp, #112" } else { "lea rcx, [rsp + 112]" });// native integer references borrow a scalar word
    emitter.label("__rt_ufbi_consumed_ready");
    load(emitter, owner(CLOSING), if arm { "x4" } else { "r8" });
    load(emitter, 0, arg0);
    load(emitter, 24, if arm { "x11" } else { "r11" });
    abi::emit_call_reg(emitter, if arm { "x11" } else { "r11" });
    // -- retain the output array while property getters and result growth can allocate --
    load(emitter, owner(OUT), arg0);
    abi::emit_symbol_address(emitter, arg1, "_brigade_buckets_key");
    abi::emit_load_int_immediate(emitter, arg2, 8);
    abi::emit_call_label(emitter, "__rt_stdclass_get");
    own(emitter, OUTPUT_ARRAY);
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
    emitter.instruction(if arm { "ldr x10, [x9]" } else { "mov r10, QWORD PTR [r9]" });// bound the walk by the array length
    save(emitter, 40, if arm { "x10" } else { "r10" });
    abi::emit_store_zero_to_address(emitter, frame(emitter), 48);
    emitter.label("__rt_ufbi_walk");
    load(emitter, 48, if arm { "x10" } else { "r10" });
    load(emitter, 40, if arm { "x11" } else { "r11" });
    emitter.instruction(if arm { "cmp x10, x11" } else { "cmp r10, r11" });     // stop after the final output bucket
    emitter.instruction(if arm { "b.hs __rt_ufbi_done" } else { "jae __rt_ufbi_done" });// all selected bytes now belong to the result
    load(emitter, 32, if arm { "x9" } else { "r9" });
    emitter.instruction(if arm { "add x9, x9, #24" } else { "add r9, 24" });    // skip the indexed array header
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
    own(emitter, DATA);
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
    load(emitter, owner(RESULT), if arm { "x0" } else { "rax" });
    load(emitter, 56, if arm { "x1" } else { "rdi" });
    load(emitter, 72, if arm { "x2" } else { "rsi" });
    abi::emit_call_label(emitter, "__rt_concat_grow");
    // The grow helper returns transient concat storage. This native lease must be
    // independently releasable even if a later getter or destructor throws.
    if arm {
        emitter.instruction("mov x9, #1");                                      // make the result an ordinary owned string
        emitter.instruction("str x9, [x0, #-8]");                               // expose the result to generic exceptional release
    } else {
        emitter.instruction(&format!("mov r9, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1)));// preserve the validated x86 heap marker
        emitter.instruction("mov QWORD PTR [rax - 8], r9");                     // expose the result to generic exceptional release
    }
    own(emitter, RESULT);
    emitter.label("__rt_ufbi_copy");
    load(emitter, 80, if arm { "x9" } else { "r9" });
    load(emitter, owner(RESULT), if arm { "x10" } else { "r10" });
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
    release(emitter, DATA);
    emitter.label("__rt_ufbi_next");
    load(emitter, 48, if arm { "x9" } else { "r9" });
    emitter.instruction(if arm { "add x9, x9, #1" } else { "inc r9" });         // advance the output bucket index after retiring its getter
    save(emitter, 48, if arm { "x9" } else { "r9" });
    abi::emit_jump(emitter, "__rt_ufbi_walk");
    // -- cleanup can invoke destructors while the independent output remains guarded --
    emitter.label("__rt_ufbi_done");
    for slot in (0..OWNER_COUNT).rev().filter(|slot| *slot != RESULT) {
        release(emitter, slot);
    }
    load(emitter, owner(RESULT), ret);
    save(emitter, 64, ret);
    transfer(emitter, RESULT);
    for slot in (0..OWNER_COUNT).rev() {
        let offset = OWNER_BASE + slot * 32;
        guards::unguard(emitter, offset, FRAME_BYTES - 16 - offset);
    }
    load(emitter, 64, if arm { "x1" } else { "rax" });
    load(emitter, 56, if arm { "x2" } else { "rdx" });
    abi::emit_frame_restore(emitter, FRAME_BYTES);
    abi::emit_return(emitter);
}
