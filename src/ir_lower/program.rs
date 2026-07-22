//! Purpose:
//! Orchestrates AST-to-EIR lowering for a complete checked program.
//!
//! Called from:
//! - `crate::ir_lower::lower_program()`.
//!
//! Key details:
//! - Declaration bodies are lowered before synthetic `main`; declaration
//!   statements themselves are no-ops inside `main`.
//! - The module is validated before it is returned to CLI/test callers.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use crate::codegen::platform::Target;
use crate::codegen::RuntimeFeatures;
use crate::intrinsics::IntrinsicCall;
use crate::ir::{
    validate_module, ExternDecl, ExternParamDecl, Function, Immediate, IrType, LocalKind, Module,
    Op, TraitMethodInfo,
};
use crate::ir_lower::{builtin_datetime, function, LoweringError};
use crate::names::php_symbol_key;
use crate::parser::ast::{
    ClassMethod, Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind, Visibility,
};
use crate::types::{CheckResult, ClassInfo, FunctionSig, InterfaceInfo, PhpType};

/// Lowers an optimized typed AST program into a validated EIR module.
///
/// `web` mirrors the CLI `--web` flag (the same source
/// `codegen_ir::block_emit::emit_module` receives) and is stored on the
/// returned module so every lowering entry point in `function.rs` can gate
/// request-superglobal type seeding on it; see `Module::web`.
pub(crate) fn lower(
    program: &Program,
    check_result: &CheckResult,
    target: Target,
    source_path: Option<&Path>,
    web: bool,
) -> Result<Module, LoweringError> {
    let mut module = Module::new(target);
    module.source_path = source_path.map(canonical_source_path);
    module.web = web;
    let constants = crate::codegen::collect_constants(program, target.platform);
    module.global_constants = constants.clone();
    let fiber_return_sigs = crate::ir_lower::fibers::collect_fiber_return_sigs(program);
    populate_metadata(&mut module, program, check_result);
    lower_function_declarations(
        program,
        &mut module,
        check_result,
        &constants,
        &fiber_return_sigs,
    );
    lower_class_like_methods(
        program,
        &mut module,
        check_result,
        &constants,
        &fiber_return_sigs,
    );
    lower_property_init_thunks(&mut module, check_result, &constants, &fiber_return_sigs);
    function::lower_main(
        program,
        &mut module,
        check_result,
        &constants,
        &fiber_return_sigs,
    );
    lower_literal_eval_aot_functions(&mut module, check_result, &constants, &fiber_return_sigs);
    include_lowered_runtime_features(&mut module);
    super::reflection::lower_referenced_builtin_methods(
        &mut module,
        check_result,
        &constants,
        &fiber_return_sigs,
    );
    lower_referenced_builtin_spl_methods(&mut module, check_result, &constants, &fiber_return_sigs);
    builtin_datetime::lower_referenced_builtin_datetime_methods(
        &mut module,
        check_result,
        &constants,
        &fiber_return_sigs,
    );
    include_lowered_runtime_features(&mut module);
    validate_module(&module)?;
    Ok(module)
}

/// Converts a PHP source path into the canonical display string stored in EIR metadata.
fn canonical_source_path(source_path: &Path) -> String {
    source_path
        .canonicalize()
        .unwrap_or_else(|_| source_path.to_path_buf())
        .display()
        .to_string()
}

/// Copies declaration metadata into the EIR module placeholder tables.
fn populate_metadata(module: &mut Module, program: &Program, check_result: &CheckResult) {
    module.class_table.names = sorted_keys(&check_result.classes);
    module.enum_table.names = sorted_keys(&check_result.enums);
    module.interface_table.names = sorted_keys(&check_result.interfaces);
    module.trait_table.names = collect_declared_trait_names(program);
    module.declared_class_names = collect_declared_class_names(program, &check_result.classes);
    module.declared_interface_names =
        collect_declared_interface_names(program, &check_result.interfaces);
    module.declared_trait_names = collect_declared_trait_names(program);
    module.declared_trait_source_lines = collect_declared_trait_source_lines(program);
    module.declared_trait_uses = collect_declared_trait_uses(program);
    module.declared_trait_method_names = collect_declared_trait_method_names(program);
    module.declared_trait_methods = collect_declared_trait_methods(program);
    module.declared_trait_property_names = collect_declared_trait_property_names(program);
    module.declared_trait_constant_names = collect_declared_trait_constant_names(program);
    module.declared_trait_constants = collect_declared_trait_constants(program);
    module.declared_trait_constant_types = collect_declared_trait_constant_types(program);
    module.declared_trait_constant_visibilities =
        collect_declared_trait_constant_visibilities(program);
    module.declared_trait_final_constants = collect_declared_trait_final_constants(program);
    module.class_infos = check_result.classes.clone();
    normalize_class_method_signatures_for_eir(module, &check_result.callable_param_sigs);
    module.interface_infos = check_result.interfaces.clone();
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
                .map(|(name, php_type)| ExternParamDecl {
                    name: name.clone(),
                    ir_type: value_or_void_ir_type(php_type),
                    php_type: php_type.clone(),
                })
                .collect(),
            return_type: value_or_void_ir_type(&sig.return_type),
            return_php_type: sig.return_type.clone(),
            link_libs: sig.library.iter().cloned().collect(),
        })
        .collect();
    module.required_runtime_features =
        crate::codegen::runtime_features_for_program_and_classes(program, &check_result.classes);
}

/// Normalizes class method metadata to the ABI contracts emitted in EIR.
fn normalize_class_method_signatures_for_eir(
    module: &mut Module,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
) {
    for (class_name, class_info) in module.class_infos.iter_mut() {
        normalize_method_map_for_eir(
            class_name,
            &mut class_info.methods,
            &class_info.method_decls,
            false,
            callable_param_sigs,
        );
        normalize_method_map_for_eir(
            class_name,
            &mut class_info.static_methods,
            &class_info.method_decls,
            true,
            callable_param_sigs,
        );
    }
}

