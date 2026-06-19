//! Purpose:
//! Emits the synthetic program entry body after frontend passes have produced a flat statement list.
//! Allocates main-frame storage, initializes globals, and lowers top-level statements in order.
//!
//! Called from:
//! - `crate::codegen::generate()`
//!
//! Key details:
//! - Frame sizing must account for locals, hidden temporaries, try handlers, and process argument globals before emission.

use std::collections::{HashMap, HashSet};

use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::Emit;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, builtins, functions, runtime, stmt};
use crate::parser::ast::{ExprKind, Program, StmtKind};
use crate::types::{
    ClassInfo, EnumInfo, ExternClassInfo, ExternFunctionSig, FunctionSig, InterfaceInfo,
    PackedClassInfo, PhpType, TypeEnv,
};

use super::driver_support::{
    align16, emit_deferred_closures, emit_enum_singleton_initializers,
    emit_main_activation_record_pop, emit_main_activation_record_push,
    emit_main_cleanup_callback, emit_static_property_initializers,
    emit_write_current_string_stderr, emit_write_literal_stderr,
};
use super::program_usage::program_uses_variable;

/// Orchestrates the full main-body emission pipeline:
/// - Builds the main `Context` with all program metadata (functions, classes, enums, etc.).
/// - Allocates main-frame locals and hidden slots (activation record, cleanup, concat offsets).
/// - Emits the frame prologue (stack alignment, process args, heap-debug flag).
/// - Publishes optional TLS runtime entry points when the checked program requires them.
/// - Zero-initializes refcounted locals to prevent stale-pointer frees.
/// - Pushes the main activation record and emits enum singleton / static property initializers.
/// - Lowers top-level statements in source order.
/// - Emits the epilogue (local cleanup, activation record pop, frame restore, exit).
/// - Emits deferred closures and the main cleanup callback.
/// - Finalizes assembly (data section, user runtime data) and returns the complete asm string.
///
/// Clones all program metadata into the context; the context is not shared with caller.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_main_and_finalize(
    mut emitter: Emitter,
    mut data: DataSection,
    program: &Program,
    global_env: &TypeEnv,
    functions: &HashMap<String, FunctionSig>,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
    callable_return_sigs: &HashMap<String, FunctionSig>,
    callable_array_return_sigs: &HashMap<String, FunctionSig>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
    function_variant_groups: &HashSet<String>,
    interfaces: &HashMap<String, InterfaceInfo>,
    traits: &HashSet<String>,
    classes: &HashMap<String, ClassInfo>,
    enums: &HashMap<String, EnumInfo>,
    packed_classes: &HashMap<String, PackedClassInfo>,
    extern_functions: &HashMap<String, ExternFunctionSig>,
    extern_classes: &HashMap<String, ExternClassInfo>,
    extern_globals: &HashMap<String, PhpType>,
    global_constants: &HashMap<String, (ExprKind, PhpType)>,
    all_global_var_names: &HashSet<String>,
    all_static_vars: &HashMap<(String, String), PhpType>,
    emitted_class_names: Option<&HashSet<String>>,
    gc_stats: bool,
    heap_debug: bool,
    requires_elephc_tls: bool,
    emit: Emit,
) -> String {
    // In cdylib mode the artifact has no process-entry point and no exit syscall —
    // the host loader (`dlopen`) drives it. We still emit user functions, class
    // methods, the data section, and runtime metadata above this call, so all
    // that remains is to finalize the assembly without a `_main` body. Lifecycle
    // entry points (`elephc_init`, `elephc_shutdown`, ...) are added in a later
    // emission pass once the export-detection slice lands.
    if matches!(emit, Emit::Cdylib) {
        let _ = (
            global_env,
            global_constants,
            requires_elephc_tls,
            packed_classes,
            extern_functions,
            extern_classes,
            extern_globals,
            gc_stats,
            heap_debug,
            program,
        );
        return finish_user_asm(
            emitter,
            data,
            functions,
            function_variant_groups,
            all_global_var_names,
            all_static_vars,
            interfaces,
            classes,
            enums,
            emitted_class_names,
        );
    }

    let mut ctx = build_main_context(
        functions,
        callable_param_sigs,
        callable_return_sigs,
        callable_array_return_sigs,
        fiber_return_sigs,
        function_variant_groups,
        interfaces,
        traits,
        classes,
        enums,
        packed_classes,
        extern_functions,
        extern_classes,
        extern_globals,
        global_constants,
        all_global_var_names,
        all_static_vars,
    );

    let uses_argc = program_uses_variable(program, "argc");
    let uses_argv = program_uses_variable(program, "argv");
    allocate_main_variables(global_env, &mut ctx, uses_argc, uses_argv);
    let main_sig = FunctionSig {
        params: Vec::new(),
        defaults: Vec::new(),
        return_type: PhpType::Void,
        declared_return: false,
        ref_params: Vec::new(),
        declared_params: Vec::new(),
        variadic: None,
        deprecation: None,
    };
    functions::collect_local_vars(program, &mut ctx, &main_sig);
    super::prescan::collect_main_try_slots(program, &mut ctx);

    let main_cleanup_label = allocate_main_hidden_slots(&mut ctx);
    let frame_size = align16(ctx.stack_offset + 16);
    emit_main_prologue(&mut emitter, &mut ctx, frame_size, heap_debug, uses_argc, uses_argv);
    if requires_elephc_tls {
        builtins::publish_tls_function_pointers(&mut emitter);
    }
    zero_initialize_main_locals(&mut emitter, &ctx, uses_argc, uses_argv);
    functions::emit_local_ref_cell_flag_zero_init(&mut emitter, &ctx);
    emit_main_activation_record_push(&mut emitter, &ctx, &main_cleanup_label);
    emit_enum_singleton_initializers(&mut emitter, &mut data, &ctx, emitted_class_names);
    emit_static_property_initializers(&mut emitter, &mut data, &mut ctx, emitted_class_names);

    emit_top_level_statements(program, &mut emitter, &mut ctx, &mut data);
    emit_main_epilogue(
        &mut emitter,
        &mut data,
        &ctx,
        frame_size,
        gc_stats,
        heap_debug,
    );

    emit_deferred_closures(&mut emitter, &mut data, &mut ctx);
    emit_main_cleanup_callback(&mut emitter, &main_cleanup_label, &ctx);
    finish_user_asm(
        emitter,
        data,
        functions,
        function_variant_groups,
        all_global_var_names,
        all_static_vars,
        interfaces,
        classes,
        enums,
        emitted_class_names,
    )
}

