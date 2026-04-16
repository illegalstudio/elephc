use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::types::PhpType;

/// Coerce a value to string (x1=ptr, x2=len) for concatenation.
/// PHP behavior: false -> "", true -> "1", null -> "", int -> itoa
pub(super) fn coerce_to_string(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    ty: &PhpType,
) {
    match ty {
        PhpType::Int => {
            // -- convert integer in x0 to string in x1/x2 --
            abi::emit_call_label(emitter, "__rt_itoa");                         // runtime: integer-to-ASCII string conversion
        }
        PhpType::Float => {
            // -- convert float in d0 to string in x1/x2 --
            abi::emit_call_label(emitter, "__rt_ftoa");                         // runtime: float-to-ASCII string conversion
        }
        PhpType::Bool => {
            // true -> "1" (via itoa), false -> "" (len=0)
            // -- convert bool to string: true="1", false="" --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("cbz x0, 1f");                          // if false (zero), skip to empty string path
                    abi::emit_call_label(emitter, "__rt_itoa");                 // convert true (1) to string "1"
                    emitter.instruction("b 2f");                                // skip over the empty-string fallback
                    emitter.raw("1:");
                    emitter.instruction("mov x2, #0");                          // false produces empty string (length = 0)
                    emitter.raw("2:");
                }
                Arch::X86_64 => {
                    let false_label = ctx.next_label("bool_to_str_false");
                    let done_label = ctx.next_label("bool_to_str_done");
                    emitter.instruction("test rax, rax");                       // test whether the boolean payload is false
                    emitter.instruction(&format!("je {}", false_label));        // skip to the empty-string path when the boolean is false
                    abi::emit_call_label(emitter, "__rt_itoa");                 // convert true (1) to string "1"
                    emitter.instruction(&format!("jmp {}", done_label));        // skip over the empty-string fallback after conversion
                    emitter.label(&false_label);
                    emitter.instruction("mov rdx, 0");                          // false produces empty string (length = 0)
                    emitter.label(&done_label);
                }
            }
        }
        PhpType::Void => {
            // -- null coerces to empty string in PHP --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x2, #0");                          // null produces empty string (length = 0)
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rdx, 0");                          // null produces empty string (length = 0)
                }
            }
        }
        PhpType::Mixed | PhpType::Union(_) => {
            // -- mixed strings dispatch on the boxed payload at runtime --
            abi::emit_call_label(emitter, "__rt_mixed_cast_string");            // cast the boxed mixed payload to string in the ABI string result registers
        }
        PhpType::Object(class_name) => {
            if ctx
                .classes
                .get(class_name)
                .is_some_and(|class_info| class_info.methods.contains_key("__toString"))
            {
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));      // push $this pointer for __toString dispatch using the active target ABI
                super::objects::emit_method_call_with_pushed_args(
                    class_name,
                    "__toString",
                    &[],
                    emitter,
                    ctx,
                );
            } else {
                emit_missing_tostring_fatal(emitter, data, class_name);
            }
        }
        PhpType::Str
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Callable
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {}
    }
}

fn emit_missing_tostring_fatal(emitter: &mut Emitter, data: &mut DataSection, class_name: &str) {
    let message = format!(
        "Fatal error: Object of class {} could not be converted to string\n",
        class_name
    );
    let (label, len) = data.add_string(message.as_bytes());
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // fd = stderr for fatal conversion diagnostics
            emitter.adrp("x1", &label);                                          // load the page that contains the fatal conversion message
            emitter.add_lo12("x1", "x1", &label);                               // resolve the fatal conversion message address within that page
            emitter.instruction(&format!("mov x2, #{}", len));                  // pass the fatal conversion message length to write()
            emitter.syscall(4);
            emitter.instruction("mov x0, #1");                                  // exit status 1 indicates abnormal termination
            emitter.syscall(1);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rsi", &label);                   // point the Linux write() buffer register at the fatal conversion message
            emitter.instruction(&format!("mov edx, {}", len));                  // pass the fatal conversion message length to write()
            emitter.instruction("mov edi, 2");                                  // fd = stderr for fatal conversion diagnostics
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal conversion message before terminating
            emitter.instruction("mov edi, 1");                                  // exit status 1 indicates abnormal termination
            emitter.instruction("mov eax, 60");                                 // Linux x86_64 syscall 60 = exit
            emitter.instruction("syscall");                                     // terminate the process after reporting the failed string conversion
        }
    }
}

