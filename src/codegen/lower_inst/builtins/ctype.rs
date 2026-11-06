//! Purpose:
//! Lowers PHP `ctype_*` character-class builtins for the EIR backend.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions`, one entry per predicate.
//!
//! Key details:
//! - PHP ctype checks operate over bytes, reject empty strings, and return scalar booleans.
//! - Every byte is looked up in `_ctype_classes`, the per-target table of
//!   `elephc_builtin_contract::ctype_class_bits`: php asks libc, and libc's answer above 0x7f
//!   depends on the platform, so a range test cannot be right everywhere.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::Instruction;
use crate::types::PhpType;
use elephc_builtin_contract::ctype_class_of;

use super::super::super::context::FunctionContext;
use super::super::{expect_operand, store_if_result};

/// Declares one lowering entry per predicate; each only names its PHP function.
macro_rules! ctype_lowerings {
    ($($lower:ident => $php:literal,)*) => {
        $(
            #[doc = concat!("Lowers `", $php, "(string)` over the byte class table.")]
            pub(crate) fn $lower(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
                lower_ctype(ctx, inst, $php)
            }
        )*
    };
}

ctype_lowerings! {
    lower_ctype_alnum => "ctype_alnum",
    lower_ctype_alpha => "ctype_alpha",
    lower_ctype_cntrl => "ctype_cntrl",
    lower_ctype_digit => "ctype_digit",
    lower_ctype_graph => "ctype_graph",
    lower_ctype_lower => "ctype_lower",
    lower_ctype_print => "ctype_print",
    lower_ctype_punct => "ctype_punct",
    lower_ctype_space => "ctype_space",
    lower_ctype_upper => "ctype_upper",
    lower_ctype_xdigit => "ctype_xdigit",
}

/// Emits the byte-scanning loop of one predicate and stores the boolean result.
fn lower_ctype(ctx: &mut FunctionContext<'_>, inst: &Instruction, name: &str) -> Result<()> {
    load_single_string_arg(ctx, inst, name)?;
    let class = ctype_class_of(name)
        .ok_or_else(|| CodegenIrError::unsupported(format!("no byte class for {name}")))?;
    let loop_label = ctx.next_label("ctype_loop");
    let fail_label = ctx.next_label("ctype_fail");
    let pass_label = ctx.next_label("ctype_pass");
    let end_label = ctx.next_label("ctype_end");
    ctx.emitter.comment(&format!("{}()", name));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x2, {}", fail_label));        // empty strings fail PHP ctype predicates
            abi::emit_symbol_address(ctx.emitter, "x5", "_ctype_classes");
            ctx.emitter.instruction("mov x3, #0");                              // start scanning at byte offset zero
            ctx.emitter.label(&loop_label);
            ctx.emitter.instruction("cmp x3, x2");                              // check whether every byte has been scanned
            ctx.emitter.instruction(&format!("b.ge {}", pass_label));           // report success once the scan reaches the end
            ctx.emitter.instruction("ldrb w4, [x1, x3]");                       // load the current byte of the PHP string
            ctx.emitter.instruction("ldrh w4, [x5, x4, lsl #1]");               // load the class bits of that byte
            ctx.emitter.instruction(&format!("tst w4, #{}", class));            // test the class of this predicate
            ctx.emitter.instruction(&format!("b.eq {}", fail_label));           // reject a byte outside the class
            ctx.emitter.instruction("add x3, x3, #1");                          // advance to the next byte
            ctx.emitter.instruction(&format!("b {}", loop_label));              // continue scanning the remaining bytes
            ctx.emitter.label(&fail_label);
            ctx.emitter.instruction("mov x0, #0");                              // materialize false
            ctx.emitter.instruction(&format!("b {}", end_label));               // skip the success result
            ctx.emitter.label(&pass_label);
            ctx.emitter.instruction("mov x0, #1");                              // materialize true when every byte matched
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rdx, rdx");                           // empty strings fail PHP ctype predicates
            ctx.emitter.instruction(&format!("je {}", fail_label));             // branch to false for an empty input
            abi::emit_symbol_address(ctx.emitter, "r9", "_ctype_classes");
            ctx.emitter.instruction("xor rcx, rcx");                            // start scanning at byte offset zero
            ctx.emitter.label(&loop_label);
            ctx.emitter.instruction("cmp rcx, rdx");                            // check whether every byte has been scanned
            ctx.emitter.instruction(&format!("jge {}", pass_label));            // report success once the scan reaches the end
            ctx.emitter.instruction("movzx r8d, BYTE PTR [rax + rcx]");         // load the current byte of the PHP string
            ctx.emitter.instruction("movzx r8d, WORD PTR [r9 + r8 * 2]");       // load the class bits of that byte
            ctx.emitter.instruction(&format!("test r8d, {}", class));           // test the class of this predicate
            ctx.emitter.instruction(&format!("je {}", fail_label));             // reject a byte outside the class
            ctx.emitter.instruction("add rcx, 1");                              // advance to the next byte
            ctx.emitter.instruction(&format!("jmp {}", loop_label));            // continue scanning the remaining bytes
            ctx.emitter.label(&fail_label);
            ctx.emitter.instruction("mov rax, 0");                              // materialize false
            ctx.emitter.instruction(&format!("jmp {}", end_label));             // skip the success result
            ctx.emitter.label(&pass_label);
            ctx.emitter.instruction("mov rax, 1");                              // materialize true when every byte matched
        }
    }
    ctx.emitter.label(&end_label);
    store_if_result(ctx, inst)
}

/// Loads the single ctype argument into the target's string result registers.
fn load_single_string_arg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 1 arg, got {}",
            name,
            inst.operands.len()
        )));
    }
    let value = expect_operand(inst, 0)?;
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Str => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}
