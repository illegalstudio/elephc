//! Purpose:
//! Builds EIR function bodies from AST function-like declarations and main code.
//!
//! Called from:
//! - `crate::ir_lower::program` while assembling an EIR module.
//!
//! Key details:
//! - Function parameters are represented as metadata plus initialized PHP local
//!   slots; the Phase 03 EIR contract keeps PHP locals addressable.
//! - Every lowered function leaves all blocks terminated before validation.

use crate::ir::{
    Builder, Function, FunctionFlags, FunctionParam, Immediate, IrType, Module,
    Op, Ownership, Terminator,
};
use crate::ir_lower::context::{
    return_ir_type, type_expr_to_php_type, value_ir_type, ClosureCapture, LoweringContext,
    StaticCallableBinding,
};
use crate::ir_lower::effects_lookup;
use crate::names::php_symbol_key;
use crate::parser::ast::{
    AttributeGroup, BinOp, CastType, ClassMethod, Expr, ExprKind, Program, Stmt, StmtKind,
    TypeExpr,
};
use crate::names::Name;
use crate::span::Span;
use crate::types::{
    collect_attribute_args, collect_attribute_names, CheckResult, ClassInfo, FunctionSig,
    PackedClassInfo, PhpType, ReturnArgAlias, TypeEnv,
};

mod builtin_wrappers;
mod closure_defaults;
pub(crate) use crate::types::deferred_defaults::contains_deferred_error;
mod object_comparisons;
pub(super) use object_comparisons::lower_object_comparison_dispatch;
pub(crate) use builtin_wrappers::{lower_array_merge_callable, lower_boxed_usort_callable, lower_prepared_builtin_callable};
mod interface_adapters;
pub(crate) use interface_adapters::lower_optional_interface_adapters;

/// AST parameter tuple shape used by function, method, and closure declarations.
type AstParams = [(
    String,
    Option<TypeExpr>,
    Option<crate::parser::ast::Expr>,
    bool,
)];

/// Scope handle a scope-aware AOT eval fragment receives by value.
///
/// Both spellings live in `crate::names` so the EIR producer here and every backend
/// exact-name lookup read one definition, and so both carry the generated-local marker
/// that keeps them out of the fragment's `get_defined_vars()`.
const EVAL_AOT_SCOPE_PARAM: &str = crate::names::EVAL_AOT_SCOPE_LOCAL;

/// Compile-time callable binding to seed for a self-recursive closure capture.
struct RecursiveClosureBinding {
    local_name: String,
    closure_name: String,
    signature: FunctionSig,
    captures: Vec<(String, bool)>,
    return_alias: ReturnArgAlias,
}

/// Lowers the top-level statement list as the synthetic `main` EIR function.
pub(crate) fn lower_main(
    program: &Program,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &crate::fast_hash::FastMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &crate::fast_hash::FastMap<String, FunctionSig>,
) {
    let web = module.web;
    let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
    function.flags.is_main = true;
    let all_global_var_names = collect_global_var_names(program);
    let mut function_global_names =
        crate::ir_lower::program::collect_function_global_names(program);
    crate::ir_lower::program::close_function_global_names_over_calls(
        &mut function_global_names,
        &module.functions,
        &module.data.function_names,
    );
    let top_level_env = web_gated_global_env(&check_result.global_env, web);
    let closures = lower_body_into_function(
        module.shared_source_catalog(),
        module.runtime_bound_functions.clone(),
        &mut function,
        None,
        &mut module.data,
        program,
        top_level_env.clone(),
        top_level_env,
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.extern_globals,
        &check_result.callable_param_sigs,
        &check_result.return_alias_summaries,
        fiber_return_sigs,
        &module.class_infos,
        &check_result.enums,
        &check_result.interfaces,
        &module.declared_trait_names,
        &module.declared_trait_methods,
        &module.declared_trait_properties,
        &check_result.packed_classes,
        &check_result.extern_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        &check_result.closure_generator_spans,
        &check_result.array_push_property_plans,
        &check_result.member_receiver_types,
        &check_result.flow_typed_property_accesses,
        &check_result.boxed_reference_promotion_sites,
        &check_result.first_class_builtin_call_types,
        &check_result.generic_call_sites,
        &check_result.loop_storage_types,
        &check_result.string_incdec_locals,
        &check_result.by_ref_local_storage_types,
        &check_result.dynamic_ref_local_types,
        &check_result.local_bind_kill_sites,
        &check_result.local_ref_detach_sites,
        &check_result.local_retype_sites,
        &check_result.mixed_storage_store_sites,
        "main".to_string(),
        constants,
        None,
        PhpType::Void,
        false,
        &[],
        None,
        true,
        all_global_var_names,
        Some(function_global_names),
        module.source_path.clone(),
        None,
        web,
    );
    add_closures(module, closures);
    module.add_function(function);
}

/// Returns `global_env` with request-superglobal entries removed unless `web`.
///
/// `check_result.global_env` (the checker's top-level environment) always
/// carries the fixed `AssocArray{Str, Mixed}` type for `$_SERVER`/`$_SESSION`/…
/// because the checker seeds every scope so PHP source can read/write them
/// without a `global` declaration. Only `--web` builds pre-initialize the
/// shared `_eir_global_*` storage those types imply; a non-web `main`/function
/// env must not inherit the seeded type, or a bare read dereferences a
/// zeroed (never-initialized) global as if it were a live Hash pointer and
/// crashes. Stripping the entries here makes the env-derived type lookups
/// fall back to `Mixed`, matching `env_from_signature`'s web gate.
fn web_gated_global_env(global_env: &TypeEnv, web: bool) -> TypeEnv {
    if web {
        return global_env.clone();
    }
    let mut env = global_env.clone();
    for name in crate::superglobals::SUPERGLOBALS {
        env.remove(*name);
    }
    env
}

/// Collects PHP variable names that must live in program-global storage.
///
/// The `global` walk itself is shared with the CHECKER (`crate::global_decls`), which vetoes ending
/// a top-level binding of one of these names for the same reason lowering refuses to abandon its
/// slot: `global $x;` in some other body reaches the very storage the top-level name uses. One walk
/// keeps the two sides identical, blind spots included — and the blind spots are load-bearing here.
/// This set decides STORAGE CLASS: a name in it moves out of main's frame slot into the
/// `_eir_global_*` symbol, which types it `Mixed`, and the array builtins have pre-existing
/// `Mixed`-array backend gaps, so widening the walk to closure bodies or enum methods broke programs
/// that compile and print PHP's output today (`implode` crashed, `array_sum`/`sort`/`in_array` and
/// friends became a hard backend error). `crate::global_decls`' preamble carries those measurements,
/// the matching reason the checker's veto must not be widened on its own either, and the
/// pre-existing closure-`global` write loss both sides preserve.
///
/// LOWERING additionally folds in every `$GLOBALS['x']` alias the program reaches. An explicit
/// `global $x` declaration and `$GLOBALS['x']` name the same slot, so collecting both keeps a
/// top-level `$x` from staying frame-local while a function reads the global. Only the STORAGE
/// decision reads the widened set; the checker's `unset`-kill veto still reads `crate::global_decls`
/// alone, because an alias read never forces a binding to end.
fn collect_global_var_names(statements: &[Stmt]) -> crate::fast_hash::FastSet<String> {
    let mut names = crate::global_decls::collect_global_var_names(statements);
    let mut usage = crate::ast_usage::Usage::default();
    for stmt in statements {
        usage.merge(crate::ast_usage::collect_stmt(stmt));
    }
    names.extend(usage.globals_keys);
    names
}

/// Lowers one user-defined function declaration into an EIR function.
pub(crate) fn lower_user_function(
    name: &str,
    is_internal: bool,
    params: &AstParams,
    return_type: Option<&TypeExpr>,
    attributes: &[AttributeGroup],
    body: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &crate::fast_hash::FastMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &crate::fast_hash::FastMap<String, FunctionSig>,
) {
    let web = module.web;
    let fallback = signature_from_ast(params, return_type);
    let signature = check_result.functions.get(name).unwrap_or(&fallback);
    let mut eir_signature =
        eir_signature_with_php_param_contracts(name, signature, &check_result.callable_param_sigs);
    // The body builds a `[null]` it returns with boxed slots, as every caller reads it
    // (`caller_visible_return_type`).
    eir_signature.return_type =
        crate::types::dynamic_params::null_only_container_return_storage(&eir_signature.return_type);
    // A generator's compiled body is a coroutine that returns the value passed
    // to `return` (Mixed, read back via `Generator::getReturn()`), not the
    // `Generator` object itself. The public signature stays `Generator` for
    // callers; only the EIR body return type becomes Mixed so `return $x`
    // lowers to a plain boxed Mixed return instead of a Generator coercion.
    let body_return_type = generator_body_return_type(body, &eir_signature.return_type, signature.is_generator);
    let mut function = Function::new(
        name.to_string(),
        return_ir_type(&body_return_type),
        body_return_type.clone(),
    );
    function.params = function_params(&eir_signature);
    function.flags.is_internal = is_internal;
    function.flags.by_ref_return = signature.by_ref_return && !signature.is_generator;
    function.source_signature = Some(source_signature(name, &eir_signature));
    function.signature = Some(eir_runtime_metadata_signature(&eir_signature));
    function.source_defaults = params.iter().map(|(_, _, default, _)| default.clone()).collect();
    function.attribute_names = check_result
        .function_attribute_names
        .get(name)
        .cloned()
        .unwrap_or_else(|| collect_attribute_names(attributes));
    function.attribute_args = check_result
        .function_attribute_args
        .get(name)
        .cloned()
        .unwrap_or_else(|| collect_attribute_args(attributes));
    mark_generator_if_needed(
        &mut function,
        body,
        signature.is_generator,
    );
    let closures = lower_body_into_function(
        module.shared_source_catalog(),
        module.runtime_bound_functions.clone(),
        &mut function,
        None,
        &mut module.data,
        body,
        env_from_signature(&eir_signature, web),
        web_gated_global_env(&check_result.global_env, web),
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.extern_globals,
        &check_result.callable_param_sigs,
        &check_result.return_alias_summaries,
        fiber_return_sigs,
        &module.class_infos,
        &check_result.enums,
        &check_result.interfaces,
        &module.declared_trait_names,
        &module.declared_trait_methods,
        &module.declared_trait_properties,
        &check_result.packed_classes,
        &check_result.extern_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        &check_result.closure_generator_spans,
        &check_result.array_push_property_plans,
        &check_result.member_receiver_types,
        &check_result.flow_typed_property_accesses,
        &check_result.boxed_reference_promotion_sites,
        &check_result.first_class_builtin_call_types,
        &check_result.generic_call_sites,
        &check_result.loop_storage_types,
        &check_result.string_incdec_locals,
        &check_result.by_ref_local_storage_types,
        &check_result.dynamic_ref_local_types,
        &check_result.local_bind_kill_sites,
        &check_result.local_ref_detach_sites,
        &check_result.local_retype_sites,
        &check_result.mixed_storage_store_sites,
        name.to_string(),
        constants,
        None,
        body_return_type.clone(),
        signature.declared_return,
        &eir_signature.params,
        None,
        false,
        crate::fast_hash::FastSet::default(),
        None,
        module.source_path.clone(),
        None,
        web,
    );
    add_closures(module, closures);
    module.add_function(function);
}

/// Lowers one class-like method body into an EIR class-method function.
pub(crate) fn lower_class_method(
    class_name: &str,
    method_name: &str,
    is_static: bool,
    params: &AstParams,
    return_type: Option<&TypeExpr>,
    body: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &crate::fast_hash::FastMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &crate::fast_hash::FastMap<String, FunctionSig>,
) {
    let web = module.web;
    let fallback = signature_from_ast(params, return_type);
    let signature = module
        .class_infos
        .get(class_name)
        .and_then(|class| method_signature(class, method_name, is_static))
        .cloned()
        .unwrap_or(fallback);
    let name = format!("{}::{}", class_name, method_name);
    // Generator methods lower their body as a Mixed-returning coroutine; see
    // `generator_body_return_type`.
    let method_body_return_type = generator_body_return_type(body, &signature.return_type, signature.is_generator);
    let mut function = Function::new(
        name.clone(),
        return_ir_type(&method_body_return_type),
        method_body_return_type.clone(),
    );
    function.lexical_class = Some(class_name.to_string());
    function.flags = FunctionFlags {
        is_method: true,
        is_static,
        by_ref_return: signature.by_ref_return && !signature.is_generator,
        ..FunctionFlags::default()
    };
    function.source_signature = Some(source_signature(&name, &signature));
    function.signature = Some(eir_runtime_metadata_signature(&signature));
    let mut env = env_from_signature(&signature, web);
    let mut body_params = signature.params.clone();
    if is_static {
        // The hidden late-static-binding argument keeps its leading position and its `Int`
        // type; only the NAME is the marked one from `crate::names`, so the slot stays
        // invisible to `get_defined_vars()` and to eval scope synchronization.
        let hidden_called_class =
            (crate::names::CALLED_CLASS_ID_LOCAL.to_string(), PhpType::Int);
        function.params.push(FunctionParam {
            name: hidden_called_class.0.clone(),
            ir_type: value_ir_type(&hidden_called_class.1),
            php_type: hidden_called_class.1.clone(),
            by_ref: false,
            variadic: false,
        });
        env.insert(hidden_called_class.0.clone(), hidden_called_class.1.clone());
        body_params.insert(0, hidden_called_class);
    } else {
        let this_type = PhpType::Object(class_name.to_string());
        function.params.push(FunctionParam {
            name: "this".to_string(),
            ir_type: value_ir_type(&this_type),
            php_type: this_type.clone(),
            by_ref: false,
            variadic: false,
        });
        env.insert("this".to_string(), this_type.clone());
        body_params.insert(0, ("this".to_string(), this_type));
    }
    function.params.extend(function_params(&signature));
    mark_generator_if_needed(
        &mut function,
        body,
        signature.is_generator,
    );
    let closures = lower_body_into_function(
        module.shared_source_catalog(),
        module.runtime_bound_functions.clone(),
        &mut function,
        None,
        &mut module.data,
        body,
        env,
        web_gated_global_env(&check_result.global_env, web),
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.extern_globals,
        &check_result.callable_param_sigs,
        &check_result.return_alias_summaries,
        fiber_return_sigs,
        &module.class_infos,
        &check_result.enums,
        &check_result.interfaces,
        &module.declared_trait_names,
        &module.declared_trait_methods,
        &module.declared_trait_properties,
        &check_result.packed_classes,
        &check_result.extern_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        &check_result.closure_generator_spans,
        &check_result.array_push_property_plans,
        &check_result.member_receiver_types,
        &check_result.flow_typed_property_accesses,
        &check_result.boxed_reference_promotion_sites,
        &check_result.first_class_builtin_call_types,
        &check_result.generic_call_sites,
        &check_result.loop_storage_types,
        &check_result.string_incdec_locals,
        &check_result.by_ref_local_storage_types,
        &check_result.dynamic_ref_local_types,
        &check_result.local_bind_kill_sites,
        &check_result.local_ref_detach_sites,
        &check_result.local_retype_sites,
        &check_result.mixed_storage_store_sites,
        name.clone(),
        constants,
        Some(class_name.to_string()),
        method_body_return_type.clone(),
        // A `: void` override of an untyped ancestor keeps the ancestor's `mixed` return ABI, and
        // falling off its end must return null, not throw "must be of type mixed, none
        // returned": Doctrine's `PersistentCollection::doInitialize(): void` overrides the
        // untyped abstract `AbstractLazyCollection::doInitialize()`.
        signature.declared_return && !matches!(return_type, Some(crate::parser::ast::TypeExpr::Void)),
        &body_params,
        None,
        false,
        crate::fast_hash::FastSet::default(),
        None,
        module.source_path.clone(),
        None,
        web,
    );
    add_closures(module, closures);
    module.class_methods.push(function);
}

