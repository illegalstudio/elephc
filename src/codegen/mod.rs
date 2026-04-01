mod abi;
mod builtins;
pub mod context;
mod data_section;
mod emit;
mod expr;
mod ffi;
mod functions;
mod runtime;
mod stmt;

use std::collections::{HashMap, HashSet};

use crate::names::{enum_case_symbol, method_symbol, static_method_symbol};
use crate::parser::ast::{ExprKind, Program, Stmt, StmtKind};
use crate::types::{
    ClassInfo, EnumCaseValue, EnumInfo, ExternClassInfo, ExternFunctionSig, FunctionSig,
    InterfaceInfo, PackedClassInfo, PhpType, TypeEnv,
};
use context::Context;
use data_section::DataSection;
use emit::Emitter;

pub fn generate(
    program: &Program,
    global_env: &TypeEnv,
    functions: &HashMap<String, FunctionSig>,
    interfaces: &HashMap<String, InterfaceInfo>,
    classes: &HashMap<String, ClassInfo>,
    enums: &HashMap<String, EnumInfo>,
    packed_classes: &HashMap<String, PackedClassInfo>,
    extern_functions: &HashMap<String, ExternFunctionSig>,
    extern_classes: &HashMap<String, ExternClassInfo>,
    extern_globals: &HashMap<String, PhpType>,
    heap_size: usize,
    gc_stats: bool,
    heap_debug: bool,
) -> (String, String) {
    let mut emitter = Emitter::new();
    let mut data = DataSection::new();

    // Pre-scan for compile-time constants (const declarations and define() calls)
    let global_constants = collect_constants(program);

    // Pre-scan for global variable names used in `global $var` statements across all functions
    let all_global_var_names = collect_global_var_names(program);

    // Pre-scan for static variable declarations across all functions
    let all_static_vars = collect_static_vars(program, global_env);

    // Emit user-defined functions before _main (skip extern functions)
    for (name, sig) in functions {
        if extern_functions.contains_key(name) {
            continue; // extern functions have no body — they're linked from C
        }
        let body = program
            .iter()
            .find_map(|s| match &s.kind {
                StmtKind::FunctionDecl { name: n, body, .. } if n == name => Some(body),
                _ => None,
            })
            .unwrap_or_else(|| panic!("codegen bug: function '{}' declared in signatures but body not found in AST", name));

        functions::emit_function(
            &mut emitter, &mut data, name, sig, body, functions,
            &global_constants, &all_global_var_names, &all_static_vars,
            interfaces,
            Some(classes),
        );
    }

    // Emit flattened class methods in class-id order for deterministic output.
    let mut sorted_classes: Vec<(&String, &ClassInfo)> = classes.iter().collect();
    sorted_classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in sorted_classes {
        for method in &class_info.method_decls {
                if method.is_abstract {
                    continue;
                }
                let (label, sig) = if method.is_static {
                    let label = static_method_symbol(class_name, &method.name);
                    let class_static_sig = class_info.static_methods.get(&method.name);
                    let mut params: Vec<(String, PhpType)> =
                        vec![("__elephc_called_class_id".to_string(), PhpType::Int)];
                    if let Some(sig) = class_static_sig {
                        params.extend(sig.params.clone());
                    } else {
                        params.extend(method.params.iter().map(|(n, _, _, _)| (n.clone(), PhpType::Int)));
                    }
                    let mut defaults: Vec<Option<crate::parser::ast::Expr>> = vec![None];
                    if let Some(sig) = class_static_sig {
                        defaults.extend(sig.defaults.clone());
                    } else {
                        defaults.extend(method.params.iter().map(|(_, _, d, _)| d.clone()));
                        if method.variadic.is_some() {
                            defaults.push(None);
                        }
                    }
                    let mut ref_params: Vec<bool> = vec![false];
                    if let Some(sig) = class_static_sig {
                        ref_params.extend(sig.ref_params.clone());
                    } else {
                        ref_params.extend(method.params.iter().map(|(_, _, _, r)| *r));
                        if method.variadic.is_some() {
                            ref_params.push(false);
                        }
                    }
                    let mut declared_params: Vec<bool> = vec![false];
                    if let Some(sig) = class_static_sig {
                        declared_params.extend(sig.declared_params.clone());
                    } else {
                        declared_params.extend(
                            method.params.iter().map(|(_, type_ann, _, _)| type_ann.is_some()),
                        );
                        if method.variadic.is_some() {
                            declared_params.push(false);
                        }
                    }
                    let return_type = class_static_sig
                        .map(|s| s.return_type.clone())
                        .unwrap_or(PhpType::Int);
                    (
                        label,
                        FunctionSig {
                            params,
                            defaults,
                            return_type,
                            ref_params,
                            declared_params,
                            variadic: method.variadic.clone(),
                        },
                    )
                } else {
                    let label = method_symbol(class_name, &method.name);
                    let class_method_sig = class_info.methods.get(&method.name);
                    let mut params: Vec<(String, PhpType)> = vec![
                        ("this".to_string(), PhpType::Object(class_name.clone())),
                    ];
                    if let Some(sig) = class_method_sig {
                        params.extend(sig.params.clone());
                    } else {
                        params.extend(method.params.iter().map(|(n, _, _, _)| (n.clone(), PhpType::Int)));
                    }
                    let mut defaults: Vec<Option<crate::parser::ast::Expr>> = vec![None]; // $this has no default
                    if let Some(sig) = class_method_sig {
                        defaults.extend(sig.defaults.clone());
                    } else {
                        defaults.extend(method.params.iter().map(|(_, _, d, _)| d.clone()));
                        if method.variadic.is_some() {
                            defaults.push(None);
                        }
                    }
                    let mut ref_params: Vec<bool> = vec![false]; // $this is not a ref
                    if let Some(sig) = class_method_sig {
                        ref_params.extend(sig.ref_params.clone());
                    } else {
                        ref_params.extend(method.params.iter().map(|(_, _, _, r)| *r));
                        if method.variadic.is_some() {
                            ref_params.push(false);
                        }
                    }
                    let mut declared_params: Vec<bool> = vec![false]; // $this is synthetic
                    if let Some(sig) = class_method_sig {
                        declared_params.extend(sig.declared_params.clone());
                    } else {
                        declared_params.extend(
                            method.params.iter().map(|(_, type_ann, _, _)| type_ann.is_some()),
                        );
                        if method.variadic.is_some() {
                            declared_params.push(false);
                        }
                    }
                    let return_type = class_method_sig
                        .map(|s| s.return_type.clone())
                        .unwrap_or(PhpType::Int);
                    (
                        label,
                        FunctionSig {
                            params,
                            defaults,
                            return_type,
                            ref_params,
                            declared_params,
                            variadic: method.variadic.clone(),
                        },
                    )
                };
                let epilogue_label = format!("{}_epilogue", label);
                functions::emit_method(
                    &mut emitter, &mut data, &label, &epilogue_label, &sig, &method.body,
                    functions, &global_constants, interfaces, classes, class_name,
                );
            }
    }

    // --- _main function ---
    let mut ctx = Context::new();
    ctx.functions = functions.clone();
    ctx.constants = global_constants.clone();
    ctx.in_main = true;
    ctx.return_type = PhpType::Void;
    ctx.all_global_var_names = all_global_var_names.clone();
    ctx.all_static_vars = all_static_vars.clone();
    ctx.classes = classes.clone();
    ctx.interfaces = interfaces.clone();
    ctx.enums = enums.clone();
    ctx.packed_classes = packed_classes.clone();
    ctx.extern_functions = extern_functions.clone();
    ctx.extern_classes = extern_classes.clone();
    ctx.extern_globals = extern_globals.clone();

    // Pre-allocate $argc and $argv superglobals
    if !global_env.contains_key("argc") {
        ctx.alloc_var("argc", PhpType::Int);
    }
    if !global_env.contains_key("argv") {
        ctx.alloc_var("argv", PhpType::Array(Box::new(PhpType::Str)));
    }
    for (name, ty) in global_env {
        ctx.alloc_var(name, ty.codegen_repr());
    }
    collect_main_try_slots(program, &mut ctx);
    let main_cleanup_label = ctx.next_label("main_cleanup_frame");
    ctx.activation_frame_base_offset = Some(ctx.alloc_hidden_slot(8));
    ctx.activation_cleanup_offset = Some(ctx.alloc_hidden_slot(8));
    ctx.activation_prev_offset = Some(ctx.alloc_hidden_slot(8));
    ctx.pending_action_offset = Some(ctx.alloc_hidden_slot(8));
    ctx.pending_target_offset = Some(ctx.alloc_hidden_slot(8));
    ctx.pending_return_value_offset = Some(ctx.alloc_hidden_slot(16));

    let vars_size = ctx.stack_offset;
    let frame_size = align16(vars_size + 16);     // 16-byte aligned, +16 for saved x29/x30

    // -- prologue: set up stack frame --
    emitter.raw(".align 2");
    emitter.blank();
    emitter.label_global("_main");
    emitter.comment("prologue");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // grow stack for locals + saved regs
    if frame_size - 16 <= 504 {
        emitter.instruction(&format!("stp x29, x30, [sp, #{}]", frame_size - 16)); // save frame pointer & return address
    } else {
        emitter.instruction(&format!("add x9, sp, #{}", frame_size - 16));      // compute address of the saved frame-link area for a large main frame
        emitter.instruction("stp x29, x30, [x9]");                              // save frame pointer & return address through the computed address
    }
    emitter.instruction(&format!("add x29, sp, #{}", frame_size - 16));         // set new frame pointer

    // -- save argc/argv to globals (for $argv runtime builder) --
    emitter.comment("save argc/argv to globals");
    emitter.instruction("adrp x9, _global_argc@PAGE");                          // load page of argc global
    emitter.instruction("add x9, x9, _global_argc@PAGEOFF");                    // add page offset
    emitter.instruction("str x0, [x9]");                                        // store argc (x0 from OS)
    emitter.instruction("adrp x9, _global_argv@PAGE");                          // load page of argv global
    emitter.instruction("add x9, x9, _global_argv@PAGEOFF");                    // add page offset
    emitter.instruction("str x1, [x9]");                                        // store argv pointer (x1 from OS)

    if heap_debug {
        emitter.comment("enable heap debug flag");
        emitter.instruction("adrp x9, _heap_debug_enabled@PAGE");               // load page of the heap-debug runtime flag
        emitter.instruction("add x9, x9, _heap_debug_enabled@PAGEOFF");         // resolve the heap-debug runtime flag address
        emitter.instruction("mov x10, #1");                                     // compile-time option enables heap debug for this binary
        emitter.instruction("str x10, [x9]");                                   // store enabled=1 into the BSS-backed runtime flag
    }

    // -- store $argc in local variable --
    let argc_offset = ctx.variables.get("argc").expect("codegen bug: $argc not pre-allocated in main scope").stack_offset;
    abi::store_at_offset(&mut emitter, "x0", argc_offset);                        // $argc = OS argc

    // -- build $argv array from OS C strings --
    let argv_offset = ctx.variables.get("argv").expect("codegen bug: $argv not pre-allocated in main scope").stack_offset;
    emitter.comment("build $argv array from OS argv");
    emitter.instruction("bl __rt_build_argv");                                  // returns array ptr in x0
    abi::store_at_offset(&mut emitter, "x0", argv_offset);                      // $argv = array

    // -- zero-initialize local variables that may be decref'd on reassignment --
    let main_skip = HashSet::from(["argc".to_string(), "argv".to_string()]);
    for (name, var) in &ctx.variables {
        if main_skip.contains(name) { continue; }
        if matches!(
            &var.ty,
            PhpType::Str | PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_)
        ) {
            abi::store_at_offset(&mut emitter, "xzr", var.stack_offset);        // zero-init to prevent stale ptr free
        }
    }
    emit_main_activation_record_push(&mut emitter, &ctx, &main_cleanup_label);
    emit_enum_singleton_initializers(&mut emitter, &mut data, &ctx);

    // -- emit user statements --
    for s in program {
        if matches!(
            &s.kind,
            StmtKind::FunctionDecl { .. }
                | StmtKind::ClassDecl { .. }
                | StmtKind::InterfaceDecl { .. }
                | StmtKind::TraitDecl { .. }
        ) {
            continue;
        }
        stmt::emit_stmt(s, &mut emitter, &mut ctx, &mut data);
    }

    // -- epilogue: restore stack and exit(0) via syscall --
    emitter.blank();
    emitter.comment("epilogue + exit(0)");
    functions::emit_owned_local_epilogue_cleanup(&mut emitter, &ctx);
    emit_main_activation_record_pop(&mut emitter, &ctx);
    if frame_size - 16 <= 504 {
        emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", frame_size - 16)); // restore frame pointer & return address
    } else {
        emitter.instruction(&format!("add x9, sp, #{}", frame_size - 16));      // compute address of the saved frame-link area for a large main frame
        emitter.instruction("ldp x29, x30, [x9]");                              // restore frame pointer & return address through the computed address
    }
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate stack frame
    // -- GC statistics (printed to stderr if --gc-stats flag was set) --
    if gc_stats {
        emitter.comment("gc-stats: print allocation statistics to stderr");
        let (lbl_a, len_a) = data.add_string(b"GC: allocs=");
        emitter.instruction(&format!("adrp x1, {}@PAGE", lbl_a));               // load gc stats label page
        emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", lbl_a));         // resolve address
        emitter.instruction(&format!("mov x2, #{}", len_a));                    // string length
        emitter.instruction("mov x0, #2");                                      // fd = stderr
        emitter.instruction("mov x16, #4");                                     // syscall write
        emitter.instruction("svc #0x80");                                       // write to stderr
        emitter.instruction("adrp x9, _gc_allocs@PAGE");                        // load gc_allocs page
        emitter.instruction("add x9, x9, _gc_allocs@PAGEOFF");                  // resolve address
        emitter.instruction("ldr x0, [x9]");                                    // load alloc count
        emitter.instruction("bl __rt_itoa");                                    // convert to string → x1/x2
        emitter.instruction("mov x0, #2");                                      // fd = stderr
        emitter.instruction("mov x16, #4");                                     // syscall write
        emitter.instruction("svc #0x80");                                       // write count
        let (lbl_f, len_f) = data.add_string(b" frees=");
        emitter.instruction(&format!("adrp x1, {}@PAGE", lbl_f));               // load frees label page
        emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", lbl_f));         // resolve address
        emitter.instruction(&format!("mov x2, #{}", len_f));                    // string length
        emitter.instruction("mov x0, #2");                                      // fd = stderr
        emitter.instruction("mov x16, #4");                                     // syscall write
        emitter.instruction("svc #0x80");                                       // write label
        emitter.instruction("adrp x9, _gc_frees@PAGE");                         // load gc_frees page
        emitter.instruction("add x9, x9, _gc_frees@PAGEOFF");                   // resolve address
        emitter.instruction("ldr x0, [x9]");                                    // load free count
        emitter.instruction("bl __rt_itoa");                                    // convert to string → x1/x2
        emitter.instruction("mov x0, #2");                                      // fd = stderr
        emitter.instruction("mov x16, #4");                                     // syscall write
        emitter.instruction("svc #0x80");                                       // write count
        let (lbl_nl, _) = data.add_string(b"\n");
        emitter.instruction(&format!("adrp x1, {}@PAGE", lbl_nl));              // load newline page
        emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", lbl_nl));        // resolve address
        emitter.instruction("mov x2, #1");                                      // newline length
        emitter.instruction("mov x0, #2");                                      // fd = stderr
        emitter.instruction("mov x16, #4");                                     // syscall write
        emitter.instruction("svc #0x80");                                       // write newline
    }

    if heap_debug {
        emitter.comment("heap-debug: print allocator summary and leak report to stderr");
        emitter.instruction("bl __rt_heap_debug_report");                       // emit the heap-debug summary at process exit
    }

    emitter.instruction("mov x0, #0");                                          // exit code 0
    emitter.instruction("mov x16, #1");                                         // syscall number 1 = exit
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel

    // -- emit deferred closures --
    emit_deferred_closures(&mut emitter, &mut data, &mut ctx);
    emit_main_cleanup_callback(&mut emitter, &main_cleanup_label, &ctx);

    // -- build user assembly (functions + main + closures + user data) --
    let data_output = data.emit();
    let user_data = runtime::emit_runtime_data_user(
        &all_global_var_names,
        &all_static_vars,
        interfaces,
        classes,
        enums,
    );

    let mut user_asm = emitter.output();
    if !data_output.is_empty() {
        user_asm.push('\n');
        user_asm.push_str(&data_output);
    }
    user_asm.push('\n');
    user_asm.push_str(&user_data);

    // -- build runtime assembly (routines + fixed data) --
    let runtime_asm = generate_runtime(heap_size);

    (user_asm, runtime_asm)
}

