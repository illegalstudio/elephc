//! Purpose:
//! Program metadata population and method ABI normalization.
//!
//! Called from:
//! - `crate::ir_lower::program`.
//!
//! Key details:
//! - Keeps program metadata deterministic and object-reference ABI shapes consistent.

use super::*;

/// Reserves extra field storage without changing PHP attributes or checker visibility rules.
pub(super) fn reserve_eval_subclass_property_storage(module: &mut Module) {
    if !module.required_runtime_features.eval_bridge {
        return;
    }
    for (name, info) in &mut module.class_infos {
        // Builtin objects may have custom payloads; their layouts remain authoritative.
        if elephc_builtin_contract::lookup_class(name).is_none() && !info.is_final {
            info.eval_property_storage = true;
        }
    }
}

/// Converts a PHP source path into the canonical display string stored in EIR metadata.
pub(super) fn canonical_source_path(source_path: &Path) -> String {
    source_path
        .canonicalize()
        .unwrap_or_else(|_| source_path.to_path_buf())
        .display()
        .to_string()
}

/// Copies declaration metadata into the EIR module placeholder tables.
pub(super) fn populate_metadata(module: &mut Module, program: &Program, check_result: &CheckResult) {
    module.class_table.names = sorted_keys(&check_result.classes);
    module.enum_table.names = sorted_keys(&check_result.enums);
    module.interface_table.names = sorted_keys(&check_result.interfaces);
    module.trait_table.names = collect_declared_trait_names(program);
    module.declared_class_names = collect_declared_class_names(program, &check_result.classes);
    module.declared_interface_names =
        collect_declared_interface_names(program, &check_result.interfaces);
    module.declared_trait_names = collect_declared_trait_names(program);
    module.declared_trait_source_lines = collect_declared_trait_source_lines(program);
    module.declared_function_source_lines = collect_declared_function_source_lines(program);
    module.declared_trait_uses = collect_declared_trait_uses(program);
    module.declared_trait_method_names = collect_declared_trait_method_names(program);
    module.declared_trait_methods = collect_declared_trait_methods(program);
    module.declared_trait_properties = collect_declared_trait_properties(program);
    module.declared_trait_property_names = module.declared_trait_properties.iter()
        .map(|(name, properties)| {
            (name.clone(), properties.iter().map(|property| property.name.clone()).collect())
        })
        .collect();
    module.declared_trait_constant_names = collect_declared_trait_constant_names(program);
    module.declared_trait_constants = collect_declared_trait_constants(program);
    module.declared_trait_constant_types = collect_declared_trait_constant_types(program);
    module.declared_trait_constant_visibilities =
        collect_declared_trait_constant_visibilities(program);
    module.declared_trait_final_constants = collect_declared_trait_final_constants(program);
    module.imported_trait_method_names = collect_imported_trait_method_names(program, &check_result.classes);
    module.class_infos = check_result.classes.clone();
    normalize_untyped_instance_storage_for_eir(&mut module.class_infos);
    normalize_class_method_signatures_for_eir(
        module,
        &check_result.callable_param_sigs,
        check_result.user_filters_reachable,
        check_result.user_filter_registered.as_ref(),
    );
    module.interface_infos = check_result.interfaces.clone();
    for interface in module.interface_infos.values_mut() {
        for signature in interface.methods.values_mut()
            .chain(interface.static_methods.values_mut())
        {
            for (index, (_, ty)) in signature.params.iter_mut().enumerate() {
                if signature.ref_params.get(index).copied().unwrap_or(false)
                    && matches!(ty, PhpType::Object(_))
                {
                    *ty = PhpType::Mixed;
                }
            }
        }
    }
    module.enum_infos = check_result.enums.clone();
    module.extern_class_infos = check_result.extern_classes.clone();
    module.packed_class_infos = check_result.packed_classes.clone();
    module.packed_layouts.names = sorted_keys(&check_result.packed_classes);
    module.extern_globals = check_result.extern_globals.clone();
    module.callable_param_sigs = check_result.callable_param_sigs.clone();
    module.extern_decls = check_result
        .extern_functions
        .values()
        .map(|sig| ExternDecl {
            name: sig.name.clone(),
            params: sig
                .params
                .iter()
                .zip(sig.abi_params.iter())
                .map(|((name, php_type), abi)| ExternParamDecl {
                    name: name.clone(),
                    ir_type: value_or_void_ir_type(php_type),
                    php_type: php_type.clone(),
                    abi: *abi,
                })
                .collect(),
            return_type: value_or_void_ir_type(&sig.return_type),
            return_php_type: sig.return_type.clone(),
            return_abi: sig.abi_return,
            link_libs: sig.library.iter().cloned().collect(),
        })
        .collect();
    // The checker keys these in a hash map; the module lists them in name order so every table
    // built from the list (function_exists candidates, link libraries) is the same on every run.
    module.extern_decls.sort_by(|left, right| left.name.cmp(&right.name));
    module.required_runtime_features =
        crate::codegen::runtime_features_for_program_and_classes(program, &check_result.classes);
    // Callable descriptors can introduce bridge calls only during assembly emission.
    // Retain their checker requirements alongside calls already present in EIR.
    module.required_runtime_features.mbstring = check_result.required_libraries
        .iter().any(|library| library == "elephc_mbstring");
    let features = module.required_runtime_features;
    if features.mbstring || features.mbregex || features.eval_bridge {
        // Library callers need the same default MIME provider and request initialization as CLI.
        // The CLI backend replaces these defaults when explicit startup overrides are supplied.
        module.mbstring_startup = Some(vec![b"UTF-8".to_vec(); 3]);
    }
}

