//! Purpose:
//! Emits PHP `number_format` string transformation or formatting calls.
//! Marshals string/scalar arguments into runtime helpers that allocate returned PHP strings.
//!
//! Called from:
//! - `crate::codegen_support::builtins::strings::emit()`.
//!
//! Key details:
//! - Returned string pointer/length pairs must be treated as owned runtime values when the helper allocates.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `number_format` builtin call.
///
/// Prepares arguments on the stack in reverse order, then pops them into ABI registers
/// and calls `__rt_number_format`. Handles all four parameters:
///
/// - `_name`: Unused name for dispatcher compatibility.
/// - `args[0]`: Numeric value as float (passed via `push_float_arg`).
/// - `args[1]`: Decimal count (default 0 when absent).
/// - `args[2]`: Decimal separator byte (default `.`, 46 ASCII).
/// - `args[3]`: Thousands separator byte (default `,`, 44 ASCII).
///
/// Emits architecture-specific assembly for x86_64 and AArch64 using stacked
/// arguments and the SysV / AArch64 calling conventions respectively.
/// Returns `Some(PhpType::Str)` as the runtime helper allocates a PHP string.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("number_format()");
    // -- prepare the numeric value as a float --
    super::args::push_float_arg(&args[0], emitter, ctx, data);

    // -- prepare decimals argument --
    if args.len() >= 2 {
        super::args::push_int_arg(&args[1], emitter, ctx, data);
    } else {
        match emitter.target.arch {
            Arch::X86_64 => {
                abi::emit_load_int_immediate(emitter, "rax", 0);                // materialize the default zero-decimal count in the active x86_64 integer result register
            }
            Arch::AArch64 => {
                abi::emit_load_int_immediate(emitter, "x0", 0);                 // materialize the default zero-decimal count in the active AArch64 integer result register
            }
        }
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the default decimal count while the separator arguments are evaluated
    }

    // -- prepare decimal point character --
    if args.len() >= 3 {
        super::args::emit_string_arg(&args[2], emitter, ctx, data);
        match emitter.target.arch {
            Arch::X86_64 => {
                emitter.instruction("movzx eax, BYTE PTR [rax]");               // load the first byte of the decimal-separator string into the x86_64 integer result register
            }
            Arch::AArch64 => {
                emitter.instruction("ldrb w0, [x1]");                           // load the first byte of the decimal-separator string into the AArch64 integer result register
            }
        }
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the decimal-separator byte while the thousands-separator argument is evaluated
    } else {
        match emitter.target.arch {
            Arch::X86_64 => {
                abi::emit_load_int_immediate(emitter, "rax", 46);               // materialize the default '.' decimal separator in the active x86_64 integer result register
            }
            Arch::AArch64 => {
                abi::emit_load_int_immediate(emitter, "x0", 46);                // materialize the default '.' decimal separator in the active AArch64 integer result register
            }
        }
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the default decimal separator while the thousands-separator argument is evaluated
    }

    // -- prepare thousands separator character --
    if args.len() >= 4 {
        super::args::emit_string_arg(&args[3], emitter, ctx, data);
        match emitter.target.arch {
            Arch::X86_64 => {
                let use_zero = ctx.next_label("nf_use_zero");
                let done = ctx.next_label("nf_sep_done");
                emitter.instruction("test rdx, rdx");                           // check whether the thousands-separator string is empty before dereferencing its first byte on x86_64
                emitter.instruction(&format!("jz {}", use_zero));               // use the no-separator sentinel when the thousands-separator string length is zero
                emitter.instruction("movzx eax, BYTE PTR [rax]");               // load the first byte of the non-empty thousands-separator string into the x86_64 integer result register
                emitter.instruction(&format!("jmp {}", done));                  // skip the empty-string fallback once the thousands-separator byte has been loaded
                emitter.label(&use_zero);
                abi::emit_load_int_immediate(emitter, "rax", 0);                // materialize the no-separator sentinel when the thousands-separator string is empty
                emitter.label(&done);
            }
            Arch::AArch64 => {
                let use_zero = ctx.next_label("nf_use_zero");
                let done = ctx.next_label("nf_sep_done");
                emitter.instruction(&format!("cbz x2, {}", use_zero));          // use the no-separator sentinel when the thousands-separator string length is zero on AArch64
                emitter.instruction("ldrb w0, [x1]");                           // load the first byte of the non-empty thousands-separator string into the AArch64 integer result register
                emitter.instruction(&format!("b {}", done));                    // skip the empty-string fallback once the thousands-separator byte has been loaded
                emitter.label(&use_zero);
                abi::emit_load_int_immediate(emitter, "x0", 0);                 // materialize the no-separator sentinel when the thousands-separator string is empty
                emitter.label(&done);
            }
        }
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the thousands-separator byte or no-separator sentinel until the runtime call is assembled
    } else {
        match emitter.target.arch {
            Arch::X86_64 => {
                abi::emit_load_int_immediate(emitter, "rax", 44);               // materialize the default ',' thousands separator in the active x86_64 integer result register
            }
            Arch::AArch64 => {
                abi::emit_load_int_immediate(emitter, "x0", 44);                // materialize the default ',' thousands separator in the active AArch64 integer result register
            }
        }
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the default thousands separator until the runtime call is assembled
    }

    // -- pop all args from stack into registers and call runtime --
    match emitter.target.arch {
        Arch::X86_64 => {
            abi::emit_pop_reg(emitter, "rdx");                                  // restore the thousands-separator byte or no-separator sentinel into the third SysV runtime argument register
            abi::emit_pop_reg(emitter, "rsi");                                  // restore the decimal-separator byte into the second SysV runtime argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the decimal-count argument into the first SysV runtime argument register
            abi::emit_pop_float_reg(emitter, "xmm0");                           // restore the floating number_format() input into the first SysV floating-point runtime argument register
        }
        Arch::AArch64 => {
            abi::emit_pop_reg(emitter, "x3");                                   // restore the thousands-separator byte or no-separator sentinel into the fourth AArch64 runtime argument register
            abi::emit_pop_reg(emitter, "x2");                                   // restore the decimal-separator byte into the third AArch64 runtime argument register
            abi::emit_pop_reg(emitter, "x1");                                   // restore the decimal-count argument into the second AArch64 runtime argument register
            abi::emit_pop_float_reg(emitter, "d0");                             // restore the floating number_format() input into the first AArch64 floating-point runtime argument register
        }
    }
    abi::emit_call_label(emitter, "__rt_number_format");                        // call the target-aware number_format() runtime helper to produce the formatted string

    Some(PhpType::Str)
}
