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
use crate::parser::ast::{Expr, ExprKind, Program, Stmt, StmtKind, TypeExpr};
use crate::span::Span;
use crate::types::{CheckResult, ClassInfo, FunctionSig, PackedClassInfo, PhpType, TypeEnv};

/// AST parameter tuple shape used by function, method, and closure declarations.
type AstParams = [(String, Option<TypeExpr>, Option<crate::parser::ast::Expr>, bool)];

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
    let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
    function.flags.is_main = true;
    let all_global_var_names = collect_global_var_names(program);
    let closures = lower_body_into_function(
        &mut function,
        &mut module.data,
        program,
        check_result.global_env.clone(),
        check_result.global_env.clone(),
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.extern_globals,
        &check_result.callable_param_sigs,
        fiber_return_sigs,
        &check_result.classes,
        &check_result.enums,
        &check_result.interfaces,
        &check_result.packed_classes,
        constants,
        None,
        PhpType::Void,
        &[],
        None,
        true,
        all_global_var_names,
    );
    add_closures(module, closures);
    module.add_function(function);
}

/// Collects PHP variable names that any function-like body declares with `global`.
fn collect_global_var_names(statements: &[Stmt]) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    collect_global_var_names_in_body(statements, &mut names);
    names
}

/// Recursively scans statement bodies for `global` declarations.
fn collect_global_var_names_in_body(
    statements: &[Stmt],
    names: &mut std::collections::HashSet<String>,
) {
    for stmt in statements {
        match &stmt.kind {
            crate::parser::ast::StmtKind::Global { vars } => {
                names.extend(vars.iter().cloned());
            }
            crate::parser::ast::StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_global_var_names_in_body(then_body, names);
                for (_, body) in elseif_clauses {
                    collect_global_var_names_in_body(body, names);
                }
                if let Some(body) = else_body {
                    collect_global_var_names_in_body(body, names);
                }
            }
            crate::parser::ast::StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                collect_global_var_names_in_body(then_body, names);
                if let Some(body) = else_body {
                    collect_global_var_names_in_body(body, names);
                }
            }
            crate::parser::ast::StmtKind::While { body, .. }
            | crate::parser::ast::StmtKind::DoWhile { body, .. }
            | crate::parser::ast::StmtKind::Foreach { body, .. }
            | crate::parser::ast::StmtKind::FunctionDecl { body, .. }
            | crate::parser::ast::StmtKind::NamespaceBlock { body, .. }
            | crate::parser::ast::StmtKind::IncludeOnceGuard { body, .. }
            | crate::parser::ast::StmtKind::Synthetic(body) => {
                collect_global_var_names_in_body(body, names);
            }
            crate::parser::ast::StmtKind::For {
                init,
                update,
                body,
                ..
            } => {
                if let Some(init) = init {
                    collect_global_var_names_in_body(std::slice::from_ref(init.as_ref()), names);
                }
                if let Some(update) = update {
                    collect_global_var_names_in_body(std::slice::from_ref(update.as_ref()), names);
                }
                collect_global_var_names_in_body(body, names);
            }
            crate::parser::ast::StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_global_var_names_in_body(body, names);
                }
                if let Some(body) = default {
                    collect_global_var_names_in_body(body, names);
                }
            }
            crate::parser::ast::StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_global_var_names_in_body(try_body, names);
                for catch in catches {
                    collect_global_var_names_in_body(&catch.body, names);
                }
                if let Some(body) = finally_body {
                    collect_global_var_names_in_body(body, names);
                }
            }
            crate::parser::ast::StmtKind::ClassDecl { methods, .. }
            | crate::parser::ast::StmtKind::InterfaceDecl { methods, .. }
            | crate::parser::ast::StmtKind::TraitDecl { methods, .. } => {
                for method in methods {
                    collect_global_var_names_in_body(&method.body, names);
                }
            }
            _ => {}
        }
    }
}

