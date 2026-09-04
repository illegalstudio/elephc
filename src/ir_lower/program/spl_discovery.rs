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
    loop {
        let mut methods = referenced_builtin_spl_methods(module);
        methods.sort();
        methods.dedup();
        methods.retain(|(class_name, method_key)| {
            !class_method_already_lowered(module, class_name, method_key, false)
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

/// Finds builtin SPL methods whose symbols are required by already-lowered EIR.
pub(super) fn referenced_builtin_spl_methods(module: &Module) -> Vec<(String, String)> {
    let mut methods = Vec::new();
    // A DYNAMIC call — `$obj->$name()` — carries no method name for the walk below to read, so
    // nothing was discovered and the dispatch ladder came up empty: MEASURED,
    // `$info->$call()` over `["getFilename", "getSize"]` died with `callable array did not
    // resolve to an invokable target` where php answers both. Calling each name STATICALLY first
    // made the same loop work, which is what said the ladder is bounded by what was EMITTED.
    //
    // The widening is bounded by the classes the program CONSTRUCTS: a program with no dynamic
    // invoke pays nothing, and one that has them pays only for the classes it built.
    if module_has_dynamic_invoke(module) {
        for class_name in constructed_builtin_spl_classes(module) {
            push_every_supported_builtin_spl_method(&mut methods, module, &class_name);
        }
    }
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
        for inst in &function.instructions {
            match inst.op {
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
                Op::DynamicObjectNewMixed => {
                    let construct_key = php_method_key("__construct");
                    for class_name in module.class_infos.keys() {
                        if !is_dynamic_new_mixed_metadata_candidate(class_name) {
                            continue;
                        }
                        push_supported_builtin_spl_method_for_receiver(
                            &mut methods,
                            module,
                            class_name,
                            &construct_key,
                        );
                        push_builtin_spl_metadata_methods(&mut methods, module, class_name);
                    }
                }
                Op::DynamicObjectNewWithoutConstructorMixed => {
                    for class_name in module.class_infos.keys() {
                        if !is_dynamic_new_mixed_metadata_candidate(class_name) {
                            continue;
                        }
                        push_builtin_spl_metadata_methods(&mut methods, module, class_name);
                    }
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
                            for (candidate_class, class_info) in &module.class_infos {
                                if class_info.methods.contains_key(&method_key) {
                                    push_supported_builtin_spl_method_for_receiver(
                                        &mut methods,
                                        module,
                                        candidate_class,
                                        &method_key,
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
    methods
}

/// Reports whether any function in the module invokes a callable whose target is decided at run
/// time — `$obj->$name()`, `call_user_func([$obj, $name])`, and the closure forms.
fn module_has_dynamic_invoke(module: &Module) -> bool {
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
            function.instructions.iter().any(|inst| {
                if !matches!(inst.op, Op::CallableDescriptorInvoke) {
                    return false;
                }
                // Only a callable ARRAY can name a method at run time. A closure call reaches the
                // same opcode and names none, so accepting it would emit every method of every
                // class the program built for a program that never dispatches on a name — and one
                // of those bodies (`SplFileObject::fscanf`) needs an engine the program did not
                // ask for, which is a link failure rather than a diagnostic.
                inst.operands.first().copied().is_some_and(|callable| {
                    matches!(
                        function.value(callable).map(|value| &value.php_type),
                        Some(PhpType::Array(_)) | Some(PhpType::AssocArray { .. })
                    )
                })
            })
        })
}

/// Every builtin SPL class the module builds an instance of, parents included.
fn constructed_builtin_spl_classes(module: &Module) -> Vec<String> {
    let mut names = Vec::new();
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
        for inst in &function.instructions {
            if !matches!(inst.op, Op::ObjectNew) {
                continue;
            }
            let Some(class_name) = class_data_name(module, inst) else {
                continue;
            };
            let mut current = Some(class_name);
            while let Some(name) = current {
                if !names.iter().any(|seen: &String| seen == name) {
                    names.push(name.to_string());
                }
                current = module
                    .class_infos
                    .get(name)
                    .and_then(|class_info| class_info.parent.as_deref());
            }
        }
    }
    names
}

/// Registers every method of `class_name` the backend can serve, for a ladder that cannot name
/// the one it wants.
fn push_every_supported_builtin_spl_method(
    methods: &mut Vec<(String, String)>,
    module: &Module,
    class_name: &str,
) {
    let Some(class_info) = module.class_infos.get(class_name) else {
        return;
    };
    let mut keys = class_info.methods.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    for method_key in keys {
        if is_supported_builtin_spl_method(class_name, &method_key) {
            methods.push((class_name.to_string(), method_key));
        }
    }
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
    push_builtin_spl_method_overrides(methods, module, class_name, method_key);
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

/// Pushes the same method on every builtin SPL class BELOW `class_name` that the program builds.
///
/// The walk above climbs to the class that DECLARES the method, which is the right answer for the
/// receiver's static type and the wrong one for the object in it. A call through an
/// `SplFileObject` parameter holding an `SplTempFileObject` dispatches through the vtable, and the
/// subclass's own slot was never lowered: MEASURED, `function walk(SplFileObject $o) { $o->eof(); }`
/// called with an `SplTempFileObject` SEGFAULTED on a null slot, and adding an unrelated
/// `$temp->eof()` elsewhere in the same program made it work — which is what said the gap was
/// emission and not dispatch.
///
/// Bounded by what the program CONSTRUCTS, because a virtual call can only land on a class
/// something instantiated. A program that builds no SPL subclass pays a walk over its class table
/// and emits nothing.
fn push_builtin_spl_method_overrides(
    methods: &mut Vec<(String, String)>,
    module: &Module,
    class_name: &str,
    method_key: &str,
) {
    for candidate in constructed_builtin_spl_classes(module) {
        if candidate == class_name
            || !is_supported_builtin_spl_method(&candidate, method_key)
            || !builtin_spl_class_descends_from(module, &candidate, class_name)
        {
            continue;
        }
        methods.push((candidate, method_key.to_string()));
    }
}

/// Reports whether `candidate` has `ancestor` somewhere above it.
fn builtin_spl_class_descends_from(module: &Module, candidate: &str, ancestor: &str) -> bool {
    let mut current = module
        .class_infos
        .get(candidate)
        .and_then(|class_info| class_info.parent.as_deref());
    while let Some(name) = current {
        if name == ancestor {
            return true;
        }
        current = module
            .class_infos
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
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
    let Some(Immediate::Data(data)) = inst.immediate else {
        return None;
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
            if inst.op != Op::DynamicObjectNewMixed {
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
    let classes = module
        .class_infos
        .iter()
        .map(|(name, info)| (name.clone(), info.clone()))
        .collect::<Vec<_>>();
    for (class_name, class_info) in classes {
        if !is_dynamic_new_mixed_metadata_candidate(&class_name) {
            continue;
        }
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
