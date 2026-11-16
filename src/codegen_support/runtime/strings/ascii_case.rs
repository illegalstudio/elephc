//! Purpose:
//! Emits PHP ASCII case conversion for `strtolower()` and `strtoupper()`.
//!
//! Called from:
//! - The strtolower and strtoupper runtime emitters on every supported target.
//!
//! Key details:
//! - Without mbstring, the helper allocates one owned heap result and converts directly
//!   into it sixteen bytes at a time, so the bytes are written exactly once.
//! - With mbstring, the logical origin of the source string must be detached before
//!   mutating bytes, so the two-pass origin-preserving path is retained.
//! - ASCII ranges only: bytes outside `A`-`Z` / `a`-`z` are preserved, including high bytes.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits one case transform for the active target and runtime feature set.
pub(super) fn emit(emitter: &mut Emitter, uppercase: bool, mbstring: bool) {
    if mbstring {
        emit_mbstring_aware(emitter, uppercase);
    } else if emitter.target.arch == Arch::AArch64 {
        emit_single_pass_aarch64(emitter, uppercase);
    } else {
        emit_single_pass_x86_64(emitter, uppercase);
    }
}

/// Emits the allocating single-pass conversion for AArch64.
fn emit_single_pass_aarch64(emitter: &mut Emitter, uppercase: bool) {
    let name = if uppercase { "__rt_strtoupper" } else { "__rt_strtolower" };
    let low = if uppercase { b'a' } else { b'A' };
    let operation = if uppercase { "sub" } else { "add" };
    let loop_label = format!("{name}_loop");
    let tail = format!("{name}_tail");
    let tail_store = format!("{name}_tail_store");
    let done = format!("{name}_done");

    emitter.blank();
    emitter.comment(&format!("--- runtime: {name} ---"));
    emitter.label_global(name);
    emitter.instruction("sub sp, sp, #48");                                     // reserve the source pair, destination, and link save
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a stable frame base
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save the source pointer and logical length
    emitter.instruction("mov x0, x2");                                          // the result has exactly the source length
    emitter.instruction("bl __rt_str_new_owned");                               // allocate the exact owned heap result up front
    emitter.instruction("str x0, [sp, #16]");                                   // remember the result start for the epilogue
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the source pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the source length
    emitter.instruction("mov x9, x0");                                          // destination cursor
    emitter.instruction("mov x10, x2");                                         // remaining source byte count
    emitter.instruction("movi v2.16b, #0x20");                                  // ASCII case offset
    emitter.instruction(&format!("movi v3.16b, #0x{:02x}", low));               // first byte of the conversion range
    emitter.instruction(&format!("movi v4.16b, #0x{:02x}", low + 26));          // one past the final byte of the range
    emitter.instruction("cmp x10, #16");                                        // can the conversion move a full vector?
    emitter.instruction(&format!("b.lt {tail}"));                               // short strings use the byte tail

    emitter.label(&loop_label);
    emitter.instruction("ldr q0, [x1], #16");                                   // load the next sixteen source bytes
    emitter.instruction(&format!("{operation} v5.16b, v0.16b, v2.16b"));        // compute the case-shifted candidate bytes
    emitter.instruction("cmhs v6.16b, v0.16b, v3.16b");                         // byte >= first letter in the range
    emitter.instruction("cmhi v7.16b, v4.16b, v0.16b");                         // byte < one past the final letter
    emitter.instruction("and v6.16b, v6.16b, v7.16b");                          // keep only bytes inside the ASCII range
    emitter.instruction("bit v0.16b, v5.16b, v6.16b");                          // select the shifted byte where the range matched
    emitter.instruction("str q0, [x9], #16");                                   // store the converted vector
    emitter.instruction("sub x10, x10, #16");                                   // account for the converted bytes
    emitter.instruction("cmp x10, #16");                                        // is another full vector available?
    emitter.instruction(&format!("b.ge {loop_label}"));                         // continue converting full vectors

    emitter.label(&tail);
    emitter.instruction(&format!("cbz x10, {done}"));                           // stop when every source byte is converted
    emitter.instruction("ldrb w11, [x1], #1");                                  // load one trailing source byte
    emitter.instruction(&format!("cmp w11, #{low}"));                           // is the byte below the conversion range?
    emitter.instruction(&format!("b.lo {tail_store}"));                         // preserve bytes below the range
    emitter.instruction(&format!("cmp w11, #{}", low + 25));                    // is the byte above the conversion range?
    emitter.instruction(&format!("b.hi {tail_store}"));                         // preserve bytes above the range
    emitter.instruction(&format!("{operation} w11, w11, #32"));                 // apply the ASCII case offset
    emitter.label(&tail_store);
    emitter.instruction("strb w11, [x9], #1");                                  // store the converted or preserved byte
    emitter.instruction("sub x10, x10, #1");                                    // account for the converted byte
    emitter.instruction(&format!("b {tail}"));                                  // continue converting trailing bytes

    emitter.label(&done);
    emitter.instruction("ldr x1, [sp, #16]");                                   // return the owned result pointer without a second copy
    emitter.instruction("ldr x2, [sp, #8]");                                    // return the unchanged logical length
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                     // release the conversion frame
    emitter.instruction("ret");                                                 // return the directly written owned string
}