/// Returns the local-binding decision maps an eval-AOT fragment lowers against: all three EMPTY.
///
/// A fragment is parsed from a string literal, so every span in it is measured from line 1 of
/// THAT string. Those spans live in a space of their own that no pass over the program ever
/// visits: the ambiguity tally (`checker::binding_decision_ambiguity`) counts the nodes of
/// `program` only, so it cannot see a fragment node and cannot report a collision with one.
///
/// A key that matched anyway would therefore be an ACCIDENT — two unrelated nodes at the same line
/// and column — and acting on it is never right: the fragment's code was never CHECKED, so no
/// decision in these maps was ever made about it. Handing over empty maps is the structural
/// statement of that, and it is observable: the outer program's marked `$b` recorded a store site
/// at 2:1, an eval string's own `$b = 9;` sat at 2:1 of that string, and the mixed pre-declare gave
/// the fragment's unrelated local boxed storage nothing had asked for.
fn eval_aot_decision_maps() -> (
    crate::fast_hash::FastMap<Span, crate::fast_hash::FastSet<String>>,
    crate::fast_hash::FastMap<Span, crate::fast_hash::FastSet<String>>,
    crate::fast_hash::FastMap<Span, crate::fast_hash::FastSet<String>>,
    crate::fast_hash::FastMap<Span, crate::fast_hash::FastSet<String>>,
    crate::fast_hash::FastMap<(String, Span), crate::fast_hash::FastSet<String>>,
) {
    (
        crate::fast_hash::FastMap::default(),
        crate::fast_hash::FastMap::default(),
        crate::fast_hash::FastMap::default(),
        crate::fast_hash::FastMap::default(),
        crate::fast_hash::FastMap::default(),
    )
}

/// Lowers one no-scope literal eval fragment as an internal EIR function.
pub(crate) fn lower_eval_aot_function(
    name: &str,
    body: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &crate::fast_hash::FastMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &crate::fast_hash::FastMap<String, FunctionSig>,
) {
    let return_type = PhpType::Mixed;
    let signature = FunctionSig {
        params: Vec::new(),
        param_type_exprs: Vec::new(),
        param_attributes: Vec::new(),
        defaults: Vec::new(),
        return_type: return_type.clone(),
        declared_return: false,
        by_ref_return: false,
        ref_params: Vec::new(),
        declared_params: Vec::new(),
        variadic: None,
        deprecation: None,
        is_generator: false,
    };
    let mut function = Function::new(
        name.to_string(),
        return_ir_type(&return_type),
        return_type.clone(),
    );
    function.source_signature = Some(source_signature(name, &signature));
    function.signature = Some(eir_runtime_metadata_signature(&signature));
    let (
        bind_kill_sites,
        ref_detach_sites,
        retype_sites,
        mixed_storage_store_sites,
        boxed_reference_promotion_sites,
    ) = eval_aot_decision_maps();
    let closures = lower_body_into_function(
        module.shared_source_catalog(),
        module.runtime_bound_functions.clone(),
        &mut function,
        None,
        &mut module.data,
        body,
        TypeEnv::default(),
        check_result.global_env.clone(),
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.extern_globals,
        &check_result.callable_param_sigs,
        &check_result.return_alias_summaries,
        fiber_return_sigs,
        &module.class_infos,
        &check_result.enums,
        &check_result.interfaces,
        &module.declared_trait_names,
        &module.declared_trait_methods,
        &module.declared_trait_properties,
        &check_result.packed_classes,
        &check_result.extern_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        &check_result.closure_generator_spans,
        &check_result.array_push_property_plans,
        &check_result.member_receiver_types,
        &check_result.flow_typed_property_accesses,
        &boxed_reference_promotion_sites,
        &check_result.first_class_builtin_call_types,
        &check_result.generic_call_sites,
        &check_result.loop_storage_types,
        &check_result.string_incdec_locals,
        &check_result.by_ref_local_storage_types,
        &check_result.dynamic_ref_local_types,
        &bind_kill_sites,
        &ref_detach_sites,
        &retype_sites,
        &mixed_storage_store_sites,
        "main".to_string(),
        constants,
        None,
        return_type,
        signature.declared_return,
        &[],
        None,
        false,
        collect_global_var_names(body),
        None,
        module.source_path.clone(),
        None,
        module.web,
    );
    add_closures(module, closures);
    module.add_function(function);
}

/// Lowers one literal eval fragment as an internal scope-aware EIR function.
pub(crate) fn lower_eval_aot_scope_function(
    name: &str,
    body: &[Stmt],
    scope_reads: &std::collections::BTreeSet<String>,
    scope_direct_writes: &std::collections::BTreeSet<String>,
    scope_flush_writes: &std::collections::BTreeSet<String>,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &crate::fast_hash::FastMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &crate::fast_hash::FastMap<String, FunctionSig>,
) {
    let return_type = PhpType::Mixed;
    let use_read_params =
        !scope_reads.is_empty() && scope_direct_writes.is_empty() && scope_flush_writes.is_empty();
    let params = if use_read_params {
        scope_reads
            .iter()
            .map(|name| (name.clone(), PhpType::Mixed))
            .collect::<Vec<_>>()
    } else {
        vec![(EVAL_AOT_SCOPE_PARAM.to_string(), PhpType::Int)]
    };
    let signature = FunctionSig {
        params,
        param_type_exprs: Vec::new(),
        param_attributes: Vec::new(),
        defaults: Vec::new(),
        return_type: return_type.clone(),
        declared_return: false,
        by_ref_return: false,
        ref_params: vec![
            false;
            if use_read_params {
                scope_reads.len()
            } else {
                1
            }
        ],
        declared_params: vec![
            false;
            if use_read_params {
                scope_reads.len()
            } else {
                1
            }
        ],
        variadic: None,
        deprecation: None,
        is_generator: false,
    };
    let mut function = Function::new(
        name.to_string(),
        return_ir_type(&return_type),
        return_type.clone(),
    );
    function.params = function_params(&signature);
    function.source_signature = Some(source_signature(name, &signature));
    function.signature = Some(eir_runtime_metadata_signature(&signature));
    let mut env = TypeEnv::default();
    for (param_name, param_type) in &signature.params {
        env.insert(param_name.clone(), param_type.clone());
    }
    let eval_scope_reads = (!use_read_params).then(|| {
        (
            EVAL_AOT_SCOPE_PARAM.to_string(),
            scope_reads.iter().cloned().collect(),
            scope_direct_writes.iter().cloned().collect(),
            scope_flush_writes.clone(),
        )
    });
    let (
        bind_kill_sites,
        ref_detach_sites,
        retype_sites,
        mixed_storage_store_sites,
        boxed_reference_promotion_sites,
    ) = eval_aot_decision_maps();
    let closures = lower_body_into_function(
        module.shared_source_catalog(),
        module.runtime_bound_functions.clone(),
        &mut function,
        None,
        &mut module.data,
        body,
        env,
        check_result.global_env.clone(),
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.extern_globals,
        &check_result.callable_param_sigs,
        &check_result.return_alias_summaries,
        fiber_return_sigs,
        &module.class_infos,
        &check_result.enums,
        &check_result.interfaces,
        &module.declared_trait_names,
        &module.declared_trait_methods,
        &module.declared_trait_properties,
        &check_result.packed_classes,
        &check_result.extern_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        &check_result.closure_generator_spans,
        &check_result.array_push_property_plans,
        &check_result.member_receiver_types,
        &check_result.flow_typed_property_accesses,
        &boxed_reference_promotion_sites,
        &check_result.first_class_builtin_call_types,
        &check_result.generic_call_sites,
        &check_result.loop_storage_types,
        &check_result.string_incdec_locals,
        &check_result.by_ref_local_storage_types,
        &check_result.dynamic_ref_local_types,
        &bind_kill_sites,
        &ref_detach_sites,
        &retype_sites,
        &mixed_storage_store_sites,
        "main".to_string(),
        constants,
        None,
        return_type,
        signature.declared_return,
        &signature.params,
        None,
        false,
        collect_global_var_names(body),
        None,
        module.source_path.clone(),
        eval_scope_reads,
        module.web,
    );
    add_closures(module, closures);
    module.add_function(function);
}

/// Builds fallback method signature metadata from parsed class-like method syntax.
///
/// Trait reflection reads this signature, so it keeps the declaration's `function &m()`
/// flag for `ReflectionMethod::returnsReference()`.
pub(crate) fn method_signature_from_ast(method: &ClassMethod) -> FunctionSig {
    let mut signature = signature_from_ast_with_variadic(
        &method.params,
        method.return_type.as_ref(),
        method.variadic.as_deref(),
        method.variadic_by_ref,
    );
    signature.by_ref_return = method.by_ref_return && !method.name.eq_ignore_ascii_case("__get");
    if !method.variadic_by_ref {
        if let Some(variadic_type) = &method.variadic_type {
            if let Some((_, php_type)) = signature.params.last_mut() {
                *php_type = type_expr_to_php_type(variadic_type);
            }
            if let Some(declared) = signature.declared_params.last_mut() {
                *declared = true;
            }
        }
    }
    signature
}

/// Lowers a synthetic `_class_propinit_<id>` function for dynamic by-name allocation.
pub(crate) fn lower_property_init_thunk(
    class_name: &str,
    class_info: &ClassInfo,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &crate::fast_hash::FastMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &crate::fast_hash::FastMap<String, FunctionSig>,
) {
    if !super::property_initializers::needs_initializer(class_info) {
        return;
    }
    lower_property_default_thunk(
        format!("_class_propinit_{}", class_info.class_id),
        Vec::new(),
        Some(class_info),
        class_name,
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
}

/// Lowers a synthetic `_class_propinit_private_<id>` function: the defaults of the private
/// properties the class declares itself, assigned in its own scope.
///
/// A by-name allocation of a descendant that hides one of those slots behind a same-named
/// declaration runs this after its own thunk, which cannot name the hidden slot.
pub(crate) fn lower_private_property_init_thunk(
    class_name: &str,
    class_info: &ClassInfo,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &crate::fast_hash::FastMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &crate::fast_hash::FastMap<String, FunctionSig>,
) {
    let body = property_init_body(class_info, |index| {
        class_info.property_slot_in_private_init_thunk(index, class_name)
    });
    lower_property_default_thunk(
        format!("_class_propinit_private_{}", class_info.class_id),
        body,
        None,
        class_name,
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
}

/// Lowers one property-default thunk body into a `$this`-only synthetic function.
fn lower_property_default_thunk(
    function_name: String,
    body: Vec<Stmt>,
    property_initializers: Option<&ClassInfo>,
    class_name: &str,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &crate::fast_hash::FastMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &crate::fast_hash::FastMap<String, FunctionSig>,
) {
    let web = module.web;
    let this_type = PhpType::Object(class_name.to_string());
    let mut function = Function::new(function_name.clone(), IrType::Void, PhpType::Void);
    function.flags.is_synthetic = true;
    // The defaults are assigned in the class's own scope. Codegen resolves a property name
    // against this scope: without it, a private slot the class declares counted as invisible
    // global-scope access, and the private thunk of a class whose descendant hides that slot
    // wrote the default into a dynamic-property hash the object does not have.
    function.lexical_class = Some(class_name.trim_start_matches('\\').to_string());
    function.params.push(FunctionParam {
        name: "this".to_string(),
        ir_type: value_ir_type(&this_type),
        php_type: this_type.clone(),
        by_ref: false,
        variadic: false,
    });
    let sig = FunctionSig {
        params: vec![("this".to_string(), this_type.clone())],
        param_type_exprs: vec![None],
        param_attributes: Vec::new(),
        defaults: vec![None],
        return_type: PhpType::Void,
        declared_return: false,
        by_ref_return: false,
        ref_params: vec![false],
        declared_params: vec![false],
        variadic: None,
        deprecation: None,
        is_generator: false,
    };
    function.source_signature = Some(source_signature(&function_name, &sig));
    function.signature = Some(eir_runtime_metadata_signature(&sig));
    let mut env = TypeEnv::default();
    env.insert("this".to_string(), this_type.clone());
    let params = vec![("this".to_string(), this_type)];
    let closures = lower_body_into_function(
        module.shared_source_catalog(),
        module.runtime_bound_functions.clone(),
        &mut function,
        property_initializers,
        &mut module.data,
        &body,
        env,
        web_gated_global_env(&check_result.global_env, web),
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.extern_globals,
        &check_result.callable_param_sigs,
        &check_result.return_alias_summaries,
        fiber_return_sigs,
        &module.class_infos,
        &check_result.enums,
        &check_result.interfaces,
        &module.declared_trait_names,
        &module.declared_trait_methods,
        &module.declared_trait_properties,
        &check_result.packed_classes,
        &check_result.extern_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        &check_result.closure_generator_spans,
        &check_result.array_push_property_plans,
        &check_result.member_receiver_types,
        &check_result.flow_typed_property_accesses,
        &check_result.boxed_reference_promotion_sites,
        &check_result.first_class_builtin_call_types,
        &check_result.generic_call_sites,
        &check_result.loop_storage_types,
        &check_result.string_incdec_locals,
        &check_result.by_ref_local_storage_types,
        &check_result.dynamic_ref_local_types,
        &check_result.local_bind_kill_sites,
        &check_result.local_ref_detach_sites,
        &check_result.local_retype_sites,
        &check_result.mixed_storage_store_sites,
        function_name.clone(),
        constants,
        Some(class_name.to_string()),
        PhpType::Void,
        false,
        &params,
        None,
        false,
        crate::fast_hash::FastSet::default(),
        None,
        module.source_path.clone(),
        None,
        web,
    );
    add_closures(module, closures);
    module.add_function(function);
}

/// Lowers a synthetic `_class_ctor_<id>_<argc>` thunk that pads a constructor call with defaults.
///
/// `new $c(…)` names its class in a string, so the checker cannot pad the call with the
/// constructor's default arguments the way it does when the class is written down — it does not
/// know which constructor it is. Codegen dispatches on the class at run time and therefore knows,
/// but by then the arguments are materialized SSA values and a default expression is not one.
///
/// The thunk closes that gap where expressions can still be lowered: it takes `$this` plus the
/// arguments the site actually passed, and calls the real constructor with those followed by the
/// declared defaults, spliced in as ordinary AST. Codegen then calls one symbol and needs to know
/// nothing about defaults.
///
/// Without it every class whose constructor has an optional parameter was refused as a candidate
/// on a strict arity comparison and fell to the generic allocation path, which produces an object
/// with no constructor run at all: `new $c(["a" => 1])` on `ArrayObject` answered
/// `ArrayObject|0|` where PHP answers `ArrayObject|1|1`.
/// Builds `if (<not coercible>) { throw new TypeError(...); }` for one overflow argument.
///
/// php COERCES a scalar into a typed variadic collector — `"7"` and `1.5` both reach an
/// `int ...$r` — and raises a `TypeError` only for a value it cannot read as one. Casting without
/// this guard would turn that TypeError into a silent `(int)"x" === 0`, which is the failure this
/// whole path exists to stop.
///
/// The predicate is per target: a numeric target takes anything numeric, plus booleans, which php
/// converts but `is_numeric()` reports false for. A string or bool target takes any scalar.
///
/// The message names the class and the argument position — both known while this is built — and
/// asks `gettype()` for the part that is only known at run time. It does NOT carry php's
/// `called in FILE on line N` clause: this thunk is shared by every site of the same arity, so it
/// has no single call line to name, and inventing one would be worse than omitting it.
fn coercible_or_throw_stmt(
    class_name: &str,
    variable: &Expr,
    argument: usize,
    target: CastType,
    span: Span,
) -> Stmt {
    let predicate = |name: &str| {
        Expr::new(
            ExprKind::FunctionCall {
                name: Name::unqualified(name),
                args: vec![variable.clone()],
            },
            span,
        )
    };
    let accepted = match target {
        CastType::Int | CastType::Float => Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(predicate("is_numeric")),
                op: BinOp::Or,
                right: Box::new(predicate("is_bool")),
            },
            span,
        ),
        _ => predicate("is_scalar"),
    };
    let expected = match target {
        CastType::Int => "int",
        CastType::Float => "float",
        CastType::String => "string",
        _ => "bool",
    };
    let concat = |left: Expr, right: Expr| {
        Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(left),
                op: BinOp::Concat,
                right: Box::new(right),
            },
            span,
        )
    };
    let literal = |text: String| Expr::new(ExprKind::StringLiteral(text), span);
    let message = concat(
        literal(format!(
            "{}::__construct(): Argument #{} expects {}, ",
            class_name, argument, expected
        )),
        concat(predicate("gettype"), literal(" given".to_string())),
    );
    Stmt::new(
        StmtKind::If {
            condition: Expr::new(ExprKind::Not(Box::new(accepted)), span),
            then_body: vec![Stmt::new(
                StmtKind::Throw(Expr::new(
                    ExprKind::NewObject {
                        class_name: Name::unqualified("TypeError"),
                        args: vec![message],
                    },
                    span,
                )),
                span,
            )],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        span,
    )
}

