//! Purpose:
//! Reconstructs callable declaration metadata from lowered EIR functions.
//!
//! Called from:
//! - Closure descriptor emission, native callable registration, and Fiber wrappers.
//!
//! Key details:
//! - Stored declaration metadata survives hidden ABI parameters and coroutine return storage.

use crate::types::{FunctionSig, PhpType};

/// Reconstructs callable signature metadata from an emitted EIR function.
pub(in crate::codegen) fn function_signature_from_eir(function: &crate::ir::Function) -> FunctionSig {
    function_signature_from_eir_with_param_count(function, function.params.len())
}

/// Reconstructs signature metadata from the first `param_count` EIR params.
pub(in crate::codegen) fn function_signature_from_eir_with_param_count(
    function: &crate::ir::Function,
    param_count: usize,
) -> FunctionSig {
    let mut source = function.signature.clone();
    if let Some(signature) = &mut source {
        ensure_variadic_param_slot(signature);
    }
    if let Some(signature) = &function.signature {
        if signature.params.len() == param_count {
            return source.unwrap();
        }
    }
    let source = source.as_ref();
    let source_indices: Vec<_> = function.params.iter().take(param_count)
        .map(|param| source.and_then(|signature| signature.params.iter()
            .position(|(name, _)| name == &param.name)))
        .collect();
    let is_generator = source.map_or(function.flags.is_generator, |signature| signature.is_generator);

    FunctionSig {
        params: function
            .params
            .iter()
            .take(param_count)
            .map(|param| (param.name.clone(), param.php_type.clone()))
            .collect(),
        param_type_exprs: source_indices.iter().map(|index| source
            .and_then(|signature| signature.param_type_exprs.get((*index)?)).cloned().flatten()).collect(),
        param_attributes: source_indices.iter().map(|index| source
            .and_then(|signature| signature.param_attributes.get((*index)?)).cloned().unwrap_or_default()).collect(),
        defaults: source_indices.iter().map(|index| source
            .and_then(|signature| signature.defaults.get((*index)?)).cloned().flatten()).collect(),
        return_type: source.map(|signature| signature.return_type.clone()).unwrap_or_else(|| {
            if is_generator { PhpType::Object("Generator".into()) }
            else { function.return_php_type.clone() }
        }),
        declared_return: source.map_or(
            !is_generator && !matches!(function.return_php_type, PhpType::Mixed),
            |signature| signature.declared_return,
        ),
        by_ref_return: function
            .signature
            .as_ref()
            .map_or(function.flags.by_ref_return, |signature| signature.by_ref_return),
        ref_params: function
            .params
            .iter()
            .take(param_count)
            .map(|param| param.by_ref)
            .collect(),
        declared_params: function
            .params
            .iter()
            .take(param_count)
            .zip(&source_indices)
            .map(|(param, index)| source
                .and_then(|signature| signature.declared_params.get((*index)?)).copied()
                .unwrap_or(!matches!(param.php_type, PhpType::Mixed)))
            .collect(),
        variadic: function
            .params
            .iter()
            .take(param_count)
            .find(|param| param.variadic)
            .map(|param| param.name.clone()),
        deprecation: source.and_then(|signature| signature.deprecation.clone()),
        is_generator,
    }
}

