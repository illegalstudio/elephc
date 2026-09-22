//! Purpose:
//! Selects and lowers the synthetic builtin Reflection surface reachable from EIR.
//!
//! Called from:
//! - `crate::ir_lower::program::lower()` after user functions and literal eval AOT bodies.
//!
//! Key details:
//! - Native programs lower Reflection classes to a fixed point from EIR types and calls.
//! - Dynamic eval keeps the full Reflection surface because names are resolved at runtime.

use std::collections::{BTreeSet, HashMap};

use crate::ir::{Function, Module, Op};
use crate::parser::ast::ExprKind;
use crate::types::{CheckResult, FunctionSig, PhpType};

use super::function;
use super::program::{
    all_lowered_functions, class_data_name, class_method_already_lowered,
    dynamic_object_new_metadata_names, include_lowered_runtime_features, php_method_key,
    string_data_name,
};

/// Builtin Reflection classes whose concrete method bodies can be lowered into EIR.
const BUILTIN_REFLECTION_CLASS_NAMES: &[&str] = &[
    "ReflectionAttribute",
    "ReflectionClass",
    "ReflectionObject",
    "ReflectionEnum",
    "ReflectionClassConstant",
    "ReflectionEnumBackedCase",
    "ReflectionEnumUnitCase",
    "ReflectionFunction",
    "ReflectionMethod",
    "ReflectionNamedType",
    "ReflectionParameter",
    "ReflectionProperty",
    "ReflectionUnionType",
    "ReflectionIntersectionType",
];

/// Reflection slots codegen fills with a freshly built Reflection object of another class.
///
/// Each row is the getter that hands the slot back, the slot, and the classes it can hold. The
/// allocations happen in `codegen::lower_inst::objects::reflection` (the
/// `emit_reflection_*_property` materializers), not as an EIR `ObjectNew`, and the getters are
/// typed `mixed`, so the EIR scan cannot see these classes on its own (#1229).
///
/// The type rows matter even where a program does not crash without them today: most type
/// getters were only safe through an incidental cascade (a `getDeclaringClass()` typed
/// `ReflectionClass`, whose surface reaches `ReflectionParameter::getType()`'s patched union).
/// `ReflectionEnum::getBackingType()` has no such path; its row lowers the type classes, but the
/// object that getter returns is unusable for a separate reason — it crashes even when nothing
/// stringifies it — so no test can exercise that row until that is fixed.
const MATERIALIZED_REFLECTION_SLOTS: &[(&str, &str, &[&str])] = &[
    ("getdeclaringfunction", "__declaring_function", &["ReflectionFunction", "ReflectionMethod"]),
    ("getdeclaringclass", "__declaring_class", &["ReflectionClass"]),
    ("getclass", "__class", &["ReflectionClass"]),
    ("getparentclass", "__parent_class", &["ReflectionClass"]),
    ("getinterfaces", "__interfaces", &["ReflectionClass"]),
    ("gettraits", "__traits", &["ReflectionClass"]),
    ("getenum", "__enum", &["ReflectionEnum"]),
    ("gettype", "__type", REFLECTION_TYPE_CLASSES),
    ("getreturntype", "__type", REFLECTION_TYPE_CLASSES),
    ("getsettabletype", "__settable_type", REFLECTION_TYPE_CLASSES),
    ("getbackingtype", "__backing_type", REFLECTION_TYPE_CLASSES),
];

/// Every class the shared type materializer can build into a `__*type` slot.
const REFLECTION_TYPE_CLASSES: &[&str] = &[
    "ReflectionNamedType",
    "ReflectionUnionType",
    "ReflectionIntersectionType",
];

/// Lowers only synthetic Reflection classes reachable from native EIR, or the full
/// surface when the dynamic eval bridge can construct and invoke them by name.
pub(super) fn lower_referenced_builtin_methods(
    module: &mut Module,
    check_result: &CheckResult,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
) {
    loop {
        let classes = referenced_builtin_reflection_classes(module);
        let before = module.class_methods.len();
        for class_name in classes {
            lower_builtin_reflection_property_init_thunk(
                &class_name,
                module,
                check_result,
                constants,
                fiber_return_sigs,
            );
            lower_builtin_reflection_class_methods(
                &class_name,
                module,
                check_result,
                constants,
                fiber_return_sigs,
            );
        }
        if module.class_methods.len() == before {
            break;
        }
        include_lowered_runtime_features(module);
    }
}

