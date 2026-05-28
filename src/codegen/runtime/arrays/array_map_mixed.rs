//! Purpose:
//! Emits the `__rt_array_map_mixed` runtime helper for descriptor-backed maps.
//! Stores boxed Mixed callback results directly into a newly allocated result array.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Descriptor callback wrappers return owned boxed Mixed cells, so array slots take ownership.
//! - Source arrays may contain scalar or string-width elements; the helper preserves that callback ABI.

use crate::codegen::emit::Emitter;
use crate::codegen::expr::arrays::emit_array_value_type_stamp;
use crate::codegen::platform::Arch;
use crate::types::PhpType;

/// Emits the `__rt_array_map_mixed` runtime helper for the active target.
pub fn emit_array_map_mixed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_map_mixed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_map_mixed ---");
    emitter.label_global("__rt_array_map_mixed");

    // -- set up stack frame, save callee-saved registers --
    emitter.instruction("sub sp, sp, #80");                                     // allocate mixed-result map loop metadata
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame pointer
    emitter.instruction("stp x19, x20, [sp, #48]");                             // save callback and destination callee-saved registers
    emitter.instruction("str x21, [sp, #40]");                                  // save descriptor callback environment register
    emitter.instruction("str x1, [sp, #8]");                                    // save source array pointer for each loop iteration
    emitter.instruction("mov x19, x0");                                         // keep callback address across every loop callback
    emitter.instruction("mov x21, x2");                                         // keep descriptor callback environment pointer across the loop

    // -- read source metadata and allocate a mixed-slot destination array --
    emitter.instruction("ldr x9, [x1]");                                        // read source array length
    emitter.instruction("str x9, [sp, #16]");                                   // save source length across callback calls
    emitter.instruction("ldr x10, [x1, #16]");                                  // read source element width for callback ABI dispatch
    emitter.instruction("str x10, [sp, #24]");                                  // save source element width across callback calls
    emitter.instruction("mov x0, x9");                                          // pass source length as destination capacity
    emitter.instruction("mov x1, #8");                                          // request boxed Mixed pointer slots for destination values
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array storage
    emit_array_value_type_stamp(emitter, "x0", &PhpType::Mixed);
    emitter.instruction("mov x20, x0");                                         // keep destination array pointer across callback calls

    // -- set up loop counter --
    emitter.instruction("mov x0, #0");                                          // initialize logical loop index to zero
    emitter.instruction("str x0, [sp, #0]");                                    // save loop index in the local frame

    // -- loop: apply callback to each source element --
    emitter.label("__rt_array_map_mixed_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // load current logical loop index
    emitter.instruction("ldr x9, [sp, #16]");                                   // load saved source array length
    emitter.instruction("cmp x0, x9");                                          // check whether every source element has been mapped
    emitter.instruction("b.ge __rt_array_map_mixed_done");                      // exit once the loop index reaches the source length
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source array pointer
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload source element width
    emitter.instruction("add x1, x1, #24");                                     // advance to the source payload region
    emitter.instruction("mul x11, x0, x10");                                    // compute current source element byte offset
    emitter.instruction("add x11, x1, x11");                                    // compute current source element address
    emitter.instruction("cmp x10, #16");                                        // does the source array hold string ptr/len pairs?
    emitter.instruction("b.eq __rt_array_map_mixed_load_str");                  // branch to the string-input callback ABI

    // -- scalar source: pass element in x0 and env in x1 --
    emitter.instruction("ldr x0, [x11]");                                       // load scalar source element for the callback
    emitter.instruction("mov x1, x21");                                         // pass descriptor environment after the scalar argument
    emitter.instruction("b __rt_array_map_mixed_call");                         // invoke the callback through the shared call site

    // -- string source: pass element in x0/x1 and env in x2 --
    emitter.label("__rt_array_map_mixed_load_str");
    emitter.instruction("ldr x0, [x11]");                                       // load source string pointer
    emitter.instruction("ldr x1, [x11, #8]");                                   // load source string length
    emitter.instruction("mov x2, x21");                                         // pass descriptor environment after the string pair

    // -- call callback and store its owned boxed Mixed result directly --
    emitter.label("__rt_array_map_mixed_call");
    emitter.instruction("blr x19");                                             // call callback and receive owned boxed Mixed in x0
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload loop index after callback clobbers caller-saved registers
    emitter.instruction("add x10, x20, #24");                                   // compute destination payload base
    emitter.instruction("str x0, [x10, x9, lsl #3]");                           // transfer owned boxed Mixed pointer into destination slot
    emitter.instruction("add x9, x9, #1");                                      // advance to the next source element
    emitter.instruction("str x9, [sp, #0]");                                    // save updated loop index
    emitter.instruction("b __rt_array_map_mixed_loop");                         // continue mapping boxed Mixed results

    // -- publish destination length and return --
    emitter.label("__rt_array_map_mixed_done");
    emitter.instruction("mov x0, x20");                                         // return destination array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // load source length for destination length publication
    emitter.instruction("str x9, [x0]");                                        // publish mapped destination length
    emitter.instruction("ldr x21, [sp, #40]");                                  // restore descriptor callback environment register
    emitter.instruction("ldp x19, x20, [sp, #48]");                             // restore callback and destination callee-saved registers
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release mixed-result map frame storage
    emitter.instruction("ret");                                                 // return mapped Mixed array pointer in x0
}

