//! Purpose:
//! Emits `__rt_defined`, the runtime helper backing a non-literal `defined($name)`
//! call by searching the closed-world constant registry.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` (gated by the
//!   `const_introspection` feature) and the EIR `defined` lowering.
//!
//! Key details:
//! - Input:  x0/rdi=name_ptr, x1/rsi=name_len.
//! - Output: x0/rax = 1 when the constant is registered, 0 otherwise. No throw.
//! - Delegates the lookup to `__rt_sorted_name_search` over the 40-byte
//!   `_const_table` entries and reports presence as a boolean.
//! - A name containing `::` is a class constant or an enum case, which that table never holds:
//!   it asks `__elephc_eval_value_class_constant_get`, exactly as `__rt_constant` reads it, and
//!   drops the boxed value. Monolog's `Logger::toMonologLevel()` tests
//!   `defined(Level::class . '::' . $name)` before its `constant()`, so every level name was
//!   refused. With no class scope only public constants exist here, as for `constant()`.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// The user-assembly getter that reads one class constant by name.
const CLASS_CONSTANT_GETTER: &str = "__elephc_eval_value_class_constant_get";

/// Emits the target-specific `__rt_defined` constant-existence helper.
pub fn emit_rt_defined(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 implementation of `__rt_defined`.
///
/// Frame (32 bytes): [0] saved fp/lr, [16] name pointer, [24] name length.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: defined ---");
    emitter.label_global("__rt_defined");

    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the new frame pointer
    emitter.instruction("stp x0, x1, [sp, #16]");                               // spill the constant name

    // -- a `Class::NAME` name is a class constant: find the `::` --
    emitter.instruction("mov x9, #0");                                          // scan offset = 0
    emitter.instruction("sub x10, x1, #1");                                     // last offset a `::` can start at
    emitter.label("__rt_defined_scan");
    emitter.instruction("cmp x9, x10");                                         // scanned every candidate offset?
    emitter.instruction("b.ge __rt_defined_global");                            // no `::`: a global constant
    emitter.instruction("ldrb w11, [x0, x9]");                                  // load the byte at the scan offset
    emitter.instruction("cmp w11, #58");                                        // is it ':'?
    emitter.instruction("b.ne __rt_defined_scan_next");                         // keep scanning
    emitter.instruction("add x12, x9, #1");                                     // offset of the following byte
    emitter.instruction("ldrb w11, [x0, x12]");                                 // load the following byte
    emitter.instruction("cmp w11, #58");                                        // is it ':' too?
    emitter.instruction("b.eq __rt_defined_class");                             // `::` separates class and constant
    emitter.label("__rt_defined_scan_next");
    emitter.instruction("add x9, x9, #1");                                      // advance the scan offset
    emitter.instruction("b __rt_defined_scan");                                 // test the next offset

    // -- search the constant registry --
    emitter.label("__rt_defined_global");
    abi::emit_symbol_address(emitter, "x2", "_const_table");
    abi::emit_load_symbol_to_reg(emitter, "x3", "_const_table_count", 0);
    emitter.instruction("mov x4, #40");                                         // constant entries are 40 bytes wide
    emitter.instruction("bl __rt_sorted_name_search");                          // search for the constant name
    emitter.instruction("cmp x0, #0");                                          // did the search return a matching entry?
    emitter.instruction("cset x0, ne");                                         // map a non-null entry pointer to true
    emitter.instruction("b __rt_defined_done");                                 // return the boolean

    // -- class constant: ask the compiled getter --
    emitter.label("__rt_defined_class");
    emitter.instruction("mov x1, x9");                                          // class-name length = the `::` offset
    emitter.instruction("add x2, x0, x9");                                      // point at the `::`
    emitter.instruction("add x2, x2, #2");                                      // constant-name pointer, past the `::`
    emitter.instruction("ldr x3, [sp, #24]");                                   // reload the whole name length
    emitter.instruction("sub x3, x3, x9");                                      // drop the class name
    emitter.instruction("sub x3, x3, #2");                                      // and the `::`: the constant-name length
    emitter.instruction("ldrb w11, [x0]");                                      // load the class name's first byte
    emitter.instruction("cmp w11, #92");                                        // a fully qualified name may lead with `\\`
    emitter.instruction("b.ne __rt_defined_class_named");                                 // no leading separator to drop
    emitter.instruction("add x0, x0, #1");                                      // skip it: class names carry none
    emitter.instruction("sub x1, x1, #1");                                      // and shorten the class name
    emitter.label("__rt_defined_class_named");
    emitter.instruction("mov x4, xzr");                                         // no class scope: public constants only
    emitter.instruction("mov x5, xzr");                                         // empty scope name length
    let getter = emitter.target.extern_symbol(CLASS_CONSTANT_GETTER);
    abi::emit_call_label_long_range(emitter, &getter);
    emitter.instruction("cbz x0, __rt_defined_done");                           // a null cell: no such constant, false
    emitter.instruction("bl __rt_decref_mixed");                                // only its existence is asked: drop the value
    emitter.instruction("mov x0, #1");                                          // the constant exists

    emitter.label("__rt_defined_done");
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the boolean in x0
}