/// Emits the allocating single-pass conversion for Linux x86_64.
fn emit_single_pass_x86_64(emitter: &mut Emitter, uppercase: bool) {
    let name = if uppercase { "__rt_strtoupper" } else { "__rt_strtolower" };
    let low = if uppercase { b'a' } else { b'A' };
    let operation = if uppercase { "psubb" } else { "paddb" };
    let scalar_operation = if uppercase { "sub" } else { "add" };
    let loop_label = format!("{name}_loop_linux_x86_64");
    let tail = format!("{name}_tail_linux_x86_64");
    let tail_store = format!("{name}_tail_store_linux_x86_64");
    let done = format!("{name}_done_linux_x86_64");

    emitter.blank();
    emitter.comment(&format!("--- runtime: {name} ---"));
    emitter.label_global(name);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 48");                                         // reserve the source pair, destination, and call padding
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source pointer and logical length
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the source length
    emitter.instruction("mov rax, rdx");                                        // the result has exactly the source length
    emitter.instruction("call __rt_str_new_owned");                             // allocate the exact owned heap result up front
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // remember the result start for the epilogue
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // source cursor
    emitter.instruction("mov r9, rax");                                         // destination cursor
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // remaining source byte count
    for (register, value) in [("xmm2", low - 1), ("xmm3", low + 26), ("xmm6", 32)] {
        emitter.instruction(&format!("mov eax, {value}"));                      // materialize one broadcast constant
        emitter.instruction(&format!("movd {register}, eax"));                  // seed the constant's vector lane
        emitter.instruction(&format!("punpcklbw {register}, {register}"));      // duplicate the byte through its word
        emitter.instruction(&format!("punpcklwd {register}, {register}"));      // duplicate the word through its dword
        emitter.instruction(&format!("pshufd {register}, {register}, 0"));      // broadcast the dword through the vector
    }
    emitter.instruction("cmp r10, 16");                                         // can the conversion move a full vector?
    emitter.instruction(&format!("jl {tail}"));                                 // short strings use the byte tail

    emitter.label(&loop_label);
    emitter.instruction("movdqu xmm0, XMMWORD PTR [r8]");                       // load the next sixteen source bytes
    emitter.instruction("movdqa xmm5, xmm0");                                   // copy the source vector for the case-shifted candidate
    emitter.instruction(&format!("{operation} xmm5, xmm6"));                    // compute the case-shifted candidate bytes
    emitter.instruction("movdqa xmm4, xmm0");                                   // copy the source vector for the range mask
    emitter.instruction("pcmpgtb xmm4, xmm2");                                  // byte > first range byte - 1
    emitter.instruction("movdqa xmm7, xmm3");                                   // copy the one-past-final range bound
    emitter.instruction("pcmpgtb xmm7, xmm0");                                  // one-past-final > byte
    emitter.instruction("pand xmm4, xmm7");                                     // keep only bytes inside the ASCII range
    emitter.instruction("pand xmm5, xmm4");                                     // keep the shifted bytes inside the range
    emitter.instruction("pandn xmm4, xmm0");                                    // keep the original bytes outside the range
    emitter.instruction("por xmm5, xmm4");                                      // merge preserved and converted bytes
    emitter.instruction("movdqu XMMWORD PTR [r9], xmm5");                       // store the converted vector
    emitter.instruction("add r8, 16");                                          // advance the source cursor
    emitter.instruction("add r9, 16");                                          // advance the destination cursor
    emitter.instruction("sub r10, 16");                                         // account for the converted bytes
    emitter.instruction("cmp r10, 16");                                         // is another full vector available?
    emitter.instruction(&format!("jge {loop_label}"));                          // continue converting full vectors

    emitter.label(&tail);
    emitter.instruction("test r10, r10");                                       // stop when every source byte is converted
    emitter.instruction(&format!("jz {done}"));                                 // publish the completed result
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load one trailing source byte
    emitter.instruction(&format!("cmp eax, {low}"));                            // is the byte below the conversion range?
    emitter.instruction(&format!("jb {tail_store}"));                           // preserve bytes below the range
    emitter.instruction(&format!("cmp eax, {}", low + 25));                     // is the byte above the conversion range?
    emitter.instruction(&format!("ja {tail_store}"));                           // preserve bytes above the range
    emitter.instruction(&format!("{scalar_operation} eax, 32"));                // apply the ASCII case offset
    emitter.label(&tail_store);
    emitter.instruction("mov BYTE PTR [r9], al");                               // store the converted or preserved byte
    emitter.instruction("inc r8");                                              // advance the source cursor
    emitter.instruction("inc r9");                                              // advance the destination cursor
    emitter.instruction("dec r10");                                             // account for the converted byte
    emitter.instruction(&format!("jmp {tail}"));                                // continue converting trailing bytes

    emitter.label(&done);
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the owned result pointer without a second copy
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // return the unchanged logical length
    emitter.instruction("leave");                                               // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the directly written owned string
}

