//! Purpose:
//! Selects runtime class and interface metadata and validates emitted method coverage.
//!
//! Called from:
//! - `super::super::finalize_user_asm()` while assembling runtime metadata tables.
//!
//! Key details:
//! - Expands throwable, reflection, inheritance, interface, and dynamic-instanceof dependencies.

use super::*;

/// Returns class metadata trimmed to method symbols emitted by the EIR backend.
pub(in crate::codegen) fn runtime_class_infos(module: &Module) -> HashMap<String, ClassInfo> {
    let emitted_methods = emitted_class_method_keys(module);
    let emitted_property_init_thunks = module
        .functions
        .iter()
        .filter(|function| is_property_init_thunk_function(function))
        .map(|function| function.name.as_str())
        .collect::<HashSet<_>>();
    let mut classes = module.class_infos.clone();
    for class_info in classes.values_mut() {
        class_info
            .method_impl_classes
            .retain(|method_name, impl_class| {
                emitted_methods.contains(&(impl_class.clone(), method_name.clone(), false))
            });
        class_info
            .static_method_impl_classes
            .retain(|method_name, impl_class| {
                emitted_methods.contains(&(impl_class.clone(), method_name.clone(), true))
            });
        let property_init_thunk = format!("_class_propinit_{}", class_info.class_id);
        if !emitted_property_init_thunks.contains(property_init_thunk.as_str()) {
            class_info.defaults.fill(None);
        }
    }
    classes
}

/// Returns classes that EIR object allocation or named `instanceof` can reference at runtime.
pub(in crate::codegen) fn runtime_referenced_class_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
    if module.required_runtime_features.dom_bridge {
        names.insert("DOMException".to_string());
        names.extend(
            module
                .class_infos
                .keys()
                .filter(|class_name| {
                    crate::internal_extensions::is_native_wrapper_class(class_name)
                        || crate::internal_extensions::is_native_wrapper_descendant(
                            &module.class_infos,
                            class_name,
                        )
                        || crate::internal_extensions::is_native_value_object_class(class_name)
                })
                .cloned(),
        );
    }
    if module_contains_generator(module) {
        names.insert("Generator".to_string());
    }
    if module_uses_dynamic_instanceof(module) {
        names.extend(dynamic_instanceof_class_names(module));
    }
    for class_name in referenced_static_property_class_names(module) {
        if module.class_infos.contains_key(&class_name) {
            names.insert(class_name);
        }
    }
    for class_name in referenced_static_method_class_names(module) {
        if module.class_infos.contains_key(&class_name) {
            names.insert(class_name);
        }
    }
    for class_name in referenced_class_data_names(module) {
        if module.class_infos.contains_key(&class_name) {
            names.insert(class_name);
        }
    }
    for class_name in referenced_dynamic_object_new_class_names(module) {
        if module.class_infos.contains_key(&class_name) {
            names.insert(class_name);
        }
    }
    for class_name in referenced_class_name_lookup_builtin_names(module) {
        if module.class_infos.contains_key(&class_name) {
            names.insert(class_name);
        }
    }
    for class_name in referenced_stream_registration_class_names(module) {
        if let Some(canonical) = canonical_module_class_name(module, &class_name) {
            names.insert(canonical);
        }
    }
    for class_name in referenced_scoped_constant_class_names(module) {
        if module.class_infos.contains_key(&class_name) {
            names.insert(class_name);
        }
    }
    seed_runtime_throwable_class_names(module, &mut names);
    seed_runtime_stdclass_name(module, &mut names);
    seed_builtin_reflection_class_names(module, &mut names);
    expand_class_dependencies(&mut names, &module.class_infos);
    names
}

/// Returns whether emitted EIR can deserialize a runtime-selected declared class.
///
/// `unserialize()` resolves serialized class names and `allowed_classes` object
/// entries through dense class-id metadata, so its indirect runtime dispatch
/// requires every declared class to remain represented in those tables.
pub(in crate::codegen) fn module_uses_unserialize(module: &Module) -> bool {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
        .flat_map(|function| function.instructions.iter())
        .any(|inst| typed_builtin_target(inst) == Some(crate::ir::RuntimeFnId::Unserialize))
}

// The eager enum-singleton reachability scan that used to live here is gone.
// It existed only to keep `main`'s prologue from allocating a case object (and
// burning an object handle) for an enum user code never touched. Cases are now
// materialized lazily on first evaluation by `crate::codegen::enum_singletons`,
// so an unreferenced enum costs nothing and no reachability analysis is needed.

