use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emit an extern (FFI) function call using the C ABI.
/// The C symbol is `_{name}` (macOS convention).
pub fn emit_extern_call(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let sig = ctx.extern_functions.get(name).cloned()
        .unwrap_or_else(|| panic!("codegen bug: extern function '{}' not found", name));

    emitter.comment(&format!("extern call: {}()", name));

    // -- evaluate and push arguments onto the stack --
    for (i, arg) in args.iter().enumerate().rev() {
        let param_ty = sig.params.get(i).map(|(_, t)| t.clone()).unwrap_or(PhpType::Int);
        let actual_ty = emit_expr(arg, emitter, ctx, data);

        // Convert elephc string (x1, x2) to null-terminated C string (x0)
        if param_ty == PhpType::Str && actual_ty == PhpType::Str {
            emitter.instruction("bl __rt_cstr");                                // convert (x1,x2) to null-terminated → x0
            emitter.instruction("str x0, [sp, #-16]!");                         // push C string pointer
        } else if actual_ty == PhpType::Float {
            emitter.instruction("str d0, [sp, #-16]!");                         // push float argument
        } else {
            emitter.instruction("str x0, [sp, #-16]!");                         // push integer/pointer argument
        }
    }

    // -- pop arguments into registers (C ABI: x0-x7, d0-d7) --
    let mut int_reg = 0usize;
    let mut float_reg = 0usize;
    for (i, _) in args.iter().enumerate() {
        let param_ty = sig.params.get(i).map(|(_, t)| t.clone()).unwrap_or(PhpType::Int);
        if param_ty == PhpType::Float {
            emitter.instruction(&format!("ldr d{}, [sp], #16", float_reg));     // pop float into d register
            float_reg += 1;
        } else {
            // String args were already converted to char* (single x register)
            emitter.instruction(&format!("ldr x{}, [sp], #16", int_reg));       // pop int/ptr/cstr into x register
            int_reg += 1;
        }
    }

    // -- call the C function --
    emitter.instruction(&format!("bl _{}", name));                              // call extern C function

    // -- handle return value --
    if sig.return_type == PhpType::Str {
        // C returned char* in x0 — convert to elephc string (x1, x2)
        emitter.instruction("bl __rt_cstr_to_str");                             // x0 → x1=ptr, x2=len
    }

    sig.return_type
}
