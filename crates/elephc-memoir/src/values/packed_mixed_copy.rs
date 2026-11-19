//! Purpose:
//! Copies retained Mixed slots into new packed array storage.
//!
//! Called from:
//! - Native SPL array and serialization adapters.
//!
//! Key details:
//! - Each populated source slot keeps its original owner.
//! - The nullable path creates a boxed null for each absent source cell.

use crate::{Arch, Emitter};

/// Caller frame slots for copying packed Mixed storage.
#[derive(Clone, Copy)]
pub struct PackedMixedCopy {
    /// Saved native storage pointer.
    pub source: usize,
    /// Slot for the logical result length.
    pub length: usize,
    /// Slot for the newly allocated result owner.
    pub array: usize,
    /// Cursor kept across retain and allocation calls.
    pub cursor: usize,
}

impl PackedMixedCopy {
    /// Allocates the result using the source pointer already loaded into the scratch register.
    pub fn emit_allocate(self, emitter: &mut impl Emitter) {
        let Self { length, array, cursor, .. } = self;
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.instruction("ldr x10, [x9]");                           // load list length
                emitter.instruction(&format!("str x10, [sp, #{length}]"));      // save list length
                emitter.instruction("mov x0, x10");                             // items array capacity equals list length
                emitter.instruction("mov x1, #8");                              // items array stores boxed Mixed pointers
                emitter.instruction("bl __rt_array_new");                       // allocate serialized dllist array
                emitter.instruction("ldr x9, [x0, #-8]");                       // load items array packed kind word
                emitter.instruction("orr x9, x9, #0x700");                      // stamp items array as boxed Mixed slots
                emitter.instruction("str x9, [x0, #-8]");                       // persist items Mixed value_type tag
                emitter.instruction(&format!("ldr x10, [sp, #{length}]"));      // reload list length
                emitter.instruction("str x10, [x0]");                           // publish exact items array length
                emitter.instruction(&format!("str x0, [sp, #{array}]"));        // save items array pointer
                emitter.instruction(&format!("str xzr, [sp, #{cursor}]"));      // initialize item copy cursor
            }
            Arch::X86_64 => {
                emitter.instruction("mov r10, QWORD PTR [r9]");                 // load list length
                emitter.instruction(&format!("mov QWORD PTR [rbp - {length}], r10")); // save list length
                emitter.instruction("mov rdi, r10");                            // items array capacity equals list length
                emitter.instruction("mov rsi, 8");                              // items array stores boxed Mixed pointers
                emitter.instruction("call __rt_array_new");                     // allocate serialized dllist array
                emitter.instruction("mov r9, QWORD PTR [rax - 8]");             // load items array packed kind word
                emitter.instruction("or r9, 0x700");                            // stamp items array as boxed Mixed slots
                emitter.instruction("mov QWORD PTR [rax - 8], r9");             // persist items Mixed value_type tag
                emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {length}]")); // reload list length
                emitter.instruction("mov QWORD PTR [rax], r10");                // publish exact items array length
                emitter.instruction(&format!("mov QWORD PTR [rbp - {array}], rax")); // save items array pointer
                emitter.instruction(&format!("mov QWORD PTR [rbp - {cursor}], 0")); // initialize item copy cursor
            }
        }
    }

    /// Acquires and copies the populated prefix.
    pub fn emit_dense_slots(self, emitter: &mut impl Emitter) {
        let Self { source, length, array, cursor } = self;
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.label("__rt_spl_dll_serialize_items_loop");
                emitter.instruction(&format!("ldr x10, [sp, #{cursor}]"));      // reload item copy cursor
                emitter.instruction(&format!("ldr x11, [sp, #{length}]"));      // reload list length
                emitter.instruction("cmp x10, x11");                            // have all list items been copied?
                emitter.instruction("b.ge __rt_spl_dll_serialize_items_done");  // stop once every item is serialized
                emitter.instruction(&format!("ldr x9, [sp, #{source}]"));       // reload internal storage pointer
                emitter.instruction("add x9, x9, #24");                         // point at internal Mixed slot base
                emitter.instruction("ldr x0, [x9, x10, lsl #3]");               // load source Mixed cell
                emitter.instruction("bl __rt_incref");                          // retain source Mixed cell for the serialized items array
                emitter.instruction(&format!("ldr x10, [sp, #{cursor}]"));      // reload item copy cursor after retain
                emitter.instruction(&format!("ldr x9, [sp, #{array}]"));        // reload items array pointer
                emitter.instruction("add x9, x9, #24");                         // point at serialized items slot base
                emitter.instruction("str x0, [x9, x10, lsl #3]");               // store retained Mixed cell into serialized items
                emitter.instruction("add x10, x10, #1");                        // advance item copy cursor
                emitter.instruction(&format!("str x10, [sp, #{cursor}]"));      // save updated item copy cursor
                emitter.instruction("b __rt_spl_dll_serialize_items_loop");     // continue copying list items
                emitter.label("__rt_spl_dll_serialize_items_done");
            }
            Arch::X86_64 => {
                emitter.label("__rt_spl_dll_serialize_items_loop_x86");
                emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {cursor}]")); // reload item copy cursor
                emitter.instruction(&format!("cmp r10, QWORD PTR [rbp - {length}]")); // have all list items been copied?
                emitter.instruction("jge __rt_spl_dll_serialize_items_done_x86"); // stop once every item is serialized
                emitter.instruction(&format!("mov r9, QWORD PTR [rbp - {source}]")); // reload internal storage pointer
                emitter.instruction("mov rax, QWORD PTR [r9 + 24 + r10 * 8]");  // load source Mixed cell
                emitter.instruction("call __rt_incref");                        // retain source Mixed cell for serialized items
                emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {cursor}]")); // reload item copy cursor after retain
                emitter.instruction(&format!("mov r9, QWORD PTR [rbp - {array}]")); // reload items array pointer
                emitter.instruction("mov QWORD PTR [r9 + 24 + r10 * 8], rax");  // store retained Mixed cell into serialized items
                emitter.instruction("add r10, 1");                              // advance item copy cursor
                emitter.instruction(&format!("mov QWORD PTR [rbp - {cursor}], r10")); // save updated item copy cursor
                emitter.instruction("jmp __rt_spl_dll_serialize_items_loop_x86"); // continue copying list items
                emitter.label("__rt_spl_dll_serialize_items_done_x86");
            }
        }
    }

    /// Acquires the populated cells and fills holes with boxed nulls.
    pub fn emit_nullable_slots(self, emitter: &mut impl Emitter) {
        let Self { source, length, array, cursor } = self;
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.label("__rt_spl_fixed_to_array_loop");
                emitter.instruction(&format!("ldr x10, [sp, #{length}]"));      // reload fixed size
                emitter.instruction(&format!("ldr x11, [sp, #{cursor}]"));      // reload copy cursor
                emitter.instruction("cmp x11, x10");                            // have all slots been copied?
                emitter.instruction("b.ge __rt_spl_fixed_to_array_done");       // finish after copying fixed size slots
                emitter.instruction(&format!("ldr x9, [sp, #{source}]"));       // reload source storage
                emitter.instruction("add x9, x9, #24");                         // point at source slot base
                emitter.instruction("ldr x0, [x9, x11, lsl #3]");               // load source Mixed cell pointer
                emitter.instruction("cbz x0, __rt_spl_fixed_to_array_null");    // unset slots become boxed null
                emitter.instruction("bl __rt_incref");                          // retain source Mixed cell for result array
                emitter.instruction("b __rt_spl_fixed_to_array_store");         // store retained slot
                emitter.label("__rt_spl_fixed_to_array_null");
                emit_boxed_null(emitter);
                emitter.label("__rt_spl_fixed_to_array_store");
                emitter.instruction(&format!("ldr x9, [sp, #{array}]"));        // reload result array
                emitter.instruction(&format!("ldr x11, [sp, #{cursor}]"));      // reload copy cursor
                emitter.instruction("add x9, x9, #24");                         // point at result slot base
                emitter.instruction("str x0, [x9, x11, lsl #3]");               // store owned Mixed cell into result array
                emitter.instruction("add x11, x11, #1");                        // advance copy cursor
                emitter.instruction(&format!("str x11, [sp, #{cursor}]"));      // save updated copy cursor
                emitter.instruction("b __rt_spl_fixed_to_array_loop");          // continue copying slots
                emitter.label("__rt_spl_fixed_to_array_done");
            }
            Arch::X86_64 => {
                emitter.label("__rt_spl_fixed_to_array_loop");
                emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {length}]")); // reload fixed size
                emitter.instruction(&format!("mov r11, QWORD PTR [rbp - {cursor}]")); // reload copy cursor
                emitter.instruction("cmp r11, r10");                            // have all slots been copied?
                emitter.instruction("jge __rt_spl_fixed_to_array_done");        // finish after copying fixed size slots
                emitter.instruction(&format!("mov r9, QWORD PTR [rbp - {source}]")); // reload source storage
                emitter.instruction("lea r9, [r9 + 24]");                       // point at source slot base
                emitter.instruction("mov rax, QWORD PTR [r9 + r11 * 8]");       // load source Mixed cell pointer
                emitter.instruction("test rax, rax");                           // is the source slot unset?
                emitter.instruction("jz __rt_spl_fixed_to_array_null");         // unset slots become boxed null
                emitter.instruction("call __rt_incref");                        // retain source Mixed cell for result array
                emitter.instruction("jmp __rt_spl_fixed_to_array_store");       // store retained slot
                emitter.label("__rt_spl_fixed_to_array_null");
                emit_boxed_null(emitter);
                emitter.label("__rt_spl_fixed_to_array_store");
                emitter.instruction(&format!("mov r9, QWORD PTR [rbp - {array}]")); // reload result array
                emitter.instruction(&format!("mov r11, QWORD PTR [rbp - {cursor}]")); // reload copy cursor
                emitter.instruction("lea r9, [r9 + 24]");                       // point at result slot base
                emitter.instruction("mov QWORD PTR [r9 + r11 * 8], rax");       // store owned Mixed cell into result array
                emitter.instruction("add r11, 1");                              // advance copy cursor
                emitter.instruction(&format!("mov QWORD PTR [rbp - {cursor}], r11")); // save updated copy cursor
                emitter.instruction("jmp __rt_spl_fixed_to_array_loop");        // continue copying slots
                emitter.label("__rt_spl_fixed_to_array_done");
            }
        }
    }
}

/// Materializes an owned null cell for an empty slot.
fn emit_boxed_null(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #8");                                  // runtime tag 8 = null
            emitter.instruction("mov x1, xzr");                                 // null payload low word is empty
            emitter.instruction("mov x2, xzr");                                 // null payload high word is empty
            emitter.instruction("bl __rt_mixed_from_value");                    // allocate boxed null Mixed cell
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, 8");                                  // runtime tag 8 = null
            emitter.instruction("xor rdi, rdi");                                // null payload low word is empty
            emitter.instruction("xor rsi, rsi");                                // null payload high word is empty
            emitter.instruction("call __rt_mixed_from_value");                  // allocate boxed null Mixed cell
        }
    }
}