/// Normalizes untyped instance slots that need EIR's runtime-dispatched storage representation.
///
/// Closed-world checking can infer a precise indexed or associative shape from the first writes
/// to an untyped property. PHP does not make that shape a contract: later assignments, casts,
/// merges, or nested keyed writes can change both storage kind and element representation. EIR
/// therefore picks ONE representation so all loads, stores, and cleanup agree.
///
/// That one has to be the HASH. `array<mixed>` is not a runtime-dispatched "either": consumers
/// read the STATIC type, so a hash parked in a packed slot is read as packed storage. `twig/twig`'s
/// `StagingExtension` is what that costs -- `private $functions = []` normalized to `array<mixed>`
/// while `$this->functions[$name] = $fn` had promoted the real property to a hash, so
/// `getFunctions(): array` asked the backend to convert a packed vector into the hash of objects
/// its contract had become, and five Twig accessors refused to lower. A hash represents every php
/// array; a packed vector cannot. Parking a list in a hash costs nothing observable: measured
/// against php 8.5.10, a hash holding sequential integer keys encodes as `["a","b"]` and
/// serializes as `a:2:{i:0;s:1:"a";i:1;s:1:"b";}`.
///
/// An untyped property with no statically attributable write remains `Void` in the checker, but
/// still needs a boxed `Mixed` cell because a bare `object` receiver can initialize it at runtime.
/// Declared PHP properties and static properties retain their explicit or specialized contracts.
fn normalize_untyped_instance_storage_for_eir(
    classes: &mut crate::fast_hash::FastMap<String, ClassInfo>,
) {
    for class_info in classes.values_mut() {
        class_info.normalize_untyped_array_property_storage();
        for index in 0..class_info.properties.len() {
            let property = class_info.properties[index].0.clone();
            if class_info.property_slot_is_declared(index, &property) {
                continue;
            }
            match class_info.properties[index].1.codegen_repr() {
                PhpType::Void | PhpType::Never => {
                    class_info.properties[index].1 = PhpType::Mixed;
                }
                _ => {}
            }
        }
    }
}

/// Normalizes class method metadata to the ABI contracts emitted in EIR.
pub(super) fn normalize_class_method_signatures_for_eir(
    module: &mut Module,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
    user_filters_reachable: bool,
    user_filter_registered: Option<&crate::fast_hash::FastSet<String>>,
) {
    // A stream filter is a registered class or a `php_user_filter` descendant (see
    // `types::is_user_filter_class`), not any class with a method named `filter`.
    let filter_classes: crate::fast_hash::FastSet<String> = if user_filters_reachable {
        module
            .class_infos
            .keys()
            .filter(|name| {
                crate::types::is_user_filter_class(
                    &module.class_infos,
                    user_filter_registered,
                    name,
                )
            })
            .cloned()
            .collect()
    } else {
        Default::default()
    };
    for (class_name, class_info) in module.class_infos.iter_mut() {
        let direct_property_returns = direct_this_property_return_types(class_info);
        let is_filter_class = filter_classes.contains(class_name);
        normalize_method_map_for_eir(
            class_name,
            &mut class_info.methods,
            &class_info.method_decls,
            false,
            callable_param_sigs,
            is_filter_class,
        );
        normalize_method_map_for_eir(
            class_name,
            &mut class_info.static_methods,
            &class_info.method_decls,
            true,
            callable_param_sigs,
            is_filter_class,
        );
        for (method, return_type) in direct_property_returns {
            if let Some(signature) = class_info.methods.get_mut(&method) {
                // A getter the checker already returns BOXED keeps the box: that is the storage an
                // override family or an untyped interface method agreed on, and narrowing it back
                // to the property type made `$this->name()` through `interface N { function
                // name(); }` read an unboxed string as Mixed and print "".
                if !signature.declared_return
                    && signature.return_type.codegen_repr() != PhpType::Mixed
                {
                    signature.return_type = return_type;
                }
            }
        }
    }
}

