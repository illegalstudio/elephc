//! Purpose:
//! Emits PHP ASCII case conversion with owned results and precise logical string origins.
//!
//! Called from:
//! - The strtolower and strtoupper runtime emitters on every supported target.
//!
//! Key details:
//! - Unchanged strings preserve their origin; changed strings retire it before mutation.
//! - Owned results do not depend on the shared concat scratch capacity.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits one case transform, separating PHP logical identity from physical native string ownership.
pub(super) fn emit(emitter: &mut Emitter, uppercase: bool, mbstring: bool) {
    let arm = emitter.target.arch == Arch::AArch64;
    let name = if uppercase { "__rt_strtoupper" } else { "__rt_strtolower" };
    let (pointer, length, cursor, count, byte) = if arm { ("x1", "x2", "x9", "x10", "w11") }
        else { ("rax", "rdx", "r8", "rcx", "r9d") };
    let low = if uppercase { b'a' } else { b'A' };
    emitter.label_global(name);
    if arm {
        emitter.instruction("stp x29, x30, [sp, #-32]!");                       // preserve linkage and reserve the conversion-needed flag
        emitter.instruction("str xzr, [sp, #16]");                              // assume the logical string is unchanged until a matching byte is found
    } else {
        emitter.instruction("push rbp");                                        // preserve native linkage and align persistence calls
        emitter.instruction("mov rbp, rsp");                                    // retain a stable frame across helper calls
        emitter.instruction("sub rsp, 16");                                     // reserve the conversion-needed flag and call padding
        emitter.instruction("mov QWORD PTR [rsp], 0");                          // begin with an unchanged logical result
    }
    emitter.instruction(&format!("mov {cursor}, {pointer}"));                   // inspect the input through a separate cursor while preserving its value pair
    emitter.instruction(&format!("mov {count}, {length}"));                     // bound every source access by the PHP byte length
    emitter.label(&format!("{name}_scan"));
    branch_empty(emitter, count, &format!("{name}_persist"));
    emitter.instruction(if arm { "ldrb w11, [x9]" } else { "movzx r9d, BYTE PTR [r8]" }); // load one unsigned source byte without modifying it
    branch_outside(emitter, byte, low, &format!("{name}_scan_next"));
    if arm {
        emitter.instruction("mov x11, #1");                                     // one changed byte requires a distinct logical result
        emitter.instruction("str x11, [sp, #16]");                              // preserve that decision across string persistence
    } else {
        emitter.instruction("mov QWORD PTR [rsp], 1");                          // record that conversion requires fresh result identity
    }
    emitter.instruction(&format!("{} {name}_persist", if arm { "b" } else { "jmp" })); // stop scanning once conversion is known to be necessary
    emitter.label(&format!("{name}_scan_next"));
    advance(emitter);
    emitter.instruction(&format!("{} {name}_scan", if arm { "b" } else { "jmp" })); // inspect the remaining source bytes
    emitter.label(&format!("{name}_persist"));
    emitter.instruction(if arm { "bl __rt_str_persist" } else { "call __rt_str_persist" }); // acquire independent native storage while preserving a complete known origin
    if arm {
        emitter.instruction("ldr x9, [sp, #16]");                               // recover the origin decision before mutating any copied bytes
        emitter.instruction(&format!("cbz x9, {name}_done"));                   // unchanged values preserve source identity, including fresh empty strings
    } else {
        emitter.instruction("cmp QWORD PTR [rsp], 0");                          // distinguish unchanged values from conversions that require fresh identity
        emitter.instruction(&format!("je {name}_done"));                        // keep the complete original origin for an unchanged result
    }
    if mbstring {
        emitter.instruction(if arm { "bl __rt_mbstring_ini_fresh" } else { "call __rt_mbstring_ini_fresh" }); // detach logical identity before changing the owned payload
    }
    emitter.instruction(&format!("mov {cursor}, {pointer}"));                   // traverse only the independently owned destination
    emitter.instruction(&format!("mov {count}, {length}"));                     // preserve the returned byte count while converting the payload
    emitter.label(&format!("{name}_convert"));
    branch_empty(emitter, count, &format!("{name}_done"));
    emitter.instruction(if arm { "ldrb w11, [x9]" } else { "movzx r9d, BYTE PTR [r8]" }); // inspect the current owned byte for ASCII case conversion
    branch_outside(emitter, byte, low, &format!("{name}_convert_next"));
    let operation = if uppercase { "sub" } else { "add" };
    emitter.instruction(&if arm { format!("{operation} w11, w11, #32") } else { format!("{operation} r9d, 32") }); // apply the ASCII case offset only to a matching letter
    emitter.instruction(if arm { "strb w11, [x9]" } else { "mov BYTE PTR [r8], r9b" }); // update only the fresh logical result's owned byte
    emitter.label(&format!("{name}_convert_next"));
    advance(emitter);
    emitter.instruction(&format!("{} {name}_convert", if arm { "b" } else { "jmp" })); // convert the remaining bytes within their exact logical length
    emitter.label(&format!("{name}_done"));
    emitter.instruction(if arm { "ldp x29, x30, [sp], #32" } else { "leave" }); // restore native linkage with the owned string result intact
    emitter.instruction("ret");                                                 // transfer one native string owner to the caller
}

/// Branches before accessing an empty remaining byte range in either native register convention.
fn branch_empty(emitter: &mut Emitter, count: &str, label: &str) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction(&format!("cbz {count}, {label}"));                  // skip the next byte access when the logical range is exhausted
    } else {
        emitter.instruction(&format!("test {count}, {count}"));                 // check the remaining byte count without changing it
        emitter.instruction(&format!("jz {label}"));                            // skip the next byte access when no payload remains
    }
}

/// Rejects bytes outside the selected ASCII letter range without treating high bytes as signed text.
fn branch_outside(emitter: &mut Emitter, byte: &str, low: u8, label: &str) {
    let arm = emitter.target.arch == Arch::AArch64;
    let prefix = if arm { "#" } else { "" };
    emitter.instruction(&format!("cmp {byte}, {prefix}{low}"));                 // compare against the first ASCII letter in the conversion range
    emitter.instruction(&format!("{} {label}", if arm { "b.lo" } else { "jb" })); // preserve bytes below the selected ASCII range
    emitter.instruction(&format!("cmp {byte}, {prefix}{}", low + 25));          // compare against the final ASCII letter in the conversion range
    emitter.instruction(&format!("{} {label}", if arm { "b.hi" } else { "ja" })); // preserve bytes above the selected ASCII range
}

/// Advances the scan or mutation cursor without changing the PHP result pointer and length.
fn advance(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("add x9, x9, #1");                                  // advance the independent byte cursor
        emitter.instruction("sub x10, x10, #1");                                // account for the byte just inspected or converted
    } else {
        emitter.instruction("inc r8");                                          // advance the independent byte cursor
        emitter.instruction("dec rcx");                                         // account for the byte just inspected or converted
    }
}
