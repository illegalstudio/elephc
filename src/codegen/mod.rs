mod abi;
mod builtins;
pub mod context;
mod data_section;
mod emit;
mod expr;
mod ffi;
mod functions;
pub mod platform;
mod runtime;
mod stmt;

use std::collections::{HashMap, HashSet};

use crate::names::{enum_case_symbol, method_symbol, static_method_symbol};
use crate::parser::ast::{Expr, ExprKind, Program, Stmt, StmtKind};
use crate::types::{
    ClassInfo, EnumCaseValue, EnumInfo, ExternClassInfo, ExternFunctionSig, FunctionSig,
    InterfaceInfo, PackedClassInfo, PhpType, TypeEnv,
};
use context::{Context, HeapOwnership};
use data_section::DataSection;
use emit::Emitter;
use platform::Target;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

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
    target: Target,
) -> (String, String) {
    let mut emitter = Emitter::new(target);
    if target.arch == platform::Arch::X86_64 {
        emitter.emit_text_prelude();
    }
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
            packed_classes,
            extern_functions,
            extern_classes,
            extern_globals,
        );
    }

    // Emit flattened class methods in class-id order for deterministic output.
    let emitted_class_names = if target.arch == platform::Arch::X86_64 {
        Some(collect_declared_class_names(program))
    } else {
        None
    };
    let mut sorted_classes: Vec<(&String, &ClassInfo)> = classes
        .iter()
        .filter(|(class_name, _)| {
            emitted_class_names
                .as_ref()
                .is_none_or(|declared| declared.contains(*class_name))
        })
        .collect();
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
                    functions, &global_constants, interfaces, classes, packed_classes, class_name,
                    extern_functions, extern_classes, extern_globals,
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

    let uses_argc = program_uses_variable(program, "argc");
    let uses_argv = program_uses_variable(program, "argv");

    // Pre-allocate $argc and $argv superglobals only when the program actually reads them.
    if uses_argc && !global_env.contains_key("argc") {
        ctx.alloc_var("argc", PhpType::Int);
    }
    if uses_argv && !global_env.contains_key("argv") {
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
    ctx.nested_concat_offset_offset = Some(ctx.alloc_hidden_slot(8));
    ctx.pending_return_value_offset = Some(ctx.alloc_hidden_slot(16));

    let vars_size = ctx.stack_offset;
    let frame_size = align16(vars_size + 16);     // 16-byte aligned, +16 for saved x29/x30

    // -- prologue: set up stack frame --
    emitter.raw(".align 2");
    emitter.blank();
    emitter.entry_label();
    abi::emit_frame_prologue(&mut emitter, frame_size);

    // -- save argc/argv to globals (for $argv runtime builder) --
    emitter.comment("save argc/argv to globals");
    abi::emit_store_process_args_to_globals(&mut emitter);

    if heap_debug {
        emitter.comment("enable heap debug flag");
        abi::emit_enable_heap_debug_flag(&mut emitter);
    }

    if uses_argc {
        // -- store $argc in local variable --
        let argc_offset = ctx.variables.get("argc").expect("codegen bug: $argc not pre-allocated in main scope").stack_offset;
        let argc_reg = abi::process_argc_reg(emitter.target);
        abi::store_at_offset(&mut emitter, argc_reg, argc_offset);              // $argc = OS argc
    }

    if uses_argv {
        // -- build $argv array from OS C strings --
        let argv_offset = ctx.variables.get("argv").expect("codegen bug: $argv not pre-allocated in main scope").stack_offset;
        emitter.comment("build $argv array from OS argv");
        abi::emit_call_label(&mut emitter, "__rt_build_argv");                  // returns array ptr in the target integer result register
        abi::emit_store(&mut emitter, &PhpType::Array(Box::new(PhpType::Str)), argv_offset); // store the built argv array through the ABI result-register helper
        if let Some(argv_var) = ctx.variables.get_mut("argv") {
            argv_var.ownership = HeapOwnership::Borrowed;
            argv_var.epilogue_cleanup_safe = false;
        }
    }

    // -- zero-initialize local variables that may be decref'd on reassignment --
    let mut main_skip = HashSet::new();
    if uses_argc {
        main_skip.insert("argc".to_string());
    }
    if uses_argv {
        main_skip.insert("argv".to_string());
    }
    for (name, var) in &ctx.variables {
        if main_skip.contains(name) { continue; }
        if matches!(
            &var.ty,
            PhpType::Str | PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_)
        ) {
            abi::emit_store_zero_to_local_slot(&mut emitter, var.stack_offset); // zero-init to prevent stale ptr free
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
    abi::emit_frame_restore(&mut emitter, frame_size);
    // -- GC statistics (printed to stderr if --gc-stats flag was set) --
    if gc_stats {
        emitter.comment("gc-stats: print allocation statistics to stderr");
        let (lbl_a, len_a) = data.add_string(b"GC: allocs=");
        emit_write_literal_stderr(&mut emitter, &lbl_a, len_a);
        let int_result_reg = abi::int_result_reg(&emitter);
        abi::emit_load_symbol_to_reg(&mut emitter, int_result_reg, "_gc_allocs", 0);
        abi::emit_call_label(&mut emitter, "__rt_itoa");                        // convert the allocation count into the active target string result registers
        emit_write_current_string_stderr(&mut emitter);
        let (lbl_f, len_f) = data.add_string(b" frees=");
        emit_write_literal_stderr(&mut emitter, &lbl_f, len_f);
        abi::emit_load_symbol_to_reg(&mut emitter, int_result_reg, "_gc_frees", 0);
        abi::emit_call_label(&mut emitter, "__rt_itoa");                        // convert the free-count total into the active target string result registers
        emit_write_current_string_stderr(&mut emitter);
        let (lbl_nl, _) = data.add_string(b"\n");
        emit_write_literal_stderr(&mut emitter, &lbl_nl, 1);
    }

    if heap_debug {
        emitter.comment("heap-debug: print allocator summary and leak report to stderr");
        emitter.instruction("bl __rt_heap_debug_report");                       // emit the heap-debug summary at process exit
    }

    abi::emit_exit(&mut emitter, 0);

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
        emitted_class_names.as_ref(),
    );

    let mut user_asm = emitter.output();
    if !data_output.is_empty() {
        user_asm.push('\n');
        user_asm.push_str(&data_output);
    }
    user_asm.push('\n');
    user_asm.push_str(&user_data);

    // -- build runtime assembly (routines + fixed data) --
    let runtime_asm = generate_runtime(heap_size, target);

    (user_asm, runtime_asm)
}

