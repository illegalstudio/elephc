//! Purpose:
//! Builds builtin callable bodies that require the full semantic EIR lowering context.
//!
//! Called from:
//! - The codegen runtime-wrapper factory for boxed array descriptor entries.
//!
//! Key details:
//! - Reuses direct-call reference capture, COW publication and exceptional cleanup.
//! - Variadic adapters validate their supported arity before indexing the argument pack.
//! - No PHP source is reparsed.
//! - Source-sensitive wrappers branch on the caller's frame-owned profile, not creation scope.

use super::*;

/// Gives a registry argument strategy the same rooted EIR path as a direct builtin call.
pub(crate) fn lower_prepared_builtin_callable(
    module: &mut Module, label: &str, name: &str, signature: &FunctionSig, strict_php: bool,
) -> Function {
    let span = Span::dummy();
    let call = Expr::new(ExprKind::FunctionCall {
        name: Name::unqualified(name),
        args: signature.params.iter().map(|(name, _)|
            Expr::new(ExprKind::Variable(name.clone()), span)).collect(),
    }, span);
    let mut internal_signature = signature.clone();
    internal_signature.params.push(("__elephc_invocation_scope".to_string(), PhpType::Int));
    internal_signature.defaults.push(None);
    internal_signature.ref_params.push(false);
    internal_signature.declared_params.push(true);
    internal_signature.param_type_exprs.push(Some(TypeExpr::Int));
    internal_signature.param_attributes.push(Vec::new());
    let mut strict_return = Stmt::new(StmtKind::Return(Some(call.clone())), span);
    strict_return.strict_types = true;
    let mut weak_return = Stmt::new(StmtKind::Return(Some(call)), span);
    weak_return.strict_types = false;
    let branch = Stmt::new(StmtKind::If {
        condition: Expr::new(ExprKind::BinaryOp {
            left: Box::new(Expr::new(ExprKind::Variable("__elephc_invocation_scope".to_string()), span)),
            op: BinOp::Lt,
            right: Box::new(Expr::new(ExprKind::IntLiteral(-1), span)),
        }, span),
        then_body: vec![strict_return], elseif_clauses: Vec::new(),
        else_body: Some(vec![weak_return]),
    }, span);
    lower_builtin_callable_body(module, label, &internal_signature, strict_php, vec![branch])
}

/// Lowers a boxed usort wrapper through the same checked body boundary as a direct PHP call.
pub(crate) fn lower_boxed_usort_callable(
    module: &mut Module,
    label: &str,
    signature: &FunctionSig,
    strict_php: bool,
) -> Function {
    let span = Span::dummy();
    let call = Expr::new(ExprKind::FunctionCall {
        name: Name::unqualified("usort"),
        args: signature.params.iter().map(|(name, _)| {
            Expr::new(ExprKind::Variable(name.clone()), span)
        }).collect(),
    }, span);
    lower_builtin_callable_body(
        module, label, signature, strict_php, vec![Stmt::new(StmtKind::ExprStmt(call), span)],
    )
}

/// Forwards the callable argument pack to `array_merge(...$args)`, which accepts any number of arrays.
pub(crate) fn lower_array_merge_callable(
    module: &mut Module,
    label: &str,
    signature: &FunctionSig,
    strict_php: bool,
) -> Function {
    let span = Span::dummy();
    let arguments = Expr::new(ExprKind::Variable(
        signature.variadic.clone().expect("array_merge callable has a variadic pack"),
    ), span);
    let merge = Expr::new(ExprKind::FunctionCall {
        name: Name::unqualified("array_merge"),
        args: vec![Expr::new(ExprKind::Spread(Box::new(arguments)), span)],
    }, span);
    lower_builtin_callable_body(
        module, label, signature, strict_php,
        vec![Stmt::new(StmtKind::Return(Some(merge)), span)],
    )
}

/// Applies ordinary parameter ownership and exceptional cleanup to a synthetic callable body.
pub(super) fn lower_builtin_callable_body(
    module: &mut Module,
    label: &str,
    signature: &FunctionSig,
    strict_php: bool,
    mut statements: Vec<Stmt>,
) -> Function {
    let source_mode = if strict_php {
        crate::source::SourceMode::Php
    } else {
        crate::source::SourceMode::Lfc
    };
    for statement in &mut statements {
        statement.source_mode = source_mode;
    }
    let return_type = signature.return_type.codegen_repr();
    let mut function = Function::new(
        label.to_string(), return_ir_type(&return_type), return_type.clone(),
    );
    function.params = function_params(signature);
    function.signature = Some(signature.clone());
    // The body calls user-level compatibility helpers (`__elephc_array_merge_gradual`, ...) by
    // name, so it needs the module's declared function signatures to resolve them; with an empty
    // table the call fell through to a runtime "undefined function" error. Prefer the emitted
    // signatures so a call into an injected prelude helper also sees the
    // hidden `func_args` ABI slots `capture_all_frames` appended to it. Without this map the
    // builtin lowering at the bottom of this wrapper cannot tell the helper needs a collector.
    let function_signatures = module
        .functions
        .iter()
        .filter_map(|function| {
            function
                .signature
                .clone()
                .map(|signature| (function.name.clone(), signature))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let closures = lower_body_into_function(
        module.shared_source_catalog(),
        module.runtime_bound_functions.clone(),
        &mut function,
        None,
        &mut module.data,
        &statements,
        env_from_signature(signature, false),
        TypeEnv::default(),
        &function_signatures,
        &Default::default(),
        &module.extern_globals,
        &module.callable_param_sigs,
        &Default::default(),
        &Default::default(),
        &module.class_infos,
        &module.enum_infos,
        &module.interface_infos,
        &module.declared_trait_names,
        &module.declared_trait_methods,
        &module.declared_trait_properties,
        &module.packed_class_infos,
        &module.extern_class_infos,
        &Default::default(), // throw access sites
        &Default::default(), // builtin call types
        &Default::default(), // source generator closure spans
        &Default::default(), // array_push property plans
        &Default::default(), // checked member receiver types are absent in synthesized bodies
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        // One more empty site map: the merge with core-align left this wrapper lowering with two
        // independent site tables to forward, and a builtin wrapper has neither.
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        label.to_string(),
        &module.global_constants,
        None,
        return_type,
        signature.declared_return,
        &signature.params,
        None,
        false,
        Default::default(),
        None,
        module.source_path.clone(),
        None,
        false,
    );
    debug_assert!(closures.is_empty(), "a descriptor wrapper must not synthesize closure declarations");
    function
}
