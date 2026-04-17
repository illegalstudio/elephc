mod abi;
mod builtins;
pub mod context;
mod data_section;
mod driver_support;
mod emit;
mod expr;
mod ffi;
mod functions;
pub mod platform;
mod prescan;
mod program_usage;
mod runtime;
mod stmt;

use std::collections::{HashMap, HashSet};

use crate::names::{method_symbol, static_method_symbol};
use crate::parser::ast::{Program, StmtKind};
use crate::types::{
    ClassInfo, EnumInfo, ExternClassInfo, ExternFunctionSig, FunctionSig, InterfaceInfo,
    PackedClassInfo, PhpType, TypeEnv,
};
use context::{Context, HeapOwnership};
use data_section::DataSection;
use driver_support::{
    align16, emit_deferred_closures, emit_enum_singleton_initializers,
    emit_main_activation_record_pop, emit_main_activation_record_push,
    emit_main_cleanup_callback, emit_write_current_string_stderr, emit_write_literal_stderr,
};
use emit::Emitter;
pub(crate) use driver_support::{
    emit_box_current_value_as_mixed, emit_box_runtime_payload_as_mixed, runtime_value_tag,
};
pub use driver_support::generate_runtime;
use platform::Target;
use prescan::{
    collect_constants, collect_global_var_names, collect_main_try_slots,
    collect_static_vars,
};
use program_usage::{collect_required_class_names, program_uses_variable};

pub fn generate_user_asm(
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
    _heap_size: usize,
    gc_stats: bool,
    heap_debug: bool,
    target: Target,
) -> String {
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
        Some(collect_required_class_names(program))
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
        abi::emit_call_label(&mut emitter, "__rt_heap_debug_report");           // emit the heap-debug summary at process exit through the active target ABI
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

    user_asm
}

#[allow(dead_code)]
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
    let user_asm = generate_user_asm(
        program,
        global_env,
        functions,
        interfaces,
        classes,
        enums,
        packed_classes,
        extern_functions,
        extern_classes,
        extern_globals,
        heap_size,
        gc_stats,
        heap_debug,
        target,
    );
    let runtime_asm = generate_runtime(heap_size, target);

    (user_asm, runtime_asm)
}
