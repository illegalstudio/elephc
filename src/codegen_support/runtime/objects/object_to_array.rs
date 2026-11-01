//! Purpose:
//! Emits the runtime helper that casts an object into an independently owned associative array.
//!
//! Called from:
//! - `crate::codegen::lower_inst::conversions::lower_cast_to_array()`.
//!
//! Key details:
//! - Declared keys use PHP visibility mangling and uninitialized typed properties are omitted.
//! - Values are inserted as independently owned Mixed cells.
//! - A class's optional dynamic-property hash is merged without changing the source object.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_object_to_array(object) -> assoc_array` for every supported target.
pub fn emit_object_to_array(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_object_to_array_aarch64(emitter),
        Arch::X86_64 => emit_object_to_array_x86_64(emitter),
    }
}

/// Emits the AArch64 object-to-array property walk and dynamic-property merge.
fn emit_object_to_array_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_to_array ---");
    emitter.label_global("__rt_object_to_array");
    emitter.instruction("mov x1, xzr");                                         // select visibility-mangled keys for an explicit array cast
    emitter.instruction("b __rt_object_to_array_common");                       // share the property walk with foreach conversion
    emitter.label_global("__rt_object_to_foreach_array");
    emitter.instruction("mov x1, #1");                                          // select bare visible names for in-scope foreach iteration
    emitter.instruction("b __rt_object_to_array_common");                       // branch, never fall through: `.subsections_via_symbols` makes the shared body its own atom
    emitter.label_global("__rt_object_to_array_common");

    // [0]=object [8]=result [16]=index [24]=count [32]=key ptr [40]=key len
    // [48]=pre-union result [80]=saved fp/lr
    emitter.instruction("sub sp, sp, #96");                                     // reserve the property-walk frame and saved return state
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the caller frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish a stable frame pointer for nested helper calls
    emitter.instruction("str x0, [sp, #0]");                                    // save the borrowed object pointer
    emitter.instruction("str x1, [sp, #56]");                                   // save the property-name mode across runtime calls
    emitter.instruction("bl __rt_obj_prop_count");                              // load the declared property descriptor row count
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the row count for the property loop
    emitter.instruction("cmp x0, #4");                                          // tiny hashes still need practical probing capacity
    emitter.instruction("mov x9, #4");                                          // materialize the minimum destination capacity
    emitter.instruction("csel x0, x0, x9, ge");                                 // use max(property count, four) as initial capacity
    emitter.instruction("mov x1, #7");                                          // store heterogeneous property values as Mixed cells
    abi::emit_call_label(emitter, "__rt_hash_new");
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the evolving associative-array pointer
    emitter.instruction("str xzr, [sp, #16]");                                  // initialize the declared property row index

    emitter.label("__rt_object_to_array_declared_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the current property row index
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the declared property row count
    emitter.instruction("cmp x9, x10");                                         // have all declared property rows been visited?
    emitter.instruction("b.ge __rt_object_to_array_dynamic");                   // merge the optional dynamic-property tail next
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass the borrowed object pointer
    emitter.instruction("mov x1, x9");                                          // pass the current declared property row index
    emitter.instruction("ldr x10, [sp, #56]");                                  // reload the property-name mode
    emitter.instruction("cbz x10, __rt_object_to_array_cast_name");             // explicit casts retain PHP visibility mangling
    abi::emit_call_label(emitter, "__rt_obj_prop_name");
    emitter.instruction("b __rt_object_to_array_name_done");                    // continue with the bare foreach-visible name
    emitter.label("__rt_object_to_array_cast_name");
    abi::emit_call_label(emitter, "__rt_obj_prop_cast_name");
    emitter.label("__rt_object_to_array_name_done");
    emitter.instruction("cbz x2, __rt_object_to_array_declared_next");          // skip uninitialized typed properties
    emitter.instruction("stp x1, x2, [sp, #32]");                               // preserve the visibility-mangled key across value boxing
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass the borrowed object pointer again
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass the same declared property row index
    abi::emit_call_label(emitter, "__rt_obj_prop_value");
    emitter.instruction("mov x3, x0");                                          // transfer the fresh Mixed cell into the hash value low word
    emitter.instruction("mov x4, xzr");                                         // boxed Mixed hash entries use no high payload word
    emitter.instruction("mov x5, #7");                                          // runtime hash value tag 7 denotes a Mixed cell
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the evolving destination hash pointer
    emitter.instruction("ldp x1, x2, [sp, #32]");                               // reload the visibility-mangled key pointer and length
    abi::emit_call_label(emitter, "__rt_hash_set");
    emitter.instruction("str x0, [sp, #8]");                                    // preserve a possibly grown destination hash

    emitter.label("__rt_object_to_array_declared_next");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the declared property row index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next descriptor row
    emitter.instruction("str x9, [sp, #16]");                                   // persist the advanced row index
    emitter.instruction("b __rt_object_to_array_declared_loop");                // continue the declared property walk

    emitter.label("__rt_object_to_array_dynamic");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the borrowed object pointer
    emitter.instruction("ldr x10, [x9]");                                       // load its runtime class id
    abi::emit_symbol_address(emitter, "x11", "_class_object_dynamic_prop_flags");
    emitter.instruction("ldr x11, [x11, x10, lsl #3]");                         // load whether this class reserves a dynamic-property tail
    emitter.instruction("cbz x11, __rt_object_to_array_done");                  // classes without a tail have no dynamic hash to merge
    abi::emit_symbol_address(emitter, "x11", "_class_object_payload_sizes");
    emitter.instruction("ldr x11, [x11, x10, lsl #3]");                         // load the exact object payload size
    emitter.instruction("sub x11, x11, #8");                                    // the dynamic-property hash pointer occupies the final word
    emitter.instruction("ldr x1, [x9, x11]");                                   // load the borrowed dynamic-property hash pointer
    emitter.instruction("cbz x1, __rt_object_to_array_done");                   // a lazily unused tail has no entries to merge
    emitter.instruction("ldr x0, [sp, #8]");                                    // pass declared properties as the left-hand hash
    emitter.instruction("str x0, [sp, #48]");                                   // preserve the pre-union destination for release
    abi::emit_call_label(emitter, "__rt_hash_union");
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the independent merged hash result
    emitter.instruction("ldr x0, [sp, #48]");                                   // release the superseded declared-only hash
    abi::emit_call_label(emitter, "__rt_decref_any");

    emitter.label("__rt_object_to_array_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the owned associative-array result
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the property-walk frame
    emitter.instruction("ret");                                                 // return to generated code
}

/// Emits the x86_64 object-to-array property walk and dynamic-property merge.
fn emit_object_to_array_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_to_array ---");
    emitter.label_global("__rt_object_to_array");
    emitter.instruction("xor esi, esi");                                        // select visibility-mangled keys for an explicit array cast
    emitter.instruction("jmp __rt_object_to_array_common_x86");                 // share the property walk with foreach conversion
    emitter.label_global("__rt_object_to_foreach_array");
    emitter.instruction("mov esi, 1");                                          // select bare visible names for in-scope foreach iteration
    emitter.instruction("jmp __rt_object_to_array_common_x86");                 // branch, never fall through: `.subsections_via_symbols` makes the shared body its own atom
    emitter.label_global("__rt_object_to_array_common_x86");

    // [rbp-8]=object [16]=result [24]=index [32]=count [40]=key ptr [48]=key len
    // [56]=pre-union result
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for nested helper calls
    emitter.instruction("sub rsp, 80");                                         // reserve aligned property-walk spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the borrowed object pointer
    emitter.instruction("mov QWORD PTR [rbp - 64], rsi");                       // save the property-name mode across runtime calls
    abi::emit_call_label(emitter, "__rt_obj_prop_count");
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the declared property row count
    emitter.instruction("mov rdi, rax");                                        // use the property row count as initial hash capacity
    emitter.instruction("cmp rdi, 4");                                          // tiny hashes still need practical probing capacity
    emitter.instruction("mov r10, 4");                                          // materialize the minimum destination capacity
    emitter.instruction("cmovl rdi, r10");                                      // use max(property count, four) as initial capacity
    emitter.instruction("mov rsi, 7");                                          // store heterogeneous property values as Mixed cells
    abi::emit_call_label(emitter, "__rt_hash_new");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the evolving associative-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the declared property row index

    emitter.label("__rt_object_to_array_declared_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the current property row index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // have all declared property rows been visited?
    emitter.instruction("jge __rt_object_to_array_dynamic_x86");                // merge the optional dynamic-property tail next
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the borrowed object pointer
    emitter.instruction("mov rsi, r10");                                        // pass the current declared property row index
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // does this conversion request bare foreach names?
    emitter.instruction("je __rt_object_to_array_cast_name_x86");               // explicit casts retain PHP visibility mangling
    abi::emit_call_label(emitter, "__rt_obj_prop_name");
    emitter.instruction("jmp __rt_object_to_array_name_done_x86");              // continue with the bare foreach-visible name
    emitter.label("__rt_object_to_array_cast_name_x86");
    abi::emit_call_label(emitter, "__rt_obj_prop_cast_name");
    emitter.label("__rt_object_to_array_name_done_x86");
    emitter.instruction("test rdx, rdx");                                       // did the row yield an initialized property name?
    emitter.instruction("jz __rt_object_to_array_declared_next_x86");           // skip uninitialized typed properties
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the visibility-mangled key pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // preserve the visibility-mangled key length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the borrowed object pointer again
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the same declared property row index
    abi::emit_call_label(emitter, "__rt_obj_prop_value");
    emitter.instruction("mov rcx, rax");                                        // transfer the fresh Mixed cell into the hash value low word
    emitter.instruction("xor r8d, r8d");                                        // boxed Mixed hash entries use no high payload word
    emitter.instruction("mov r9, 7");                                           // runtime hash value tag 7 denotes a Mixed cell
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the evolving destination hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reload the visibility-mangled key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // reload the visibility-mangled key length
    abi::emit_call_label(emitter, "__rt_hash_set");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve a possibly grown destination hash

    emitter.label("__rt_object_to_array_declared_next_x86");
    emitter.instruction("add QWORD PTR [rbp - 24], 1");                         // advance to the next declared property row
    emitter.instruction("jmp __rt_object_to_array_declared_loop_x86");          // continue the declared property walk

    emitter.label("__rt_object_to_array_dynamic_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the borrowed object pointer
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load its runtime class id
    abi::emit_symbol_address(emitter, "r11", "_class_object_dynamic_prop_flags");
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // load whether this class reserves a dynamic-property tail
    emitter.instruction("test r11, r11");                                       // is a dynamic-property tail present?
    emitter.instruction("jz __rt_object_to_array_done_x86");                    // classes without a tail need no merge
    abi::emit_symbol_address(emitter, "r11", "_class_object_payload_sizes");
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // load the exact object payload size
    emitter.instruction("sub r11, 8");                                          // the dynamic-property hash pointer occupies the final word
    emitter.instruction("mov rsi, QWORD PTR [rax + r11]");                      // load the borrowed dynamic-property hash pointer
    emitter.instruction("test rsi, rsi");                                       // has the lazy dynamic-property hash been allocated?
    emitter.instruction("jz __rt_object_to_array_done_x86");                    // an unused tail has no entries to merge
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass declared properties as the left-hand hash
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // preserve the pre-union destination for release
    abi::emit_call_label(emitter, "__rt_hash_union");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the independent merged hash result
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // release the superseded declared-only hash
    abi::emit_call_label(emitter, "__rt_decref_any");

    emitter.label("__rt_object_to_array_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the owned associative-array result
    emitter.instruction("mov rsp, rbp");                                        // release the property-walk frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to generated code
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Keeps the shared property walk as a global target so macOS dead stripping retains it.
    #[test]
    fn object_to_array_common_body_is_global_for_every_target() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_object_to_array(&mut emitter);
            let asm = emitter.output();
            let label = match target.arch {
                Arch::AArch64 => ".globl __rt_object_to_array_common",
                Arch::X86_64 => ".globl __rt_object_to_array_common_x86",
            };
            assert!(asm.contains(label), "{target:?}: missing {label}");
        }
    }
}