/// Generate the runtime assembly string independently.
/// This output is identical for all programs compiled with the same heap_size
/// and can be pre-assembled and cached.
pub fn generate_runtime(heap_size: usize) -> String {
    let mut emitter = Emitter::new();
    emitter.raw(".text");
    runtime::emit_runtime(&mut emitter);
    let mut output = emitter.output();
    output.push('\n');
    output.push_str(&runtime::emit_runtime_data_fixed(heap_size));
    output
}

fn emit_enum_singleton_initializers(
    emitter: &mut Emitter,
    data: &mut DataSection,
    ctx: &Context,
) {
    let mut sorted_enums: Vec<(&String, &EnumInfo)> = ctx.enums.iter().collect();
    sorted_enums.sort_by_key(|(name, _)| name.as_str());
    for (enum_name, enum_info) in sorted_enums {
        let Some(class_info) = ctx.classes.get(enum_name) else {
            continue;
        };
        for case in &enum_info.cases {
            emitter.comment(&format!("initialize enum singleton {}::{}", enum_name, case.name));
            let obj_size = 8 + class_info.properties.len() * 16;
            emitter.instruction(&format!("mov x0, #{}", obj_size));             // enum singleton object size in bytes
            emitter.instruction("bl __rt_heap_alloc");                          // allocate enum singleton object storage
            emitter.instruction("mov x9, #4");                                  // heap kind 4 = object instance
            emitter.instruction("str x9, [x0, #-8]");                           // store object kind in the uniform heap header
            emitter.instruction(&format!("mov x10, #{}", class_info.class_id)); // load compile-time enum class id
            emitter.instruction("str x10, [x0]");                               // store enum class id at object header
            emitter.instruction("str x0, [sp, #-16]!");                         // save singleton object pointer while initializing properties

            for i in 0..class_info.properties.len() {
                let offset = 8 + i * 16;
                emitter.instruction("ldr x9, [sp]");                            // peek enum singleton pointer from the stack
                emitter.instruction(&format!("str xzr, [x9, #{}]", offset));    // zero-init property lo word
                emitter.instruction(&format!("str xzr, [x9, #{}]", offset + 8)); // zero-init property hi word
            }

            if let Some(case_value) = &case.value {
                emitter.instruction("ldr x9, [sp]");                            // reload enum singleton pointer for backing-value initialization
                match case_value {
                    EnumCaseValue::Int(value) => {
                        load_immediate(emitter, "x10", *value);                 // materialize the enum int backing value
                        emitter.instruction("str x10, [x9, #8]");               // store the int backing value in the first property slot
                        emitter.instruction("str xzr, [x9, #16]");              // clear the metadata/high word for the int property
                    }
                    EnumCaseValue::Str(value) => {
                        let (label, len) = data.add_string(value.as_bytes());
                        emitter.instruction(&format!("adrp x10, {}@PAGE", label)); // load page of the enum string backing literal
                        emitter.instruction(&format!("add x10, x10, {}@PAGEOFF", label)); // resolve the enum string backing literal address
                        emitter.instruction("str x10, [x9, #8]");               // store the string backing pointer in the first property slot
                        emitter.instruction(&format!("mov x10, #{}", len));     // materialize the enum string backing length
                        emitter.instruction("str x10, [x9, #16]");              // store the string backing length in the second property word
                    }
                }
            }

            emitter.instruction("ldr x0, [sp], #16");                           // pop initialized enum singleton pointer into x0
            let slot_label = enum_case_symbol(enum_name, &case.name);
            emitter.instruction(&format!("adrp x9, {}@PAGE", slot_label));      // load page of the enum singleton slot
            emitter.instruction(&format!("add x9, x9, {}@PAGEOFF", slot_label)); // resolve the enum singleton slot address
            emitter.instruction("str x0, [x9]");                                // publish the enum singleton pointer in its global slot
        }
    }
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
            functions::emit_closure(
                emitter,
                data,
                &closure.label,
                &closure.sig,
                &closure.body,
                &ctx.functions,
                &ctx.constants,
                &ctx.interfaces,
                &ctx.classes,
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
                    if name.as_str() == "define" && args.len() == 2 {
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
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_global_vars_in_body(try_body, names);
                for catch_clause in catches {
                    collect_global_vars_in_body(&catch_clause.body, names);
                }
                if let Some(body) = finally_body {
                    collect_global_vars_in_body(body, names);
                }
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
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_static_vars_in_body(func_name, try_body, statics, global_env);
                for catch_clause in catches {
                    collect_static_vars_in_body(func_name, &catch_clause.body, statics, global_env);
                }
                if let Some(body) = finally_body {
                    collect_static_vars_in_body(func_name, body, statics, global_env);
                }
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

fn collect_main_try_slots(stmts: &[Stmt], ctx: &mut Context) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                let slot_offset = ctx.alloc_hidden_slot(208);
                ctx.try_slot_offsets.push(slot_offset);
                collect_main_try_slots(try_body, ctx);
                for catch_clause in catches {
                    collect_main_try_slots(&catch_clause.body, ctx);
                }
                if let Some(body) = finally_body {
                    collect_main_try_slots(body, ctx);
                }
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_main_try_slots(then_body, ctx);
                for (_, body) in elseif_clauses {
                    collect_main_try_slots(body, ctx);
                }
                if let Some(body) = else_body {
                    collect_main_try_slots(body, ctx);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Foreach { body, .. } => collect_main_try_slots(body, ctx),
            StmtKind::For { init, update, body, .. } => {
                if let Some(s) = init {
                    collect_main_try_slots(&[*s.clone()], ctx);
                }
                if let Some(s) = update {
                    collect_main_try_slots(&[*s.clone()], ctx);
                }
                collect_main_try_slots(body, ctx);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_main_try_slots(body, ctx);
                }
                if let Some(body) = default {
                    collect_main_try_slots(body, ctx);
                }
            }
            StmtKind::FunctionDecl { .. }
            | StmtKind::ClassDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. } => {}
            _ => {}
        }
    }
}