/// Replace null sentinel with 0 in x0 (for arithmetic/comparison with null).
/// Handles both compile-time null (Void type) and runtime null (variable
/// that was assigned null - sentinel value in x0).
pub(super) fn coerce_null_to_zero(emitter: &mut Emitter, ty: &PhpType) {
    if *ty == PhpType::Void {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #0");                              // null is zero in arithmetic/comparison context
            }
            Arch::X86_64 => {
                emitter.instruction("mov rax, 0");                              // null is zero in arithmetic/comparison context
            }
        }
    } else if *ty == PhpType::Bool {
        // Bool is already 0/1 in x0, compatible with Int arithmetic
    } else if *ty == PhpType::Float {
        // Float is already in d0, no null sentinel to check
    } else if *ty == PhpType::Int {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("movz x9, #0xFFFE");                        // build null sentinel in x9: bits 0-15
                emitter.instruction("movk x9, #0xFFFF, lsl #16");               // null sentinel bits 16-31
                emitter.instruction("movk x9, #0xFFFF, lsl #32");               // null sentinel bits 32-47
                emitter.instruction("movk x9, #0x7FFF, lsl #48");               // null sentinel bits 48-63, completing value
                emitter.instruction("cmp x0, x9");                              // compare value against null sentinel
                emitter.instruction("csel x0, xzr, x0, eq");                    // if x0 == sentinel, replace with zero
            }
            Arch::X86_64 => {
                let sentinel_reg = abi::temp_int_reg(emitter.target);
                let zero_reg = abi::symbol_scratch_reg(emitter);
                emitter.instruction(&format!("mov {}, 9223372036854775806", sentinel_reg)); // materialize the runtime null sentinel in a scratch register
                emitter.instruction(&format!("xor {}, {}", zero_reg, zero_reg)); // materialize an integer zero in a second scratch register
                emitter.instruction(&format!("cmp {}, {}", abi::int_result_reg(emitter), sentinel_reg)); // compare the current integer result against the runtime null sentinel
                emitter.instruction(&format!("cmove {}, {}", abi::int_result_reg(emitter), zero_reg)); // replace the sentinel with zero while leaving ordinary integers unchanged
            }
        }
    }
}