/// Lowers the thunk that `new $class(...)` calls when the class is only known
/// at run time.
///
/// The thunk pads the call with the constructor's declared defaults for the
/// arguments the site did not provide, which a dynamic call site cannot know.
pub(crate) fn lower_dynamic_constructor_thunk(
    class_name: &str,
    class_info: &ClassInfo,
    provided_args: usize,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &crate::fast_hash::FastMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &crate::fast_hash::FastMap<String, FunctionSig>,
) {
    let function_name = dynamic_constructor_thunk_name(class_info.class_id, provided_args);
    if module
        .functions
        .iter()
        .any(|function| function.name == function_name)
    {
        return;
    }
    let Some(constructor) = class_info.methods.get(&php_symbol_key("__construct")) else {
        return;
    };
    let regular = crate::types::call_args::regular_param_count(constructor);
    let is_variadic = constructor.variadic.is_some();
    if is_variadic {
        // A VARIADIC constructor needs a thunk at EVERY arity, including its declared one. The
        // site passes N separate arguments; the lowered callee takes the collector as ONE array
        // (`int ...$r` becomes a single `array<int>` parameter), so there is no arity at which the
        // site's frame already matches. Without the thunk the class dropped out of the ladder and
        // `new $c(...)` allocated by name with the constructor never run — measured wrong at 11 of
        // 12 shape/arity combinations, against a STATIC `new V(...)` that works.
        //
        // Only the omitted REGULAR parameters need a default to splice. The collector is padded
        // with nothing: the thunk body simply passes fewer arguments and the call lowering builds
        // the empty collection, exactly as it does for a static `new V()`.
        if provided_args < regular
            && constructor.defaults[provided_args..regular]
                .iter()
                .any(|default| default.is_none())
        {
            return;
        }
    } else {
        // Only the padding case: an exact arity needs no thunk, and a site passing MORE arguments
        // than a non-collecting constructor declares is not a candidate at all.
        if provided_args >= constructor.params.len() {
            return;
        }
        // Every omitted parameter must have a default to splice. One without is a call PHP would
        // reject too, so there is nothing to build.
        if constructor.defaults[provided_args..]
            .iter()
            .any(|default| default.is_none())
        {
            return;
        }
    }
    // A by-reference parameter would have to carry the callee's write back out through the thunk,
    // and no builtin constructor declares one, so that path has never been exercised. Refusing
    // leaves the site on the behaviour it had before padding existed rather than risking a write
    // that silently goes nowhere. The collector's own flag counts too, since `&...$out` would have
    // the same problem for every argument that lands in it.
    if constructor
        .ref_params
        .iter()
        .take(provided_args.min(regular))
        .any(|&by_ref| by_ref)
    {
        return;
    }
    if is_variadic
        && provided_args > regular
        && constructor.ref_params.get(regular).copied().unwrap_or(false)
    {
        return;
    }
    // A TYPED collector is NOT refused here. Whether an overflow argument can safely land in it
    // depends on what the SITE passes, which only codegen knows — `new $c(7)` on `int ...$r` is
    // exactly right and must keep working — so that check lives in
    // `codegen::…::dynamic_new_candidate`, next to the site's argument types.
    // Names the thunk's own parameters. Past the regular ones there is no declared name to reuse,
    // so the overflow slots are numbered; they are the thunk's locals and nothing reads them by
    // name. The TYPE comes from the shared `positional_param_type`, which codegen also uses to
    // materialize the matching arguments — the two must describe the same frame.
    let slot_name = |index: usize| -> String {
        if index < regular {
            constructor.params[index].0.clone()
        } else {
            format!("__variadic_arg{}", index)
        }
    };
    let slot_type = |index: usize| -> Option<PhpType> {
        crate::types::call_args::positional_param_type(constructor, index)
    };
    if (0..provided_args).any(|index| slot_type(index).is_none()) {
        return;
    }
    // What the collector declares one element to be. `None` means it collects anything, and the
    // overflow is passed through untouched.
    let element = crate::types::call_args::variadic_element_type(constructor);
    let cast = match element.as_ref().map(PhpType::codegen_repr) {
        None | Some(PhpType::Mixed) => None,
        Some(PhpType::Int) => Some(CastType::Int),
        Some(PhpType::Float) => Some(CastType::Float),
        Some(PhpType::Str) => Some(CastType::String),
        Some(PhpType::Bool) => Some(CastType::Bool),
        // A collector of objects or arrays has no cast that means what php means, so the class
        // stays out of the ladder and `dynamic_new_mixed_refusals` reports the site instead.
        Some(_) => return,
    };

    let span = Span::dummy();
    let this_type = PhpType::Object(class_name.to_string());
    let mut args = Vec::with_capacity(provided_args.max(constructor.params.len()));
    let mut guards = Vec::new();
    for index in 0..provided_args {
        let variable = Expr::new(ExprKind::Variable(slot_name(index)), span);
        let overflow = is_variadic && index >= regular;
        match cast.clone().filter(|_| overflow) {
            None => args.push(variable),
            Some(target) => {
                // php COERCES at this boundary, so the thunk does too — in PHP, where the rules
                // can be spelled out, rather than in two architectures of hand-written assembly.
                // `new $c("7")` and `new $c(1.5)` construct here exactly as they do in php.
                //
                // The guard is what keeps that from becoming a silent `(int)"x" === 0`: php raises
                // a TypeError for a value it cannot read as a number, and so does this.
                guards.push(coercible_or_throw_stmt(
                    class_name,
                    &variable,
                    index + 1,
                    target.clone(),
                    span,
                ));
                args.push(Expr::new(
                    ExprKind::Cast {
                        target,
                        expr: Box::new(variable),
                    },
                    span,
                ));
            }
        }
    }
    // Pad only up to the last REGULAR parameter. For a collecting signature the entry beyond that
    // is the collector, which takes what is passed rather than a default.
    let pad_upto = if is_variadic {
        regular
    } else {
        constructor.params.len()
    };
    if provided_args < pad_upto {
        for default in &constructor.defaults[provided_args..pad_upto] {
            args.push(default.clone().expect("padding requires a default"));
        }
    }
    let mut body = guards;
    body.push(Stmt::new(
        StmtKind::ExprStmt(Expr::new(
            ExprKind::MethodCall {
                object: Box::new(Expr::new(ExprKind::This, span)),
                method: "__construct".to_string(),
                args,
            },
            span,
        )),
        span,
    ));

    let mut function = Function::new(function_name.clone(), IrType::Void, PhpType::Void);
    function.flags.is_synthetic = true;
    let mut params = vec![("this".to_string(), this_type.clone())];
    function.params.push(FunctionParam {
        name: "this".to_string(),
        ir_type: value_ir_type(&this_type),
        php_type: this_type.clone(),
        by_ref: false,
        variadic: false,
    });
    for index in 0..provided_args {
        let name = slot_name(index);
        let php_type = slot_type(index).expect("slot types were checked above");
        params.push((name.clone(), php_type.clone()));
        function.params.push(FunctionParam {
            name,
            ir_type: value_ir_type(&php_type),
            php_type,
            by_ref: constructor.ref_params.get(index).copied().unwrap_or(false),
            variadic: false,
        });
    }
    let sig = FunctionSig {
        params: params.clone(),
        param_type_exprs: vec![None; params.len()],
        param_attributes: Vec::new(),
        defaults: vec![None; params.len()],
        return_type: PhpType::Void,
        declared_return: false,
        by_ref_return: false,
        ref_params: vec![false; params.len()],
        declared_params: vec![false; params.len()],
        variadic: None,
        deprecation: None,
        is_generator: false,
    };
    function.source_signature = Some(source_signature(&function_name, &sig));
    function.signature = Some(eir_runtime_metadata_signature(&sig));
    let mut env = TypeEnv::default();
    for (name, php_type) in &params {
        env.insert(name.clone(), php_type.clone());
    }
    let web = module.web;
    let closures = lower_body_into_function(
        module.shared_source_catalog(),
        module.runtime_bound_functions.clone(),
        &mut function,
        None,
        &mut module.data,
        &body,
        env,
        web_gated_global_env(&check_result.global_env, web),
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.extern_globals,
        &check_result.callable_param_sigs,
        &check_result.return_alias_summaries,
        fiber_return_sigs,
        &module.class_infos,
        &check_result.enums,
        &check_result.interfaces,
        &module.declared_trait_names,
        &module.declared_trait_methods,
        &module.declared_trait_properties,
        &check_result.packed_classes,
        &check_result.extern_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        &check_result.closure_generator_spans,
        &check_result.array_push_property_plans,
        &check_result.member_receiver_types,
        &check_result.flow_typed_property_accesses,
        &check_result.boxed_reference_promotion_sites,
        &check_result.first_class_builtin_call_types,
        &check_result.generic_call_sites,
        &check_result.loop_storage_types,
        &check_result.string_incdec_locals,
        &check_result.by_ref_local_storage_types,
        &check_result.dynamic_ref_local_types,
        &check_result.local_bind_kill_sites,
        &check_result.local_ref_detach_sites,
        &check_result.local_retype_sites,
        &check_result.mixed_storage_store_sites,
        function_name.clone(),
        constants,
        Some(class_name.to_string()),
        PhpType::Void,
        false,
        &params,
        None,
        false,
        crate::fast_hash::FastSet::default(),
        None,
        module.source_path.clone(),
        None,
        web,
    );
    add_closures(module, closures);
    module.add_function(function);
}

/// Lowers one synthetic function belonging to the PHP 8.5 `clone()` override engine.
///
/// Both shapes go through here: the per-`(runtime class, invocation scope)` applicator, whose
/// parameters are `(this: Object(<clone class>), __elephc_clone_overrides: mixed)`, and the
/// per-`(declaring class, property)` scoped setter helper, whose `this` is typed with the
/// ancestor that owns the private slot the scope selects.
///
/// `scope` becomes the body's lexical class, so property-slot resolution, set-hook routing and
/// `__set()` selection inside the body see the PHP scope the statements were synthesized for.
/// Visibility itself is decided BEFORE lowering, in `crate::ir_lower::clone_overrides::arms`,
/// and is already baked into the statements as explicit assignment or denial arms.
///
/// The statements carry `Span::dummy()`. `CheckResult::throw_access_sites` is keyed by source
/// span, so the checker's statically decided readonly REFUSAL never applies to these
/// engine-owned stores, which is exactly the reinitialization `clone()` is specified to
/// perform, while no process-global or fiber-visible allowance state is introduced anywhere.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_clone_override_function(
    function_name: &str,
    params: &[(String, PhpType)],
    scope: Option<&str>,
    body: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    functions: &crate::fast_hash::FastMap<String, FunctionSig>,
    constants: &crate::fast_hash::FastMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &crate::fast_hash::FastMap<String, FunctionSig>,
) -> FunctionSig {
    let sig = FunctionSig {
        params: params.to_vec(),
        param_type_exprs: vec![None; params.len()],
        param_attributes: Vec::new(),
        defaults: vec![None; params.len()],
        return_type: PhpType::Void,
        declared_return: false,
        by_ref_return: false,
        ref_params: vec![false; params.len()],
        declared_params: vec![false; params.len()],
        variadic: None,
        deprecation: None,
        is_generator: false,
    };
    if module
        .functions
        .iter()
        .any(|function| function.name == function_name)
    {
        return sig;
    }
    let mut function = Function::new(function_name.to_string(), IrType::Void, PhpType::Void);
    function.flags.is_synthetic = true;
    // The body is lowered AS IF it were written inside `scope`, so the EIR function has to carry
    // that scope too. Leaving `lexical_class` at `None` made a nested `clone($o, [...])` inside an
    // applicator or a scoped setter resolve its own invocation scope as global scope, both when
    // `collect_sites` plans the candidate scopes and when the backend picks the applicator arm.
    function.lexical_class = scope.map(str::to_string);
    for (name, php_type) in params {
        function.params.push(FunctionParam {
            name: name.clone(),
            ir_type: value_ir_type(php_type),
            php_type: php_type.clone(),
            by_ref: false,
            variadic: false,
        });
    }
    function.source_signature = Some(source_signature(function_name, &sig));
    function.signature = Some(eir_runtime_metadata_signature(&sig));
    let mut env = TypeEnv::default();
    for (name, php_type) in params {
        env.insert(name.clone(), php_type.clone());
    }
    let web = module.web;
    let closures = lower_body_into_function(
        module.shared_source_catalog(),
        module.runtime_bound_functions.clone(),
        &mut function,
        None,
        &mut module.data,
        body,
        env,
        web_gated_global_env(&check_result.global_env, web),
        functions,
        &check_result.extern_functions,
        &check_result.extern_globals,
        &check_result.callable_param_sigs,
        &check_result.return_alias_summaries,
        fiber_return_sigs,
        &module.class_infos,
        &check_result.enums,
        &check_result.interfaces,
        &module.declared_trait_names,
        &module.declared_trait_methods,
        &module.declared_trait_properties,
        &check_result.packed_classes,
        &check_result.extern_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        &check_result.closure_generator_spans,
        &check_result.array_push_property_plans,
        &check_result.member_receiver_types,
        &check_result.flow_typed_property_accesses,
        &check_result.boxed_reference_promotion_sites,
        &check_result.first_class_builtin_call_types,
        &check_result.generic_call_sites,
        &check_result.loop_storage_types,
        &check_result.string_incdec_locals,
        &check_result.by_ref_local_storage_types,
        &check_result.dynamic_ref_local_types,
        &check_result.local_bind_kill_sites,
        &check_result.local_ref_detach_sites,
        &check_result.local_retype_sites,
        &check_result.mixed_storage_store_sites,
        function_name.to_string(),
        constants,
        scope.map(str::to_string),
        PhpType::Void,
        false,
        params,
        None,
        false,
        crate::fast_hash::FastSet::default(),
        None,
        module.source_path.clone(),
        None,
        web,
    );
    add_closures(module, closures);
    module.add_function(function);
    sig
}