fn emit_main_activation_record_push(emitter: &mut Emitter, ctx: &Context, cleanup_label: &str) {
    let prev_offset = ctx
        .activation_prev_offset
        .expect("codegen bug: missing main activation prev slot");
    let cleanup_offset = ctx
        .activation_cleanup_offset
        .expect("codegen bug: missing main activation cleanup slot");
    let frame_base_offset = ctx
        .activation_frame_base_offset
        .expect("codegen bug: missing main activation frame-base slot");

    emitter.comment("register main exception cleanup frame");
    emitter.instruction("adrp x9, _exc_call_frame_top@PAGE");                   // load page of the call-frame stack top
    emitter.instruction("add x9, x9, _exc_call_frame_top@PAGEOFF");             // resolve the call-frame stack top address
    emitter.instruction("ldr x10, [x9]");                                       // load the previous call-frame pointer
    abi::store_at_offset(emitter, "x10", prev_offset);                          // save the previous call-frame pointer in the main activation record
    emitter.instruction(&format!("adrp x10, {}@PAGE", cleanup_label));          // load page of the main cleanup callback label
    emitter.instruction(&format!("add x10, x10, {}@PAGEOFF", cleanup_label));   // resolve the main cleanup callback label address
    abi::store_at_offset(emitter, "x10", cleanup_offset);                       // save the main cleanup callback address in the activation record
    emitter.instruction("mov x10, x29");                                        // x10 = current main frame pointer for cleanup callbacks
    abi::store_at_offset(emitter, "x10", frame_base_offset);                    // save the main frame pointer in the activation record
    abi::store_at_offset(emitter, "xzr", ctx.pending_action_offset.expect("codegen bug: missing main pending-action slot")); // clear any stale finally action before running main
    emitter.instruction(&format!("sub x10, x29, #{}", prev_offset));            // x10 = address of the main activation record's first slot
    emitter.instruction("adrp x9, _exc_call_frame_top@PAGE");                   // reload page of the call-frame stack top after stack-slot stores may clobber x9
    emitter.instruction("add x9, x9, _exc_call_frame_top@PAGEOFF");             // resolve the call-frame stack top address again
    emitter.instruction("str x10, [x9]");                                       // publish the main activation record as the new call-frame stack top
}

