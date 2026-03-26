use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::{BinOp, Expr, ExprKind, StmtKind};
use crate::types::PhpType;

/// Infer whether a callback returns a string type from its AST.
fn callback_returns_str(args: &[Expr], ctx: &Context) -> bool {
    match &args[0].kind {
        ExprKind::Closure { body, .. } => {
            // Arrow functions produce [Return(Some(expr))]; check the return expression
            for stmt in body {
                if let StmtKind::Return(Some(expr)) = &stmt.kind {
                    return expr_is_str(expr);
                }
            }
            false
        }
        ExprKind::StringLiteral(name) => {
            // Named function — check its registered return type
            if let Some(sig) = ctx.functions.get(name) {
                return sig.return_type == PhpType::Str;
            }
            false
        }
        _ => false,
    }
}

/// Check if an expression produces a string result.
fn expr_is_str(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_) => true,
        ExprKind::BinaryOp { op: BinOp::Concat, .. } => true,
        ExprKind::FunctionCall { name, .. } => {
            matches!(name.as_str(),
                "substr" | "strtolower" | "strtoupper" | "trim" | "ltrim" | "rtrim"
                | "str_repeat" | "strrev" | "chr" | "str_replace" | "ucfirst"
                | "lcfirst" | "ucwords" | "str_pad" | "implode" | "join"
                | "sprintf" | "str_word_count" | "nl2br" | "wordwrap"
                | "number_format" | "chunk_split" | "md5" | "sha1" | "hash"
            )
        }
        ExprKind::Cast { target: crate::parser::ast::CastType::String, .. } => true,
        _ => false,
    }
}

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_map()");

    // -- determine callback return type at compile time --
    let returns_str = callback_returns_str(args, ctx);

    // -- evaluate the callback argument (may be a string literal or closure) --
    let is_closure = matches!(&args[0].kind, ExprKind::Closure { .. });
    if is_closure {
        // Evaluate closure → x0 = function address
        emit_expr(&args[0], emitter, ctx, data);
        emitter.instruction("str x0, [sp, #-16]!");                             // save callback address on stack
    }

    // -- evaluate the array argument --
    let _arr_ty = emit_expr(&args[1], emitter, ctx, data);

    // -- save array pointer, load callback address into x19 --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack

    if is_closure {
        // -- load callback address from saved stack slot --
        emitter.instruction("ldr x19, [sp, #16]");                              // peek callback address (saved before array)
    } else if let ExprKind::Variable(var_name) = &args[0].kind {
        // Callable variable — load from stack slot
        let var = ctx.variables.get(var_name).expect("undefined callback variable");
        let offset = var.stack_offset;
        abi::load_at_offset(emitter, "x19", offset);                              // load callback address from variable
    } else {
        // String literal — resolve at compile time
        let func_name = match &args[0].kind {
            ExprKind::StringLiteral(name) => name.clone(),
            _ => panic!("array_map() callback must be a string literal, closure, or callable variable"),
        };
        let label = format!("_fn_{}", func_name);
        emitter.instruction(&format!("adrp x19, {}@PAGE", label));              // load page address of callback function
        emitter.instruction(&format!("add x19, x19, {}@PAGEOFF", label));       // resolve full address of callback function
    }

    // -- call runtime: x0=callback_addr, x1=array_ptr --
    emitter.instruction("mov x0, x19");                                         // x0 = callback function address
    emitter.instruction("ldr x1, [sp], #16");                                   // pop array pointer into x1
    if is_closure {
        emitter.instruction("add sp, sp, #16");                                 // discard saved callback address
    }

    if returns_str {
        emitter.instruction("bl __rt_array_map_str");                           // call runtime: map callback over array → x0=new string array
        Some(PhpType::Array(Box::new(PhpType::Str)))
    } else {
        emitter.instruction("bl __rt_array_map");                               // call runtime: map callback over array → x0=new array
        Some(PhpType::Array(Box::new(PhpType::Int)))
    }
}
