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
    emitter.comment("is_iterable()");
    let ty = emit_expr(&args[0], emitter, ctx, data);

    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        // Mixed/Union values are boxed cells. Unwrap to the concrete runtime tag and
        // report true only when the unboxed payload is an indexed array or hash.
        // Objects implementing Traversable are not yet modelled in elephc, so they
        // currently report false here even though PHP would say true.
        let true_case = ctx.next_label("builtin_is_iterable_true");
        let done = ctx.next_label("builtin_is_iterable_done");

        abi::emit_call_label(emitter, "__rt_mixed_unbox");                      // resolve the boxed mixed payload tag for the iterable predicate
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #4");                              // runtime tag 4 = indexed array
                emitter.instruction(&format!("b.eq {}", true_case));            // indexed arrays satisfy is_iterable
                emitter.instruction("cmp x0, #5");                              // runtime tag 5 = associative hash
                emitter.instruction(&format!("b.eq {}", true_case));            // hash tables satisfy is_iterable
                emitter.instruction("mov x0, #0");                              // every other concrete payload reports false
                emitter.instruction(&format!("b {}", done));                    // skip the truthy assignment
            }
            Arch::X86_64 => {
                emitter.instruction("cmp rax, 4");                              // runtime tag 4 = indexed array
                emitter.instruction(&format!("je {}", true_case));              // indexed arrays satisfy is_iterable
                emitter.instruction("cmp rax, 5");                              // runtime tag 5 = associative hash
                emitter.instruction(&format!("je {}", true_case));              // hash tables satisfy is_iterable
                emitter.instruction("mov rax, 0");                              // every other concrete payload reports false
                emitter.instruction(&format!("jmp {}", done));                  // skip the truthy assignment
            }
        }

        emitter.label(&true_case);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #1");                              // record the truthy is_iterable result on AArch64
            }
            Arch::X86_64 => {
                emitter.instruction("mov rax, 1");                              // record the truthy is_iterable result on x86_64
            }
        }
        emitter.label(&done);
        return Some(PhpType::Bool);
    }

    let val = matches!(
        ty,
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable
    );
    abi::emit_load_int_immediate(
        emitter,
        abi::int_result_reg(emitter),
        if val { 1 } else { 0 },
    );                                                                          // record the compile-time is_iterable predicate result
    Some(PhpType::Bool)
}