/// Normalizes one instance/static method table for EIR call and bridge metadata.
fn normalize_method_map_for_eir(
    class_name: &str,
    methods: &mut HashMap<String, FunctionSig>,
    method_decls: &[ClassMethod],
    is_static: bool,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
) {
    // Stream-wrapper and user-filter contract methods are invoked through
    // runtime vtables with raw fixed-ABI arguments; widening their untyped
    // params to boxed Mixed would desynchronize the dispatcher and the body.
    let is_wrapper_class = methods.contains_key("stream_open");
    let is_filter_class = methods.contains_key("filter");
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
fn method_return_exposes_dynamic_param(
    method: &ClassMethod,
    signature: &FunctionSig,
    owner_name: &str,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
) -> bool {
    if signature.declared_return {
        return false;
    }
    let dynamic_params = dynamic_untyped_param_names(owner_name, signature, callable_param_sigs);
    !dynamic_params.is_empty() && body_returns_dynamic_param(&method.body, &dynamic_params)
}

/// Collects untyped by-value parameter names that need a boxed EIR ABI.
fn dynamic_untyped_param_names(
    owner_name: &str,
    signature: &FunctionSig,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
) -> HashSet<String> {
    let mut names = HashSet::new();
    for (index, (name, php_type)) in signature.params.iter().enumerate() {
        let declared = signature
            .declared_params
            .get(index)
            .copied()
            .unwrap_or(false);
        let by_ref = signature.ref_params.get(index).copied().unwrap_or(false);
        let variadic = signature.variadic.as_deref() == Some(name.as_str());
        let preserved = matches!(php_type.codegen_repr(), PhpType::Callable)
            || callable_param_sigs.contains_key(&(owner_name.to_string(), name.to_string()));
        if !declared && !by_ref && !variadic && !preserved {
            names.insert(name.clone());
        }
    }
    names
}

/// Recursively scans a method body for returns that expose dynamic parameters.
fn body_returns_dynamic_param(body: &[Stmt], dynamic_params: &HashSet<String>) -> bool {
    body.iter()
        .any(|stmt| stmt_returns_dynamic_param(stmt, dynamic_params))
}

/// Returns true when one statement can return a dynamic parameter directly.
fn stmt_returns_dynamic_param(stmt: &Stmt, dynamic_params: &HashSet<String>) -> bool {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => expr_exposes_dynamic_param(expr, dynamic_params),
        StmtKind::Return(None) => false,
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            body_returns_dynamic_param(then_body, dynamic_params)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| body_returns_dynamic_param(body, dynamic_params))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body_returns_dynamic_param(body, dynamic_params))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            body_returns_dynamic_param(then_body, dynamic_params)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body_returns_dynamic_param(body, dynamic_params))
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => body_returns_dynamic_param(body, dynamic_params),
        StmtKind::For {
            init, update, body, ..
        } => {
            init.as_ref()
                .is_some_and(|stmt| stmt_returns_dynamic_param(stmt.as_ref(), dynamic_params))
                || update
                    .as_ref()
                    .is_some_and(|stmt| stmt_returns_dynamic_param(stmt.as_ref(), dynamic_params))
                || body_returns_dynamic_param(body, dynamic_params)
        }
        StmtKind::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|(_, body)| body_returns_dynamic_param(body, dynamic_params))
                || default
                    .as_ref()
                    .is_some_and(|body| body_returns_dynamic_param(body, dynamic_params))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            body_returns_dynamic_param(try_body, dynamic_params)
                || catches
                    .iter()
                    .any(|catch| body_returns_dynamic_param(&catch.body, dynamic_params))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body_returns_dynamic_param(body, dynamic_params))
        }
        _ => false,
    }
}

/// Returns true when an expression can yield one of the dynamic parameters directly.
fn expr_exposes_dynamic_param(expr: &Expr, dynamic_params: &HashSet<String>) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => dynamic_params.contains(name),
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            expr_exposes_dynamic_param(value, dynamic_params)
                || expr_exposes_dynamic_param(default, dynamic_params)
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            expr_exposes_dynamic_param(then_expr, dynamic_params)
                || expr_exposes_dynamic_param(else_expr, dynamic_params)
        }
        ExprKind::Match { arms, default, .. } => {
            arms.iter()
                .any(|(_, arm)| expr_exposes_dynamic_param(arm, dynamic_params))
                || default
                    .as_ref()
                    .is_some_and(|expr| expr_exposes_dynamic_param(expr, dynamic_params))
        }
        ExprKind::ErrorSuppress(inner) => expr_exposes_dynamic_param(inner, dynamic_params),
        ExprKind::Assignment { value, .. } => {
            expr_exposes_dynamic_param(value, dynamic_params)
        }
        _ => false,
    }
}

/// Adds optional runtime features referenced by synthetic or lowered EIR functions.
pub(super) fn include_lowered_runtime_features(module: &mut Module) {
    let features = lowered_runtime_features(module);
    module.required_runtime_features.regex |= features.regex;
    module.required_runtime_features.mb_strlen |= features.mb_strlen;
    module.required_runtime_features.phar_archive |= features.phar_archive;
    module.required_runtime_features.descriptor_invoker |= features.descriptor_invoker;
    module.required_runtime_features.eval_bridge |= features.eval_bridge;
    module.required_runtime_features.eval_scope |= features.eval_scope;
}

/// Derives optional runtime features from the actual EIR instruction stream.
fn lowered_runtime_features(module: &Module) -> RuntimeFeatures {
    let mut features = RuntimeFeatures::none();
    for function in all_lowered_functions(module) {
        if function_contains_eval_scope_state(function) {
            features.eval_scope = true;
        }
        if function_contains_eval_context_state(function) {
            features.eval_bridge = true;
        }
        for (inst_index, inst) in function.instructions.iter().enumerate() {
            match inst.op {
                Op::RuntimeCall => {
                    if let Some(target) = typed_builtin_target(inst) {
                        features.regex |= target.uses_regex_runtime();
                        features.mb_strlen |= target.uses_mb_strlen_runtime();
                        features.phar_archive |= target.publishes_phar_symbols()
                            && function_belongs_to_phar_archive_helper_class(function);
                        features.descriptor_invoker |=
                            typed_builtin_requires_descriptor_invoker(function, inst, target);
                    }
                }
                Op::LanguageConstructCall => {
                    if language_construct_call_requires_eval(module, inst) {
                        features.eval_bridge = true;
                    }
                }
                Op::EvalLiteralCall => {
                    if eval_literal_call_requires_bridge(module, function, inst_index, inst) {
                        features.eval_bridge = true;
                    }
                }
                Op::EvalScopeGet | Op::EvalScopeSet => {
                    features.eval_scope = true;
                }
                Op::EvalFunctionCall
                | Op::EvalFunctionCallArray
                | Op::EvalFunctionExists
                | Op::EvalClassExists
                | Op::EvalConstantExists
                | Op::EvalConstantFetch
                | Op::EvalStaticMethodCall => {
                    features.eval_bridge = true;
                }
                Op::ExprCall | Op::CallableDescriptorInvoke => {
                    features.descriptor_invoker = true;
                }
                _ => {}
            }
        }
    }
    features
}

/// Returns true when a lowered function owns hidden eval scope handle slots.
/// Scope-only functions use the native scope helpers and must not force the
/// magician bridge staticlib into the link.
fn function_contains_eval_scope_state(function: &Function) -> bool {
    function.locals.iter().any(|local| {
        matches!(
            local.kind,
            LocalKind::EvalScope | LocalKind::EvalGlobalScope
        )
    })
}

/// Returns true when a lowered function owns an interpreter context handle,
/// which requires the full magician eval bridge runtime.
fn function_contains_eval_context_state(function: &Function) -> bool {
    function
        .locals
        .iter()
        .any(|local| matches!(local.kind, LocalKind::EvalContext))
}

/// Returns true when a literal eval call still needs the magician bridge runtime.
fn eval_literal_call_requires_bridge(
    module: &Module,
    function: &Function,
    inst_index: usize,
    inst: &crate::ir::Instruction,
) -> bool {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return true;
    };
    let Some(fragment) = module.data.strings.get(data.as_raw() as usize) else {
        return true;
    };
    let plan = crate::eval_aot::plan_literal_fragment_with_source_path_and_static_and_method_calls(
        fragment,
        module.source_path.as_deref(),
        |name, args| eval_literal_static_function_supported_by_module(module, name, args),
        |receiver, method, args| {
            eval_literal_static_method_supported_by_module(module, receiver, method, args)
        },
    );
    if plan.uses_scope_read_params() {
        return !eval_literal_call_can_use_scope_read_params(module, function, inst_index, &plan);
    }
    if plan.requires_runtime_eval_scope()
        && !eval_literal_call_scope_constraints_supported(module, function, inst_index, &plan)
    {
        return true;
    }
    plan.requires_runtime_eval_bridge()
}

