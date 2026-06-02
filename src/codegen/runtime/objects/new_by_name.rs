//! Purpose:
//! Emits the `__rt_new_by_name` runtime helper that allocates an instance
//! of a class identified by a string name (Phase 10 step 2). The lookup
//! consults the `_classes_by_name` data table emitted by
//! `crate::codegen::runtime::data::user::emit_classes_by_name_table`.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via
//!   `crate::codegen::runtime::objects`.
//! - `crate::codegen::expr::objects::emit_new_dynamic` for
//!   `new $variable()` expressions.
//!
//! Key details:
//! - Each `_classes_by_name` entry is 32 bytes: name_ptr (8) + name_len
//!   (8) + class_id (8) + obj_size (8). A linear scan compares lengths
//!   first, then byte-by-byte until a full match.
//! - On match: allocates obj_size bytes through `__rt_heap_alloc`, stamps
//!   the uniform heap-kind word (heap kind 4 = object) ahead of the
//!   payload, writes the class id at offset 0, and zeroes the property
//!   region so later property-store paths see clean memory.
//! - On miss: returns 0 (null), which `emit_new_dynamic` boxes as PHP
//!   null (`gettype()` reports "NULL").

use crate::codegen::{abi, emit::Emitter, platform::Arch};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

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
    //   [32) matched class_id, [40) matched obj_size, [48) entry cursor.
    emitter.instruction("sub sp, sp, #64");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
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
    emitter.instruction("b.ne __rt_nbn_skip");
    emitter.instruction("ldr x12, [x10]");                                      // stored name pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the input name pointer
    emitter.instruction("mov x14, #0");                                         // byte compare index

    emitter.label("__rt_nbn_bytecmp");
    emitter.instruction("cmp x14, x2");                                         // every byte compared?
    emitter.instruction("b.ge __rt_nbn_match");                                 // full match: allocate the object
    emitter.instruction("ldrb w15, [x12, x14]");                                // stored byte
    emitter.instruction("ldrb w16, [x1, x14]");                                 // input byte
    emitter.instruction("cmp w15, w16");                                        // do they differ?
    emitter.instruction("b.ne __rt_nbn_skip");                                  // mismatch: try the next entry
    emitter.instruction("add x14, x14, #1");                                    // advance the byte cursor
    emitter.instruction("b __rt_nbn_bytecmp");                                  // continue comparing

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

    // -- allocate the object payload --
    emitter.instruction("mov x0, x13");                                         // allocation size
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = object pointer
    emitter.instruction("mov x9, #4");                                          // heap kind 4 = object instance
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the uniform heap header
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
    // -- run the per-class property-default thunk, if this class has one --
    emitter.instruction("ldr x12, [sp, #32]");                                  // reload the matched class_id
    abi::emit_symbol_address(emitter, "x10", "_class_propinit_ptrs");
    emitter.instruction("ldr x10, [x10, x12, lsl #3]");                         // _class_propinit_ptrs[class_id] (0 = no defaults)
    emitter.instruction("cbz x10, __rt_nbn_no_propinit");                       // class has no property defaults: skip
    emitter.instruction("str x0, [sp, #40]");                                   // save the object across the thunk (obj_size slot is free now)
    emitter.instruction("blr x10");                                             // _class_propinit_<id>(this = object in x0)
    emitter.instruction("ldr x0, [sp, #40]");                                   // restore the object pointer (the thunk may clobber x0)
    emitter.label("__rt_nbn_no_propinit");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the frame
    emitter.instruction("ret");                                                 // return the object pointer

    emitter.label("__rt_nbn_miss");
    emitter.instruction("mov x0, #0");                                          // no class with that name
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the frame
    emitter.instruction("ret");                                                 // return null
}

fn emit_new_by_name_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: new_by_name ---");
    emitter.label_global("__rt_new_by_name");

    // Frame (rbp-relative): [-8) name_ptr [-16) name_len [-24) entry cursor
    //   [-32) class_id stash [-40) obj_size stash.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // helper frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the name pointer (elephc string ABI: rax)
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the name length (elephc string ABI: rdx)

    // -- load the lookup-table cursor + bound --
    emitter.instruction("mov r9, QWORD PTR [rip + _classes_by_name_count]");    // r9 = entry count
    emitter.instruction("test r9, r9");                                         // empty registry?
    emitter.instruction("jz __rt_nbn_miss_x86");                                // no entries → no match
    emitter.instruction("lea r10, [rip + _classes_by_name]");                   // r10 = table base
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
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // stored name pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the input name pointer
    emitter.instruction("xor rcx, rcx");                                        // byte compare index

    emitter.label("__rt_nbn_bytecmp_x86");
    emitter.instruction("cmp rcx, rdx");                                        // every byte compared?
    emitter.instruction("jge __rt_nbn_match_x86");                              // full match: allocate the object
    emitter.instruction("movzx eax, BYTE PTR [rsi + rcx]");                     // stored byte
    emitter.instruction("movzx r8d, BYTE PTR [rdi + rcx]");                     // input byte
    emitter.instruction("cmp eax, r8d");                                        // do they differ?
    emitter.instruction("jne __rt_nbn_skip_x86");                               // mismatch: try the next entry
    emitter.instruction("inc rcx");                                             // advance the byte cursor
    emitter.instruction("jmp __rt_nbn_bytecmp_x86");                            // continue comparing

    emitter.label("__rt_nbn_skip_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the entry cursor
    emitter.instruction("add r10, 32");                                         // advance to the next 32-byte entry
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // persist the cursor
    emitter.instruction("add r11, 1");                                          // advance the entry index
    emitter.instruction("mov r9, QWORD PTR [rip + _classes_by_name_count]");    // reload the count (lost across the table walk)
    emitter.instruction("jmp __rt_nbn_loop_x86");                               // continue scanning

    emitter.label("__rt_nbn_match_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the matched entry cursor
    emitter.instruction("mov rcx, QWORD PTR [r10 + 16]");                       // class_id
    emitter.instruction("mov rdx, QWORD PTR [r10 + 24]");                       // obj_size
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // stash class_id
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // stash obj_size

    // -- allocate the object payload --
    emitter.instruction("mov rax, rdx");                                        // allocation size
    emitter.instruction("call __rt_heap_alloc");                                // rax = object pointer
    emitter.instruction(&format!(
        "mov r10, 0x{:x}",
        (X86_64_HEAP_MAGIC_HI32 << 32) | 4
    ));                                                                         // object heap-kind word with the x86_64 marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the uniform heap header
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
    // -- run the per-class property-default thunk, if this class has one --
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the matched class_id
    emitter.instruction("lea r10, [rip + _class_propinit_ptrs]");               // property-init thunk table base
    emitter.instruction("mov r10, QWORD PTR [r10 + rcx*8]");                    // _class_propinit_ptrs[class_id] (0 = no defaults)
    emitter.instruction("test r10, r10");                                       // does this class have a property-init thunk?
    emitter.instruction("jz __rt_nbn_no_propinit_x86");                         // class has no property defaults: skip
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the object across the thunk (obj_size slot is free now)
    emitter.instruction("mov rdi, rax");                                        // this = object (first SysV argument)
    emitter.instruction("call r10");                                            // _class_propinit_<id>(this)
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // restore the object pointer (the thunk may clobber rax)
    emitter.label("__rt_nbn_no_propinit_x86");
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the object pointer

    emitter.label("__rt_nbn_miss_x86");
    emitter.instruction("xor eax, eax");                                        // no class with that name
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return null
}
