//! Purpose:
//! Adds AST-only procedural entry points to the native date declarations.
//!
//! Called from:
//! - The generated DateTime declaration injector.
//!
//! Key details:
//! - Public method signatures are reused, not restated. EIR lowers these reserved
//!   wrappers to native exact calls while ordinary method calls remain virtual.

use std::collections::HashMap;
use crate::names::Name;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind, TypeExpr, Visibility};
use crate::span::Span;
use crate::types::traits::FlattenedClass;

/// Installs private static wrappers whose bodies preserve normal argument evaluation.
pub(super) fn install(class_map: &mut HashMap<String, FlattenedClass>) {
    install_debug_properties(class_map);
    let Some(class) = class_map.get_mut("DateTime") else { return; };
    for &(name, method) in crate::types::date_method_dispatch::NATIVE_PROCEDURAL_READS.iter()
        .chain(crate::types::date_method_dispatch::NATIVE_PROCEDURAL_MUTATORS.iter()) {
        if class.methods.iter().any(|method| method.name == name) { continue; }
        let Some(mut wrapper) = class.methods.iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(method)).cloned()
            else { continue; };
        let function = name.strip_prefix("__elephc_").expect("reserved procedural wrapper");
        let signature = crate::types::reflection_builtin_function_sig(function)
            .expect("native procedural wrapper has a public PHP signature");
        assert_eq!(signature.params.len(), wrapper.params.len() + 1);
        let receiver = signature.params[0].0.clone();
        for (parameter, public_parameter) in wrapper.params.iter_mut().zip(&signature.params[1..]) {
            parameter.0 = public_parameter.0.clone();
        }
        let span = Span::dummy();
        let args = wrapper.params.iter().map(|(name, _, _, _)| {
            Expr::new(ExprKind::Variable(name.clone()), span)
        }).collect();
        wrapper.param_attributes.resize(wrapper.params.len(), Vec::new());
        wrapper.param_attributes.insert(0, Vec::new());
        wrapper.params.insert(0, (receiver.clone(), Some(TypeExpr::Named(Name::unqualified("mixed"))), None, false));
        wrapper.name = name.to_string();
        wrapper.visibility = Visibility::Private;
        wrapper.is_static = true;
        wrapper.is_final = true;
        wrapper.body = vec![Stmt::new(StmtKind::Return(Some(Expr::new(ExprKind::MethodCall {
            object: Box::new(Expr::new(ExprKind::Variable(receiver), span)),
            method: method.into(), args,
        }, span))), span)];
        class.methods.push(wrapper);
    }
}

/// Installs a boxed native debug snapshot without invoking user serialization overrides.
fn install_debug_properties(class_map: &mut HashMap<String, FlattenedClass>) {
    use crate::synthetic_class::{e_array_assoc, e_not, e_this_prop, s_if, s_return};
    for name in ["DateTime", "DateTimeImmutable"] {
        let Some(class) = class_map.get_mut(name) else { continue; };
        if class.methods.iter().any(|method| method.name == "__elephc_debug_properties") {
            continue;
        }
        let Some(mut method) = class.methods.iter()
            .find(|method| method.name == "__serialize").cloned() else { continue; };
        method.name = "__elephc_debug_properties".into();
        method.visibility = Visibility::Private;
        method.is_final = true;
        method.return_type = Some(TypeExpr::Named(Name::unqualified("mixed")));
        method.body.insert(0, s_if(e_not(e_this_prop("__elephc_initialized")),
            vec![s_return(e_array_assoc(Vec::new()))], Vec::new(), None));
        class.methods.push(method);
    }
}