fn emit_write_literal_stderr(emitter: &mut Emitter, label: &str, len: usize) {
    match emitter.target.arch {
        platform::Arch::AArch64 => {
            emitter.adrp("x1", label);                                          // load the page address of the stderr literal on AArch64
            emitter.add_lo12("x1", "x1", label);                                // resolve the exact stderr literal address on AArch64
            emitter.instruction(&format!("mov x2, #{}", len));                  // materialize the stderr literal byte length in the AArch64 write-length register
            emitter.instruction("mov x0, #2");                                  // target the stderr file descriptor on AArch64
            emitter.syscall(4);
        }
        platform::Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rsi", label);
            emitter.instruction(&format!("mov edx, {}", len));                  // materialize the stderr literal byte length in the x86_64 write-length register
            emitter.instruction("mov edi, 2");                                  // target the stderr file descriptor on x86_64
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall number 1 = write
            emitter.instruction("syscall");                                     // write the requested literal bytes to stderr on x86_64
        }
    }
}

fn emit_write_current_string_stderr(emitter: &mut Emitter) {
    match emitter.target.arch {
        platform::Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // target the stderr file descriptor on AArch64
            emitter.syscall(4);
        }
        platform::Arch::X86_64 => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            emitter.instruction(&format!("mov rsi, {}", ptr_reg));              // move the current string pointer into the x86_64 write buffer register
            emitter.instruction(&format!("mov rdx, {}", len_reg));              // move the current string length into the x86_64 write length register
            emitter.instruction("mov edi, 2");                                  // target the stderr file descriptor on x86_64
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall number 1 = write
            emitter.instruction("syscall");                                     // write the current string payload to stderr on x86_64
        }
    }
}

