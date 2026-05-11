//! Purpose:
//! Coordinates assembly generation for complete programs and re-exports shared codegen helpers.
//! Builds class metadata, emits user code, and assembles the runtime-facing sections.
//!
//! Called from:
//! - `crate::pipeline::compile()` through `crate::codegen::generate()`
//!
//! Key details:
//! - Keeps frontend type metadata, runtime cache assumptions, and target-specific emission ordered before linking.

mod abi;
mod builtins;
mod class_methods;
pub mod context;
mod data_section;
mod driver_support;
mod emit;
mod expr;
mod ffi;
mod function_variants;
mod functions;
mod interface_wrappers;
mod main_emission;
pub mod platform;
mod prescan;
mod program_usage;
mod runtime;
mod stmt;

use std::collections::{HashMap, HashSet};

use crate::parser::ast::{Program, StmtKind};
use crate::types::{
    ClassInfo, EnumInfo, ExternClassInfo, ExternFunctionSig, FunctionSig, InterfaceInfo,
    PackedClassInfo, PhpType, TypeEnv,
};
use class_methods::emit_class_methods;
use data_section::DataSection;
use driver_support::align16;
use emit::Emitter;
use interface_wrappers::emit_interface_return_wrappers;
use main_emission::emit_main_and_finalize;
pub(crate) use driver_support::{
    emit_box_current_expr_value_as_mixed_for_container, emit_box_current_value_as_mixed,
    emit_box_iterable_value_for_mixed_container, emit_box_runtime_payload_as_mixed,
    emit_normalized_hash_key, emit_release_pushed_refcounted_temp_after_array_push,
    runtime_value_tag,
};
pub use driver_support::generate_runtime;
use platform::Target;
use prescan::{collect_constants, collect_global_var_names, collect_static_vars};
use program_usage::{collect_required_class_names, program_has_dynamic_instanceof};

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
    let global_constants = collect_constants(program, target.platform);

    // Pre-scan for global variable names used in `global $var` statements across all functions
    let all_global_var_names = collect_global_var_names(program);

    // Pre-scan for static variable declarations across all functions
    let all_static_vars = collect_static_vars(program, global_env);

    // Emit user-defined functions before _main (skip extern functions)
    let function_variant_groups = function_variants::collect_function_variant_groups(program);
    let function_variant_group_names: HashSet<String> =
        function_variant_groups.keys().cloned().collect();
    for (name, sig) in functions {
        if extern_functions.contains_key(name) {
            continue; // extern functions have no body — they're linked from C
        }
        if function_variant_groups.contains_key(name) {
            function_variants::emit_function_variant_dispatcher(&mut emitter, &mut data, name);
            continue;
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
            &function_variant_group_names,
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
    let emitted_class_names = if target.arch == platform::Arch::X86_64
        && !program_has_dynamic_instanceof(program)
    {
        Some(collect_x86_emitted_class_names(program, classes))
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
        emit_class_methods(
            &mut emitter,
            &mut data,
            class_name,
            class_info,
            functions,
            &function_variant_group_names,
            &global_constants,
            interfaces,
            classes,
            packed_classes,
            extern_functions,
            extern_classes,
            extern_globals,
        );
    }

    emit_interface_return_wrappers(
        &mut emitter,
        interfaces,
        classes,
        emitted_class_names.as_ref(),
    );

    emit_main_and_finalize(
        emitter,
        data,
        program,
        global_env,
        functions,
        &function_variant_group_names,
        interfaces,
        classes,
        enums,
        packed_classes,
        extern_functions,
        extern_classes,
        extern_globals,
        &global_constants,
        &all_global_var_names,
        &all_static_vars,
        emitted_class_names.as_ref(),
        gc_stats,
        heap_debug,
    )
}

fn collect_x86_emitted_class_names(
    program: &Program,
    classes: &HashMap<String, ClassInfo>,
) -> HashSet<String> {
    let mut names = collect_required_class_names(program);
    if names.contains("Fiber") {
        names.insert("FiberError".to_string());
    }
    expand_emitted_class_dependencies(&mut names, classes);
    names
}

fn expand_emitted_class_dependencies(
    names: &mut HashSet<String>,
    classes: &HashMap<String, ClassInfo>,
) {
    loop {
        let mut changed = false;
        let snapshot: Vec<String> = names.iter().cloned().collect();
        for class_name in snapshot {
            let Some(class_info) = classes.get(&class_name) else {
                continue;
            };
            if let Some(parent) = &class_info.parent {
                changed |= names.insert(parent.clone());
            }
            for impl_class in class_info
                .method_impl_classes
                .values()
                .chain(class_info.static_method_impl_classes.values())
            {
                changed |= names.insert(impl_class.clone());
            }
        }
        if !changed {
            break;
        }
    }
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
