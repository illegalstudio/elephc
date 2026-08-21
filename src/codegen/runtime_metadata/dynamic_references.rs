//! Purpose:
//! Finds class metadata referenced by object factories, lookup builtins, registrations, and scoped constants.
//!
//! Called from:
//! - `super::classes::runtime_referenced_class_names()` and interface selection.
//!
//! Key details:
//! - Interprets typed runtime targets and EIR immediates without broadening dynamic lookup metadata.

use super::*;

/// Returns class-name data entries attached to runtime object metadata opcodes.
pub(in crate::codegen) fn referenced_class_data_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
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
                Op::ObjectNew => {}
                Op::InstanceOf if instance_of_value_needs_runtime_metadata(function, inst) => {}
                Op::InstanceOf => continue,
                _ => continue,
            }
            let Some(Immediate::Data(data)) = inst.immediate else {
                continue;
            };
            if let Some(name) = module.data.class_names.get(data.as_raw() as usize) {
                names.insert(name.clone());
            }
        }
    }
    names
}

/// Returns class metadata needed by dynamic object factories.
pub(in crate::codegen) fn referenced_dynamic_object_new_class_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
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
            if matches!(
                inst.op,
                Op::DynamicObjectNewMixed | Op::DynamicObjectNewWithoutConstructorMixed
            ) {
                names.extend(
                    module
                        .class_infos
                        .keys()
                        .filter(|class_name| is_dynamic_new_mixed_metadata_candidate(class_name))
                        .cloned(),
                );
                continue;
            }
            if !matches!(inst.op, Op::DynamicObjectNew) {
                continue;
            }
            let Some((fallback_class, required_parent)) =
                dynamic_object_new_metadata_names(module, inst)
            else {
                continue;
            };
            names.insert(fallback_class.to_string());
            names.insert(required_parent.to_string());
            for class_name in module.class_infos.keys() {
                if is_same_or_descendant(module, class_name, required_parent) {
                    names.insert(class_name.clone());
                }
            }
        }
    }
    names
}

/// Returns true when generic `new $class` can emit static metadata for this class.
pub(in crate::codegen) fn is_dynamic_new_mixed_metadata_candidate(class_name: &str) -> bool {
    if class_name.starts_with("__Elephc") {
        return false;
    }
    if supported_dynamic_new_builtin_class_name(class_name) {
        return true;
    }
    !known_dynamic_new_builtin_class_name(class_name)
}

/// Returns true for builtin classes with safe static allocation paths in generic dynamic new.
pub(in crate::codegen) fn supported_dynamic_new_builtin_class_name(class_name: &str) -> bool {
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
            | "reflectionclass"
            | "reflectionmethod"
            | "reflectionproperty"
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
pub(in crate::codegen) fn known_dynamic_new_builtin_class_name(class_name: &str) -> bool {
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

/// Parses the fallback and required-parent names from a dynamic object factory immediate.
pub(in crate::codegen) fn dynamic_object_new_metadata_names<'a>(
    module: &'a Module,
    inst: &crate::ir::Instruction,
) -> Option<(&'a str, &'a str)> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return None;
    };
    module
        .data
        .class_names
        .get(data.as_raw() as usize)?
        .split_once('|')
        .map(|(fallback_class, required_parent)| {
            (
                fallback_class.trim_start_matches('\\'),
                required_parent.trim_start_matches('\\'),
            )
        })
}

/// Returns static class names that can feed `get_class()`/`get_parent_class()` lookups.
pub(in crate::codegen) fn referenced_class_name_lookup_builtin_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
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
            if !is_class_name_lookup_builtin(inst) {
                continue;
            }
            if inst.operands.is_empty() {
                if let Some(class_name) = current_function_class(function) {
                    names.insert(class_name.to_string());
                }
                continue;
            }
            for value in &inst.operands {
                let Some(metadata) = function.value(*value) else {
                    continue;
                };
                if let PhpType::Object(class_name) = metadata.php_type.codegen_repr() {
                    names.insert(class_name.trim_start_matches('\\').to_string());
                }
            }
        }
    }
    names
}

/// Returns whether an instruction is a class-name lookup builtin call.
pub(in crate::codegen) fn is_class_name_lookup_builtin(inst: &crate::ir::Instruction) -> bool {
    typed_builtin_target(inst).is_some_and(|target| target.is_class_name_lookup())
}

/// Returns class names passed as literals to stream wrapper/filter registration builtins.
pub(in crate::codegen) fn referenced_stream_registration_class_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
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
            if !is_stream_registration_builtin(inst)
                || inst.operands.len() < 2
            {
                continue;
            }
            if let Some(class_name) = const_string_value(module, function, inst.operands[1]) {
                names.insert(class_name.trim_start_matches('\\').to_string());
            }
        }
    }
    names
}

/// Resolves a class name against module metadata using PHP case-insensitive class rules.
pub(in crate::codegen) fn canonical_module_class_name(module: &Module, class_name: &str) -> Option<String> {
    let wanted = php_symbol_key(class_name.trim_start_matches('\\'));
    module
        .class_infos
        .keys()
        .find(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == wanted)
        .cloned()
}

/// Returns true for builtins whose literal class argument is consumed by runtime metadata.
pub(in crate::codegen) fn is_stream_registration_builtin(inst: &crate::ir::Instruction) -> bool {
    typed_builtin_target(inst).is_some_and(|target| target.is_stream_registration())
}

/// Returns the typed builtin target carried by an EIR runtime call.
pub(in crate::codegen) fn typed_builtin_target(inst: &crate::ir::Instruction) -> Option<crate::ir::RuntimeFnId> {
    match inst.immediate {
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(target))) => Some(target),
        Some(Immediate::RuntimeCall(
            crate::ir::RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => Some(target),
        _ => None,
    }
}

/// Returns the literal string payload produced by a string or `::class` constant.
pub(in crate::codegen) fn const_string_value<'a>(
    module: &'a Module,
    function: &'a Function,
    value: crate::ir::ValueId,
) -> Option<&'a str> {
    let value_ref = function.value(value)?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return None;
    };
    let inst_ref = function.instruction(inst)?;
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return None;
    };
    match inst_ref.op {
        Op::ConstStr => module.data.strings.get(data.as_raw() as usize),
        Op::ConstClassName => module.data.class_names.get(data.as_raw() as usize),
        _ => None,
    }
    .map(String::as_str)
}

/// Returns class-like receiver names encoded in scoped constant immediates.
pub(in crate::codegen) fn referenced_scoped_constant_class_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
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
            if !matches!(inst.op, Op::ScopedConstantGet) {
                continue;
            }
            let Some(Immediate::Data(data)) = inst.immediate else {
                continue;
            };
            let Some(label) = module.data.strings.get(data.as_raw() as usize) else {
                continue;
            };
            let Some((class_name, _)) = label.rsplit_once("::") else {
                continue;
            };
            names.insert(class_name.trim_start_matches('\\').to_string());
        }
    }
    names
}

/// Returns whether an `instanceof` value can reach the runtime metadata matcher.
pub(in crate::codegen) fn instance_of_value_needs_runtime_metadata(
    function: &crate::ir::Function,
    inst: &crate::ir::Instruction,
) -> bool {
    let Some(value) = inst.operands.first() else {
        return false;
    };
    function.value(*value).is_some_and(|metadata| {
        matches!(
            metadata.php_type.codegen_repr(),
            PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_)
        )
    })
}
