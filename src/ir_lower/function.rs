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
    Builder, Function, FunctionFlags, FunctionParam, GeneratorSource, Immediate, IrType, Module,
    Op, Ownership, Terminator,
};
use crate::ir_lower::context::{
    return_ir_type, type_expr_to_php_type, value_ir_type, ClosureCapture, LoweringContext,
    StaticCallableBinding,
};
use crate::ir_lower::effects_lookup;
use crate::names::php_symbol_key;
use crate::parser::ast::{
    AttributeGroup, BinOp, CastType, ClassMethod, Expr, ExprKind, Program, StaticReceiver, Stmt,
    StmtKind, TypeExpr,
};
use crate::names::Name;
use crate::span::Span;
use crate::types::{
    collect_attribute_args, collect_attribute_names, CheckResult, ClassInfo, FunctionSig,
    PackedClassInfo, PhpType, TypeEnv,
};

/// AST parameter tuple shape used by function, method, and closure declarations.
type AstParams = [(
    String,
    Option<TypeExpr>,
    Option<crate::parser::ast::Expr>,
    bool,
)];

const EVAL_AOT_SCOPE_PARAM: &str = "__eir_eval_scope";

const CALLED_CLASS_ID_PARAM: &str = "__elephc_called_class_id";

/// Compile-time callable binding to seed for a self-recursive closure capture.
struct RecursiveClosureBinding {
    local_name: String,
    closure_name: String,
    signature: FunctionSig,
    capture_names: Vec<String>,
}

