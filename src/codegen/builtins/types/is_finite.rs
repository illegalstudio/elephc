use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_finite()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if ty != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize integer inputs into the active floating-point result register before the finite check
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- check if |value| is strictly less than infinity (not NaN, not Inf) --
            emitter.instruction("fabs d0, d0");                                 // take the absolute value so both +INF and -INF compare against the same constant
            let inf_label = data.add_float(f64::INFINITY);
            emitter.adrp("x9", &inf_label);                                     // load the page that contains the infinity constant
            emitter.add_lo12("x9", "x9", &inf_label);                           // resolve the infinity constant address within that page
            emitter.instruction("ldr d1, [x9]");                                // load the infinity constant into the comparison register
            emitter.instruction("fcmp d0, d1");                                 // compare the absolute value against positive infinity
            emitter.instruction("cset x0, mi");                                 // materialize the strict-less-than-infinity result as a boolean integer
        }
        Arch::X86_64 => {
            let pos_inf_label = data.add_float(f64::INFINITY);
            let neg_inf_label = data.add_float(f64::NEG_INFINITY);
            let not_finite_label = ctx.next_label("is_finite_false");
            let done_label = ctx.next_label("is_finite_done");
            emitter.instruction("ucomisd xmm0, xmm0");                          // compare the value against itself so NaN sets the parity flag
            emitter.instruction(&format!("jp {}", not_finite_label));           // NaN is not finite
            emitter.instruction(&format!("movsd xmm1, QWORD PTR [rip + {}]", pos_inf_label)); // load the positive infinity constant into the comparison register
            emitter.instruction("ucomisd xmm0, xmm1");                          // compare the value against positive infinity
            emitter.instruction(&format!("je {}", not_finite_label));           // +INF is not finite
            emitter.instruction(&format!("movsd xmm1, QWORD PTR [rip + {}]", neg_inf_label)); // load the negative infinity constant into the comparison register
            emitter.instruction("ucomisd xmm0, xmm1");                          // compare the value against negative infinity
            emitter.instruction(&format!("je {}", not_finite_label));           // -INF is not finite
            emitter.instruction("mov rax, 1");                                  // any remaining non-NaN and non-infinite value is finite
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false materialization path after confirming finiteness
            emitter.label(&not_finite_label);
            emitter.instruction("mov rax, 0");                                  // NaN and +/-INF are not finite
            emitter.label(&done_label);
        }
    }
    Some(PhpType::Bool)
}