/// Returns true when a local slot is initialized before the eval instruction.
fn eval_scope_read_slot_initialized(
    function: &Function,
    slot: crate::ir::LocalSlotId,
    inst_index: usize,
) -> bool {
    if function
        .params
        .get(slot.as_raw() as usize)
        .is_some_and(|param| !param.by_ref)
    {
        return true;
    }
    function
        .instructions
        .iter()
        .take(inst_index)
        .any(|inst| inst.op == Op::StoreLocal && inst.immediate == Some(Immediate::LocalSlot(slot)))
}

/// Returns true when a read-only eval call can pass direct Mixed params safely.
fn eval_literal_call_can_use_scope_read_params(
    module: &Module,
    function: &Function,
    inst_index: usize,
    plan: &crate::eval_aot::EvalAotPlan,
) -> bool {
    plan.reads().iter().all(|name| {
        eval_literal_call_scope_read_param_supported(module, function, inst_index, name)
    }) && plan.array_read_constraints().iter().all(|name| {
        eval_literal_call_scope_read_array_param_supported(module, function, inst_index, name)
    }) && plan.assoc_array_read_constraints().iter().all(|name| {
        eval_literal_call_scope_read_assoc_array_param_supported(module, function, inst_index, name)
    }) && plan.float_predicate_read_constraints().iter().all(|name| {
        eval_literal_call_scope_read_float_predicate_param_supported(
            module, function, inst_index, name,
        )
    })
}

/// Returns true when scope-based eval AOT satisfies caller-side type constraints.
fn eval_literal_call_scope_constraints_supported(
    module: &Module,
    function: &Function,
    inst_index: usize,
    plan: &crate::eval_aot::EvalAotPlan,
) -> bool {
    plan.array_read_constraints().iter().all(|name| {
        eval_literal_call_scope_read_array_param_supported(module, function, inst_index, name)
    }) && plan.assoc_array_read_constraints().iter().all(|name| {
        eval_literal_call_scope_read_assoc_array_param_supported(module, function, inst_index, name)
    }) && plan.float_predicate_read_constraints().iter().all(|name| {
        eval_literal_call_scope_read_float_predicate_param_supported(
            module, function, inst_index, name,
        )
    })
}

/// Returns true when one caller read can be boxed or represented as undefined null.
fn eval_literal_call_scope_read_param_supported(
    _module: &Module,
    function: &Function,
    inst_index: usize,
    name: &str,
) -> bool {
    if crate::superglobals::is_superglobal(name) {
        return false;
    }
    let Some(slot) = eval_scope_local_slot(function, name) else {
        return true;
    };
    eval_scope_read_param_type_supported(&slot.php_type)
        && eval_scope_read_slot_initialized(function, slot.id, inst_index)
}

/// Returns true when one caller read is initialized with an array-compatible type.
fn eval_literal_call_scope_read_array_param_supported(
    _module: &Module,
    function: &Function,
    inst_index: usize,
    name: &str,
) -> bool {
    if crate::superglobals::is_superglobal(name) {
        return false;
    }
    let Some(slot) = eval_scope_local_slot(function, name) else {
        return false;
    };
    eval_scope_read_array_param_type_supported(&slot.php_type)
        && eval_scope_read_slot_initialized(function, slot.id, inst_index)
}

/// Returns true when one caller read is initialized with an associative-array type.
fn eval_literal_call_scope_read_assoc_array_param_supported(
    _module: &Module,
    function: &Function,
    inst_index: usize,
    name: &str,
) -> bool {
    if crate::superglobals::is_superglobal(name) {
        return false;
    }
    let Some(slot) = eval_scope_local_slot(function, name) else {
        return false;
    };
    eval_scope_read_assoc_array_param_type_supported(&slot.php_type)
        && eval_scope_read_slot_initialized(function, slot.id, inst_index)
}

/// Returns true when one caller read can feed IEEE float predicates safely.
fn eval_literal_call_scope_read_float_predicate_param_supported(
    _module: &Module,
    function: &Function,
    inst_index: usize,
    name: &str,
) -> bool {
    if crate::superglobals::is_superglobal(name) {
        return false;
    }
    let Some(slot) = eval_scope_local_slot(function, name) else {
        return false;
    };
    eval_scope_read_float_predicate_param_type_supported(&slot.php_type)
        && eval_scope_read_slot_initialized(function, slot.id, inst_index)
}

/// Returns true when a caller local can be boxed into a direct eval read param.
fn eval_scope_read_param_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Void
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
            | PhpType::Mixed
            | PhpType::Union(_)
    )
}

/// Returns true when a caller local satisfies array-only read-param semantics.
fn eval_scope_read_array_param_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. }
    )
}

/// Returns true when a caller local satisfies associative-array-only semantics.
fn eval_scope_read_assoc_array_param_type_supported(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::AssocArray { .. })
}

/// Returns true when a caller local can feed IEEE float predicates without TypeError.
fn eval_scope_read_float_predicate_param_type_supported(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Int | PhpType::Float)
}

/// Returns the caller local slot that can provide a direct scope-read parameter.
fn eval_scope_local_slot<'a>(
    function: &'a Function,
    name: &str,
) -> Option<&'a crate::ir::LocalSlot> {
    function
        .locals
        .iter()
        .find(|local| local.name.as_deref() == Some(name) && local.kind == LocalKind::PhpLocal)
}

/// Returns true when a static function call matches the codegen-supported subset.
fn eval_literal_static_function_supported_by_module(
    module: &Module,
    name: &str,
    args: &[Expr],
) -> bool {
    if args.len() > 6 {
        return false;
    }
    let key = php_symbol_key(name.trim_start_matches('\\'));
    let Some(function) = module
        .functions
        .iter()
        .find(|function| php_symbol_key(function.name.trim_start_matches('\\')) == key)
    else {
        return false;
    };
    let Some(signature) = &function.signature else {
        return false;
    };
    crate::eval_aot::static_function_signature_supported(signature, args)
}

/// Returns true when a static method call matches the codegen-supported subset.
fn eval_literal_static_method_supported_by_module(
    module: &Module,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
) -> bool {
    if args.len() > 6 {
        return false;
    }
    let StaticReceiver::Named(class_name) = receiver else {
        return false;
    };
    let class_name = class_name.as_str().trim_start_matches('\\');
    let method_key = php_symbol_key(method);
    let Some(receiver_info) = module.class_infos.get(class_name) else {
        return false;
    };
    if receiver_info
        .static_method_visibilities
        .get(&method_key)
        .unwrap_or(&Visibility::Public)
        != &Visibility::Public
    {
        return false;
    }
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(class_name);
    let Some(signature) = module
        .class_infos
        .get(impl_class)
        .and_then(|class_info| class_info.static_methods.get(&method_key))
    else {
        return false;
    };
    crate::eval_aot::static_function_signature_supported(signature, args)
}

