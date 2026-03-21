pub mod context;
mod data_section;
mod emit;
mod expr;
mod runtime;
mod stmt;

use crate::parser::ast::{Expr, Program, Stmt};
use crate::types::checker::PhpType;
use context::Context;
use data_section::DataSection;
use emit::Emitter;

pub fn generate(program: &Program) -> String {
    let mut emitter = Emitter::new();
    let mut ctx = Context::new();
    let mut data = DataSection::new();

    // Pre-scan to allocate all variables
    for s in program {
        if let Stmt::Assign { name, value } = s {
            if !ctx.variables.contains_key(name) {
                let ty = infer_expr_type(value);
                ctx.alloc_var(name, ty);
            }
        }
    }

    // Stack frame size: variables + 16 (for saved fp/lr), aligned to 16
    let vars_size = ctx.stack_offset;
    let frame_size = align16(vars_size + 16);

    // Text section
    emitter.raw(".global _main");
    emitter.raw(".align 2");
    emitter.blank();

    // Entry point with stack frame
    emitter.label("_main");
    emitter.comment("prologue");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));
    emitter.instruction(&format!(
        "stp x29, x30, [sp, #{}]",
        frame_size - 16
    ));
    emitter.instruction(&format!("add x29, sp, #{}", frame_size - 16));

    // Emit statements
    for s in program {
        stmt::emit_stmt(s, &mut emitter, &mut ctx, &mut data);
    }

    // Epilogue + exit
    emitter.blank();
    emitter.comment("epilogue + exit(0)");
    emitter.instruction(&format!(
        "ldp x29, x30, [sp, #{}]",
        frame_size - 16
    ));
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));
    emitter.instruction("mov x0, #0");
    emitter.instruction("mov x16, #1");
    emitter.instruction("svc #0x80");

    // Runtime routines
    runtime::emit_runtime(&mut emitter);

    // Data section
    let data_output = data.emit();

    let mut output = emitter.output();
    if !data_output.is_empty() {
        output.push('\n');
        output.push_str(&data_output);
    }

    output
}

fn align16(n: usize) -> usize {
    (n + 15) & !15
}

fn infer_expr_type(expr: &Expr) -> PhpType {
    match expr {
        Expr::StringLiteral(_) => PhpType::Str,
        Expr::IntLiteral(_) => PhpType::Int,
        Expr::Variable(_) => PhpType::Int, // will be resolved properly by type checker
        Expr::Negate(_) => PhpType::Int,
        Expr::BinaryOp { op, .. } => match op {
            crate::parser::ast::BinOp::Concat => PhpType::Str,
            _ => PhpType::Int,
        },
    }
}