/// Builds the main-function `Context` by cloning all program-level metadata tables
/// (functions, classes, enums, traits, interfaces, extern signatures, globals, constants).
/// Marks `ctx.in_main = true` and sets `ctx.return_type = PhpType::Void`.
/// No emission occurs here — only context population.
#[allow(clippy::too_many_arguments)]
fn build_main_context(
    functions: &HashMap<String, FunctionSig>,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
    callable_return_sigs: &HashMap<String, FunctionSig>,
    callable_array_return_sigs: &HashMap<String, FunctionSig>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
    function_variant_groups: &HashSet<String>,
    interfaces: &HashMap<String, InterfaceInfo>,
    traits: &HashSet<String>,
    classes: &HashMap<String, ClassInfo>,
    enums: &HashMap<String, EnumInfo>,
    packed_classes: &HashMap<String, PackedClassInfo>,
    extern_functions: &HashMap<String, ExternFunctionSig>,
    extern_classes: &HashMap<String, ExternClassInfo>,
    extern_globals: &HashMap<String, PhpType>,
    global_constants: &HashMap<String, (ExprKind, PhpType)>,
    all_global_var_names: &HashSet<String>,
    all_static_vars: &HashMap<(String, String), PhpType>,
) -> Context {
    let mut ctx = Context::new();
    ctx.functions = functions.clone();
    ctx.callable_param_sigs = callable_param_sigs.clone();
    ctx.callable_return_sigs = callable_return_sigs.clone();
    ctx.callable_array_return_sigs = callable_array_return_sigs.clone();
    ctx.fiber_return_sigs = fiber_return_sigs.clone();
    ctx.function_variant_groups = function_variant_groups.clone();
    ctx.constants = global_constants.clone();
    ctx.in_main = true;
    ctx.return_type = PhpType::Void;
    ctx.all_global_var_names = all_global_var_names.clone();
    ctx.all_static_vars = all_static_vars.clone();
    ctx.classes = classes.clone();
    ctx.interfaces = interfaces.clone();
    ctx.traits = traits.clone();
    ctx.enums = enums.clone();
    ctx.packed_classes = packed_classes.clone();
    ctx.extern_functions = extern_functions.clone();
    ctx.extern_classes = extern_classes.clone();
    ctx.extern_globals = extern_globals.clone();
    ctx
}