/// Lowers a selected Reflection class's property-default thunk once it becomes reachable.
fn lower_builtin_reflection_property_init_thunk(
    class_name: &str,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
) {
    let Some(class_info) = check_result.classes.get(class_name) else {
        return;
    };
    let function_name = format!("_class_propinit_{}", class_info.class_id);
    if module
        .functions
        .iter()
        .any(|function| function.name == function_name)
    {
        return;
    }
    function::lower_property_init_thunk(
        class_name,
        class_info,
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
}

/// Collects Reflection class owners reachable from lowered values and calls.
fn referenced_builtin_reflection_classes(module: &Module) -> BTreeSet<String> {
    let mut classes = BTreeSet::new();
    if module.required_runtime_features.eval_bridge {
        for class_name in BUILTIN_REFLECTION_CLASS_NAMES {
            insert_builtin_reflection_class(module, class_name, &mut classes);
        }
        return classes;
    }

    for function in all_lowered_functions(module) {
        collect_builtin_reflection_class_from_type(
            module,
            &function.return_php_type,
            &mut classes,
        );
        for param in &function.params {
            collect_builtin_reflection_class_from_type(module, &param.php_type, &mut classes);
        }
        for local in &function.locals {
            collect_builtin_reflection_class_from_type(module, &local.php_type, &mut classes);
        }
        for value in &function.values {
            collect_builtin_reflection_class_from_type(module, &value.php_type, &mut classes);
        }

        for inst in &function.instructions {
            match inst.op {
                Op::ObjectNew => {
                    if let Some(class_name) = class_data_name(module, inst) {
                        insert_builtin_reflection_class(module, class_name, &mut classes);
                    }
                }
                Op::DynamicObjectNew => {
                    if let Some((fallback_class, required_parent)) =
                        dynamic_object_new_metadata_names(module, inst)
                    {
                        insert_builtin_reflection_class(module, fallback_class, &mut classes);
                        insert_builtin_reflection_class(module, required_parent, &mut classes);
                    }
                }
                Op::StaticMethodCall => {
                    if let Some((class_name, _)) =
                        string_data_name(module, inst).and_then(|name| name.rsplit_once("::"))
                    {
                        insert_builtin_reflection_class(module, class_name, &mut classes);
                    }
                }
                Op::MethodCall | Op::NullsafeMethodCall => {
                    collect_dynamic_reflection_method_candidates(
                        module,
                        function,
                        inst,
                        &mut classes,
                    );
                }
                _ => {}
            }
        }
    }
    collect_named_materialized_companions(module, &mut classes);
    classes
}

/// Adds Reflection implementations that a mixed/union method receiver may dispatch to.
fn collect_dynamic_reflection_method_candidates(
    module: &Module,
    function: &Function,
    inst: &crate::ir::Instruction,
    classes: &mut BTreeSet<String>,
) {
    let Some(receiver) = inst.operands.first().copied() else {
        return;
    };
    let Some(receiver_type) = function.value(receiver).map(|value| value.php_type.codegen_repr())
    else {
        return;
    };
    if !matches!(receiver_type, PhpType::Mixed | PhpType::Union(_)) {
        return;
    }
    let Some(method_name) = string_data_name(module, inst) else {
        return;
    };
    let method_key = php_method_key(method_name);
    for (class_name, class_info) in &module.class_infos {
        if class_info.methods.contains_key(&method_key) {
            insert_builtin_reflection_class(module, class_name, classes);
        }
    }
}

/// Adds Reflection class names nested in one EIR-visible PHP type.
fn collect_builtin_reflection_class_from_type(
    module: &Module,
    php_type: &PhpType,
    classes: &mut BTreeSet<String>,
) {
    match php_type {
        PhpType::Object(class_name) => {
            insert_builtin_reflection_class(module, class_name, classes);
        }
        PhpType::Array(element) | PhpType::Buffer(element) => {
            collect_builtin_reflection_class_from_type(module, element, classes);
        }
        PhpType::AssocArray { key, value } => {
            collect_builtin_reflection_class_from_type(module, key, classes);
            collect_builtin_reflection_class_from_type(module, value, classes);
        }
        PhpType::Union(members) => {
            for member in members {
                collect_builtin_reflection_class_from_type(module, member, classes);
            }
        }
        PhpType::Pointer(Some(class_name)) => {
            insert_builtin_reflection_class(module, class_name, classes);
        }
        PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Bool
        | PhpType::False
        | PhpType::Void
        | PhpType::Never
        | PhpType::Iterable
        | PhpType::Mixed
        | PhpType::Callable
        | PhpType::Packed(_)
        | PhpType::Pointer(None)
        | PhpType::Resource(_)
        | PhpType::TaggedScalar => {}
    }
}

/// Inserts one canonical Reflection class plus Reflection ancestors and method owners.
fn insert_builtin_reflection_class(
    module: &Module,
    class_name: &str,
    classes: &mut BTreeSet<String>,
) {
    let Some(canonical) = canonical_builtin_reflection_class_name(class_name) else {
        return;
    };
    if !module.class_infos.contains_key(canonical) || !classes.insert(canonical.to_string()) {
        return;
    }
    let Some(class_info) = module.class_infos.get(canonical) else {
        return;
    };
    let dependencies = class_info
        .parent
        .iter()
        .chain(class_info.method_impl_classes.values())
        .chain(class_info.static_method_impl_classes.values())
        .cloned()
        .collect::<Vec<_>>();
    for dependency in dependencies {
        insert_builtin_reflection_class(module, &dependency, classes);
    }
}

/// Adds the Reflection classes codegen can hand back through a getter the program names.
///
/// Codegen fills some Reflection slots with a freshly built Reflection object of ANOTHER class —
/// `ReflectionParameter`'s declaring function is a `ReflectionFunction` or a `ReflectionMethod`,
/// its declaring class a `ReflectionClass`, an enum case's enum a `ReflectionEnum`. Those
/// allocations happen in `codegen::lower_inst::objects::reflection` (the
/// `emit_reflection_*_property` materializers), not as an EIR `ObjectNew`, and the getters that
/// hand them back are typed `mixed`, so neither the instruction scan nor the type scan ever saw
/// the class. Its methods were never lowered, its vtable held null, and the first implicit
/// `__toString` on such an object — `(string)`, `echo`, `.`, interpolation, `strval()`,
/// `strlen()` — jumped to address zero (#1229). `sprintf("%s")`, which asks the class metadata
/// instead, reported the object as not convertible.
///
/// A companion is added when a referenced class holds the slot AND the program names the getter
/// somewhere in its string data. That one condition covers every route by which the getter is
/// written out: a direct call (`MethodCall` keeps the name as data), `call_user_func([$p, ...])`,
/// a first-class callable (kept as `object::<method>`), a `Class::method` callable string, and a
/// method name held in a variable. It is deliberately not keyed on the owner alone: pulling
/// `ReflectionClass` in for every program that merely builds a `ReflectionParameter` cascades
/// through its whole surface, and multiplied such a program's assembly by 3.6.
///
/// What this cannot see: a getter name that never appears as a literal — assembled at run time,
/// or read back from metadata such as `get_class_methods()` — and any route that reaches a slot's
/// object without calling its getter at all. The slots are filled eagerly when the holder is
/// built, so `(array) $holder`, which today exposes them (#1251), reaches an unlowered companion.
/// That route is a divergence of its own — PHP never shows these slots — and closing it there is
/// what keeps this rule sufficient.
fn collect_named_materialized_companions(module: &Module, classes: &mut BTreeSet<String>) {
    // A first-class callable keeps its target as `object::<method>`, and a callable string may
    // be `Class::method`, so the name is also matched after a `::` qualifier.
    let named = |getter: &str| {
        module.data.strings.iter().any(|string| {
            let method = string.rsplit_once("::").map_or(string.as_str(), |(_, method)| method);
            php_method_key(method) == getter
        })
    };
    // A companion can itself hold a slot of an earlier row, so repeat until nothing is added
    // rather than rely on the table's row order.
    loop {
        let before = classes.len();
        for (getter, slot, held) in MATERIALIZED_REFLECTION_SLOTS {
            let holder_referenced = classes.iter().any(|class_name| {
                module
                    .class_infos
                    .get(class_name.as_str())
                    .is_some_and(|class_info| class_info.property_offsets.contains_key(*slot))
            });
            if holder_referenced && named(getter) {
                for companion in held.iter() {
                    insert_builtin_reflection_class(module, companion, classes);
                }
            }
        }
        if classes.len() == before {
            break;
        }
    }
}

/// Resolves a class spelling to the canonical builtin Reflection name.
pub(super) fn canonical_builtin_reflection_class_name(class_name: &str) -> Option<&'static str> {
    let key = crate::names::php_symbol_key(class_name.trim_start_matches('\\'));
    BUILTIN_REFLECTION_CLASS_NAMES
        .iter()
        .copied()
        .find(|candidate| crate::names::php_symbol_key(candidate) == key)
}