/// The class whose constructor body a padding thunk for `class_name` calls: the class that
/// IMPLEMENTS the constructor, not the instantiated one.
///
/// A subclass that inherits its constructor runs the very same body with the very same
/// arguments, so a thunk per (instantiated class, argc) was the same function emitted once per
/// descendant. Symfony's `new $class(...)` sites reach about 660 classes that inherit one
/// variadic constructor, at eight arities each: 4915 thunks and 2.7 M instructions where one per
/// (implementing class, argc) says the same. The object is still the descendant's; the thunk is
/// typed with the ancestor, whose layout is a prefix of it.
pub(crate) fn dynamic_constructor_thunk_owner<'a>(
    classes: &'a crate::fast_hash::FastMap<String, ClassInfo>,
    class_name: &str,
) -> Option<(&'a str, &'a ClassInfo)> {
    let (owner_name, owner) = crate::types::constructor_owner(classes, class_name)?;
    let key = php_symbol_key("__construct");
    if let Some(implementing) = owner.method_impl_classes.get(&key) {
        if let Some((name, info)) = classes.get_key_value(implementing.as_str()) {
            // A built-in implementation keeps one thunk per descendant: its dynamic-new
            // eligibility is decided per class name, not by the constructor body.
            if info.methods.contains_key(&key) && !info.is_internal {
                return Some((name.as_str(), info));
            }
        }
    }
    Some((owner_name, owner))
}

/// The symbol a dynamic-new candidate calls when it has to pad the constructor with defaults.
pub(crate) fn dynamic_constructor_thunk_name(class_id: u64, provided_args: usize) -> String {
    format!("_class_ctor_{}_{}", class_id, provided_args)
}

/// Returns the internal function name that materializes one native eval parameter default.
pub(crate) fn eval_native_default_helper_name(
    class_id: u64,
    is_static: bool,
    method_name: &str,
    param_index: usize,
) -> String {
    format!(
        "__elephc_eval_default\\{}\\{}\\{}\\{}",
        class_id,
        if is_static { "static" } else { "instance" },
        method_name,
        param_index,
    )
}

/// Lowers native deferred defaults and eval-safe deep literals into zero-argument EIR helpers.
///
/// Compact eval metadata deliberately has a recursion bound. A finite source literal beyond that
/// bound is still safe for the compiler to lower normally, so the binder calls one of these
/// helpers when no compact value was registered. Deferred-error helpers run only on native
/// callable and reflection paths. Eval receives those trees as owned data, never as callbacks
/// that could unwind through interpreter-owned Rust frames.
pub(crate) fn lower_eval_native_default_helpers(
    module: &mut Module,
    check_result: &CheckResult,
    constants: &crate::fast_hash::FastMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &crate::fast_hash::FastMap<String, FunctionSig>,
) {
    let uses_eval = super::program::all_lowered_functions(module).any(|function| {
        function.locals.iter().any(|local| {
            matches!(
                local.kind,
                crate::ir::LocalKind::EvalContext
                    | crate::ir::LocalKind::EvalScope
                    | crate::ir::LocalKind::EvalGlobalScope
            )
        })
    });
    let mut specs = Vec::new();
    let mut classes = module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, info)| info.class_id);
    for (class_name, class_info) in classes {
        for (is_static, methods) in [
            (false, &class_info.methods),
            (true, &class_info.static_methods),
        ] {
            let mut methods = methods.iter().collect::<Vec<_>>();
            methods.sort_by_key(|(name, _)| name.as_str());
            for (method_name, signature) in methods {
                for (param_index, default) in signature.defaults.iter().enumerate() {
                    let Some(default) = default else { continue };
                    if contains_deferred_error(default)
                        && !matches!(default.kind, ExprKind::Throw(_))
                        || (uses_eval && eval_native_default_helper_literal(&default.kind)
                            && crate::types::signatures::literal_default_exceeds_compact_depth(default))
                    {
                        let scope = if is_static {
                            class_info.static_method_impl_classes.get(method_name)
                        } else { class_info.method_impl_classes.get(method_name) }
                            .cloned().unwrap_or_else(|| class_name.clone());
                        specs.push((eval_native_default_helper_name(
                            class_info.class_id, is_static, method_name, param_index,
                        ), Some(scope), default.clone()));
                    }
                }
            }
        }
    }
    let mut functions = check_result.functions.iter().collect::<Vec<_>>();
    functions.sort_by_key(|(name, _)| name.as_str());
    for (name, signature) in functions {
        for (index, default) in signature.defaults.iter().enumerate() {
            let Some(default) = default else { continue; };
            if contains_deferred_error(default) && !matches!(default.kind, ExprKind::Throw(_)) {
                specs.push((format!("__elephc_eval_default\\function\\{name}\\{index}"),
                    None, default.clone()));
            }
        }
    }
    for (function_name, default_scope, default) in specs {
        if module.functions.iter().any(|function| function.name == function_name) {
            continue;
        }
        let mut function = Function::new(
            function_name.clone(),
            value_ir_type(&PhpType::Mixed),
            PhpType::Mixed,
        );
        function.flags.is_synthetic = true;
        function.lexical_class = default_scope.clone();
        function.callable_default = Some(default.clone());
        let signature = FunctionSig {
            params: Vec::new(),
            param_type_exprs: Vec::new(),
            param_attributes: Vec::new(),
            defaults: Vec::new(),
            return_type: PhpType::Mixed,
            declared_return: true,
            by_ref_return: false,
            ref_params: Vec::new(),
            declared_params: Vec::new(),
            variadic: None,
            deprecation: None,
        is_generator: false,
        };
        function.source_signature = Some(source_signature(&function_name, &signature));
        function.signature = Some(eir_runtime_metadata_signature(&signature));
        let body = [Stmt::new(StmtKind::Return(Some(default)), Span::dummy())];
        // The helper inherits class scope through `current_class` below. It must not enter the
        // separate property-initializer path, which skips `body` and expects an object receiver.
        let closures = lower_body_into_function(
            module.shared_source_catalog(),
            module.runtime_bound_functions.clone(),
            &mut function,
            None,
            &mut module.data,
            &body,
            TypeEnv::default(),
            web_gated_global_env(&check_result.global_env, module.web),
            &check_result.functions,
            &check_result.extern_functions,
            &check_result.extern_globals,
            &check_result.callable_param_sigs,
            &check_result.return_alias_summaries,
            fiber_return_sigs,
            &module.class_infos,
            &check_result.enums,
            &check_result.interfaces,
            &module.declared_trait_names,
            &module.declared_trait_methods,
            &module.declared_trait_properties,
            &check_result.packed_classes,
            &check_result.extern_classes,
            &check_result.throw_access_sites,
            &check_result.builtin_call_types,
            &check_result.closure_generator_spans,
            &check_result.array_push_property_plans,
            &check_result.member_receiver_types,
            &check_result.flow_typed_property_accesses,
            &check_result.boxed_reference_promotion_sites,
            &check_result.first_class_builtin_call_types,
            &check_result.generic_call_sites,
            &check_result.loop_storage_types,
            &check_result.string_incdec_locals,
            &check_result.by_ref_local_storage_types,
            &check_result.dynamic_ref_local_types,
            &check_result.local_bind_kill_sites,
            &check_result.local_ref_detach_sites,
            &check_result.local_retype_sites,
            &check_result.mixed_storage_store_sites,
            function_name.clone(),
            constants,
            default_scope,
            PhpType::Mixed,
            true,
            &[],
            None,
            false,
            crate::fast_hash::FastSet::default(),
            None,
            module.source_path.clone(),
            None,
            module.web,
        );
        add_closures(module, closures);
        module.add_function(function);
    }
}

/// Returns whether a default is a finite, side-effect-free literal tree.
fn eval_native_default_helper_literal(expr: &ExprKind) -> bool {
    match expr {
        ExprKind::Null
        | ExprKind::BoolLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_) => true,
        ExprKind::Negate(inner) => matches!(
            &inner.kind,
            ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)
        ),
        ExprKind::ArrayLiteral(items) => items
            .iter()
            .all(|item| eval_native_default_helper_literal(&item.kind)),
        ExprKind::ArrayLiteralAssoc(items) => items.iter().all(|(key, value)| {
            eval_native_default_helper_literal(&key.kind)
                && eval_native_default_helper_literal(&value.kind)
        }),
        _ => false,
    }
}

/// Builds `$this->property = <default>;` statements for property-default initialization.
///
/// A null default whose slot type cannot represent null (a scalar slot rebound by
/// constructor-argument propagation) is skipped: those slots are always overwritten
/// before an observable read, and the store would be unrepresentable.
fn property_init_body(class_info: &ClassInfo, include: impl Fn(usize) -> bool) -> Vec<Stmt> {
    let span = Span::dummy();
    class_info
        .defaults
        .iter()
        .enumerate()
        .filter(|(index, _)| include(*index))
        .filter_map(|(index, default)| {
            let default = default.as_ref()?;
            let (name, php_type) = class_info.properties.get(index)?;
            if matches!(default.kind, ExprKind::Null) && !php_type.null_property_default_required() {
                return None;
            }
            let property = name.clone();
            Some(Stmt::new(
                StmtKind::ExprStmt(Expr::new(
                    ExprKind::Assignment {
                        target: Box::new(Expr::new(
                            ExprKind::PropertyAccess {
                                object: Box::new(Expr::new(ExprKind::This, span)),
                                property,
                            },
                            span,
                        )),
                        value: Box::new(default.clone()),
                        result_target: None,
                        prelude: Vec::new(),
                        conditional_value_temp: None,
                    },
                    span,
                )),
                span,
            ))
        })
        .collect()
}

/// Lowers one closure literal into an EIR function plus any nested closure functions.
pub(crate) fn lower_closure_function(
    parent: &mut LoweringContext<'_, '_>,
    name: &str,
    params: &AstParams,
    variadic: Option<&str>,
    variadic_by_ref: bool,
    return_type: Option<&TypeExpr>,
    body: &[Stmt],
    captures: &[(String, PhpType, bool)],
    signature_captures: &[(String, PhpType, bool)],
    self_ref_callable_capture: Option<&str>,
    by_ref_return: bool,
    is_static: bool,
    loop_storage_scope: String,
    source_is_generator: bool,
) -> FunctionSig {
    // The checker recorded this closure's builtin result types under ITS scope, not the parent's.
    parent.closure_signature_scope = Some(loop_storage_scope.clone());
    let mut signature = closure_signature_from_ast(
        params,
        variadic,
        variadic_by_ref,
        return_type,
        body,
        signature_captures,
        parent,
        source_is_generator,
    );
    parent.closure_signature_scope = None;
    signature.by_ref_return = by_ref_return;
    lower_closure_function_with_signature(
        parent,
        name,
        params,
        signature,
        body,
        captures,
        self_ref_callable_capture,
        is_static,
        loop_storage_scope,
        None,
    )
}

/// Lowers one closure literal using contextual types for unannotated parameters and, when the
/// binding context supplies one, for an undeclared result type.
///
/// The contextual result is applied BEFORE the body is lowered, because the body's `return`
/// statements are lowered against the signature's return type: a by-reference `return
/// $this->prop` decides there which payload representation it publishes to the caller. It is
/// only a default: a closure that declares its own return type keeps it, so an explicit
/// declaration is never silently overridden by the binding.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_closure_function_with_context(
    parent: &mut LoweringContext<'_, '_>,
    name: &str,
    params: &AstParams,
    variadic: Option<&str>,
    variadic_by_ref: bool,
    return_type: Option<&TypeExpr>,
    body: &[Stmt],
    captures: &[(String, PhpType, bool)],
    signature_captures: &[(String, PhpType, bool)],
    contextual_arg_types: &[PhpType],
    contextual_return_type: Option<&PhpType>,
    self_ref_callable_capture: Option<&str>,
    by_ref_return: bool,
    is_static: bool,
    loop_storage_scope: String,
    source_is_generator: bool,
) -> FunctionSig {
    // The checker recorded this closure's builtin result types under ITS scope, not the parent's.
    parent.closure_signature_scope = Some(loop_storage_scope.clone());
    let mut signature = closure_signature_from_ast(
        params,
        variadic,
        variadic_by_ref,
        return_type,
        body,
        signature_captures,
        parent,
        source_is_generator,
    );
    parent.closure_signature_scope = None;
    signature.by_ref_return = by_ref_return;
    if let Some(contextual_return_type) = contextual_return_type {
        if !signature.declared_return {
            signature.return_type = contextual_return_type.clone();
        }
    }
    for (idx, (_, type_ann, _, is_ref)) in params.iter().enumerate() {
        if type_ann.is_none() && !is_ref {
            if let Some(contextual_ty) = contextual_arg_types.get(idx) {
                if let Some((_, param_ty)) = signature.params.get_mut(idx) {
                    *param_ty = contextual_ty.clone();
                }
            }
        }
    }
    lower_closure_function_with_signature(
        parent,
        name,
        params,
        signature,
        body,
        captures,
        self_ref_callable_capture,
        is_static,
        loop_storage_scope,
        None,
    )
}