/// Adds builtin throwable classes that runtime helpers can materialize without EIR class references.
pub(in crate::codegen) fn seed_runtime_throwable_class_names(module: &Module, names: &mut HashSet<String>) {
    if names.contains("Fiber") && module.class_infos.contains_key("FiberError") {
        names.insert("FiberError".to_string());
    }
    // WHAT BELONGS IN THIS LIST. Only a class some helper can MATERIALIZE without an EIR
    // class reference — everything a program names is already covered by the referenced-name
    // scans above and by `expand_class_dependencies` below. The authority on "can be
    // materialized" is the class-id symbol table in `codegen_support::runtime::data::user`:
    // a helper stamps `[obj+0]` from a `_*_class_id` symbol, so a throwable with no such
    // symbol has no unreferenced producer anywhere in the runtime.
    for class_name in [
        "Throwable",
        "Error",               // _spl_error_class_id
        "TypeError",           // _spl_type_error_class_id
        "ValueError",          // _spl_value_error_class_id
        "ArithmeticError",     // _spl_arithmetic_error_class_id
        "DivisionByZeroError", // _spl_division_by_zero_error_class_id
        "JsonException",       // _json_exception_class_id
        // `JsonException extends Exception` DIRECTLY, as in reference PHP, so the ancestor
        // expansion brings Exception back regardless; naming it here only states the dependency
        // the catch-time walk relies on. (It also has `_exception_class_id` of its own.)
        "Exception",
    ] {
        if module.class_infos.contains_key(class_name) {
            names.insert(class_name.to_string());
        }
    }
    // ArgumentCountError, AssertionError and UnhandledMatchError have NO id symbol, and the
    // reason is the same in each case: nothing in elephc raises them.
    // - ArgumentCountError: reference PHP raises it at runtime for a bad builtin arity;
    //   elephc rejects that arity at COMPILE time (`error_class_hierarchy_tests` pins both
    //   halves of the divergence).
    // - AssertionError: `assert()` is not implemented — it is still an undefined function.
    // - UnhandledMatchError: a match with no arm and no default ends in
    //   `Terminator::Fatal` (`ir_lower::expr::match_expr`), not a throw. Reference PHP throws
    //   a catchable `UnhandledMatchError` there, so this is a real gap; closing it will add
    //   an EIR reference, which is exactly what makes the class survive this gate.
    // A program that names one of them still gets it — `throw new AssertionError(...)` is an
    // EIR reference. Only eval can conjure one from a string with nothing to scan, and the
    // eval constructor bridge emits helpers for the whole family (see
    // `codegen::eval_constructor_helpers::BUILTIN_THROWABLE_CONSTRUCTOR_CLASSES`).
    if module.required_runtime_features.eval_bridge {
        for class_name in ["ArgumentCountError", "AssertionError", "UnhandledMatchError"] {
            if module.class_infos.contains_key(class_name) {
                names.insert(class_name.to_string());
            }
        }
    }
    // THE SECOND HALF OF THE SAME GATE. `codegen_support::emitted_classes` decides which class
    // descriptors exist at all; this decides which of them the runtime tables carry. Both seed
    // the throwable hierarchy, so narrowing one alone changes nothing — measured, after gating
    // only the other: 7,990 lines before, 7,990 after.
    //
    // The condition matches that one exactly: these five are thrown by id from SPL container
    // helpers only. RuntimeException is one of them and has no other producer — it used to come
    // back through the ancestor expansion because `JsonException extends RuntimeException`, which
    // was elephc's own invention; reference PHP puts JsonException directly under Exception. See
    // `emitted_classes` for the full reasoning and for why getting it wrong is loud rather than
    // silent.
    if ["SplDoublyLinkedList", "SplFixedArray", "IteratorIterator"]
        .iter()
        .any(|name| module.class_infos.contains_key(*name))
    {
        for class_name in [
            "LogicException",
            "RuntimeException",
            "InvalidArgumentException",
            "OutOfBoundsException",
            "OutOfRangeException",
        ] {
            if module.class_infos.contains_key(class_name) {
                names.insert(class_name.to_string());
            }
        }
    }
    if module.class_infos.contains_key("ReflectionClass") {
        names.insert("ReflectionException".to_string());
    }
}

