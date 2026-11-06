//! Purpose:
//! Emits `__rt_obj_loose_eq`, the runtime implementation of PHP's `==` between two
//! objects: the same instance, or the same class with every declared property
//! loosely equal.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::compare`.
//! - `__rt_mixed_loose_eq` once both operands unbox to the object tag.
//!
//! Key details:
//! - Properties are read through `__rt_obj_prop_count` / `__rt_obj_prop_value`, the
//!   same per-class descriptor `var_dump`, `print_r` and `var_export` walk, so the
//!   set of compared properties is exactly the set elephc considers to exist.
//! - `__rt_obj_prop_value` hands back an OWNED boxed cell per property, so both
//!   sides are released after each comparison; the walker itself only borrows the
//!   two receivers.
//! - Enum cases are singletons, so the leading pointer-identity check gives PHP's
//!   "enum cases compare by identity" for free before any property walk starts.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_obj_loose_eq` for the active target.
///
/// Input: AArch64 `x0`/`x1` = the two object pointers, `x2` = recursion depth;
/// x86_64 `rdi`/`rsi`/`rdx`. Output: `x0` / `rax` = 1 when loosely equal.
pub fn emit_obj_loose_eq(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_obj_loose_eq_x86_64(emitter);
        return;
    }
    emit_obj_loose_eq_aarch64(emitter);
}

/// Emits the AArch64 object comparison walker.
///
/// Frame (160 bytes): `[sp,#0]` left object, `[sp,#8]` right object, `[sp,#16]`
/// depth, `[sp,#24]` property count, `[sp,#32]` current index, `[sp,#40]` /
/// `[sp,#48]` the two owned property cells, `[sp,#56]` the comparison result, and
/// `[sp,#80]` the saved `x29`/`x30` pair, followed by two 32-byte owner guards.
fn emit_obj_loose_eq_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: obj_loose_eq ---");
    emitter.label_global("__rt_obj_loose_eq");

    emitter.instruction("sub sp, sp, #160");                                    // allocate the object comparison frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the object comparison frame pointer
    emitter.instruction("cmp x0, x1");                                          // is this the very same instance?
    emitter.instruction("b.eq __rt_ole_true");                                  // an instance is always loosely equal to itself
    emitter.instruction("cbz x0, __rt_ole_false");                              // a missing left receiver cannot match
    emitter.instruction("cbz x1, __rt_ole_false");                              // a missing right receiver cannot match
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save both receivers for the property walk
    emitter.instruction("str x2, [sp, #16]");                                   // save the current recursion depth
    emitter.instruction("ldr x9, [x0]");                                        // load the left runtime class id
    emitter.instruction("ldr x10, [x1]");                                       // load the right runtime class id
    emitter.instruction("cmp x9, x10");                                         // PHP requires the same class before comparing properties
    emitter.instruction("b.ne __rt_ole_false");                                 // different classes are never loosely equal
    emitter.instruction("bl __rt_obj_prop_count");                              // count the left receiver's renderable properties
    emitter.instruction("str x0, [sp, #24]");                                   // save the property count
    emitter.instruction("mov x11, #0");                                         // start the property walk at index zero
    emitter.instruction("str x11, [sp, #32]");                                  // save the initial property index

    emitter.label("__rt_ole_loop");
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the current property index
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload the property count
    emitter.instruction("cmp x11, x12");                                        // has every property been compared?
    emitter.instruction("b.ge __rt_ole_true");                                  // all properties matched loosely
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the left receiver
    emitter.instruction("mov x1, x11");                                         // pass the current property index
    emitter.instruction("bl __rt_obj_prop_value");                              // read the left property as an owned boxed cell
    emitter.instruction("str x0, [sp, #40]");                                   // save the owned left property cell
    abi::emit_guard_owned_at_stack(emitter, "x0", "__rt_exception_release_owned", 96);
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right receiver
    emitter.instruction("ldr x1, [sp, #32]");                                   // pass the same property index
    emitter.instruction("bl __rt_obj_prop_value");                              // read the right property as an owned boxed cell
    emitter.instruction("str x0, [sp, #48]");                                   // save the owned right property cell
    abi::emit_guard_owned_at_stack(emitter, "x0", "__rt_exception_release_owned", 128);
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the left property cell
    emitter.instruction("ldr x1, [sp, #48]");                                   // reload the right property cell
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the current recursion depth
    emitter.instruction("bl __rt_mixed_loose_eq_d");                            // compare the two property values loosely
    emitter.instruction("str x0, [sp, #56]");                                   // save the property comparison result
    abi::emit_unguard_owned_at_stack(emitter, 96);
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the owned left property cell
    emitter.instruction("bl __rt_decref_mixed");                                // release the left property copy
    abi::emit_unguard_owned_at_stack(emitter, 128);
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the owned right property cell
    emitter.instruction("bl __rt_decref_mixed");                                // release the right property copy
    emitter.instruction("ldr x0, [sp, #56]");                                   // reload the property comparison result
    emitter.instruction("cbz x0, __rt_ole_false");                              // one differing property ends the comparison
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the current property index
    emitter.instruction("add x11, x11, #1");                                    // advance to the next declared property
    emitter.instruction("str x11, [sp, #32]");                                  // save the advanced property index
    emitter.instruction("b __rt_ole_loop");                                     // keep walking the property descriptor

    emitter.label("__rt_ole_true");
    emitter.instruction("mov x0, #1");                                          // report that the two objects are loosely equal
    emitter.instruction("b __rt_ole_done");                                     // return the true result

    emitter.label("__rt_ole_false");
    emitter.instruction("mov x0, #0");                                          // report that the two objects differ

    emitter.label("__rt_ole_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #160");                                    // release the object comparison frame
    emitter.instruction("ret");                                                 // return the object loose-equality boolean
}