/// Adds internal EIR functions for literal eval fragments accepted by the EIR AOT subset.
fn lower_literal_eval_aot_functions(
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    let candidates = collect_literal_eval_aot_function_candidates(module);
    let mut lowered_names = all_lowered_functions(module)
        .map(|function| function.name.clone())
        .collect::<HashSet<_>>();
    for (name, body) in candidates {
        match body {
            EvalAotFunctionCandidate::NoScope { body } => {
                if !lowered_names.insert(name.clone()) {
                    continue;
                }
                function::lower_eval_aot_function(
                    &name,
                    &body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
            }
            EvalAotFunctionCandidate::Scope {
                body,
                reads,
                direct_writes,
                flush_writes,
            } => {
                if !lowered_names.insert(name.clone()) {
                    continue;
                }
                function::lower_eval_aot_scope_function(
                    &name,
                    &body,
                    &reads,
                    &direct_writes,
                    &flush_writes,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
            }
        }
    }
}

/// Candidate shape for an internal eval AOT EIR function.
enum EvalAotFunctionCandidate {
    NoScope {
        body: Program,
    },
    Scope {
        body: Program,
        reads: BTreeSet<String>,
        direct_writes: BTreeSet<String>,
        flush_writes: BTreeSet<String>,
    },
}

/// Collects unique literal eval fragments that can be emitted as internal EIR functions.
fn collect_literal_eval_aot_function_candidates(
    module: &Module,
) -> Vec<(String, EvalAotFunctionCandidate)> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    for function in all_lowered_functions(module) {
        for inst in &function.instructions {
            let Some(fragment) = eval_literal_fragment_from_inst(module, inst) else {
                continue;
            };
            let mut plan =
                crate::eval_aot::plan_literal_fragment_with_source_path_and_static_and_method_calls(
                    &fragment,
                    module.source_path.as_deref(),
                    |name, args| {
                        eval_literal_static_function_supported_by_module(module, name, args)
                    },
                    |receiver, method, args| {
                        eval_literal_static_method_supported_by_module(
                            module, receiver, method, args,
                        )
                    },
                );
            if let Some(name) = plan.take_function_name() {
                if !seen.insert(name.clone()) {
                    continue;
                }
                let Some(program) = plan.take_eir_program() else {
                    continue;
                };
                candidates.push((name, EvalAotFunctionCandidate::NoScope { body: program }));
                continue;
            }
            let Some(name) = plan.take_scope_function_name() else {
                continue;
            };
            if !seen.insert(name.clone()) {
                continue;
            };
            let reads = plan.reads().clone();
            let direct_writes = plan.direct_writes().clone();
            let flush_writes = plan.flush_writes().clone();
            let Some(program) = plan.take_scope_eir_program() else {
                continue;
            };
            candidates.push((
                name,
                EvalAotFunctionCandidate::Scope {
                    body: program,
                    reads,
                    direct_writes,
                    flush_writes,
                },
            ));
        }
    }
    candidates
}

/// Returns the string payload from an `EvalLiteralCall` instruction.
fn eval_literal_fragment_from_inst(
    module: &Module,
    inst: &crate::ir::Instruction,
) -> Option<String> {
    if inst.op != Op::EvalLiteralCall {
        return None;
    }
    let Some(Immediate::Data(data)) = inst.immediate else {
        return None;
    };
    module.data.strings.get(data.as_raw() as usize).cloned()
}

/// Iterates every function-like body already materialized into the EIR module.
pub(super) fn all_lowered_functions(module: &Module) -> impl Iterator<Item = &Function> {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
}

/// Returns the typed builtin target carried by a runtime-call instruction.
fn typed_builtin_target(
    inst: &crate::ir::Instruction,
) -> Option<crate::ir::RuntimeFnId> {
    match inst.immediate {
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(target))) => Some(target),
        _ => None,
    }
}

/// Returns whether a typed builtin callback operand needs descriptor invocation support.
fn typed_builtin_requires_descriptor_invoker(
    function: &Function,
    inst: &crate::ir::Instruction,
    target: crate::ir::RuntimeFnId,
) -> bool {
    let Some(callback_index) = target.string_callback_operand_index() else {
        return false;
    };
    let Some(callback) = inst.operands.get(callback_index).copied() else {
        return false;
    };
    function
        .value(callback)
        .is_some_and(|value| value.php_type.codegen_repr() == PhpType::Str)
}

/// Returns whether a compiler-resident call references the optional eval bridge.
fn language_construct_call_requires_eval(
    module: &Module,
    inst: &crate::ir::Instruction,
) -> bool {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return false;
    };
    module
        .data
        .function_names
        .get(data.as_raw() as usize)
        .is_some_and(|name| php_symbol_key(name.trim_start_matches('\\')) == "eval")
}

/// Returns whether a function belongs to a stream/archive helper class.
fn function_belongs_to_phar_archive_helper_class(function: &Function) -> bool {
    let Some((class_name, _)) = function.name.split_once("::") else {
        return false;
    };
    is_phar_archive_helper_class_name(class_name)
}

/// Returns true when a class has generated methods that can route paths through PHAR helpers.
fn is_phar_archive_helper_class_name(name: &str) -> bool {
    matches!(
        crate::names::php_symbol_key(name.trim_start_matches('\\')).as_str(),
        "phar" | "phardata" | "splfileobject" | "spltempfileobject"
    )
}

/// Lowers per-class property-default thunks referenced by `_class_propinit_ptrs`.
fn lower_property_init_thunks(
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    let mut classes = check_result.classes.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        if super::reflection::canonical_builtin_reflection_class_name(class_name).is_some() {
            continue;
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
}

/// Returns deterministic sorted keys for metadata placeholder tables.
fn sorted_keys<T>(map: &std::collections::HashMap<String, T>) -> Vec<String> {
    let mut keys = map.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    keys
}

/// Collects PHP-visible class and enum names in the order `get_declared_classes()` must expose.
fn collect_declared_class_names(
    program: &Program,
    classes: &HashMap<String, ClassInfo>,
) -> Vec<String> {
    let mut user_names = Vec::new();
    collect_program_declared_names(
        program,
        classes,
        &mut HashSet::new(),
        &mut user_names,
        |stmt| match &stmt.kind {
            StmtKind::ClassDecl { name, .. } | StmtKind::EnumDecl { name, .. } => {
                Some(name.as_str())
            }
            _ => None,
        },
    );
    prepend_internal_names(classes.keys(), &user_names)
}

/// Collects PHP-visible interface names in the order `get_declared_interfaces()` must expose.
fn collect_declared_interface_names(
    program: &Program,
    interfaces: &HashMap<String, InterfaceInfo>,
) -> Vec<String> {
    let mut user_names = Vec::new();
    collect_program_declared_names(
        program,
        interfaces,
        &mut HashSet::new(),
        &mut user_names,
        |stmt| match &stmt.kind {
            StmtKind::InterfaceDecl { name, .. } => Some(name.as_str()),
            _ => None,
        },
    );
    prepend_internal_names(interfaces.keys(), &user_names)
}

/// Collects user-declared trait names in source order, including namespace blocks.
fn collect_declared_trait_names(program: &Program) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl { name, .. } => names.push(name.clone()),
            StmtKind::NamespaceBlock { body, .. } => {
                names.extend(collect_declared_trait_names(body));
            }
            _ => {}
        }
    }
    names
}

/// Collects source line metadata for user-declared traits, keyed by trait name.
fn collect_declared_trait_source_lines(program: &Program) -> HashMap<String, u32> {
    let mut lines = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl { name, .. } => {
                lines.insert(name.clone(), stmt.span.line);
            }
            StmtKind::NamespaceBlock { body, .. } => {
                lines.extend(collect_declared_trait_source_lines(body));
            }
            _ => {}
        }
    }
    lines
}