/// Emits the x86_64 Linux implementation of `__rt_array_map_mixed`.
fn emit_array_map_mixed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_map_mixed ---");
    emitter.label_global("__rt_array_map_mixed");

    emitter.instruction("push rbp");                                            // preserve caller frame pointer before reserving mixed-result map slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for loop metadata
    emitter.instruction("push r12");                                            // preserve callback address across descriptor callback calls
    emitter.instruction("push r13");                                            // preserve loop index across descriptor callback calls
    emitter.instruction("sub rsp, 48");                                         // reserve source, destination, width, and environment slots
    emitter.instruction("mov r12, rdi");                                        // keep callback address in a callee-saved register
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save source array pointer for every loop iteration
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save descriptor callback environment pointer
    emitter.instruction("mov r10, QWORD PTR [rsi]");                            // load source array length
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save source length across destination allocation
    emitter.instruction("mov r11, QWORD PTR [rsi + 16]");                       // load source element width for ABI dispatch
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // save source element width across callback calls
    emitter.instruction("mov rdi, r10");                                        // pass source length as destination capacity
    emitter.instruction("mov rsi, 8");                                          // request boxed Mixed pointer slots for destination values
    emitter.instruction("call __rt_array_new");                                 // allocate destination array storage
    emit_array_value_type_stamp(emitter, "rax", &PhpType::Mixed);
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save destination array pointer for direct slot stores
    emitter.instruction("xor r13d, r13d");                                      // initialize loop index to zero

    emitter.label("__rt_array_map_mixed_loop");
    emitter.instruction("cmp r13, QWORD PTR [rbp - 32]");                       // compare loop index against source length
    emitter.instruction("jge __rt_array_map_mixed_done");                       // exit once every source element has been mapped
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload source array pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload source element width
    emitter.instruction("mov rcx, r13");                                        // copy loop index before byte-offset scaling
    emitter.instruction("imul rcx, r11");                                       // compute current source element byte offset
    emitter.instruction("lea rcx, [r10 + rcx + 24]");                           // compute current source element address
    emitter.instruction("cmp r11, 16");                                         // does the source slot hold a string pair?
    emitter.instruction("je __rt_array_map_mixed_load_str");                    // branch to string-input callback ABI
    emitter.instruction("mov rdi, QWORD PTR [rcx]");                            // load scalar source element into first callback argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // pass descriptor environment after the scalar argument
    emitter.instruction("jmp __rt_array_map_mixed_call");                       // invoke the descriptor callback wrapper

    emitter.label("__rt_array_map_mixed_load_str");
    emitter.instruction("mov rdi, QWORD PTR [rcx]");                            // load source string pointer for callback argument
    emitter.instruction("mov rsi, QWORD PTR [rcx + 8]");                        // load source string length for callback argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // pass descriptor environment after the string pair

    emitter.label("__rt_array_map_mixed_call");
    emitter.instruction("call r12");                                            // invoke descriptor callback and receive owned boxed Mixed in rax
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload destination array pointer after callback clobbers caller-saved regs
    emitter.instruction("mov QWORD PTR [r10 + r13 * 8 + 24], rax");             // transfer owned boxed Mixed pointer into destination slot
    emitter.instruction("add r13, 1");                                          // advance loop index after storing the mapped Mixed value
    emitter.instruction("jmp __rt_array_map_mixed_loop");                       // continue mapping boxed Mixed results

    emitter.label("__rt_array_map_mixed_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload destination array pointer for return
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload source length for destination length publication
    emitter.instruction("mov QWORD PTR [rax], r10");                            // publish destination array length
    emitter.instruction("add rsp, 48");                                         // release mixed-result map local slots
    emitter.instruction("pop r13");                                             // restore loop-index callee-saved register
    emitter.instruction("pop r12");                                             // restore callback-address callee-saved register
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return mapped Mixed array pointer in rax
}
