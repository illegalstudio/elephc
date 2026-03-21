pub mod context;
mod data_section;
mod emit;
mod expr;
mod runtime;
mod stmt;

use crate::parser::ast::Program;
use context::Context;
use data_section::DataSection;
use emit::Emitter;

pub fn generate(program: &Program) -> String {
    let mut emitter = Emitter::new();
    let mut ctx = Context::new();
    let mut data = DataSection::new();

    // Text section
    emitter.raw(".global _main");
    emitter.raw(".align 2");
    emitter.blank();

    // Entry point
    emitter.label("_main");

    // Emit statements
    for s in program {
        stmt::emit_stmt(s, &mut emitter, &mut ctx, &mut data);
    }

    // exit(0)
    emitter.blank();
    emitter.comment("exit(0)");
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
