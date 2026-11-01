//! Purpose:
//! Builtin SPL method reachability and dynamic-construction metadata.
//!
//! Called from:
//! - `crate::ir_lower::program`.
//!
//! Key details:
//! - Keeps program metadata deterministic and EIR lowering behavior unchanged.

use super::*;

/// Lowers the small builtin SPL method slice currently consumed by the EIR backend.
pub(super) fn lower_referenced_builtin_spl_methods(
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    // Each round scans only the bodies the previous round appended: what an already-scanned body
    // requires depends on its own instructions and the (fixed) class table alone, and every
    // round used to rescan the whole module (half of a Symfony build's lowering phase).
    let mut scanned = SplScanProgress::default();
    loop {
        let mut methods = referenced_builtin_spl_methods(module, &mut scanned);
        methods.sort();
        methods.dedup();
        // A key names one method, static or not: checking only the instance flavour re-lowered a
        // static body such as `Phar::running()` every round and never reached the fixpoint.
        methods.retain(|(class_name, method_key)| {
            !class_method_already_lowered(module, class_name, method_key, false)
                && !class_method_already_lowered(module, class_name, method_key, true)
                && !runtime_intrinsic_method_has_wrapper(class_name, method_key, false)
        });
        if methods.is_empty() {
            break;
        }

        let before = module.class_methods.len();
        for (class_name, method_key) in methods {
            lower_builtin_spl_method(
                &class_name,
                &method_key,
                module,
                check_result,
                constants,
                fiber_return_sigs,
            );
        }
        for method in module.class_methods.iter_mut().skip(before) {
            method.flags.is_synthetic = true;
        }
        if module.class_methods.len() == before {
            break;
        }
    }
}

/// Adds the builtin SPL surface a runtime include or an `eval` fragment can name.
///
/// WHY A SCAN CANNOT FIND THESE. Every other producer in `referenced_builtin_spl_methods` reads an
/// EIR instruction: an `ObjectNew` with a class immediate, a `MethodCall` with a receiver type.
/// Text that arrives at RUN TIME has no instruction to read — `include $path` lowers to one
/// `RuntimeCall(DynamicInclude)` and the `new ArrayObject(…)` inside the included file is not in
/// this module at all. So the surface has to be forced, exactly as
/// `ir_lower::builtin_datetime::lower_eval_date_alias_methods_if_needed` forces the date aliases.
///
/// WHY THE INTERPRETER CANNOT COVER FOR US, which is what makes SPL different from Reflection.
/// `ir_lower::reflection` notes that Reflection values owned by the bridge "are dispatched by
/// Magician, whose interpreter exposes the complete Reflection surface without forcing those
/// bodies into AOT" — the magician owns `crates/elephc-magician/src/interpreter/reflection/`. It
/// owns no SPL implementation. An `ArrayObject` the interpreter builds is an AOT object, allocated
/// by `__rt_new_by_name` and initialized by the AOT `__construct`, so if that body never reached
/// EIR there is nothing to run.
///
/// THE GATE IS THE BRIDGE FLAG. `required_runtime_features.eval_bridge` is the same authority the
/// date/time pass now reads, and it is already set for every route into the interpreter —
/// a bridge-requiring `eval`, `extract`, `spl_autoload_register`, and `DynamicInclude`. A program
/// with none of them keeps paying nothing: the loop below is skipped, and the class family was
/// never registered by `builtin_spl_classes::program_may_reference_spl` in the first place, so
/// `module.class_infos` holds none of these names to iterate.
///
/// WHAT IT FORCES is what the `DynamicObjectNewMixed` arm below already forces for `new $c`: the
/// constructor, plus `push_builtin_spl_metadata_methods` — the interface and vtable methods a
/// constructed object needs even when nothing calls them by name. Promising more than `new $c`
/// already promises would be a new claim; promising less would allocate an object whose runtime
/// class-id dispatch jumps through a null vtable slot.
///
/// Measured, `php -n` 8.5.6 as the oracle: a runtime-included file doing
/// `new ArrayObject([1,2,3])` then `new SplFixedArray(2)` printed
/// `Fatal error: eval() runtime failed: could not construct class "ArrayObject"` before the
/// checker gate opened — the ALLOCATION failing, one layer above this — and then a bare
/// `Fatal error: eval() runtime failed` with the gate open and this pass absent, which
/// `ELEPHC_EVAL_TRACE=1` resolves to `phase=native_constructor_error stage=construct`. PHP prints
/// `3;a;done`.
fn push_eval_bridge_builtin_spl_methods(methods: &mut Vec<(String, String)>, module: &Module) {
    if !module.required_runtime_features.eval_bridge {
        return;
    }
    let construct_key = php_method_key("__construct");
    for class_name in module.class_infos.keys() {
        // Every registered class the interpreter can allocate by name, not only the ones generic
        // `new $c` in compiled code may build: the bridge's `new_object` answers for any class in
        // the name table. `EmptyIterator` is "known" to dynamic `new` but not a candidate there,
        // so an interpreted `new EmptyIterator()` handed compiled code an object whose `valid()`
        // slot was null, and the first `foreach` over it jumped to address 0.
        if class_name.starts_with("__Elephc") {
            continue;
        }
        // The regex iterators call the `preg_*` helpers, an optional capability (`pcre2`) that
        // only `--with-regex` or a compiled regex call turns on. Forcing their constructors here
        // would make every eval-bridge program require that native package; without it, an
        // interpreted `new RegexIterator(...)` could not match anything anyway.
        if !module.required_runtime_features.regex && is_regex_iterator_class(class_name) {
            continue;
        }
        push_supported_builtin_spl_method_for_receiver(methods, module, class_name, &construct_key);
        push_builtin_spl_metadata_methods(methods, module, class_name);
        // Interpreted text calls the rest of the surface by name too (`$heap->insert()`,
        // `$storage->attach()`), and no EIR instruction names those calls: without their bodies
        // the bridge had no slot and reported "unsupported MethodCall ->insert()". Only the
        // classes whose allocation the bridge can reach are widened, so a program that never
        // opens the bridge pays nothing.
        if let Some(class_info) = module.class_infos.get(class_name) {
            let mut keys: Vec<&String> = class_info.methods.keys().collect();
            keys.sort();
            for key in keys {
                push_supported_builtin_spl_method_for_receiver(methods, module, class_name, key);
            }
        }
    }
}

