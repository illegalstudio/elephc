//! Purpose:
//! Builds EIR interface adapters for implementations with additional optional parameters.
//!
//! Called from:
//! - `crate::ir_lower::program` after class-like methods and constructor thunks.
//!
//! Key details:
//! - Default expressions and reference arguments use ordinary method-call lowering.
//! - The adapter has the interface ABI and returns through ordinary ownership cleanup.

use super::*;
use crate::codegen_support::source_method_adapters::{plan_method_abi, MethodAbiPlan};

/// Adds deterministic interface entry functions where the physical method has optional extras.
pub(crate) fn lower_optional_interface_adapters(module: &mut Module) {
    let mut adapters = Vec::new();
    for (class_name, class) in &module.class_infos {
        if !module.enum_infos.contains_key(class_name) { continue; }
        for interface_name in &class.interfaces {
            let Some(interface) = module.interface_infos.get(interface_name) else { continue; };
            for method in &interface.method_order {
                let Some(owner) = class.method_impl_classes.get(method) else { continue; };
                let Some(actual) = module.class_infos.get(owner).and_then(|info| info.methods.get(method)) else { continue; };
                let Some(caller) = interface.methods.get(method) else { continue; };
                if plan_method_abi(caller, actual) != Ok(MethodAbiPlan::OptionalDefaults) { continue; }
                let name = crate::names::interface_method_wrapper_symbol(
                    class.class_id, interface.interface_id, method,
                );
                adapters.push((name, owner.clone(), method.clone(), caller.clone()));
            }
        }
    }
    adapters.sort_by(|left, right| left.0.cmp(&right.0));
    for (name, owner, method, caller) in adapters {
        let function = lower_interface_adapter(module, &name, &owner, &method, &caller);
        module.add_function(function);
    }
}

/// Lowers one interface-to-concrete call with the interface's explicit parameters.
fn lower_interface_adapter(
    module: &mut Module, name: &str, owner: &str, method: &str, caller: &FunctionSig,
) -> Function {
    let span = Span::dummy();
    let receiver_name = crate::names::generated_local_name("__elephc_interface_receiver");
    let mut signature = caller.clone();
    signature.params.insert(0, (receiver_name.clone(), PhpType::Object(owner.to_string())));
    signature.param_type_exprs.insert(0, None);
    signature.param_attributes.insert(0, Vec::new());
    signature.defaults.insert(0, None);
    signature.ref_params.insert(0, false);
    signature.declared_params.insert(0, true);
    let call = Expr::new(ExprKind::MethodCall {
        object: Box::new(Expr::new(ExprKind::Variable(receiver_name), span)),
        method: method.to_string(),
        args: caller.params.iter().map(|(name, _)| Expr::new(ExprKind::Variable(name.clone()), span)).collect(),
    }, span);
    let body = if caller.return_type == PhpType::Void {
        vec![Stmt::new(StmtKind::ExprStmt(call), span)]
    } else { vec![Stmt::new(StmtKind::Return(Some(call)), span)] };
    let return_type = signature.return_type.clone();
    let mut function = Function::new(name.to_string(), return_ir_type(&return_type), return_type.clone());
    function.params = function_params(&signature);
    function.signature = Some(signature.clone());
    function.flags.is_internal = true;
    function.flags.by_ref_return = signature.by_ref_return;
    let closures = lower_body_into_function(
        &mut function, None, &mut module.data, &body,
        env_from_signature(&signature, module.web), TypeEnv::new(),
        &Default::default(), &Default::default(), &module.extern_globals,
        &module.callable_param_sigs, &Default::default(), &Default::default(),
        &module.class_infos, &module.enum_infos, &module.interface_infos,
        &module.declared_trait_names, &module.declared_trait_methods,
        &module.declared_trait_properties, &module.packed_class_infos,
        &Default::default(), &Default::default(), &Default::default(),
        &Default::default(), &Default::default(), &Default::default(),
        &Default::default(), &Default::default(), &Default::default(),
        &Default::default(), name.to_string(), &module.global_constants,
        Some(owner.to_string()), return_type, signature.declared_return,
        &signature.params, None, false, Default::default(), None,
        module.source_path.clone(), None, module.web,
    );
    debug_assert!(closures.is_empty(), "interface defaults must not create closures");
    function
}