/// Lowers one user-defined function declaration into an EIR function.
pub(crate) fn lower_user_function(
    name: &str,
    params: &AstParams,
    return_type: Option<&TypeExpr>,
    body: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, FunctionSig>,
) {
    let fallback = signature_from_ast(params, return_type);
    let signature = check_result.functions.get(name).unwrap_or(&fallback);
    let eir_signature = eir_signature_with_php_param_contracts(
        name,
        signature,
        &check_result.callable_param_sigs,
    );
    // A generator's compiled body is a coroutine that returns the value passed
    // to `return` (Mixed, read back via `Generator::getReturn()`), not the
    // `Generator` object itself. The public signature stays `Generator` for
    // callers; only the EIR body return type becomes Mixed so `return $x`
    // lowers to a plain boxed Mixed return instead of a Generator coercion.
    let body_return_type =
        generator_body_return_type(body, &eir_signature.return_type);
    let mut function = Function::new(
        name.to_string(),
        return_ir_type(&body_return_type),
        body_return_type.clone(),
    );
    function.params = function_params(&eir_signature);
    function.source_signature = Some(source_signature(name, &eir_signature));
    function.signature = Some(eir_runtime_metadata_signature(&eir_signature));
    attach_generator_source_if_needed(&mut function, body, eir_signature.params.len());
    let closures = lower_body_into_function(
        &mut function,
        &mut module.data,
        body,
        env_from_signature(&eir_signature),
        check_result.global_env.clone(),
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.extern_globals,
        &check_result.callable_param_sigs,
        fiber_return_sigs,
        &check_result.classes,
        &check_result.enums,
        &check_result.interfaces,
        &check_result.packed_classes,
        constants,
        None,
        body_return_type.clone(),
        &eir_signature.params,
        None,
        false,
        std::collections::HashSet::new(),
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
    let fallback = signature_from_ast(params, return_type);
    let signature = check_result
        .classes
        .get(class_name)
        .and_then(|class| method_signature(class, method_name, is_static))
        .unwrap_or(&fallback);
    let name = format!("{}::{}", class_name, method_name);
    // Generator methods lower their body as a Mixed-returning coroutine; see
    // `generator_body_return_type`.
    let method_body_return_type = generator_body_return_type(body, &signature.return_type);
    let mut function = Function::new(
        name.clone(),
        return_ir_type(&method_body_return_type),
        method_body_return_type.clone(),
    );
    function.flags = FunctionFlags {
        is_method: true,
        is_static,
        ..FunctionFlags::default()
    };
    function.source_signature = Some(source_signature(&name, signature));
    function.signature = Some(eir_runtime_metadata_signature(signature));
    let mut env = env_from_signature(signature);
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
    function.params.extend(function_params(signature));
    attach_generator_source_if_needed(&mut function, body, body_params.len());
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
        fiber_return_sigs,
        &check_result.classes,
        &check_result.enums,
        &check_result.interfaces,
        &check_result.packed_classes,
        constants,
        Some(class_name.to_string()),
        method_body_return_type.clone(),
        &body_params,
        None,
        false,
        std::collections::HashSet::new(),
    );
    add_closures(module, closures);
    module.class_methods.push(function);
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
    let body = property_init_body(class_info);
    let function_name = format!("_class_propinit_{}", class_info.class_id);
    let this_type = PhpType::Object(class_name.to_string());
    let mut function = Function::new(function_name.clone(), IrType::Void, PhpType::Void);
    function.params.push(FunctionParam {
        name: "this".to_string(),
        ir_type: value_ir_type(&this_type),
        php_type: this_type.clone(),
        by_ref: false,
        variadic: false,
    });
    let sig = FunctionSig {
        params: vec![("this".to_string(), this_type.clone())],
        defaults: vec![None],
        return_type: PhpType::Void,
        declared_return: false,
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
        check_result.global_env.clone(),
        &check_result.functions,
        &check_result.extern_functions,
        &check_result.extern_globals,
        &check_result.callable_param_sigs,
        fiber_return_sigs,
        &check_result.classes,
        &check_result.enums,
        &check_result.interfaces,
        &check_result.packed_classes,
        constants,
        Some(class_name.to_string()),
        PhpType::Void,
        &params,
        None,
        false,
        std::collections::HashSet::new(),
    );
    add_closures(module, closures);
    module.add_function(function);
}