/// Emits the x86_64 object comparison walker.
///
/// Frame (160 bytes below `rbp`): `[rbp-8]` left object, `[rbp-16]` right object,
/// `[rbp-24]` depth, `[rbp-32]` property count, `[rbp-40]` current index,
/// `[rbp-48]` / `[rbp-56]` the two owned property cells, `[rbp-64]` the comparison
/// result. The bottom 64 bytes hold two owned-cell exception guards.
fn emit_obj_loose_eq_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: obj_loose_eq ---");
    emitter.label_global("__rt_obj_loose_eq");

    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the object comparison frame pointer
    emitter.instruction("sub rsp, 160");                                        // allocate the aligned object comparison frame
    emitter.instruction("cmp rdi, rsi");                                        // is this the very same instance?
    emitter.instruction("je __rt_ole_true");                                    // an instance is always loosely equal to itself
    emitter.instruction("test rdi, rdi");                                       // is the left receiver missing?
    emitter.instruction("jz __rt_ole_false");                                   // a missing left receiver cannot match
    emitter.instruction("test rsi, rsi");                                       // is the right receiver missing?
    emitter.instruction("jz __rt_ole_false");                                   // a missing right receiver cannot match
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the left receiver
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the right receiver
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the current recursion depth
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the left runtime class id
    emitter.instruction("mov r11, QWORD PTR [rsi]");                            // load the right runtime class id
    emitter.instruction("cmp r10, r11");                                        // PHP requires the same class before comparing properties
    emitter.instruction("jne __rt_ole_false");                                  // different classes are never loosely equal
    abi::emit_call_label(emitter, "__rt_obj_prop_count"); // count the left receiver's renderable properties
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the property count
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // start the property walk at index zero

    emitter.label("__rt_ole_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current property index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // has every property been compared?
    emitter.instruction("jge __rt_ole_true");                                   // all properties matched loosely
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left receiver
    emitter.instruction("mov rsi, r10");                                        // pass the current property index
    abi::emit_call_label(emitter, "__rt_obj_prop_value"); // read the left property as an owned boxed cell
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the owned left property cell
    abi::emit_guard_owned_at_stack(emitter, "rax", "__rt_exception_release_owned", 0);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the right receiver
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // pass the same property index
    abi::emit_call_label(emitter, "__rt_obj_prop_value"); // read the right property as an owned boxed cell
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the owned right property cell
    abi::emit_guard_owned_at_stack(emitter, "rax", "__rt_exception_release_owned", 32);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the left property cell
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload the right property cell
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the current recursion depth
    abi::emit_call_label(emitter, "__rt_mixed_loose_eq_d"); // compare the two property values loosely
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the property comparison result
    abi::emit_unguard_owned_at_stack(emitter, 0);
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the owned left property cell
    abi::emit_call_label(emitter, "__rt_decref_mixed"); // release the left property copy
    abi::emit_unguard_owned_at_stack(emitter, 32);
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the owned right property cell
    abi::emit_call_label(emitter, "__rt_decref_mixed"); // release the right property copy
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // did the two property values compare equal?
    emitter.instruction("je __rt_ole_false");                                   // one differing property ends the comparison
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current property index
    emitter.instruction("add r10, 1");                                          // advance to the next declared property
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the advanced property index
    emitter.instruction("jmp __rt_ole_loop");                                   // keep walking the property descriptor

    emitter.label("__rt_ole_true");
    emitter.instruction("mov rax, 1");                                          // report that the two objects are loosely equal
    emitter.instruction("jmp __rt_ole_done");                                   // return the true result

    emitter.label("__rt_ole_false");
    emitter.instruction("xor rax, rax");                                        // report that the two objects differ

    emitter.label("__rt_ole_done");
    emitter.instruction("add rsp, 160");                                        // release the object comparison frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the object loose-equality boolean
}
