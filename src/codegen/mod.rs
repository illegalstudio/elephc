//! Purpose:
//! Coordinates assembly generation for complete programs and re-exports shared codegen helpers.
//! Builds class metadata, emits user code, and assembles the runtime-facing sections.
//!
//! Called from:
//! - `crate::pipeline::compile()` through `crate::codegen::generate()`
//!
//! Key details:
//! - Keeps frontend type metadata, runtime cache assumptions, and target-specific emission ordered before linking.

pub(crate) mod abi;
pub(crate) mod builtins;
pub(crate) mod cdylib;
pub(crate) mod callable_descriptor;
pub(crate) mod callable_dispatch;
pub(crate) mod runtime_callable_invoker;
mod callables;
mod class_methods;
mod property_init_thunks;
/// Codegen context module.
pub mod context;
pub(crate) mod data_section;
mod driver_support;
pub(crate) mod emit;
mod expr;
mod ffi;
mod fiber_sigs;
mod function_variants;
mod functions;
pub(crate) mod interface_wrappers;
mod main_emission;
/// Platform module.
pub mod platform;
mod prescan;
mod program_usage;
pub(crate) mod reflection;
pub(crate) mod runtime;
mod runtime_features;
pub(crate) mod sentinels;
mod stmt;
pub(crate) mod visibility;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

pub(crate) use functions::{emit_callback_wrapper, emit_extern_callback_trampoline};

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
    static DECLARED_TRAIT_USES: RefCell<HashMap<String, Vec<String>>> = RefCell::new(HashMap::new());
}

/// Sets the number of autoload rules registered.
pub fn set_autoload_rule_count(n: usize) {
    AUTOLOAD_RULE_COUNT.with(|c| c.set(n));
}

/// Returns the number of autoload rules registered.
pub fn autoload_rule_count() -> usize {
    AUTOLOAD_RULE_COUNT.with(|c| c.get())
}

/// Stores the declaration order of classes, interfaces, and traits so that
/// `declared_class_names()` / `declared_interface_names()` / `declared_trait_names()`
/// can reproduce it for class-id ordering in user assembly.
fn set_declared_name_order(classes: Vec<String>, interfaces: Vec<String>, traits: Vec<String>) {
    DECLARED_CLASS_NAMES.with(|names| *names.borrow_mut() = classes);
    DECLARED_INTERFACE_NAMES.with(|names| *names.borrow_mut() = interfaces);
    DECLARED_TRAIT_NAMES.with(|names| *names.borrow_mut() = traits);
}

/// Prepares declaration-order registries shared by legacy and EIR introspection builtins.
pub fn prepare_declared_name_order(
    program: &Program,
    classes: &HashMap<String, ClassInfo>,
    interfaces: &HashMap<String, InterfaceInfo>,
) {
    let declared_trait_order = collect_declared_trait_names(program);
    DECLARED_TRAIT_USES.with(|uses| *uses.borrow_mut() = collect_declared_trait_uses(program));
    set_declared_name_order(
        collect_declared_class_names(program, classes),
        collect_declared_interface_names(program, interfaces),
        declared_trait_order,
    );
}

/// Returns the ordered list of class names declared in the program,
/// including internal classes prepended by the compiler.
pub(crate) fn declared_class_names() -> Vec<String> {
    DECLARED_CLASS_NAMES.with(|names| names.borrow().clone())
}

/// Returns the ordered list of interface names declared in the program,
/// including internal interfaces prepended by the compiler.
pub(crate) fn declared_interface_names() -> Vec<String> {
    DECLARED_INTERFACE_NAMES.with(|names| names.borrow().clone())
}

/// Returns the ordered list of trait names declared in the program,
/// including internal traits prepended by the compiler.
pub(crate) fn declared_trait_names() -> Vec<String> {
    DECLARED_TRAIT_NAMES.with(|names| names.borrow().clone())
}

/// Provides the Declared trait uses helper used by the codegen module.
pub(crate) fn declared_trait_uses(name: &str) -> Vec<String> {
    let key = crate::names::php_symbol_key(name.trim_start_matches('\\'));
    DECLARED_TRAIT_USES.with(|uses| {
        uses.borrow()
            .iter()
            .find(|(candidate, _)| {
                crate::names::php_symbol_key(candidate.trim_start_matches('\\')) == key
            })
            .map(|(_, traits)| traits.clone())
            .unwrap_or_default()
    })
}