/// Allocates main-function local variables into `ctx`.
/// If `$argc` / `$argv` are used in the program but not already in `global_env`,
/// adds them as `PhpType::Int` / `PhpType::Array(Str)` locals.
/// Then iterates `global_env` and allocates every global variable with its
/// codegen representation and full type preserved.
fn allocate_main_variables(
    global_env: &TypeEnv,
    ctx: &mut Context,
    uses_argc: bool,
    uses_argv: bool,
) {
    if uses_argc && !global_env.contains_key("argc") {
        ctx.alloc_var("argc", PhpType::Int);
    }
    if uses_argv && !global_env.contains_key("argv") {
        ctx.alloc_var("argv", PhpType::Array(Box::new(PhpType::Str)));
    }
    for (name, ty) in global_env {
        ctx.alloc_var_with_static_type(name, ty.codegen_repr(), ty.clone());
    }
}

/// Allocates main-function hidden slots into `ctx`:
/// - `activation_frame_base_offset` — base for the main activation record (8 bytes)
/// - `activation_cleanup_offset` — cleanup handler address (8 bytes)
/// - `activation_prev_offset` — previous frame link (8 bytes)
/// - `pending_action_offset` — generic pending-action pointer (8 bytes)
/// - `pending_target_offset` — generic pending-target pointer (8 bytes)
/// - `nested_concat_offset_offset` — concat state offset (8 bytes)
/// - `pending_return_value_offset` — return value slot (16 bytes, naturally aligned)
///
/// Returns the generated `main_cleanup_frame` label name for use in epilogue emits.
fn allocate_main_hidden_slots(ctx: &mut Context) -> String {
    let main_cleanup_label = ctx.next_label("main_cleanup_frame");
    ctx.activation_frame_base_offset = Some(ctx.alloc_hidden_slot(8));
    ctx.activation_cleanup_offset = Some(ctx.alloc_hidden_slot(8));
    ctx.activation_prev_offset = Some(ctx.alloc_hidden_slot(8));
    ctx.pending_action_offset = Some(ctx.alloc_hidden_slot(8));
    ctx.pending_target_offset = Some(ctx.alloc_hidden_slot(8));
    ctx.nested_concat_offset_offset = Some(ctx.alloc_hidden_slot(8));
    ctx.pending_return_value_offset = Some(ctx.alloc_hidden_slot(16));
    main_cleanup_label
}

