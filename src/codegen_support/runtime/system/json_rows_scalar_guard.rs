//! Purpose:
//! Emits `__rt_json_rows_scalar_guard`, the depth-two scalar preflight used by the
//! loop-invariant `strlen(json_encode($rows))` cache.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via the system module.
//!
//! Key details:
//! - The source must be a packed array of associative rows. The outer payload walk reads one
//!   hash pointer per element; each hash is iterated in insertion order.
//! - A row value is safe to re-encode from a cached length only when its runtime tag is a
//!   scalar (int, string, float, bool) or null. Objects, resources, nested containers and
//!   every unknown tag return 0, so the caller keeps the per-iteration encoder and its
//!   `JsonSerializable` side effects.
//! - The helper only reads heap storage and never allocates, warns or throws.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the depth-two scalar-row guard for the active target.
pub fn emit_json_rows_scalar_guard(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }
    emit_aarch64(emitter);
}

/// Emits the AArch64 guard.
fn emit_aarch64(emitter: &mut Emitter) {
    let outer_loop = "__rt_json_rows_guard_outer";
    let inner_loop = "__rt_json_rows_guard_inner";
    let next_row = "__rt_json_rows_guard_next";
    let scalar = "__rt_json_rows_guard_scalar";
    let done = "__rt_json_rows_guard_done";
    let fail = "__rt_json_rows_guard_fail";
    let exit = "__rt_json_rows_guard_exit";

    emitter.blank();
    emitter.comment("--- runtime: json_rows_scalar_guard ---");
    emitter.label_global("__rt_json_rows_scalar_guard");
    emitter.instruction("cbnz x0, __rt_json_rows_guard_body");                  // a non-null outer container enters the frame
    emitter.instruction("mov x0, #0");                                          // a null outer container keeps the encoder
    emitter.instruction("ret");                                                 // return the guard result without a frame
    emitter.label("__rt_json_rows_guard_body");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the caller frame and return address
    emitter.instruction("mov x29, sp");                                         // establish a stable helper frame
    emitter.instruction("stp x19, x20, [sp, #-16]!");                           // preserve the outer array and its element count
    emitter.instruction("stp x21, x22, [sp, #-16]!");                           // preserve the outer payload base and row index
    emitter.instruction("stp x23, x24, [sp, #-16]!");                           // preserve the current row hash and its remaining count
    emitter.instruction("stp x25, x26, [sp, #-16]!");                           // preserve the hash iterator cursor and alignment slot
    emitter.instruction("mov x19, x0");                                         // x19 = packed outer array
    emitter.instruction("ldr x20, [x19]");                                      // x20 = outer element count
    emitter.instruction("add x21, x19, #24");                                   // x21 = outer payload base
    emitter.instruction("mov x22, xzr");                                        // x22 = outer row index

    emitter.label(outer_loop);
    emitter.instruction("cmp x22, x20");                                        // has every row been checked?
    emitter.instruction(&format!("b.hs {done}"));                               // all rows are scalar-only
    emitter.instruction("ldr x23, [x21, x22, lsl #3]");                         // x23 = current row hash pointer
    emitter.instruction(&format!("cbz x23, {fail}"));                           // a missing row falls back to the encoder
    emitter.instruction("ldr x24, [x23]");                                      // x24 = remaining hash entries
    emitter.instruction("mov x25, xzr");                                        // x25 = insertion-order cursor

    emitter.label(inner_loop);
    emitter.instruction(&format!("cbz x24, {next_row}"));                       // every row value has been classified
    emitter.instruction("mov x0, x23");                                         // pass the row hash
    emitter.instruction("mov x1, x25");                                         // pass the iterator cursor
    abi::emit_call_label(emitter, "__rt_hash_iter_next_value");                 // x0=cursor, x5=value tag
    emitter.instruction("mov x25, x0");                                         // keep the advanced cursor
    emitter.instruction("cmp x5, #3");                                          // int/string/float/bool tags are scalar
    emitter.instruction(&format!("b.ls {scalar}"));                             // accept the scalar tags
    emitter.instruction("cmp x5, #8");                                          // is this PHP null?
    emitter.instruction(&format!("b.eq {scalar}"));                             // null encodes deterministically
    emitter.instruction("cmp x5, #7");                                          // is this a boxed Mixed value?
    emitter.instruction(&format!("b.ne {fail}"));                               // objects, containers and resources fall back
    emitter.instruction(&format!("cbz x3, {fail}"));                            // a missing Mixed cell falls back
    emitter.instruction("ldr x6, [x3]");                                        // read the boxed cell's runtime tag
    emitter.instruction("cmp x6, #3");                                          // int/string/float/bool are scalar
    emitter.instruction(&format!("b.ls {scalar}"));                             // accept the unboxed scalar tags
    emitter.instruction("cmp x6, #8");                                          // is the boxed value PHP null?
    emitter.instruction(&format!("b.eq {scalar}"));                             // null encodes deterministically
    emitter.instruction(&format!("b {fail}"));                                  // nested containers and objects fall back

    emitter.label(scalar);
    emitter.instruction("sub x24, x24, #1");                                    // account for the classified value
    emitter.instruction(&format!("b {inner_loop}"));                            // classify the next row value

    emitter.label(next_row);
    emitter.instruction("add x22, x22, #1");                                    // advance to the next outer row
    emitter.instruction(&format!("b {outer_loop}"));                            // classify the next row

    emitter.label(done);
    emitter.instruction("mov x0, #1");                                          // the cached encoded length is safe
    emitter.instruction(&format!("b {exit}"));                                  // restore the helper frame

    emitter.label(fail);
    emitter.instruction("mov x0, #0");                                          // keep the per-iteration encoder
    emitter.instruction("b __rt_json_rows_guard_unwind");                       // restore the helper frame

    emitter.label("__rt_json_rows_guard_unwind");
    emitter.instruction("ldp x25, x26, [sp], #16");                             // restore the iterator cursor and alignment slot
    emitter.instruction("ldp x23, x24, [sp], #16");                             // restore the row hash and its remaining count
    emitter.instruction("ldp x21, x22, [sp], #16");                             // restore the payload base and row index
    emitter.instruction("ldp x19, x20, [sp], #16");                             // restore the outer array and element count
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame and return address
    emitter.instruction("ret");                                                 // return the guard result

    emitter.label(exit);
    emitter.instruction("ldp x25, x26, [sp], #16");                             // restore the iterator cursor and alignment slot
    emitter.instruction("ldp x23, x24, [sp], #16");                             // restore the row hash and its remaining count
    emitter.instruction("ldp x21, x22, [sp], #16");                             // restore the payload base and row index
    emitter.instruction("ldp x19, x20, [sp], #16");                             // restore the outer array and element count
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame and return address
    emitter.instruction("ret");                                                 // return the guard result
}