use crate::parser::ast::{Expr, ExprKind, Program, Stmt, StmtKind};
use crate::types::{
    ClassInfo, EnumInfo, ExternClassInfo, ExternFunctionSig, FunctionSig, InterfaceInfo,
    PackedClassInfo, PhpType, TypeEnv,
};
use class_methods::emit_class_methods;
use property_init_thunks::emit_property_init_thunk;
use data_section::DataSection;
use driver_support::align16;
use emit::Emitter;
use interface_wrappers::emit_interface_return_wrappers;
use main_emission::emit_main_and_finalize;
pub(crate) use driver_support::{
    emit_box_current_expr_value_as_mixed_for_container, emit_box_current_owned_value_as_mixed,
    emit_box_current_value_as_mixed, emit_box_iterable_value_for_mixed_container,
    emit_box_runtime_payload_as_mixed, emit_deferred_closures,
    emit_write_current_string_stderr, emit_write_literal_stderr,
    emit_normalized_hash_key, emit_release_pushed_refcounted_temp_after_array_push,
    runtime_value_tag,
};
pub(crate) use expr::arrays::emit_array_value_type_stamp;
pub(crate) use functions::{emit_fiber_wrapper, emit_generator_with_label};
pub(crate) use sentinels::{NULL_SENTINEL, UNINITIALIZED_TYPED_PROPERTY_SENTINEL};
pub use sentinels::{set_null_repr, NullRepr};
#[allow(unused_imports)]
pub use driver_support::{
    generate_runtime, generate_runtime_with_features, generate_runtime_with_features_pic,
};
pub use runtime_features::{
    required_libraries_for_runtime_features, runtime_features_for_program_and_classes,
};
pub use runtime_features::RuntimeFeatures;
use platform::Target;
pub(crate) use prescan::collect_constants;

/// Output artifact kind selected by the compiler's `--emit` flag.
///
/// `Executable` (default) produces a standalone native binary with a `_main`
/// entry point and a process-exit call at the end of top-level statements.
///
/// `Cdylib` produces a position-independent shared library (`.so` on Linux,
/// `.dylib` on macOS) loadable via `dlopen(3)` and friends. Cdylib output has
/// no `_main` entry, no implicit top-level execution at load time, and exposes
/// PHP functions marked with `#[Export]` under their unmangled PHP names plus
/// the `elephc_init` / `elephc_shutdown` / `elephc_last_error` / `elephc_free`
/// lifecycle entry points for embedding hosts.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Emit {
    Executable,
    Cdylib,
}
use prescan::{collect_global_var_names, collect_static_vars};
use program_usage::{
    collect_required_class_names, collect_required_class_names_in_stmts,
    program_has_dynamic_instanceof,
};