/// Generate the runtime assembly string independently.
/// This output is identical for all programs compiled with the same heap_size
/// and can be pre-assembled and cached.
pub fn generate_runtime(heap_size: usize, target: Target) -> String {
    let mut emitter = Emitter::new(target);
    emitter.emit_text_prelude();
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
            let result_reg = abi::int_result_reg(emitter);
            let object_reg = abi::symbol_scratch_reg(emitter);
            let temp_reg = abi::temp_int_reg(emitter.target);
            abi::emit_load_int_immediate(emitter, result_reg, obj_size as i64); // enum singleton object size in bytes in the heap allocator input register
            abi::emit_call_label(emitter, "__rt_heap_alloc");                   // allocate enum singleton object storage
            abi::emit_load_int_immediate(emitter, temp_reg, 4);                 // heap kind 4 = object instance
            match emitter.target.arch {
                platform::Arch::AArch64 => {
                    emitter.instruction(&format!("str {}, [{}, #-8]", temp_reg, result_reg)); // store object kind in the uniform heap header just before the payload pointer
                }
                platform::Arch::X86_64 => {
                    emitter.instruction(&format!("mov {}, 0x{:x}", temp_reg, (X86_64_HEAP_MAGIC_HI32 << 32) | 4)); // materialize the x86_64 object heap kind word with the uniform heap marker
                    emitter.instruction(&format!("mov QWORD PTR [{} - 8], {}", result_reg, temp_reg)); // store object kind in the x86_64 uniform heap header just before the payload pointer
                }
            }
            abi::emit_load_int_immediate(emitter, temp_reg, class_info.class_id as i64); // load compile-time enum class id
            abi::emit_store_to_address(emitter, temp_reg, result_reg, 0);       // store enum class id at object header
            abi::emit_push_reg(emitter, result_reg);                            // save singleton object pointer while initializing properties

            for i in 0..class_info.properties.len() {
                let offset = 8 + i * 16;
                abi::emit_load_temporary_stack_slot(emitter, object_reg, 0);    // peek enum singleton pointer from the temporary stack slot
                abi::emit_store_zero_to_address(emitter, object_reg, offset);   // zero-initialize the low property word
                abi::emit_store_zero_to_address(emitter, object_reg, offset + 8); // zero-initialize the high property word
            }

            if let Some(case_value) = &case.value {
                abi::emit_load_temporary_stack_slot(emitter, object_reg, 0);    // reload enum singleton pointer for backing-value initialization
                match case_value {
                    EnumCaseValue::Int(value) => {
                        load_immediate(emitter, temp_reg, *value);              // materialize the enum int backing value
                        abi::emit_store_to_address(emitter, temp_reg, object_reg, 8); // store the int backing value in the first property slot
                        abi::emit_store_zero_to_address(emitter, object_reg, 16); // clear the metadata/high word for the int property
                    }
                    EnumCaseValue::Str(value) => {
                        let (label, len) = data.add_string(value.as_bytes());
                        abi::emit_symbol_address(emitter, temp_reg, &label);    // materialize the enum string backing literal address
                        abi::emit_store_to_address(emitter, temp_reg, object_reg, 8); // store the string backing pointer in the first property slot
                        abi::emit_load_int_immediate(emitter, temp_reg, len as i64); // materialize the enum string backing length
                        abi::emit_store_to_address(emitter, temp_reg, object_reg, 16); // store the string backing length in the second property word
                    }
                }
            }

            abi::emit_pop_reg(emitter, result_reg);                             // pop initialized enum singleton pointer into the active integer result register
            let slot_label = enum_case_symbol(enum_name, &case.name);
            abi::emit_store_reg_to_symbol(emitter, result_reg, &slot_label, 0); // publish the enum singleton pointer in its global slot
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
                &ctx.packed_classes,
                &ctx.extern_functions,
                &ctx.extern_classes,
                &ctx.extern_globals,
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

fn collect_declared_class_names(program: &Program) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_declared_class_names_in_body(program, &mut names);
    names
}

fn collect_declared_class_names_in_body(stmts: &[Stmt], names: &mut HashSet<String>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::ClassDecl { name, .. } => {
                names.insert(name.clone());
            }
            StmtKind::NamespaceBlock { body, .. } => {
                collect_declared_class_names_in_body(body, names);
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                collect_declared_class_names_in_body(then_body, names);
                if let Some(body) = else_body {
                    collect_declared_class_names_in_body(body, names);
                }
            }
            _ => {}
        }
    }
}

fn program_uses_variable(program: &Program, needle: &str) -> bool {
    program.iter().any(|stmt| stmt_uses_variable(stmt, needle))
}