/// Adds builtin `stdClass` metadata for runtime helpers that materialize dynamic objects.
pub(in crate::codegen) fn seed_runtime_stdclass_name(module: &Module, names: &mut HashSet<String>) {
    if module.class_infos.contains_key("stdClass") {
        names.insert("stdClass".to_string());
    }
}

/// Adds builtin reflection classes whose objects can be materialized by metadata helpers.
pub(in crate::codegen) fn seed_builtin_reflection_class_names(module: &Module, names: &mut HashSet<String>) {
    for class_name in [
        "ReflectionAttribute",
        "ReflectionClass",
        "ReflectionObject",
        "ReflectionEnum",
        "ReflectionClassConstant",
        "ReflectionEnumBackedCase",
        "ReflectionEnumUnitCase",
        "ReflectionMethod",
        "ReflectionProperty",
        "ReflectionFunction",
        "ReflectionParameter",
        "ReflectionNamedType",
        "ReflectionUnionType",
        "ReflectionIntersectionType",
    ] {
        let emitted_natively = module.class_methods.iter().any(|function| {
            current_function_class(function)
                .is_some_and(|owner| php_symbol_key(owner) == php_symbol_key(class_name))
        });
        if module.class_infos.contains_key(class_name)
            && (module.required_runtime_features.eval_bridge || emitted_natively)
        {
            names.insert(class_name.to_string());
        }
    }
}

/// Returns true when any EIR function is emitted through the generator bridge.
pub(in crate::codegen) fn module_contains_generator(module: &Module) -> bool {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
        .any(|function| function.flags.is_generator)
}

/// Returns interface metadata needed by named `instanceof` and emitted class metadata.
pub(in crate::codegen) fn runtime_referenced_interfaces(
    module: &Module,
    class_names: &HashSet<String>,
) -> HashMap<String, InterfaceInfo> {
    let mut names = HashSet::new();
    if module.required_runtime_features.eval_bridge {
        names.extend(module.interface_infos.keys().cloned());
    }
    if module_uses_dynamic_instanceof(module) {
        names.extend(dynamic_instanceof_interface_names(module));
    }
    for class_name in referenced_class_data_names(module) {
        if module.interface_infos.contains_key(&class_name) {
            names.insert(class_name);
        }
    }
    for class_name in class_names {
        if let Some(class_info) = module.class_infos.get(class_name) {
            names.extend(class_info.interfaces.iter().cloned());
        }
    }
    expand_interface_dependencies(&mut names, &module.interface_infos);
    names
        .into_iter()
        .filter_map(|name| {
            module
                .interface_infos
                .get(&name)
                .cloned()
                .map(|info| (name, info))
        })
        .collect()
}

/// Returns whether any lowered EIR function uses dynamic `instanceof`.
pub(in crate::codegen) fn module_uses_dynamic_instanceof(module: &Module) -> bool {
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        if function
            .instructions
            .iter()
            .any(|inst| matches!(inst.op, Op::InstanceOfDynamic))
        {
            return true;
        }
    }
    false
}

/// Returns class names safe to include in dynamic lookup metadata for the current EIR slice.
pub(in crate::codegen) fn dynamic_instanceof_class_names(module: &Module) -> HashSet<String> {
    module
        .class_infos
        .keys()
        .filter(|name| class_metadata_supported_for_dynamic_instanceof(name, module))
        .cloned()
        .collect()
}

/// Returns interface names safe to include in dynamic lookup metadata for the current EIR slice.
pub(in crate::codegen) fn dynamic_instanceof_interface_names(module: &Module) -> HashSet<String> {
    module
        .interface_infos
        .keys()
        .filter(|name| {
            interface_metadata_supported_for_dynamic_instanceof(name, &module.interface_infos)
        })
        .cloned()
        .collect()
}

/// Returns true when class metadata can be emitted for dynamic `instanceof` lookup.
pub(in crate::codegen) fn class_metadata_supported_for_dynamic_instanceof(class_name: &str, module: &Module) -> bool {
    let emitted_methods = emitted_class_method_keys(module);
    let mut seen = HashSet::new();
    let mut current = Some(class_name);
    while let Some(name) = current {
        if !seen.insert(name.to_string()) {
            return false;
        }
        let Some(class_info) = module.class_infos.get(name) else {
            return false;
        };
        if !class_interfaces_supported_for_dynamic_instanceof(class_info, &module.interface_infos) {
            return false;
        }
        if !class_method_symbols_supported(
            class_info,
            name,
            false,
            &class_info.vtable_methods,
            &class_info.method_impl_classes,
            &emitted_methods,
        ) {
            return false;
        }
        if !class_method_symbols_supported(
            class_info,
            name,
            true,
            &class_info.static_vtable_methods,
            &class_info.static_method_impl_classes,
            &emitted_methods,
        ) {
            return false;
        }
        current = class_info.parent.as_deref();
    }
    true
}

