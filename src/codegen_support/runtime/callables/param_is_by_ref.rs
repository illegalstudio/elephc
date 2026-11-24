//! Purpose:
//! Emits `__rt_callable_param_is_by_ref`, the native answer to "does this callable take the
//! argument at this position or name by reference?" for a resolved callable descriptor.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::callables`.
//! - `crate::codegen::lower_inst::builtins::eval::lower_callable_parameter_is_by_ref` when no
//!   interpreter is linked (a closed world owns no interpreter-side callable).
//!
//! Key details:
//! - It reads only the signature side record every descriptor already carries (visible parameter
//!   count, source-variadic index, parameter-name table, by-reference flag table), so one shared
//!   body serves closures, first-class callables, bound methods, named functions, builtins and
//!   extern functions alike; no per-site ladder and no new descriptor data.
//! - The selection mirrors the interpreter's `CallArgumentModes::parameter_index`: a name selects
//!   the parameter of that name, else the source variadic; a position selects the parameter at
//!   that index, clamped to the variadic index. Anything outside the signature, or a descriptor
//!   with no signature record, answers `false`.

use crate::codegen_support::callable_descriptor::{
    CALLABLE_DESC_SIGNATURE_OFFSET, CALLABLE_SIG_PARAM_NAMES_OFFSET, CALLABLE_SIG_REF_FLAGS_OFFSET,
    CALLABLE_SIG_VARIADIC_INDEX_OFFSET, CALLABLE_SIG_VISIBLE_COUNT_OFFSET,
};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_callable_param_is_by_ref` runtime helper for the active target.
///
/// ## ARM64 ABI
/// - **Input**: `x0` = descriptor pointer, `x1`/`x2` = argument name pointer/length (length 0
///   selects by position), `x3` = argument position
/// - **Output**: `x0` = 1 when the selected parameter is by reference, else 0
///
/// ## x86_64 ABI
/// - **Input**: `rdi` = descriptor pointer, `rsi`/`rdx` = argument name pointer/length (length 0
///   selects by position), `rcx` = argument position
/// - **Output**: `rax` = 1 when the selected parameter is by reference, else 0
pub(crate) fn emit_callable_param_is_by_ref(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_callable_param_is_by_ref_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: callable parameter passing mode ---");
    emitter.label_global("__rt_callable_param_is_by_ref");

    emitter.instruction("cbz x0, __rt_callable_param_is_by_ref_no");            // a null descriptor has no parameters
    emitter.instruction(&format!("ldr x4, [x0, #{}]", CALLABLE_DESC_SIGNATURE_OFFSET)); // x4 = signature side record
    emitter.instruction("cbz x4, __rt_callable_param_is_by_ref_no");            // a descriptor without a signature takes everything by value
    emitter.instruction(&format!("ldr x5, [x4, #{}]", CALLABLE_SIG_VISIBLE_COUNT_OFFSET)); // x5 = visible parameter count
    emitter.instruction(&format!("ldr x6, [x4, #{}]", CALLABLE_SIG_VARIADIC_INDEX_OFFSET)); // x6 = source variadic index or all-ones
    emitter.instruction("cbz x2, __rt_callable_param_is_by_ref_positional");    // no name: select by position
    emitter.instruction(&format!("ldr x7, [x4, #{}]", CALLABLE_SIG_PARAM_NAMES_OFFSET)); // x7 = (pointer, length) name table
    emitter.instruction("mov x8, #0");                                          // x8 = candidate parameter index

    emitter.label("__rt_callable_param_is_by_ref_loop");
    emitter.instruction("cmp x8, x5");                                          // every visible name tried?
    emitter.instruction("b.hs __rt_callable_param_is_by_ref_notfound");         // an unknown name binds to the variadic
    emitter.instruction("add x9, x7, x8, lsl #4");                              // x9 = this parameter's name entry
    emitter.instruction("ldr x10, [x9, #8]");                                   // x10 = its name length
    emitter.instruction("cmp x10, x2");                                         // same length as the requested name?
    emitter.instruction("b.ne __rt_callable_param_is_by_ref_next");             // different length cannot match
    emitter.instruction("ldr x11, [x9]");                                       // x11 = its name pointer
    emitter.instruction("mov x12, #0");                                         // x12 = byte index

    emitter.label("__rt_callable_param_is_by_ref_bytes");
    emitter.instruction("cmp x12, x2");                                         // whole name compared?
    emitter.instruction("b.hs __rt_callable_param_is_by_ref_select");           // equal: x8 is the parameter
    emitter.instruction("ldrb w13, [x11, x12]");                                // declared name byte
    emitter.instruction("ldrb w14, [x1, x12]");                                 // requested name byte
    emitter.instruction("cmp w13, w14");                                        // identical?
    emitter.instruction("b.ne __rt_callable_param_is_by_ref_next");             // mismatch: try the next parameter
    emitter.instruction("add x12, x12, #1");                                    // next byte
    emitter.instruction("b __rt_callable_param_is_by_ref_bytes");               // keep comparing

    emitter.label("__rt_callable_param_is_by_ref_next");
    emitter.instruction("add x8, x8, #1");                                      // next parameter
    emitter.instruction("b __rt_callable_param_is_by_ref_loop");                // keep searching

    emitter.label("__rt_callable_param_is_by_ref_notfound");
    emitter.instruction("mov x8, x6");                                          // the source variadic, if any, takes the unknown name
    emitter.instruction("b __rt_callable_param_is_by_ref_select");              // an all-ones index falls out as false

    emitter.label("__rt_callable_param_is_by_ref_positional");
    emitter.instruction("cmp x6, x3");                                          // is the variadic before the position?
    emitter.instruction("csel x8, x6, x3, lo");                                 // the variadic absorbs every later position

    emitter.label("__rt_callable_param_is_by_ref_select");
    emitter.instruction("cmp x8, x5");                                          // outside the visible parameters?
    emitter.instruction("b.hs __rt_callable_param_is_by_ref_no");               // by value
    emitter.instruction(&format!("ldr x7, [x4, #{}]", CALLABLE_SIG_REF_FLAGS_OFFSET)); // x7 = by-reference flag table
    emitter.instruction("cbz x7, __rt_callable_param_is_by_ref_no");            // no table: by value
    emitter.instruction("ldr x0, [x7, x8, lsl #3]");                            // the parameter's by-reference word
    emitter.instruction("ret");                                                 // return 0 or 1

    emitter.label("__rt_callable_param_is_by_ref_no");
    emitter.instruction("mov x0, #0");                                          // by value
    emitter.instruction("ret");                                                 // return false
}

/// Emits the x86_64 variant of `__rt_callable_param_is_by_ref`.
fn emit_callable_param_is_by_ref_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: callable parameter passing mode ---");
    emitter.label_global("__rt_callable_param_is_by_ref");

    emitter.instruction("test rdi, rdi");                                       // a null descriptor has no parameters
    emitter.instruction("jz __rt_callable_param_is_by_ref_no");                 // by value
    emitter.instruction(&format!("mov r8, QWORD PTR [rdi + {}]", CALLABLE_DESC_SIGNATURE_OFFSET)); // r8 = signature side record
    emitter.instruction("test r8, r8");                                         // does the descriptor carry a signature?
    emitter.instruction("jz __rt_callable_param_is_by_ref_no");                 // no signature: by value
    emitter.instruction(&format!("mov r10, QWORD PTR [r8 + {}]", CALLABLE_SIG_VARIADIC_INDEX_OFFSET)); // r10 = source variadic index or all-ones
    emitter.instruction("test rdx, rdx");                                       // is a name requested?
    emitter.instruction("jz __rt_callable_param_is_by_ref_positional");         // no name: select by position
    emitter.instruction(&format!("mov r11, QWORD PTR [r8 + {}]", CALLABLE_SIG_PARAM_NAMES_OFFSET)); // r11 = (pointer, length) name table
    emitter.instruction("xor ecx, ecx");                                        // rcx = candidate parameter index

    emitter.label("__rt_callable_param_is_by_ref_loop");
    emitter.instruction(&format!("cmp rcx, QWORD PTR [r8 + {}]", CALLABLE_SIG_VISIBLE_COUNT_OFFSET)); // every visible name tried?
    emitter.instruction("jae __rt_callable_param_is_by_ref_notfound");          // an unknown name binds to the variadic
    emitter.instruction("mov rax, rcx");                                        // rax = index
    emitter.instruction("shl rax, 4");                                          // two words per name entry
    emitter.instruction("add rax, r11");                                        // rax = this parameter's name entry
    emitter.instruction("cmp QWORD PTR [rax + 8], rdx");                        // same length as the requested name?
    emitter.instruction("jne __rt_callable_param_is_by_ref_next");              // different length cannot match
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // rax = its name pointer
    emitter.instruction("xor edi, edi");                                        // rdi = byte index

    emitter.label("__rt_callable_param_is_by_ref_bytes");
    emitter.instruction("cmp rdi, rdx");                                        // whole name compared?
    emitter.instruction("jae __rt_callable_param_is_by_ref_select");            // equal: rcx is the parameter
    emitter.instruction("movzx r9d, BYTE PTR [rax + rdi]");                     // declared name byte
    emitter.instruction("cmp r9b, BYTE PTR [rsi + rdi]");                       // identical to the requested byte?
    emitter.instruction("jne __rt_callable_param_is_by_ref_next");              // mismatch: try the next parameter
    emitter.instruction("add rdi, 1");                                          // next byte
    emitter.instruction("jmp __rt_callable_param_is_by_ref_bytes");             // keep comparing

    emitter.label("__rt_callable_param_is_by_ref_next");
    emitter.instruction("add rcx, 1");                                          // next parameter
    emitter.instruction("jmp __rt_callable_param_is_by_ref_loop");              // keep searching

    emitter.label("__rt_callable_param_is_by_ref_notfound");
    emitter.instruction("mov rcx, r10");                                        // the source variadic, if any, takes the unknown name
    emitter.instruction("jmp __rt_callable_param_is_by_ref_select");            // an all-ones index falls out as false

    emitter.label("__rt_callable_param_is_by_ref_positional");
    emitter.instruction("cmp r10, rcx");                                        // is the variadic before the position?
    emitter.instruction("jae __rt_callable_param_is_by_ref_select");            // no: keep the position
    emitter.instruction("mov rcx, r10");                                        // the variadic absorbs every later position

    emitter.label("__rt_callable_param_is_by_ref_select");
    emitter.instruction(&format!("cmp rcx, QWORD PTR [r8 + {}]", CALLABLE_SIG_VISIBLE_COUNT_OFFSET)); // outside the visible parameters?
    emitter.instruction("jae __rt_callable_param_is_by_ref_no");                // by value
    emitter.instruction(&format!("mov r11, QWORD PTR [r8 + {}]", CALLABLE_SIG_REF_FLAGS_OFFSET)); // r11 = by-reference flag table
    emitter.instruction("test r11, r11");                                       // no table means by value
    emitter.instruction("jz __rt_callable_param_is_by_ref_no");                 // by value
    emitter.instruction("mov rax, QWORD PTR [r11 + rcx * 8]");                  // the parameter's by-reference word
    emitter.instruction("ret");                                                 // return 0 or 1

    emitter.label("__rt_callable_param_is_by_ref_no");
    emitter.instruction("xor eax, eax");                                        // by value
    emitter.instruction("ret");                                                 // return false
}