/// Adds the virtual variadic array slot when the EIR ABI stores it outside `params`.
fn ensure_variadic_param_slot(signature: &mut FunctionSig) {
    let Some(variadic) = signature.variadic.clone() else {
        return;
    };
    if signature.params.iter().any(|(name, _)| name == &variadic) {
        return;
    }
    let variadic_index = signature.params.len();
    let variadic_type_expr = if signature.param_type_exprs.len() > variadic_index {
        signature.param_type_exprs.remove(variadic_index)
    } else {
        None
    };
    let variadic_ref = if signature.ref_params.len() > variadic_index {
        signature.ref_params.remove(variadic_index)
    } else {
        false
    };
    let variadic_declared = if signature.declared_params.len() > variadic_index {
        signature.declared_params.remove(variadic_index)
    } else {
        false
    };
    signature
        .params
        .push((variadic, PhpType::Array(Box::new(PhpType::Mixed))));
    signature.defaults.push(None);
    signature.ref_params.push(variadic_ref);
    signature.declared_params.push(variadic_declared);
    signature.param_type_exprs.push(variadic_type_expr);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::IrType;

    /// EIR-only callable metadata retains the declaration's reference-return flag.
    #[test]
    fn callable_signature_fallback_preserves_eir_reference_return() {
        for by_ref_return in [false, true] {
            let mut function = crate::ir::Function::new("ref_result".into(), IrType::I64, PhpType::Int);
            function.flags.by_ref_return = by_ref_return;

            assert_eq!(function_signature_from_eir(&function).by_ref_return, by_ref_return);
        }
    }

    /// A parameter-count mismatch must not discard stored declaration metadata.
    #[test]
    fn callable_signature_fallback_preserves_stored_reference_return() {
        for by_ref_return in [false, true] {
            let mut function = crate::ir::Function::new("ref_result".into(), IrType::I64, PhpType::Int);
            let mut signature = function_signature_from_eir(&function);
            signature.by_ref_return = by_ref_return;
            function.signature = Some(signature);
            function.params.push(crate::ir::FunctionParam {
                name: "capture".into(),
                ir_type: IrType::I64,
                php_type: PhpType::Int,
                by_ref: false,
                variadic: false,
            });

            assert_eq!(function_signature_from_eir(&function).by_ref_return, by_ref_return);
            assert_eq!(
                function_signature_from_eir_with_param_count(&function, 0).by_ref_return,
                by_ref_return,
            );
        }
    }

    /// Rebuilding with hidden capture parameters keeps generator, default and deprecation metadata.
    #[test]
    fn callable_signature_rebuild_preserves_declaration_metadata() {
        let mut function = crate::ir::Function::new("generator".into(), IrType::I64, PhpType::Mixed);
        function.params.push(crate::ir::FunctionParam {
            name: "value".into(), ir_type: IrType::I64, php_type: PhpType::Int,
            by_ref: false, variadic: false,
        });
        let mut signature = function_signature_from_eir(&function);
        signature.is_generator = true;
        signature.return_type = PhpType::Object("Generator".into());
        signature.declared_return = false;
        signature.deprecation = Some("use another generator".into());
        signature.defaults[0] = Some(crate::parser::ast::Expr {
            kind: crate::parser::ast::ExprKind::IntLiteral(7),
            span: crate::span::Span::new(1, 1),
        });
        signature.param_type_exprs[0] = Some(crate::parser::ast::TypeExpr::Int);
        function.signature = Some(signature);
        function.params.push(crate::ir::FunctionParam {
            name: "capture".into(), ir_type: IrType::I64, php_type: PhpType::Int,
            by_ref: false, variadic: false,
        });

        let rebuilt = function_signature_from_eir(&function);
        assert!(rebuilt.is_generator);
        assert_eq!(rebuilt.return_type, PhpType::Object("Generator".into()));
        assert!(!rebuilt.declared_return);
        assert_eq!(rebuilt.deprecation.as_deref(), Some("use another generator"));
        assert!(matches!(&rebuilt.defaults[0], Some(default)
            if matches!(default.kind, crate::parser::ast::ExprKind::IntLiteral(7))));
        assert!(rebuilt.defaults[1].is_none());
        assert_eq!(rebuilt.param_type_exprs[0], Some(crate::parser::ast::TypeExpr::Int));
        assert_eq!(rebuilt.param_type_exprs[1], None);
    }

    /// An EIR-only coroutine publishes Generator rather than its internal Mixed return cell.
    #[test]
    fn callable_signature_rebuild_uses_eir_generator_flag() {
        let mut function = crate::ir::Function::new("generator".into(), IrType::I64, PhpType::Mixed);
        function.flags.is_generator = true;
        let rebuilt = function_signature_from_eir(&function);
        assert!(rebuilt.is_generator);
        assert_eq!(rebuilt.return_type, PhpType::Object("Generator".into()));
        assert!(!rebuilt.declared_return);
    }

}
