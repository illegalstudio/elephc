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
pub(in crate::codegen) fn runtime_class_infos(module: &Module) -> crate::fast_hash::FastMap<String, ClassInfo> {
    let emitted_methods = emitted_class_method_keys(module);
    let emitted_property_init_thunks = module
        .functions
        .iter()
        .filter(|function| is_property_init_thunk_function(function))
        .map(|function| function.name.as_str())
        .collect::<HashSet<_>>();
    // Do not clone the entire class map at once: on a framework-scale program each
    // `method_decls` entry still owns its AST body, so a whole-map clone doubles the
    // largest compiler graph during code generation. Runtime metadata needs method names,
    // visibility and signatures, but not ordinary method bodies. Build one trimmed clone at a
    // time; keep `__debugInfo` intact because its runtime projection inspects that body.
    let mut classes = crate::fast_hash::FastMap::default();
    for (name, original) in &module.class_infos {
        let mut class_info = original.clone();
        for method in &mut class_info.method_decls {
            if !method.name.eq_ignore_ascii_case("__debugInfo") {
                method.body.clear();
            }
        }
        classes.insert(name.clone(), class_info);
    }
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
    (*module.runtime_referenced_class_names(|| scan_runtime_referenced_class_names(module))).clone()
}

/// Walks the module for [`runtime_referenced_class_names`].
fn scan_runtime_referenced_class_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
    if module.functions.iter().any(|function| function.signature.as_ref().is_some_and(|signature|
        signature.param_attributes.iter().flatten().flat_map(|group| &group.attributes)
            .any(|attribute| attribute.name.as_canonical().eq_ignore_ascii_case("SensitiveParameter")))) {
        names.insert("SensitiveParameterValue".to_string());
    }
    // `module_contains_generator` only sees a `yield` in THIS module's own compiled AST. The
    // eval bridge (an `include`/`eval` whose target is only known at run time) can execute a
    // generator function or method the compiler never parsed, exactly like
    // `runtime_referenced_interfaces` already widens to every known interface once the bridge is
    // in play. Without this, `values.new_object("Generator")` found no registered class, and
    // `eval_generator_new` turned that into a bare `RuntimeFatal` the outer `Call`/`MethodCall`
    // then mislabeled as "unsupported ... expression".
    if module_contains_generator(module) || module.required_runtime_features.eval_bridge {
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
    if module_uses_dynamic_class_name_constant_lookup(module) {
        names.extend(
            module
                .class_infos
                .iter()
                .filter(|(_, class_info)| !class_info.constants.is_empty())
                .map(|(class_name, _)| class_name.clone()),
        );
    }
    if module_binds_closure_to_runtime_scope(module) {
        // A class-name string scope resolves through the name table, so every user class a
        // bound closure could be scoped to must be addressable there.
        names.extend(
            module
                .class_infos
                .iter()
                .filter(|(_, class_info)| !class_info.is_internal)
                .map(|(class_name, _)| class_name.clone()),
        );
    }
    seed_runtime_throwable_class_names(module, &mut names);
    seed_runtime_stdclass_name(module, &mut names);
    seed_builtin_reflection_class_names(module, &mut names);
    expand_class_dependencies(&mut names, &module.class_infos);
    names
}

/// Returns whether a `Closure::bind()` / `bindTo()` takes its scope from a run-time value, which
/// may be a class-name string that the class name table must resolve.
fn module_binds_closure_to_runtime_scope(module: &Module) -> bool {
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
        .any(|instruction| {
            instruction.op == Op::ClosureBind
                && matches!(
                    instruction.immediate,
                    Some(crate::ir::Immediate::ClosureBindObjectScope(_))
                )
        })
}