fn stmt_uses_variable(stmt: &Stmt, needle: &str) -> bool {
    match &stmt.kind {
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::Echo(value)
        | StmtKind::Throw(value)
        | StmtKind::ExprStmt(value)
        | StmtKind::ConstDecl { value, .. } => expr_uses_variable(value, needle),
        StmtKind::Return(Some(value)) => expr_uses_variable(value, needle),
        StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => false,
        StmtKind::ArrayAssign { array, index, value } => {
            array == needle || expr_uses_variable(index, needle) || expr_uses_variable(value, needle)
        }
        StmtKind::ArrayPush { array, value } => array == needle || expr_uses_variable(value, needle),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_uses_variable(condition, needle)
                || then_body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                || elseif_clauses.iter().any(|(cond, body)| {
                    expr_uses_variable(cond, needle)
                        || body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                })
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(|stmt| stmt_uses_variable(stmt, needle)))
        }
        StmtKind::IfDef { then_body, else_body, .. } => {
            then_body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(|stmt| stmt_uses_variable(stmt, needle)))
        }
        StmtKind::While { condition, body } => {
            expr_uses_variable(condition, needle)
                || body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
        }
        StmtKind::DoWhile { body, condition } => {
            body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                || expr_uses_variable(condition, needle)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_ref().is_some_and(|stmt| stmt_uses_variable(stmt, needle))
                || condition.as_ref().is_some_and(|expr| expr_uses_variable(expr, needle))
                || update.as_ref().is_some_and(|stmt| stmt_uses_variable(stmt, needle))
                || body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
        }
        StmtKind::Foreach { array, body, .. } => {
            expr_uses_variable(array, needle)
                || body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
        }
        StmtKind::Switch { subject, cases, default } => {
            expr_uses_variable(subject, needle)
                || cases.iter().any(|(values, body)| {
                    values.iter().any(|expr| expr_uses_variable(expr, needle))
                        || body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(|stmt| stmt_uses_variable(stmt, needle)))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                || catches
                    .iter()
                    .any(|catch_clause| catch_clause.body.iter().any(|stmt| stmt_uses_variable(stmt, needle)))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(|stmt| stmt_uses_variable(stmt, needle)))
        }
        StmtKind::ListUnpack { value, .. } => expr_uses_variable(value, needle),
        StmtKind::StaticVar { init, .. } => expr_uses_variable(init, needle),
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_uses_variable(object, needle) || expr_uses_variable(value, needle)
        }
        StmtKind::FunctionDecl { body, .. } | StmtKind::NamespaceBlock { body, .. } => {
            body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
        }
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. } => methods.iter().any(|method| {
            method.body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
        }),
        StmtKind::EnumDecl { cases, .. } => cases
            .iter()
            .any(|case| case.value.as_ref().is_some_and(|expr| expr_uses_variable(expr, needle))),
        StmtKind::Global { vars } => vars.iter().any(|name| name == needle),
        StmtKind::PackedClassDecl { .. }
        | StmtKind::Include { .. }
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => false,
    }
}

