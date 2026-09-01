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
                Op::ObjectNewWithoutConstructor => {
                    if let Some(class_name) = class_data_name(module, inst) {
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
                Op::VarDump => push_datetime_var_dump_methods(
                    &mut methods,
                    module,
                    function,
                    &inst.operands,
                ),
                Op::PrintR => push_datetime_print_r_methods(
                    &mut methods,
                    module,
                    function,
                    &inst.operands,
                ),
                Op::RuntimeCall
                    if typed_builtin_target(inst) == Some(crate::ir::RuntimeFnId::VarDump) =>
                {
                    push_datetime_var_dump_methods(
                        &mut methods,
                        module,
                        function,
                        &inst.operands,
                    );
                }
                Op::RuntimeCall
                    if typed_builtin_target(inst) == Some(crate::ir::RuntimeFnId::PrintR) =>
                {
                    push_datetime_print_r_methods(
                        &mut methods,
                        module,
                        function,
                        &inst.operands,
                    );
                }
                _ => {}
            }
        }
    }
    methods
}

/// Adds the php-src-specific date/time debug renderer methods referenced by `var_dump()`.
fn push_datetime_var_dump_methods(
    methods: &mut Vec<(String, String)>,
    module: &Module,
    function: &Function,
    operands: &[crate::ir::ValueId],
) {
    let debug_key = php_method_key("__elephc_debug_dump");
    for operand in operands {
        let Some(operand_type) = function
            .value(*operand)
            .map(|value| value.php_type.codegen_repr())
        else {
            continue;
        };
        push_datetime_var_dump_type_methods(methods, module, &operand_type, &debug_key);
    }
}

/// Adds the php-src-specific date/time renderer methods referenced by `print_r()`.
fn push_datetime_print_r_methods(
    methods: &mut Vec<(String, String)>,
    module: &Module,
    function: &Function,
    operands: &[crate::ir::ValueId],
) {
    let print_key = php_method_key("__elephc_print_r_dump");
    for operand in operands {
        let Some(operand_type) = function
            .value(*operand)
            .map(|value| value.php_type.codegen_repr())
        else {
            continue;
        };
        push_datetime_var_dump_type_methods(methods, module, &operand_type, &print_key);
    }
}

/// Registers ext/date debug renderers and their initialization guards reachable
/// through one debug-output operand type.
fn push_datetime_var_dump_type_methods(
    methods: &mut Vec<(String, String)>,
    module: &Module,
    operand_type: &PhpType,
    debug_key: &str,
) {
    let initialized_key = php_method_key("__elephc_assert_initialized");
    match operand_type {
        PhpType::Object(class_name) => {
            let normalized = class_name.trim_start_matches('\\');
            if is_datetime_debug_class(normalized) {
                push_supported_builtin_spl_method_for_receiver(
                    methods,
                    module,
                    normalized,
                    debug_key,
                );
                push_supported_builtin_spl_method_for_receiver(
                    methods,
                    module,
                    normalized,
                    &initialized_key,
                );
            } else if normalized.eq_ignore_ascii_case("DateTimeInterface") {
                for class_name in ["DateTime", "DateTimeImmutable"] {
                    push_supported_builtin_spl_method_for_receiver(
                        methods,
                        module,
                        class_name,
                        debug_key,
                    );
                    push_supported_builtin_spl_method_for_receiver(
                        methods,
                        module,
                        class_name,
                        &initialized_key,
                    );
                }
            }
        }
        PhpType::Array(element) => {
            push_datetime_var_dump_type_methods(methods, module, element, debug_key);
        }
        PhpType::AssocArray { value, .. } => {
            push_datetime_var_dump_type_methods(methods, module, value, debug_key);
        }
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => {
            for class_name in [
                "DateTime",
                "DateTimeImmutable",
                "DateTimeZone",
                "DateInterval",
                "DatePeriod",
            ] {
                push_supported_builtin_spl_method_for_receiver(
                    methods,
                    module,
                    class_name,
                    debug_key,
                );
                push_supported_builtin_spl_method_for_receiver(
                    methods,
                    module,
                    class_name,
                    &initialized_key,
                );
            }
        }
        _ => {}
    }
}

/// Returns whether a builtin class owns a php-src-specific `var_dump()` renderer.
fn is_datetime_debug_class(class_name: &str) -> bool {
    matches!(
        php_symbol_key(class_name).as_str(),
        "datetime" | "datetimeimmutable" | "datetimezone" | "dateinterval" | "dateperiod"
    )
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