/// Emits the main entry-point prologue:
/// - Aligns to 4 bytes, emits the entry label.
/// - Emits the stack frame (size rounded to 16-byte alignment via `align16`).
/// - Stores OS argc/argv into global slots via `abi::emit_store_process_args_to_globals`.
/// - Optionally enables the heap-debug flag.
/// - If `uses_argc`: copies OS `argc` into the `$argc` local slot via `abi::store_at_offset`.
/// - If `uses_argv`: calls `__rt_build_argv` to construct the `$argv` array from OS argv,
///   stores the result pointer through the ABI result-register helper, and marks the
///   `$argv` variable `Borrowed` with `epilogue_cleanup_safe = false` (runtime owns the array).
///
/// The heap-debug flag and argc/argv globals are independent concerns; the function
/// branches on each independently rather than pre-computing a combined condition.
fn emit_main_prologue(
    emitter: &mut Emitter,
    ctx: &mut Context,
    frame_size: usize,
    heap_debug: bool,
    uses_argc: bool,
    uses_argv: bool,
) {
    emitter.raw(".align 2");
    emitter.blank();
    emitter.entry_label();
    abi::emit_frame_prologue(emitter, frame_size);

    emitter.comment("save argc/argv to globals");
    abi::emit_store_process_args_to_globals(emitter);

    if heap_debug {
        emitter.comment("enable heap debug flag");
        abi::emit_enable_heap_debug_flag(emitter);
    }

    if uses_argc {
        let argc_offset = ctx
            .variables
            .get("argc")
            .expect("codegen bug: $argc not pre-allocated in main scope")
            .stack_offset;
        let argc_reg = abi::process_argc_reg(emitter.target);
        abi::store_at_offset(emitter, argc_reg, argc_offset);                  // $argc = OS argc
    }

    if uses_argv {
        let argv_offset = ctx
            .variables
            .get("argv")
            .expect("codegen bug: $argv not pre-allocated in main scope")
            .stack_offset;
        emitter.comment("build $argv array from OS argv");
        abi::emit_call_label(emitter, "__rt_build_argv");                      // returns array ptr in the target integer result register
        abi::emit_store(
            emitter,
            &PhpType::Array(Box::new(PhpType::Str)),
            argv_offset,
        );                                                                     // store the built argv array through the ABI result-register helper
        if let Some(argv_var) = ctx.variables.get_mut("argv") {
            argv_var.ownership = HeapOwnership::Borrowed;
            argv_var.epilogue_cleanup_safe = false;
        }
    }
}

/// Zero-initializes main-function local variables that are cleanup-tracked heap values.
/// Skips `$argc` and `$argv` since those are populated from OS arguments in the prologue.
/// Uses `abi::emit_store_zero_to_local_slot` for each refcounted local to ensure
/// no stale pointer is freed at function exit (prevents use-after-free on uninitialized slot).
fn zero_initialize_main_locals(
    emitter: &mut Emitter,
    ctx: &Context,
    uses_argc: bool,
    uses_argv: bool,
) {
    let mut main_skip = HashSet::new();
    if uses_argc {
        main_skip.insert("argc".to_string());
    }
    if uses_argv {
        main_skip.insert("argv".to_string());
    }
    for (name, var) in &ctx.variables {
        if main_skip.contains(name) {
            continue;
        }
        if matches!(&var.ty, PhpType::Str | PhpType::Callable) || var.ty.is_refcounted() {
            abi::emit_store_zero_to_local_slot(emitter, var.stack_offset);     // zero-init to prevent stale ptr free
        }
    }
}

/// Emits all top-level executable statements from `program` in source order.
/// Skips `FunctionDecl`, `FunctionVariantGroup`, `ClassDecl`, `InterfaceDecl`, and `TraitDecl`
/// statements — these are declaration-only and handled by the resolver/ name-resolver phases
/// prior to codegen. Every other statement (expression statements, control flow, include/require,
/// echo, return, etc.) is passed to `stmt::emit_stmt` with the current `emitter`, `ctx`, and `data`.
fn emit_top_level_statements(
    program: &Program,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    for s in program {
        if matches!(
            &s.kind,
            StmtKind::FunctionDecl { .. }
                | StmtKind::FunctionVariantGroup { .. }
                | StmtKind::ClassDecl { .. }
                | StmtKind::InterfaceDecl { .. }
                | StmtKind::TraitDecl { .. }
        ) {
            continue;
        }
        stmt::emit_stmt(s, emitter, ctx, data);
    }
}

