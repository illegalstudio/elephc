//! Purpose:
//! Selects and lowers the synthetic builtin Reflection surface reachable from EIR.
//!
//! Called from:
//! - `crate::ir_lower::program::lower()` after user functions and literal eval AOT bodies.
//!
//! Key details:
//! - Native programs lower Reflection classes to a fixed point from EIR types and calls.
//! - Eval-owned Reflection values are executed by Magician and do not root unrelated AOT bodies.

use std::collections::{BTreeMap, BTreeSet};
use crate::fast_hash::FastMap as HashMap;

use crate::ir::{Function, Immediate, IrType, Module, Op};
use crate::parser::ast::ExprKind;
use crate::types::{CheckResult, FunctionSig, PhpType};

use super::function;
use super::program::{
    all_lowered_functions, class_data_name, class_method_already_lowered,
    dynamic_object_new_metadata_names, include_lowered_runtime_features, php_method_key,
    string_data_name,
};

/// The Reflection classes whose constructors read the module reflection table: an allocation of
/// one (or of a user subclass) needs the bodies of the interface methods it implements.
const TABLE_REFLECTOR_CLASSES: &[&str] = &[
    "ReflectionClass",
    "ReflectionObject",
    "ReflectionEnum",
    "ReflectionClassConstant",
    "ReflectionEnumUnitCase",
    "ReflectionEnumBackedCase",
    "ReflectionProperty",
    "ReflectionMethod",
    "ReflectionParameter",
    "ReflectionFunction",
    "ReflectionNamedType",
    "ReflectionUnionType",
    "ReflectionIntersectionType",
];

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
    // Abstract, but a user subclass (psysh's `ReflectionLanguageConstruct`) runs its bodies for
    // every method it does not override.
    "ReflectionFunctionAbstract",
    "ReflectionType",
    "ReflectionNamedType",
    "ReflectionParameter",
    "ReflectionProperty",
    "ReflectionUnionType",
    "ReflectionIntersectionType",
];

