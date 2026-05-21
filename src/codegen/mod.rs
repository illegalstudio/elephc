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
mod callables;
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
mod reflection;
mod runtime;
mod stmt;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

thread_local! {
    /// Number of `spl_autoload_register` closure rules the autoload pass
    /// extracted at compile time. Set via [`set_autoload_rule_count`]
    /// before `generate` is called; read by `spl_autoload_functions()`
    /// codegen to size the introspection array. Thread-local so
    /// parallel test runs don't interfere.
    static AUTOLOAD_RULE_COUNT: Cell<usize> = const { Cell::new(0) };
    static DECLARED_CLASS_NAMES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static DECLARED_INTERFACE_NAMES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static DECLARED_TRAIT_NAMES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

pub fn set_autoload_rule_count(n: usize) {
    AUTOLOAD_RULE_COUNT.with(|c| c.set(n));
}

pub fn autoload_rule_count() -> usize {
    AUTOLOAD_RULE_COUNT.with(|c| c.get())
}

fn set_declared_name_order(classes: Vec<String>, interfaces: Vec<String>, traits: Vec<String>) {
    DECLARED_CLASS_NAMES.with(|names| *names.borrow_mut() = classes);
    DECLARED_INTERFACE_NAMES.with(|names| *names.borrow_mut() = interfaces);
    DECLARED_TRAIT_NAMES.with(|names| *names.borrow_mut() = traits);
}

pub(crate) fn declared_class_names() -> Vec<String> {
    DECLARED_CLASS_NAMES.with(|names| names.borrow().clone())
}

pub(crate) fn declared_interface_names() -> Vec<String> {
    DECLARED_INTERFACE_NAMES.with(|names| names.borrow().clone())
}

pub(crate) fn declared_trait_names() -> Vec<String> {
    DECLARED_TRAIT_NAMES.with(|names| names.borrow().clone())
}

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
    runtime_value_tag, UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
};
pub use driver_support::generate_runtime;
use platform::Target;
use prescan::{collect_constants, collect_global_var_names, collect_static_vars};
use program_usage::{collect_required_class_names, program_has_dynamic_instanceof};

pub fn generate_user_asm(
    program: &Program,
    global_env: &TypeEnv,
    functions: &HashMap<String, FunctionSig>,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
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
    let declared_trait_order = collect_declared_trait_names(program);
    let declared_traits: HashSet<String> = declared_trait_order.iter().cloned().collect();
    set_declared_name_order(
        collect_declared_class_names(program, classes),
        collect_declared_interface_names(program, interfaces),
        declared_trait_order,
    );

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
            callable_param_sigs,
            &function_variant_group_names,
            &global_constants, &all_global_var_names, &all_static_vars,
            interfaces,
            &declared_traits,
            Some(classes),
            enums,
            packed_classes,
            extern_functions,
            extern_classes,
            extern_globals,
        );
    }

    // Emit flattened class methods in class-id order for deterministic output.
    // The x86_64 path filters classes to those visibly used by the program so
    // that the test asm stays compact; the filter must include every parent
    // and implemented interface in the inheritance chain because vtables and
    // interface_impl tables reference the inherited method symbols (e.g.
    // JsonException's vtable points at _method_Exception_getmessage).
    //
    // The builtin throwable hierarchy is always included unconditionally
    // because runtime helpers (e.g. __rt_json_throw_error) can raise
    // JsonException objects whose class_id lands in the user-asm tables
    // (parent_ids, vtable_ptrs, interface_ptrs). Without those slots
    // populated, the catch-time inheritance walk in __rt_exception_matches
    // sees a -1 parent for the thrown class and reports no match.
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
            callable_param_sigs,
            &function_variant_group_names,
            &global_constants,
            interfaces,
            &declared_traits,
            classes,
            enums,
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
        callable_param_sigs,
        &function_variant_group_names,
        interfaces,
        &declared_traits,
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

fn collect_declared_class_names(
    program: &Program,
    classes: &HashMap<String, ClassInfo>,
) -> Vec<String> {
    let mut user_names = Vec::new();
    collect_program_declared_names(
        program,
        classes,
        &mut HashSet::new(),
        &mut user_names,
        |stmt| match &stmt.kind {
            StmtKind::ClassDecl { name, .. } | StmtKind::EnumDecl { name, .. } => {
                Some(name.as_str())
            }
            _ => None,
        },
    );
    prepend_internal_names(classes.keys(), &user_names)
}

fn collect_declared_interface_names(
    program: &Program,
    interfaces: &HashMap<String, InterfaceInfo>,
) -> Vec<String> {
    let mut user_names = Vec::new();
    collect_program_declared_names(
        program,
        interfaces,
        &mut HashSet::new(),
        &mut user_names,
        |stmt| match &stmt.kind {
            StmtKind::InterfaceDecl { name, .. } => Some(name.as_str()),
            _ => None,
        },
    );
    prepend_internal_names(interfaces.keys(), &user_names)
}

fn collect_declared_trait_names(program: &Program) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl { name, .. } => {
                names.push(name.clone());
            }
            StmtKind::NamespaceBlock { body, .. } => {
                names.extend(collect_declared_trait_names(body));
            }
            _ => {}
        }
    }
    names
}

fn collect_program_declared_names<T>(
    program: &Program,
    known: &HashMap<String, T>,
    seen: &mut HashSet<String>,
    out: &mut Vec<String>,
    pick: impl Copy + Fn(&crate::parser::ast::Stmt) -> Option<&str>,
) {
    for stmt in program {
        match &stmt.kind {
            StmtKind::NamespaceBlock { body, .. } => {
                collect_program_declared_names(body, known, seen, out, pick);
            }
            _ => {
                let Some(name) = pick(stmt) else {
                    continue;
                };
                let key = crate::names::php_symbol_key(name);
                if known.contains_key(name) && seen.insert(key) {
                    out.push(name.to_string());
                }
            }
        }
    }
}

fn prepend_internal_names<'a>(
    known_names: impl Iterator<Item = &'a String>,
    user_names: &[String],
) -> Vec<String> {
    let user_keys: HashSet<String> = user_names
        .iter()
        .map(|name| crate::names::php_symbol_key(name))
        .collect();
    let mut names: Vec<String> = known_names
        .filter(|name| !user_keys.contains(&crate::names::php_symbol_key(name)))
        .cloned()
        .collect();
    names.sort();
    names.extend(user_names.iter().cloned());
    names
}

fn collect_x86_emitted_class_names(
    program: &Program,
    classes: &HashMap<String, ClassInfo>,
) -> HashSet<String> {
    let mut names = collect_required_class_names(program);
    if names.contains("Fiber") {
        names.insert("FiberError".to_string());
    }
    // Seed the throwable hierarchy unconditionally: json_encode /
    // json_decode / json_validate can throw JsonException at runtime
    // through JSON_THROW_ON_ERROR even when user code only catches a
    // wider type (e.g. `catch (Exception $e)`). Without these
    // descriptors in the user-asm tables, the catch-time inheritance
    // walk in __rt_exception_matches sees a -1 parent for the thrown
    // class and reports no match.
    for builtin in [
        "Throwable",
        "Error",
        "Exception",
        "RuntimeException",
        "JsonException",
    ] {
        names.insert(builtin.to_string());
    }
    for builtin in [
        "ReflectionAttribute",
        "ReflectionClass",
        "ReflectionMethod",
        "ReflectionProperty",
    ] {
        names.insert(builtin.to_string());
    }
    for factory in reflection::collect_attribute_factories(classes) {
        names.insert(factory.class_name);
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
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
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
        callable_param_sigs,
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