/// Returns class-method symbols emitted by the EIR backend.
pub(in crate::codegen) fn emitted_class_method_keys(module: &Module) -> HashSet<(String, String, bool)> {
    let mut keys = eir_class_method_keys(module);
    for wrapper in intrinsic_method_wrapper_specs(module) {
        keys.insert((wrapper.class_name, wrapper.method_key, wrapper.is_static));
    }
    keys
}

/// Returns class-method symbols backed by actual lowered EIR functions.
pub(in crate::codegen) fn eir_class_method_keys(module: &Module) -> HashSet<(String, String, bool)> {
    module
        .class_methods
        .iter()
        .filter_map(|function| {
            let (class_name, method_name) = function.name.rsplit_once("::")?;
            Some((
                class_name.to_string(),
                crate::names::php_symbol_key(method_name),
                function.flags.is_static,
            ))
        })
        .collect()
}

/// Returns true when all vtable methods resolve to emitted EIR method symbols.
pub(in crate::codegen) fn class_method_symbols_supported(
    class_info: &ClassInfo,
    fallback_class: &str,
    is_static: bool,
    methods: &[String],
    impl_classes: &HashMap<String, String>,
    emitted_methods: &HashSet<(String, String, bool)>,
) -> bool {
    methods.iter().all(|method_name| {
        let impl_class = impl_classes
            .get(method_name)
            .map(String::as_str)
            .unwrap_or(fallback_class);
        let key = (impl_class.to_string(), method_name.clone(), is_static);
        emitted_methods.contains(&key)
            || (!is_static
                && class_info.methods.contains_key(method_name)
                && emitted_methods.contains(&(impl_class.to_string(), method_name.clone(), false)))
    })
}

/// Returns true when implemented interfaces do not require missing method wrappers.
pub(in crate::codegen) fn class_interfaces_supported_for_dynamic_instanceof(
    class_info: &ClassInfo,
    interfaces: &HashMap<String, InterfaceInfo>,
) -> bool {
    class_info
        .interfaces
        .iter()
        .all(|name| interface_metadata_supported_for_dynamic_instanceof(name, interfaces))
}

/// Returns true when interface metadata does not require wrapper symbols missing from EIR output.
pub(in crate::codegen) fn interface_metadata_supported_for_dynamic_instanceof(
    interface_name: &str,
    interfaces: &HashMap<String, InterfaceInfo>,
) -> bool {
    let mut seen = HashSet::new();
    let mut stack = vec![interface_name];
    while let Some(name) = stack.pop() {
        if !seen.insert(name.to_string()) {
            continue;
        }
        let Some(interface_info) = interfaces.get(name) else {
            return false;
        };
        if !interface_info.method_order.is_empty() {
            return false;
        }
        stack.extend(interface_info.parents.iter().map(String::as_str));
    }
    true
}

/// Adds parent classes needed by runtime class-id tables.
pub(in crate::codegen) fn expand_class_dependencies(names: &mut HashSet<String>, classes: &HashMap<String, ClassInfo>) {
    loop {
        let mut changed = false;
        let snapshot = names.iter().cloned().collect::<Vec<_>>();
        for class_name in snapshot {
            if let Some(parent) = classes
                .get(&class_name)
                .and_then(|class_info| class_info.parent.as_ref())
            {
                changed |= names.insert(parent.clone());
            }
        }
        if !changed {
            break;
        }
    }
}

/// Adds parent interfaces needed by runtime interface matching tables.
pub(in crate::codegen) fn expand_interface_dependencies(
    names: &mut HashSet<String>,
    interfaces: &HashMap<String, InterfaceInfo>,
) {
    loop {
        let mut changed = false;
        let snapshot = names.iter().cloned().collect::<Vec<_>>();
        for interface_name in snapshot {
            if let Some(interface_info) = interfaces.get(&interface_name) {
                for parent in &interface_info.parents {
                    changed |= names.insert(parent.clone());
                }
            }
        }
        if !changed {
            break;
        }
    }
}