/// Collects untyped instance methods that directly return one `$this->property` slot.
///
/// Property ABI normalization happens after semantic checking. A getter inferred from the old
/// precise property shape must expose the normalized EIR storage type as well, otherwise return
/// lowering invents a conversion from `array<mixed>` back to stale phpdoc-like metadata.
fn direct_this_property_return_types(class_info: &ClassInfo) -> Vec<(String, PhpType)> {
    let mut returns = Vec::new();
    for method in &class_info.method_decls {
        if method.is_static || method.return_type.is_some() {
            continue;
        }
        let [stmt] = method.body.as_slice() else {
            continue;
        };
        let StmtKind::Return(Some(expr)) = &stmt.kind else {
            continue;
        };
        let ExprKind::PropertyAccess { object, property } = &expr.kind else {
            continue;
        };
        if !matches!(&object.kind, ExprKind::This) {
            continue;
        }
        let Some((_, (_, property_type))) = class_info.visible_property(property) else {
            continue;
        };
        returns.push((php_symbol_key(&method.name), property_type.clone()));
    }
    returns
}

/// Normalizes one instance/static method table for EIR call and bridge metadata.
pub(super) fn normalize_method_map_for_eir<S: std::hash::BuildHasher>(
    class_name: &str,
    methods: &mut std::collections::HashMap<String, FunctionSig, S>,
    method_decls: &[ClassMethod],
    is_static: bool,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
    is_filter_class: bool,
) {
    // Stream-wrapper and user-filter contract methods are invoked through
    // runtime vtables with raw fixed-ABI arguments; widening their untyped
    // params to boxed Mixed would desynchronize the dispatcher and the body.
    // Directory-only and metadata-only wrappers need not implement stream_open.
    // Match the same method inventory that populates the runtime wrapper vtable.
    let is_wrapper_class = methods.keys().any(|method| {
        crate::codegen_support::runtime::is_user_wrapper_contract_method(method)
    });
    // Only a program that can register a filter has one; see `Checker::user_filters_reachable`.
    let is_filter_class = is_filter_class && methods.contains_key("filter");
    for (method_key, signature) in methods.iter_mut() {
        if (is_wrapper_class
            && crate::codegen_support::runtime::is_user_wrapper_contract_method(method_key))
            || (is_filter_class
                && crate::codegen_support::runtime::is_user_filter_contract_method(method_key))
        {
            continue;
        }
        let owner_name = format!("{}::{}", class_name, method_key);
        let mut normalized = function::eir_signature_with_php_param_contracts(
            &owner_name,
            signature,
            callable_param_sigs,
        );
        if method_decls
            .iter()
            .find(|method| {
                method.is_static == is_static && php_symbol_key(&method.name) == *method_key
            })
            .is_some_and(|method| {
                method_return_exposes_dynamic_param(
                    method,
                    signature,
                    &owner_name,
                    callable_param_sigs,
                )
            })
        {
            normalized.return_type = PhpType::Mixed;
        }
        *signature = normalized;
    }
}

/// Returns true when an untyped method return can expose a dynamic parameter directly.
///
/// The rule itself lives in `crate::types::dynamic_params`, because the checker has to reach the
/// same answer when it records a function's inferred return type: when the two disagreed, the
/// callee returned a boxed cell that the caller read as a raw integer (issue #576).
pub(super) fn method_return_exposes_dynamic_param(
    method: &ClassMethod,
    signature: &FunctionSig,
    owner_name: &str,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
) -> bool {
    crate::types::dynamic_params::return_exposes_dynamic_param(
        &method.body,
        signature,
        owner_name,
        callable_param_sigs,
    )
}
