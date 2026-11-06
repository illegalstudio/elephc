//! Purpose:
//! Emits `__rt_unser_refuse_class`, which refuses a class PHP never lets `unserialize()`
//! hydrate: throwing `Unserialization of '<Class>' is not allowed` before any value exists.
//!
//! Called from:
//! - `super::emit_unserialize()` with the other shared unserialize diagnostics.
//! - The target-specific preflight validators, once each object's class name is bounded.
//!
//! Key details:
//! - The per-program `_unser_refused_classes` table (emitted with the user data) lists every
//!   wire name this program refuses: the refused classes it declares (elephc's own builtins and
//!   their subclasses) and the refused builtins it does not declare at all (a Reflection class
//!   pruning dropped, a Closure, which is no class-table entry).
//! - Refusing during the allocation-free preflight means a refused class, at any nesting depth,
//!   throws before the decoder allocates an object, box, or container, and before any
//!   `__wakeup()`/`__unserialize()` hook runs, like PHP, which calls those hooks only once the
//!   whole payload decoded.
//! - A class the `allowed_classes` policy blocks becomes `__PHP_Incomplete_Class` instead, as in
//!   PHP, so the policy is asked first.
//! - The wire spelling is matched case-insensitively, and the message names the class as it is
//!   declared. A match never returns: it tail-calls `__rt_throw_unserialization_denied_name`,
//!   whose Throwable unwinds to the public entry boundary that closes the unserialize context.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the unserialize refusal check on the selected target.
///
/// Input is the wire class-name pair (AArch64 `x1`/`x2`, x86_64 `rax`/`rdx`). The helper returns
/// normally when the class may be decoded (or becomes an incomplete object), and clobbers the
/// caller-saved registers.
pub(super) fn emit_refuse_class(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_refuse_class_aarch64(emitter),
        Arch::X86_64 => emit_refuse_class_x86_64(emitter),
    }
}

/// Emits the AArch64 refusal scan. Frame: `[sp]` wire pointer, `[sp, #8]` wire length,
/// `[sp, #16]` row index, `[sp, #32]` saved frame linkage.
fn emit_refuse_class_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: refuse unserializing a class PHP marks not serializable ---");
    emitter.label_global("__rt_unser_refuse_class");
    emitter.instruction("sub sp, sp, #48");                                     // reserve the wire name, the row index, and frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a stable frame for the table scan
    emitter.instruction("stp x1, x2, [sp]");                                    // keep the wire class name across the comparisons
    emitter.instruction("str xzr, [sp, #16]");                                  // start scanning at the first refused class

    // -- a class the allowed_classes policy blocks becomes an incomplete object, never a refusal --
    emitter.instruction("mov x0, x1");                                          // class-name pointer for the policy gate
    emitter.instruction("mov x1, x2");                                          // class-name length for the policy gate
    emitter.instruction("bl __rt_unserialize_class_allowed");                   // would unserialize() hydrate this class at all?
    emitter.instruction("cbz x0, __rt_unser_refuse_none");                      // blocked classes are never refused

    // -- compare the wire name with each class this program refuses --
    emitter.label("__rt_unser_refuse_loop");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the current row index
    abi::emit_load_symbol_to_reg(emitter, "x10", "_unser_refused_class_count", 0);
    emitter.instruction("cmp x11, x10");                                        // has every refused class been compared?
    emitter.instruction("b.hs __rt_unser_refuse_none");                         // no refused class matches: let the decoder run
    abi::emit_symbol_address(emitter, "x10", "_unser_refused_classes");
    emitter.instruction("add x10, x10, x11, lsl #4");                           // select the 16-byte (pointer, length) row
    emitter.instruction("ldp x3, x4, [x10]");                                   // refused class-name pointer and byte length
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the wire class-name length
    emitter.instruction("cmp x4, x2");                                          // names of different lengths cannot match
    emitter.instruction("b.ne __rt_unser_refuse_next");                         // skip the byte comparison for this row
    emitter.instruction("ldr x1, [sp]");                                        // reload the wire class-name pointer
    emitter.instruction("bl __rt_strcasecmp");                                  // class names compare case-insensitively in PHP
    emitter.instruction("cbz x0, __rt_unser_refuse_match");                     // equal names: PHP refuses this class
    emitter.label("__rt_unser_refuse_next");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the row index after the string helper
    emitter.instruction("add x11, x11, #1");                                    // advance to the next refused class
    emitter.instruction("str x11, [sp, #16]");                                  // persist the advanced row index
    emitter.instruction("b __rt_unser_refuse_loop");                            // compare the next refused class

    // -- a refused class: throw with its declared spelling --
    emitter.label("__rt_unser_refuse_match");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the matching row index
    abi::emit_symbol_address(emitter, "x10", "_unser_refused_classes");
    emitter.instruction("add x10, x10, x11, lsl #4");                           // select the matching row again
    emitter.instruction("ldp x1, x2, [x10]");                                   // name the class as declared, not as the wire spells it
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame before the tail call
    emitter.instruction("add sp, sp, #48");                                     // release the scan frame
    emitter.instruction("b __rt_throw_unserialization_denied_name");            // throw the catchable Exception; never returns

    emitter.label("__rt_unser_refuse_none");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                     // release the scan frame
    emitter.instruction("ret");                                                 // the class is not refused
}

