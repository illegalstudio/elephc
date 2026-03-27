mod abi;
mod builtins;
pub mod context;
mod data_section;
mod emit;
mod expr;
mod functions;
mod runtime;
mod stmt;

use std::collections::{HashMap, HashSet};

use crate::parser::ast::{ExprKind, Program, Stmt, StmtKind};
use crate::types::{ClassInfo, FunctionSig, PhpType, TypeEnv};
use context::Context;
use data_section::DataSection;
use emit::Emitter;

pub fn generate(
    program: &Program,
    global_env: &TypeEnv,
    functions: &HashMap<String, FunctionSig>,
    classes: &HashMap<String, ClassInfo>,
    heap_size: usize,
) -> String {
    let mut emitter = Emitter::new();
    let mut data = DataSection::new();

    // Pre-scan for compile-time constants (const declarations and define() calls)
    let global_constants = collect_constants(program);

    // Pre-scan for global variable names used in `global $var` statements across all functions
    let all_global_var_names = collect_global_var_names(program);

    // Pre-scan for static variable declarations across all functions
    let all_static_vars = collect_static_vars(program, global_env);

    // Emit user-defined functions before _main
    for (name, sig) in functions {
        let body = program
            .iter()
            .find_map(|s| match &s.kind {
                StmtKind::FunctionDecl { name: n, body, .. } if n == name => Some(body),
                _ => None,
            })
            .expect("function body not found");

        self::functions::emit_function(
            &mut emitter, &mut data, name, sig, body, functions,
            &global_constants, &all_global_var_names, &all_static_vars,
            Some(classes),
        );
    }

    // Emit class methods
    for stmt in program {
        if let StmtKind::ClassDecl { name: class_name, methods, .. } = &stmt.kind {
            for method in methods {
                let (label, sig) = if method.is_static {
                    let label = format!("_static_{}_{}", class_name, method.name);
                    // Use param types from ClassInfo sig (set by type checker)
                    let class_static_sig = classes.get(class_name)
                        .and_then(|c| c.static_methods.get(&method.name));
                    let params: Vec<(String, PhpType)> = method.params.iter().enumerate()
                        .map(|(i, (n, _, _))| {
                            let ty = class_static_sig
                                .and_then(|s| s.params.get(i))
                                .map(|(_, t)| t.clone())
                                .unwrap_or(PhpType::Int);
                            (n.clone(), ty)
                        })
                        .collect();
                    let defaults: Vec<Option<crate::parser::ast::Expr>> = method.params.iter()
                        .map(|(_, d, _)| d.clone())
                        .collect();
                    let ref_params: Vec<bool> = method.params.iter()
                        .map(|(_, _, r)| *r)
                        .collect();
                    let return_type = classes.get(class_name)
                        .and_then(|c| c.static_methods.get(&method.name))
                        .map(|s| s.return_type.clone())
                        .unwrap_or(PhpType::Int);
                    (label, FunctionSig { params, defaults, return_type, ref_params, variadic: method.variadic.clone() })
                } else {
                    let label = format!("_method_{}_{}", class_name, method.name);
                    // $this is the first parameter
                    let mut params: Vec<(String, PhpType)> = vec![
                        ("this".to_string(), PhpType::Object(class_name.clone())),
                    ];
                    // Use param types from ClassInfo sig (set by type checker post-pass)
                    let class_method_sig = classes.get(class_name)
                        .and_then(|c| c.methods.get(&method.name));
                    params.extend(method.params.iter().enumerate().map(|(i, (n, _, _))| {
                        let ty = class_method_sig
                            .and_then(|s| s.params.get(i))
                            .map(|(_, t)| t.clone())
                            .unwrap_or(PhpType::Int);
                        (n.clone(), ty)
                    }));
                    let mut defaults: Vec<Option<crate::parser::ast::Expr>> = vec![None]; // $this has no default
                    defaults.extend(method.params.iter().map(|(_, d, _)| d.clone()));
                    let mut ref_params: Vec<bool> = vec![false]; // $this is not a ref
                    ref_params.extend(method.params.iter().map(|(_, _, r)| *r));
                    let return_type = classes.get(class_name)
                        .and_then(|c| c.methods.get(&method.name))
                        .map(|s| s.return_type.clone())
                        .unwrap_or(PhpType::Int);
                    (label, FunctionSig { params, defaults, return_type, ref_params, variadic: method.variadic.clone() })
                };
                let epilogue_label = format!("{}_epilogue", label);
                self::functions::emit_method(
                    &mut emitter, &mut data, &label, &epilogue_label, &sig, &method.body,
                    functions, &global_constants, classes, class_name,
                );
            }
        }
    }

    // --- _main function ---
    let mut ctx = Context::new();
    ctx.functions = functions.clone();
    ctx.constants = global_constants.clone();
    ctx.in_main = true;
    ctx.all_global_var_names = all_global_var_names.clone();
    ctx.all_static_vars = all_static_vars.clone();
    ctx.classes = classes.clone();

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
    abi::store_at_offset(&mut emitter, "x0", argc_offset);                        // $argc = OS argc

    // -- build $argv array from OS C strings --
    let argv_offset = ctx.variables.get("argv").unwrap().stack_offset;
    emitter.comment("build $argv array from OS argv");
    emitter.instruction("bl __rt_build_argv");                                  // returns array ptr in x0
    abi::store_at_offset(&mut emitter, "x0", argv_offset);                      // $argv = array

    // -- emit user statements --
    for s in program {
        if matches!(&s.kind, StmtKind::FunctionDecl { .. } | StmtKind::ClassDecl { .. }) {
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
    let runtime_data = runtime::emit_runtime_data(&all_global_var_names, &all_static_vars, heap_size);

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
                &ctx.constants,
            );
        }
    }
}