/// Lowers the top-level statement list as the synthetic `main` EIR function.
pub(crate) fn lower_main(
    program: &Program,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, FunctionSig>,
) {
    let web = module.web;
    let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
    function.flags.is_main = true;
    let all_global_var_names = collect_global_var_names(program);
    let top_level_env = web_gated_global_env(&check_result.global_env, web);
    let closures = lower_body_into_function(
        &mut function,
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
        &check_result.packed_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        &check_result.loop_storage_types,
        &check_result.string_incdec_locals,
        &check_result.local_bind_kill_sites,
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

/// Collects PHP variable names that any function-like STATEMENT body declares with `global`.
///
/// Shared with the CHECKER (`crate::global_decls`), which vetoes ending a top-level binding of one
/// of these names for the same reason lowering refuses to abandon its slot: `global $x;` in some
/// other body reaches the very storage the top-level name uses. One walk keeps the two sides
/// identical, blind spots included — and the blind spots are load-bearing here. This set decides
/// STORAGE CLASS: a name in it moves out of main's frame slot into the `_eir_global_*` symbol,
/// which types it `Mixed`, and the array builtins have pre-existing `Mixed`-array backend gaps, so
/// widening the walk to closure bodies or enum methods broke programs that compile and print PHP's
/// output today (`implode` crashed, `array_sum`/`sort`/`in_array` and friends became a hard backend
/// error). `crate::global_decls`' preamble carries those measurements, the matching reason the
/// checker's veto must not be widened on its own either, and the pre-existing closure-`global`
/// write loss both sides preserve.
fn collect_global_var_names(statements: &[Stmt]) -> std::collections::HashSet<String> {
    crate::global_decls::collect_global_var_names(statements)
}

/// Lowers one user-defined function declaration into an EIR function.
pub(crate) fn lower_user_function(
    name: &str,
    params: &AstParams,
    return_type: Option<&TypeExpr>,
    attributes: &[AttributeGroup],
    body: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, FunctionSig>,
) {
    let web = module.web;
    let fallback = signature_from_ast(params, return_type);
    let signature = check_result.functions.get(name).unwrap_or(&fallback);
    let eir_signature =
        eir_signature_with_php_param_contracts(name, signature, &check_result.callable_param_sigs);
    // A generator's compiled body is a coroutine that returns the value passed
    // to `return` (Mixed, read back via `Generator::getReturn()`), not the
    // `Generator` object itself. The public signature stays `Generator` for
    // callers; only the EIR body return type becomes Mixed so `return $x`
    // lowers to a plain boxed Mixed return instead of a Generator coercion.
    let body_return_type = generator_body_return_type(body, &eir_signature.return_type);
    let mut function = Function::new(
        name.to_string(),
        return_ir_type(&body_return_type),
        body_return_type.clone(),
    );
    function.params = function_params(&eir_signature);
    function.flags.by_ref_return = signature.by_ref_return;
    function.source_signature = Some(source_signature(name, &eir_signature));
    function.signature = Some(eir_runtime_metadata_signature(&eir_signature));
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
    attach_generator_source_if_needed(&mut function, body, eir_signature.params.len());
    let closures = lower_body_into_function(
        &mut function,
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
        &check_result.packed_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        &check_result.loop_storage_types,
        &check_result.string_incdec_locals,
        &check_result.local_bind_kill_sites,
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
        std::collections::HashSet::new(),
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
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, FunctionSig>,
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
    let method_body_return_type = generator_body_return_type(body, &signature.return_type);
    let mut function = Function::new(
        name.clone(),
        return_ir_type(&method_body_return_type),
        method_body_return_type.clone(),
    );
    function.lexical_class = Some(class_name.to_string());
    function.flags = FunctionFlags {
        is_method: true,
        is_static,
        by_ref_return: signature.by_ref_return,
        ..FunctionFlags::default()
    };
    function.source_signature = Some(source_signature(&name, &signature));
    function.signature = Some(eir_runtime_metadata_signature(&signature));
    let mut env = env_from_signature(&signature, web);
    let mut body_params = signature.params.clone();
    if is_static {
        let hidden_called_class = (CALLED_CLASS_ID_PARAM.to_string(), PhpType::Int);
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
    attach_generator_source_if_needed(&mut function, body, body_params.len());
    let closures = lower_body_into_function(
        &mut function,
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
        &check_result.packed_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        &check_result.loop_storage_types,
        &check_result.string_incdec_locals,
        &check_result.local_bind_kill_sites,
        &check_result.local_retype_sites,
        &check_result.mixed_storage_store_sites,
        name.clone(),
        constants,
        Some(class_name.to_string()),
        method_body_return_type.clone(),
        signature.declared_return,
        &body_params,
        None,
        false,
        std::collections::HashSet::new(),
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
    std::collections::HashMap<Span, std::collections::HashSet<String>>,
    std::collections::HashMap<Span, std::collections::HashSet<String>>,
    std::collections::HashMap<Span, std::collections::HashSet<String>>,
) {
    (
        std::collections::HashMap::new(),
        std::collections::HashMap::new(),
        std::collections::HashMap::new(),
    )
}

/// Lowers one no-scope literal eval fragment as an internal EIR function.
pub(crate) fn lower_eval_aot_function(
    name: &str,
    body: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, FunctionSig>,
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
    };
    let mut function = Function::new(
        name.to_string(),
        return_ir_type(&return_type),
        return_type.clone(),
    );
    function.source_signature = Some(source_signature(name, &signature));
    function.signature = Some(eir_runtime_metadata_signature(&signature));
    let (bind_kill_sites, retype_sites, mixed_storage_store_sites) = eval_aot_decision_maps();
    let closures = lower_body_into_function(
        &mut function,
        &mut module.data,
        body,
        TypeEnv::new(),
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
        &check_result.packed_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        &check_result.loop_storage_types,
        &check_result.string_incdec_locals,
        &bind_kill_sites,
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
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, FunctionSig>,
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
    };
    let mut function = Function::new(
        name.to_string(),
        return_ir_type(&return_type),
        return_type.clone(),
    );
    function.params = function_params(&signature);
    function.source_signature = Some(source_signature(name, &signature));
    function.signature = Some(eir_runtime_metadata_signature(&signature));
    let mut env = TypeEnv::new();
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
    let (bind_kill_sites, retype_sites, mixed_storage_store_sites) = eval_aot_decision_maps();
    let closures = lower_body_into_function(
        &mut function,
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
        &check_result.packed_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        &check_result.loop_storage_types,
        &check_result.string_incdec_locals,
        &bind_kill_sites,
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
        module.source_path.clone(),
        eval_scope_reads,
        module.web,
    );
    add_closures(module, closures);
    module.add_function(function);
}

/// Builds fallback method signature metadata from parsed class-like method syntax.
pub(crate) fn method_signature_from_ast(method: &ClassMethod) -> FunctionSig {
    let mut signature = signature_from_ast_with_variadic(
        &method.params,
        method.return_type.as_ref(),
        method.variadic.as_deref(),
        method.variadic_by_ref,
    );
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
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, FunctionSig>,
) {
    if !class_info.defaults.iter().any(|default| default.is_some()) {
        return;
    }
    let web = module.web;
    let body = property_init_body(class_info);
    let function_name = format!("_class_propinit_{}", class_info.class_id);
    let this_type = PhpType::Object(class_name.to_string());
    let mut function = Function::new(function_name.clone(), IrType::Void, PhpType::Void);
    function.flags.is_synthetic = true;
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
    };
    function.source_signature = Some(source_signature(&function_name, &sig));
    function.signature = Some(eir_runtime_metadata_signature(&sig));
    let mut env = TypeEnv::new();
    env.insert("this".to_string(), this_type.clone());
    let params = vec![("this".to_string(), this_type)];
    let closures = lower_body_into_function(
        &mut function,
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
        &check_result.packed_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        &check_result.loop_storage_types,
        &check_result.string_incdec_locals,
        &check_result.local_bind_kill_sites,
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
        std::collections::HashSet::new(),
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
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, FunctionSig>,
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
    };
    function.source_signature = Some(source_signature(&function_name, &sig));
    function.signature = Some(eir_runtime_metadata_signature(&sig));
    let mut env = TypeEnv::new();
    for (name, php_type) in &params {
        env.insert(name.clone(), php_type.clone());
    }
    let web = module.web;
    let closures = lower_body_into_function(
        &mut function,
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
        &check_result.packed_classes,
        &check_result.throw_access_sites,
        &check_result.builtin_call_types,
        &check_result.loop_storage_types,
        &check_result.string_incdec_locals,
        &check_result.local_bind_kill_sites,
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
        std::collections::HashSet::new(),
        module.source_path.clone(),
        None,
        web,
    );
    add_closures(module, closures);
    module.add_function(function);
}

/// The symbol a dynamic-new candidate calls when it has to pad the constructor with defaults.
pub(crate) fn dynamic_constructor_thunk_name(class_id: u64, provided_args: usize) -> String {
    format!("_class_ctor_{}_{}", class_id, provided_args)
}

/// Builds `$this->property = <default>;` statements for property-default initialization.
///
/// A null default whose slot type cannot represent null (a scalar slot rebound by
/// constructor-argument propagation) is skipped: those slots are always overwritten
/// before an observable read, and the store would be unrepresentable.
fn property_init_body(class_info: &ClassInfo) -> Vec<Stmt> {
    let span = Span::dummy();
    class_info
        .defaults
        .iter()
        .enumerate()
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
    self_ref_callable_capture: Option<&str>,
    by_ref_return: bool,
    loop_storage_scope: String,
) -> FunctionSig {
    let mut signature = closure_signature_from_ast(
        params,
        variadic,
        variadic_by_ref,
        return_type,
        body,
        captures,
        parent.classes,
    );
    signature.by_ref_return = by_ref_return;
    lower_closure_function_with_signature(
        parent,
        name,
        signature,
        body,
        captures,
        self_ref_callable_capture,
        loop_storage_scope,
    )
}

/// Lowers one closure literal using contextual types for unannotated parameters.
pub(crate) fn lower_closure_function_with_context(
    parent: &mut LoweringContext<'_, '_>,
    name: &str,
    params: &AstParams,
    variadic: Option<&str>,
    variadic_by_ref: bool,
    return_type: Option<&TypeExpr>,
    body: &[Stmt],
    captures: &[(String, PhpType, bool)],
    contextual_arg_types: &[PhpType],
    self_ref_callable_capture: Option<&str>,
    by_ref_return: bool,
    loop_storage_scope: String,
) -> FunctionSig {
    let mut signature = closure_signature_from_ast(
        params,
        variadic,
        variadic_by_ref,
        return_type,
        body,
        captures,
        parent.classes,
    );
    signature.by_ref_return = by_ref_return;
    for (idx, (_, type_ann, _, _)) in params.iter().enumerate() {
        if type_ann.is_none() {
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
        signature,
        body,
        captures,
        self_ref_callable_capture,
        loop_storage_scope,
    )
}

/// Lowers one closure function from an already-built signature.
fn lower_closure_function_with_signature(
    parent: &mut LoweringContext<'_, '_>,
    name: &str,
    signature: FunctionSig,
    body: &[Stmt],
    captures: &[(String, PhpType, bool)],
    self_ref_callable_capture: Option<&str>,
    loop_storage_scope: String,
) -> FunctionSig {
    // Generator closures lower their body as a Mixed-returning coroutine; see
    // `generator_body_return_type`.
    let closure_body_return_type = generator_body_return_type(body, &signature.return_type);
    let mut function = Function::new(
        name.to_string(),
        return_ir_type(&closure_body_return_type),
        closure_body_return_type.clone(),
    );
    function.lexical_class = parent.current_class.clone();
    function.flags = FunctionFlags {
        is_closure: true,
        by_ref_return: signature.by_ref_return,
        ..FunctionFlags::default()
    };
    function.params = function_params(&signature);
    function.params.extend(closure_capture_params(captures));
    function.source_signature = Some(source_signature(name, &signature));
    function.signature = Some(eir_runtime_metadata_signature(&signature));
    attach_generator_source_if_needed(&mut function, body, signature.params.len());
    let env = env_with_closure_captures(&signature, captures, parent.web);
    let lowered_params = params_with_closure_captures(&signature, captures);
    let recursive_binding = self_ref_callable_capture.map(|local_name| RecursiveClosureBinding {
        local_name: local_name.to_string(),
        closure_name: name.to_string(),
        signature: signature.clone(),
        capture_names: captures
            .iter()
            .map(|(capture_name, _, _)| capture_name.clone())
            .collect(),
    });
    let closures = lower_body_into_function(
        &mut function,
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
        parent.packed_classes,
        parent.throw_access_sites,
        parent.builtin_call_types,
        parent.loop_storage_types,
        parent.string_incdec_locals,
        parent.bind_kill_sites,
        parent.retype_sites,
        parent.mixed_storage_store_sites,
        loop_storage_scope,
        &parent.constants,
        parent.current_class.clone(),
        closure_body_return_type.clone(),
        signature.declared_return,
        &lowered_params,
        recursive_binding,
        false,
        collect_global_var_names(body),
        parent.source_path().map(str::to_string),
        None,
        parent.web,
    );
    parent.extend_closures(std::iter::once(function).chain(closures));
    signature
}

/// Lowers the supplied statements into `function` and appends a default terminator if needed.
fn lower_body_into_function(
    function: &mut Function,
    data: &mut crate::ir::DataPool,
    body: &[Stmt],
    env: TypeEnv,
    top_level_env: TypeEnv,
    functions: &std::collections::HashMap<String, FunctionSig>,
    extern_functions: &std::collections::HashMap<String, crate::types::ExternFunctionSig>,
    extern_globals: &std::collections::HashMap<String, PhpType>,
    callable_param_sigs: &std::collections::HashMap<(String, String), FunctionSig>,
    return_alias_summaries: &crate::types::ReturnAliasSummaries,
    fiber_return_sigs: &std::collections::HashMap<String, FunctionSig>,
    classes: &std::collections::HashMap<String, crate::types::ClassInfo>,
    enums: &std::collections::HashMap<String, crate::types::EnumInfo>,
    interfaces: &std::collections::HashMap<String, crate::types::InterfaceInfo>,
    packed_classes: &std::collections::HashMap<String, PackedClassInfo>,
    throw_access_sites: &std::collections::HashMap<Span, crate::types::ThrowAccessInfo>,
    builtin_call_types: &std::collections::HashMap<Span, PhpType>,
    loop_storage_types: &crate::types::LoopStorageTypes,
    string_incdec_locals: &std::collections::HashSet<(String, String)>,
    bind_kill_sites: &std::collections::HashMap<Span, std::collections::HashSet<String>>,
    retype_sites: &std::collections::HashMap<Span, std::collections::HashSet<String>>,
    mixed_storage_store_sites: &std::collections::HashMap<
        Span,
        std::collections::HashSet<String>,
    >,
    loop_storage_scope: String,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    current_class: Option<String>,
    return_php_type: PhpType,
    return_type_is_declared: bool,
    params: &[(String, PhpType)],
    recursive_closure_binding: Option<RecursiveClosureBinding>,
    in_main: bool,
    all_global_var_names: std::collections::HashSet<String>,
    source_path: Option<String>,
    eval_scope_reads: Option<(
        String,
        std::collections::HashSet<String>,
        std::collections::HashSet<String>,
        std::collections::BTreeSet<String>,
    )>,
    web: bool,
) -> Vec<Function> {
    let owner_name = function.name.clone();
    let function_by_ref_return = function.flags.by_ref_return;
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
        packed_classes,
        throw_access_sites,
        builtin_call_types,
        loop_storage_types,
        string_incdec_locals,
        bind_kill_sites,
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
    ctx.by_ref_return = function_by_ref_return;
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
    // depends on. This one site is the single funnel for free functions, methods, static methods
    // and closures, so every call flavour — including `call_user_func`, dynamic `$f(...)` and
    // recursion — is covered without any per-flavour code.
    //
    // By-reference parameters are excluded by definition: `array &$a` must alias, not copy.
    // `$this` is excluded because it is an object, never a container.
    for (index, (name, php_type)) in params.iter().enumerate() {
        if by_ref_params.get(index).copied().unwrap_or(false) {
            continue;
        }
        if name == "this" {
            continue;
        }
        if !matches!(
            php_type.codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. }
        ) {
            continue;
        }
        ctx.privatize_container_param(name, php_type, None);
    }
    seed_recursive_closure_binding(&mut ctx, recursive_closure_binding);
    for stmt in body {
        crate::ir_lower::stmt::lower_stmt(&mut ctx, stmt);
    }
    terminate_open_block(&mut ctx);
    // Final storage types are now known: erase deferred loop-store releases that
    // guard slots which never widened to lifetime-tracked storage (issue #534).
    ctx.builder.prune_untracked_release_local_slot_ops();
    // Likewise, erase provisional releases for concrete local loads unless a
    // later store widened their final frame slot to Mixed (issue #538).
    ctx.builder.prune_borrowed_local_load_release_ops();
    // Publish the lowering-time ownership proof after provisional local-load
    // releases have been pruned, so codegen can consume EIR metadata instead of
    // maintaining a second producer allow-list (issue #595).
    ctx.finalize_value_ownership_metadata();
    ctx.into_closures()
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
        .capture_names
        .iter()
        .map(|capture_name| ClosureCapture {
            value: ctx.load_local(capture_name, None).value,
        })
        .collect();
    ctx.bind_static_callable_local(
        &binding.local_name,
        StaticCallableBinding::Closure {
            name: binding.closure_name,
            signature: binding.signature,
            captures,
        },
    );
}

/// Appends lowered closure functions to the module with stable closure-table ids.
fn add_closures(module: &mut Module, closures: Vec<Function>) {
    for closure in closures {
        module.add_closure(closure);
    }
}

/// Retains generator source metadata until the EIR backend has native generator-state lowering.
fn attach_generator_source_if_needed(
    function: &mut Function,
    body: &[Stmt],
    visible_param_count: usize,
) {
    if !crate::types::checker::yield_validation::body_contains_yield(body)
        && !is_generator_return_type(&function.return_php_type)
    {
        return;
    }
    function.flags.is_generator = true;
    function.generator_source = Some(GeneratorSource {
        body: body.to_vec(),
        visible_param_count,
    });
}

/// Returns true when checked function metadata already identifies a generator return.
fn is_generator_return_type(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Object(name) if name.trim_start_matches('\\') == "Generator")
}

/// Returns the EIR return type to lower a function body with.
///
/// For a generator (body contains `yield`, or the declared return type is
/// `Generator`) the compiled body is a coroutine whose `return` produces the
/// value later read by `Generator::getReturn()`, so the body return type is
/// `Mixed`. For every other function it is the declared signature return type.
fn generator_body_return_type(body: &[Stmt], signature_return: &PhpType) -> PhpType {
    if crate::types::checker::yield_validation::body_contains_yield(body)
        || is_generator_return_type(signature_return)
    {
        PhpType::Mixed
    } else {
        signature_return.clone()
    }
}

/// Adds a default function terminator when the current block can still fall through.
fn terminate_open_block(ctx: &mut LoweringContext<'_, '_>) {
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    if matches!(ctx.return_php_type, PhpType::Never) {
        let message = ctx
            .intern_string("Fatal error: A never-returning function must not implicitly return\n");
        ctx.builder.terminate(Terminator::Fatal { message });
        return;
    }
    if ctx.return_type == IrType::Void {
        ctx.emit_eval_scope_finalizer(None);
        ctx.builder.terminate(Terminator::Return { value: None });
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

/// Returns an EIR ABI signature that keeps dynamic untyped PHP parameters boxed.
pub(crate) fn eir_signature_with_php_param_contracts(
    owner_name: &str,
    signature: &FunctionSig,
    callable_param_sigs: &std::collections::HashMap<(String, String), FunctionSig>,
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
        eir_signature.return_type = dynamic_param_container_return_type(&eir_signature.return_type);
    }
    eir_signature
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
    callable_param_sigs: &std::collections::HashMap<(String, String), FunctionSig>,
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

/// Widens inferred container return elements that may be built from dynamic params.
fn dynamic_param_container_return_type(return_type: &PhpType) -> PhpType {
    match return_type.codegen_repr() {
        PhpType::Array(_) => PhpType::Array(Box::new(PhpType::Mixed)),
        PhpType::AssocArray { key, .. } => PhpType::AssocArray {
            key,
            value: Box::new(PhpType::Mixed),
        },
        PhpType::Union(members) => PhpType::Union(
            members
                .iter()
                .map(dynamic_param_container_return_type)
                .collect(),
        ),
        other => other,
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
    classes: &std::collections::HashMap<String, crate::types::ClassInfo>,
) -> FunctionSig {
    let mut signature =
        signature_from_ast_with_variadic(params, return_type, variadic, variadic_by_ref);
    if crate::types::checker::yield_validation::body_contains_yield(body) {
        signature.return_type = PhpType::Object("Generator".to_string());
        return signature;
    }
    if return_type.is_none() {
        if let Some(return_ty) =
            direct_closure_return_type(body, captures, &signature.params, classes)
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
    classes: &std::collections::HashMap<String, crate::types::ClassInfo>,
) -> Option<PhpType> {
    let [stmt] = body else {
        return None;
    };
    let StmtKind::Return(Some(expr)) = &stmt.kind else {
        return None;
    };
    Some(direct_closure_return_expr_type(expr, captures, params, classes))
}

/// Returns a direct closure return expression type, consulting capture and parameter
/// metadata first. A bare `return $x` where `$x` is a parameter must adopt the parameter's
/// declared type (e.g. `mixed`) rather than falling back to the syntactic integer default,
/// which would otherwise coerce a boxed Mixed argument to an integer on return. A
/// `return $obj->prop` where `$obj` is a captured/parameter object of a known class adopts
/// the property's declared type, so a `fn &() => $o->items` closure returns the array type
/// rather than the syntactic integer default. An array literal built out of those same
/// variables resolves its element/value slots the same way (see
/// `direct_closure_return_array_element_type`).
fn direct_closure_return_expr_type(
    expr: &crate::parser::ast::Expr,
    captures: &[(String, PhpType, bool)],
    params: &[(String, PhpType)],
    classes: &std::collections::HashMap<String, crate::types::ClassInfo>,
) -> PhpType {
    // An array literal returned directly is stamped with this inferred type and its elements
    // are coerced into it by `lower_return_expr`, so its slots must be resolved against the
    // closure signature instead of the syntactic integer default.
    if let ExprKind::ArrayLiteral(items) = &expr.kind {
        if !items.is_empty() {
            return PhpType::Array(Box::new(direct_closure_return_array_element_type(
                items, captures, params, classes,
            )));
        }
    }
    if let ExprKind::ArrayLiteralAssoc(pairs) = &expr.kind {
        if !pairs.is_empty() {
            return direct_closure_return_assoc_literal_type(pairs, captures, params, classes);
        }
    }
    if let ExprKind::ScopedConstantAccess {
        receiver: crate::parser::ast::StaticReceiver::Named(class_name),
        name,
    } = &expr.kind
    {
        let normalized = class_name.as_str().trim_start_matches('\\');
        if let Some(value) = classes
            .get(normalized)
            .and_then(|class_info| class_info.constants.get(name))
        {
            return crate::types::checker::infer_expr_type_syntactic(value);
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
            if let Some(PhpType::Object(class)) = receiver_ty {
                if let Some(info) = classes.get(class.trim_start_matches('\\')) {
                    if let Some((_, ty)) =
                        info.properties.iter().find(|(name, _)| name == property)
                    {
                        return ty.clone();
                    }
                }
            }
        }
    }
    if let ExprKind::MethodCall { object, method, .. } = &expr.kind {
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
            if let Some(PhpType::Object(class)) = receiver_ty {
                if let Some(signature) = classes
                    .get(class.trim_start_matches('\\'))
                    .and_then(|info| method_signature(info, method, false))
                {
                    return signature.return_type.clone();
                }
            }
        }
    }
    if let ExprKind::StaticMethodCall {
        receiver: StaticReceiver::Named(class),
        method,
        ..
    } = &expr.kind
    {
        if let Some(signature) = classes
            .get(class.as_str().trim_start_matches('\\'))
            .and_then(|info| method_signature(info, method, true))
        {
            return signature.return_type.clone();
        }
    }
    crate::types::checker::infer_expr_type_syntactic(expr)
}

/// Returns the EIR storage element type for an indexed array literal returned directly
/// from a closure, resolving every item against the closure's captures and parameters.
///
/// This mirrors `crate::ir_lower::expr::array_literal_type_for_ir`, which types the very
/// same literal while lowering the body from `LoweringContext::local_types`. The two must
/// agree: `lower_return_expr` feeds the inferred return element type back into
/// `lower_array_literal_with_expected_type`, so a slot typed `int` here casts a boxed
/// `Mixed` argument to an integer on the way into the array — `function (mixed $a, mixed $b)
/// { return [$a, $b]; }` called as `(1, "z")` produced `[1, 0]`. The syntactic fallback used
/// before this helper existed types every unrecognized item `int`, which also mis-stamped
/// `string`, `float`, `bool`, and `array` parameters.
fn direct_closure_return_array_element_type(
    items: &[crate::parser::ast::Expr],
    captures: &[(String, PhpType, bool)],
    params: &[(String, PhpType)],
    classes: &std::collections::HashMap<String, crate::types::ClassInfo>,
) -> PhpType {
    let mut elem_ty = PhpType::Never;
    for item in items {
        elem_ty = crate::ir_lower::expr::merge_ir_indexed_element_type(
            elem_ty,
            direct_closure_return_array_item_type(item, captures, params, classes),
        );
    }
    elem_ty
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
    classes: &std::collections::HashMap<String, crate::types::ClassInfo>,
) -> PhpType {
    if let ExprKind::Spread(inner) = &item.kind {
        let source = direct_closure_return_array_item_type(inner, captures, params, classes);
        return match source.codegen_repr() {
            PhpType::Array(elem) => match elem.codegen_repr() {
                PhpType::Void | PhpType::Never => PhpType::Mixed,
                other => other,
            },
            _ => PhpType::Mixed,
        };
    }
    // `null` has no narrower storage than the boxed cell, exactly as the lowering-side
    // `ExprKind::Null` arm decides.
    if matches!(item.kind, ExprKind::Null) {
        return PhpType::Mixed;
    }
    crate::ir_lower::expr::ir_array_storage_type(direct_closure_return_expr_type(
        item, captures, params, classes,
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
    classes: &std::collections::HashMap<String, crate::types::ClassInfo>,
) -> PhpType {
    let mut key_ty = PhpType::Never;
    let mut value_ty = PhpType::Never;
    for (key, value) in pairs {
        let next_key = crate::types::normalized_array_key_type(
            key,
            crate::types::checker::infer_expr_type_syntactic(key),
        );
        key_ty = if matches!(key_ty, PhpType::Never) {
            next_key
        } else {
            crate::types::merge_array_key_types(key_ty, next_key)
        };
        value_ty = crate::ir_lower::expr::merge_ir_assoc_value_type(
            value_ty,
            direct_closure_return_array_item_type(value, captures, params, classes),
        );
    }
    PhpType::AssocArray {
        key: Box::new(key_ty),
        value: Box::new(value_ty),
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