fn expr_uses_variable(expr: &Expr, needle: &str) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => name == needle,
        ExprKind::BinaryOp { left, right, .. } => {
            expr_uses_variable(left, needle) || expr_uses_variable(right, needle)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Spread(inner)
        | ExprKind::PtrCast { expr: inner, .. } => expr_uses_variable(inner, needle),
        ExprKind::NullCoalesce { value, default } => {
            expr_uses_variable(value, needle) || expr_uses_variable(default, needle)
        }
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => name == needle,
        ExprKind::FunctionCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. } => {
            args.iter().any(|arg| expr_uses_variable(arg, needle))
        }
        ExprKind::ExprCall { callee, args } => {
            expr_uses_variable(callee, needle)
                || args.iter().any(|arg| expr_uses_variable(arg, needle))
        }
        ExprKind::ArrayLiteral(items) => items.iter().any(|item| expr_uses_variable(item, needle)),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .any(|(key, value)| expr_uses_variable(key, needle) || expr_uses_variable(value, needle)),
        ExprKind::Match { subject, arms, default } => {
            expr_uses_variable(subject, needle)
                || arms.iter().any(|(values, value)| {
                    values.iter().any(|expr| expr_uses_variable(expr, needle))
                        || expr_uses_variable(value, needle)
                })
                || default
                    .as_ref()
                    .is_some_and(|expr| expr_uses_variable(expr, needle))
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_uses_variable(array, needle) || expr_uses_variable(index, needle)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_uses_variable(condition, needle)
                || expr_uses_variable(then_expr, needle)
                || expr_uses_variable(else_expr, needle)
        }
        ExprKind::Cast { expr, .. }
        | ExprKind::NamedArg { value: expr, .. }
        | ExprKind::BufferNew { len: expr, .. } => expr_uses_variable(expr, needle),
        ExprKind::Closure { body, .. } => body.iter().any(|stmt| stmt_uses_variable(stmt, needle)),
        ExprKind::PropertyAccess { object, .. } => expr_uses_variable(object, needle),
        ExprKind::MethodCall { object, args, .. } => {
            expr_uses_variable(object, needle)
                || args.iter().any(|arg| expr_uses_variable(arg, needle))
        }
        ExprKind::FirstClassCallable(callable) => match callable {
            crate::parser::ast::CallableTarget::Function(_) | crate::parser::ast::CallableTarget::StaticMethod { .. } => false,
            crate::parser::ast::CallableTarget::Method { object, .. } => expr_uses_variable(object, needle),
        },
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::EnumCase { .. }
        | ExprKind::This => false,
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
    let scratch = abi::temp_int_reg(emitter.target);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_exc_call_frame_top", 0);
    abi::store_at_offset(emitter, scratch, prev_offset);                        // save the previous call-frame pointer in the main activation record
    abi::emit_symbol_address(emitter, scratch, cleanup_label);
    abi::store_at_offset(emitter, scratch, cleanup_offset);                     // save the main cleanup callback address in the activation record
    abi::emit_copy_frame_pointer(emitter, scratch);
    abi::store_at_offset(emitter, scratch, frame_base_offset);                  // save the main frame pointer in the activation record
    abi::emit_store_zero_to_local_slot(emitter, ctx.pending_action_offset.expect("codegen bug: missing main pending-action slot")); // clear any stale finally action before running main
    abi::emit_frame_slot_address(emitter, scratch, prev_offset);                // compute the address of the main activation record's first slot
    abi::emit_store_reg_to_symbol(emitter, scratch, "_exc_call_frame_top", 0);
}

fn emit_main_activation_record_pop(emitter: &mut Emitter, ctx: &Context) {
    let prev_offset = ctx
        .activation_prev_offset
        .expect("codegen bug: missing main activation prev slot");

    emitter.comment("unregister main exception cleanup frame");
    let scratch = abi::temp_int_reg(emitter.target);
    abi::load_at_offset(emitter, scratch, prev_offset);                         // reload the previous call-frame pointer from the main activation record
    abi::emit_store_reg_to_symbol(emitter, scratch, "_exc_call_frame_top", 0);
}

fn emit_main_cleanup_callback(emitter: &mut Emitter, cleanup_label: &str, ctx: &Context) {
    emitter.label(cleanup_label);
    abi::emit_cleanup_callback_prologue(emitter, abi::int_result_reg(emitter));
    functions::emit_owned_local_epilogue_cleanup(emitter, ctx);
    abi::emit_cleanup_callback_epilogue(emitter);
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
    match emitter.target.arch {
        platform::Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, {}", value_tag_reg));         // x0 = runtime value tag for the mixed boxing helper
            emitter.instruction(&format!("mov x1, {}", value_lo_reg));          // x1 = low payload word for the mixed boxing helper
            emitter.instruction(&format!("mov x2, {}", value_hi_reg));          // x2 = high payload word for the mixed boxing helper
            emitter.instruction("bl __rt_mixed_from_value");                    // retain/persist the payload as needed and return a boxed mixed cell
        }
        platform::Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", value_tag_reg));        // rax = runtime value tag for the mixed boxing helper
            emitter.instruction(&format!("mov rdi, {}", value_lo_reg));         // rdi = low payload word for the mixed boxing helper
            emitter.instruction(&format!("mov rsi, {}", value_hi_reg));         // rsi = high payload word for the mixed boxing helper
            emitter.instruction("call __rt_mixed_from_value");                  // box the payload into a temporary mixed cell on x86_64
        }
    }
}

