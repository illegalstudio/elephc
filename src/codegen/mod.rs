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

    // Emit user-defined functions before _main
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

    // --- _main function ---
    let mut ctx = Context::new();
    ctx.functions = functions.clone();

    // Pre-allocate $argc and $argv superglobals
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
    let frame_size = align16(vars_size + 16);     // 16-byte aligned, +16 for saved x29/x30

    // -- prologue: set up stack frame --
    emitter.raw(".global _main");
    emitter.raw(".align 2");
    emitter.blank();
    emitter.label("_main");
    emitter.comment("prologue");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // grow stack for locals + saved regs
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", frame_size - 16));  // save frame pointer & return address
    emitter.instruction(&format!("add x29, sp, #{}", frame_size - 16));         // set new frame pointer

    // -- save argc/argv to globals (for $argv runtime builder) --
    emitter.comment("save argc/argv to globals");
    emitter.instruction("adrp x9, _global_argc@PAGE");                          // load page of argc global
    emitter.instruction("add x9, x9, _global_argc@PAGEOFF");                    // add page offset
    emitter.instruction("str x0, [x9]");                                        // store argc (x0 from OS)
    emitter.instruction("adrp x9, _global_argv@PAGE");                          // load page of argv global
    emitter.instruction("add x9, x9, _global_argv@PAGEOFF");                    // add page offset
    emitter.instruction("str x1, [x9]");                                        // store argv pointer (x1 from OS)

    // -- store $argc in local variable --
    let argc_offset = ctx.variables.get("argc").unwrap().stack_offset;
    emitter.instruction(&format!("stur x0, [x29, #-{}]", argc_offset));         // $argc = OS argc

    // -- build $argv array from OS C strings --
    let argv_offset = ctx.variables.get("argv").unwrap().stack_offset;
    emitter.comment("build $argv array from OS argv");
    emitter.instruction("bl __rt_build_argv");                                  // returns array ptr in x0
    emitter.instruction(&format!("stur x0, [x29, #-{}]", argv_offset));         // $argv = array

    // -- emit user statements --
    for s in program {
        if matches!(&s.kind, StmtKind::FunctionDecl { .. }) {
            continue;
        }
        stmt::emit_stmt(s, &mut emitter, &mut ctx, &mut data);
    }

    // -- epilogue: restore stack and exit(0) via syscall --
    emitter.blank();
    emitter.comment("epilogue + exit(0)");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", frame_size - 16));  // restore frame pointer & return address
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate stack frame
    emitter.instruction("mov x0, #0");                                          // exit code 0
    emitter.instruction("mov x16, #1");                                         // syscall number 1 = exit
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel

    // -- emit deferred closures --
    emit_deferred_closures(&mut emitter, &mut data, &mut ctx);

    // -- emit runtime routines and data sections --
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

fn emit_deferred_closures(
    emitter: &mut Emitter,
    data: &mut DataSection,
    ctx: &mut Context,
) {
    // Drain closures — new closures can be added during emission (nested closures)
    while !ctx.deferred_closures.is_empty() {
        let closures: Vec<_> = ctx.deferred_closures.drain(..).collect();
        for closure in closures {
            self::functions::emit_closure(
                emitter,
                data,
                &closure.label,
                &closure.sig,
                &closure.body,
                &ctx.functions,
            );
        }
    }
}

fn align16(n: usize) -> usize {
    (n + 15) & !15
}
