use crate::codegen::platform::Arch;

use super::super::abi;
use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::{Expr, PhpType};

const NULL_SENTINEL: i64 = 0x7fff_ffff_ffff_fffe;

pub(super) fn emit_bool_literal(b: bool, emitter: &mut Emitter) -> PhpType {
    emitter.comment(&format!("bool {}", b));
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), if b { 1 } else { 0 });
    PhpType::Bool
}

pub(super) fn emit_null_literal(emitter: &mut Emitter) -> PhpType {
    emitter.comment("null");
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), NULL_SENTINEL);
    PhpType::Void
}

pub(super) fn emit_string_literal(
    value: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) -> PhpType {
    let (label, len) = data.add_string(value.as_bytes());
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    emitter.comment(&format!("load string \"{}\"", value.escape_default()));
    abi::emit_symbol_address(emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(emitter, len_reg, len as i64);
    PhpType::Str
}

pub(super) fn emit_int_literal(value: i64, emitter: &mut Emitter) -> PhpType {
    emitter.comment(&format!("load int {}", value));
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), value);
    PhpType::Int
}

pub(super) fn emit_float_literal(
    value: f64,
    emitter: &mut Emitter,
    data: &mut DataSection,
) -> PhpType {
    let label = data.add_float(value);
    let scratch = abi::symbol_scratch_reg(emitter);
    emitter.comment(&format!("load float {}", value));
    abi::emit_symbol_address(emitter, scratch, &label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {}, [{}]", abi::float_result_reg(emitter), scratch)); // load the 64-bit float literal through the symbol scratch register
        }
        Arch::X86_64 => {
            emitter.instruction(&format!(
                "movsd {}, QWORD PTR [{}]",
                abi::float_result_reg(emitter),
                scratch
            ));                                                                 // load the 64-bit float literal through the symbol scratch register
        }
    }
    PhpType::Float
}

pub(super) fn emit_negate(
    inner: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let ty = super::emit_expr(inner, emitter, ctx, data);
    emitter.comment("negate");
    if ty == PhpType::Float {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!(
                    "fneg {}, {}",
                    abi::float_result_reg(emitter),
                    abi::float_result_reg(emitter)
                ));                                                             // flip the sign bit of the current floating-point result
            }
            Arch::X86_64 => {
                emitter.instruction("xorpd xmm15, xmm15");                      // materialize +0.0 in a scratch xmm register before subtracting the value
                emitter.instruction(&format!(
                    "subsd xmm15, {}",
                    abi::float_result_reg(emitter)
                ));                                                             // compute 0.0 - value to negate the current floating-point result
                emitter.instruction(&format!(
                    "movsd {}, xmm15",
                    abi::float_result_reg(emitter)
                ));                                                             // move the negated floating-point result back into the ABI return register
            }
        }
        PhpType::Float
    } else {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!(
                    "neg {}, {}",
                    abi::int_result_reg(emitter),
                    abi::int_result_reg(emitter)
                ));                                                             // two's-complement negate the current integer result in place
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("neg {}", abi::int_result_reg(emitter))); // two's-complement negate the current integer result in place
            }
        }
        PhpType::Int
    }
}

pub(super) fn emit_bit_not(
    inner: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let ty = super::emit_expr(inner, emitter, ctx, data);
    super::coerce_null_to_zero(emitter, &ty);
    emitter.comment("bitwise not");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!(
                "mvn {}, {}",
                abi::int_result_reg(emitter),
                abi::int_result_reg(emitter)
            ));                                                                 // invert every bit of the current integer result in place
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("not {}", abi::int_result_reg(emitter))); // invert every bit of the current integer result in place
        }
    }
    PhpType::Int
}

pub(super) fn emit_not(
    inner: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let ty = super::emit_expr(inner, emitter, ctx, data);
    emitter.comment("logical not");
    super::coerce_to_truthiness(emitter, ctx, &ty);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // test if the coerced truthiness result is falsy
            emitter.instruction("cset x0, eq");                                 // return 1 when the coerced truthiness result was false, else 0
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 0");                                  // test if the coerced truthiness result is falsy
            emitter.instruction("sete al");                                     // write 1 to the low byte when the coerced truthiness result was false
            emitter.instruction("movzx rax, al");                               // widen the boolean low byte back into the full integer result register
        }
    }
    PhpType::Bool
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::context::Context;
    use crate::codegen::data_section::DataSection;
    use crate::codegen::platform::{Arch, Platform, Target};
    use crate::parser::ast::Expr;

    fn test_emitter_x86() -> Emitter {
        Emitter::new(Target::new(Platform::Linux, Arch::X86_64))
    }

    #[test]
    fn test_emit_scalar_literals_for_linux_x86_64_use_native_result_registers() {
        let mut emitter = test_emitter_x86();
        let mut data = DataSection::new();

        emit_bool_literal(true, &mut emitter);
        emit_null_literal(&mut emitter);
        emit_string_literal("hi", &mut emitter, &mut data);
        emit_int_literal(42, &mut emitter);
        emit_float_literal(3.5, &mut emitter, &mut data);

        let out = emitter.output();
        assert!(out.contains("    mov rax, 1\n"));
        assert!(out.contains("    mov rax, 9223372036854775806\n"));
        assert!(out.contains("    lea rax, [rip + "));
        assert!(out.contains("    mov rdx, 2\n"));
        assert!(out.contains("    mov rax, 42\n"));
        assert!(out.contains("    lea r11, [rip + "));
        assert!(out.contains("    movsd xmm0, QWORD PTR [r11]\n"));
    }

    #[test]
    fn test_emit_negate_and_bit_not_for_linux_x86_64_use_native_instructions() {
        let mut emitter = test_emitter_x86();
        let mut ctx = Context::new();
        let mut data = DataSection::new();

        emit_negate(&Expr::int_lit(7), &mut emitter, &mut ctx, &mut data);
        emit_bit_not(&Expr::int_lit(3), &mut emitter, &mut ctx, &mut data);

        let out = emitter.output();
        assert!(out.contains("    mov rax, 7\n"));
        assert!(out.contains("    neg rax\n"));
        assert!(out.contains("    mov rax, 3\n"));
        assert!(out.contains("    not rax\n"));
    }

    #[test]
    fn test_emit_not_for_linux_x86_64_uses_native_boolean_normalization() {
        let mut emitter = test_emitter_x86();
        let mut ctx = Context::new();
        let mut data = DataSection::new();

        emit_not(&Expr::int_lit(0), &mut emitter, &mut ctx, &mut data);
        emit_not(&Expr::string_lit("0"), &mut emitter, &mut ctx, &mut data);

        let out = emitter.output();
        assert!(out.contains("    cmp rax, 0\n"));
        assert!(out.contains("    sete al\n"));
        assert!(out.contains("    movzx rax, al\n"));
        assert!(out.contains("    test rdx, rdx\n"));
        assert!(out.contains("    movzx r10d, BYTE PTR [rax]\n"));
    }
}