/// Emits the main entry-point epilogue:
/// - Emits owned-local cleanup via `functions::emit_owned_local_epilogue_cleanup` (frees refcounted locals).
/// - Pops the main activation record.
/// - Restores the previous frame pointer and stack pointer via `abi::emit_frame_restore`.
/// - Optionally emits GC allocation/free statistics to stderr (via `emit_gc_stats`).
/// - Optionally emits the heap-debug summary and leak report via `__rt_heap_debug_report`.
/// - Finally calls `abi::emit_exit(0)` to terminate the process cleanly.
///
/// `gc_stats` and `heap_debug` are independent flags; both branches may fire in the same build.
fn emit_main_epilogue(
    emitter: &mut Emitter,
    data: &mut DataSection,
    ctx: &Context,
    frame_size: usize,
    gc_stats: bool,
    heap_debug: bool,
) {
    emitter.blank();
    emitter.comment("epilogue + exit(0)");
    functions::emit_owned_local_epilogue_cleanup(emitter, ctx, "main_epilogue");
    emit_main_activation_record_pop(emitter, ctx);
    abi::emit_frame_restore(emitter, frame_size);
    if gc_stats {
        emit_gc_stats(emitter, data);
    }
    if heap_debug {
        emitter.comment("heap-debug: print allocator summary and leak report to stderr");
        abi::emit_call_label(emitter, "__rt_heap_debug_report");               // emit the heap-debug summary at process exit through the active target ABI
    }
    abi::emit_exit(emitter, 0);
}

/// Emits GC allocation statistics to stderr:
/// Writes the literal string "GC: allocs=" via `emit_write_literal_stderr`,
/// then loads `_gc_allocs` symbol, converts it to a decimal string via `__rt_itoa`,
/// writes that to stderr, repeats for `_gc_frees`, and finally writes a newline.
/// The active target's integer-result register holds the numeric value during conversion;
/// the string-result registers (`x1`, `x2` on ARM64) receive the formatted output.
fn emit_gc_stats(emitter: &mut Emitter, data: &mut DataSection) {
    emitter.comment("gc-stats: print allocation statistics to stderr");
    let (lbl_a, len_a) = data.add_string(b"GC: allocs=");
    emit_write_literal_stderr(emitter, &lbl_a, len_a);
    let int_result_reg = abi::int_result_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, int_result_reg, "_gc_allocs", 0);
    abi::emit_call_label(emitter, "__rt_itoa");                                // convert the allocation count into the active target string result registers
    emit_write_current_string_stderr(emitter);
    let (lbl_f, len_f) = data.add_string(b" frees=");
    emit_write_literal_stderr(emitter, &lbl_f, len_f);
    abi::emit_load_symbol_to_reg(emitter, int_result_reg, "_gc_frees", 0);
    abi::emit_call_label(emitter, "__rt_itoa");                                // convert the free-count total into the active target string result registers
    emit_write_current_string_stderr(emitter);
    let (lbl_nl, _) = data.add_string(b"\n");
    emit_write_literal_stderr(emitter, &lbl_nl, 1);
}

/// Finalizes the assembly output:
/// - Emits the data section via `data.emit()` and appends it to the emitter output.
/// - Calls `runtime::emit_runtime_data_user` with all global metadata (vars, statics, functions,
///   variant groups, interfaces, classes, enums, emitted class names) to produce user-facing
///   runtime data (singleton tables, function metadata, class descriptors, etc.).
/// - Concatenates everything into one complete asm string and returns it.
///
/// The function does not invoke `as`/`ld`; assembly is returned as a string for caller to link.
#[allow(clippy::too_many_arguments)]
fn finish_user_asm(
    emitter: Emitter,
    data: DataSection,
    functions: &HashMap<String, FunctionSig>,
    function_variant_groups: &HashSet<String>,
    all_global_var_names: &HashSet<String>,
    all_static_vars: &HashMap<(String, String), PhpType>,
    interfaces: &HashMap<String, InterfaceInfo>,
    classes: &HashMap<String, ClassInfo>,
    enums: &HashMap<String, EnumInfo>,
    emitted_class_names: Option<&HashSet<String>>,
) -> String {
    let data_output = data.emit();
    let user_data = runtime::emit_runtime_data_user(
        all_global_var_names,
        all_static_vars,
        functions,
        function_variant_groups,
        interfaces,
        classes,
        enums,
        emitted_class_names,
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