/// Emits the x86_64 guard.
fn emit_x86_64(emitter: &mut Emitter) {
    let outer_loop = "__rt_json_rows_guard_outer_x86";
    let inner_loop = "__rt_json_rows_guard_inner_x86";
    let next_row = "__rt_json_rows_guard_next_x86";
    let scalar = "__rt_json_rows_guard_scalar_x86";
    let done = "__rt_json_rows_guard_done_x86";
    let fail = "__rt_json_rows_guard_fail_x86";
    let exit = "__rt_json_rows_guard_exit_x86";

    emitter.blank();
    emitter.comment("--- runtime: json_rows_scalar_guard ---");
    emitter.label_global("__rt_json_rows_scalar_guard");
    emitter.instruction("test rdi, rdi");                                       // a null outer container cannot use the cached length
    emitter.instruction("jnz __rt_json_rows_guard_body_x86");                   // a non-null outer container enters the frame
    emitter.instruction("xor eax, eax");                                        // a null outer container keeps the encoder
    emitter.instruction("ret");                                                 // return the guard result without a frame
    emitter.label("__rt_json_rows_guard_body_x86");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("push r12");                                            // preserve the outer array
    emitter.instruction("push r13");                                            // preserve the outer element count
    emitter.instruction("push r14");                                            // preserve the outer payload base
    emitter.instruction("push r15");                                            // preserve the outer row index
    emitter.instruction("push rbx");                                            // preserve the current row hash
    emitter.instruction("sub rsp, 24");                                         // remaining hash count, cursor, and call alignment
    emitter.instruction("mov r12, rdi");                                        // r12 = packed outer array
    emitter.instruction("mov r13, QWORD PTR [r12]");                            // r13 = outer element count
    emitter.instruction("lea r14, [r12 + 24]");                                 // r14 = outer payload base
    emitter.instruction("xor r15d, r15d");                                      // r15 = outer row index

    emitter.label(outer_loop);
    emitter.instruction("cmp r15, r13");                                        // has every row been checked?
    emitter.instruction(&format!("jae {done}"));                                // all rows are scalar-only
    emitter.instruction("mov rbx, QWORD PTR [r14 + r15 * 8]");                  // rbx = current row hash pointer
    emitter.instruction("test rbx, rbx");                                       // a missing row cannot use the cached length
    emitter.instruction(&format!("jz {fail}"));                                 // keep the per-iteration encoder
    emitter.instruction("mov rax, QWORD PTR [rbx]");                            // load the row hash entry count
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // save the remaining row entries
    emitter.instruction("mov QWORD PTR [rsp + 8], 0");                          // start the insertion-order cursor at zero

    emitter.label(inner_loop);
    emitter.instruction("cmp QWORD PTR [rsp], 0");                              // every row value has been classified
    emitter.instruction(&format!("je {next_row}"));                             // advance to the next row
    emitter.instruction("mov rdi, rbx");                                        // pass the row hash
    emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                        // pass the iterator cursor
    abi::emit_call_label(emitter, "__rt_hash_iter_next_value");                 // rax=cursor, r9=value tag
    emitter.instruction("mov QWORD PTR [rsp + 8], rax");                        // keep the advanced cursor
    emitter.instruction("cmp r9, 3");                                           // int/string/float/bool tags are scalar
    emitter.instruction(&format!("jbe {scalar}"));                              // accept the scalar tags
    emitter.instruction("cmp r9, 8");                                           // is this PHP null?
    emitter.instruction(&format!("je {scalar}"));                               // null encodes deterministically
    emitter.instruction("cmp r9, 7");                                           // is this a boxed Mixed value?
    emitter.instruction(&format!("jne {fail}"));                                // objects, containers and resources fall back
    emitter.instruction("test rcx, rcx");                                       // a missing Mixed cell falls back
    emitter.instruction(&format!("jz {fail}"));                                 // keep the per-iteration encoder
    emitter.instruction("mov r10, QWORD PTR [rcx]");                            // read the boxed cell's runtime tag
    emitter.instruction("cmp r10, 3");                                          // int/string/float/bool are scalar
    emitter.instruction(&format!("jbe {scalar}"));                              // accept the unboxed scalar tags
    emitter.instruction("cmp r10, 8");                                          // is the boxed value PHP null?
    emitter.instruction(&format!("je {scalar}"));                               // null encodes deterministically
    emitter.instruction(&format!("jmp {fail}"));                                // nested containers and objects fall back

    emitter.label(scalar);
    emitter.instruction("sub QWORD PTR [rsp], 1");                              // account for the classified value
    emitter.instruction(&format!("jmp {inner_loop}"));                          // classify the next row value

    emitter.label(next_row);
    emitter.instruction("add r15, 1");                                          // advance to the next outer row
    emitter.instruction(&format!("jmp {outer_loop}"));                          // classify the next row

    emitter.label(done);
    emitter.instruction("mov eax, 1");                                          // the cached encoded length is safe
    emitter.instruction(&format!("jmp {exit}"));                                // restore the helper frame

    emitter.label(fail);
    emitter.instruction("xor eax, eax");                                        // keep the per-iteration encoder
    emitter.instruction("jmp __rt_json_rows_guard_unwind_x86");                 // restore the helper frame

    emitter.label("__rt_json_rows_guard_unwind_x86");
    emitter.instruction("add rsp, 24");                                         // release the two local slots and alignment
    emitter.instruction("pop rbx");                                             // restore the current row hash
    emitter.instruction("pop r15");                                             // restore the outer row index
    emitter.instruction("pop r14");                                             // restore the outer payload base
    emitter.instruction("pop r13");                                             // restore the outer element count
    emitter.instruction("pop r12");                                             // restore the outer array
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the guard result

    emitter.label(exit);
    emitter.instruction("add rsp, 24");                                         // release the two local slots and alignment
    emitter.instruction("pop rbx");                                             // restore the current row hash
    emitter.instruction("pop r15");                                             // restore the outer row index
    emitter.instruction("pop r14");                                             // restore the outer payload base
    emitter.instruction("pop r13");                                             // restore the outer element count
    emitter.instruction("pop r12");                                             // restore the outer array
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the guard result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported target emits the outer walk, hash iteration and scalar tag checks.
    #[test]
    fn json_rows_scalar_guard_emits_all_targets() {
        for target in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_json_rows_scalar_guard(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_json_rows_scalar_guard"), "{target}");
            assert!(asm.contains("__rt_hash_iter_next_value"), "{target}");
            if target == "linux-x86_64" {
                assert!(asm.contains("cmp r9, 3"), "{target}");
                assert!(asm.contains("cmp r9, 8"), "{target}");
            } else {
                assert!(asm.contains("cmp x5, #3"), "{target}");
                assert!(asm.contains("cmp x5, #8"), "{target}");
            }
        }
    }
}