/// Generates user-code assembly for the target.
/// Returns the raw assembly string.
/// Generates the user assembly object for a checked and optimized program.
///
/// The returned assembly contains user functions, class metadata, `_main`, and
/// user-specific data, but not the shared cached runtime object. When
/// `requires_elephc_tls` is true, `_main` publishes the TLS staticlib entry
/// points before user code runs so dynamic URL helpers can call through them.
#[allow(clippy::too_many_arguments)]
pub fn generate_user_asm(
    program: &Program,
    global_env: &TypeEnv,
    functions: &HashMap<String, FunctionSig>,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
    callable_return_sigs: &HashMap<String, FunctionSig>,
    callable_array_return_sigs: &HashMap<String, FunctionSig>,
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
    requires_elephc_tls: bool,
    null_repr: NullRepr,
    emit: Emit,
    exported_functions: &HashMap<String, crate::exports::ExportedFunction>,
) -> String {
    sentinels::set_null_repr(null_repr);
    let mut emitter = match emit {
        Emit::Cdylib => Emitter::new_pic(target),
        Emit::Executable => Emitter::new(target),
    };
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
    DECLARED_TRAIT_USES.with(|uses| *uses.borrow_mut() = collect_declared_trait_uses(program));
    set_declared_name_order(
        collect_declared_class_names(program, classes),
        collect_declared_interface_names(program, interfaces),
        declared_trait_order,
    );

    // Emit user-defined functions before _main (skip extern functions)
    let function_variant_groups = function_variants::collect_function_variant_groups(program);
    let function_variant_group_names: HashSet<String> =
        function_variant_groups.keys().cloned().collect();
    let fiber_return_sigs = fiber_sigs::collect_fiber_return_sigs(program);
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
            callable_return_sigs,
            callable_array_return_sigs,
            &fiber_return_sigs,
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
    // Filter classes to those visibly used by the program so that test asm
    // stays compact; the filter must include every parent
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
    let emitted_class_names = if !program_has_dynamic_instanceof(program) {
        Some(collect_emitted_class_names(program, classes))
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
            callable_return_sigs,
            callable_array_return_sigs,
            &fiber_return_sigs,
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
        // Per-class property-default thunk (_class_propinit_<id>), invoked by
        // __rt_new_by_name so new $var() / registered wrappers + filters get
        // their declared property defaults. Same filtered/sorted class set as
        // the method emission above and the _class_propinit_ptrs table.
        emit_property_init_thunk(
            &mut emitter,
            &mut data,
            class_name,
            class_info,
            functions,
            callable_param_sigs,
            callable_return_sigs,
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

    // Cdylib emission appends C-ABI export trampolines and lifecycle symbols
    // after user functions so each `_fn_<name>` target the trampolines branch
    // into has already been emitted. Executable mode skips this step entirely.
    if matches!(emit, Emit::Cdylib) {
        let mut sorted_exports: Vec<&crate::exports::ExportedFunction> =
            exported_functions.values().collect();
        sorted_exports.sort_by(|a, b| a.name.cmp(&b.name));
        cdylib::emit_cdylib_exports(&mut emitter, target, &sorted_exports);
    }

    let user_asm = emit_main_and_finalize(
        emitter,
        data,
        program,
        global_env,
        functions,
        callable_param_sigs,
        callable_return_sigs,
        callable_array_return_sigs,
        &fiber_return_sigs,
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
        requires_elephc_tls,
        emit,
    );

    // ELF cdylibs hide every internal global so the artifact exports only its
    // public ABI (lifecycle entry points + #[Export] trampolines). Without
    // this, internal runtime state would be preemptible and two elephc modules
    // loaded into one process would alias each other's globals. Mach-O uses
    // two-level namespace binding, so macOS needs no directive.
    if matches!(emit, Emit::Cdylib) && target.platform == platform::Platform::Linux {
        let mut exported: std::collections::HashSet<String> = exported_functions
            .keys()
            .map(|name| target.extern_symbol(name))
            .collect();
        for lifecycle in [
            "elephc_init",
            "elephc_shutdown",
            "elephc_last_error",
            "elephc_free",
        ] {
            exported.insert(target.extern_symbol(lifecycle));
        }
        return visibility::append_hidden_directives(&user_asm, &exported);
    }
    user_asm
}

/// Collects user-declared class and enum names from the program AST, merges them
/// with internal class names, and returns the combined list in declaration order
/// with internal names prepended and sorted.
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

/// Collects user-declared interface names from the program AST, merges them
/// with internal interface names, and returns the combined list in declaration
/// order with internal names prepended and sorted.
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

/// Recursively collects user-declared trait names from the program AST,
/// including those inside namespace blocks, and returns them in declaration order.
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

/// Collects declared trait uses for the surrounding analysis or metadata result.
fn collect_declared_trait_uses(program: &Program) -> HashMap<String, Vec<String>> {
    let mut uses = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name, trait_uses, ..
            } => {
                uses.insert(
                    name.clone(),
                    trait_uses
                        .iter()
                        .flat_map(|use_decl| {
                            use_decl
                                .trait_names
                                .iter()
                                .map(|trait_name| trait_name.as_str().to_string())
                        })
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                uses.extend(collect_declared_trait_uses(body));
            }
            _ => {}
        }
    }
    uses
}

/// Helper for collecting declared names of a specific AST statement kind.
/// Walks the program (recursing into namespace blocks), asks the `pick` callback
/// to extract a name from each statement, and outputs it only if it exists in
/// `known` and hasn't been seen before (deduplicated by PHP symbol key).
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
                let is_known = known.contains_key(name)
                    || known.keys().any(|candidate| {
                        crate::names::php_symbol_key(candidate.trim_start_matches('\\')) == key
                    });
                if is_known && seen.insert(key) {
                    out.push(name.to_string());
                }
            }
        }
    }
}