/// Collects direct trait-to-trait use declarations keyed by declaring trait name.
fn collect_declared_trait_uses(program: &Program) -> HashMap<String, Vec<String>> {
    let mut uses = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name, trait_uses, ..
            } => {
                uses.insert(
                    name.clone(),
                    trait_uses
                        .iter()
                        .flat_map(|trait_use| trait_use.trait_names.iter())
                        .map(|trait_name| trait_name.as_str().to_string())
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                uses.extend(collect_declared_trait_uses(body));
            }
            _ => {}
        }
    }
    uses
}

/// Collects direct PHP method names declared by each trait in source order.
fn collect_declared_trait_method_names(program: &Program) -> HashMap<String, Vec<String>> {
    let mut methods = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                methods: trait_methods,
                ..
            } => {
                methods.insert(
                    name.clone(),
                    trait_methods
                        .iter()
                        .map(|method| php_symbol_key(&method.name))
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                methods.extend(collect_declared_trait_method_names(body));
            }
            _ => {}
        }
    }
    methods
}

/// Collects direct trait method metadata keyed by trait and PHP method key.
fn collect_declared_trait_methods(
    program: &Program,
) -> HashMap<String, HashMap<String, TraitMethodInfo>> {
    let mut methods = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                methods: trait_methods,
                ..
            } => {
                methods.insert(
                    name.clone(),
                    trait_methods
                        .iter()
                        .map(|method| {
                            let method_key = php_symbol_key(&method.name);
                            let info = TraitMethodInfo {
                                signature: function::method_signature_from_ast(method),
                                visibility: method.visibility.clone(),
                                is_static: method.is_static,
                                is_final: method.is_final,
                                is_abstract: method.is_abstract,
                            };
                            (method_key, info)
                        })
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                methods.extend(collect_declared_trait_methods(body));
            }
            _ => {}
        }
    }
    methods
}

/// Collects direct PHP property names declared by each trait in source order.
fn collect_declared_trait_property_names(program: &Program) -> HashMap<String, Vec<String>> {
    let mut properties = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                properties: trait_properties,
                ..
            } => {
                properties.insert(
                    name.clone(),
                    trait_properties
                        .iter()
                        .map(|property| property.name.clone())
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                properties.extend(collect_declared_trait_property_names(body));
            }
            _ => {}
        }
    }
    properties
}

/// Collects direct PHP constant names declared by each trait in source order.
fn collect_declared_trait_constant_names(program: &Program) -> HashMap<String, Vec<String>> {
    let mut constants = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                constants: trait_constants,
                ..
            } => {
                constants.insert(
                    name.clone(),
                    trait_constants
                        .iter()
                        .map(|constant| constant.name.clone())
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                constants.extend(collect_declared_trait_constant_names(body));
            }
            _ => {}
        }
    }
    constants
}

/// Collects direct PHP constant expressions declared by each trait.
fn collect_declared_trait_constants(program: &Program) -> HashMap<String, HashMap<String, Expr>> {
    let mut constants = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                constants: trait_constants,
                ..
            } => {
                constants.insert(
                    name.clone(),
                    trait_constants
                        .iter()
                        .map(|constant| (constant.name.clone(), constant.value.clone()))
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                constants.extend(collect_declared_trait_constants(body));
            }
            _ => {}
        }
    }
    constants
}

/// Collects direct PHP declared constant types for each trait.
fn collect_declared_trait_constant_types(
    program: &Program,
) -> HashMap<String, HashMap<String, crate::parser::ast::TypeExpr>> {
    let mut types = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                constants: trait_constants,
                ..
            } => {
                types.insert(
                    name.clone(),
                    trait_constants
                        .iter()
                        .filter_map(|constant| {
                            constant
                                .type_expr
                                .clone()
                                .map(|type_expr| (constant.name.clone(), type_expr))
                        })
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                types.extend(collect_declared_trait_constant_types(body));
            }
            _ => {}
        }
    }
    types
}

/// Collects direct PHP constant visibilities declared by each trait.
fn collect_declared_trait_constant_visibilities(
    program: &Program,
) -> HashMap<String, HashMap<String, Visibility>> {
    let mut constants = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                constants: trait_constants,
                ..
            } => {
                constants.insert(
                    name.clone(),
                    trait_constants
                        .iter()
                        .map(|constant| (constant.name.clone(), constant.visibility.clone()))
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                constants.extend(collect_declared_trait_constant_visibilities(body));
            }
            _ => {}
        }
    }
    constants
}

/// Collects direct PHP final constant names declared by each trait.
fn collect_declared_trait_final_constants(program: &Program) -> HashMap<String, HashSet<String>> {
    let mut constants = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                constants: trait_constants,
                ..
            } => {
                constants.insert(
                    name.clone(),
                    trait_constants
                        .iter()
                        .filter(|constant| constant.is_final)
                        .map(|constant| constant.name.clone())
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                constants.extend(collect_declared_trait_final_constants(body));
            }
            _ => {}
        }
    }
    constants
}

/// Recursively collects source-declared names that are present in checked metadata.
fn collect_program_declared_names<T>(
    program: &Program,
    known: &HashMap<String, T>,
    seen: &mut HashSet<String>,
    out: &mut Vec<String>,
    pick: impl Copy + Fn(&Stmt) -> Option<&str>,
) {
    for stmt in program {
        match &stmt.kind {
            StmtKind::NamespaceBlock { body, .. } => {
                collect_program_declared_names(body, known, seen, out, pick);
            }
            _ => {
                let Some(name) = pick(stmt) else {
                    continue;
                };
                let key = crate::names::php_symbol_key(name);
                let is_known = known.contains_key(name)
                    || known.keys().any(|candidate| {
                        crate::names::php_symbol_key(candidate.trim_start_matches('\\')) == key
                    });
                if is_known && seen.insert(key) {
                    out.push(name.to_string());
                }
            }
        }
    }
}

/// Prepends deterministic internal names before source-order user declarations.
fn prepend_internal_names<'a>(
    known_names: impl Iterator<Item = &'a String>,
    user_names: &[String],
) -> Vec<String> {
    let user_keys: HashSet<String> = user_names
        .iter()
        .map(|name| crate::names::php_symbol_key(name))
        .collect();
    let mut names: Vec<String> = known_names
        .filter(|name| !is_internal_synthetic_class_name(name))
        .filter(|name| !user_keys.contains(&crate::names::php_symbol_key(name)))
        .cloned()
        .collect();
    names.sort();
    names.extend(user_names.iter().cloned());
    names
}

/// Returns true for compiler-internal helper classes hidden from PHP introspection.
fn is_internal_synthetic_class_name(name: &str) -> bool {
    crate::names::php_symbol_key(name).starts_with("__elephc")
}

/// Converts a PHP type to EIR storage while preserving true void returns.
fn value_or_void_ir_type(php_type: &PhpType) -> IrType {
    match php_type {
        PhpType::Void | PhpType::Never => IrType::Void,
        other => IrType::from_php(other),
    }
}