fn emit_main_activation_record_pop(emitter: &mut Emitter, ctx: &Context) {
    let prev_offset = ctx
        .activation_prev_offset
        .expect("codegen bug: missing main activation prev slot");

    emitter.comment("unregister main exception cleanup frame");
    emitter.instruction("adrp x9, _exc_call_frame_top@PAGE");                   // load page of the call-frame stack top
    emitter.instruction("add x9, x9, _exc_call_frame_top@PAGEOFF");             // resolve the call-frame stack top address
    abi::load_at_offset(emitter, "x10", prev_offset);                           // reload the previous call-frame pointer from the main activation record
    emitter.instruction("adrp x9, _exc_call_frame_top@PAGE");                   // reload page of the call-frame stack top after the load helper may clobber x9
    emitter.instruction("add x9, x9, _exc_call_frame_top@PAGEOFF");             // resolve the call-frame stack top address again
    emitter.instruction("str x10, [x9]");                                       // restore the previous call-frame stack top before exiting
}

fn emit_main_cleanup_callback(emitter: &mut Emitter, cleanup_label: &str, ctx: &Context) {
    emitter.label(cleanup_label);
    emitter.instruction("sub sp, sp, #16");                                     // reserve callback spill space for x29/x30
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save the caller frame pointer and return address
    emitter.instruction("mov x29, x0");                                         // treat the unwound main frame pointer as our temporary base
    functions::emit_owned_local_epilogue_cleanup(emitter, ctx);
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore the callback frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the callback spill space
    emitter.instruction("ret");                                                 // finish unwound-main cleanup callback before catch dispatch
    emitter.blank();
}