/// Emits the origin-preserving two-pass conversion used when mbstring tracks literals.
fn emit_mbstring_aware(emitter: &mut Emitter, uppercase: bool) {
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
    emitter.instruction(
        if arm { "ldrb w11, [x9]" } else { "movzx r9d, BYTE PTR [r8]" }
    );                                                                          // load one unsigned source byte without modifying it
    branch_outside(emitter, byte, low, &format!("{name}_scan_next"));
    if arm {
        emitter.instruction("mov x11, #1");                                     // one changed byte requires a distinct logical result
        emitter.instruction("str x11, [sp, #16]");                              // preserve that decision across string persistence
    } else {
        emitter.instruction("mov QWORD PTR [rsp], 1");                          // record that conversion requires fresh result identity
    }
    emitter.instruction(
        &format!("{} {name}_persist", if arm { "b" } else { "jmp" })
    );                                                                          // stop scanning once conversion is known to be necessary
    emitter.label(&format!("{name}_scan_next"));
    advance(emitter);
    emitter.instruction(
        &format!("{} {name}_scan", if arm { "b" } else { "jmp" })
    );                                                                          // inspect the remaining source bytes
    emitter.label(&format!("{name}_persist"));
    emitter.instruction(
        if arm { "bl __rt_str_persist" } else { "call __rt_str_persist" }
    );                                                                          // acquire independent native storage while preserving a complete known origin
    if arm {
        emitter.instruction("ldr x9, [sp, #16]");                               // recover the origin decision before mutating any copied bytes
        emitter.instruction(&format!("cbz x9, {name}_done"));                   // unchanged values preserve source identity, including fresh empty strings
    } else {
        emitter.instruction("cmp QWORD PTR [rsp], 0");                          // distinguish unchanged values from conversions that require fresh identity
        emitter.instruction(&format!("je {name}_done"));                        // keep the complete original origin for an unchanged result
    }
    emitter.instruction(
        if arm { "bl __rt_mbstring_ini_fresh" } else { "call __rt_mbstring_ini_fresh" }
    );                                                                          // detach logical identity before changing the owned payload
    emitter.instruction(&format!("mov {cursor}, {pointer}"));                   // traverse only the independently owned destination
    emitter.instruction(&format!("mov {count}, {length}"));                     // preserve the returned byte count while converting the payload
    emitter.label(&format!("{name}_convert"));
    branch_empty(emitter, count, &format!("{name}_done"));
    emitter.instruction(
        if arm { "ldrb w11, [x9]" } else { "movzx r9d, BYTE PTR [r8]" }
    );                                                                          // inspect the current owned byte for ASCII case conversion
    branch_outside(emitter, byte, low, &format!("{name}_convert_next"));
    let operation = if uppercase { "sub" } else { "add" };
    emitter.instruction(
        &if arm { format!("{operation} w11, w11, #32") } else { format!("{operation} r9d, 32") }
    );                                                                          // apply the ASCII case offset only to a matching letter
    emitter.instruction(
        if arm { "strb w11, [x9]" } else { "mov BYTE PTR [r8], r9b" }
    );                                                                          // update only the fresh logical result's owned byte
    emitter.label(&format!("{name}_convert_next"));
    advance(emitter);
    emitter.instruction(
        &format!("{} {name}_convert", if arm { "b" } else { "jmp" })
    );                                                                          // convert the remaining bytes within their exact logical length
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
    emitter.instruction(
        &format!("{} {label}", if arm { "b.lo" } else { "jb" })
    );                                                                          // preserve bytes below the selected ASCII range
    emitter.instruction(&format!("cmp {byte}, {prefix}{}", low + 25));          // compare against the final ASCII letter in the conversion range
    emitter.instruction(
        &format!("{} {label}", if arm { "b.hi" } else { "ja" })
    );                                                                          // preserve bytes above the selected ASCII range
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