pub(crate) fn emit_box_current_value_as_mixed(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Mixed | PhpType::Union(_) => {}
        PhpType::Int | PhpType::Bool | PhpType::Void => {
            match emitter.target.arch {
                platform::Arch::AArch64 => {
                    emitter.instruction("mov x1, x0");                          // move the current scalar payload into the mixed helper argument register
                    emitter.instruction("mov x2, xzr");                         // scalar mixed payloads do not use a second word
                    emitter.instruction(&format!("mov x0, #{}", runtime_value_tag(ty))); // materialize the static value tag for this scalar
                    emitter.instruction("bl __rt_mixed_from_value");            // box the scalar payload into a mixed cell
                }
                platform::Arch::X86_64 => {
                    emitter.instruction("mov rdi, rax");                        // move the current scalar payload into the mixed helper low-word register
                    emitter.instruction("xor rsi, rsi");                        // scalar mixed payloads do not use a second word
                    abi::emit_load_int_immediate(emitter, "rax", runtime_value_tag(ty) as i64);
                    emitter.instruction("call __rt_mixed_from_value");          // box the scalar payload into a mixed cell
                }
            }
        }
        PhpType::Float => {
            match emitter.target.arch {
                platform::Arch::AArch64 => {
                    emitter.instruction("fmov x1, d0");                         // move the current float bits into the mixed helper payload register
                    emitter.instruction("mov x2, xzr");                         // float payloads only use the low word
                    emitter.instruction("mov x0, #2");                          // runtime tag 2 = float
                    emitter.instruction("bl __rt_mixed_from_value");            // box the float payload into a mixed cell
                }
                platform::Arch::X86_64 => {
                    emitter.instruction("movq rdi, xmm0");                      // move the current float bits into the mixed helper payload register
                    emitter.instruction("xor rsi, rsi");                        // float payloads only use the low word
                    abi::emit_load_int_immediate(emitter, "rax", 2);
                    emitter.instruction("call __rt_mixed_from_value");          // box the float payload into a mixed cell
                }
            }
        }
        PhpType::Str => {
            match emitter.target.arch {
                platform::Arch::AArch64 => {
                    emitter.instruction("mov x0, #1");                          // runtime tag 1 = string
                    emitter.instruction("bl __rt_mixed_from_value");            // persist the string payload and box it into a mixed cell
                }
                platform::Arch::X86_64 => {
                    emitter.instruction("mov rdi, rax");                        // move the current string pointer into the mixed helper low-word register
                    emitter.instruction("mov rsi, rdx");                        // move the current string length into the mixed helper high-word register
                    abi::emit_load_int_immediate(emitter, "rax", 1);
                    emitter.instruction("call __rt_mixed_from_value");          // box the string payload into a mixed cell
                }
            }
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            match emitter.target.arch {
                platform::Arch::AArch64 => {
                    emitter.instruction("mov x1, x0");                          // move the current heap pointer into the mixed helper payload register
                    emitter.instruction("mov x2, xzr");                         // heap-backed payloads only use the low word
                    emitter.instruction(&format!("mov x0, #{}", runtime_value_tag(ty))); // materialize the heap payload tag for the mixed helper
                    emitter.instruction("bl __rt_mixed_from_value");            // retain the heap child and box it into a mixed cell
                }
                platform::Arch::X86_64 => {
                    emitter.instruction("mov rdi, rax");                        // move the current heap pointer into the mixed helper payload register
                    emitter.instruction("xor rsi, rsi");                        // heap-backed payloads only use the low word
                    abi::emit_load_int_immediate(emitter, "rax", runtime_value_tag(ty) as i64);
                    emitter.instruction("call __rt_mixed_from_value");          // box the heap child into a mixed cell
                }
            }
        }
        PhpType::Callable | PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) => {
            match emitter.target.arch {
                platform::Arch::AArch64 => {
                    emitter.instruction("mov x1, x0");                          // move the raw pointer into the mixed helper payload register
                    emitter.instruction("mov x2, xzr");                         // raw pointers only use the low word
                    emitter.instruction("mov x0, #0");                          // treat unsupported raw pointers as integer-like payloads for now
                    emitter.instruction("bl __rt_mixed_from_value");            // box the raw pointer bits into a mixed cell
                }
                platform::Arch::X86_64 => {
                    emitter.instruction("mov rdi, rax");                        // move the raw pointer into the mixed helper payload register
                    emitter.instruction("xor rsi, rsi");                        // raw pointers only use the low word
                    abi::emit_load_int_immediate(emitter, "rax", 0);
                    emitter.instruction("call __rt_mixed_from_value");          // box the raw pointer bits into a mixed cell
                }
            }
        }
    }
}

fn align16(n: usize) -> usize {
    (n + 15) & !15
}

fn load_immediate(emitter: &mut Emitter, reg: &str, value: i64) {
    abi::emit_load_int_immediate(emitter, reg, value);                          // materialize the immediate through the shared target-aware helper
}