/// Splits `known_names` into internal-only and user-declared by checking against
/// `user_names` (matched by PHP symbol key), sorts the internal names, and
/// appends the user names in their original order.
fn prepend_internal_names<'a>(
    known_names: impl Iterator<Item = &'a String>,
    user_names: &[String],
) -> Vec<String> {
    let user_keys: HashSet<String> = user_names
        .iter()
        .map(|name| crate::names::php_symbol_key(name))
        .collect();
    let mut names: Vec<String> = known_names
        .filter(|name| !is_internal_synthetic_class_name(name))
        .filter(|name| !user_keys.contains(&crate::names::php_symbol_key(name)))
        .cloned()
        .collect();
    names.sort();
    names.extend(user_names.iter().cloned());
    names
}

/// Returns true when internal synthetic class name.
fn is_internal_synthetic_class_name(name: &str) -> bool {
    crate::names::php_symbol_key(name).starts_with("__elephc")
}

/// Returns the set of class names that should be emitted in the
/// user-asm section. Starts from required classes, unconditionally includes
/// the throwable hierarchy (needed by runtime JSON helpers), reflection
/// classes, and attribute factories, then expands to cover the full
/// inheritance and implementation dependency chain.
fn collect_emitted_class_names(
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
        "TypeError",
        "ValueError",
        "Exception",
        "LogicException",
        "RuntimeException",
        "JsonException",
        "InvalidArgumentException",
        "OutOfBoundsException",
        "OutOfRangeException",
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
        // Only resolvable attribute classes are emitted; non-class attributes
        // are registered solely so `getArguments()` can return their arguments.
        if factory.resolvable {
            names.insert(factory.class_name);
        }
    }
    collect_dynamic_object_factory_classes(program, classes, &mut names);
    expand_emitted_class_dependencies(&mut names, classes);
    names
}

/// Repeatedly expands `names` by adding parent classes and all
/// method-implementation classes (both instance and static) until a
/// fixed point is reached, ensuring emitted vtables and interface
/// tables are complete.
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
            let previous_len = names.len();
            for method in &class_info.method_decls {
                collect_dynamic_object_factory_classes(&method.body, classes, names);
                collect_required_class_names_in_stmts(&method.body, names);
            }
            changed |= names.len() != previous_len;
        }
        if !changed {
            break;
        }
    }
}

/// Adds every concrete class that an internal dynamic object factory can instantiate.
fn collect_dynamic_object_factory_classes(
    stmts: &[Stmt],
    classes: &HashMap<String, ClassInfo>,
    names: &mut HashSet<String>,
) {
    for stmt in stmts {
        collect_dynamic_object_factory_classes_in_stmt(stmt, classes, names);
    }
}

/// Adds dynamic factory class dependencies found in a statement.
fn collect_dynamic_object_factory_classes_in_stmt(
    stmt: &Stmt,
    classes: &HashMap<String, ClassInfo>,
    names: &mut HashSet<String>,
) {
    match &stmt.kind {
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. } => methods
            .iter()
            .for_each(|method| collect_dynamic_object_factory_classes(&method.body, classes, names)),
        StmtKind::FunctionDecl { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => {
            collect_dynamic_object_factory_classes(body, classes, names);
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_dynamic_object_factory_classes(try_body, classes, names);
            for catch in catches {
                collect_dynamic_object_factory_classes(&catch.body, classes, names);
            }
            if let Some(finally_body) = finally_body {
                collect_dynamic_object_factory_classes(finally_body, classes, names);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            collect_dynamic_object_factory_classes(then_body, classes, names);
            if let Some(else_body) = else_body {
                collect_dynamic_object_factory_classes(else_body, classes, names);
            }
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_dynamic_object_factory_classes_in_expr(condition, classes, names);
            collect_dynamic_object_factory_classes(then_body, classes, names);
            for (condition, body) in elseif_clauses {
                collect_dynamic_object_factory_classes_in_expr(condition, classes, names);
                collect_dynamic_object_factory_classes(body, classes, names);
            }
            if let Some(else_body) = else_body {
                collect_dynamic_object_factory_classes(else_body, classes, names);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            collect_dynamic_object_factory_classes_in_expr(condition, classes, names);
            collect_dynamic_object_factory_classes(body, classes, names);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_dynamic_object_factory_classes_in_stmt(init, classes, names);
            }
            if let Some(condition) = condition {
                collect_dynamic_object_factory_classes_in_expr(condition, classes, names);
            }
            if let Some(update) = update {
                collect_dynamic_object_factory_classes_in_stmt(update, classes, names);
            }
            collect_dynamic_object_factory_classes(body, classes, names);
        }
        StmtKind::Foreach { array, body, .. } => {
            collect_dynamic_object_factory_classes_in_expr(array, classes, names);
            collect_dynamic_object_factory_classes(body, classes, names);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            collect_dynamic_object_factory_classes_in_expr(subject, classes, names);
            for (patterns, body) in cases {
                for pattern in patterns {
                    collect_dynamic_object_factory_classes_in_expr(pattern, classes, names);
                }
                collect_dynamic_object_factory_classes(body, classes, names);
            }
            if let Some(default) = default {
                collect_dynamic_object_factory_classes(default, classes, names);
            }
        }
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::Return(Some(expr))
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::PropertyAssign { value: expr, .. }
        | StmtKind::PropertyArrayPush { value: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. } => {
            collect_dynamic_object_factory_classes_in_expr(expr, classes, names);
        }
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            collect_dynamic_object_factory_classes_in_expr(index, classes, names);
            collect_dynamic_object_factory_classes_in_expr(value, classes, names);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            collect_dynamic_object_factory_classes_in_expr(target, classes, names);
            collect_dynamic_object_factory_classes_in_expr(value, classes, names);
        }
        _ => {}
    }
}

