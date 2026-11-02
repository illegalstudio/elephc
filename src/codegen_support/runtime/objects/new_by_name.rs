//! Purpose:
//! Emits the `__rt_new_by_name` runtime helper that allocates an instance
//! of a class identified by a string name (Phase 10 step 2). The lookup
//! consults the `_classes_by_name` data table emitted by
//! `crate::codegen_support::runtime::data::user::emit_classes_by_name_table`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::objects`.
//! - `crate::codegen::lower_inst::objects` for
//!   `new $variable()` expressions.
//!
//! Key details:
//! - Each `_classes_by_name` entry is 32 bytes: name_ptr (8) + name_len
//!   (8) + class_id (8) + obj_size (8). A linear scan compares lengths
//!   first, then delegates to `__rt_strcasecmp` for PHP-style class lookup.
//! - On match: allocates obj_size bytes through `__rt_heap_alloc`, stamps
//!   the uniform heap-kind word (heap kind 4 = object) ahead of the
//!   payload, writes the class id at offset 0, and zeroes the property
//!   region so later property-store paths see clean memory.
//! - Zeroing is not the whole layout, though, and everything the DIRECT
//!   allocator (`lower_inst::objects::emit_object_allocation`) does after its
//!   own zero-fill has to be reproduced here from a class id known only at run
//!   time: the typed-uninitialized markers (`_class_uninit_prop_*`) and the
//!   object-owned reference cells (`_class_ref_prop_*`). An owned reference
//!   slot holds a POINTER to a 16-byte cell rather than a value, and the
//!   property-default thunk called next stores THROUGH it — so a slot left at
//!   zero is a null store inside compiled code, with no PHP-level diagnostic.
//! - On miss: returns 0 (null), which EIR object lowering boxes as PHP
//!   null (`gettype()` reports "NULL").

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};


