mod abi;
pub mod context;
mod data_section;
mod emit;
mod expr;
mod runtime;
mod stmt;

use std::collections::HashMap;

use crate::parser::ast::{Program, StmtKind};
use crate::types::{FunctionSig, PhpType, TypeEnv};
use context::Context;
use data_section::DataSection;
use emit::Emitter;

pub fn generate(
    program: &Program,
    global_env: &TypeEnv,
    functions: &HashMap<String, FunctionSig>,
) -> String {
    let mut emitter = Emitter::new();
    let mut data = DataSection::new();

    // Emit user functions first
    for (name, sig) in functions {
        // Find the function body in the AST
        let body = program
            .iter()
            .find_map(|s| match &s.kind {
                StmtKind::FunctionDecl {
                    name: n, body, ..
                } if n == name => Some(body),
                _ => None,
            })
            .expect("function body not found");

        emit_function(&mut emitter, &mut data, name, sig, body, functions);
    }

    // Emit _main with global statements
    let mut ctx = Context::new();
    ctx.functions = functions.clone();
    for (name, ty) in global_env {
        ctx.alloc_var(name, ty.clone());
    }

    let vars_size = ctx.stack_offset;
    let frame_size = align16(vars_size + 16);

    emitter.raw(".global _main");
    emitter.raw(".align 2");
    emitter.blank();
    emitter.label("_main");
    emitter.comment("prologue");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", frame_size - 16));
    emitter.instruction(&format!("add x29, sp, #{}", frame_size - 16));

    for s in program {
        if matches!(&s.kind, StmtKind::FunctionDecl { .. }) {
            continue; // already emitted
        }
        stmt::emit_stmt(s, &mut emitter, &mut ctx, &mut data);
    }

    emitter.blank();
    emitter.comment("epilogue + exit(0)");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", frame_size - 16));
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));
    emitter.instruction("mov x0, #0");
    emitter.instruction("mov x16, #1");
    emitter.instruction("svc #0x80");

    // Runtime routines
    runtime::emit_runtime(&mut emitter);

    // Data section
    let data_output = data.emit();
    let runtime_data = runtime::emit_runtime_data();

    let mut output = emitter.output();
    if !data_output.is_empty() {
        output.push('\n');
        output.push_str(&data_output);
    }
    output.push('\n');
    output.push_str(&runtime_data);

    output
}

fn emit_function(
    emitter: &mut Emitter,
    data: &mut DataSection,
    name: &str,
    sig: &FunctionSig,
    body: &[crate::parser::ast::Stmt],
    all_functions: &HashMap<String, FunctionSig>,
) {
    let label = format!("_fn_{}", name);
    let epilogue_label = format!("_fn_{}_epilogue", name);

    let mut ctx = Context::new();
    ctx.return_label = Some(epilogue_label.clone());
    ctx.functions = all_functions.clone();

    // Allocate stack slots for parameters
    for (pname, pty) in &sig.params {
        ctx.alloc_var(pname, pty.clone());
    }

    // We also need to pre-scan the body for local variable assignments
    collect_local_vars(body, &mut ctx, &sig);

    let vars_size = ctx.stack_offset;
    let frame_size = align16(vars_size + 16);

    emitter.raw(".align 2");
    emitter.label(&label);
    emitter.comment("prologue");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", frame_size - 16));
    emitter.instruction(&format!("add x29, sp, #{}", frame_size - 16));

    // Copy arguments from registers to stack slots
    let mut reg_idx = 0usize;
    for (pname, pty) in &sig.params {
        let var = ctx.variables.get(pname).unwrap();
        let offset = var.stack_offset;
        match pty {
            PhpType::Int => {
                emitter.comment(&format!("param ${} from x{}", pname, reg_idx));
                emitter.instruction(&format!("stur x{}, [x29, #-{}]", reg_idx, offset));
                reg_idx += 1;
            }
            PhpType::Str => {
                emitter.comment(&format!("param ${} from x{},x{}", pname, reg_idx, reg_idx + 1));
                emitter.instruction(&format!("stur x{}, [x29, #-{}]", reg_idx, offset));
                emitter.instruction(&format!("stur x{}, [x29, #-{}]", reg_idx + 1, offset - 8));
                reg_idx += 2;
            }
            PhpType::Void => {}
        }
    }

    // Emit body
    for s in body {
        stmt::emit_stmt(s, emitter, &mut ctx, data);
    }

    // Epilogue
    emitter.label(&epilogue_label);
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", frame_size - 16));
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));
    emitter.instruction("ret");
    emitter.blank();
}

/// Pre-scan function body for variable assignments to allocate stack slots.
fn collect_local_vars(
    stmts: &[crate::parser::ast::Stmt],
    ctx: &mut Context,
    sig: &FunctionSig,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assign { name, value } => {
                if !ctx.variables.contains_key(name) {
                    let ty = infer_local_type(value, sig);
                    ctx.alloc_var(name, ty);
                }
            }
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                collect_local_vars(then_body, ctx, sig);
                for (_, body) in elseif_clauses {
                    collect_local_vars(body, ctx, sig);
                }
                if let Some(body) = else_body {
                    collect_local_vars(body, ctx, sig);
                }
            }
            StmtKind::While { body, .. } => {
                collect_local_vars(body, ctx, sig);
            }
            StmtKind::For { init, update, body, .. } => {
                if let Some(s) = init {
                    collect_local_vars(&[*s.clone()], ctx, sig);
                }
                if let Some(s) = update {
                    collect_local_vars(&[*s.clone()], ctx, sig);
                }
                collect_local_vars(body, ctx, sig);
            }
            _ => {}
        }
    }
}

/// Simple type inference for pre-scan (without full env).
fn infer_local_type(
    expr: &crate::parser::ast::Expr,
    _sig: &FunctionSig,
) -> PhpType {
    use crate::parser::ast::ExprKind;
    match &expr.kind {
        ExprKind::StringLiteral(_) => PhpType::Str,
        ExprKind::IntLiteral(_) => PhpType::Int,
        ExprKind::Negate(_) => PhpType::Int,
        ExprKind::BinaryOp { op, .. } => match op {
            crate::parser::ast::BinOp::Concat => PhpType::Str,
            _ => PhpType::Int,
        },
        ExprKind::FunctionCall { .. } => PhpType::Int, // conservative default
        _ => PhpType::Int,
    }
}

fn align16(n: usize) -> usize {
    (n + 15) & !15
}