/// Lowers one closure function from an already-built signature.
fn lower_closure_function_with_signature(
    parent: &mut LoweringContext<'_, '_>,
    name: &str,
    params: &AstParams,
    signature: FunctionSig,
    body: &[Stmt],
    captures: &[(String, PhpType, bool)],
    self_ref_callable_capture: Option<&str>,
    is_static: bool,
    loop_storage_scope: String,
    callable_default: Option<Expr>,
) -> FunctionSig {
    closure_defaults::lower_helpers(parent, name, &signature);
    // Generator closures lower their body as a Mixed-returning coroutine; see
    // `generator_body_return_type`.
    let closure_body_return_type = generator_body_return_type(body, &signature.return_type, signature.is_generator);
    let mut function = Function::new(
        name.to_string(),
        return_ir_type(&closure_body_return_type),
        closure_body_return_type.clone(),
    );
    function.lexical_class = parent.current_class.clone();
    function.callable_default = callable_default;
    function.display_name = parent.take_closure_display_name(name);
    function.flags = FunctionFlags {
        is_closure: true,
        is_synthetic: function.callable_default.is_some(),
        is_static,
        by_ref_return: signature.by_ref_return && !signature.is_generator,
        ..FunctionFlags::default()
    };
    function.params = function_params(&signature);
    function.params.extend(closure_capture_params(captures));
    function.source_signature = Some(source_signature(name, &signature));
    function.signature = Some(eir_runtime_metadata_signature(&signature));
    mark_generator_if_needed(
        &mut function,
        body,
        signature.is_generator,
    );
    let env = env_with_closure_captures(&signature, captures, parent.web);
    let lowered_params = params_with_closure_captures(&signature, captures);
    let recursive_binding = self_ref_callable_capture.map(|local_name| RecursiveClosureBinding {
        local_name: local_name.to_string(),
        closure_name: name.to_string(),
        signature: signature.clone(),
        captures: captures
            .iter()
            .map(|(capture_name, _, by_ref)| (capture_name.clone(), *by_ref))
            .collect(),
        // The DECLARED parameters, with their hints: the alias summary needs to know which
        // are spelled `string`, the one slot shape a `(string)` cast passes through (#700).
        return_alias: crate::types::summarize_callable_return_alias(
            params
                .iter()
                .map(|(name, hint, _, by_ref)| (name.as_str(), hint.as_ref(), *by_ref)),
            None,
            signature.by_ref_return,
            // Closures keep the conservative return-type summary.
            None,
            params.iter().any(|(_, _, _, by_ref)| *by_ref),
            body,
        ),
    });
    let nested_constants = parent.constants.nested();
    let constants = nested_constants.as_ref().unwrap_or(parent.constants.base());
    let closures = lower_body_into_function(
        parent.source_catalog.clone(),
        parent.runtime_bound_functions.clone(),
        &mut function,
        None,
        parent.data,
        body,
        env,
        parent.top_level_env.clone(),
        parent.functions,
        parent.extern_functions,
        parent.extern_globals,
        parent.callable_param_sigs,
        parent.return_alias_summaries,
        parent.fiber_return_sigs,
        parent.classes,
        parent.enums,
        parent.interfaces,
        parent.declared_trait_names,
        parent.declared_trait_methods,
        parent.declared_trait_properties,
        parent.packed_classes,
        parent.extern_classes,
        parent.throw_access_sites,
        parent.builtin_call_types,
        parent.closure_generator_spans,
        parent.array_push_property_plans,
        parent.member_receiver_types,
        parent.flow_typed_property_accesses,
        parent.boxed_reference_promotion_sites,
        parent.first_class_builtin_call_types,
        parent.generic_call_sites,
        parent.loop_storage_types,
        parent.string_incdec_locals,
        parent.by_ref_local_storage_types,
        parent.dynamic_ref_local_types,
        parent.bind_kill_sites,
        parent.ref_detach_sites,
        parent.retype_sites,
        parent.mixed_storage_store_sites,
        loop_storage_scope,
        constants,
        parent.current_class.clone(),
        closure_body_return_type.clone(),
        signature.declared_return,
        &lowered_params,
        recursive_binding,
        false,
        collect_global_var_names(body),
        None,
        parent.source_path().map(str::to_string),
        None,
        parent.web,
    );
    parent.extend_closures(std::iter::once(function).chain(closures));
    signature
}

/// Lowers the supplied statements into `function` and appends a default terminator if needed.
fn lower_body_into_function(
    source_catalog: Option<std::sync::Arc<crate::ir::SourceCatalog>>,
    runtime_bound_functions: std::sync::Arc<crate::fast_hash::FastSet<String>>,
    function: &mut Function,
    property_initializers: Option<&ClassInfo>,
    data: &mut crate::ir::DataPool,
    body: &[Stmt],
    env: TypeEnv,
    top_level_env: TypeEnv,
    functions: &crate::fast_hash::FastMap<String, FunctionSig>,
    extern_functions: &crate::fast_hash::FastMap<String, crate::types::ExternFunctionSig>,
    extern_globals: &crate::fast_hash::FastMap<String, PhpType>,
    callable_param_sigs: &crate::fast_hash::FastMap<(String, String), FunctionSig>,
    return_alias_summaries: &crate::types::ReturnAliasSummaries,
    fiber_return_sigs: &crate::fast_hash::FastMap<String, FunctionSig>,
    classes: &crate::fast_hash::FastMap<String, crate::types::ClassInfo>,
    enums: &crate::fast_hash::FastMap<String, crate::types::EnumInfo>,
    interfaces: &crate::fast_hash::FastMap<String, crate::types::InterfaceInfo>,
    declared_trait_names: &[String],
    declared_trait_methods: &crate::fast_hash::FastMap<
        String,
        crate::fast_hash::FastMap<String, crate::ir::TraitMethodInfo>,
    >,
    declared_trait_properties: &crate::fast_hash::FastMap<String, Vec<crate::parser::ast::ClassProperty>>,
    packed_classes: &crate::fast_hash::FastMap<String, PackedClassInfo>,
    extern_classes: &crate::fast_hash::FastMap<String, crate::types::ExternClassInfo>,
    throw_access_sites: &crate::fast_hash::FastMap<(String, Span), crate::types::ThrowAccessInfo>,
    builtin_call_types: &crate::fast_hash::FastMap<(String, Span), PhpType>,
    closure_generator_spans: &crate::fast_hash::FastSet<Span>,
    array_push_property_plans: &crate::fast_hash::FastMap<(String, Span), crate::types::ArrayPushPropertyPlan>,
    member_receiver_types: &crate::fast_hash::FastMap<(String, Span), Option<PhpType>>,
    flow_typed_property_accesses: &crate::fast_hash::FastMap<(String, Span), PhpType>,
    boxed_reference_promotion_sites: &crate::fast_hash::FastMap<
        (String, Span),
        crate::fast_hash::FastSet<String>,
    >,
    first_class_builtin_call_types: &crate::fast_hash::FastMap<Span, PhpType>,
    generic_call_sites: &crate::fast_hash::FastMap<(String, Span), String>,
    loop_storage_types: &crate::types::LoopStorageTypes,
    string_incdec_locals: &crate::fast_hash::FastSet<(String, String)>,
    by_ref_local_storage_types: &crate::fast_hash::FastMap<(String, String), PhpType>,
    dynamic_ref_local_types: &crate::fast_hash::FastMap<(String, String), PhpType>,
    bind_kill_sites: &crate::fast_hash::FastMap<Span, crate::fast_hash::FastSet<String>>,
    ref_detach_sites: &crate::fast_hash::FastMap<Span, crate::fast_hash::FastSet<String>>,
    retype_sites: &crate::fast_hash::FastMap<Span, crate::fast_hash::FastSet<String>>,
    mixed_storage_store_sites: &crate::fast_hash::FastMap<
        Span,
        crate::fast_hash::FastSet<String>,
    >,
    loop_storage_scope: String,
    constants: &crate::fast_hash::FastMap<String, (ExprKind, PhpType)>,
    current_class: Option<String>,
    return_php_type: PhpType,
    return_type_is_declared: bool,
    params: &[(String, PhpType)],
    recursive_closure_binding: Option<RecursiveClosureBinding>,
    in_main: bool,
    all_global_var_names: crate::fast_hash::FastSet<String>,
    function_global_names: Option<
        crate::fast_hash::FastMap<String, Option<crate::fast_hash::FastSet<String>>>,
    >,
    source_path: Option<String>,
    eval_scope_reads: Option<(
        String,
        crate::fast_hash::FastSet<String>,
        crate::fast_hash::FastSet<String>,
        std::collections::BTreeSet<String>,
    )>,
    web: bool,
) -> Vec<Function> {
    let owner_name = function.name.clone();
    let function_by_ref_return = function.flags.by_ref_return;
    let function_is_generator = function.flags.is_generator;
    let by_ref_params = function
        .params
        .iter()
        .map(|param| param.by_ref)
        .collect::<Vec<_>>();
    let mut builder = Builder::new(function);
    let entry = builder.create_named_block("entry", Vec::new());
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let mut ctx = LoweringContext::new(
        builder,
        data,
        env,
        functions,
        extern_functions,
        extern_globals,
        callable_param_sigs,
        return_alias_summaries,
        fiber_return_sigs,
        classes,
        enums,
        interfaces,
        declared_trait_names,
        declared_trait_methods,
        declared_trait_properties,
        packed_classes,
        extern_classes,
        throw_access_sites,
        builtin_call_types,
        closure_generator_spans,
        array_push_property_plans,
        member_receiver_types,
        flow_typed_property_accesses,
        boxed_reference_promotion_sites,
        first_class_builtin_call_types,
        generic_call_sites,
        loop_storage_types,
        string_incdec_locals,
        by_ref_local_storage_types,
        dynamic_ref_local_types,
        bind_kill_sites,
        ref_detach_sites,
        retype_sites,
        mixed_storage_store_sites,
        loop_storage_scope,
        constants,
        top_level_env,
        current_class,
        owner_name,
        return_php_type,
        in_main,
        all_global_var_names,
        source_path,
        web,
    );
    ctx.source_catalog = source_catalog;
    ctx.runtime_bound_functions = runtime_bound_functions;
    ctx.body_contains_eval = crate::ir_lower::body_contains_eval_call(body);
    if let Some(function_global_names) = function_global_names {
        ctx.set_function_global_names(function_global_names);
    }
    ctx.this_storage_stays_concrete = {
        let flags = &ctx.builder.function().flags;
        flags.is_method && !flags.is_static && !flags.is_closure
            && property_initializers.is_none() && !body.is_empty()
            && !crate::ir_lower::body_contains_eval_call(body)
    };
    ctx.by_ref_return = function_by_ref_return;
    ctx.runtime_global_bindings = crate::global_decls::collect_global_var_names(body);
    ctx.reference_bound_statics =
        crate::ir_lower::static_refs::reference_bound_statics(body, ctx.functions);
    ctx.reassigned_statics = crate::ir_lower::static_refs::reassigned_statics(body);
    ctx.return_type_is_declared = return_type_is_declared;
    if let Some((scope_param, read_names, write_names, flush_names)) = eval_scope_reads {
        ctx.enable_eval_scope_access(scope_param, read_names, write_names, flush_names);
    }
    for (index, (name, php_type)) in params.iter().enumerate() {
        ctx.declare_local(name, php_type.clone());
        ctx.mark_local_initialized(name);
        if by_ref_params.get(index).copied().unwrap_or(false) {
            ctx.mark_ref_bound_local(name);
        }
    }
    // PHP passes arrays BY VALUE. The call site hands the callee a `+0` borrow of the caller's
    // array, so `__rt_array_ensure_unique` (which only splits at refcount >= 2) stayed inert and
    // every write in the callee landed in the CALLER's storage. Re-bind each by-value container
    // parameter to an owning shadow slot, which restores the refcount the copy-on-write split
    // depends on. Exact PHP `array` parameters carry a boxed Mixed cell, so the shadow clones that
    // wrapper as well as retaining its payload. This one site is the single funnel for free
    // functions, methods, static methods and closures, so every call flavour, including
    // `call_user_func`, dynamic `$f(...)` and recursion, is covered without per-flavour code.
    //
    // Mixed parameters also need an owning cell: native return values must outlive an eval
    // caller's temporary arguments, and a boxed array mutation must not replace the caller's
    // cell payload. The resource-aware clone preserves shared resource identity.
    // By-reference parameters are excluded by definition: `array &$a` must alias, not copy.
    // `$this` is excluded because it is an object, never a container.
    //
    // NOT when the body can hand its scope to code the compiler does not see (`eval`, a runtime
    // `include`/`require`). The shadow slot is named `{name}#cow`, and both the eval-scope bridge
    // and the prologue find a parameter's slot BY NAME, so included code read the ORIGINAL slot:
    // `$file .= '.php'; require $dir.$file;` showed the included file the pre-write `$file`, and in
    // Symfony's generated `Container::load()` the stale slot fed `$class::do($this, $lazyLoad)` a
    // dangling reference that crashed `--web` workers depending on memory layout.
    let exposes_scope = crate::ir_lower::body_contains_eval_call(body);
    for (index, (name, php_type)) in params.iter().enumerate() {
        if exposes_scope {
            continue;
        }
        if name == "this" {
            continue;
        }
        if !FunctionSig::parameter_needs_owned_shadow(
            php_type, by_ref_params.get(index).copied().unwrap_or(false),
        ) {
            continue;
        }
        ctx.privatize_container_param(name, php_type, None);
    }
    seed_recursive_closure_binding(&mut ctx, recursive_closure_binding);
    // Before any statement: the `static` declaration's storage decision reads this, and it is
    // the first statement of the bodies that have one.
    ctx.record_non_integer_keyed_locals(body);
    seed_vivified_array_locals(&mut ctx, body, params);
    box_array_reference_sources(&mut ctx, body, params);
    ctx.lower_with_final_string_storage(|ctx| {
        if let Some(class) = property_initializers {
            super::property_initializers::lower(ctx, class);
        } else if !super::throwable_constructors::lower(ctx) && !super::core_methods::lower(ctx, body) {
            crate::ir_lower::numeric_storage::lower_body(ctx, body);
        }
    });
    terminate_open_block(&mut ctx, function_is_generator);
    // Cleanup-only loads must follow the final frame representation after every
    // source-order store has had a chance to widen its local slot.
    ctx.builder.repair_owned_local_cleanup_load_types();
    // Final storage types are now known: erase deferred loop-store releases that
    // guard slots which never widened to lifetime-tracked storage (issue #534).
    ctx.builder.prune_untracked_release_local_slot_ops();
    // Pointer by-reference bindings need a pointer-word slot, which only the final storage shows.
    ctx.refuse_pointer_reference_bindings();
    // Likewise, erase provisional releases for concrete local loads unless a
    // later store widened their final frame slot to Mixed (issue #538).
    ctx.builder.prune_borrowed_local_load_release_ops();
    // And retype the ones that SURVIVE but only ever named a slot's occupant: a
    // later store can have widened the slot to Mixed after the release was
    // lowered, and codegen would otherwise unbox+retain and hand the retain
    // straight back, leaking the box that owns the value being overwritten.
    ctx.builder.retype_stale_local_load_release_ops();
    // Provisional exception guards obey the same final-storage boundary. A
    // concrete slot remains the owner and must survive a throwing call.
    ctx.builder.prune_borrowed_local_load_guard_ops();
    // Publish the lowering-time ownership proof after provisional local-load
    // releases have been pruned, so codegen can consume EIR metadata instead of
    // maintaining a second producer allow-list (issue #595).
    ctx.finalize_value_ownership_metadata();
    ctx.into_closures()
}

/// Stores an empty array into every local this scope CREATES with an array element write.
///
/// `$keys[] = $k` against a name nothing has assigned yet is how PHP makes the array, so the
/// storage has to exist before the write runs. The store is emitted here, in the still-open entry
/// block, rather than at the write: the write is usually inside a loop, and initializing it there
/// would discard the previous iterations' elements.
///
/// A name backed by global storage is skipped — `global $rows;` and the `$GLOBALS` aliases already
/// have a home, and re-seeding it would clear what another scope put there.
fn seed_vivified_array_locals(
    ctx: &mut LoweringContext<'_, '_>,
    body: &[Stmt],
    params: &[(String, PhpType)],
) {
    let names = crate::append_vivify::vivified_array_locals(
        body,
        params.iter().map(|(name, _)| name.as_str()),
    );
    if names.is_empty() {
        return;
    }
    let span = body.first().map_or_else(|| Span::new(0, 0), |stmt| stmt.span);
    for name in names {
        if ctx.uses_global_storage_for_seeding(&name) {
            continue;
        }
        let seed = Stmt::new(
            StmtKind::Assign {
                name,
                value: Expr::new(ExprKind::ArrayLiteral(Vec::new()), span),
            },
            span,
        );
        crate::ir_lower::stmt::lower_stmt(ctx, &seed);
    }
}

