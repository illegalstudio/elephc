//! Purpose:
//! Proves simple callable inputs at closed source declaration boundaries.
//!
//! Called from:
//! - The environment-only AST usage collection and reachable declaration walk.
//!
//! Key details:
//! - Every syntactic caller participates, including callers in unreferenced declarations.
//! - Untyped method receivers are unioned; named static receivers follow source class edges.
//! - Escapes, implicit entries and non-positional arguments prevent certification.

use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};

use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver, TypeExpr};
use super::callable_names::simple_callable;
use super::environment_callables::{self, DeclarationRoots};

/// Parameter shape before the shared semantic argument planner becomes available.
#[derive(Clone, Debug)]
struct Parameter {
    by_ref: bool,
    default: Option<bool>,
}

/// One source declaration, with entry obligations kept separate from explicit callers.
#[derive(Clone, Debug)]
struct Declaration {
    name: String,
    eligible: bool,
    params: Vec<Parameter>,
}

/// A direct caller, with an optional statically named receiver and positional input facts.
#[derive(Clone, Debug)]
struct Caller {
    receiver: Option<String>,
    args: Option<Vec<bool>>,
}

/// Whole-program inputs to the omission proof; this never changes PHP call lowering.
#[derive(Clone, Debug, Default)]
pub(super) struct ParameterInputs {
    declarations: Vec<Declaration>,
    calls: HashMap<String, Vec<Caller>>,
    escaped: HashSet<String>,
}

/// Certified parameter indices, keyed by source declaration rather than local variable name.
#[derive(Clone, Debug, Default)]
pub(super) struct ParameterProof {
    known: HashMap<String, HashSet<usize>>,
}

impl ParameterInputs {
    /// Records one declaration, retaining every duplicate or ambiguous pre-resolution shape.
    pub(super) fn declaration(
        &mut self, name: &str,
        params: &[(String, Option<TypeExpr>, Option<Expr>, bool)], eligible: bool,
    ) {
        self.declarations.push(Declaration {
            name: declaration_key(name), eligible,
            params: params.iter().map(|(_, _, default, by_ref)| Parameter {
                by_ref: *by_ref, default: default.as_ref().map(simple_callable),
            }).collect(),
        });
    }

    /// Records literal positional inputs without attempting independent named/spread planning.
    pub(super) fn call(&mut self, name: &str, method: bool, args: &[Expr]) {
        self.record_call(name, method, args, None);
    }

    /// Keeps named/self receivers distinct while late-static and parent dispatch stay opaque.
    pub(super) fn static_call(&mut self, receiver: &StaticReceiver, class: Option<&str>, method: &str, args: &[Expr]) {
        let receiver = match receiver.written_class_receiver() {
            StaticReceiver::Named(name) => Some(leaf(&php_symbol_key(&name.as_canonical())).to_owned()),
            StaticReceiver::Self_ => class.map(|name| leaf(&php_symbol_key(name)).to_owned()),
            _ => None,
        };
        self.record_call(method, true, args, receiver);
    }

    /// Stores one direct input shape without guessing named/spread argument normalization.
    fn record_call(&mut self, name: &str, method: bool, args: &[Expr], receiver: Option<String>) {
        let positional = !args.iter().any(|arg| matches!(arg.kind,
            ExprKind::NamedArg { .. } | ExprKind::Spread(_)));
        self.calls.entry(call_key(name, method)).or_default().push(Caller {
            receiver, args: positional.then(|| args.iter().map(simple_callable).collect()),
        });
    }

    /// Disables proofs for values that can be invoked outside a directly spelled call site.
    pub(super) fn escape(&mut self, name: &str, method: bool) {
        self.escaped.insert(call_key(name, method));
    }

    /// Certifies an input only when every possible caller and declaration shape agrees.
    pub(super) fn prove(
        &self, literals: &HashSet<String>, roots: &DeclarationRoots,
        edges: &HashMap<String, HashSet<String>>,
    ) -> ParameterProof {
        let literals: HashSet<_> = literals.iter().map(|name| leaf(name).to_owned()).collect();
        let mut proof = ParameterProof::default();
        let mut rejected: HashMap<String, HashSet<usize>> = HashMap::default();
        for declaration in &self.declarations {
            let method = declaration.name.contains("::");
            let target = call_key(&declaration.name, method);
            let calls: Vec<_> = self.calls.get(&target).into_iter().flatten().filter(|call| {
                call.receiver.as_ref().zip(declaration.name.split_once("::"))
                    .is_none_or(|(receiver, (class, _))| can_reach_class(receiver, class, edges))
            }).collect();
            let closed = declaration.eligible
                && !environment_callables::is_referenced(roots, &declaration.name)
                && !self.escaped.contains(&target)
                && !literals.contains(leaf(&declaration.name));
            let required = declaration.params.iter().rposition(|param| param.default.is_none())
                .map_or(0, |index| index + 1);
            for (index, param) in declaration.params.iter().enumerate() {
                let known = closed && !param.by_ref && !calls.is_empty()
                    && calls.iter().all(|call| call.args.as_ref().is_some_and(|args| {
                        // A too-short positional call cannot enter this declaration's body.
                        args.len() < required || args.get(index).copied().or(param.default).unwrap_or(false)
                    }));
                let slots = if known { &mut proof.known } else { &mut rejected };
                slots.entry(declaration.name.clone()).or_default().insert(index);
            }
        }
        for (name, slots) in rejected {
            if let Some(known) = proof.known.get_mut(&name) { known.retain(|index| !slots.contains(index)); }
        }
        proof
    }
}

/// Follows conservative alias/inheritance edges before excluding a statically different receiver.
fn can_reach_class(receiver: &str, target: &str, edges: &HashMap<String, HashSet<String>>) -> bool {
    let mut pending = vec![receiver];
    let mut seen = HashSet::default();
    while let Some(class) = pending.pop() {
        if class == target { return true; }
        if !seen.insert(class) { continue; }
        if let Some(next) = edges.get(class) { pending.extend(next.iter().map(String::as_str)); }
    }
    false
}

impl ParameterProof {
    /// Reports a certified input for the declaration currently being scanned.
    pub(super) fn contains(&self, name: &str, index: usize) -> bool {
        self.known.get(&declaration_key(name)).is_some_and(|slots| slots.contains(&index))
    }
}

/// Keeps function and method dispatch channels distinct while unioning ambiguous receivers.
fn call_key(name: &str, method: bool) -> String {
    format!("{}:{}", if method { "method" } else { "function" }, leaf(&php_symbol_key(name)))
}

/// Normalizes declaration identity while retaining its owning class for implicit-entry checks.
fn declaration_key(name: &str) -> String {
    let key = php_symbol_key(name);
    if let Some((class, method)) = key.split_once("::") {
        format!("{}::{}", leaf(class), leaf(method))
    } else { leaf(&key).to_owned() }
}

/// Compares source spellings before namespace resolution and explicit generic specialization.
fn leaf(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name).split('<').next().unwrap_or(name)
        .rsplit('\\').next().unwrap_or(name)
}
