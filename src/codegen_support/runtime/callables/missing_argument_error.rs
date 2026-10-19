//! Purpose:
//! Emits `__rt_throw_missing_argument`, PHP's catchable
//! `ArgumentCountError: <callee>(): Argument #N ($name) not passed`.
//!
//! Called from:
//! - `crate::codegen::runtime_callable_invoker`, when a descriptor invoker binds an associative
//!   (named) argument container and a required parameter has neither a name nor a position.
//!
//! Key details:
//! - The callee name is only known at run time: invokers are shared by signature, so the name is
//!   read from the descriptor (`<class>::<method>` or the function name) and an anonymous
//!   descriptor prints `{closure}`, like PHP.
//! - The `(): Argument #N ($name) not passed` tail is a per-parameter constant the invoker passes.
//! - `__rt_concat` reads its LEFT operand from the string-result pair and its RIGHT one from a
//!   different pair per target; the right pair is stashed in the frame across nothing else.
//! - Never returns: `__rt_throw_current` unwinds to the nearest handler or reports it uncaught.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::data::CLOSURE_CALLEE_DISPLAY_NAME;
use crate::codegen_support::sentinels::{
    emit_throwable_creation_line_unknown, x86_64_heap_kind_word,
};

/// Emits `__rt_throw_missing_argument`.
///
/// Input: the callee name (pointer, byte length; length 0 means an anonymous callee) and the
/// message tail (pointer, byte length) as `x0`-`x3` on ARM64 and `rdi`/`rsi`/`rdx`/`rcx` on
/// x86_64. Raises `ArgumentCountError`. Never returns.
pub fn emit_throw_missing_argument(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }
    emit_aarch64(emitter);
}

/// Emits the ARM64 thrower.
fn emit_aarch64(emitter: &mut Emitter) {
    let closure_len = CLOSURE_CALLEE_DISPLAY_NAME.len();
    emitter.blank();
    emitter.comment("--- runtime: throw missing-argument ArgumentCountError ---");
    emitter.label_global("__rt_throw_missing_argument");

    // Stack (48 bytes): [sp, #0] the message pair, [sp, #16] the tail pair.
    emitter.instruction("sub sp, sp, #48");                                     // reserve message state and frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a stable Error-construction frame
    emitter.instruction("stp x2, x3, [sp, #16]");                               // stash the message tail across the concatenation
    emitter.instruction("cbnz x1, __rt_throw_missing_argument_named");          // a named callee prints its own name
    abi::emit_symbol_address(emitter, "x0", "_closure_callee_display_name");    // an anonymous callee prints {closure}
    emitter.instruction(&format!("mov x1, #{closure_len}"));                    // byte length of {closure}
    emitter.label("__rt_throw_missing_argument_named");
    emitter.instruction("mov x2, x1");                                          // concat left operand length: the callee name
    emitter.instruction("mov x1, x0");                                          // concat left operand pointer
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // right operand: the message tail
    emitter.instruction("bl __rt_concat");                                      // build `<callee>(): Argument #N ($name) not passed`
    emitter.instruction("bl __rt_str_persist");                                 // give the Throwable stable message ownership
    emitter.instruction("stp x1, x2, [sp]");                                    // preserve the message pair across the allocation

    emitter.instruction("mov x0, #56");                                         // canonical Throwable payload size
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the ArgumentCountError payload
    emitter.instruction("mov x9, #6");                                          // heap kind 6 identifies a throwable object
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the allocation as a runtime object
    emitter.instruction("bl __rt_object_handle_acquire");                       // bind the Throwable to its PHP object handle
    abi::emit_load_symbol_to_reg(emitter, "x9", "_spl_argument_count_error_class_id", 0);
    emitter.instruction("str x9, [x0]");                                        // stamp the ArgumentCountError class id
    emitter.instruction("ldp x10, x11, [sp]");                                  // recover the persisted message pair
    emitter.instruction("str x10, [x0, #8]");                                   // message pointer
    emitter.instruction("str x11, [x0, #16]");                                  // message byte length
    // __rt_heap_alloc recycles blocks without zeroing, so every remaining slot is written here.
    emitter.instruction("str xzr, [x0, #24]");                                  // code = 0
    emit_throwable_creation_line_unknown(emitter, "x0");
    emitter.instruction("str xzr, [x0, #40]");                                  // previous = null
    abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);              // publish the Throwable for the unwinder
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("b __rt_throw_current");                                // unwind, or report it uncaught and exit like PHP
}