/// Gives every local the body stores into an array as a reference (`$a[$k] = &$v`, `[&$v]`)
/// boxed storage from the entry on.
///
/// Such a local ends up as a Mixed reference cell, and when the promotion sits in a loop it is
/// lowered once but runs every iteration: re-boxing a slot that is already a cell leaked one box
/// per extra iteration. With the slot boxed from the start, the promotion is only the idempotent
/// `PromoteLocalRefCell`. Parameters keep their calling convention, and names backed by global
/// storage have their own home.
fn box_array_reference_sources(
    ctx: &mut LoweringContext<'_, '_>,
    body: &[Stmt],
    params: &[(String, PhpType)],
) {
    let mut names: Vec<String> = crate::ast_usage::array_reference_sources(body)
        .into_iter()
        .filter(|name| !params.iter().any(|(param, _)| param == name))
        .filter(|name| !crate::superglobals::is_superglobal(name))
        .collect();
    names.sort();
    for name in names {
        if ctx.uses_global_storage_for_seeding(&name) || ctx.local_slots.contains_key(&name) {
            continue;
        }
        ctx.declare_local(&name, PhpType::Mixed);
    }
}

/// Seeds a self-recursive closure capture as a static callable local inside its body.
fn seed_recursive_closure_binding(
    ctx: &mut LoweringContext<'_, '_>,
    binding: Option<RecursiveClosureBinding>,
) {
    let Some(binding) = binding else {
        return;
    };
    let captures = binding
        .captures
        .iter()
        .map(|(capture_name, by_ref)| ClosureCapture {
            value: ctx.load_local(capture_name, None).value,
            by_ref_local: by_ref.then(|| capture_name.clone()),
        })
        .collect();
    ctx.bind_static_callable_local(
        &binding.local_name,
        StaticCallableBinding::Closure {
            name: binding.closure_name,
            signature: binding.signature,
            captures,
            return_alias: binding.return_alias,
            loop_guarded: false,
            numeric_loop_guarded: false,
        },
    );
}

/// Appends lowered closure functions to the module with stable closure-table ids.
fn add_closures(module: &mut Module, closures: Vec<Function>) {
    for closure in closures {
        module.add_closure(closure);
    }
}

/// Marks a function as a generator using its signature classification and remaining yield tokens.
fn mark_generator_if_needed(
    function: &mut Function,
    body: &[Stmt],
    is_generator: bool,
) {
    // Named functions and methods pass the checker's source-body bit as `is_generator`.
    // Closures preserve the checker's source-body classification by literal span.
    // Retaining a true classification matters for two separate reasons:
    //
    // - Scanning `body` alone answers `false` once a pass has pruned the last `yield`, which is
    //   what made a folded generator compile to a plain function returning null and hang its
    //   `foreach` (issue #673).
    // - Falling back to `return_type == Generator`, as this did, conflates a generator with a
    //   FACTORY: `function f(): Generator { return inner(); }` declares and returns a Generator
    //   without being one. That made its `return` lower to `Generator::getReturn()` and its
    //   `foreach` yield nothing (issue #1086).
    //
    // The body scan stays as a second chance for callables whose signature this path cannot
    // reach, but it can only ever ADD generator-ness, never withhold it.
    if !is_generator && !crate::types::checker::yield_validation::body_contains_yield(body) {
        return;
    }
    function.flags.is_generator = true;
}

/// Returns the EIR return type to lower a function body with.
///
/// For a generator the compiled body is a coroutine whose `return` produces the value later
/// read by `Generator::getReturn()`, so the body return type is `Mixed`. For every other
/// function it is the declared signature return type.
///
/// `is_generator` comes from source-body checker metadata for named callables and closures.
/// The additional body scan can only add
/// generator-ness, never withhold it. A declared `: Generator` return is
/// deliberately NOT consulted, because a factory declares one without being one.
fn generator_body_return_type(
    body: &[Stmt],
    signature_return: &PhpType,
    is_generator: bool,
) -> PhpType {
    if is_generator || crate::types::checker::yield_validation::body_contains_yield(body) {
        PhpType::Mixed
    } else {
        signature_return.clone()
    }
}

/// Throws php's `TypeError` for a declared return type the body fell off the end of.
fn throw_none_returned(ctx: &mut LoweringContext<'_, '_>) {
    let owner = ctx.owner_name();
    let callee = if owner.starts_with("__eir_closure") || owner.is_empty() {
        "{closure}".to_string()
    } else {
        owner.to_string()
    };
    let declared = crate::types::checker::php_declared_type_spelling(
        &ctx.return_php_type,
    );
    let span = Span::dummy();
    let error_expr = Expr::new(
        ExprKind::NewObject {
            class_name: crate::names::Name::unqualified("TypeError"),
            args: vec![Expr::new(
                ExprKind::StringLiteral(format!(
                    "{}(): Return value must be of type {}, none returned",
                    callee, declared
                )),
                span,
            )],
        },
        span,
    );
    let error = crate::ir_lower::expr::lower_expr(ctx, &error_expr);
    ctx.builder
        .terminate(Terminator::Throw { value: error.value });
}

/// Adds a default function terminator when the current block can still fall through.
fn terminate_open_block(ctx: &mut LoweringContext<'_, '_>, is_generator: bool) {
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    if matches!(ctx.return_php_type, PhpType::Never) {
        let message = ctx
            .intern_string("Fatal error: A never-returning function must not implicitly return\n");
        ctx.builder.terminate(Terminator::Fatal { message });
        return;
    }
    if matches!(ctx.return_php_type, PhpType::Mixed) {
        ctx.emit_eval_scope_finalizer(None);
        // Only a DECLARED `: mixed` refuses to fall off the end. A function with no return type
        // whose paths the checker merged into `mixed` returns null there, as php does: Laravel's
        // `Container::getContextualConcrete()` ends in a `foreach` and threw this TypeError out
        // of every `make()`.
        if is_generator || !ctx.return_type_is_declared {
            let value = emit_default_return_value(ctx);
            ctx.builder
                .terminate(Terminator::Return { value: Some(value) });
            return;
        }
        throw_none_returned(ctx);
        return;
    }
    // Every other DECLARED value type refuses to fall off the end the same way. The checker used
    // to refuse the whole program instead, which php never does: `TagAwareAdapter::getItem():
    // CacheItem` returns from inside a `foreach` and has nothing after it.
    if ctx.return_type_is_declared
        && !is_generator
        && !ctx.by_ref_return
        && ctx.return_type != IrType::Void
    {
        ctx.emit_eval_scope_finalizer(None);
        throw_none_returned(ctx);
        return;
    }
    if ctx.return_type == IrType::Void {
        ctx.emit_eval_scope_finalizer(None);
        ctx.builder.terminate(Terminator::Return { value: None });
        return;
    }
    if ctx.by_ref_return {
        // A by-reference result is a raw CELL ADDRESS the caller dereferences and may alias.
        // A fallthrough has no accepted reference to transport, and the default placeholder
        // would be a null or zero word the caller would read as a cell, so this path fails
        // closed with the same catchable `Error` the run-time provenance guard raises.
        crate::ir_lower::stmt::lower_throw_access_error(
            ctx,
            "Cannot return a reference from a path that has no by-reference return",
            crate::span::Span::dummy(),
        );
        return;
    }
    ctx.emit_eval_scope_finalizer(None);
    let value = emit_default_return_value(ctx);
    ctx.builder
        .terminate(Terminator::Return { value: Some(value) });
}

/// Emits a placeholder value compatible with the function return storage type.
fn emit_default_return_value(ctx: &mut LoweringContext<'_, '_>) -> crate::ir::ValueId {
    match ctx.return_type {
        IrType::I64 => ctx
            .builder
            .emit_with_effects(
                Op::ConstNull,
                Vec::new(),
                None,
                IrType::I64,
                PhpType::Void,
                Ownership::NonHeap,
                Op::ConstNull.default_effects(),
                None,
            )
            .expect("const_null produces a value"),
        IrType::F64 => ctx
            .builder
            .emit_with_effects(
                Op::ConstF64,
                Vec::new(),
                Some(Immediate::F64(0.0)),
                IrType::F64,
                PhpType::Float,
                Ownership::NonHeap,
                Op::ConstF64.default_effects(),
                None,
            )
            .expect("const_f64 produces a value"),
        IrType::Str => {
            let data = ctx.intern_string("");
            ctx.builder
                .emit_with_effects(
                    Op::ConstStr,
                    Vec::new(),
                    Some(Immediate::Data(data)),
                    IrType::Str,
                    PhpType::Str,
                    Ownership::Persistent,
                    Op::ConstStr.default_effects(),
                    None,
                )
                .expect("const_str produces a value")
        }
        IrType::TaggedScalar => ctx
            .builder
            .emit_with_effects(
                Op::ConstNull,
                Vec::new(),
                None,
                IrType::TaggedScalar,
                PhpType::TaggedScalar,
                Ownership::NonHeap,
                Op::ConstNull.default_effects(),
                None,
            )
            .expect("const_null produces a tagged scalar value"),
        IrType::Heap(_) if ctx.return_php_type.codegen_repr() == PhpType::Mixed => {
            // A Mixed-returning body that falls through yields PHP null. This is
            // how a generator with no explicit `return` produces the value later
            // read by `Generator::getReturn()`. Mirror the `return null;` path
            // (`coerce_to_return_type`): a null scalar boxed into a Mixed cell.
            let null_value = ctx
                .builder
                .emit_with_effects(
                    Op::ConstNull,
                    Vec::new(),
                    None,
                    IrType::I64,
                    PhpType::Void,
                    Ownership::NonHeap,
                    Op::ConstNull.default_effects(),
                    None,
                )
                .expect("const_null produces a value");
            // A fresh null is non-refcounted: there is no producer reference to
            // release, so this boxes directly rather than via box_value_as_mixed
            // (issue #484).
            ctx.emit_value(
                Op::MixedBox,
                vec![null_value],
                None,
                ctx.return_php_type.clone(),
                Op::MixedBox.default_effects(),
                None,
            )
            .value
        }
        IrType::Heap(_) => {
            let lowered = ctx.emit_value(
                Op::RuntimeCall,
                Vec::new(),
                None,
                ctx.return_php_type.clone(),
                effects_lookup::runtime_effects(),
                None,
            );
            lowered.value
        }
        IrType::Void => unreachable!("void returns do not materialize values"),
    }
}

/// Converts a checker signature into EIR parameter metadata.
fn function_params(signature: &FunctionSig) -> Vec<FunctionParam> {
    signature
        .params
        .iter()
        .enumerate()
        .map(|(index, (name, php_type))| FunctionParam {
            name: name.clone(),
            ir_type: value_ir_type(php_type),
            php_type: php_type.clone(),
            by_ref: signature.ref_params.get(index).copied().unwrap_or(false),
            variadic: signature.variadic.as_deref() == Some(name.as_str()),
        })
        .collect()
}

/// Returns an EIR ABI signature with boxed dynamic and object-reference parameters.
pub(crate) fn eir_signature_with_php_param_contracts(
    owner_name: &str,
    signature: &FunctionSig,
    callable_param_sigs: &crate::fast_hash::FastMap<(String, String), FunctionSig>,
) -> FunctionSig {
    let mut eir_signature = signature.clone();
    let mut has_dynamic_untyped_param = false;
    for (index, (name, php_type)) in eir_signature.params.iter_mut().enumerate() {
        let declared = signature
            .declared_params
            .get(index)
            .copied()
            .unwrap_or(false);
        let by_ref = signature.ref_params.get(index).copied().unwrap_or(false);
        let variadic = signature.variadic.as_deref() == Some(name.as_str());
        if by_ref && matches!(php_type, PhpType::Object(_)) {
            *php_type = PhpType::Mixed;
            continue;
        }
        if magic_call_args_param(owner_name, index) {
            // Every route into `__call()` passes the array it holds with ITS element layout: a
            // typed site specialized the checker's element type, while a call through a `mixed`
            // receiver (and `__elephc_magic_call()`) passes whatever array it has, unconverted.
            // A body specialized to one layout read the other's words as values (`$args` printed
            // raw pointers; Carbon's `day(1)->month(6)` chain answered the current date). The
            // declared `array` contract is the one every caller can meet.
            *php_type = PhpType::Array(Box::new(PhpType::Mixed));
            continue;
        }
        if !declared && !by_ref && !variadic {
            if preserve_untyped_eir_param_contract(
                owner_name,
                index,
                name,
                php_type,
                callable_param_sigs,
            ) {
                continue;
            }
            *php_type = PhpType::Mixed;
            has_dynamic_untyped_param = true;
        }
    }
    if has_dynamic_untyped_param && !signature.declared_return {
        eir_signature.return_type = crate::types::dynamic_params::dynamic_param_container_return_type(
            &eir_signature.return_type,
        );
    }
    eir_signature
}

/// Whether `index` is the `$args` array of an `__call()` or `__callStatic()` method.
fn magic_call_args_param(owner_name: &str, index: usize) -> bool {
    index == 1
        && owner_name.rsplit_once("::").is_some_and(|(_, method)| {
            matches!(php_symbol_key(method).as_str(), "__call" | "__callstatic")
        })
}

/// Marks boxed ABI parameters as materialization targets for reused runtime invokers.
fn eir_runtime_metadata_signature(signature: &FunctionSig) -> FunctionSig {
    let mut signature = signature.clone();
    for (index, (_, php_type)) in signature.params.iter().enumerate() {
        if matches!(php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            if let Some(declared) = signature.declared_params.get_mut(index) {
                *declared = true;
            }
        }
    }
    signature
}

/// Returns true when an inferred untyped parameter has an EIR-safe concrete ABI contract.
fn preserve_untyped_eir_param_contract(
    owner_name: &str,
    param_index: usize,
    param_name: &str,
    php_type: &PhpType,
    callable_param_sigs: &crate::fast_hash::FastMap<(String, String), FunctionSig>,
) -> bool {
    magic_method_param_keeps_eir_contract(owner_name, param_index, php_type)
        || matches!(php_type.codegen_repr(), PhpType::Callable)
        || callable_param_sigs.contains_key(&(owner_name.to_string(), param_name.to_string()))
}

/// Returns whether a checker-patched magic-method parameter must keep its real ABI type.
fn magic_method_param_keeps_eir_contract(
    owner_name: &str,
    param_index: usize,
    php_type: &PhpType,
) -> bool {
    let Some((_, method_name)) = owner_name.rsplit_once("::") else {
        return false;
    };
    let method_key = php_symbol_key(method_name);
    match method_key.as_str() {
        "__get" | "__isset" | "__unset" => {
            param_index == 0 && matches!(php_type.codegen_repr(), PhpType::Str)
        }
        "__set" => {
            param_index == 0 && matches!(php_type.codegen_repr(), PhpType::Str)
        }
        // The unserialize runtime calls a user `__unserialize($data)` with the bare hash it
        // parsed, whatever the declaration says: the checker types the first parameter as that
        // hash (`build_method_sig`), and boxing an UNDECLARED one as `mixed` here made the body
        // read the hash header as a Mixed cell -- `gettype($data)` answered "string" and every
        // `$data['key']` was null. A declared `array $data` already kept its contract.
        "__unserialize" => {
            param_index == 0 && matches!(php_type.codegen_repr(), PhpType::AssocArray { .. })
        }
        "__call" | "__callstatic" => {
            // The $args array keeps its contract only once call sites have
            // specialized the element type. The checker seeds it as
            // Array<Never>; eval-only magic calls never specialize it, and a
            // Never element would lower every $args[N] read to an empty
            // constant, so those fall back to the boxed Mixed widening.
            (param_index == 0 && matches!(php_type.codegen_repr(), PhpType::Str))
                || (param_index == 1
                    && matches!(php_type.codegen_repr(), PhpType::Array(_))
                    // Check the raw element type: codegen_repr normalizes the
                    // Never seed to Void and would hide it.
                    && !matches!(
                        php_type,
                        PhpType::Array(elem) if matches!(elem.as_ref(), PhpType::Never)
                    ))
        }
        _ => false,
    }
}

