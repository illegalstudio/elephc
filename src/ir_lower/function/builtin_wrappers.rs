//! Purpose:
//! Builds builtin callable bodies that require the full semantic EIR lowering context.
//!
//! Called from:
//! - The codegen runtime-wrapper factory for boxed `usort` descriptor entries.
//!
//! Key details:
//! - Reuses direct-call reference capture, COW publication and exceptional cleanup.
//! - Parameters are already validated by the descriptor ABI; no PHP source is reparsed.

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
    let mut statement = Stmt::new(StmtKind::ExprStmt(call), span);
    statement.source_mode = if strict_php {
        crate::source::SourceMode::Php
    } else {
        crate::source::SourceMode::Lfc
    };
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
        &[statement],
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
