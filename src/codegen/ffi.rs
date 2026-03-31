use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
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
    let sig = ctx
        .extern_functions
        .get(name)
        .cloned()
        .unwrap_or_else(|| panic!("codegen bug: extern function '{}' not found", name));

    emitter.comment(&format!("extern call: {}()", name));

    let string_arg_count = sig
        .params
        .iter()
        .take(args.len())
        .filter(|(_, ty)| *ty == PhpType::Str)
        .count();

    if string_arg_count > 0 {
        // -- preserve callee-saved registers used to track borrowed C-string temporaries --
        emitter.instruction("sub sp, sp, #64");                                 // reserve spill space for x19-x26 before argument marshaling
        emitter.instruction("stp x19, x20, [sp, #0]");                          // preserve cstr cleanup register pair 0
        emitter.instruction("stp x21, x22, [sp, #16]");                         // preserve cstr cleanup register pair 1
        emitter.instruction("stp x23, x24, [sp, #32]");                         // preserve cstr cleanup register pair 2
        emitter.instruction("stp x25, x26, [sp, #48]");                         // preserve cstr cleanup register pair 3
    }

    // -- evaluate and push arguments onto the stack --
    for (i, arg) in args.iter().enumerate().rev() {
        let param_ty = sig
            .params
            .get(i)
            .map(|(_, t)| t.clone())
            .unwrap_or(PhpType::Int);
        let actual_ty = if param_ty == PhpType::Callable {
            match &arg.kind {
                ExprKind::StringLiteral(func_name) => {
                    let label = function_symbol(func_name);
                    emitter.instruction(&format!("adrp x0, {}@PAGE", label));   // load page address of callback target
                    emitter.instruction(&format!("add x0, x0, {}@PAGEOFF", label)); //resolve callback function address
                    PhpType::Callable
                }
                _ => panic!(
                    "codegen bug: extern callable argument must be a function-name string literal"
                ),
            }
        } else {
            emit_expr(arg, emitter, ctx, data)
        };

        if param_ty == PhpType::Float && actual_ty != PhpType::Float {
            emitter.instruction("scvtf d0, x0");                                // widen integer-like value to C double
        } else if matches!(param_ty, PhpType::Pointer(_)) && actual_ty == PhpType::Void {
            emitter.instruction("mov x0, #0");                                  // PHP null becomes a null pointer for C
        }

        // Convert elephc string (x1, x2) to a dedicated null-terminated C string (x0)
        if param_ty == PhpType::Str && actual_ty == PhpType::Str {
            emitter.instruction("bl __rt_str_to_cstr");                         // allocate null-terminated copy for C ABI
            emitter.instruction("str x0, [sp, #-16]!");                         // push C string pointer
        } else if param_ty == PhpType::Float {
            emitter.instruction("str d0, [sp, #-16]!");                         // push float argument
        } else {
            emitter.instruction("str x0, [sp, #-16]!");                         // push integer/pointer argument
        }
    }

    // -- pop arguments into registers (C ABI: x0-x7, d0-d7) --
    let mut int_reg = 0usize;
    let mut float_reg = 0usize;
    let mut cleanup_reg = 19usize;
    for (i, _) in args.iter().enumerate() {
        let param_ty = sig
            .params
            .get(i)
            .map(|(_, t)| t.clone())
            .unwrap_or(PhpType::Int);
        if param_ty == PhpType::Float {
            emitter.instruction(&format!("ldr d{}, [sp], #16", float_reg));     // pop float into d register
            float_reg += 1;
        } else {
            // String args were already converted to char* (single x register)
            emitter.instruction(&format!("ldr x{}, [sp], #16", int_reg));       // pop int/ptr/cstr into x register
            if param_ty == PhpType::Str {
                emitter.instruction(&format!("mov x{}, x{}", cleanup_reg, int_reg)); //preserve borrowed cstr pointer so it can be freed after the call
                cleanup_reg += 1;
            }
            int_reg += 1;
        }
    }

    // -- call the C function --
    crate::codegen::expr::save_concat_offset_before_nested_call(emitter);
    emitter.instruction(&format!("bl _{}", name));                              // call extern C function
    emitter.instruction("ldr x10, [sp], #16");                                  // pop saved caller concat offset from stack
    emitter.instruction("adrp x9, _concat_off@PAGE");                           // load page of caller concat offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                     // resolve caller concat offset address
    emitter.instruction("str x10, [x9]");                                       // restore caller concat offset after extern call

    // -- handle return value --
    if sig.return_type == PhpType::Str {
        // C returned char* in x0 — convert to owned elephc string (x1, x2)
        emitter.instruction("bl __rt_cstr_to_str");                             // x0 → x1=ptr, x2=len
    }

    if string_arg_count > 0 {
        // -- preserve the extern return value while borrowed C-string temps are released --
        match &sig.return_type {
            PhpType::Float => {
                emitter.instruction("str d0, [sp, #-16]!");                     // preserve float return value across borrowed cstr cleanup
            }
            PhpType::Str => {
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve owned elephc string return registers across cstr cleanup
            }
            PhpType::Void => {}
            _ => {
                emitter.instruction("str x0, [sp, #-16]!");                     // preserve scalar/pointer return value across borrowed cstr cleanup
            }
        }

        // -- borrowed C-string arguments are call-scoped and freed immediately after the call --
        for reg in 19..(19 + string_arg_count) {
            emitter.instruction(&format!("mov x0, x{}", reg));                  // move borrowed temporary cstr pointer into heap_free argument register
            emitter.instruction("bl __rt_heap_free");                           // release the call-scoped C-string copy after the extern call returns
        }

        match &sig.return_type {
            PhpType::Float => {
                emitter.instruction("ldr d0, [sp], #16");                       // restore float return value after borrowed cstr cleanup
            }
            PhpType::Str => {
                emitter.instruction("ldp x1, x2, [sp], #16");                   // restore owned elephc string return registers after cstr cleanup
            }
            PhpType::Void => {}
            _ => {
                emitter.instruction("ldr x0, [sp], #16");                       // restore scalar/pointer return value after borrowed cstr cleanup
            }
        }

        // -- restore callee-saved cstr cleanup registers --
        emitter.instruction("ldp x19, x20, [sp, #0]");                          // restore preserved cstr cleanup register pair 0
        emitter.instruction("ldp x21, x22, [sp, #16]");                         // restore preserved cstr cleanup register pair 1
        emitter.instruction("ldp x23, x24, [sp, #32]");                         // restore preserved cstr cleanup register pair 2
        emitter.instruction("ldp x25, x26, [sp, #48]");                         // restore preserved cstr cleanup register pair 3
        emitter.instruction("add sp, sp, #64");                                 // release cstr cleanup register spill space
    }

    sig.return_type
}