/// Lowers all concrete synthetic methods for one builtin reflection class.
fn lower_builtin_reflection_class_methods(
    class_name: &str,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
) {
    let Some(class_info) = check_result.classes.get(class_name) else {
        return;
    };
    let before = module.class_methods.len();
    for method in &class_info.method_decls {
        if !method.has_body {
            continue;
        }
        let generated_body;
        let method_key = crate::names::php_symbol_key(&method.name);
        if class_method_already_lowered(module, class_name, &method_key, method.is_static) {
            continue;
        }
        let body = if class_name == "ReflectionAttribute" && method_key == "newinstance" {
            let function_attrs = function_attribute_sources(module);
            generated_body =
                crate::codegen::reflection::build_attribute_new_instance_body_with_extra(
                    &check_result.classes,
                    &function_attrs,
                );
            generated_body.as_slice()
        } else if class_name == "ReflectionAttribute" && method_key == "getarguments" {
            // Materialize captured attribute arguments through the normal array
            // lowering (named arguments and associative arrays included) rather
            // than a bespoke codegen path.
            let function_attrs = function_attribute_sources(module);
            generated_body = crate::codegen::reflection::build_attribute_get_arguments_body_with_extra(
                &check_result.classes,
                &function_attrs,
            );
            generated_body.as_slice()
        } else {
            &method.body
        };
        function::lower_class_method(
            class_name,
            &method.name,
            method.is_static,
            &method.params,
            method.return_type.as_ref(),
            body,
            module,
            check_result,
            constants,
            fiber_return_sigs,
        );
    }
    for method in module.class_methods.iter_mut().skip(before) {
        method.flags.is_synthetic = true;
    }
}