/// new_by_name: instantiate a class by its textual name.
/// Input:  AArch64 x1 = name pointer, x2 = name length
///         x86_64  rax = name pointer, rdx = name length
/// Output: object pointer, or 0 when no class with that name is known.
pub fn emit_new_by_name(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_new_by_name_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: new_by_name ---");
    emitter.label_global("__rt_new_by_name");

    // Frame (64 bytes): [0..16) saved x29/x30, [16) name_ptr, [24) name_len,
    //   [32) matched class_id, [40) matched obj_size, [48) entry cursor,
    //   [56) entry index saved across __rt_strcasecmp.
    emitter.instruction("sub sp, sp, #64");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("bl __rt_class_alias_resolve");                         // a class_alias() name instantiates its class
    emitter.instruction("str x1, [sp, #16]");                                   // save the name pointer
    emitter.instruction("str x2, [sp, #24]");                                   // save the name length

    // -- load the lookup-table cursor + bound --
    abi::emit_symbol_address(emitter, "x9", "_classes_by_name_count");
    emitter.instruction("ldr x9, [x9]");                                        // x9 = entry count
    emitter.instruction("cbz x9, __rt_nbn_miss");                               // empty registry → no match
    abi::emit_symbol_address(emitter, "x10", "_classes_by_name");
    emitter.instruction("str x10, [sp, #48]");                                  // initialise the entry cursor
    emitter.instruction("mov x11, #0");                                         // entry index

    emitter.label("__rt_nbn_loop");
    emitter.instruction("cmp x11, x9");                                         // scanned every registered class?
    emitter.instruction("b.ge __rt_nbn_miss");                                  // exhausted the table without a match
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload the entry cursor
    emitter.instruction("ldr x13, [x10, #8]");                                  // stored name length
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload the input name length
    emitter.instruction("cmp x13, x2");                                         // length mismatch → skip
    emitter.instruction("b.ne __rt_nbn_skip");                                  // skip this class when the name lengths differ
    emitter.instruction("str x11, [sp, #56]");                                  // save the entry index across the string helper
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the input name pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload the input name length
    emitter.instruction("ldr x3, [x10]");                                       // stored class-name pointer
    emitter.instruction("mov x4, x13");                                         // stored class-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare class names case-insensitively
    emitter.instruction("ldr x11, [sp, #56]");                                  // restore the entry index after the string helper
    emitter.instruction("cmp x0, #0");                                          // did the class names match case-insensitively?
    emitter.instruction("b.eq __rt_nbn_match");                                 // full match: allocate the object
    emitter.instruction("b __rt_nbn_skip");                                     // mismatch: try the next entry

    emitter.label("__rt_nbn_skip");
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload the entry cursor
    emitter.instruction("add x10, x10, #32");                                   // advance to the next 32-byte entry
    emitter.instruction("str x10, [sp, #48]");                                  // persist the cursor
    emitter.instruction("add x11, x11, #1");                                    // advance the entry index
    abi::emit_symbol_address(emitter, "x9", "_classes_by_name_count");
    emitter.instruction("ldr x9, [x9]");                                        // reload the count (lost across the table walk)
    emitter.instruction("b __rt_nbn_loop");                                     // continue scanning

    emitter.label("__rt_nbn_match");
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload the matched entry cursor
    emitter.instruction("ldr x12, [x10, #16]");                                 // class_id
    emitter.instruction("ldr x13, [x10, #24]");                                 // obj_size
    emitter.instruction("str x12, [sp, #32]");                                  // save class_id across the heap call
    emitter.instruction("str x13, [sp, #40]");                                  // save obj_size across the heap call
    abi::emit_symbol_address(emitter, "x10", "_class_initcheck_ptrs");
    emitter.instruction("ldr x10, [x10, x12, lsl #3]");                         // preallocation default-error thunk
    emitter.instruction("cbz x10, __rt_nbn_initcheck_ok");                      // valid defaults need no early check
    emitter.instruction("mov x0, #0");                                          // invalid thunk never uses a this operand
    emitter.instruction("blr x10");                                             // raise the EIR default error before allocating
    emitter.label("__rt_nbn_initcheck_ok");
    emit_runtime_managed_match_aarch64(emitter);

    // -- allocate the object payload --
    emitter.label("__rt_nbn_generic_alloc");
    emitter.instruction("mov x0, x13");                                         // allocation size
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = object pointer
    emitter.instruction("mov x9, #4");                                          // heap kind 4 = object instance
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the uniform heap header
    emitter.instruction("bl __rt_object_handle_acquire");                       // bind the new object to its PHP object handle
    emitter.instruction("ldr x12, [sp, #32]");                                  // reload class_id
    emitter.instruction("str x12, [x0]");                                       // class_id at offset 0

    // -- zero the property region [obj+8 .. obj+obj_size) --
    emitter.instruction("ldr x13, [sp, #40]");                                  // obj_size
    emitter.instruction("mov x14, #8");                                         // start past the class_id header
    emitter.label("__rt_nbn_zero");
    emitter.instruction("cmp x14, x13");                                        // every byte zeroed?
    emitter.instruction("b.ge __rt_nbn_done");                                  // property region cleared
    emitter.instruction("str xzr, [x0, x14]");                                  // 8-byte zero store
    emitter.instruction("add x14, x14, #8");                                    // advance the zero cursor
    emitter.instruction("b __rt_nbn_zero");                                     // continue zeroing

    emitter.label("__rt_nbn_done");
    // -- restore typed-uninitialized markers after zeroing the selected layout --
    emitter.instruction("ldr x12, [sp, #32]");                                  // reload the matched class id for marker-table lookup
    abi::emit_symbol_address(emitter, "x9", "_class_uninit_prop_counts");
    emitter.instruction("ldr x10, [x9, x12, lsl #3]");                          // load the number of typed-uninitialized slots
    emitter.instruction("cbz x10, __rt_nbn_no_uninit_markers");                 // a class without such slots needs no marker stores
    abi::emit_symbol_address(emitter, "x9", "_class_uninit_prop_offset_ptrs");
    emitter.instruction("ldr x9, [x9, x12, lsl #3]");                           // load the physical-offset table for this class
    emitter.instruction("movz x11, #0xfffd");                                   // materialize the typed-uninitialized sentinel low bits
    emitter.instruction("movk x11, #0xffff, lsl #16");                          // materialize the typed-uninitialized sentinel bits 16..31
    emitter.instruction("movk x11, #0xffff, lsl #32");                          // materialize the typed-uninitialized sentinel bits 32..47
    emitter.instruction("movk x11, #0x7fff, lsl #48");                          // materialize the typed-uninitialized sentinel high bits
    emitter.instruction("mov x13, #0");                                         // start at the first marker offset
    emitter.label("__rt_nbn_uninit_marker_loop");
    emitter.instruction("cmp x13, x10");                                        // have all typed-uninitialized slots been marked?
    emitter.instruction("b.ge __rt_nbn_no_uninit_markers");                     // continue once every marker is in place
    emitter.instruction("ldr x14, [x9, x13, lsl #3]");                          // load one property high-word offset
    emitter.instruction("str x11, [x0, x14]");                                  // stamp the slot as PHP-typed but uninitialized
    emitter.instruction("add x13, x13, #1");                                    // advance to the next marker offset
    emitter.instruction("b __rt_nbn_uninit_marker_loop");                       // continue marking this object's typed slots
    emitter.label("__rt_nbn_no_uninit_markers");
    // -- allocate the ref-cells this layout's object-owned reference properties need --
    // An owned reference slot holds a POINTER to a 16-byte cell, not a value: every read and
    // every write of the property dereferences it, including the property-default stores in
    // `_class_propinit_<id>` below. The direct EIR allocator allocates those cells right after
    // zeroing the layout; the by-name allocator must do the same or the very next instruction
    // stores through the null the zero-fill left behind.
    emitter.instruction("ldr x12, [sp, #32]");                                  // reload the matched class id for the reference-cell table lookup
    abi::emit_symbol_address(emitter, "x9", "_class_ref_prop_counts");
    emitter.instruction("ldr x10, [x9, x12, lsl #3]");                          // load the number of object-owned reference slots
    emitter.instruction("cbz x10, __rt_nbn_no_ref_cells");                      // a class without reference properties needs no cells
    abi::emit_symbol_address(emitter, "x9", "_class_ref_prop_offset_ptrs");
    emitter.instruction("ldr x9, [x9, x12, lsl #3]");                           // load the slot-offset table for this class
    emitter.instruction("str x0, [sp, #16]");                                   // save the object across __rt_heap_alloc (name_ptr slot is free now)
    emitter.instruction("str x9, [sp, #24]");                                   // save the slot-offset table (name_len slot is free now)
    emitter.instruction("str x10, [sp, #48]");                                  // save the slot count (entry cursor is free now)
    emitter.instruction("str xzr, [sp, #56]");                                  // start at the first reference slot
    emitter.label("__rt_nbn_ref_cell_loop");
    emitter.instruction("ldr x11, [sp, #56]");                                  // reload the reference-slot index
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload the reference-slot count
    emitter.instruction("cmp x11, x10");                                        // has every reference slot been given a cell?
    emitter.instruction("b.ge __rt_nbn_ref_cells_done");                        // continue once every cell exists
    emitter.instruction("mov x0, #16");                                         // 16-byte ref cell: value at +0, tag/length at +8
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = cell pointer
    emitter.instruction("str xzr, [x0]");                                       // zero the cell value word
    emitter.instruction("str xzr, [x0, #8]");                                   // zero the cell tag/length word
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the slot-offset table
    emitter.instruction("ldr x11, [sp, #56]");                                  // reload the reference-slot index
    emitter.instruction("ldr x14, [x9, x11, lsl #3]");                          // load this reference property's slot offset
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload the object pointer
    emitter.instruction("str x0, [x13, x14]");                                  // store the cell pointer in the reference-property slot
    emitter.instruction("add x11, x11, #1");                                    // advance to the next reference slot
    emitter.instruction("str x11, [sp, #56]");                                  // persist the reference-slot index
    emitter.instruction("b __rt_nbn_ref_cell_loop");                            // continue allocating this object's reference cells
    emitter.label("__rt_nbn_ref_cells_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // restore the object pointer after the cell allocations
    emitter.label("__rt_nbn_no_ref_cells");
    // -- run the per-class property-default thunk, if this class has one --
    emitter.instruction("ldr x12, [sp, #32]");                                  // reload the matched class_id
    abi::emit_symbol_address(emitter, "x10", "_class_propinit_ptrs");
    emitter.instruction("ldr x10, [x10, x12, lsl #3]");                         // _class_propinit_ptrs[class_id] (0 = no defaults)
    emitter.instruction("cbz x10, __rt_nbn_no_propinit");                       // class has no property defaults: skip
    emitter.instruction("str x0, [sp, #40]");                                   // save the object across the thunk (obj_size slot is free now)
    emitter.instruction("blr x10");                                             // _class_propinit_<id>(this = object in x0)
    emitter.instruction("ldr x0, [sp, #40]");                                   // restore the object pointer (the thunk may clobber x0)
    emitter.label("__rt_nbn_no_propinit");
    // -- a private ancestor slot hidden by a same-named declaration keeps its own default --
    // The thunk above assigns `$this-><name>` in the allocated class's scope, which reaches only
    // the slot visible by name. Each ancestor that declares a hidden slot has a thunk of its own
    // private defaults; run every ancestor's, nearest first (their slots never overlap).
    emitter.instruction("ldr x12, [sp, #32]");                                  // reload the matched class_id
    abi::emit_symbol_address(emitter, "x10", "_class_hidden_private_slot_flags");
    emitter.instruction("ldr x10, [x10, x12, lsl #3]");                         // does this layout hide a private ancestor slot?
    emitter.instruction("cbz x10, __rt_nbn_return_allocated");                  // ordinary layouts are fully initialized already
    emitter.instruction("str x0, [sp, #40]");                                   // save the object across the ancestor thunks
    abi::emit_symbol_address(emitter, "x10", "_class_parent_ids");
    emitter.instruction("ldr x12, [x10, x12, lsl #3]");                         // start at the parent class id
    emitter.label("__rt_nbn_private_init_loop");
    emitter.instruction("cmp x12, #0");                                         // walked past the root class?
    emitter.instruction("b.lt __rt_nbn_private_init_done");                     // every ancestor has had its turn
    emitter.instruction("str x12, [sp, #48]");                                  // save the ancestor id across its thunk
    abi::emit_symbol_address(emitter, "x10", "_class_propinit_private_ptrs");
    emitter.instruction("ldr x10, [x10, x12, lsl #3]");                         // _class_propinit_private_ptrs[ancestor] (0 = none)
    emitter.instruction("cbz x10, __rt_nbn_private_init_next");                 // this ancestor declares no hidden private slot
    emitter.instruction("ldr x0, [sp, #40]");                                   // this = the allocated object
    emitter.instruction("blr x10");                                             // _class_propinit_private_<ancestor>(this)
    emitter.label("__rt_nbn_private_init_next");
    emitter.instruction("ldr x12, [sp, #48]");                                  // reload the ancestor id
    abi::emit_symbol_address(emitter, "x10", "_class_parent_ids");
    emitter.instruction("ldr x12, [x10, x12, lsl #3]");                         // advance to that ancestor's parent
    emitter.instruction("b __rt_nbn_private_init_loop");                        // continue up the chain
    emitter.label("__rt_nbn_private_init_done");
    emitter.instruction("ldr x0, [sp, #40]");                                   // restore the object pointer
    emitter.label("__rt_nbn_return_allocated");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the frame
    emitter.instruction("ret");                                                 // return the object pointer

    emitter.label("__rt_nbn_miss");
    emitter.instruction("mov x0, #0");                                          // no class with that name
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the frame
    emitter.instruction("ret");                                                 // return null
}

/// Emits the Linux x86_64 object runtime helper for new by name.
fn emit_new_by_name_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: new_by_name ---");
    emitter.label_global("__rt_new_by_name");

    // Frame (rbp-relative): [-8) name_ptr [-16) name_len [-24) entry cursor
    //   [-32) class_id stash [-40) obj_size stash [-48) entry index stash.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // helper frame
    emitter.instruction("call __rt_class_alias_resolve");                       // a class_alias() name instantiates its class
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the name pointer (elephc string ABI: rax)
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the name length (elephc string ABI: rdx)

    // -- load the lookup-table cursor + bound --
    abi::emit_load_symbol_to_reg(emitter, "r9", "_classes_by_name_count", 0); // r9 = entry count
    emitter.instruction("test r9, r9");                                         // empty registry?
    emitter.instruction("jz __rt_nbn_miss_x86");                                // no entries → no match
    abi::emit_symbol_address(emitter, "r10", "_classes_by_name"); // r10 = table base
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // entry cursor
    emitter.instruction("xor r11, r11");                                        // entry index

    emitter.label("__rt_nbn_loop_x86");
    emitter.instruction("cmp r11, r9");                                         // scanned every registered class?
    emitter.instruction("jge __rt_nbn_miss_x86");                               // exhausted the table without a match
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the entry cursor
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // stored name length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the input name length
    emitter.instruction("cmp rcx, rdx");                                        // length mismatch?
    emitter.instruction("jne __rt_nbn_skip_x86");                               // skip on length mismatch
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // save the entry index across the string helper
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the input name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the input name length
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // stored class-name pointer
    emitter.instruction("call __rt_strcasecmp");                                // compare class names case-insensitively
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // restore the entry index after the string helper
    emitter.instruction("test rax, rax");                                       // did the class names match case-insensitively?
    emitter.instruction("je __rt_nbn_match_x86");                               // full match: allocate the object
    emitter.instruction("jmp __rt_nbn_skip_x86");                               // mismatch: try the next entry

    emitter.label("__rt_nbn_skip_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the entry cursor
    emitter.instruction("add r10, 32");                                         // advance to the next 32-byte entry
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // persist the cursor
    emitter.instruction("add r11, 1");                                          // advance the entry index
    abi::emit_load_symbol_to_reg(emitter, "r9", "_classes_by_name_count", 0); // reload the count (lost across the table walk)
    emitter.instruction("jmp __rt_nbn_loop_x86");                               // continue scanning

    emitter.label("__rt_nbn_match_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the matched entry cursor
    emitter.instruction("mov rcx, QWORD PTR [r10 + 16]");                       // class_id
    emitter.instruction("mov rdx, QWORD PTR [r10 + 24]");                       // obj_size
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // stash class_id
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // stash obj_size
    abi::emit_symbol_address(emitter, "r10", "_class_initcheck_ptrs");
    emitter.instruction("mov r10, QWORD PTR [r10 + rcx*8]");                    // preallocation default-error thunk
    emitter.instruction("test r10, r10");                                       // valid defaults need no early check
    emitter.instruction("jz __rt_nbn_initcheck_ok_x86");                        // skip the absent error thunk
    emitter.instruction("xor edi, edi");                                        // invalid thunk never uses a this operand
    emitter.instruction("call r10");                                            // raise the EIR default error before allocating
    emitter.label("__rt_nbn_initcheck_ok_x86");
    emit_runtime_managed_match_x86_64(emitter);

    // -- allocate the object payload --
    emitter.label("__rt_nbn_generic_alloc_x86");
    emitter.instruction("mov rax, rdx");                                        // allocation size
    emitter.instruction("call __rt_heap_alloc");                                // rax = object pointer
    emitter.instruction(&format!(
        "mov r10, 0x{:x}",
        crate::codegen_support::sentinels::x86_64_heap_kind_word(4)
    ));                                                                         // object heap-kind word with the x86_64 marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the uniform heap header
    emitter.instruction("call __rt_object_handle_acquire");                     // bind the new object to its PHP object handle
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload class_id
    emitter.instruction("mov QWORD PTR [rax], rcx");                            // class_id at offset 0

    // -- zero the property region [obj+8 .. obj+obj_size) --
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // obj_size
    emitter.instruction("mov rcx, 8");                                          // start past the class_id header
    emitter.label("__rt_nbn_zero_x86");
    emitter.instruction("cmp rcx, rdx");                                        // every byte zeroed?
    emitter.instruction("jge __rt_nbn_done_x86");                               // property region cleared
    emitter.instruction("mov QWORD PTR [rax + rcx], 0");                        // 8-byte zero store
    emitter.instruction("add rcx, 8");                                          // advance the zero cursor
    emitter.instruction("jmp __rt_nbn_zero_x86");                               // continue zeroing

    emitter.label("__rt_nbn_done_x86");
    // -- restore typed-uninitialized markers after zeroing the selected layout --
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the matched class id for marker-table lookup
    // ADDRESS, not load: both symbols name a dense class_id-indexed TABLE, and the next
    // instruction indexes it. `emit_load_symbol_to_reg` would put the table's FIRST ENTRY in the
    // register — a small integer — and `[r8 + rcx*8]` would then dereference it as a pointer.
    // The AArch64 path above uses `emit_symbol_address` for exactly this reason.
    abi::emit_symbol_address(emitter, "r8", "_class_uninit_prop_counts"); // marker-count table base
    emitter.instruction("mov r9, QWORD PTR [r8 + rcx*8]");                      // load the number of typed-uninitialized slots
    emitter.instruction("test r9, r9");                                         // does this class need any marker stores?
    emitter.instruction("jz __rt_nbn_no_uninit_markers_x86");                   // skip marker initialization when every slot has a default
    abi::emit_symbol_address(emitter, "r8", "_class_uninit_prop_offset_ptrs"); // marker-offset pointer table base
    emitter.instruction("mov r8, QWORD PTR [r8 + rcx*8]");                      // load this class's physical-offset table
    emitter.instruction("mov r11, 0x7ffffffffffffffd");                         // materialize the typed-uninitialized sentinel word
    emitter.instruction("xor r10d, r10d");                                      // start at the first marker offset
    emitter.label("__rt_nbn_uninit_marker_loop_x86");
    emitter.instruction("cmp r10, r9");                                         // have all typed-uninitialized slots been marked?
    emitter.instruction("jge __rt_nbn_no_uninit_markers_x86");                  // continue once every marker is in place
    emitter.instruction("mov rdx, QWORD PTR [r8 + r10*8]");                     // load one property high-word offset
    emitter.instruction("mov QWORD PTR [rax + rdx], r11");                      // stamp the slot as PHP-typed but uninitialized
    emitter.instruction("add r10, 1");                                          // advance to the next marker offset
    emitter.instruction("jmp __rt_nbn_uninit_marker_loop_x86");                 // continue marking this object's typed slots
    emitter.label("__rt_nbn_no_uninit_markers_x86");
    // -- allocate the ref-cells this layout's object-owned reference properties need --
    // See the AArch64 path: an owned reference slot holds a POINTER to a 16-byte cell, and the
    // property-default thunk below stores THROUGH it.
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the matched class id for the reference-cell table lookup
    abi::emit_symbol_address(emitter, "r8", "_class_ref_prop_counts"); // reference-slot count table base
    emitter.instruction("mov r9, QWORD PTR [r8 + rcx*8]");                      // load the number of object-owned reference slots
    emitter.instruction("test r9, r9");                                         // does this class have any reference properties?
    emitter.instruction("jz __rt_nbn_no_ref_cells_x86");                        // a class without reference properties needs no cells
    abi::emit_symbol_address(emitter, "r8", "_class_ref_prop_offset_ptrs"); // slot-offset pointer table base
    emitter.instruction("mov r8, QWORD PTR [r8 + rcx*8]");                      // load the slot-offset table for this class
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the object across __rt_heap_alloc (name_ptr slot is free now)
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // save the slot-offset table (name_len slot is free now)
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // save the slot count (entry cursor is free now)
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // start at the first reference slot
    emitter.label("__rt_nbn_ref_cell_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the reference-slot index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // has every reference slot been given a cell?
    emitter.instruction("jge __rt_nbn_ref_cells_done_x86");                     // continue once every cell exists
    emitter.instruction("mov rax, 16");                                         // 16-byte ref cell: value at +0, tag/length at +8
    emitter.instruction("call __rt_heap_alloc");                                // rax = cell pointer
    emitter.instruction("mov QWORD PTR [rax], 0");                              // zero the cell value word
    emitter.instruction("mov QWORD PTR [rax + 8], 0");                          // zero the cell tag/length word
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the slot-offset table
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the reference-slot index
    emitter.instruction("mov rdx, QWORD PTR [r8 + r10*8]");                     // load this reference property's slot offset
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload the object pointer
    emitter.instruction("mov QWORD PTR [rcx + rdx], rax");                      // store the cell pointer in the reference-property slot
    emitter.instruction("add r10, 1");                                          // advance to the next reference slot
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // persist the reference-slot index
    emitter.instruction("jmp __rt_nbn_ref_cell_loop_x86");                      // continue allocating this object's reference cells
    emitter.label("__rt_nbn_ref_cells_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the object pointer after the cell allocations
    emitter.label("__rt_nbn_no_ref_cells_x86");
    // -- run the per-class property-default thunk, if this class has one --
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the matched class_id
    abi::emit_symbol_address(emitter, "r10", "_class_propinit_ptrs"); // property-init thunk table base
    emitter.instruction("mov r10, QWORD PTR [r10 + rcx*8]");                    // _class_propinit_ptrs[class_id] (0 = no defaults)
    emitter.instruction("test r10, r10");                                       // does this class have a property-init thunk?
    emitter.instruction("jz __rt_nbn_no_propinit_x86");                         // class has no property defaults: skip
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the object across the thunk (obj_size slot is free now)
    emitter.instruction("mov rdi, rax");                                        // this = object (first SysV argument)
    emitter.instruction("call r10");                                            // _class_propinit_<id>(this)
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // restore the object pointer (the thunk may clobber rax)
    emitter.label("__rt_nbn_no_propinit_x86");
    // -- a private ancestor slot hidden by a same-named declaration keeps its own default --
    // The thunk above assigns `$this-><name>` in the allocated class's scope, which reaches only
    // the slot visible by name. Each ancestor that declares a hidden slot has a thunk of its own
    // private defaults; run every ancestor's, nearest first (their slots never overlap).
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the matched class_id
    abi::emit_symbol_address(emitter, "r10", "_class_hidden_private_slot_flags");
    emitter.instruction("cmp QWORD PTR [r10 + rcx*8], 0");                      // does this layout hide a private ancestor slot?
    emitter.instruction("je __rt_nbn_return_allocated_x86");                    // ordinary layouts are fully initialized already
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the object across the ancestor thunks
    abi::emit_symbol_address(emitter, "r10", "_class_parent_ids");
    emitter.instruction("mov rcx, QWORD PTR [r10 + rcx*8]");                    // start at the parent class id
    emitter.label("__rt_nbn_private_init_loop_x86");
    emitter.instruction("test rcx, rcx");                                       // walked past the root class?
    emitter.instruction("js __rt_nbn_private_init_done_x86");                   // every ancestor has had its turn
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // save the ancestor id across its thunk
    abi::emit_symbol_address(emitter, "r10", "_class_propinit_private_ptrs");
    emitter.instruction("mov r10, QWORD PTR [r10 + rcx*8]");                    // _class_propinit_private_ptrs[ancestor] (0 = none)
    emitter.instruction("test r10, r10");                                       // does this ancestor declare a hidden private slot?
    emitter.instruction("jz __rt_nbn_private_init_next_x86");                   // no thunk to run for it
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // this = the allocated object (first SysV argument)
    emitter.instruction("call r10");                                            // _class_propinit_private_<ancestor>(this)
    emitter.label("__rt_nbn_private_init_next_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the ancestor id
    abi::emit_symbol_address(emitter, "r10", "_class_parent_ids");
    emitter.instruction("mov rcx, QWORD PTR [r10 + rcx*8]");                    // advance to that ancestor's parent
    emitter.instruction("jmp __rt_nbn_private_init_loop_x86");                  // continue up the chain
    emitter.label("__rt_nbn_private_init_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // restore the object pointer
    emitter.label("__rt_nbn_return_allocated_x86");
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the object pointer

    emitter.label("__rt_nbn_miss_x86");
    emitter.instruction("xor eax, eax");                                        // no class with that name
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return null
}

/// Emits ARM64 class-id checks for runtime-managed builtin payload layouts.
fn emit_runtime_managed_match_aarch64(emitter: &mut Emitter) {
    emitter.instruction("ldr x12, [sp, #32]");                                  // reload matched class id for runtime-managed allocation checks
    emitter.instruction("mov x1, xzr");                                         // SplDoublyLinkedList default mode: FIFO, no fix bit
    abi::emit_symbol_address(emitter, "x10", "_spl_dll_class_id");
    emitter.instruction("ldr x10, [x10]");                                      // load SplDoublyLinkedList class id
    emitter.instruction("cmp x12, x10");                                        // is the requested class SplDoublyLinkedList?
    emitter.instruction("b.eq __rt_nbn_alloc_spl_dll");                         // allocate the SPL list payload for SplDoublyLinkedList
    emitter.instruction("mov x1, #6");                                          // SplStack default mode: FIX | LIFO
    abi::emit_symbol_address(emitter, "x10", "_spl_stack_class_id");
    emitter.instruction("ldr x10, [x10]");                                      // load SplStack class id
    emitter.instruction("cmp x12, x10");                                        // is the requested class SplStack?
    emitter.instruction("b.eq __rt_nbn_alloc_spl_dll");                         // allocate the shared SPL list payload for SplStack
    emitter.instruction("mov x1, #4");                                          // SplQueue default mode: FIX
    abi::emit_symbol_address(emitter, "x10", "_spl_queue_class_id");
    emitter.instruction("ldr x10, [x10]");                                      // load SplQueue class id
    emitter.instruction("cmp x12, x10");                                        // is the requested class SplQueue?
    emitter.instruction("b.eq __rt_nbn_alloc_spl_dll");                         // allocate the shared SPL list payload for SplQueue
    abi::emit_symbol_address(emitter, "x10", "_spl_fixed_array_class_id");
    emitter.instruction("ldr x10, [x10]");                                      // load SplFixedArray class id
    emitter.instruction("cmp x12, x10");                                        // is the requested class SplFixedArray?
    emitter.instruction("b.eq __rt_nbn_alloc_spl_fixed");                       // allocate the SPL fixed-array payload with size zero
    abi::emit_symbol_address(emitter, "x10", "_stdclass_class_id");
    emitter.instruction("ldr x10, [x10]");                                      // load stdClass class id
    emitter.instruction("cmp x12, x10");                                        // is the requested class stdClass?
    emitter.instruction("b.eq __rt_nbn_alloc_stdclass");                        // stdClass keeps its properties in a hash, not in declared slots
    emitter.instruction("b __rt_nbn_generic_alloc");                            // continue with the generic property-object allocator
    emitter.label("__rt_nbn_alloc_stdclass");
    emitter.instruction("bl __rt_stdclass_new");                                // allocate the [class_id][hash_ptr] layout with an empty property hash
    emitter.instruction("b __rt_nbn_return_allocated");                         // return the initialized runtime-managed object
    emitter.label("__rt_nbn_alloc_spl_dll");
    emitter.instruction("mov x0, x12");                                         // pass the matched concrete SPL list class id
    emitter.instruction("bl __rt_spl_dll_new");                                 // allocate the runtime-managed SPL list object layout
    emitter.instruction("b __rt_nbn_return_allocated");                         // return the initialized runtime-managed object
    emitter.label("__rt_nbn_alloc_spl_fixed");
    emitter.instruction("mov x0, x12");                                         // pass the matched SplFixedArray class id
    emitter.instruction("mov x1, xzr");                                         // default dynamic size is zero before constructor dispatch
    emitter.instruction("bl __rt_spl_fixed_new");                               // allocate the runtime-managed fixed-array object layout
    emitter.instruction("b __rt_nbn_return_allocated");                         // return the initialized runtime-managed object
}

/// Emits x86_64 class-id checks for runtime-managed builtin payload layouts.
fn emit_runtime_managed_match_x86_64(emitter: &mut Emitter) {
    emitter.instruction("xor esi, esi");                                        // SplDoublyLinkedList default mode: FIFO, no fix bit
    abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_dll_class_id", 0);
    emitter.instruction("cmp rcx, r10");                                        // is the requested class SplDoublyLinkedList?
    emitter.instruction("je __rt_nbn_alloc_spl_dll_x86");                       // allocate the SPL list payload for SplDoublyLinkedList
    emitter.instruction("mov esi, 6");                                          // SplStack default mode: FIX | LIFO
    abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_stack_class_id", 0);
    emitter.instruction("cmp rcx, r10");                                        // is the requested class SplStack?
    emitter.instruction("je __rt_nbn_alloc_spl_dll_x86");                       // allocate the shared SPL list payload for SplStack
    emitter.instruction("mov esi, 4");                                          // SplQueue default mode: FIX
    abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_queue_class_id", 0);
    emitter.instruction("cmp rcx, r10");                                        // is the requested class SplQueue?
    emitter.instruction("je __rt_nbn_alloc_spl_dll_x86");                       // allocate the shared SPL list payload for SplQueue
    abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_fixed_array_class_id", 0);
    emitter.instruction("cmp rcx, r10");                                        // is the requested class SplFixedArray?
    emitter.instruction("je __rt_nbn_alloc_spl_fixed_x86");                     // allocate the SPL fixed-array payload with size zero
    abi::emit_load_symbol_to_reg(emitter, "r10", "_stdclass_class_id", 0);
    emitter.instruction("cmp rcx, r10");                                        // is the requested class stdClass?
    emitter.instruction("je __rt_nbn_alloc_stdclass_x86");                      // stdClass keeps its properties in a hash, not in declared slots
    emitter.instruction("jmp __rt_nbn_generic_alloc_x86");                      // continue with the generic property-object allocator
    emitter.label("__rt_nbn_alloc_stdclass_x86");
    emitter.instruction("call __rt_stdclass_new");                              // allocate the [class_id][hash_ptr] layout with an empty property hash
    emitter.instruction("jmp __rt_nbn_return_allocated_x86");                   // return the initialized runtime-managed object
    emitter.label("__rt_nbn_alloc_spl_dll_x86");
    emitter.instruction("mov rdi, rcx");                                        // pass the matched concrete SPL list class id
    emitter.instruction("call __rt_spl_dll_new");                               // allocate the runtime-managed SPL list object layout
    emitter.instruction("jmp __rt_nbn_return_allocated_x86");                   // return the initialized runtime-managed object
    emitter.label("__rt_nbn_alloc_spl_fixed_x86");
    emitter.instruction("mov rdi, rcx");                                        // pass the matched SplFixedArray class id
    emitter.instruction("xor esi, esi");                                        // default dynamic size is zero before constructor dispatch
    emitter.instruction("call __rt_spl_fixed_new");                             // allocate the runtime-managed fixed-array object layout
    emitter.instruction("jmp __rt_nbn_return_allocated_x86");                   // return the initialized runtime-managed object
}