pub(crate) fn runtime_value_tag(ty: &PhpType) -> u8 {
    match ty {
        PhpType::Int => 0,
        PhpType::Str => 1,
        PhpType::Float => 2,
        PhpType::Bool => 3,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed => 7,
        PhpType::Union(_) => 7,
        PhpType::Void => 8,
        PhpType::Callable | PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) => 0,
    }
}

pub(crate) fn emit_box_runtime_payload_as_mixed(
    emitter: &mut Emitter,
    value_tag_reg: &str,
    value_lo_reg: &str,
    value_hi_reg: &str,
) {
    emitter.instruction(&format!("mov x0, {}", value_tag_reg));                 // x0 = runtime value tag for the mixed boxing helper
    emitter.instruction(&format!("mov x1, {}", value_lo_reg));                  // x1 = low payload word for the mixed boxing helper
    emitter.instruction(&format!("mov x2, {}", value_hi_reg));                  // x2 = high payload word for the mixed boxing helper
    emitter.instruction("bl __rt_mixed_from_value");                            // retain/persist the payload as needed and return a boxed mixed cell
}

pub(crate) fn emit_box_current_value_as_mixed(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Mixed | PhpType::Union(_) => {}
        PhpType::Int | PhpType::Bool | PhpType::Void => {
            emitter.instruction("mov x1, x0");                                  // move the current scalar payload into the mixed helper argument register
            emitter.instruction("mov x2, xzr");                                 // scalar mixed payloads do not use a second word
            emitter.instruction(&format!("mov x0, #{}", runtime_value_tag(ty))); //materialize the static value tag for this scalar
            emitter.instruction("bl __rt_mixed_from_value");                    // box the scalar payload into a mixed cell
        }
        PhpType::Float => {
            emitter.instruction("fmov x1, d0");                                 // move the current float bits into the mixed helper payload register
            emitter.instruction("mov x2, xzr");                                 // float payloads only use the low word
            emitter.instruction("mov x0, #2");                                  // runtime tag 2 = float
            emitter.instruction("bl __rt_mixed_from_value");                    // box the float payload into a mixed cell
        }
        PhpType::Str => {
            emitter.instruction("mov x0, #1");                                  // runtime tag 1 = string
            emitter.instruction("bl __rt_mixed_from_value");                    // persist the string payload and box it into a mixed cell
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            emitter.instruction("mov x1, x0");                                  // move the current heap pointer into the mixed helper payload register
            emitter.instruction("mov x2, xzr");                                 // heap-backed payloads only use the low word
            emitter.instruction(&format!("mov x0, #{}", runtime_value_tag(ty))); //materialize the heap payload tag for the mixed helper
            emitter.instruction("bl __rt_mixed_from_value");                    // retain the heap child and box it into a mixed cell
        }
        PhpType::Callable | PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) => {
            emitter.instruction("mov x1, x0");                                  // move the raw pointer into the mixed helper payload register
            emitter.instruction("mov x2, xzr");                                 // raw pointers only use the low word
            emitter.instruction("mov x0, #0");                                  // treat unsupported raw pointers as integer-like payloads for now
            emitter.instruction("bl __rt_mixed_from_value");                    // box the raw pointer bits into a mixed cell
        }
    }
}