/// Returns true for `RegexIterator` and `RecursiveRegexIterator`, whose bodies need the optional
/// regex runtime capability.
fn is_regex_iterator_class(class_name: &str) -> bool {
    matches!(
        php_symbol_key(class_name.trim_start_matches('\\')).as_str(),
        "regexiterator" | "recursiveregexiterator"
    )
}

/// How far [`referenced_builtin_spl_methods`] has scanned each body table of the module.
#[derive(Default)]
pub(super) struct SplScanProgress {
    /// Per body table, in the order `body_tables` lists them, the bodies already scanned.
    tables: [usize; 7],
    /// The eval-bridge surface is a function of the class table alone: pushed once.
    bridge_pushed: bool,
}

/// The module's body tables, in a fixed order.
fn body_tables(module: &Module) -> [&[crate::ir::Function]; 7] {
    [
        &module.functions,
        &module.class_methods,
        &module.closures,
        &module.fiber_wrappers,
        &module.callback_wrappers,
        &module.extern_callback_trampolines,
        &module.runtime_callable_invokers,
    ]
}

/// Finds builtin SPL methods whose symbols are required by the EIR lowered since `progress`.
pub(super) fn referenced_builtin_spl_methods(
    module: &Module,
    progress: &mut SplScanProgress,
) -> Vec<(String, String)> {
    let mut methods = Vec::new();
    if !progress.bridge_pushed {
        push_eval_bridge_builtin_spl_methods(&mut methods, module);
        progress.bridge_pushed = true;
    }
    // Answered once per scan instead of once per instruction: each walks every class.
    let mut mixed_method_keys: crate::fast_hash::FastSet<String> = Default::default();
    let mut dynamic_new_mixed = false;
    let mut dynamic_new_without_constructor_mixed = false;
    let tables = body_tables(module);
    let mut new_bodies = Vec::new();
    for (table, scanned) in tables.iter().zip(progress.tables.iter_mut()) {
        // A table never shrinks while this pass runs; if it ever did, rescan it whole.
        let start = if *scanned > table.len() { 0 } else { *scanned };
        new_bodies.extend(table[start..].iter());
        *scanned = table.len();
    }
    for function in new_bodies {
        for inst in &function.instructions {
            match inst.op {
                Op::StaticMethodCall => {
                    let Some((receiver, method)) = string_data_name(module, inst)
                        .and_then(|name| name.rsplit_once("::")) else {
                        continue;
                    };
                    let lexical_class = function.name.rsplit_once("::").map(|(class, _)| class);
                    let class_name = match receiver {
                        "parent" => lexical_class.and_then(|class| module.class_infos.get(class))
                            .and_then(|class| class.parent.as_deref()),
                        "self" | "static" => lexical_class,
                        name => Some(name.trim_start_matches('\\')),
                    };
                    if let Some(class_name) = class_name {
                        push_supported_builtin_spl_method_for_receiver(
                            &mut methods, module, class_name, &php_method_key(method),
                        );
                    }
                }
                Op::ObjectNew => {
                    if let Some(class_name) = class_data_name(module, inst) {
                        let construct_key = php_method_key("__construct");
                        push_supported_builtin_spl_method_for_receiver(
                            &mut methods,
                            module,
                            class_name,
                            &construct_key,
                        );
                        push_builtin_spl_metadata_methods(&mut methods, module, class_name);
                    }
                }
                Op::DynamicObjectNew => {
                    if let Some((fallback_class, required_parent)) =
                        dynamic_object_new_metadata_names(module, inst)
                    {
                        let construct_key = php_method_key("__construct");
                        if is_supported_builtin_spl_method(fallback_class, &construct_key) {
                            methods.push((fallback_class.to_string(), construct_key.clone()));
                        }
                        if is_supported_builtin_spl_method(required_parent, &construct_key) {
                            methods.push((required_parent.to_string(), construct_key));
                        }
                        push_builtin_spl_metadata_methods(&mut methods, module, fallback_class);
                        push_builtin_spl_metadata_methods(&mut methods, module, required_parent);
                    }
                }
                Op::DynamicObjectNewMixed => dynamic_new_mixed = true,
                Op::DynamicObjectNewWithoutConstructorMixed => {
                    dynamic_new_without_constructor_mixed = true;
                }
                Op::MethodCall | Op::NullsafeMethodCall => {
                    let Some(receiver) = inst.operands.first().copied() else {
                        continue;
                    };
                    let Some(receiver_ty) = function
                        .value(receiver)
                        .map(|value| value.php_type.codegen_repr())
                    else {
                        continue;
                    };
                    let Some(method_name) = string_data_name(module, inst) else {
                        continue;
                    };
                    let method_key = php_method_key(method_name);
                    match receiver_ty {
                        PhpType::Object(class_name) => {
                            let normalized = class_name.trim_start_matches('\\');
                            push_supported_builtin_spl_method_for_receiver(
                                &mut methods,
                                module,
                                normalized,
                                &method_key,
                            );
                        }
                        // A Mixed/Union receiver dispatches at runtime over every class whose
                        // flattened method set contains this name (mirrors `mixed_method_candidates`
                        // in the EIR backend). Register the builtin SPL implementation behind each
                        // candidate so its vtable slot is emitted; otherwise the runtime class-id
                        // dispatch jumps through a null vtable slot and segfaults. This covers
                        // method calls on a `mixed` value and on foreach values from object
                        // iterators (e.g. DirectoryIterator), which the EIR lowers as Mixed locals.
                        PhpType::Mixed | PhpType::Union(_) => {
                            mixed_method_keys.insert(method_key);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
    if dynamic_new_mixed || dynamic_new_without_constructor_mixed {
        let construct_key = php_method_key("__construct");
        for class_name in module.class_infos.keys() {
            if !is_dynamic_new_mixed_metadata_candidate(class_name) {
                continue;
            }
            if dynamic_new_mixed {
                push_supported_builtin_spl_method_for_receiver(
                    &mut methods,
                    module,
                    class_name,
                    &construct_key,
                );
            }
            push_builtin_spl_metadata_methods(&mut methods, module, class_name);
        }
    }
    if !mixed_method_keys.is_empty() {
        for (candidate_class, class_info) in &module.class_infos {
            for method_key in class_info.methods.keys() {
                if mixed_method_keys.contains(method_key) {
                    push_supported_builtin_spl_method_for_receiver(
                        &mut methods,
                        module,
                        candidate_class,
                        method_key,
                    );
                }
            }
        }
    }
    methods
}

/// Returns true when generic `new $class` can emit static metadata for this class.
pub(super) fn is_dynamic_new_mixed_metadata_candidate(class_name: &str) -> bool {
    if class_name.starts_with("__Elephc") {
        return false;
    }
    if supported_dynamic_new_builtin_class_name(class_name) {
        return true;
    }
    !known_dynamic_new_builtin_class_name(class_name)
}

/// Returns true for builtin classes with safe static allocation paths in generic dynamic new.
pub(super) fn supported_dynamic_new_builtin_class_name(class_name: &str) -> bool {
    matches!(
        php_symbol_key(class_name.trim_start_matches('\\')).as_str(),
        "arrayiterator"
            | "arrayobject"
            | "badfunctioncallexception"
            | "badmethodcallexception"
            | "callbackfilteriterator"
            | "domainexception"
            | "error"
            | "exception"
            | "fiber"
            | "fibererror"
            | "invalidargumentexception"
            | "iteratoriterator"
            | "jsonexception"
            | "lengthexception"
            | "logicexception"
            | "outofboundsexception"
            | "outofrangeexception"
            | "overflowexception"
            | "rangeexception"
            | "recursivecallbackfilteriterator"
            | "appenditerator"
            | "cachingiterator"
            | "emptyiterator"
            | "infiniteiterator"
            | "limititerator"
            | "multipleiterator"
            | "norewinditerator"
            | "recursivearrayiterator"
            | "recursiveiteratoriterator"
            | "splmaxheap"
            | "splminheap"
            | "splobjectstorage"
            | "splpriorityqueue"
            | "runtimeexception"
            | "spldoublylinkedlist"
            | "splfixedarray"
            | "splqueue"
            | "splstack"
            | "typeerror"
            | "underflowexception"
            | "unexpectedvalueexception"
            | "valueerror"
            | "stdclass"
    )
}

/// Returns true for builtin classes that generic dynamic new must not treat as user classes.
pub(super) fn known_dynamic_new_builtin_class_name(class_name: &str) -> bool {
    matches!(
        php_symbol_key(class_name.trim_start_matches('\\')).as_str(),
        "appenditerator"
            | "arrayiterator"
            | "arrayobject"
            | "badfunctioncallexception"
            | "badmethodcallexception"
            | "cachingiterator"
            | "callbackfilteriterator"
            | "directoryiterator"
            | "domainexception"
            | "emptyiterator"
            | "error"
            | "exception"
            | "fiber"
            | "fibererror"
            | "filesystemiterator"
            | "filteriterator"
            | "generator"
            | "globiterator"
            | "infiniteiterator"
            | "internaliterator"
            | "invalidargumentexception"
            | "iteratoriterator"
            | "jsonexception"
            | "lengthexception"
            | "limititerator"
            | "logicexception"
            | "multipleiterator"
            | "norewinditerator"
            | "outofboundsexception"
            | "outofrangeexception"
            | "overflowexception"
            | "parentiterator"
            | "phar"
            | "phardata"
            | "rangeexception"
            | "recursivearrayiterator"
            | "recursivecachingiterator"
            | "recursivecallbackfilteriterator"
            | "recursivedirectoryiterator"
            | "recursivefilteriterator"
            | "recursiveiteratoriterator"
            | "recursiveregexiterator"
            | "reflectionattribute"
            | "reflectionclass"
            | "reflectionmethod"
            | "reflectionparameter"
            | "reflectionproperty"
            | "regexiterator"
            | "runtimeexception"
            | "spldoublylinkedlist"
            | "splfileinfo"
            | "splfileobject"
            | "splfixedarray"
            | "splheap"
            | "splmaxheap"
            | "splminheap"
            | "splobjectstorage"
            | "splpriorityqueue"
            | "splqueue"
            | "splstack"
            | "spltempfileobject"
            | "typeerror"
            | "underflowexception"
            | "unexpectedvalueexception"
            | "valueerror"
            | "stdclass"
    )
}

/// Adds the supported builtin SPL method owner for a receiver class or one of its parents.
pub(super) fn push_supported_builtin_spl_method_for_receiver(
    methods: &mut Vec<(String, String)>,
    module: &Module,
    class_name: &str,
    method_key: &str,
) {
    let mut current = Some(class_name);
    while let Some(name) = current {
        if is_supported_builtin_spl_method(name, method_key) {
            methods.push((name.to_string(), method_key.to_string()));
            return;
        }
        current = module
            .class_infos
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
}

/// Returns the class-name immediate attached to an instruction.
pub(in crate::ir_lower) fn class_data_name<'a>(
    module: &'a Module,
    inst: &crate::ir::Instruction,
) -> Option<&'a str> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return None;
    };
    module
        .data
        .class_names
        .get(data.as_raw() as usize)
        .map(String::as_str)
}

/// Parses dynamic object factory fallback and required-parent metadata.
pub(in crate::ir_lower) fn dynamic_object_new_metadata_names<'a>(
    module: &'a Module,
    inst: &crate::ir::Instruction,
) -> Option<(&'a str, &'a str)> {
    class_data_name(module, inst)?.split_once('|')
}

/// Returns the string immediate attached to an instruction.
pub(in crate::ir_lower) fn string_data_name<'a>(
    module: &'a Module,
    inst: &crate::ir::Instruction,
) -> Option<&'a str> {
    let data = match inst.immediate {
        Some(Immediate::Data(data)) | Some(Immediate::ProfiledData { data, .. }) => data,
        _ => return None,
    };
    module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
}

/// Normalizes a PHP method name for metadata lookups.
pub(in crate::ir_lower) fn php_method_key(method_name: &str) -> String {
    crate::names::php_symbol_key(method_name)
}

/// Lowers a constructor-padding thunk for every (class, argument count) a dynamic `new` can reach.
///
/// Runs after `main` because the argument counts are read off the LOWERED instructions: a
/// `new $c($a)` site carries its operands, and codegen will later dispatch that site across every
/// candidate class. A class whose constructor declares more parameters than the site passes needs
/// its defaults filled in, and only a thunk lowered here can do that — see
/// `function::lower_dynamic_constructor_thunk`.
///
/// The pass is bounded by the sites actually present: one thunk per (class, argc) pair, and only
/// for classes that can pad. A program with no dynamic `new` lowers nothing.
pub(super) fn lower_dynamic_constructor_thunks(
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    let mut arg_counts = BTreeSet::new();
    for function in all_lowered_functions(module) {
        for inst in &function.instructions {
            // `new $class(...)` lowers to DynamicObjectNewMixed; `new static(...)` and a named
            // dynamic `new` lower to DynamicObjectNew. Both pick a constructor at run time, so
            // both need padding thunks for the defaults the call site omits (#797).
            if !matches!(inst.op, Op::DynamicObjectNewMixed | Op::DynamicObjectNew) {
                continue;
            }
            // A runtime argument container carries its arguments dynamically; there is no static
            // count to pad against, and codegen does not build fixed candidates for it either.
            if matches!(inst.immediate, Some(Immediate::Bool(true))) {
                continue;
            }
            arg_counts.insert(inst.operands.len().saturating_sub(1));
        }
    }
    if arg_counts.is_empty() {
        return;
    }
    // The MODULE's ClassInfo, not the checker's: codegen forms the thunk symbol from
    // `module.class_infos[..].class_id`, and looking the class up anywhere else risks naming a
    // symbol nothing will call.
    // One thunk per constructor IMPLEMENTATION: descendants that inherit a constructor share the
    // ancestor's thunk (see `function::dynamic_constructor_thunk_owner`).
    let mut owners = std::collections::BTreeMap::new();
    for class_name in module.class_infos.keys() {
        if !is_dynamic_new_mixed_metadata_candidate(class_name) {
            continue;
        }
        if let Some((owner_name, owner_info)) =
            function::dynamic_constructor_thunk_owner(&module.class_infos, class_name)
        {
            owners
                .entry(owner_name.to_string())
                .or_insert_with(|| owner_info.clone());
        }
    }
    for (class_name, class_info) in owners {
        for &provided_args in &arg_counts {
            function::lower_dynamic_constructor_thunk(
                &class_name,
                &class_info,
                provided_args,
                module,
                check_result,
                constants,
                fiber_return_sigs,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scan records how far it read each body table: asking again over the same module finds
    /// nothing new, and only a body appended since is read. Every fixpoint round used to rescan
    /// the whole module.
    #[test]
    fn spl_discovery_scans_each_body_once() {
        let dir = std::env::temp_dir();
        let mut module = crate::ir_lower::tests::lower_source_at_for_target(
            "<?php function f(mixed $it) { return $it->count(); } echo f(new ArrayObject([1, 2]));",
            &dir.join("spl_scan.php"),
            &dir,
            crate::codegen::platform::Target::parse("macos-aarch64").expect("target"),
        );
        let mut progress = SplScanProgress::default();
        let first = referenced_builtin_spl_methods(&module, &mut progress);
        assert!(
            first.iter().any(|(class, key)| class.eq_ignore_ascii_case("ArrayObject") && key == "count"),
            "{first:?}"
        );
        assert!(referenced_builtin_spl_methods(&module, &mut progress).is_empty());
        let caller = module.functions.iter().find(|function| function.name == "f").expect("f").clone();
        module.functions.push(caller);
        let again = referenced_builtin_spl_methods(&module, &mut progress);
        assert!(again.iter().any(|(_, key)| key == "count"), "an appended body is scanned: {again:?}");
    }
}