/// Adds dynamic factory class dependencies found in an expression.
fn collect_dynamic_object_factory_classes_in_expr(
    expr: &Expr,
    classes: &HashMap<String, ClassInfo>,
    names: &mut HashSet<String>,
) {
    match &expr.kind {
        ExprKind::NewDynamicObject {
            class_name,
            required_parent,
            args,
            ..
        } => {
            collect_dynamic_factory_descendants(required_parent.as_str(), classes, names);
            collect_dynamic_object_factory_classes_in_expr(class_name, classes, names);
            for arg in args {
                collect_dynamic_object_factory_classes_in_expr(arg, classes, names);
            }
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_dynamic_object_factory_classes_in_expr(left, classes, names);
            collect_dynamic_object_factory_classes_in_expr(right, classes, names);
        }
        ExprKind::InstanceOf { value, target } => {
            collect_dynamic_object_factory_classes_in_expr(value, classes, names);
            if let crate::parser::ast::InstanceOfTarget::Expr(expr) = target {
                collect_dynamic_object_factory_classes_in_expr(expr, classes, names);
            }
        }
        ExprKind::Negate(expr)
        | ExprKind::Not(expr)
        | ExprKind::BitNot(expr)
        | ExprKind::Throw(expr)
        | ExprKind::ErrorSuppress(expr)
        | ExprKind::Print(expr)
        | ExprKind::Spread(expr)
        | ExprKind::Cast { expr, .. }
        | ExprKind::PtrCast { expr, .. } => {
            collect_dynamic_object_factory_classes_in_expr(expr, classes, names);
        }
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            collect_dynamic_object_factory_classes_in_expr(value, classes, names);
            collect_dynamic_object_factory_classes_in_expr(default, classes, names);
        }
        ExprKind::Pipe { value, callable } => {
            collect_dynamic_object_factory_classes_in_expr(value, classes, names);
            collect_dynamic_object_factory_classes_in_expr(callable, classes, names);
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            collect_dynamic_object_factory_classes(prelude, classes, names);
            collect_dynamic_object_factory_classes_in_expr(target, classes, names);
            collect_dynamic_object_factory_classes_in_expr(value, classes, names);
            if let Some(result_target) = result_target {
                collect_dynamic_object_factory_classes_in_expr(result_target, classes, names);
            }
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => {
            for arg in args {
                collect_dynamic_object_factory_classes_in_expr(arg, classes, names);
            }
        }
        ExprKind::NewDynamic { name_expr, args } => {
            for class_name in expr::objects::supported_dynamic_new_builtin_class_names() {
                if classes.contains_key(*class_name) {
                    names.insert((*class_name).to_string());
                }
            }
            collect_dynamic_object_factory_classes_in_expr(name_expr, classes, names);
            for arg in args {
                collect_dynamic_object_factory_classes_in_expr(arg, classes, names);
            }
        }
        ExprKind::ExprCall { callee, args } => {
            collect_dynamic_object_factory_classes_in_expr(callee, classes, names);
            for arg in args {
                collect_dynamic_object_factory_classes_in_expr(arg, classes, names);
            }
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_dynamic_object_factory_classes_in_expr(item, classes, names);
            }
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                collect_dynamic_object_factory_classes_in_expr(key, classes, names);
                collect_dynamic_object_factory_classes_in_expr(value, classes, names);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_dynamic_object_factory_classes_in_expr(subject, classes, names);
            for (patterns, value) in arms {
                for pattern in patterns {
                    collect_dynamic_object_factory_classes_in_expr(pattern, classes, names);
                }
                collect_dynamic_object_factory_classes_in_expr(value, classes, names);
            }
            if let Some(default) = default {
                collect_dynamic_object_factory_classes_in_expr(default, classes, names);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_dynamic_object_factory_classes_in_expr(array, classes, names);
            collect_dynamic_object_factory_classes_in_expr(index, classes, names);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_dynamic_object_factory_classes_in_expr(condition, classes, names);
            collect_dynamic_object_factory_classes_in_expr(then_expr, classes, names);
            collect_dynamic_object_factory_classes_in_expr(else_expr, classes, names);
        }
        ExprKind::Closure { body, .. } => collect_dynamic_object_factory_classes(body, classes, names),
        ExprKind::NamedArg { value, .. } => {
            collect_dynamic_object_factory_classes_in_expr(value, classes, names);
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            collect_dynamic_object_factory_classes_in_expr(object, classes, names);
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            collect_dynamic_object_factory_classes_in_expr(object, classes, names);
            collect_dynamic_object_factory_classes_in_expr(property, classes, names);
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            collect_dynamic_object_factory_classes_in_expr(object, classes, names);
            for arg in args {
                collect_dynamic_object_factory_classes_in_expr(arg, classes, names);
            }
        }
        ExprKind::FirstClassCallable(crate::parser::ast::CallableTarget::Method {
            object,
            ..
        }) => collect_dynamic_object_factory_classes_in_expr(object, classes, names),
        ExprKind::BufferNew { len, .. } => {
            collect_dynamic_object_factory_classes_in_expr(len, classes, names);
        }
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                collect_dynamic_object_factory_classes_in_expr(key, classes, names);
            }
            if let Some(value) = value {
                collect_dynamic_object_factory_classes_in_expr(value, classes, names);
            }
        }
        ExprKind::YieldFrom(inner) => {
            collect_dynamic_object_factory_classes_in_expr(inner, classes, names);
        }
        _ => {}
    }
}