fn align16(n: usize) -> usize {
    (n + 15) & !15
}

fn load_immediate(emitter: &mut Emitter, reg: &str, value: i64) {
    if (0..=65535).contains(&value) || (-65536..0).contains(&value) {
        emitter.instruction(&format!("mov {}, #{}", reg, value));               // load a small signed immediate directly into the target register
        return;
    }

    let uval = value as u64;
    emitter.instruction(&format!("movz {}, #0x{:x}", reg, uval & 0xFFFF));     // seed the low 16 bits of the wider immediate value
    if (uval >> 16) & 0xFFFF != 0 {
        emitter.instruction(&format!(
            "movk {}, #0x{:x}, lsl #16",
            reg,
            (uval >> 16) & 0xFFFF
        ));                                                                     // patch bits 16-31 of the wider immediate value
    }
    if (uval >> 32) & 0xFFFF != 0 {
        emitter.instruction(&format!(
            "movk {}, #0x{:x}, lsl #32",
            reg,
            (uval >> 32) & 0xFFFF
        ));                                                                     // patch bits 32-47 of the wider immediate value
    }
    if (uval >> 48) & 0xFFFF != 0 {
        emitter.instruction(&format!(
            "movk {}, #0x{:x}, lsl #48",
            reg,
            (uval >> 48) & 0xFFFF
        ));                                                                     // patch bits 48-63 of the wider immediate value
    }
}