/// Converts closure captures into hidden EIR ABI parameters.
fn closure_capture_params(captures: &[(String, PhpType, bool)]) -> Vec<FunctionParam> {
    captures
        .iter()
        .map(|(name, php_type, by_ref)| FunctionParam {
            name: name.clone(),
            ir_type: value_ir_type(php_type),
            php_type: php_type.clone(),
            by_ref: *by_ref,
            variadic: false,
        })
        .collect()
}

/// Creates an initial local type environment from a function signature.
///
/// Under `--web`, request superglobals (`$_SERVER`/`$_GET`/`$_POST`/`$_SESSION`/…)
/// are seeded here so `local_type` returns their fixed `AssocArray{Str, Mixed}`
/// type inside function bodies. Without this, `$_SESSION = []` in a function
/// contextualizes as a scalar `Array(Never)` instead of a hash and crashes the
/// runtime when the store targets the shared `_eir_global__u_SESSION` slot.
/// `or_insert` never clobbers a parameter that happens to share a superglobal
/// name.
///
/// Outside `--web` nothing pre-initializes that shared global storage, so the
/// seeding is skipped: `local_type` falls back to `Mixed` for these names,
/// matching pre-superglobal-support behavior and avoiding a read/index-write
/// that dereferences a never-initialized (zeroed) global as a live Hash
/// pointer. See `crate::ir_lower::context::LoweringContext::global_alias_type`
/// for the matching gate on the fallback lookup path.
fn env_from_signature(signature: &FunctionSig, web: bool) -> TypeEnv {
    let mut env: TypeEnv = signature
        .params
        .iter()
        .map(|(name, php_type)| (name.clone(), php_type.clone()))
        .collect();
    if web {
        for name in crate::superglobals::SUPERGLOBALS {
            env.entry((*name).to_string())
                .or_insert_with(crate::superglobals::superglobal_type);
        }
    }
    env
}

/// Creates a closure environment that includes hidden captured locals.
fn env_with_closure_captures(
    signature: &FunctionSig,
    captures: &[(String, PhpType, bool)],
    web: bool,
) -> TypeEnv {
    let mut env = env_from_signature(signature, web);
    for (name, php_type, _) in captures {
        env.insert(name.clone(), php_type.clone());
    }
    env
}

/// Returns visible params followed by hidden closure capture params for slot setup.
fn params_with_closure_captures(
    signature: &FunctionSig,
    captures: &[(String, PhpType, bool)],
) -> Vec<(String, PhpType)> {
    let mut params = signature.params.clone();
    params.extend(
        captures
            .iter()
            .map(|(name, php_type, _)| (name.clone(), php_type.clone())),
    );
    params
}

/// Builds a fallback function signature from AST syntax when checker metadata is unavailable.
fn signature_from_ast(params: &AstParams, return_type: Option<&TypeExpr>) -> FunctionSig {
    signature_from_ast_with_variadic(params, return_type, None, false)
}

/// Builds an EIR closure signature and infers fallthrough-only closures as `void`.
fn closure_signature_from_ast(
    params: &AstParams,
    variadic: Option<&str>,
    variadic_by_ref: bool,
    return_type: Option<&TypeExpr>,
    body: &[Stmt],
    captures: &[(String, PhpType, bool)],
    ctx: &LoweringContext<'_, '_>,
    source_is_generator: bool,
) -> FunctionSig {
    let current_class = ctx.current_class.as_deref();
    let classes = ctx.classes;
    let parent_class = current_class.and_then(|class_name| {
        classes
            .get(class_name)
            .and_then(|class_info| class_info.parent.as_deref())
    });
    let resolved_params = current_class.map(|class_name| {
        params
            .iter()
            .map(|(name, type_ann, default, by_ref)| {
                (
                    name.clone(),
                    type_ann.as_ref().map(|type_ann| {
                        type_ann.substitute_relative_class_types(class_name, parent_class)
                    }),
                    default.clone(),
                    *by_ref,
                )
            })
            .collect::<Vec<_>>()
    });
    let resolved_return_type = current_class.and_then(|class_name| {
        return_type.map(|return_type| {
            return_type.substitute_relative_class_types(class_name, parent_class)
        })
    });
    let signature_params = resolved_params.as_deref().unwrap_or(params);
    let signature_return_type = resolved_return_type.as_ref().or(return_type);
    let mut signature = signature_from_ast_with_variadic(
        signature_params,
        signature_return_type,
        variadic,
        variadic_by_ref,
    );
    for default in &mut signature.defaults {
        *default = crate::types::checker::normalize_lexical_callable_default(default, current_class, parent_class)
            .expect("closure defaults passed lexical validation before EIR lowering");
    }
    signature.is_generator = source_is_generator
        || crate::types::checker::yield_validation::body_contains_yield(body);
    if signature.is_generator {
        signature.return_type = PhpType::Object("Generator".to_string());
        return signature;
    }
    if return_type.is_none() {
        if let Some(return_ty) =
            direct_closure_return_type(
                body,
                captures,
                &signature.params,
                ctx,
            )
        {
            signature.return_type = return_ty;
        } else if !body_contains_value_return(body) {
            signature.return_type = PhpType::Void;
        }
    }
    signature
}

/// Infers a closure return type for the no-fallthrough `return <expr>;` shape.
fn direct_closure_return_type(
    body: &[Stmt],
    captures: &[(String, PhpType, bool)],
    params: &[(String, PhpType)],
    ctx: &LoweringContext<'_, '_>,
) -> Option<PhpType> {
    let [stmt] = body else {
        return None;
    };
    let StmtKind::Return(Some(expr)) = &stmt.kind else {
        return None;
    };
    Some(direct_closure_return_expr_type(
        expr,
        captures,
        params,
        ctx,
    ))
}

/// Returns a direct closure return expression type, consulting capture and parameter
/// metadata first. A bare `return $x` where `$x` is a parameter must adopt the parameter's
/// declared type (e.g. `mixed`) rather than falling back to the syntactic integer default,
/// which would otherwise coerce a boxed Mixed argument to an integer on return. A
/// `return $obj->prop` where `$obj` is a captured/parameter object of a known class adopts
/// the property's declared type, so a `fn &() => $o->items` closure returns the array type
/// rather than the syntactic integer default. An array literal built out of those same
/// variables and scoped constants resolves its element/value slots the same way (see
/// `direct_closure_return_array_type`). A call resolves through the same metadata ordinary
/// call lowering uses, and a call whose callee is only known at runtime is boxed `Mixed`.
fn direct_closure_return_expr_type(
    expr: &crate::parser::ast::Expr,
    captures: &[(String, PhpType, bool)],
    params: &[(String, PhpType)],
    ctx: &LoweringContext<'_, '_>,
) -> PhpType {
    if matches!(expr.kind, ExprKind::Null) {
        return PhpType::Mixed;
    }
    // A postfix chain holding `?->` is lowered as one lazy unit whose short-circuit and success
    // paths share one boxed result, so that box is what the body returns.
    if let Some(storage) = crate::ir_lower::expr::nullsafe_chain_result_storage_type(expr) {
        return storage;
    }
    if let ExprKind::FunctionCall { name, .. } = &expr.kind {
        if let Some(ty) = ctx.builtin_call_type_at(expr.span) {
            return ty.clone();
        }
        // A USER function is not in `builtin_call_types` -- that map only records builtin call
        // results -- so without this the call fell all the way to the syntactic fallback at the
        // end of this function, whose `FunctionCall` arm answers `Int` for anything it does not
        // recognize. `$f = function ($v) { return tag($v); }` therefore returned a declared
        // `: string` through an int slot and printed 0, with no diagnostic (issue #1028). The
        // checker gets this right; only this re-derivation did not.
        // Through the caller rule, NOT the raw signature type, because that is what ordinary
        // call lowering hands back and the two have to agree. A callee with an untyped by-value
        // parameter receives it as a boxed Mixed, so a container it builds out of that parameter
        // has Mixed elements whatever the signature's inferred element type says. Copying the
        // raw type here stamped `array<string>` on a call that really produces `array<mixed>`,
        // and the caller then read the boxed element with the wrong layout: the element came
        // back as its own pointer printed as an integer. Raised in review.
        // The rule is read before the boxed storage of a callee returning only `null`s
        // (`caller_visible_checker_return_type`), so such a closure keeps `array<void>`, as on
        // main: its `StaticCallableBinding` signature is all a `?:` or `??` merge of `$f()` can
        // read, and beside a pointer array that arm must join into `?ptr<T>` (task 1539dae5
        // tracks closures and methods).
        if let Some(sig) = ctx.functions.get(name.as_str()) {
            return crate::types::dynamic_params::caller_visible_checker_return_type(sig);
        }
    }
    // The other call shapes fell to the same syntactic fallback, whose answer for a call it does
    // not recognize is `Int`: `function () use ($o) { return $o->name(); }` returned a string
    // through an int slot and printed 0 (issue #1269), and a captured closure, an expression
    // call or a pipe did the same (issue #1272). Methods resolve through the metadata their call
    // lowering reads; a callee chosen at runtime hands back a boxed result.
    match &expr.kind {
        ExprKind::MethodCall { object, method, .. } => {
            let receiver_ty = match &object.kind {
                ExprKind::This => captures
                    .iter()
                    .find(|(capture_name, _, _)| capture_name == "this")
                    .map(|(_, ty, _)| ty.clone())
                    .unwrap_or(PhpType::Mixed),
                _ => direct_closure_return_expr_type(object, captures, params, ctx),
            };
            return crate::ir_lower::expr::closure_return_method_call_type(
                ctx,
                &receiver_ty,
                method,
                expr,
            );
        }
        ExprKind::StaticMethodCall { receiver, method, .. } => {
            return crate::ir_lower::expr::static_method_call_expr_type_for_ir(
                ctx, receiver, method,
            )
            .unwrap_or(PhpType::Mixed);
        }
        ExprKind::ClosureCall { .. }
        | ExprKind::ExprCall { .. }
        | ExprKind::Pipe { .. }
        | ExprKind::NullsafeDynamicMethodCall { .. } => return PhpType::Mixed,
        _ => {}
    }
    // An array literal returned directly is stamped with this inferred type and its elements
    // are coerced into it by `lower_return_expr`, so its slots must be resolved against the
    // closure signature instead of the syntactic integer default.
    if let ExprKind::ArrayLiteral(items) = &expr.kind {
        if !items.is_empty() {
            return direct_closure_return_array_type(
                items,
                captures,
                params,
                ctx,
            );
        }
    }
    if let ExprKind::ArrayLiteralAssoc(pairs) = &expr.kind {
        if !pairs.is_empty() {
            return direct_closure_return_assoc_literal_type(
                pairs,
                captures,
                params,
                ctx,
            );
        }
    }
    if let ExprKind::ArrayLiteralMixed(entries) = &expr.kind {
        return direct_closure_return_mixed_literal_type(
            entries,
            captures,
            params,
            ctx,
        );
    }
    if let ExprKind::ScopedConstantAccess { receiver, name } = &expr.kind {
        // A descendant can override static::CONST with a different storage type.
        if matches!(receiver, crate::parser::ast::StaticReceiver::Static) {
            return PhpType::Mixed;
        }
        if let Some(class_name) =
            crate::ir_lower::expr::static_receiver_class_name(ctx, receiver)
        {
            if ctx.enums.get(&class_name).is_some_and(|enum_info| {
                enum_info.cases.iter().any(|case| case.name.as_str() == name.as_str())
            }) {
                return PhpType::Mixed;
            }
            if let Some(value) = ctx.scoped_constant_value(&class_name, name) {
                return direct_closure_return_expr_type(&value, captures, params, ctx);
            }
        }
    }
    if let ExprKind::Variable(name) = &expr.kind {
        if let Some((_, php_type, _)) = captures
            .iter()
            .find(|(capture_name, _, _)| capture_name == name)
        {
            return php_type.clone();
        }
        if let Some((_, php_type)) = params.iter().find(|(param_name, _)| param_name == name) {
            return php_type.clone();
        }
        // Neither a capture nor a parameter: a local the return expression assigns itself
        // (`is_dir($d = path()) ? $d : ...`), which this signature cannot type. The syntactic
        // default answers `int`, and the body then converted the string it returned to `0` --
        // Laravel's bootstrap path became "0".
        return PhpType::Mixed;
    }
    // A conditional's arms are typed the way a whole return is, so a local assigned in the
    // condition reaches the arm as the `Mixed` above rather than as the syntactic `int`.
    let arms = match &expr.kind {
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => Some((then_expr, else_expr)),
        ExprKind::ShortTernary { value, default } => Some((value, default)),
        _ => None,
    };
    if let Some((first, second)) = arms {
        let first = direct_closure_return_expr_type(
            first,
            captures,
            params,
            ctx,
        );
        let second = direct_closure_return_expr_type(
            second,
            captures,
            params,
            ctx,
        );
        if first == second {
            return first;
        }
        if first == PhpType::Mixed || second == PhpType::Mixed {
            return PhpType::Mixed;
        }
    }
    // A ternary whose arm is read at run time (an untyped parameter, an element, a property) can
    // yield that arm's own type, so the syntactic join -- where a string arm absorbs the other --
    // would convert it: `fn ($v) => $v instanceof Closure ? 'closure' : $v` answered `string`,
    // and the boolean `true` came back as the byte `"` (`array_map` over an options array, in
    // Symfony's OptionsResolver). Arms that agree keep their type, an unknown arm makes the whole
    // expression `Mixed`, and two known, different arms keep the syntactic coercion rule.
    let ternary_arms = match &expr.kind {
        ExprKind::Ternary { then_expr, else_expr, .. } => Some((then_expr, else_expr)),
        ExprKind::ShortTernary { value, default } => Some((value, default)),
        _ => None,
    };
    if let Some((then_expr, else_expr)) = ternary_arms {
        let then_ty = direct_closure_return_expr_type(
            then_expr, captures, params, ctx,
        );
        let else_ty = direct_closure_return_expr_type(
            else_expr, captures, params, ctx,
        );
        if then_ty == else_ty {
            return then_ty;
        }
        let unknown = |ty: &PhpType| matches!(ty, PhpType::Mixed | PhpType::Union(_));
        if unknown(&then_ty) || unknown(&else_ty) {
            return PhpType::Mixed;
        }
    }
    // `$a ?? null` answers whatever `$a` holds, or null: with an arm read at run time (an untyped
    // parameter, an element, a property) or a null default, the result is `Mixed`. The syntactic
    // join typed `fn ($a) => $a['k'] ?? null` as `null`, and every call returned `int(0)`.
    // An assignment's value is the value assigned. The syntactic default answered `int`, so
    // `fn () => $this->value = $r` returned `int(1)` with an "Object of class Ref could not be
    // converted to int" warning. A form whose result is read back from its target (a compound
    // assignment) can widen, so it is read at run time.
    if let ExprKind::Assignment { value, result_target, .. } = &expr.kind {
        if result_target.is_some() {
            return PhpType::Mixed;
        }
        return direct_closure_return_expr_type(value, captures, params, ctx);
    }
    if let ExprKind::NullCoalesce { value, default } = &expr.kind {
        let value_ty = direct_closure_return_expr_type(value, captures, params, ctx);
        let default_ty = direct_closure_return_expr_type(default, captures, params, ctx);
        if value_ty == default_ty && !matches!(value_ty, PhpType::Void) {
            return value_ty;
        }
        let unknown = |ty: &PhpType| matches!(ty, PhpType::Mixed | PhpType::Union(_) | PhpType::Void);
        if unknown(&value_ty) || unknown(&default_ty) {
            return PhpType::Mixed;
        }
    }
    if let ExprKind::BinaryOp { left, op, right } = &expr.kind {
        let left_ty = direct_closure_return_expr_type(left, captures, params, ctx);
        let right_ty = direct_closure_return_expr_type(right, captures, params, ctx);
        if let Some(plan) = crate::types::object_operators::binary_plan(op, &left_ty, &right_ty) {
            return plan.result_type(op);
        }
    }
    if let ExprKind::PropertyAccess { object, property } = &expr.kind {
        let receiver_name = match &object.kind {
            ExprKind::Variable(name) => Some(name.as_str()),
            ExprKind::This => Some("this"),
            _ => None,
        };
        if let Some(receiver_name) = receiver_name {
            let receiver_ty = captures
                .iter()
                .find(|(capture_name, _, _)| capture_name == receiver_name)
                .map(|(_, ty, _)| ty)
                .or_else(|| {
                    params
                        .iter()
                        .find(|(param_name, _)| param_name == receiver_name)
                        .map(|(_, ty)| ty)
                });
            match receiver_ty {
                Some(PhpType::Object(class)) => {
                    if let Some(info) = ctx.classes.get(class.trim_start_matches('\\')) {
                        if let Some((_, ty)) =
                            info.properties.iter().find(|(name, _)| name == property)
                        {
                            return ty.clone();
                        }
                    }
                    if receiver_name == "this" {
                        // A method-defined closure may later be rebound to an unrelated class.
                        // Its signature keeps the enclosing `$this` type so ordinary known
                        // properties remain precise, but a property absent from that class is
                        // resolved against the eventual bound receiver and must stay boxed.
                        return PhpType::Mixed;
                    }
                }
                Some(receiver_ty) if receiver_ty.codegen_repr() == PhpType::Mixed => {
                    // A runtime-shaped receiver makes the property runtime-shaped too. Falling
                    // through to syntactic inference stamps an otherwise unknown property read
                    // as `int`; a later scoped `Closure::bind` clone can then find a string slot
                    // correctly but coerce that string through the stale integer signature.
                    return PhpType::Mixed;
                }
                _ => {}
            }
        }
        // A property this signature cannot type is read at run time and can hold anything.
        // The syntactic default answers `int`, and the body then converted the value on its way
        // out: `fn ($t) => $t->name` over an untyped `$t` returned `0` for every string name.
        return PhpType::Mixed;
    }
    if matches!(
        expr.kind,
        ExprKind::NullsafePropertyAccess { .. }
            | ExprKind::DynamicPropertyAccess { .. }
            | ExprKind::NullsafeDynamicPropertyAccess { .. }
    ) {
        return PhpType::Mixed;
    }
    // An element read is typed by its container when the signature knows it, and read at run
    // time otherwise. The syntactic default answers `int` here too: `fn ($r) => $r["n"]` over an
    // untyped `$r` cast every string element to `0`.
    if let ExprKind::ArrayAccess { array, .. } = &expr.kind {
        if let ExprKind::Variable(name) = &array.kind {
            let receiver_ty = captures
                .iter()
                .find(|(capture_name, _, _)| capture_name == name)
                .map(|(_, ty, _)| ty)
                .or_else(|| {
                    params
                        .iter()
                        .find(|(param_name, _)| param_name == name)
                        .map(|(_, ty)| ty)
                });
            match receiver_ty.map(PhpType::codegen_repr) {
                Some(PhpType::Array(elem)) => return elem.codegen_repr(),
                Some(PhpType::AssocArray { value, .. }) => return value.codegen_repr(),
                Some(PhpType::Str) => return PhpType::Str,
                _ => {}
            }
        }
        return PhpType::Mixed;
    }
    crate::types::checker::infer_expr_type_syntactic(expr)
}