/// Lowers only synthetic Reflection classes reachable from native EIR.
///
/// Reflection values owned by the dynamic eval bridge are dispatched by Magician, whose
/// interpreter exposes the complete Reflection surface without forcing those bodies into AOT.
pub(super) fn lower_referenced_builtin_methods(
    module: &mut Module,
    check_result: &CheckResult,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
) {
    loop {
        let methods = referenced_builtin_reflection_methods(module);
        let before = module.class_methods.len();
        for (class_name, methods) in methods {
            lower_builtin_reflection_property_init_thunk(
                &class_name,
                module,
                check_result,
                constants,
                fiber_return_sigs,
            );
            lower_builtin_reflection_class_methods(
                &class_name,
                &methods,
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

/// Keeps every member of the Reflection classes when interpreted code can call one.
///
/// A member's body is lowered when COMPILED code names it, and only then, because that is all this
/// pass can see. Interpreted code names members no compiled line mentions -- a
/// `ReflectionParameter`'s `getName()` above all -- and reached a method with no body:
/// `native_method_error stage=invoke`, which the process prints as the six-word
/// `Fatal error: eval() runtime failed`. The interpreter answers the CLASS-level members itself,
/// which is why only the member classes fell over.
///
/// The gate is the bridge flag, the same authority the SPL discovery pass reads: a program that
/// can reach the interpreter at all may reflect on a parameter exactly as it may reflect on a
/// class, and a program with no route into the interpreter keeps paying nothing.
fn insert_bridge_reachable_reflection_methods(
    module: &Module,
    methods: &mut BTreeMap<String, BTreeSet<String>>,
) -> bool {
    if !module.required_runtime_features.eval_bridge {
        return false;
    }
    // Literal eval fragments name exactly the Reflection classes they use; a fragment whose
    // source is only known at run time can name any of them.
    let demand = super::program::eval_demand(module);
    // A callable descriptor / expression call names its method at run time
    // (`call_user_func([$p, 'get' . $x])`, `$callable()`), so no fragment scan and no static
    // receiver type can say which Reflection member it reaches. The branch kept every member
    // whenever the bridge linked; the eval-demand pruning must not undo that for these calls.
    let runtime_named_call = super::program::all_lowered_functions(module).any(|function| {
        function
            .instructions
            .iter()
            .any(|inst| matches!(inst.op, Op::CallableDescriptorInvoke | Op::ExprCall))
    });
    for class_name in BUILTIN_REFLECTION_CLASS_NAMES {
        if !runtime_named_call && !demand.names_class(class_name) {
            continue;
        }
        let Some(class_info) = module.class_infos.get(*class_name) else {
            continue;
        };
        let entry = methods.entry((*class_name).to_string()).or_default();
        for method_key in class_info.methods.keys() {
            entry.insert(method_key.clone());
        }
        for method_key in class_info.static_methods.keys() {
            entry.insert(method_key.clone());
        }
    }
    // A fragment whose source is only known at run time can name any class, so every member is
    // already kept and the scan of native calls below would add nothing.
    runtime_named_call || demand.is_everything()
}

/// Collects Reflection constructors and methods reachable from EIR calls and descriptors.
fn referenced_builtin_reflection_methods(module: &Module) -> BTreeMap<String, BTreeSet<String>> {
    let family = reflection_family(module);
    let family = family.as_slice();
    let mut methods = BTreeMap::new();
    if insert_bridge_reachable_reflection_methods(module, &mut methods) {
        return methods;
    }
    for function in all_lowered_functions(module) {
        for inst in &function.instructions {
            if instruction_uses_mixed_string_dispatch(function, inst) {
                collect_builtin_reflection_methods_for_receiver_type(
                    module,
                    family,
                    &PhpType::Mixed,
                    "__toString",
                    &mut methods,
                );
            }
            match inst.op {
                Op::ObjectNew => {
                    if let Some(class_name) = class_data_name(module, inst) {
                        insert_builtin_reflection_method(module, class_name, "__construct", &mut methods);
                        // The class reflectors implement `Reflector`: an allocation needs the
                        // interface's method body even when nothing calls it.
                        // A user subclass inherits those bodies from the reflector ancestor.
                        let mut reflector = class_name.trim_start_matches('\\').to_string();
                        for _ in 0..64 {
                            if TABLE_REFLECTOR_CLASSES.contains(&reflector.as_str()) {
                                break;
                            }
                            match module
                                .class_infos
                                .get(&reflector)
                                .and_then(|info| info.parent.clone())
                            {
                                Some(parent) => reflector = parent.trim_start_matches('\\').to_string(),
                                None => break,
                            }
                        }
                        if crate::reflection_table::enabled()
                            && TABLE_REFLECTOR_CLASSES.contains(&reflector.as_str())
                        {
                            insert_builtin_reflection_method(
                                module,
                                &reflector,
                                "__construct",
                                &mut methods,
                            );
                            let interfaces = module
                                .class_infos
                                .get(&reflector)
                                .map(|info| info.interfaces.clone())
                                .unwrap_or_default();
                            for interface in interfaces {
                                let required = module
                                    .interface_infos
                                    .get(&interface)
                                    .map(|info| info.method_order.clone())
                                    .unwrap_or_default();
                                for method_name in required {
                                    insert_builtin_reflection_method(
                                        module,
                                        &reflector,
                                        &method_name,
                                        &mut methods,
                                    );
                                }
                            }
                        }
                    }
                }
                Op::DynamicObjectNew => {
                    if let Some((fallback_class, required_parent)) =
                        dynamic_object_new_metadata_names(module, inst)
                    {
                        insert_builtin_reflection_method(
                            module,
                            fallback_class,
                            "__construct",
                            &mut methods,
                        );
                        insert_builtin_reflection_method(
                            module,
                            required_parent,
                            "__construct",
                            &mut methods,
                        );
                    }
                }
                Op::StaticMethodCall => {
                    if let Some((class_name, method_name)) =
                        string_data_name(module, inst).and_then(|name| name.rsplit_once("::"))
                    {
                        insert_builtin_reflection_method(module, class_name, method_name, &mut methods);
                        // `parent::__construct()` of a user Reflection subclass names no class:
                        // it runs the ancestor's body, whichever Reflection class that is.
                        if class_name.eq_ignore_ascii_case("parent") {
                            let parent = function
                                .name
                                .rsplit_once("::")
                                .and_then(|(owner, _)| module.class_infos.get(owner))
                                .and_then(|info| info.parent.clone());
                            if let Some(parent) = parent {
                                insert_builtin_reflection_method(
                                    module,
                                    parent.trim_start_matches('\\'),
                                    method_name,
                                    &mut methods,
                                );
                            }
                        }
                    }
                }
                Op::MethodCall | Op::NullsafeMethodCall => {
                    collect_builtin_reflection_method_call(module, family, function, inst, &mut methods);
                }
                Op::FirstClassCallableNew => {
                    collect_builtin_reflection_first_class_callable(
                        module,
                        family,
                        function,
                        inst,
                        &mut methods,
                    );
                }
                _ => {}
            }
        }
    }
    // `getAttributes()` answers a boxed array, so `->newInstance()` on its elements is a call on a
    // `mixed` receiver: variadic, hence a callable-descriptor call that names no static class.
    // The bridge answers it in a default build; a closed-world build lowers the body. Every
    // reflector with attributes counts, not only the class ones: a property's or a method's
    // `getAttributes()[0]->newInstance()` reached no body and died on "callable array did not
    // resolve to an invokable target".
    //
    // Not gated on closed-world: the `ReflectionAttribute` objects `getAttributes()` hands back
    // are the NATIVE ones, so a descriptor call finds `newInstance` in the compiled class or not
    // at all, and the interpreter never gets to answer (`Fatal error: eval() runtime failed`,
    // 20+ branch tests). The eval-demand pruning above does not see descriptor calls.
    if methods
        .values()
        .any(|lowered: &BTreeSet<String>| lowered.contains("getattributes"))
    {
        insert_builtin_reflection_method(module, "ReflectionAttribute", "newInstance", &mut methods);
    }
    methods
}

/// Returns whether one EIR string context can dispatch a boxed receiver through `__toString`.
fn instruction_uses_mixed_string_dispatch(function: &Function, inst: &crate::ir::Instruction) -> bool {
    let string_context = inst.op == Op::EchoValue
        || (inst.op == Op::Cast && inst.immediate == Some(Immediate::CastTarget(IrType::Str)));
    string_context && inst.operands.first().is_some_and(|operand| {
        function.value(*operand).is_some_and(|value| {
            matches!(
                value.php_type.codegen_repr(),
                PhpType::Mixed | PhpType::Union(_)
            )
        })
    })
}

/// The classes a `mixed` receiver's Reflection method can resolve through: the builtin Reflection
/// classes themselves and every class with a method implemented by one, in class-table order.
///
/// `insert_builtin_reflection_method` does nothing for any other class, so the `mixed` case scans
/// only these instead of cloning every class name per mixed call site and descending into each
/// (a hotspot of lowering a Symfony build).
pub(in crate::ir_lower) fn reflection_family(module: &Module) -> Vec<String> {
    module
        .class_infos
        .iter()
        .filter(|(name, info)| {
            canonical_builtin_reflection_class_name(name).is_some()
                || info
                    .method_impl_classes
                    .values()
                    .chain(info.static_method_impl_classes.values())
                    .any(|implementation| canonical_builtin_reflection_class_name(implementation).is_some())
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// Adds the Reflection method selected by an `object::method` first-class callable descriptor.
fn collect_builtin_reflection_first_class_callable(
    module: &Module,
    family: &[String],
    function: &Function,
    inst: &crate::ir::Instruction,
    methods: &mut BTreeMap<String, BTreeSet<String>>,
) {
    let Some(method_name) = string_data_name(module, inst)
        .and_then(|name| name.strip_prefix("object::"))
    else {
        return;
    };
    let Some(receiver) = inst.operands.first().copied() else {
        return;
    };
    let Some(receiver_type) = function.value(receiver).map(|value| value.php_type.codegen_repr())
    else {
        return;
    };
    collect_builtin_reflection_methods_for_receiver_type(
        module,
        family,
        &receiver_type,
        method_name,
        methods,
    );
}

/// Adds the concrete Reflection implementation(s) that can service one EIR method call.
fn collect_builtin_reflection_method_call(
    module: &Module,
    family: &[String],
    function: &Function,
    inst: &crate::ir::Instruction,
    methods: &mut BTreeMap<String, BTreeSet<String>>,
) {
    let Some(receiver) = inst.operands.first().copied() else {
        return;
    };
    let Some(receiver_type) = function.value(receiver).map(|value| value.php_type.codegen_repr())
    else {
        return;
    };
    let Some(method_name) = string_data_name(module, inst) else {
        return;
    };
    collect_builtin_reflection_methods_for_receiver_type(
        module,
        family,
        &receiver_type,
        method_name,
        methods,
    );
}

/// Adds methods for the exact or gradual Reflection receiver alternatives in one PHP type.
fn collect_builtin_reflection_methods_for_receiver_type(
    module: &Module,
    family: &[String],
    receiver_type: &PhpType,
    method_name: &str,
    methods: &mut BTreeMap<String, BTreeSet<String>>,
) {
    match receiver_type {
        PhpType::Object(class_name) => {
            insert_builtin_reflection_method(module, class_name, method_name, methods);
            // A call on a base type (`ReflectionType`, `ReflectionFunctionAbstract`) dispatches to
            // the override of whichever subclass the object is.
            let base = class_name.trim_start_matches('\\');
            for candidate in BUILTIN_REFLECTION_CLASS_NAMES {
                let derives = module
                    .class_infos
                    .get(*candidate)
                    .and_then(|info| info.parent.as_deref())
                    .is_some_and(|parent| parent.trim_start_matches('\\') == base);
                if derives {
                    insert_builtin_reflection_method(module, candidate, method_name, methods);
                }
            }
            // A receiver typed as an interface (`Reflector`) or a user class dispatches to
            // whichever Reflection implementer the object is, exactly like a `mixed` receiver.
            if canonical_builtin_reflection_class_name(class_name).is_none() {
                collect_builtin_reflection_methods_for_receiver_type(
                    module,
                    family,
                    &PhpType::Mixed,
                    method_name,
                    methods,
                );
            }
        }
        PhpType::Union(members) => {
            for member in members {
                collect_builtin_reflection_methods_for_receiver_type(
                    module,
                    family,
                    member,
                    method_name,
                    methods,
                );
            }
        }
        PhpType::Mixed => {
            let method_key = php_method_key(method_name);
            for class_name in family {
                if module
                    .class_infos
                    .get(class_name)
                    .is_some_and(|class_info| class_info.methods.contains_key(&method_key))
                {
                    insert_builtin_reflection_method(module, class_name, method_name, methods);
                }
            }
        }
        PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Bool
        | PhpType::False
        | PhpType::Void
        | PhpType::Never
        | PhpType::Iterable
        | PhpType::Callable
        | PhpType::Packed(_)
        | PhpType::Pointer(_)
        | PhpType::Resource(_)
        | PhpType::Narrow(_)
        | PhpType::TaggedScalar => {}
    }
}

/// Inserts the owning builtin Reflection implementation for one reachable method.
fn insert_builtin_reflection_method(
    module: &Module,
    class_name: &str,
    method_name: &str,
    methods: &mut BTreeMap<String, BTreeSet<String>>,
) {
    let Some(canonical) = canonical_builtin_reflection_class_name(class_name) else {
        // A user class deriving from a Reflection class runs the ancestor's body for every method
        // it does not override.
        let method_key = php_method_key(method_name);
        if let Some(implementation) = module
            .class_infos
            .get(class_name.trim_start_matches('\\'))
            .and_then(|class_info| {
                class_info
                    .method_impl_classes
                    .get(&method_key)
                    .or_else(|| class_info.static_method_impl_classes.get(&method_key))
            })
        {
            if canonical_builtin_reflection_class_name(implementation).is_some() {
                insert_builtin_reflection_method(module, implementation, method_name, methods);
            }
        }
        return;
    };
    let Some(class_info) = module.class_infos.get(canonical) else {
        return;
    };
    let method_key = php_method_key(method_name);
    let implementation = class_info
        .method_impl_classes
        .get(&method_key)
        .or_else(|| class_info.static_method_impl_classes.get(&method_key))
        .map(String::as_str)
        .unwrap_or(canonical);
    let Some(owner) = canonical_builtin_reflection_class_name(implementation) else {
        return;
    };
    methods
        .entry(owner.to_string())
        .or_default()
        .insert(method_key);
}

/// Resolves a class spelling to the canonical builtin Reflection name.
pub(super) fn canonical_builtin_reflection_class_name(class_name: &str) -> Option<&'static str> {
    // `php_symbol_key` is `to_ascii_lowercase`, so comparing two of its results is exactly
    // `eq_ignore_ascii_case` — with a `String` allocated per candidate, on every call.
    let wanted = class_name.trim_start_matches('\\');
    BUILTIN_REFLECTION_CLASS_NAMES
        .iter()
        .copied()
        .find(|candidate| candidate.eq_ignore_ascii_case(wanted))
}

/// Lowers all concrete synthetic methods for one builtin reflection class.
fn lower_builtin_reflection_class_methods(
    class_name: &str,
    reachable_methods: &BTreeSet<String>,
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
        if !reachable_methods.contains(&method_key) {
            continue;
        }
        if class_method_already_lowered(module, class_name, &method_key, method.is_static) {
            continue;
        }
        let body = if class_name == "ReflectionAttribute" && method_key == "newinstance" {
            let owned = crate::codegen::reflection::function_attribute_metadata(&module.functions);
            let function_attrs = crate::codegen::reflection::borrow_attribute_metadata(&owned);
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
            let owned = crate::codegen::reflection::function_attribute_metadata(&module.functions);
            let function_attrs = crate::codegen::reflection::borrow_attribute_metadata(&owned);
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

#[cfg(test)]
mod catalog_tests {
    use super::BUILTIN_REFLECTION_CLASS_NAMES;

    /// Pins this lowering list to the shared class catalog: it is exactly the `ext/reflection`
    /// CLASSES minus the two the AOT backend never lowers a body for. The module's enum
    /// (`PropertyHookType`) is injected by `builtin_enums` and has no methods either.
    ///
    /// * `ReflectionException` is a throwable with no synthetic methods at all — the checker
    ///   injects it with the other builtin throwables.
    /// * `ReflectionReference`'s only public entry point is the static `fromArrayElement()`,
    ///   whose AOT body is the conservative `null` the checker builds; Magician owns the
    ///   hard-reference answer.
    #[test]
    fn reflection_class_list_matches_the_catalog() {
        const NOT_LOWERED: &[&str] = &[
            "ReflectionException",
            "ReflectionReference",
        ];
        let mut expected: Vec<&str> = elephc_builtin_contract::classes()
            .iter()
            .filter(|class| {
                class.module == elephc_builtin_contract::PhpModule::Reflection
                    && class.kind == elephc_builtin_contract::ClassKind::Class
                    && !NOT_LOWERED.contains(&class.name)
            })
            .map(|class| class.name)
            .collect();
        expected.sort_unstable();
        let mut actual: Vec<&str> = BUILTIN_REFLECTION_CLASS_NAMES.to_vec();
        actual.sort_unstable();
        assert_eq!(actual, expected);
    }
}

