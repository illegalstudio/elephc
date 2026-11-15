//! Purpose:
//! Checks proven original call arguments before folding removes calls or converts their values.
//!
//! Called from:
//! - The source numeric evidence visitor, including expressions in declaration defaults.
//!
//! Key details:
//! - Argument placement and trait composition reuse their authoritative compiler planners.
//! - Unknown receivers, ambiguous declarations and generic contracts remain checker responsibilities.

use super::*;
use crate::types::{FunctionSig, PhpType};
use crate::types::call_args::{plan_call_args, expand_planned_positional_spreads, PlannedSourceValue};

/// Scalar source signatures retain parsed hints without resolving generic or relative types early.
#[derive(Default)]
pub(super) struct Signatures {
    functions: HashMap<String, Option<FunctionSig>>,
    classes: HashMap<String, Option<ClassSignatures>>,
}

/// Flattened methods retain static dispatch eligibility and a canonical inheritance edge.
struct ClassSignatures {
    parent: Option<String>,
    methods: HashMap<String, (bool, FunctionSig)>,
}

impl Signatures {
    /// Collects source declarations without inventing a second trait composition algorithm.
    pub(super) fn collect(program: &Program) -> Self {
        let mut result = Self::default();
        walk_program(program, &mut FunctionCollector(&mut result.functions));
        let mut flattened_program = Vec::new();
        flatten_synthetic(program, &mut flattened_program);
        let (classes, enums, errors) = crate::types::traits::flatten_classes(&flattened_program);
        if !errors.is_empty() { return result; }
        for class in classes.into_iter().chain(enums.into_values()) {
            let methods = class.methods.into_iter().map(|method| {
                let sig = signature(&method.params, method.variadic.as_deref(), method.variadic_type.as_ref());
                (key(&method.name), (method.is_static, sig))
            }).collect();
            let entry = result.classes.entry(key(&class.name));
            match entry {
                std::collections::hash_map::Entry::Vacant(entry) => { entry.insert(Some(ClassSignatures {
                    parent: class.extends.as_deref().map(key), methods,
                })); }
                std::collections::hash_map::Entry::Occupied(mut entry) => { entry.insert(None); }
            }
        }
        result
    }

    /// Resolves an inherited method only while every declaration and parent edge is unambiguous.
    fn method(&self, class: &str, method: &str) -> Option<&(bool, FunctionSig)> {
        let mut current = key(class);
        let mut seen = HashSet::new();
        while seen.insert(current.clone()) {
            let info = self.classes.get(&current)?.as_ref()?;
            if let Some(method) = info.methods.get(&key(method)) { return Some(method); }
            current = info.parent.clone()?;
        }
        None
    }

    /// Binds lexical self/parent while leaving late-static dispatch to runtime-aware consumers.
    fn receiver(&self, receiver: &StaticReceiver, lexical: Option<&str>) -> Option<String> {
        match receiver {
            StaticReceiver::Named(name) => Some(key(name.as_str())),
            StaticReceiver::Self_ => lexical.map(key),
            StaticReceiver::Parent => self.classes.get(lexical?)?.as_ref()?.parent.clone(),
            _ => None,
        }
    }
}

/// Flattens only compiler grouping nodes, retaining source declarations and conditional uncertainty.
fn flatten_synthetic(program: &[Stmt], output: &mut Program) {
    for stmt in program {
        if let StmtKind::Synthetic(body) = &stmt.kind { flatten_synthetic(body, output); }
        else { output.push(stmt.clone()); }
    }
}

/// Records duplicate and extern declarations as unknown rather than selecting an arbitrary binding.
struct FunctionCollector<'a>(&'a mut HashMap<String, Option<FunctionSig>>);

impl AstVisitor for FunctionCollector<'_> {
    /// Retains source hints and planner shape for forward and conditional free functions.
    fn statement(&mut self, stmt: &Stmt) {
        let (name, sig) = match &stmt.kind {
            StmtKind::FunctionDecl { name, params, variadic, variadic_type, .. } =>
                (name, Some(signature(params, variadic.as_deref(), variadic_type.as_ref()))),
            StmtKind::ExternFunctionDecl { name, .. } => (name, None),
            _ => return,
        };
        match self.0.entry(key(name)) {
            std::collections::hash_map::Entry::Vacant(entry) => { entry.insert(sig); }
            std::collections::hash_map::Entry::Occupied(mut entry) => { entry.insert(None); }
        }
    }
}