/// Returns literal storage for a closure return, resolving elements and spread keys
/// against the closure's captures and parameters.
///
/// This mirrors `crate::ir_lower::expr::array_literal_type_for_ir`, which types the very
/// same literal while lowering the body from `LoweringContext::local_types`. The two must
/// agree: `lower_return_expr` feeds the inferred return element type back into
/// `lower_array_literal_with_expected_type`, so a slot typed `int` here casts a boxed
/// `Mixed` argument to an integer on the way into the array — `function (mixed $a, mixed $b)
/// { return [$a, $b]; }` called as `(1, "z")` produced `[1, 0]`. The syntactic fallback used
/// before this helper existed types every unrecognized item `int`, which also mis-stamped
/// `string`, `float`, `bool`, and `array` parameters.
fn direct_closure_return_array_type(
    items: &[crate::parser::ast::Expr],
    captures: &[(String, PhpType, bool)],
    params: &[(String, PhpType)],
    ctx: &LoweringContext<'_, '_>,
) -> PhpType {
    let mut elem_ty = PhpType::Never;
    let mut has_hash_spread = false;
    for item in items {
        if let ExprKind::Spread(inner) = &item.kind {
            has_hash_spread |= matches!(
                direct_closure_return_expr_type(
                    inner,
                    captures,
                    params,
                    ctx,
                )
                .codegen_repr(),
                PhpType::AssocArray { .. } | PhpType::Mixed
            );
        }
        elem_ty = crate::ir_lower::expr::merge_ir_indexed_element_type(
            elem_ty,
            direct_closure_return_array_item_type(
                item,
                captures,
                params,
                ctx,
            ),
        );
    }
    if has_hash_spread {
        PhpType::AssocArray { key: Box::new(PhpType::Mixed), value: Box::new(elem_ty) }
    } else {
        PhpType::Array(Box::new(elem_ty))
    }
}

/// Returns the EIR storage element type contributed by one indexed array-literal item.
///
/// A spread contributes its source array's element type (widened to `Mixed` for an
/// empty/unknown source, since `Void`/`Never` has no array-element representation), matching
/// the `ExprKind::Spread` arm of `array_literal_element_type_for_ir`.
fn direct_closure_return_array_item_type(
    item: &crate::parser::ast::Expr,
    captures: &[(String, PhpType, bool)],
    params: &[(String, PhpType)],
    ctx: &LoweringContext<'_, '_>,
) -> PhpType {
    if let ExprKind::Spread(inner) = &item.kind {
        let source = direct_closure_return_array_item_type(
            inner,
            captures,
            params,
            ctx,
        );
        return match source.codegen_repr() {
            PhpType::Array(elem) => match elem.codegen_repr() {
                PhpType::Void | PhpType::Never => PhpType::Mixed,
                other => other,
            },
            PhpType::AssocArray { value, .. } => value.codegen_repr(),
            _ => PhpType::Mixed,
        };
    }
    // `null` has no narrower storage than the boxed cell, exactly as the lowering-side
    // `ExprKind::Null` arm decides.
    if matches!(item.kind, ExprKind::Null) {
        return PhpType::Mixed;
    }
    crate::ir_lower::expr::ir_array_storage_type(direct_closure_return_expr_type(
        item,
        captures,
        params,
        ctx,
    ))
}

/// Returns the EIR storage type for an associative array literal returned directly from a
/// closure, resolving each value against the closure's captures and parameters.
///
/// Keys keep the syntactic rules (`normalized_array_key_type` / `merge_array_key_types`)
/// used by `assoc_array_literal_type_for_ir`; only the value slots need the signature,
/// since a `function (string $s) { return ['k' => $s]; }` value slot typed `int` made the
/// caller read the string payload back as a raw integer.
fn direct_closure_return_assoc_literal_type(
    pairs: &[(crate::parser::ast::Expr, crate::parser::ast::Expr)],
    captures: &[(String, PhpType, bool)],
    params: &[(String, PhpType)],
    ctx: &LoweringContext<'_, '_>,
) -> PhpType {
    let mut key_ty = PhpType::Never;
    let mut value_ty = PhpType::Never;
    for (key, value) in pairs {
        let next_key = crate::types::normalized_array_key_type(
            key,
            crate::types::checker::infer_expr_type_syntactic(key),
        );
        let next_value = direct_closure_return_array_item_type(
            value,
            captures,
            params,
            ctx,
        );
        key_ty = if matches!(key_ty, PhpType::Never) {
            next_key
        } else {
            crate::types::merge_array_key_types(key_ty, next_key)
        };
        value_ty = crate::ir_lower::expr::merge_ir_assoc_value_type(value_ty, next_value);
    }
    PhpType::AssocArray {
        key: Box::new(key_ty),
        value: Box::new(value_ty),
    }
}

/// Returns the EIR storage type for a directly returned literal that mixes keys with spreads.
///
/// The lowering (`lower_mixed_array_literal`) always builds a hash, and a spread renumbers the
/// integer keys it contributes, so the KEY slot is `Mixed` regardless of the written keys; the
/// value slot merges every entry's storage type, with a spread contributing its source's.
fn direct_closure_return_mixed_literal_type(
    entries: &[crate::parser::ast::ArrayEntry],
    captures: &[(String, PhpType, bool)],
    params: &[(String, PhpType)],
    ctx: &LoweringContext<'_, '_>,
) -> PhpType {
    let mut value_ty = PhpType::Never;
    for entry in entries {
        let next_value = match entry {
            crate::parser::ast::ArrayEntry::Spread(source) => {
                direct_closure_return_assoc_spread_entry_types(
                    source,
                    captures,
                    params,
                    ctx,
                )
                .1
            }
            crate::parser::ast::ArrayEntry::Keyed(_, value)
            | crate::parser::ast::ArrayEntry::Value(value) => direct_closure_return_array_item_type(
                value,
                captures,
                params,
                ctx,
            ),
        };
        value_ty = crate::ir_lower::expr::merge_ir_assoc_value_type(value_ty, next_value);
    }
    PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(value_ty),
    }
}

/// Returns the `(key, value)` storage types one `...$source` entry of a directly returned literal
/// contributes, resolved against the closure's captures and parameters.
///
/// An indexed source contributes integer keys, a hash source contributes its own key type, and a
/// source this pass cannot name stays `Mixed` on both slots.
fn direct_closure_return_assoc_spread_entry_types(
    inner: &crate::parser::ast::Expr,
    captures: &[(String, PhpType, bool)],
    params: &[(String, PhpType)],
    ctx: &LoweringContext<'_, '_>,
) -> (PhpType, PhpType) {
    let source = direct_closure_return_array_item_type(
        inner,
        captures,
        params,
        ctx,
    );
    match source.codegen_repr() {
        PhpType::Array(elem) => (PhpType::Int, elem.codegen_repr()),
        PhpType::AssocArray { key, value } => (key.codegen_repr(), value.codegen_repr()),
        _ => (PhpType::Mixed, PhpType::Mixed),
    }
}

/// Returns true when a statement list contains a `return <expr>` for its own function body.
fn body_contains_value_return(statements: &[Stmt]) -> bool {
    statements.iter().any(stmt_contains_value_return)
}

/// Returns true when `stmt` can return a value from the currently lowered function body.
fn stmt_contains_value_return(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Return(Some(_)) => true,
        StmtKind::Return(None) => false,
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            body_contains_value_return(then_body)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| body_contains_value_return(body))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body_contains_value_return(body))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            body_contains_value_return(then_body)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body_contains_value_return(body))
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => body_contains_value_return(body),
        StmtKind::For {
            init, update, body, ..
        } => {
            init.as_ref()
                .is_some_and(|stmt| stmt_contains_value_return(stmt.as_ref()))
                || update
                    .as_ref()
                    .is_some_and(|stmt| stmt_contains_value_return(stmt.as_ref()))
                || body_contains_value_return(body)
        }
        StmtKind::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|(_, body)| body_contains_value_return(body))
                || default
                    .as_ref()
                    .is_some_and(|body| body_contains_value_return(body))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            body_contains_value_return(try_body)
                || catches
                    .iter()
                    .any(|catch| body_contains_value_return(&catch.body))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body_contains_value_return(body))
        }
        _ => false,
    }
}

/// Builds a fallback function signature from AST syntax and optional variadic metadata.
fn signature_from_ast_with_variadic(
    params: &AstParams,
    return_type: Option<&TypeExpr>,
    variadic: Option<&str>,
    variadic_by_ref: bool,
) -> FunctionSig {
    let mut signature = FunctionSig {
        params: params
            .iter()
            .map(|(name, ty, _, _)| {
                (
                    name.clone(),
                    ty.as_ref()
                        .map(type_expr_to_php_type)
                        .unwrap_or(PhpType::Mixed),
                )
            })
            .collect(),
        param_type_exprs: params
            .iter()
            .map(|(_, type_ann, _, _)| type_ann.clone())
            .collect(),
        param_attributes: Vec::new(),
        defaults: params
            .iter()
            .map(|(_, _, default, _)| default.clone())
            .collect(),
        return_type: return_type
            .map(type_expr_to_php_type)
            .unwrap_or(PhpType::Mixed),
        declared_return: return_type.is_some(),
        by_ref_return: false,
        ref_params: params.iter().map(|(_, _, _, by_ref)| *by_ref).collect(),
        declared_params: params.iter().map(|(_, ty, _, _)| ty.is_some()).collect(),
        variadic: variadic.map(str::to_string),
        deprecation: None,
        is_generator: false,
    };
    append_variadic_param_slot(&mut signature, variadic_by_ref);
    signature
}

/// Adds the variadic `array<mixed>` parameter slot omitted from parsed parameter tuples.
fn append_variadic_param_slot(signature: &mut FunctionSig, variadic_by_ref: bool) {
    let Some(variadic) = signature.variadic.clone() else {
        return;
    };
    if signature.params.iter().any(|(name, _)| name == &variadic) {
        return;
    }
    signature
        .params
        .push((variadic, PhpType::Array(Box::new(PhpType::Mixed))));
    signature.param_type_exprs.push(None);
    signature.defaults.push(None);
    signature.ref_params.push(variadic_by_ref);
    signature.declared_params.push(false);
}

/// Finds a method signature using PHP's case-insensitive method key convention.
fn method_signature<'a>(
    class: &'a crate::types::ClassInfo,
    method_name: &str,
    is_static: bool,
) -> Option<&'a FunctionSig> {
    let methods = if is_static {
        &class.static_methods
    } else {
        &class.methods
    };
    methods
        .get(method_name)
        .or_else(|| methods.get(&method_name.to_ascii_lowercase()))
}

/// Formats a compact source signature string for textual EIR diagnostics.
fn source_signature(name: &str, signature: &FunctionSig) -> String {
    let params = signature
        .params
        .iter()
        .map(|(param, php_type)| format!("{}: {}", param, php_type))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{}({}) -> {}", name, params, signature.return_type)
}
