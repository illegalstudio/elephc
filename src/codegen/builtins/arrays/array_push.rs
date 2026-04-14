use super::store_mutating_arg::emit_store_mutating_arg;
use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_push()");
    let _arr_ty = emit_expr(&args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        emit_array_push_linux_x86_64(args, emitter, ctx, data);
        return Some(PhpType::Void);
    }

    // -- save array pointer, evaluate value to push --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack
    let val_ty = emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("ldr x9, [sp], #16");                                   // pop saved array pointer into x9
    match &val_ty {
        PhpType::Int | PhpType::Bool => {
            // -- push integer/bool value onto array --
            emitter.instruction("mov x1, x0");                                  // move integer value to x1 (second arg)
            emitter.instruction("mov x0, x9");                                  // move array pointer to x0 (first arg)
            emitter.instruction("bl __rt_array_push_int");                      // call runtime: append integer to array
        }
        PhpType::Float => {
            // -- push float value onto array (store as 8-byte int via bit cast) --
            emitter.instruction("fmov x1, d0");                                 // move float bits to integer register
            emitter.instruction("mov x0, x9");                                  // move array pointer to x0 (first arg)
            emitter.instruction("bl __rt_array_push_int");                      // call runtime: append float bits as 8 bytes
        }
        PhpType::Str => {
            // -- push string to array (push_str persists to heap internally) --
            emitter.instruction("mov x0, x9");                                  // move array pointer to x0
            emitter.instruction("bl __rt_array_push_str");                      // call runtime: persist + append string to array
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            // -- push nested refcounted pointer onto array --
            emitter.instruction("mov x1, x0");                                  // move pointer value to x1
            emitter.instruction("mov x0, x9");                                  // move outer array pointer to x0
            emitter.instruction("bl __rt_array_push_refcounted");               // append retained pointer and stamp array metadata
        }
        PhpType::Callable => {
            // -- push callable pointer onto array as a plain 8-byte scalar --
            emitter.instruction("mov x1, x0");                                  // move callable pointer value to x1
            emitter.instruction("mov x0, x9");                                  // move outer array pointer to x0
            emitter.instruction("bl __rt_array_push_int");                      // append function pointer bits as a plain scalar slot
        }
        _ => {}
    }

    // -- update stored array pointer (may have changed due to COW splitting or reallocation) --
    emit_store_mutating_arg(emitter, ctx, &args[0]);

    Some(PhpType::Void)
}

fn emit_array_push_linux_x86_64(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    abi::emit_push_reg(emitter, "rax");                                          // preserve the indexed-array pointer while evaluating the appended value
    let val_ty = emit_expr(&args[1], emitter, ctx, data);
    abi::emit_pop_reg(emitter, "r11");                                           // restore the indexed-array pointer after evaluating the appended value
    match &val_ty {
        PhpType::Int | PhpType::Bool => {
            emitter.instruction("mov rsi, rax");                                 // place the appended scalar payload in the x86_64 runtime value register
            emitter.instruction("mov rdi, r11");                                 // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_int");                // append the scalar payload and return the possibly-grown indexed-array pointer
        }
        PhpType::Float => {
            emitter.instruction("movq rsi, xmm0");                               // move the floating-point payload bits into the scalar append register
            emitter.instruction("mov rdi, r11");                                 // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_int");                // append the floating-point payload bits as an 8-byte scalar slot
        }
        PhpType::Str => {
            emitter.instruction("mov rsi, rax");                                 // place the appended string pointer in the x86_64 runtime payload register
            emitter.instruction("mov rdi, r11");                                 // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_str");                // persist and append the string payload, returning the possibly-grown indexed-array pointer
        }
        PhpType::Callable => {
            emitter.instruction("mov rsi, rax");                                 // place the callable pointer bits in the x86_64 scalar append register
            emitter.instruction("mov rdi, r11");                                 // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_int");                // append the callable pointer bits as a plain scalar slot
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            emitter.instruction("mov rsi, rax");                                 // place the retained refcounted payload pointer in the x86_64 runtime child register
            emitter.instruction("mov rdi, r11");                                 // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_refcounted");         // append the retained heap payload and stamp the indexed-array value_type metadata
        }
        _ => {}
    }

    emit_store_mutating_arg(emitter, ctx, &args[0]);                             // publish the possibly-grown indexed-array pointer back through the mutating argument slot
}
