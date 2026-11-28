//! Purpose:
//! Projects borrowed property slots for the native variable bridge.
//!
//! Called from:
//! - Native mbstring variable and object traversal.
//!
//! Key details:
//! - Class descriptors are supplied by the generated host context.
//! - The query returns a declared property or the remaining ordinal in the dynamic hash.

use crate::{Arch, Emitter};

/// Emits a property query with the C arguments context, object, ordinal, and output.
/// Status zero describes a fixed slot, one describes the dynamic tail, and two is invalid.
pub(super) fn emit(emitter: &mut impl Emitter) {
    const INVALID: &str = "__rt_mbstring_variable_invalid_object_slot";
    const DYNAMIC: &str = "__rt_mbstring_variable_dynamic_slot";
    const PLAIN: &str = "__rt_mbstring_variable_plain_object";
    const END: &str = "__rt_mbstring_variable_object_slot_end";

    let arm = emitter.arch() == Arch::AArch64;
    let context = emitter.int_arg_reg_name(0);
    let object = emitter.int_arg_reg_name(1);
    let index = emitter.int_arg_reg_name(2);
    let output = emitter.int_arg_reg_name(3);
    let result = emitter.int_result_reg();
    let (class, size, flags, scratch, descriptors) = if arm {
        ("x9", "x10", "x11", "x12", "x13")
    } else {
        ("r8", "r9", "r10", "r11", "rax")
    };

    emitter.label_global(&emitter.extern_symbol("elephc_mbstring_variable_object_slot_v1"));
    emitter.emit_load_from_address(class, object, 0);
    emitter.emit_load_from_address(scratch, context, 16);
    emit_require_table(emitter, scratch, INVALID);
    emitter.emit_load_from_address(scratch, scratch, 0);
    emitter.instruction(&format!("cmp {class}, {scratch}"));                    // restrict metadata access to known class IDs
    emitter.instruction(if arm { "b.hs __rt_mbstring_variable_invalid_object_slot" }
        else { "jae __rt_mbstring_variable_invalid_object_slot" });             // an unknown class has no native property layout

    for (target, offset) in [(size, 24), (flags, 32), (descriptors, 40)] {
        emitter.emit_load_from_address(target, context, offset);
        emit_require_table(emitter, target, INVALID);
        emitter.instruction(&if arm { format!("ldr {target}, [{target}, {class}, lsl #3]") }
            else { format!("mov {target}, QWORD PTR [{target} + {class} * 8]") }); // read this class's payload geometry
    }
    if arm {
        emitter.instruction(&format!("cbz {descriptors}, {INVALID}"));          // missing descriptor prevents slot decoding
        emitter.instruction(&format!("cmp {size}, #8"));                        // the payload has at least a class word
        emitter.instruction(&format!("b.lo {INVALID}"));                        // reject a wrapped fixed payload span
        emitter.instruction(&format!("sub {size}, {size}, #8"));                // exclude the class word
        emitter.instruction(&format!("cbz {flags}, {PLAIN}"));                  // only dynamic objects carry another word
        emitter.instruction(&format!("cmp {size}, #8"));                        // require space for the dynamic hash owner
        emitter.instruction(&format!("b.lo {INVALID}"));                        // reject a truncated descriptor
        emitter.instruction(&format!("sub {size}, {size}, #8"));                // exclude the dynamic hash owner
    } else {
        emitter.instruction(&format!("test {descriptors}, {descriptors}"));     // missing descriptor prevents slot decoding
        emitter.instruction(&format!("jz {INVALID}"));                          // reject an absent descriptor
        emitter.instruction(&format!("sub {size}, 8"));                         // exclude the class word
        emitter.instruction(&format!("jb {INVALID}"));                          // reject a wrapped fixed payload span
        emitter.instruction(&format!("test {flags}, {flags}"));                 // only dynamic objects carry another word
        emitter.instruction(&format!("jz {PLAIN}"));                            // fixed-only objects already have a full span
        emitter.instruction(&format!("sub {size}, 8"));                         // exclude the dynamic hash owner
        emitter.instruction(&format!("jb {INVALID}"));                          // reject a truncated descriptor
    }
    emitter.label(PLAIN);
    emitter.instruction(&if arm { format!("tst {size}, #15") }
        else { format!("test {size}, 15") });                                   // declared properties occupy complete 16-byte slots
    emitter.instruction(&format!("{} {INVALID}", if arm { "b.ne" } else { "jnz" })); // reject inconsistent metadata
    emitter.instruction(&if arm { format!("lsr {scratch}, {size}, #4") }
        else { format!("mov {scratch}, {size}") });                             // compute the number of declared properties
    if !arm {
        emitter.instruction(&format!("shr {scratch}, 4"));                      // convert byte span to property count
    }
    emitter.instruction(&format!("cmp {index}, {scratch}"));                    // pick the declared slot or dynamic tail
    emitter.instruction(&format!("{} {DYNAMIC}", if arm { "b.hs" } else { "jae" })); // ordinals after fixed fields belong to the hash

    if arm {
        emitter.instruction("ldrb w11, [x13, x2]");                             // read the fixed property's native tag
        emitter.instruction("add x9, x1, x2, lsl #4");                          // select the fixed property after the class word
        emitter.instruction("add x9, x9, #8");                                  // form its borrowed payload address
    } else {
        emitter.instruction("movzx r10, BYTE PTR [rax + rdx]");                 // read the fixed property's native tag
        emitter.instruction("mov r8, rdx");                                     // keep the requested ordinal for address calculation
        emitter.instruction("shl r8, 4");                                       // scale by the fixed property stride
        emitter.instruction("lea r8, [rsi + r8 + 8]");                          // form its borrowed payload address
    }
    emitter.emit_store_to_address(class, output, 0);
    emitter.emit_store_to_address(flags, output, 8);
    emitter.emit_load_int_immediate(result, 0);
    emitter.emit_return();

    emitter.label(DYNAMIC);
    emitter.emit_load_int_immediate(class, 0);
    if arm {
        emitter.instruction("sub x12, x2, x12");                                // yield the ordinal relative to the dynamic hash
        emitter.instruction(&format!("cbz x11, {END}"));                        // fixed-only classes have no dynamic tail
        emitter.instruction("add x9, x1, x10");                                 // locate the dynamic hash slot after declared properties
        emitter.instruction("ldr x9, [x9, #8]");                                // borrow the dynamic hash owner
    } else {
        emitter.instruction("sub rdx, r11");                                    // yield the ordinal relative to the dynamic hash
        emitter.instruction("mov r11, rdx");                                    // retain the remaining ordinal in the shared output register
        emitter.instruction("test r10, r10");                                   // fixed-only classes have no dynamic tail
        emitter.instruction(&format!("jz {END}"));                              // return an empty dynamic tail
        emitter.instruction("mov r8, QWORD PTR [rsi + r9 + 8]");                // borrow the dynamic hash owner
    }
    emitter.label(END);
    emitter.emit_store_to_address(class, output, 0);
    emitter.emit_store_to_address(scratch, output, 8);
    emitter.emit_load_int_immediate(result, 1);
    emitter.emit_return();
    emitter.label(INVALID);
    emitter.emit_load_int_immediate(result, 2);
    emitter.emit_return();
}

/// Rejects an absent class-descriptor table before reading any entry.
fn emit_require_table(emitter: &mut impl Emitter, table: &str, invalid: &str) {
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("cbz {table}, {invalid}")), // require a present class table
        Arch::X86_64 => {
            emitter.instruction(&format!("test {table}, {table}"));             // check the host's optional class table
            emitter.instruction(&format!("jz {invalid}"));                      // reject absent class metadata
        }
    }
}