/// Emits the x86_64 refusal scan. Frame: `[rbp - 8]` wire pointer, `[rbp - 16]` wire length,
/// `[rbp - 24]` row index.
fn emit_refuse_class_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: refuse unserializing a class PHP marks not serializable ---");
    emitter.label_global("__rt_unser_refuse_class");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame for the table scan
    emitter.instruction("sub rsp, 32");                                         // reserve the wire name and the row index, keeping rsp aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // keep the wire class-name pointer across the comparisons
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // keep the wire class-name length across the comparisons
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // start scanning at the first refused class

    // -- a class the allowed_classes policy blocks becomes an incomplete object, never a refusal --
    emitter.instruction("call __rt_unserialize_class_allowed");                 // would unserialize() hydrate this class at all?
    emitter.instruction("test rax, rax");                                       // did the policy allow the class?
    emitter.instruction("jz __rt_unser_refuse_none_x");                         // blocked classes are never refused

    // -- compare the wire name with each class this program refuses --
    emitter.label("__rt_unser_refuse_loop_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the current row index
    abi::emit_load_symbol_to_reg(emitter, "r10", "_unser_refused_class_count", 0);
    emitter.instruction("cmp rcx, r10");                                        // has every refused class been compared?
    emitter.instruction("jae __rt_unser_refuse_none_x");                        // no refused class matches: let the decoder run
    abi::emit_symbol_address(emitter, "r10", "_unser_refused_classes");
    emitter.instruction("shl rcx, 4");                                          // scale the row index to the 16-byte row
    emitter.instruction("mov rdx, QWORD PTR [r10 + rcx]");                      // refused class-name pointer (second string)
    emitter.instruction("mov rcx, QWORD PTR [r10 + rcx + 8]");                  // refused class-name byte length (second string)
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // names of different lengths cannot match
    emitter.instruction("jne __rt_unser_refuse_next_x");                        // skip the byte comparison for this row
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // wire class-name pointer (first string)
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // wire class-name length (first string)
    emitter.instruction("call __rt_strcasecmp");                                // class names compare case-insensitively in PHP
    emitter.instruction("test rax, rax");                                       // did the names match?
    emitter.instruction("je __rt_unser_refuse_match_x");                        // equal names: PHP refuses this class
    emitter.label("__rt_unser_refuse_next_x");
    emitter.instruction("add QWORD PTR [rbp - 24], 1");                         // advance to the next refused class
    emitter.instruction("jmp __rt_unser_refuse_loop_x");                        // compare the next refused class

    // -- a refused class: throw with its declared spelling --
    emitter.label("__rt_unser_refuse_match_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the matching row index
    abi::emit_symbol_address(emitter, "r10", "_unser_refused_classes");
    emitter.instruction("shl rcx, 4");                                          // scale the row index to the 16-byte row
    emitter.instruction("mov rax, QWORD PTR [r10 + rcx]");                      // name the class as declared, not as the wire spells it
    emitter.instruction("mov rdx, QWORD PTR [r10 + rcx + 8]");                  // and pass its byte length
    emitter.instruction("leave");                                               // release the scan frame before the tail call
    emitter.instruction("jmp __rt_throw_unserialization_denied_name");          // throw the catchable Exception; never returns

    emitter.label("__rt_unser_refuse_none_x");
    emitter.instruction("leave");                                               // release the scan frame
    emitter.instruction("ret");                                                 // the class is not refused
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported target asks the allowed_classes policy first, then scans the per-program
    /// refused-class table and tail-calls the name thrower.
    #[test]
    fn refusal_scan_covers_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_refuse_class(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_unser_refuse_class"), "{name}: {asm}");
            let policy = asm.find("__rt_unserialize_class_allowed").expect("policy gate");
            let table = asm.find("_unser_refused_classes").expect("refused-class table");
            assert!(policy < table, "{name}: the policy must be asked before the scan");
            assert!(asm.contains("__rt_strcasecmp"), "{name}: {asm}");
            assert!(asm.contains("__rt_throw_unserialization_denied_name"), "{name}: {asm}");
        }
    }
}
