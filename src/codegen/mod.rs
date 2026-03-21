mod abi;
pub mod context;
mod data_section;
mod emit;
mod expr;
mod runtime;
mod stmt;

use crate::parser::ast::Program;
use crate::types::TypeEnv;
use context::Context;
use data_section::DataSection;
use emit::Emitter;

pub fn generate(program: &Program, type_env: &TypeEnv) -> String {
    let mut emitter = Emitter::new();
    let mut ctx = Context::new();
    let mut data = DataSection::new();

    // Pre-allocate all variables using type info from the checker
    for (name, ty) in type_env {
        ctx.alloc_var(name, ty.clone());
    }

    // Stack frame size: variables + 16 (saved fp/lr), aligned to 16
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
