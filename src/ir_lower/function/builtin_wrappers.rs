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

use super::*;

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

/// Expands the callable argument pack into the two operands supported by the merge backend.
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
    let count = Expr::new(ExprKind::FunctionCall {
        name: Name::unqualified("count"), args: vec![arguments.clone()],
    }, span);
    let guard = Stmt::new(StmtKind::If {
        condition: Expr::new(ExprKind::BinaryOp {
            left: Box::new(count), op: BinOp::NotEq,
            right: Box::new(Expr::new(ExprKind::IntLiteral(2), span)),
        }, span),
        then_body: vec![Stmt::new(StmtKind::Throw(Expr::new(ExprKind::NewObject {
            class_name: Name::unqualified("ArgumentCountError"),
            args: vec![Expr::new(ExprKind::StringLiteral(
                "array_merge() takes exactly 2 arguments".to_string(),
            ), span)],
        }, span)), span)],
        elseif_clauses: Vec::new(),
        else_body: None,
    }, span);
    let merge = Expr::new(ExprKind::FunctionCall {
        name: Name::unqualified("array_merge"),
        args: (0..2).map(|index| Expr::new(ExprKind::ArrayAccess {
            array: Box::new(arguments.clone()),
            index: Box::new(Expr::new(ExprKind::IntLiteral(index), span)),
        }, span)).collect(),
    }, span);
    lower_builtin_callable_body(
        module, label, signature, strict_php,
        vec![guard, Stmt::new(StmtKind::Return(Some(merge)), span)],
    )
}

/// Applies ordinary parameter ownership and exceptional cleanup to a synthetic callable body.
fn lower_builtin_callable_body(
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
    let closures = lower_body_into_function(
        &mut function,
        None,
        &mut module.data,
        &statements,
        env_from_signature(signature, false),
        TypeEnv::new(),
        &Default::default(),
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
        &Default::default(),
        &Default::default(),
        &Default::default(),
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
        module.source_path.clone(),
        None,
        false,
    );
    debug_assert!(closures.is_empty(), "a descriptor wrapper must not synthesize closure declarations");
    function
}