/// Lowers every function declaration reachable in the statement tree.
fn lower_function_declarations(
    statements: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    for stmt in statements {
        match &stmt.kind {
            StmtKind::FunctionDecl {
                by_ref_return: _,
                name,
                params,
                variadic: _,
                variadic_by_ref: _,
                variadic_type: _,
                return_type,
                body,
                ..
            } => function::lower_user_function(
                name,
                params,
                return_type.as_ref(),
                &stmt.attributes,
                body,
                module,
                check_result,
                constants,
                fiber_return_sigs,
            ),
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => {
                lower_function_declarations(
                    body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                lower_function_declarations(
                    then_body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
                for (_, body) in elseif_clauses {
                    lower_function_declarations(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
                if let Some(body) = else_body {
                    lower_function_declarations(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                lower_function_declarations(
                    then_body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
                if let Some(body) = else_body {
                    lower_function_declarations(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                lower_function_declarations(
                    body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    lower_function_declarations(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
                if let Some(body) = default {
                    lower_function_declarations(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                lower_function_declarations(
                    try_body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
                for catch in catches {
                    lower_function_declarations(
                        &catch.body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
                if let Some(body) = finally_body {
                    lower_function_declarations(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
            }
            _ => {}
        }
    }
}

/// Lowers concrete class/interface methods, including trait methods flattened into classes.
fn lower_class_like_methods(
    statements: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    for stmt in statements {
        match &stmt.kind {
            StmtKind::ClassDecl { name, methods, .. } => {
                let methods = check_result
                    .classes
                    .get(name)
                    .map(|class_info| class_info.method_decls.as_slice())
                    .unwrap_or(methods.as_slice());
                lower_methods_for_class_like(
                    name,
                    methods,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
            }
            StmtKind::TraitDecl { .. } => {}
            StmtKind::InterfaceDecl { name, methods, .. } => {
                lower_methods_for_class_like(
                    name,
                    methods,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
            }
            StmtKind::EnumDecl { name, methods, .. } => {
                // Enum methods are lowered like class methods on the case singleton; prefer the
                // checker's flattened declarations (with `self` types resolved to the enum).
                let methods = check_result
                    .classes
                    .get(name)
                    .map(|class_info| class_info.method_decls.as_slice())
                    .unwrap_or(methods.as_slice());
                lower_methods_for_class_like(
                    name,
                    methods,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => {
                lower_class_like_methods(body, module, check_result, constants, fiber_return_sigs);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                lower_class_like_methods(
                    then_body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
                for (_, body) in elseif_clauses {
                    lower_class_like_methods(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
                if let Some(body) = else_body {
                    lower_class_like_methods(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                lower_class_like_methods(
                    then_body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
                if let Some(body) = else_body {
                    lower_class_like_methods(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                lower_class_like_methods(body, module, check_result, constants, fiber_return_sigs);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    lower_class_like_methods(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
                if let Some(body) = default {
                    lower_class_like_methods(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                lower_class_like_methods(
                    try_body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
                for catch in catches {
                    lower_class_like_methods(
                        &catch.body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
                if let Some(body) = finally_body {
                    lower_class_like_methods(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
            }
            _ => {}
        }
    }
}

/// Lowers all concrete methods for one class-like declaration.
fn lower_methods_for_class_like(
    class_name: &str,
    methods: &[ClassMethod],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    for method in methods {
        if !method.has_body {
            continue;
        }
        let method_key = php_method_key(&method.name);
        if class_method_already_lowered(module, class_name, &method_key, method.is_static) {
            continue;
        }
        function::lower_class_method(
            class_name,
            &method.name,
            method.is_static,
            &method.params,
            method.return_type.as_ref(),
            &method.body,
            module,
            check_result,
            constants,
            fiber_return_sigs,
        );
    }
}

/// Lowers the small builtin SPL method slice currently consumed by the EIR backend.
fn lower_referenced_builtin_spl_methods(
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
fn referenced_builtin_spl_methods(module: &Module) -> Vec<(String, String)> {
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

/// Returns true when generic `new $class` can emit static metadata for this class.
fn is_dynamic_new_mixed_metadata_candidate(class_name: &str) -> bool {
    if class_name.starts_with("__Elephc") {
        return false;
    }
    if supported_dynamic_new_builtin_class_name(class_name) {
        return true;
    }
    !known_dynamic_new_builtin_class_name(class_name)
}

/// Returns true for builtin classes with safe static allocation paths in generic dynamic new.
fn supported_dynamic_new_builtin_class_name(class_name: &str) -> bool {
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
fn known_dynamic_new_builtin_class_name(class_name: &str) -> bool {
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
fn push_supported_builtin_spl_method_for_receiver(
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
pub(super) fn class_data_name<'a>(
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
pub(super) fn dynamic_object_new_metadata_names<'a>(
    module: &'a Module,
    inst: &crate::ir::Instruction,
) -> Option<(&'a str, &'a str)> {
    class_data_name(module, inst)?.split_once('|')
}

/// Returns the string immediate attached to an instruction.
pub(super) fn string_data_name<'a>(
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
pub(super) fn php_method_key(method_name: &str) -> String {
    crate::names::php_symbol_key(method_name)
}

/// Adds builtin SPL methods required by runtime class/interface metadata.
fn push_builtin_spl_metadata_methods(
    methods: &mut Vec<(String, String)>,
    module: &Module,
    class_name: &str,
) {
    let mut current = Some(class_name);
    while let Some(name) = current {
        push_builtin_spl_interface_metadata_methods(methods, module, name);
        for method_name in required_builtin_spl_metadata_methods(name) {
            let method_key = php_method_key(method_name);
            if is_supported_builtin_spl_method(name, &method_key) {
                methods.push((name.to_string(), method_key));
            }
        }
        current = module
            .class_infos
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
}

/// Adds builtin SPL methods referenced by runtime interface dispatch tables for one class.
fn push_builtin_spl_interface_metadata_methods(
    methods: &mut Vec<(String, String)>,
    module: &Module,
    class_name: &str,
) {
    let Some(class_info) = module.class_infos.get(class_name) else {
        return;
    };
    let mut seen = HashSet::new();
    let mut stack = class_info
        .interfaces
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    while let Some(interface_name) = stack.pop() {
        if !seen.insert(interface_name.to_string()) {
            continue;
        }
        let Some(interface_info) = module.interface_infos.get(interface_name) else {
            continue;
        };
        for method_key in &interface_info.method_order {
            if let Some(impl_class) = class_info.method_impl_classes.get(method_key) {
                if is_supported_builtin_spl_method(impl_class, method_key) {
                    methods.push((impl_class.clone(), method_key.clone()));
                    continue;
                }
            }
            push_supported_builtin_spl_method_for_receiver(methods, module, class_name, method_key);
        }
        stack.extend(interface_info.parents.iter().map(String::as_str));
    }
}

/// Returns methods needed even when user code does not call them directly.
fn required_builtin_spl_metadata_methods(class_name: &str) -> &'static [&'static str] {
    match class_name {
        "EmptyIterator" => &["current", "key", "next", "rewind", "valid"],
        "ArrayIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "seek",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
            "count",
        ],
        "ArrayObject" => &[
            "getIterator",
            "count",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
        ],
        "SplFixedArray" => &[
            "getIterator",
            "count",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
            "jsonSerialize",
        ],
        "InternalIterator" => &["current", "key", "next", "rewind", "valid"],
        "IteratorIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
        "LimitIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "seek",
            "getPosition",
        ],
        "NoRewindIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
        "InfiniteIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
        "FilterIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
        "CallbackFilterIterator" => &["accept"],
        "CachingIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "hasNext",
            "__toString",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
            "getCache",
            "count",
        ],
        "AppendIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
        "MultipleIterator" => &["current", "key", "next", "rewind", "valid"],
        "__ElephcAppendIteratorArrayIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "seek",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
            "count",
        ],
        "SplDoublyLinkedList" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "count",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
        ],
        "SplHeap" => &["current", "key", "next", "rewind", "valid", "count"],
        "SplMaxHeap" | "SplMinHeap" => &["compare"],
        "SplPriorityQueue" => &["current", "key", "next", "rewind", "valid", "count"],
        "SplObjectStorage" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "count",
            "offsetExists",
            "offsetGet",
            "offsetSet",
            "offsetUnset",
        ],
        "RegexIterator" => &["accept", "current", "key"],
        "RecursiveArrayIterator" => &["hasChildren", "getChildren"],
        "RecursiveFilterIterator" => &["hasChildren"],
        "RecursiveCallbackFilterIterator" => &["hasChildren", "getChildren"],
        "RecursiveRegexIterator" => &["accept", "current", "key", "hasChildren", "getChildren"],
        "ParentIterator" => &["accept", "getChildren"],
        "RecursiveIteratorIterator" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "getInnerIterator",
        ],
        "SplFileInfo" => &["__toString"],
        "SplFileObject" => &[
            "current",
            "key",
            "next",
            "rewind",
            "valid",
            "seek",
            "hasChildren",
            "getChildren",
        ],
        "DirectoryIterator" => &["current", "key", "next", "rewind", "valid", "seek"],
        "FilesystemIterator" => &["current", "key"],
        "GlobIterator" => &["count"],
        "RecursiveDirectoryIterator" => &["hasChildren", "getChildren"],
        "RecursiveCachingIterator" => &["hasChildren", "getChildren"],
        _ => &[],
    }
}

/// Returns true for builtin SPL methods intentionally lowered into EIR today.
fn is_supported_builtin_spl_method(class_name: &str, method_key: &str) -> bool {
    match class_name {
        "SplFileInfo" => matches!(
            method_key,
            "__construct"
                | "__tostring"
                | "getpath"
                | "getfilename"
                | "getextension"
                | "getbasename"
                | "getpathname"
                | "getperms"
                | "getinode"
                | "getsize"
                | "getowner"
                | "getgroup"
                | "getatime"
                | "getmtime"
                | "getctime"
                | "gettype"
                | "iswritable"
                | "iswriteable"
                | "isreadable"
                | "isexecutable"
                | "isfile"
                | "isdir"
                | "islink"
                | "getlinktarget"
                | "getrealpath"
                | "getfileinfo"
                | "getpathinfo"
                | "setinfoclass"
                | "openfile"
                | "setfileclass"
        ),
        "SplFileObject" => matches!(
            method_key,
            "__construct"
                | "current"
                | "key"
                | "next"
                | "rewind"
                | "valid"
                | "seek"
                | "haschildren"
                | "getchildren"
                | "eof"
                | "fgets"
                | "getcurrentline"
                | "fgetc"
                | "fread"
                | "fwrite"
                | "ftruncate"
                | "ftell"
                | "fseek"
                | "getflags"
                | "setflags"
                | "getmaxlinelen"
                | "setmaxlinelen"
                | "setcsvcontrol"
                | "fgetcsv"
                | "fputcsv"
        ),
        "SplTempFileObject" => matches!(
            method_key,
            "__construct"
                | "eof"
                | "fgetc"
                | "fflush"
                | "fgets"
                | "fread"
                | "fwrite"
                | "fstat"
                | "ftell"
                | "fseek"
                | "ftruncate"
                | "rewind"
                | "__elephcspilltofile"
        ),
        "DirectoryIterator" => matches!(
            method_key,
            "__construct"
                | "current"
                | "key"
                | "next"
                | "rewind"
                | "seek"
                | "valid"
                | "isdot"
                | "__tostring"
                | "__elephcrefreshpath"
        ),
        "FilesystemIterator" => matches!(
            method_key,
            "__construct" | "current" | "key" | "getflags" | "setflags"
        ),
        "GlobIterator" => matches!(method_key, "__construct" | "count" | "setflags"),
        "RecursiveDirectoryIterator" => {
            matches!(method_key, "__construct" | "haschildren" | "getchildren")
        }
        "RecursiveCachingIterator" => matches!(
            method_key,
            "__construct" | "haschildren" | "getchildren" | "__elephcassumerecursiveiterator"
        ),
        "EmptyIterator" => matches!(method_key, "current" | "key" | "next" | "rewind" | "valid"),
        "ArrayIterator" => matches!(
            method_key,
            "__construct"
                | "current"
                | "key"
                | "next"
                | "rewind"
                | "valid"
                | "seek"
                | "count"
                | "offsetexists"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "append"
                | "getarraycopy"
        ),
        "ArrayObject" => matches!(
            method_key,
            "__construct"
                | "getiterator"
                | "count"
                | "offsetexists"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "append"
                | "getarraycopy"
        ),
        "SplFixedArray" => matches!(
            method_key,
            "__construct"
                | "__wakeup"
                | "__serialize"
                | "__unserialize"
                | "count"
                | "getiterator"
                | "toarray"
                | "getsize"
                | "setsize"
                | "offsetexists"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "jsonserialize"
        ),
        "InternalIterator" => matches!(
            method_key,
            "__construct" | "current" | "key" | "next" | "rewind" | "valid"
        ),
        "SplDoublyLinkedList" | "SplStack" | "SplQueue" => matches!(
            method_key,
            "add"
                | "pop"
                | "shift"
                | "push"
                | "unshift"
                | "top"
                | "bottom"
                | "count"
                | "isempty"
                | "setiteratormode"
                | "getiteratormode"
                | "offsetexists"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "rewind"
                | "current"
                | "key"
                | "prev"
                | "next"
                | "valid"
                | "serialize"
                | "unserialize"
                | "__serialize"
                | "__unserialize"
                | "__debuginfo"
                | "enqueue"
                | "dequeue"
        ),
        "SplHeap" => matches!(
            method_key,
            "__construct"
                | "insert"
                | "extract"
                | "top"
                | "count"
                | "isempty"
                | "rewind"
                | "current"
                | "key"
                | "next"
                | "valid"
                | "recoverfromcorruption"
                | "iscorrupted"
                | "__debuginfo"
                | "compare"
                | "__elephcbestindex"
                | "__elephcremoveat"
        ),
        "SplMaxHeap" | "SplMinHeap" => matches!(method_key, "compare"),
        "SplPriorityQueue" => matches!(
            method_key,
            "__construct"
                | "compare"
                | "insert"
                | "setextractflags"
                | "getextractflags"
                | "extract"
                | "top"
                | "count"
                | "isempty"
                | "rewind"
                | "current"
                | "key"
                | "next"
                | "valid"
                | "recoverfromcorruption"
                | "iscorrupted"
                | "__debuginfo"
                | "__elephcbestindex"
                | "__elephcoutputat"
                | "__elephcremoveat"
        ),
        "SplObjectStorage" => matches!(
            method_key,
            "__construct"
                | "attach"
                | "detach"
                | "contains"
                | "addall"
                | "removeall"
                | "removeallexcept"
                | "getinfo"
                | "setinfo"
                | "count"
                | "rewind"
                | "valid"
                | "key"
                | "current"
                | "next"
                | "seek"
                | "offsetexists"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "gethash"
                | "serialize"
                | "unserialize"
                | "__serialize"
                | "__unserialize"
                | "__debuginfo"
                | "__elephcindexof"
        ),
        "Phar" | "PharData" => matches!(
            method_key,
            "__construct"
                | "offsetexists"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "addfromstring"
                | "__tostring"
                | "getpath"
                | "getpathname"
                | "getfilename"
                | "setmetadata"
                | "getmetadata"
                | "hasmetadata"
                | "delmetadata"
                | "setstub"
                | "getstub"
                | "rewind"
                | "next"
                | "valid"
                | "key"
                | "current"
                | "count"
                | "compressfiles"
                | "decompressfiles"
                | "compress"
                | "decompress"
                | "setsignaturealgorithm"
                | "getsignature"
                | "setzippassword"
                | "delete"
        ),
        "PharFileInfo" => matches!(
            method_key,
            "__construct"
                | "getcontent"
                | "setmetadata"
                | "getmetadata"
                | "hasmetadata"
                | "delmetadata"
                | "__tostring"
                | "getpath"
                | "getfilename"
                | "getextension"
                | "getbasename"
                | "getpathname"
                | "getperms"
                | "getinode"
                | "getsize"
                | "getowner"
                | "getgroup"
                | "getatime"
                | "getmtime"
                | "getctime"
                | "gettype"
                | "iswritable"
                | "iswriteable"
                | "isreadable"
                | "isexecutable"
                | "isfile"
                | "isdir"
                | "islink"
                | "getlinktarget"
                | "getrealpath"
        ),
        "IteratorIterator" => matches!(
            method_key,
            "current" | "key" | "next" | "rewind" | "valid" | "getinneriterator"
        ),
        "LimitIterator" => matches!(
            method_key,
            "__construct" | "rewind" | "next" | "valid" | "seek" | "getposition"
        ),
        "NoRewindIterator" => matches!(method_key, "__construct" | "rewind"),
        "InfiniteIterator" => matches!(method_key, "__construct" | "next"),
        "FilterIterator" => matches!(method_key, "__construct" | "rewind" | "next"),
        "CallbackFilterIterator" => matches!(method_key, "accept" | "__elephcsetcallbackenv"),
        "CachingIterator" => matches!(
            method_key,
            "__construct"
                | "rewind"
                | "valid"
                | "next"
                | "current"
                | "key"
                | "hasnext"
                | "__tostring"
                | "getflags"
                | "setflags"
                | "offsetget"
                | "offsetset"
                | "offsetunset"
                | "offsetexists"
                | "getcache"
                | "count"
                | "__elephccapturecurrent"
        ),
        "AppendIterator" => matches!(
            method_key,
            "__construct"
                | "append"
                | "rewind"
                | "valid"
                | "current"
                | "key"
                | "next"
                | "getinneriterator"
                | "getiteratorindex"
                | "getarrayiterator"
                | "__elephcstoragecount"
                | "__elephcstoragephysicalcount"
                | "__elephcstorageisactive"
                | "__elephcstorageappend"
                | "__elephcstorageoffsetset"
                | "__elephcstorageoffsetexists"
                | "__elephcstorageoffsetget"
                | "__elephcstorageoffsetunset"
                | "__elephcstoragegetarraycopy"
                | "__elephcstoragekey"
                | "__elephcstoragecurrent"
        ),
        "MultipleIterator" => matches!(
            method_key,
            "__construct"
                | "getflags"
                | "setflags"
                | "attachiterator"
                | "detachiterator"
                | "containsiterator"
                | "countiterators"
                | "rewind"
                | "valid"
                | "key"
                | "current"
                | "next"
        ),
        "RegexIterator" | "RecursiveRegexIterator" => matches!(
            method_key,
            "__construct"
                | "accept"
                | "current"
                | "key"
                | "getmode"
                | "setmode"
                | "getflags"
                | "setflags"
                | "getregex"
                | "getpregflags"
                | "setpregflags"
                | "__elephcregextarget"
                | "__elephcfirstmatch"
                | "__elephcallmatches"
                | "__elephcsplit"
                | "haschildren"
                | "getchildren"
                | "__elephcassumerecursiveiterator"
        ),
        "RecursiveArrayIterator" => matches!(
            method_key,
            "__construct" | "haschildren" | "getchildren" | "__elephcassumerecursiveiterator"
        ),
        "RecursiveFilterIterator" => matches!(
            method_key,
            "__construct" | "haschildren" | "getchildren" | "__elephcassumerecursiveiterator"
        ),
        "RecursiveCallbackFilterIterator" => matches!(
            method_key,
            "__construct" | "haschildren" | "getchildren" | "__elephcassumerecursiveiterator"
        ),
        "ParentIterator" => matches!(
            method_key,
            "__construct" | "accept" | "getchildren" | "__elephcassumerecursiveiterator"
        ),
        "RecursiveIteratorIterator" => matches!(
            method_key,
            "__construct"
                | "rewind"
                | "valid"
                | "current"
                | "key"
                | "next"
                | "getdepth"
                | "getinneriterator"
                | "getsubiterator"
                | "__elephcadvance"
                | "__elephcslotfordepth"
                | "__elephcassumerecursiveiterator"
        ),
        "__ElephcAppendIteratorArrayIterator" => matches!(
            method_key,
            "__construct"
                | "count"
                | "append"
                | "offsetset"
                | "offsetexists"
                | "offsetget"
                | "offsetunset"
                | "getarraycopy"
                | "rewind"
                | "next"
                | "valid"
                | "key"
                | "current"
        ),
        _ => false,
    }
}

/// Returns true when this SPL method is implemented by an intrinsic runtime wrapper.
fn runtime_intrinsic_method_has_wrapper(
    class_name: &str,
    method_key: &str,
    is_static: bool,
) -> bool {
    let intrinsic = if is_static {
        IntrinsicCall::static_method(class_name, method_key)
    } else {
        IntrinsicCall::instance_method(class_name, method_key)
    };
    intrinsic.is_some_and(|intrinsic| intrinsic.runtime_helper().is_some())
}

/// Lowers one supported builtin SPL method body if it has not already been emitted.
fn lower_builtin_spl_method(
    class_name: &str,
    method_key: &str,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    if class_method_already_lowered(module, class_name, method_key, false)
        || !is_supported_builtin_spl_method(class_name, method_key)
        || runtime_intrinsic_method_has_wrapper(class_name, method_key, false)
    {
        return;
    }
    let Some(class_info) = check_result.classes.get(class_name) else {
        return;
    };
    let Some(method) = class_info
        .method_decls
        .iter()
        .find(|method| php_method_key(&method.name) == method_key && method.has_body)
    else {
        return;
    };
    function::lower_class_method(
        class_name,
        &method.name,
        method.is_static,
        &method.params,
        method.return_type.as_ref(),
        &method.body,
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
}

/// Returns true when `module.class_methods` already contains a class-method body.
pub(super) fn class_method_already_lowered(
    module: &Module,
    class_name: &str,
    method_key: &str,
    is_static: bool,
) -> bool {
    module.class_methods.iter().any(|function| {
        function.flags.is_static == is_static
            && function
                .name
                .rsplit_once("::")
                .is_some_and(|(candidate_class, candidate_method)| {
                    candidate_class == class_name && php_method_key(candidate_method) == method_key
                })
    })
}