/// Returns whether EIR needs name metadata for a runtime `$class::CONSTANT` lookup.
///
/// A class referenced only by `SomeClass::class` has no object allocation or static-member
/// opcode, so ordinary reachability intentionally omits it. Once the class string feeds a
/// dynamic constant access, however, every already-known class that declares constants must be
/// addressable by the shared name-to-id table; no extra source files are introduced here.
fn module_uses_dynamic_class_name_constant_lookup(module: &Module) -> bool {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
        .any(|function| {
            function
                .instructions
                .iter()
                .any(|instruction| {
                    instruction.op == Op::ClassNameToId
                        // `constant($name)` resolves a `Class::NAME` through the same table, to
                        // tell `Undefined constant C::X` from `Class "C" not found`.
                        || matches!(
                            instruction.immediate,
                            Some(crate::ir::Immediate::RuntimeCall(
                                crate::ir::RuntimeCallTarget::Function(
                                    crate::ir::RuntimeFnId::Constant
                                )
                            ))
                        )
                })
        })
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
        "ArgumentCountError",  // _spl_argument_count_error_class_id, including dynamic mbstring calls
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
    // AssertionError and UnhandledMatchError have no id symbol, and the
    // reason is the same in each case: nothing in elephc raises them.
    // - AssertionError: `assert()` is not implemented — it is still an undefined function.
    // - UnhandledMatchError: a match with no arm and no default ends in
    //   `Terminator::Fatal` (`ir_lower::expr::match_expr`), not a throw. Reference PHP throws
    //   a catchable `UnhandledMatchError` there, so this is a real gap; closing it will add
    //   an EIR reference, which is exactly what makes the class survive this gate.
    // A program that names one of them still gets it — `throw new AssertionError(...)` is an
    // EIR reference. Only eval can conjure one from a string with nothing to scan — and eval
    // can conjure ANY of them, which is why the clause below is the whole family and not the
    // three names that used to sit here.
    //
    // THE ALLOCATION HALF OF THE EVAL CONSTRUCTOR BRIDGE. `eval_constructor_helpers` emits a
    // constructor helper for every name in BUILTIN_THROWABLE_CONSTRUCTOR_CLASSES under exactly
    // this gate. Constructing needs two things: a constructor, and something to construct. The
    // second is this table — `runtime_referenced_class_names` becomes `allowed_class_names`,
    // which is what `runtime::data::user` retains before emitting `_classes_by_name`, the table
    // `__rt_new_by_name` scans. Ship one half and the interpreter's
    // `throw new RuntimeException(...)` allocates null, `RuntimeValueOps::new_object` maps that
    // to `EvalStatus::RuntimeFatal`, and the bridge reports fatal status 2 where PHP throws a
    // catchable Throwable that the very next `catch (\Throwable $e)` would have taken.
    //
    // Measured before the fix, `php -n` 8.5.6 as the oracle: a runtime-included file doing
    // `throw new RuntimeException("boom")` inside `try { } catch (\Throwable $e) { }` printed
    // `Fatal error: eval() runtime failed` and exited 1, where PHP printed the caught message
    // and exited 0. `new Exception` worked in the same file, because the eight unconditional
    // seeds above already carried it — which is what made the divergence look class-specific.
    // Reading the list from the constructor side is the point: adding a 26th class there now
    // brings its metadata along instead of shipping a constructor with nothing to construct.
    if module.required_runtime_features.eval_bridge {
        for class_name in crate::codegen::eval_constructor_helpers::BUILTIN_THROWABLE_CONSTRUCTOR_CLASSES {
            if module.class_infos.contains_key(*class_name) {
                names.insert((*class_name).to_string());
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
    // One walk, not one per name: this used to scan every class method for EACH of the
    // fourteen names below, allocating a lowercased owner AND a lowercased name per method.
    let native_owners: crate::fast_hash::FastSet<String> = module
        .class_methods
        .iter()
        .filter_map(|function| current_function_class(function).map(php_symbol_key))
        .collect();
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
        let emitted_natively = native_owners.contains(&php_symbol_key(class_name));
        if module.class_infos.contains_key(class_name)
            && (module.required_runtime_features.eval_bridge || emitted_natively)
        {
            names.insert(class_name.to_string());
        }
    }
    // ReflectionEnum owner construction eagerly materializes one internal case object per enum
    // case, even when user code never calls getCases(). Keep both layouts available; only one
    // family can be instantiated for a given enum, depending on whether it is backed.
    if names.contains("ReflectionEnum") {
        for case_class in ["ReflectionEnumBackedCase", "ReflectionEnumUnitCase"] {
            if module.class_infos.contains_key(case_class) {
                names.insert(case_class.to_string());
            }
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
    let emitted_methods = emitted_class_method_keys(module);
    module
        .class_infos
        .keys()
        .filter(|name| {
            class_metadata_supported_for_dynamic_instanceof_with_methods(
                name,
                module,
                &emitted_methods,
            )
        })
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

/// Returns whether one class is safe for dynamic lookup using precomputed emitted methods.
fn class_metadata_supported_for_dynamic_instanceof_with_methods(
    class_name: &str,
    module: &Module,
    emitted_methods: &HashSet<(String, String, bool)>,
) -> bool {
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
            emitted_methods,
        ) {
            return false;
        }
        if !class_method_symbols_supported(
            class_info,
            name,
            true,
            &class_info.static_vtable_methods,
            &class_info.static_method_impl_classes,
            emitted_methods,
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
pub(in crate::codegen) fn class_method_symbols_supported<S: std::hash::BuildHasher>(
    class_info: &ClassInfo,
    fallback_class: &str,
    is_static: bool,
    methods: &[String],
    impl_classes: &std::collections::HashMap<String, String, S>,
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
pub(in crate::codegen) fn expand_class_dependencies(names: &mut HashSet<String>, classes: &crate::fast_hash::FastMap<String, ClassInfo>) {
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