/// Pre-scan the program for compile-time constants (const declarations and define() calls).
fn collect_constants(program: &Program) -> HashMap<String, (ExprKind, PhpType)> {
    let mut constants = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::ConstDecl { name, value } => {
                let ty = match &value.kind {
                    ExprKind::IntLiteral(_) => PhpType::Int,
                    ExprKind::FloatLiteral(_) => PhpType::Float,
                    ExprKind::StringLiteral(_) => PhpType::Str,
                    ExprKind::BoolLiteral(_) => PhpType::Bool,
                    _ => PhpType::Int,
                };
                constants.insert(name.clone(), (value.kind.clone(), ty));
            }
            StmtKind::ExprStmt(expr) => {
                if let ExprKind::FunctionCall { name, args } = &expr.kind {
                    if name == "define" && args.len() == 2 {
                        if let ExprKind::StringLiteral(const_name) = &args[0].kind {
                            let ty = match &args[1].kind {
                                ExprKind::IntLiteral(_) => PhpType::Int,
                                ExprKind::FloatLiteral(_) => PhpType::Float,
                                ExprKind::StringLiteral(_) => PhpType::Str,
                                ExprKind::BoolLiteral(_) => PhpType::Bool,
                                _ => PhpType::Int,
                            };
                            constants.insert(const_name.clone(), (args[1].kind.clone(), ty));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    constants
}

/// Pre-scan for all variable names used in `global $var` statements across all functions.
fn collect_global_var_names(program: &Program) -> HashSet<String> {
    let mut names = HashSet::new();
    for stmt in program {
        if let StmtKind::FunctionDecl { body, .. } = &stmt.kind {
            collect_global_vars_in_body(body, &mut names);
        }
    }
    names
}

fn collect_global_vars_in_body(stmts: &[Stmt], names: &mut HashSet<String>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Global { vars } => {
                for v in vars {
                    names.insert(v.clone());
                }
            }
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                collect_global_vars_in_body(then_body, names);
                for (_, body) in elseif_clauses {
                    collect_global_vars_in_body(body, names);
                }
                if let Some(body) = else_body {
                    collect_global_vars_in_body(body, names);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                collect_global_vars_in_body(body, names);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_global_vars_in_body(body, names);
                }
                if let Some(body) = default {
                    collect_global_vars_in_body(body, names);
                }
            }
            _ => {}
        }
    }
}

/// Pre-scan for all static variable declarations across all functions.
fn collect_static_vars(
    program: &Program,
    global_env: &TypeEnv,
) -> HashMap<(String, String), PhpType> {
    let mut statics = HashMap::new();
    for stmt in program {
        if let StmtKind::FunctionDecl { name, body, .. } = &stmt.kind {
            collect_static_vars_in_body(name, body, &mut statics, global_env);
        }
    }
    statics
}

fn collect_static_vars_in_body(
    func_name: &str,
    stmts: &[Stmt],
    statics: &mut HashMap<(String, String), PhpType>,
    global_env: &TypeEnv,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::StaticVar { name, init } => {
                let ty = match &init.kind {
                    ExprKind::IntLiteral(_) => PhpType::Int,
                    ExprKind::FloatLiteral(_) => PhpType::Float,
                    ExprKind::StringLiteral(_) => PhpType::Str,
                    ExprKind::BoolLiteral(_) => PhpType::Bool,
                    _ => PhpType::Int,
                };
                statics.insert((func_name.to_string(), name.clone()), ty);
            }
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                collect_static_vars_in_body(func_name, then_body, statics, global_env);
                for (_, body) in elseif_clauses {
                    collect_static_vars_in_body(func_name, body, statics, global_env);
                }
                if let Some(body) = else_body {
                    collect_static_vars_in_body(func_name, body, statics, global_env);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                collect_static_vars_in_body(func_name, body, statics, global_env);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_static_vars_in_body(func_name, body, statics, global_env);
                }
                if let Some(body) = default {
                    collect_static_vars_in_body(func_name, body, statics, global_env);
                }
            }
            _ => {}
        }
    }
    let _ = global_env;
}

fn align16(n: usize) -> usize {
    (n + 15) & !15
}