/// Returns reflection-visible top-level function attribute metadata sources.
fn function_attribute_sources(
    module: &Module,
) -> Vec<crate::codegen::reflection::AttributeMetadataSource<'_>> {
    module
        .functions
        .iter()
        .filter(|function| !function.attribute_names.is_empty())
        .map(|function| {
            (
                function.attribute_names.as_slice(),
                function.attribute_args.as_slice(),
            )
        })
        .collect()
}

#[cfg(test)]
mod catalog_tests {
    use super::BUILTIN_REFLECTION_CLASS_NAMES;

    /// Pins this lowering list to the shared class catalog: it is exactly the `ext/reflection`
    /// CLASSES minus `ReflectionException`, which is a throwable with no synthetic methods to
    /// lower (the checker injects it with the other builtin throwables). The module's enum
    /// (`PropertyHookType`) is injected by `builtin_enums` and has no methods either.
    #[test]
    fn reflection_class_list_matches_the_catalog() {
        let mut expected: Vec<&str> = elephc_builtin_contract::classes()
            .iter()
            .filter(|class| {
                class.module == elephc_builtin_contract::PhpModule::Reflection
                    && class.kind == elephc_builtin_contract::ClassKind::Class
                    && class.name != "ReflectionException"
            })
            .map(|class| class.name)
            .collect();
        expected.sort_unstable();
        let mut actual: Vec<&str> = BUILTIN_REFLECTION_CLASS_NAMES.to_vec();
        actual.sort_unstable();
        assert_eq!(actual, expected);
    }
}
