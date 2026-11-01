//! Purpose:
//! Emits `__rt_eval_borrow_arg`, the shared borrow of one normalized eval call argument, and
//! `__rt_eval_scope_allowed`, the shared visibility-scope test of the eval dispatch helpers.
//!
//! Called from:
//! - `crate::codegen_support::runtime::eval_bridge::emit_eval_value_runtime()`.
//!
//! Key details:
//! - A protected member's allowed scopes are every class related to its declaring class. They
//!   were compared inline, one `strcasecmp` per scope at every dispatch arm, and Symfony's
//!   compiled container (hundreds of factory subclasses, thousands of protected statics) made
//!   that three million arms. The arm now passes a deduplicated table (`[count, ptr, len, ...]`)
//!   to this loop.
//! - Every native method, static method and constructor bridge body reads its arguments out of
//!   the interpreter's normalized argument array. That validation used to be emitted inline for
//!   each parameter of each bridged method (22 instructions), which made the bridge scale with
//!   the program's method count; the body now calls this helper and keeps one branch.
//! - Contract: AArch64 `x0` = argument array box, `x1` = index; x86_64 `rax` = box, `rdi` =
//!   index. The borrowed cell comes back in `x0`/`rax` and an ok flag in `x1`/`rdi` (0 when the
//!   array is missing, not indexed storage of boxed cells, or too short). No refcount changes.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::emit_branch_if_null_container;

/// Emits the shared argument-borrow and scope-test helpers for the current target.
pub(crate) fn emit(emitter: &mut Emitter) {
    emit_borrow_arg(emitter);
    emit_scope_allowed(emitter);
}

/// Emits `__rt_eval_scope_allowed`: AArch64 `x1`/`x2` = active scope name, `x3` = scope table;
/// x86_64 `rdi`/`rsi` = scope name, `rdx` = table. Answers 1 in `x0`/`rax` when the scope
/// matches one table entry case-insensitively, else 0.
fn emit_scope_allowed(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: eval bridge scope test ---");
    emitter.label_global("__rt_eval_scope_allowed");
    let lp = "__rt_eval_scope_allowed_loop";
    let no = "__rt_eval_scope_allowed_no";
    let yes = "__rt_eval_scope_allowed_yes";
    let out = "__rt_eval_scope_allowed_out";
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #48");                             // frame for the scope name, cursor and count
            emitter.instruction("stp x29, x30, [sp, #32]");                     // save frame pointer and return address
            emitter.instruction("add x29, sp, #32");                            // establish the frame pointer
            emitter.instruction("stp x19, x20, [sp]");                          // x19 = scope pointer, x20 = scope length
            emitter.instruction("stp x21, x22, [sp, #16]");                     // x21 = table cursor, x22 = remaining entries
            emitter.instruction("mov x19, x1");                                 // keep the scope pointer across compares
            emitter.instruction("mov x20, x2");                                 // keep the scope length across compares
            emitter.instruction("ldr x22, [x3]");                               // number of allowed scope names
            emitter.instruction("add x21, x3, #8");                             // first (pointer, length) entry
            emitter.label(lp);
            emitter.instruction(&format!("cbz x22, {no}"));                     // no entry matched the active scope
            emitter.instruction("mov x1, x19");                                 // active scope pointer
            emitter.instruction("mov x2, x20");                                 // active scope length
            emitter.instruction("ldp x3, x4, [x21]");                           // allowed scope name pointer and length
            emitter.instruction("bl __rt_strcasecmp");                          // compare class names with PHP case-insensitive rules
            emitter.instruction(&format!("cbz x0, {yes}"));                     // the active scope may see this member
            emitter.instruction("add x21, x21, #16");                           // next table entry
            emitter.instruction("sub x22, x22, #1");                            // one entry fewer to try
            emitter.instruction(&format!("b {lp}"));                            // keep scanning the allowed scopes
            emitter.label(yes);
            emitter.instruction("mov x0, #1");                                  // scope allowed
            emitter.instruction(&format!("b {out}"));                           // restore and return
            emitter.label(no);
            emitter.instruction("mov x0, xzr");                                 // scope rejected
            emitter.label(out);
            emitter.instruction("ldp x19, x20, [sp]");                          // restore callee-saved registers
            emitter.instruction("ldp x21, x22, [sp, #16]");                     // restore callee-saved registers
            emitter.instruction("ldp x29, x30, [sp, #32]");                     // restore frame pointer and return address
            emitter.instruction("add sp, sp, #48");                             // release the frame
            emitter.instruction("ret");                                         // answer the dispatch arm
        }
        Arch::X86_64 => {
            emitter.instruction("push rbx");                                    // scope pointer
            emitter.instruction("push r12");                                    // scope length
            emitter.instruction("push r13");                                    // table cursor
            emitter.instruction("push r14");                                    // remaining entries
            emitter.instruction("sub rsp, 8");                                  // keep the stack 16-byte aligned for calls
            emitter.instruction("mov rbx, rdi");                                // keep the scope pointer across compares
            emitter.instruction("mov r12, rsi");                                // keep the scope length across compares
            emitter.instruction("mov r14, QWORD PTR [rdx]");                    // number of allowed scope names
            emitter.instruction("lea r13, [rdx + 8]");                          // first (pointer, length) entry
            emitter.label(lp);
            emitter.instruction("test r14, r14");                               // any entry left?
            emitter.instruction(&format!("jz {no}"));                           // no entry matched the active scope
            emitter.instruction("mov rdi, rbx");                                // active scope pointer
            emitter.instruction("mov rsi, r12");                                // active scope length
            emitter.instruction("mov rdx, QWORD PTR [r13]");                    // allowed scope name pointer
            emitter.instruction("mov rcx, QWORD PTR [r13 + 8]");                // allowed scope name length
            emitter.instruction("call __rt_strcasecmp");                        // compare class names with PHP case-insensitive rules
            emitter.instruction("test rax, rax");                               // did the names match?
            emitter.instruction(&format!("jz {yes}"));                          // the active scope may see this member
            emitter.instruction("add r13, 16");                                 // next table entry
            emitter.instruction("dec r14");                                     // one entry fewer to try
            emitter.instruction(&format!("jmp {lp}"));                          // keep scanning the allowed scopes
            emitter.label(yes);
            emitter.instruction("mov eax, 1");                                  // scope allowed
            emitter.instruction(&format!("jmp {out}"));                         // restore and return
            emitter.label(no);
            emitter.instruction("xor eax, eax");                                // scope rejected
            emitter.label(out);
            emitter.instruction("add rsp, 8");                                  // drop the alignment pad
            emitter.instruction("pop r14");                                     // restore callee-saved registers
            emitter.instruction("pop r13");                                     // restore callee-saved registers
            emitter.instruction("pop r12");                                     // restore callee-saved registers
            emitter.instruction("pop rbx");                                     // restore callee-saved registers
            emitter.instruction("ret");                                         // answer the dispatch arm
        }
    }
}