/// Builds `$this->property = <default>;` statements for property-default initialization.
fn property_init_body(class_info: &ClassInfo) -> Vec<Stmt> {
    let span = Span::dummy();
    class_info
        .defaults
        .iter()
        .enumerate()
        .filter_map(|(index, default)| {
            let default = default.as_ref()?;
            let property = class_info.properties.get(index)?.0.clone();
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
    return_type: Option<&TypeExpr>,
    body: &[Stmt],
    captures: &[(String, PhpType, bool)],
    self_ref_callable_capture: Option<&str>,
) -> FunctionSig {
    let signature = closure_signature_from_ast(params, variadic, return_type, body, captures);
    lower_closure_function_with_signature(
        parent,
        name,
        signature,
        body,
        captures,
        self_ref_callable_capture,
    )
}

/// Lowers one closure literal using contextual types for unannotated parameters.
pub(crate) fn lower_closure_function_with_context(
    parent: &mut LoweringContext<'_, '_>,
    name: &str,
    params: &AstParams,
    variadic: Option<&str>,
    return_type: Option<&TypeExpr>,
    body: &[Stmt],
    captures: &[(String, PhpType, bool)],
    contextual_arg_types: &[PhpType],
    self_ref_callable_capture: Option<&str>,
) -> FunctionSig {
    let mut signature = closure_signature_from_ast(params, variadic, return_type, body, captures);
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
) -> FunctionSig {
    // Generator closures lower their body as a Mixed-returning coroutine; see
    // `generator_body_return_type`.
    let closure_body_return_type = generator_body_return_type(body, &signature.return_type);
    let mut function = Function::new(
        name.to_string(),
        return_ir_type(&closure_body_return_type),
        closure_body_return_type.clone(),
    );
    function.flags = FunctionFlags {
        is_closure: true,
        ..FunctionFlags::default()
    };
    function.params = function_params(&signature);
    function.params.extend(closure_capture_params(captures));
    function.source_signature = Some(source_signature(name, &signature));
    function.signature = Some(eir_runtime_metadata_signature(&signature));
    attach_generator_source_if_needed(&mut function, body, signature.params.len());
    let env = env_with_closure_captures(&signature, captures);
    let lowered_params = params_with_closure_captures(&signature, captures);
    let recursive_binding =
        self_ref_callable_capture.map(|local_name| RecursiveClosureBinding {
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
        parent.fiber_return_sigs,
        parent.classes,
        parent.enums,
        parent.interfaces,
        parent.packed_classes,
        &parent.constants,
        parent.current_class.clone(),
        closure_body_return_type.clone(),
        &lowered_params,
        recursive_binding,
        false,
        collect_global_var_names(body),
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
    fiber_return_sigs: &std::collections::HashMap<String, FunctionSig>,
    classes: &std::collections::HashMap<String, crate::types::ClassInfo>,
    enums: &std::collections::HashMap<String, crate::types::EnumInfo>,
    interfaces: &std::collections::HashMap<String, crate::types::InterfaceInfo>,
    packed_classes: &std::collections::HashMap<String, PackedClassInfo>,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    current_class: Option<String>,
    return_php_type: PhpType,
    params: &[(String, PhpType)],
    recursive_closure_binding: Option<RecursiveClosureBinding>,
    in_main: bool,
    all_global_var_names: std::collections::HashSet<String>,
) -> Vec<Function> {
    let owner_name = function.name.clone();
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
        fiber_return_sigs,
        classes,
        enums,
        interfaces,
        packed_classes,
        constants,
        top_level_env,
        current_class,
        owner_name,
        return_php_type,
        in_main,
        all_global_var_names,
    );
    for (index, (name, php_type)) in params.iter().enumerate() {
        ctx.declare_local(name, php_type.clone());
        ctx.mark_local_initialized(name);
        if by_ref_params.get(index).copied().unwrap_or(false) {
            ctx.mark_ref_bound_local(name);
        }
    }
    seed_recursive_closure_binding(&mut ctx, recursive_closure_binding);
    for stmt in body {
        crate::ir_lower::stmt::lower_stmt(&mut ctx, stmt);
    }
    terminate_open_block(&mut ctx);
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
        let message =
            ctx.intern_string("Fatal error: A never-returning function must not implicitly return\n");
        ctx.builder.terminate(Terminator::Fatal { message });
        return;
    }
    if ctx.return_type == IrType::Void {
        ctx.builder.terminate(Terminator::Return { value: None });
        return;
    }
    let value = emit_default_return_value(ctx);
    ctx.builder.terminate(Terminator::Return { value: Some(value) });
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
fn eir_signature_with_php_param_contracts(
    owner_name: &str,
    signature: &FunctionSig,
    callable_param_sigs: &std::collections::HashMap<(String, String), FunctionSig>,
) -> FunctionSig {
    let mut eir_signature = signature.clone();
    let mut has_dynamic_untyped_param = false;
    for (index, (name, php_type)) in eir_signature.params.iter_mut().enumerate() {
        let declared = signature.declared_params.get(index).copied().unwrap_or(false);
        let by_ref = signature.ref_params.get(index).copied().unwrap_or(false);
        let variadic = signature.variadic.as_deref() == Some(name.as_str());
        if !declared && !by_ref && !variadic {
            if preserve_untyped_eir_param_contract(owner_name, name, php_type, callable_param_sigs) {
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
    param_name: &str,
    php_type: &PhpType,
    callable_param_sigs: &std::collections::HashMap<(String, String), FunctionSig>,
) -> bool {
    matches!(php_type.codegen_repr(), PhpType::Callable)
        || callable_param_sigs.contains_key(&(owner_name.to_string(), param_name.to_string()))
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
fn env_from_signature(signature: &FunctionSig) -> TypeEnv {
    signature
        .params
        .iter()
        .map(|(name, php_type)| (name.clone(), php_type.clone()))
        .collect()
}

/// Creates a closure environment that includes hidden captured locals.
fn env_with_closure_captures(
    signature: &FunctionSig,
    captures: &[(String, PhpType, bool)],
) -> TypeEnv {
    let mut env = env_from_signature(signature);
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
    signature_from_ast_with_variadic(params, return_type, None)
}

/// Builds an EIR closure signature and infers fallthrough-only closures as `void`.
fn closure_signature_from_ast(
    params: &AstParams,
    variadic: Option<&str>,
    return_type: Option<&TypeExpr>,
    body: &[Stmt],
    captures: &[(String, PhpType, bool)],
) -> FunctionSig {
    let mut signature = signature_from_ast_with_variadic(params, return_type, variadic);
    if crate::types::checker::yield_validation::body_contains_yield(body) {
        signature.return_type = PhpType::Object("Generator".to_string());
        return signature;
    }
    if return_type.is_none() {
        if let Some(return_ty) = direct_closure_return_type(body, captures) {
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
) -> Option<PhpType> {
    let [stmt] = body else {
        return None;
    };
    let StmtKind::Return(Some(expr)) = &stmt.kind else {
        return None;
    };
    Some(direct_closure_return_expr_type(expr, captures))
}

/// Returns a direct closure return expression type, consulting capture metadata first.
fn direct_closure_return_expr_type(
    expr: &crate::parser::ast::Expr,
    captures: &[(String, PhpType, bool)],
) -> PhpType {
    if let ExprKind::Variable(name) = &expr.kind {
        if let Some((_, php_type, _)) = captures
            .iter()
            .find(|(capture_name, _, _)| capture_name == name)
        {
            return php_type.clone();
        }
    }
    crate::types::checker::infer_expr_type_syntactic(expr)
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
            init,
            update,
            body,
            ..
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
) -> FunctionSig {
    let mut signature = FunctionSig {
        params: params
            .iter()
            .map(|(name, ty, _, _)| {
                (
                    name.clone(),
                    ty.as_ref().map(type_expr_to_php_type).unwrap_or(PhpType::Mixed),
                )
            })
            .collect(),
        defaults: params.iter().map(|(_, _, default, _)| default.clone()).collect(),
        return_type: return_type
            .map(type_expr_to_php_type)
            .unwrap_or(PhpType::Mixed),
        declared_return: return_type.is_some(),
        ref_params: params.iter().map(|(_, _, _, by_ref)| *by_ref).collect(),
        declared_params: params.iter().map(|(_, ty, _, _)| ty.is_some()).collect(),
        variadic: variadic.map(str::to_string),
        deprecation: None,
    };
    append_variadic_param_slot(&mut signature);
    signature
}

/// Adds the variadic `array<mixed>` parameter slot omitted from parsed parameter tuples.
fn append_variadic_param_slot(signature: &mut FunctionSig) {
    let Some(variadic) = signature.variadic.clone() else {
        return;
    };
    if signature.params.iter().any(|(name, _)| name == &variadic) {
        return;
    }
    signature
        .params
        .push((variadic, PhpType::Array(Box::new(PhpType::Mixed))));
    signature.defaults.push(None);
    signature.ref_params.push(false);
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
