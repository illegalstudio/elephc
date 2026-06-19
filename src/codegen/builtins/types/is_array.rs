//! Purpose:
//! Emits PHP `is_array` type predicate calls.
//! Inspects static or boxed runtime value representation and returns a PHP boolean.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()`.
//!
//! Key details:
//! - Boxed Mixed arrays use runtime tags 4 and 5 for indexed and associative arrays.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `is_array` type predicate.
///
/// For boxed Mixed or Union values, unboxes the runtime payload and checks the
/// indexed-array and associative-array tags. For concrete static types, folds
/// the predicate to a boolean constant.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_array()");
    let ty = emit_expr(&args[0], emitter, ctx, data);

    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        let true_case = ctx.next_label("builtin_is_array_true");
        let done = ctx.next_label("builtin_is_array_done");

        abi::emit_call_label(emitter, "__rt_mixed_unbox");
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #4");                              // runtime tag 4 = indexed array
                emitter.instruction(&format!("b.eq {}", true_case));            // indexed arrays satisfy is_array
                emitter.instruction("cmp x0, #5");                              // runtime tag 5 = associative array
                emitter.instruction(&format!("b.eq {}", true_case));            // associative arrays satisfy is_array
                emitter.instruction("mov x0, #0");                              // every other concrete payload reports false
                emitter.instruction(&format!("b {}", done));                    // skip the truthy assignment
            }
            Arch::X86_64 => {
                emitter.instruction("cmp rax, 4");                              // runtime tag 4 = indexed array
                emitter.instruction(&format!("je {}", true_case));              // indexed arrays satisfy is_array
                emitter.instruction("cmp rax, 5");                              // runtime tag 5 = associative array
                emitter.instruction(&format!("je {}", true_case));              // associative arrays satisfy is_array
                emitter.instruction("mov rax, 0");                              // every other concrete payload reports false
                emitter.instruction(&format!("jmp {}", done));                  // skip the truthy assignment
            }
        }
        emitter.label(&true_case);
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 1);
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
    );
    Some(PhpType::Bool)
}