/// Emits `__rt_eval_borrow_arg` for the current target.
fn emit_borrow_arg(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: eval bridge argument borrow ---");
    emitter.label_global("__rt_eval_borrow_arg");
    let fail = "__rt_eval_borrow_arg_fail";
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {fail}"));                    // reject a missing argument array
            emitter.instruction("ldr x9, [x0]");                                // read the normalized argument container tag
            emitter.instruction("cmp x9, #4");                                  // normalized call arguments use indexed storage
            emitter.instruction(&format!("b.ne {fail}"));                       // do not interpret hash or scalar payloads as indexed storage
            emitter.instruction("ldr x0, [x0, #8]");                            // borrow the indexed payload rooted by the argument box
            emit_branch_if_null_container(emitter, "x0", "x9", fail);
            emitter.instruction("ldr x9, [x0, #-8]");                           // read the indexed element representation
            emitter.instruction("ubfx x9, x9, #8, #7");                         // isolate the element value tag
            emitter.instruction("cmp x9, #7");                                  // a borrowed argument must already be a boxed Mixed cell
            emitter.instruction(&format!("b.ne {fail}"));                       // reject raw scalar element storage
            emitter.instruction("ldr x9, [x0]");                                // read the number of bound arguments
            emitter.instruction("cmp x1, x9");                                  // check the requested argument before loading its slot
            emitter.instruction(&format!("b.hs {fail}"));                       // reject a missing argument without reading past the array
            emitter.instruction("add x0, x0, #24");                             // address the packed Mixed pointer slots
            emitter.instruction("ldr x0, [x0, x1, lsl #3]");                    // borrow the argument cell without incrementing its refcount
            emitter.instruction("mov x1, #1");                                  // report a successful borrow
            emitter.instruction("ret");                                         // return the borrowed cell to the bridge body
            emitter.label(fail);
            emitter.instruction("mov x0, xzr");                                 // no cell on failure
            emitter.instruction("mov x1, xzr");                                 // report the failed borrow
            emitter.instruction("ret");                                         // let the bridge body take its prep-fail path
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // check whether an argument array exists
            emitter.instruction(&format!("jz {fail}"));                         // reject a missing argument array
            emitter.instruction("cmp QWORD PTR [rax], 4");                      // normalized call arguments use indexed storage
            emitter.instruction(&format!("jne {fail}"));                        // do not interpret hash or scalar payloads as indexed storage
            emitter.instruction("mov rax, QWORD PTR [rax + 8]");                // borrow the indexed payload rooted by the argument box
            emit_branch_if_null_container(emitter, "rax", "r10", fail);
            emitter.instruction("mov r10, QWORD PTR [rax - 8]");                // read the indexed element representation
            emitter.instruction("shr r10, 8");                                  // shift the element value tag to the low bits
            emitter.instruction("and r10, 127");                                // discard heap flags outside the value tag
            emitter.instruction("cmp r10, 7");                                  // a borrowed argument must already be a boxed Mixed cell
            emitter.instruction(&format!("jne {fail}"));                        // reject raw scalar element storage
            emitter.instruction("cmp rdi, QWORD PTR [rax]");                    // check the argument index against the logical length
            emitter.instruction(&format!("jae {fail}"));                        // reject a missing argument without reading past the array
            emitter.instruction("mov rax, QWORD PTR [rax + rdi * 8 + 24]");     // borrow the argument cell without incrementing its refcount
            emitter.instruction("mov edi, 1");                                  // report a successful borrow
            emitter.instruction("ret");                                         // return the borrowed cell to the bridge body
            emitter.label(fail);
            emitter.instruction("xor eax, eax");                                // no cell on failure
            emitter.instruction("xor edi, edi");                                // report the failed borrow
            emitter.instruction("ret");                                         // let the bridge body take its prep-fail path
        }
    }
}