/// Emits the x86_64 thrower.
fn emit_x86_64(emitter: &mut Emitter) {
    let closure_len = CLOSURE_CALLEE_DISPLAY_NAME.len();
    emitter.blank();
    emitter.comment("--- runtime: throw missing-argument ArgumentCountError ---");
    emitter.label_global("__rt_throw_missing_argument");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish an Error-construction frame
    emitter.instruction("and rsp, -16");                                        // align the frame for the nested calls whatever the caller left
    emitter.instruction("sub rsp, 32");                                         // reserve the message and tail pairs
    emitter.instruction("mov QWORD PTR [rsp + 16], rdx");                       // stash the message tail pointer across the concatenation
    emitter.instruction("mov QWORD PTR [rsp + 24], rcx");                       // stash its byte length too
    emitter.instruction("mov rax, rdi");                                        // concat left operand pointer: the callee name
    emitter.instruction("mov rdx, rsi");                                        // concat left operand length
    emitter.instruction("test rsi, rsi");                                       // an anonymous callee has no name bytes
    emitter.instruction("jnz __rt_throw_missing_argument_named");               // a named callee prints its own name
    emitter.instruction("lea rax, [rip + _closure_callee_display_name]");       // an anonymous callee prints {closure}
    emitter.instruction(&format!("mov rdx, {closure_len}"));                    // byte length of {closure}
    emitter.label("__rt_throw_missing_argument_named");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");                       // right operand: the message tail
    emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");                       // and its byte length
    abi::emit_call_label(emitter, "__rt_concat");                               // build `<callee>(): Argument #N ($name) not passed`
    abi::emit_call_label(emitter, "__rt_str_persist");                          // give the Throwable stable message ownership
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // preserve the message pointer across the allocation
    emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                        // preserve the message byte length

    emitter.instruction("mov rax, 56");                                         // canonical Throwable payload size
    abi::emit_call_label(emitter, "__rt_heap_alloc");                           // allocate the ArgumentCountError payload (rax = payload)
    emitter.instruction(&format!("mov r10, 0x{:x}", x86_64_heap_kind_word(6))); // magic + kind 6 identifies a throwable object
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the uniform heap header
    abi::emit_call_label(emitter, "__rt_object_handle_acquire");                // bind the Throwable to its PHP object handle
    abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_argument_count_error_class_id", 0);
    emitter.instruction("mov QWORD PTR [rax], r10");                            // stamp the ArgumentCountError class id
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // recover the message pointer
    emitter.instruction("mov r11, QWORD PTR [rsp + 8]");                        // recover the message byte length
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // message pointer
    emitter.instruction("mov QWORD PTR [rax + 16], r11");                       // message byte length
    // __rt_heap_alloc recycles blocks without zeroing, so every remaining slot is written here.
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // code = 0
    emit_throwable_creation_line_unknown(emitter, "rax");
    emitter.instruction("mov QWORD PTR [rax + 40], 0");                         // previous = null
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);             // publish the Throwable for the unwinder
    emitter.instruction("mov rsp, rbp");                                        // release the local frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_throw_current");                              // unwind, or report it uncaught and exit like PHP
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target composes the message from the runtime callee name and the constant tail,
    /// falls back to `{closure}` for an anonymous callee, and throws an ArgumentCountError.
    #[test]
    fn missing_argument_error_composes_callee_name_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_throw_missing_argument(&mut emitter);
            let asm = emitter.output();
            if let Ok(dir) = std::env::var("ELEPHC_DUMP_MISSING_ARGUMENT_ASM") {
                std::fs::write(format!("{dir}/{name}.s"), &asm).unwrap();
            }
            let entry = asm.find("__rt_throw_missing_argument:").unwrap();
            let closure = asm.find("_closure_callee_display_name").unwrap();
            let concat = asm.find("__rt_concat").unwrap();
            let class = asm.find("_spl_argument_count_error_class_id").unwrap();
            let throw = asm.find("__rt_throw_current").unwrap();
            assert!(entry < closure && closure < concat && concat < class && class < throw, "{name}: {asm}");
        }
    }
}