/// Builds only the shape required by the shared argument planner, with no inferred return contract.
fn signature(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    variadic: Option<&str>, variadic_type: Option<&TypeExpr>) -> FunctionSig {
    let mut sig = FunctionSig {
        params: params.iter().map(|(name, hint, _, _)| (name.clone(), hint.as_ref()
            .and_then(crate::semantics::conversions::scalar_hint).unwrap_or(PhpType::Mixed))).collect(),
        param_type_exprs: params.iter().map(|(_, hint, _, _)| hint.clone()).collect(),
        param_attributes: Vec::new(), defaults: params.iter().map(|(_, _, value, _)| value.clone()).collect(),
        return_type: PhpType::Mixed, declared_return: false, by_ref_return: false,
        ref_params: params.iter().map(|(_, _, _, by_ref)| *by_ref).collect(),
        declared_params: params.iter().map(|(_, hint, _, _)| hint.is_some()).collect(),
        variadic: variadic.map(str::to_string), deprecation: None, is_generator: false,
    };
    if let Some(name) = variadic {
        let element = variadic_type.and_then(crate::semantics::conversions::scalar_hint).unwrap_or(PhpType::Mixed);
        sig.params.push((name.to_string(), PhpType::Array(Box::new(element))));
        sig.param_type_exprs.push(variadic_type.cloned());
        sig.defaults.push(None);
        sig.ref_params.push(false);
        sig.declared_params.push(variadic_type.is_some());
    }
    sig
}

/// Uses canonical PHP symbol matching without conflating case-sensitive parameter names.
fn key(name: &str) -> String { crate::names::php_symbol_key(name.trim_start_matches('\\')) }

/// Checks known scalar operands without executing defaults, dynamic dispatch or uncertain unpacking.
pub(super) fn check(expr: &Expr, class: Option<&str>, definitions: &Definitions, evidence: &Evidence)
    -> Result<(), CompileError> {
    if !evidence.profile.contains(StrictFeature::Coercions) || definitions.unresolved_namespaces { return Ok(()); }
    let signatures = &definitions.calls;
    let generated;
    let mut builtin = None;
    let (sig, args) = match &expr.kind {
        ExprKind::FunctionCall { name, args } => {
            match signatures.functions.get(&key(name.as_str())) {
                Some(Some(sig)) => (sig, args),
                Some(None) => return Ok(()),
                None => {
                    let Some(definition) = crate::builtins::registry::lookup(name.as_str()) else { return Ok(()); };
                    generated = crate::builtins::registry::function_sig(name.as_str()).expect("registered builtin signature");
                    builtin = Some(definition);
                    (&generated, args)
                }
            }
        }
        ExprKind::NewObject { class_name, args } => {
            let Some((_, sig)) = signatures.method(class_name.as_str(), "__construct") else { return Ok(()); };
            (sig, args)
        }
        ExprKind::NewScopedObject { receiver, args } => {
            let Some(owner) = signatures.receiver(receiver, class) else { return Ok(()); };
            let Some((_, sig)) = signatures.method(&owner, "__construct") else { return Ok(()); };
            (sig, args)
        }
        ExprKind::StaticMethodCall { receiver, method, args } => {
            let Some(owner) = signatures.receiver(receiver, class) else { return Ok(()); };
            let Some((true, sig)) = signatures.method(&owner, method) else { return Ok(()); };
            (sig, args)
        }
        _ => return Ok(()),
    };
    let Ok(mut plan) = plan_call_args(sig, args, expr.span, true, true) else { return Ok(()); };
    if let Some(expanded) = expand_planned_positional_spreads(args) {
        let Ok(expanded) = plan_call_args(sig, &expanded, expr.span, true, true) else { return Ok(()); };
        plan = expanded;
    }
    // Planner expressions have their own lifetime, so never reuse address memo entries from source.
    let mut local = Evidence { profile: evidence.profile, active: evidence.active.clone(),
        strict_types: evidence.strict_types, ..Default::default() };
    let regular = crate::types::call_args::regular_param_count(sig);
    if plan.first_named_pos.is_none() {
        for (index, value) in plan.source_args.iter().enumerate() {
            if matches!(value.kind, ExprKind::Spread(_)) { break; }
            check_argument(sig, builtin, if index < regular { index } else { regular }, value, class, definitions, &mut local)?;
        }
    } else {
        for value in &plan.source_values {
            let (index, value) = match value {
                PlannedSourceValue::Regular { param_idx, expr, .. } => (*param_idx, expr),
                PlannedSourceValue::Variadic { expr, .. } => (regular, expr),
            };
            check_argument(sig, builtin, index, value, class, definitions, &mut local)?;
        }
    }
    Ok(())
}

/// Applies a parsed scalar parameter contract to one explicitly supplied original value.
fn check_argument(sig: &FunctionSig, builtin: Option<&crate::builtins::registry::BuiltinDef>,
    index: usize, value: &Expr, class: Option<&str>,
    definitions: &Definitions, evidence: &mut Evidence) -> Result<(), CompileError> {
    if let Some(builtin) = builtin {
        if let Some(expected) = crate::builtins::input_contracts::parameter_type(builtin, index) {
            declarations::check_expected(&expected, value, class, definitions, evidence, "builtin argument")?;
        }
        return Ok(());
    }
    let Some(Some(hint)) = sig.param_type_exprs.get(index) else { return Ok(()); };
    declarations::check_value(hint, value, class, definitions, evidence, "call argument")
}
