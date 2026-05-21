//! Purpose:
//! Provides shared argument materialization helpers for string builtin emitters.
//! Normalizes PHP string operands into the pointer/length register convention used by runtime helpers.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::*::emit()`.
//!
//! Key details:
//! - Helpers must preserve temporary ownership while leaving string results in the ABI registers expected by callers.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_to_string, emit_expr};
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub(super) fn emit_string_arg(
    arg: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let ty = emit_expr(arg, emitter, ctx, data);
    coerce_to_string(emitter, ctx, data, &ty);
}

pub(super) fn push_int_arg(
    arg: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    crate::codegen::expr::calls::args::push_expr_arg(
        arg,
        Some(&PhpType::Int),
        emitter,
        ctx,
        data,
    )
}

pub(super) fn emit_int_arg(
    arg: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let ty = push_int_arg(arg, emitter, ctx, data);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
    ty
}

pub(super) fn push_float_arg(
    arg: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    crate::codegen::expr::calls::args::push_expr_arg(
        arg,
        Some(&PhpType::Float),
        emitter,
        ctx,
        data,
    )
}

#[cfg(test)]
mod tests {
    use crate::codegen::context::Context;
    use crate::codegen::data_section::DataSection;
    use crate::codegen::emit::Emitter;
    use crate::codegen::platform::{Arch, Platform, Target};
    use crate::parser::ast::{Expr, ExprKind};
    use crate::span::Span;
    use crate::types::PhpType;

    use super::*;

    #[test]
    fn test_emit_string_arg_coerces_mixed_on_x86_64() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        let mut ctx = Context::new();
        let mut data = DataSection::new();
        ctx.alloc_var("value", PhpType::Mixed);
        let expr = Expr {
            kind: ExprKind::Variable("value".to_string()),
            span: Span::dummy(),
        };

        emit_string_arg(&expr, &mut emitter, &mut ctx, &mut data);

        let asm = emitter.output();
        assert!(asm.contains("mov rax, QWORD PTR [rbp - 8]\n"));
        assert!(asm.contains("call __rt_mixed_cast_string\n"));
    }
}