/// Emits the x86_64 implementation of `__rt_defined`.
///
/// Frame: [rbp - 8] name pointer, [rbp - 16] name length.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: defined ---");
    emitter.label_global("__rt_defined");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // keep the nested call 16-byte aligned
    emitter.instruction("sub rsp, 16");                                         // reserve the name slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // spill the constant name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // spill the constant name length

    // -- a `Class::NAME` name is a class constant: find the `::` --
    emitter.instruction("xor r9, r9");                                          // scan offset = 0
    emitter.instruction("mov r10, rsi");                                        // the name length
    emitter.instruction("sub r10, 1");                                          // last offset a `::` can start at
    emitter.label("__rt_defined_scan_x86");
    emitter.instruction("cmp r9, r10");                                         // scanned every candidate offset?
    emitter.instruction("jge __rt_defined_global_x86");                         // no `::`: a global constant
    emitter.instruction("movzx r11d, BYTE PTR [rdi + r9]");                     // load the byte at the scan offset
    emitter.instruction("cmp r11d, 58");                                        // is it ':'?
    emitter.instruction("jne __rt_defined_scan_next_x86");                      // keep scanning
    emitter.instruction("movzx r11d, BYTE PTR [rdi + r9 + 1]");                 // load the following byte
    emitter.instruction("cmp r11d, 58");                                        // is it ':' too?
    emitter.instruction("je __rt_defined_class_x86");                           // `::` separates class and constant
    emitter.label("__rt_defined_scan_next_x86");
    emitter.instruction("add r9, 1");                                           // advance the scan offset
    emitter.instruction("jmp __rt_defined_scan_x86");                           // test the next offset

    // -- search the constant registry --
    emitter.label("__rt_defined_global_x86");
    abi::emit_symbol_address(emitter, "rdx", "_const_table");
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_const_table_count", 0);
    emitter.instruction("mov r8, 40");                                          // constant entries are 40 bytes wide
    emitter.instruction("call __rt_sorted_name_search");                        // search for the constant name
    emitter.instruction("test rax, rax");                                       // did the search return a matching entry?
    emitter.instruction("setne al");                                            // map a non-null entry pointer to true
    emitter.instruction("movzx eax, al");                                       // widen the boolean byte into the result
    emitter.instruction("jmp __rt_defined_done_x86");                           // return the boolean

    // -- class constant: ask the compiled getter --
    emitter.label("__rt_defined_class_x86");
    emitter.instruction("mov rsi, r9");                                         // class-name length = the `::` offset
    emitter.instruction("lea rdx, [rdi + r9 + 2]");                             // constant-name pointer, past the `::`
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the whole name length
    emitter.instruction("sub rcx, r9");                                         // drop the class name
    emitter.instruction("sub rcx, 2");                                          // and the `::`: the constant-name length
    emitter.instruction("cmp BYTE PTR [rdi], 92");                              // a fully qualified name may lead with `\\`
    emitter.instruction("jne __rt_defined_class_named_x86");                                  // no leading separator to drop
    emitter.instruction("add rdi, 1");                                          // skip it: class names carry none
    emitter.instruction("sub rsi, 1");                                          // and shorten the class name
    emitter.label("__rt_defined_class_named_x86");
    emitter.instruction("xor r8, r8");                                          // no class scope: public constants only
    emitter.instruction("xor r9, r9");                                          // empty scope name length
    let getter = emitter.target.extern_symbol(CLASS_CONSTANT_GETTER);
    abi::emit_call_label_long_range(emitter, &getter);
    emitter.instruction("test rax, rax");                                       // did the getter box a value?
    emitter.instruction("je __rt_defined_done_x86");                            // a null cell: no such constant, false
    emitter.instruction("call __rt_decref_mixed");                              // only its existence is asked: drop the value
    emitter.instruction("mov eax, 1");                                          // the constant exists

    emitter.label("__rt_defined_done_x86");
    emitter.instruction("leave");                                               // drop the frame and restore rbp
    emitter.instruction("ret");                                                 // return the boolean in rax
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::{Platform, Target};

    /// Both targets answer a `Class::NAME` through the getter and release the cell it boxed.
    #[test]
    fn a_class_constant_name_reaches_the_getter_on_every_target() {
        for (platform, arch) in [(Platform::MacOS, Arch::AArch64), (Platform::Linux, Arch::X86_64)] {
            let mut emitter = Emitter::new(Target::new(platform, arch));
            emit_rt_defined(&mut emitter);
            let text = emitter.output();
            assert!(text.contains(CLASS_CONSTANT_GETTER), "{arch:?} never calls the getter");
            assert!(text.contains("__rt_decref_mixed"), "{arch:?} leaks the boxed cell");
            assert!(text.contains("__rt_sorted_name_search"), "{arch:?} lost the global search");
        }
    }
}