/// Coerce any type to a truthiness value in x0 for use in conditions
/// (if, while, for, ternary, &&, ||). For strings, PHP treats both ""
/// and "0" as falsy. For other types, x0 already holds the truthiness.
pub(super) fn coerce_to_truthiness(emitter: &mut Emitter, ctx: &mut Context, ty: &PhpType) {
    coerce_null_to_zero(emitter, ty);
    if *ty == PhpType::Str {
        // -- PHP string truthiness: "" and "0" are falsy, everything else truthy --
        let falsy_label = ctx.next_label("str_falsy");
        let truthy_label = ctx.next_label("str_truthy");
        let done_label = ctx.next_label("str_truth_done");
        let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cbz {}, {falsy_label}", len_reg)); // empty string is falsy
                emitter.instruction(&format!("cmp {}, #1", len_reg));           // check if length is 1
                emitter.instruction(&format!("b.ne {truthy_label}"));           // length != 1 means truthy
                emitter.instruction(&format!("ldrb w9, [{}]", ptr_reg));        // load first byte of string
                emitter.instruction("cmp w9, #48");                             // compare with ASCII '0'
                emitter.instruction(&format!("b.eq {falsy_label}"));            // string "0" is falsy
                emitter.label(&truthy_label);
                emitter.instruction(&format!("mov {}, #1", abi::int_result_reg(emitter))); // truthy: set result = 1
                emitter.instruction(&format!("b {done_label}"));                // skip falsy path
                emitter.label(&falsy_label);
                emitter.instruction(&format!("mov {}, #0", abi::int_result_reg(emitter))); // falsy: set result = 0
                emitter.label(&done_label);
            }
            Arch::X86_64 => {
                let scratch = abi::temp_int_reg(emitter.target);
                emitter.instruction(&format!("test {}, {}", len_reg, len_reg)); // empty string is falsy
                emitter.instruction(&format!("je {}", falsy_label));            // branch to falsy path when the string length is zero
                emitter.instruction(&format!("cmp {}, 1", len_reg));            // check if length is 1
                emitter.instruction(&format!("jne {}", truthy_label));          // any other non-empty length is truthy
                emitter.instruction(&format!("movzx {}d, BYTE PTR [{}]", scratch, ptr_reg)); // load the first byte of the one-character string
                emitter.instruction(&format!("cmp {}d, 48", scratch));          // compare against ASCII '0'
                emitter.instruction(&format!("je {}", falsy_label));            // the string \"0\" is falsy in PHP
                emitter.label(&truthy_label);
                emitter.instruction(&format!("mov {}, 1", abi::int_result_reg(emitter))); // truthy: set result = 1
                emitter.instruction(&format!("jmp {}", done_label));            // skip the falsy path once the result is known
                emitter.label(&falsy_label);
                emitter.instruction(&format!("mov {}, 0", abi::int_result_reg(emitter))); // falsy: set result = 0
                emitter.label(&done_label);
            }
        }
    } else if *ty == PhpType::Float {
        // -- float truthiness: 0.0 is falsy --
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("fcmp d0, #0.0");                           // compare float against zero
                emitter.instruction("cset x0, ne");                             // x0=1 if nonzero (truthy), 0 if zero
            }
            Arch::X86_64 => {
                let bits_reg = abi::temp_int_reg(emitter.target);
                emitter.instruction(&format!("movq {}, xmm0", bits_reg));       // move the current float bits into a scratch integer register
                emitter.instruction(&format!("shl {}, 1", bits_reg));           // discard the sign bit so +0.0 and -0.0 both normalize to zero
                emitter.instruction(&format!("cmp {}, 0", bits_reg));           // compare the signless float bits against zero
                emitter.instruction("setne al");                                // set al when the float payload is non-zero
                emitter.instruction("movzx rax, al");                           // widen the boolean byte into the full integer result register
            }
        }
    } else if matches!(
        ty,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Void
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Buffer(_)
            | PhpType::Packed(_)
            | PhpType::Pointer(_)
    ) {
        // -- scalars and pointer-like values are truthy when non-zero --
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #0");                              // compare the normalized scalar/pointer value against zero
                emitter.instruction("cset x0, ne");                             // produce 1 when the scalar/pointer value is non-zero, else 0
            }
            Arch::X86_64 => {
                let result_reg = abi::int_result_reg(emitter);
                emitter.instruction(&format!("test {}, {}", result_reg, result_reg)); // compare the normalized scalar/pointer value against zero
                emitter.instruction("setne al");                                // produce a boolean byte when the scalar/pointer value is non-zero
                emitter.instruction("movzx rax, al");                           // widen the boolean byte into the canonical integer result register
            }
        }
    } else if matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        // -- arrays are truthy when their runtime length is non-zero --
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x0, [x0]");                            // load the runtime array length from the header
                emitter.instruction("cmp x0, #0");                              // compare the array length against zero
                emitter.instruction("cset x0, ne");                             // produce 1 for non-empty arrays, else 0
            }
            Arch::X86_64 => {
                emitter.instruction("mov rax, QWORD PTR [rax]");                // load the runtime array length from the header
                emitter.instruction("test rax, rax");                           // compare the array length against zero
                emitter.instruction("setne al");                                // produce a boolean byte when the array is non-empty
                emitter.instruction("movzx rax, al");                           // widen the boolean byte into the canonical integer result register
            }
        }
    } else if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        // -- mixed/union truthiness dispatches on the boxed payload at runtime --
        abi::emit_call_label(emitter, "__rt_mixed_cast_bool");                  // normalize the boxed mixed payload to PHP truthiness
    }
}
