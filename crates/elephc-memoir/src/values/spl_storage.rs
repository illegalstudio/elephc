//! Purpose:
//! Defines the fixed native SPL object layouts and initializes their Mixed backing storage.
//!
//! Called from:
//! - The SPL constructor emitters, GC traversal and deep object reclamation.
//!
//! Key details:
//! - The compiler stages class identity, checked size and the default iteration mode.
//! - The installed backing array starts with a single owner held by the object.

use crate::{Arch, Emitter};

/// Offset of the list's owned Mixed array.
pub const SPL_DLL_STORAGE_OFFSET: usize = 8;
/// Offset of the list iterator index.
pub const SPL_DLL_ITER_INDEX_OFFSET: usize = 16;
/// Offset of the class-selected iteration flags.
pub const SPL_DLL_ITER_MODE_OFFSET: usize = 24;
/// Offset of the fixed array's owned Mixed array.
pub const SPL_FIXED_STORAGE_OFFSET: usize = 8;

const SPL_DLL_OBJECT_SIZE: i64 = 32;
const SPL_FIXED_OBJECT_SIZE: i64 = 16;
const SPL_DLL_INITIAL_CAPACITY: i64 = 4;

/// Creates the native list with its class id and iterator mode staged in the constructor frame.
pub fn emit_dll_storage(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => emit_dll_aarch64(emitter),
        Arch::X86_64 => emit_dll_x86_64(emitter),
    }
}

/// Creates the native fixed array with its class id and checked size staged in the constructor frame.
pub fn emit_fixed_storage(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => emit_fixed_aarch64(emitter),
        Arch::X86_64 => emit_fixed_x86_64(emitter),
    }
}

/// Initializes the list object and its backing on aarch64.
fn emit_dll_aarch64(emitter: &mut impl Emitter) {
    emitter.instruction(&format!("mov x0, #{}", SPL_DLL_OBJECT_SIZE));          // request the fixed SPL list object payload size
    super::object_storage::emit_allocate_object(emitter);
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the concrete SPL class id
    emitter.instruction("str x9, [x0]");                                        // store the class id at the object header
    emitter.instruction("str x0, [sp, #8]");                                    // save the object pointer while allocating storage
    emitter.instruction(&format!("mov x0, #{}", SPL_DLL_INITIAL_CAPACITY));     // initial internal storage capacity
    super::array_header::emit_allocate_mixed_slots(emitter);
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the object pointer
    emitter.instruction(&format!("str x0, [x9, #{}]", SPL_DLL_STORAGE_OFFSET)); // object.storage = internal Mixed array
    emitter.instruction(&format!("str xzr, [x9, #{}]", SPL_DLL_ITER_INDEX_OFFSET)); // iterator index starts at zero
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the class default iterator mode
    emitter.instruction(&format!("str x10, [x9, #{}]", SPL_DLL_ITER_MODE_OFFSET)); // seed the class's default mode (FIX/LIFO bits)
    emitter.instruction("mov x0, x9");                                          // return the initialized SPL object
}

/// Initializes the list object and its backing on x86_64.
fn emit_dll_x86_64(emitter: &mut impl Emitter) {
    emitter.instruction(&format!("mov rax, {}", SPL_DLL_OBJECT_SIZE));          // request fixed SPL list object payload size
    super::object_storage::emit_allocate_object(emitter);
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload concrete SPL class id
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store class id at object header
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save object pointer while allocating storage
    emitter.instruction(&format!("mov rdi, {}", SPL_DLL_INITIAL_CAPACITY));     // initial internal storage capacity
    super::array_header::emit_allocate_mixed_slots(emitter);
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload object pointer
    emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", SPL_DLL_STORAGE_OFFSET)); // object.storage = internal Mixed array
    emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", SPL_DLL_ITER_INDEX_OFFSET)); // iterator index starts at zero
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the class default iterator mode
    emitter.instruction(&format!("mov QWORD PTR [r11 + {}], r10", SPL_DLL_ITER_MODE_OFFSET)); // seed the class's default mode (FIX/LIFO bits)
    emitter.instruction("mov rax, r11");                                        // return initialized SPL object
}

/// Initializes the fixed array object and its backing on aarch64.
fn emit_fixed_aarch64(emitter: &mut impl Emitter) {
    emitter.instruction(&format!("mov x0, #{}", SPL_FIXED_OBJECT_SIZE));        // request fixed-array object payload size
    super::object_storage::emit_allocate_object(emitter);
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload concrete class id
    emitter.instruction("str x9, [x0]");                                        // store class id at object header
    emitter.instruction("str x0, [sp, #16]");                                   // save object pointer while allocating storage
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload requested fixed size as array capacity
    super::array_header::emit_allocate_mixed_slots(emitter);
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload fixed size
    emitter.instruction("str x10, [x0]");                                       // logical length is the fixed size
    emitter.instruction("add x11, x0, #24");                                    // point at first storage slot
    emitter.instruction("mov x12, #0");                                         // initialize slot-zeroing cursor
    emitter.label("__rt_spl_fixed_new_zero_loop");
    emitter.instruction("cmp x12, x10");                                        // have all slots been initialized?
    emitter.instruction("b.ge __rt_spl_fixed_new_done");                        // finish once every slot is zeroed
    emitter.instruction("str xzr, [x11, x12, lsl #3]");                         // initialize slot as unset/null
    emitter.instruction("add x12, x12, #1");                                    // advance zeroing cursor
    emitter.instruction("b __rt_spl_fixed_new_zero_loop");                      // continue zeroing storage slots
    emitter.label("__rt_spl_fixed_new_done");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload object pointer
    emitter.instruction(&format!("str x0, [x9, #{}]", SPL_FIXED_STORAGE_OFFSET)); // object.storage = fixed-array storage
    emitter.instruction("mov x0, x9");                                          // return initialized SplFixedArray object
}

/// Initializes the fixed array object and its backing on x86_64.
fn emit_fixed_x86_64(emitter: &mut impl Emitter) {
    emitter.instruction(&format!("mov rax, {}", SPL_FIXED_OBJECT_SIZE));        // request fixed-array object payload size
    super::object_storage::emit_allocate_object(emitter);
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload concrete class id
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store class id at object header
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save object pointer while allocating storage
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass requested size as storage capacity
    super::array_header::emit_allocate_mixed_slots(emitter);
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload fixed size
    emitter.instruction("mov QWORD PTR [rax], r10");                            // logical length is fixed size
    emitter.instruction("lea r11, [rax + 24]");                                 // point at first storage slot
    emitter.instruction("xor rcx, rcx");                                        // initialize slot-zeroing cursor
    emitter.label("__rt_spl_fixed_new_zero_loop");
    emitter.instruction("cmp rcx, r10");                                        // have all slots been initialized?
    emitter.instruction("jge __rt_spl_fixed_new_done");                         // finish once every slot is zeroed
    emitter.instruction("mov QWORD PTR [r11 + rcx * 8], 0");                    // initialize slot as unset/null
    emitter.instruction("add rcx, 1");                                          // advance zeroing cursor
    emitter.instruction("jmp __rt_spl_fixed_new_zero_loop");                    // continue zeroing storage slots
    emitter.label("__rt_spl_fixed_new_done");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload object pointer
    emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", SPL_FIXED_STORAGE_OFFSET)); // object.storage = fixed-array storage
    emitter.instruction("mov rax, r11");                                        // return initialized SplFixedArray object
}