/// Adds every known class that can satisfy an internal dynamic factory parent constraint.
fn collect_dynamic_factory_descendants(
    required_parent: &str,
    classes: &HashMap<String, ClassInfo>,
    names: &mut HashSet<String>,
) {
    for class_name in classes.keys() {
        if emitted_class_descends_from(class_name, required_parent, classes) {
            names.insert(class_name.clone());
        }
    }
}

/// Returns true if `class_name` is the required class or extends it.
fn emitted_class_descends_from(
    class_name: &str,
    required_parent: &str,
    classes: &HashMap<String, ClassInfo>,
) -> bool {
    let mut current = Some(class_name);
    while let Some(name) = current {
        if crate::names::php_symbol_key(name.trim_start_matches('\\'))
            == crate::names::php_symbol_key(required_parent.trim_start_matches('\\'))
        {
            return true;
        }
        current = classes.get(name).and_then(|info| info.parent.as_deref());
    }
    false
}

/// Generates complete target assembly including runtime.
/// Returns tuple of (user_asm, full_asm_with_runtime).
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub fn generate(
    program: &Program,
    global_env: &TypeEnv,
    functions: &HashMap<String, FunctionSig>,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
    callable_return_sigs: &HashMap<String, FunctionSig>,
    callable_array_return_sigs: &HashMap<String, FunctionSig>,
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
    requires_elephc_tls: bool,
    null_repr: NullRepr,
) -> (String, String) {
    let user_asm = generate_user_asm(
        program,
        global_env,
        functions,
        callable_param_sigs,
        callable_return_sigs,
        callable_array_return_sigs,
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
        requires_elephc_tls,
        null_repr,
        Emit::Executable,
        &HashMap::new(),
    );
    let runtime_features = runtime_features_for_program_and_classes(program, classes);
    let runtime_asm = generate_runtime_with_features(heap_size, target, runtime_features);

    (user_asm, runtime_asm)
}
