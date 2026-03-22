mod abi;
mod builtins;
pub mod context;
mod data_section;
mod emit;
mod expr;
mod functions;
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

    // Emit user functions
    for (name, sig) in functions {
        let body = program
            .iter()
            .find_map(|s| match &s.kind {
                StmtKind::FunctionDecl { name: n, body, .. } if n == name => Some(body),
                _ => None,
            })
            .expect("function body not found");

        self::functions::emit_function(&mut emitter, &mut data, name, sig, body, functions);
    }

    // Emit _main
    let mut ctx = Context::new();
    ctx.functions = functions.clone();

    if !global_env.contains_key("argc") {
        ctx.alloc_var("argc", PhpType::Int);
    }
    if !global_env.contains_key("argv") {
        ctx.alloc_var("argv", PhpType::Array(Box::new(PhpType::Str)));
    }
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

    // Save argc/argv
    emitter.comment("save argc/argv");
    emitter.instruction("adrp x9, _global_argc@PAGE");
    emitter.instruction("add x9, x9, _global_argc@PAGEOFF");
    emitter.instruction("str x0, [x9]");
    emitter.instruction("adrp x9, _global_argv@PAGE");
    emitter.instruction("add x9, x9, _global_argv@PAGEOFF");
    emitter.instruction("str x1, [x9]");

    let argc_offset = ctx.variables.get("argc").unwrap().stack_offset;
    emitter.instruction(&format!("stur x0, [x29, #-{}]", argc_offset));

    // Build $argv array from OS argv
    let argv_offset = ctx.variables.get("argv").unwrap().stack_offset;
    emitter.comment("build $argv array");
    emitter.instruction("bl __rt_build_argv");
    emitter.instruction(&format!("stur x0, [x29, #-{}]", argv_offset));

    for s in program {
        if matches!(&s.kind, StmtKind::FunctionDecl { .. }) {
            continue;
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

    runtime::emit_runtime(&mut emitter);

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

fn align16(n: usize) -> usize {
    (n + 15) & !15
}
