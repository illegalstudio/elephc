//! Purpose:
//! Emits a runtime helper that tests class metadata against an interface id.
//! This supports dynamic class-string checks without requiring an object instance.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::exceptions`.
//!
//! Key details:
//! - The table walk must match `_class_interface_ptrs` emitted by runtime user data.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits assembly for class implements interface.
pub fn emit_class_implements_interface(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_class_implements_interface_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: class_implements_interface ---");
    emitter.label_global("__rt_class_implements_interface");

    emitter.adrp("x9", "_class_gc_desc_count");                               // load page of the emitted class-count table
    emitter.add_lo12("x9", "x9", "_class_gc_desc_count");                     // resolve the emitted class-count table address
    emitter.instruction("ldr x9, [x9]");                                        // x9 = total number of emitted class metadata slots
    emitter.instruction("cmp x0, x9");                                          // is the requested class id within the emitted class table?
    emitter.instruction("b.hs __rt_class_implements_interface_no");             // out-of-range class ids cannot implement any interface
    emitter.adrp("x10", "_class_interface_ptrs");                              // load page of the class-to-interface metadata table
    emitter.add_lo12("x10", "x10", "_class_interface_ptrs");                  // resolve the class-to-interface metadata table address
    emitter.instruction("lsl x11, x0, #3");                                     // scale class id by 8 bytes per metadata pointer
    emitter.instruction("ldr x10, [x10, x11]");                                 // x10 = pointer to this class's interface metadata block
    emitter.instruction("ldr x11, [x10]");                                      // x11 = number of emitted interfaces for this class
    emitter.instruction("add x10, x10, #8");                                    // advance to the first [interface_id, impl_ptr] pair

    emitter.label("__rt_class_implements_interface_loop");
    emitter.instruction("cbz x11, __rt_class_implements_interface_no");         // no remaining interfaces means the class does not implement the target
    emitter.instruction("ldr x12, [x10]");                                      // x12 = current implemented interface id
    emitter.instruction("cmp x12, x1");                                         // does this implemented interface match the requested id?
    emitter.instruction("b.eq __rt_class_implements_interface_yes");            // matching interface ids make the runtime class test true
    emitter.instruction("add x10, x10, #16");                                   // advance to the next [interface_id, impl_ptr] pair
    emitter.instruction("sub x11, x11, #1");                                    // consume one implemented interface entry
    emitter.instruction("b __rt_class_implements_interface_loop");              // continue scanning this class's interface list

    emitter.label("__rt_class_implements_interface_yes");
    emitter.instruction("mov x0, #1");                                          // return true when the class implements the requested interface
    emitter.instruction("ret");                                                 // finish the metadata-only interface test

    emitter.label("__rt_class_implements_interface_no");
    emitter.instruction("mov x0, #0");                                          // return false when the class does not implement the requested interface
    emitter.instruction("ret");                                                 // finish the metadata-only interface test
}

/// Emits assembly for class implements interface linux x86 64.
fn emit_class_implements_interface_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: class_implements_interface ---");
    emitter.label_global("__rt_class_implements_interface");

    emitter.instruction("mov r8, QWORD PTR [rip + _class_gc_desc_count]");      // r8 = total number of emitted class metadata slots
    emitter.instruction("cmp rdi, r8");                                         // is the requested class id within the emitted class table?
    emitter.instruction("jae __rt_class_implements_interface_no");              // out-of-range class ids cannot implement any interface
    emitter.instruction("lea r9, [rip + _class_interface_ptrs]");               // materialize the class-to-interface metadata table base pointer
    emitter.instruction("mov r9, QWORD PTR [r9 + rdi * 8]");                    // r9 = pointer to this class's interface metadata block
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // r10 = number of emitted interfaces for this class
    emitter.instruction("add r9, 8");                                           // advance to the first [interface_id, impl_ptr] pair

    emitter.label("__rt_class_implements_interface_loop");
    emitter.instruction("test r10, r10");                                       // are there any remaining interfaces to scan?
    emitter.instruction("je __rt_class_implements_interface_no");               // no remaining interfaces means the class does not implement the target
    emitter.instruction("mov r11, QWORD PTR [r9]");                             // r11 = current implemented interface id
    emitter.instruction("cmp r11, rsi");                                        // does this implemented interface match the requested id?
    emitter.instruction("je __rt_class_implements_interface_yes");              // matching interface ids make the runtime class test true
    emitter.instruction("add r9, 16");                                          // advance to the next [interface_id, impl_ptr] pair
    emitter.instruction("sub r10, 1");                                          // consume one implemented interface entry
    emitter.instruction("jmp __rt_class_implements_interface_loop");            // continue scanning this class's interface list

    emitter.label("__rt_class_implements_interface_yes");
    emitter.instruction("mov eax, 1");                                          // return true when the class implements the requested interface
    emitter.instruction("ret");                                                 // finish the metadata-only interface test

    emitter.label("__rt_class_implements_interface_no");
    emitter.instruction("xor eax, eax");                                        // return false when the class does not implement the requested interface
    emitter.instruction("ret");                                                 // finish the metadata-only interface test
}
