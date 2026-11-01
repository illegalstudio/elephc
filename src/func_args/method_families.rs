//! Purpose:
//! Propagates the private variadic tail used by argument introspection across method families.
//!
//! Called from:
//! - `crate::func_args::desugar()` after individual function-like bodies have been rewritten.
//!
//! Key details:
//! - A virtual method family must use one ABI shape even when only one implementation reads
//!   surplus positional arguments.
//! - Relationships come from class inheritance, interface implementation and extension, and
//!   trait use; method names follow PHP's case-insensitive lookup rules.
//! - A family that contains a method with a SOURCE variadic cannot adopt the hidden tail: the
//!   source variadic already owns the tail slot, so its ABI would differ from every sibling's.
//!   `capture_all_opt_outs` names those families so whole-program frame capture can leave every
//!   member on its source signature (the frames just carry no captured arguments).

use std::collections::{HashMap, HashSet, VecDeque};

use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, Program, Stmt, StmtKind, TypeExpr, Visibility};

use super::HIDDEN_ARGS_PARAM;

type MethodCapability = (String, bool);

/// Describes one class-like declaration and the virtual methods that already need a hidden tail.
struct OwnerInfo {
    name: String,
    related: Vec<String>,
    hidden_methods: Vec<MethodCapability>,
    /// Virtual methods declared with a variadic the source wrote.
    source_variadic_methods: Vec<MethodCapability>,
}

/// Widens every declared member of an introspection-aware virtual method family to the same ABI.
pub(super) fn propagate(program: &mut Program) {
    let mut owners = Vec::new();
    collect_owners(program, &mut owners);
    let required = spread_capabilities(&owners, |owner| &owner.hidden_methods);
    apply_requirements(program, &required);
}

/// Names, per owner, the virtual methods whose family holds a source-variadic declaration.
///
/// Whole-program frame capture appends the hidden tail to every method; for these families that
/// would split the ABI between the source-variadic member and the rest, so they are skipped.
pub(super) fn capture_all_opt_outs(
    program: &[Stmt],
) -> HashMap<String, HashSet<MethodCapability>> {
    let mut owners = Vec::new();
    collect_owners(program, &mut owners);
    spread_capabilities(&owners, |owner| &owner.source_variadic_methods)
}

/// Spreads each owner's capabilities over its whole relationship component.
fn spread_capabilities(
    owners: &[OwnerInfo],
    capabilities_of: impl Fn(&OwnerInfo) -> &Vec<MethodCapability>,
) -> HashMap<String, HashSet<MethodCapability>> {
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    for owner in owners {
        adjacency.entry(owner.name.clone()).or_default();
        for related in &owner.related {
            adjacency
                .entry(owner.name.clone())
                .or_default()
                .push(related.clone());
            adjacency
                .entry(related.clone())
                .or_default()
                .push(owner.name.clone());
        }
    }

    let mut required: HashMap<String, HashSet<MethodCapability>> = HashMap::new();
    for owner in owners {
        for capability in capabilities_of(owner) {
            let mut queue = VecDeque::from([owner.name.clone()]);
            let mut visited = HashSet::new();
            while let Some(name) = queue.pop_front() {
                if !visited.insert(name.clone()) {
                    continue;
                }
                required
                    .entry(name.clone())
                    .or_default()
                    .insert(capability.clone());
                if let Some(neighbors) = adjacency.get(&name) {
                    queue.extend(neighbors.iter().cloned());
                }
            }
        }
    }
    required
}

/// Collects class-like declarations recursively from every statement body.
fn collect_owners(stmts: &[Stmt], owners: &mut Vec<OwnerInfo>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::ClassDecl {
                name,
                extends,
                implements,
                trait_uses,
                methods,
                ..
            } => {
                let mut related: Vec<String> = extends
                    .iter()
                    .map(|parent| parent.as_str().to_string())
                    .chain(implements.iter().map(|interface| interface.as_str().to_string()))
                    .collect();
                extend_trait_names(&mut related, trait_uses);
                owners.push(owner_info(name, related, methods));
                collect_method_bodies(methods, owners);
            }
            StmtKind::InterfaceDecl {
                name,
                extends,
                methods,
                ..
            } => {
                let related = extends
                    .iter()
                    .map(|parent| parent.as_str().to_string())
                    .collect();
                owners.push(owner_info(name, related, methods));
                collect_method_bodies(methods, owners);
            }
            StmtKind::TraitDecl {
                name,
                trait_uses,
                methods,
                ..
            } => {
                let mut related = Vec::new();
                extend_trait_names(&mut related, trait_uses);
                owners.push(owner_info(name, related, methods));
                collect_method_bodies(methods, owners);
            }
            StmtKind::EnumDecl {
                name,
                implements,
                trait_uses,
                methods,
                ..
            } => {
                let mut related = implements
                    .iter()
                    .map(|interface| interface.as_str().to_string())
                    .collect();
                extend_trait_names(&mut related, trait_uses);
                owners.push(owner_info(name, related, methods));
                collect_method_bodies(methods, owners);
            }
            StmtKind::FunctionDecl { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::NamespaceBlock { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. } => collect_owners(body, owners),
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_owners(then_body, owners);
                for (_, body) in elseif_clauses {
                    collect_owners(body, owners);
                }
                if let Some(body) = else_body {
                    collect_owners(body, owners);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                collect_owners(then_body, owners);
                if let Some(body) = else_body {
                    collect_owners(body, owners);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => collect_owners(body, owners),
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_owners(body, owners);
                }
                if let Some(body) = default {
                    collect_owners(body, owners);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_owners(try_body, owners);
                for catch in catches {
                    collect_owners(&catch.body, owners);
                }
                if let Some(body) = finally_body {
                    collect_owners(body, owners);
                }
            }
            _ => {}
        }
    }
}

/// Adds every trait named by a use clause to an owner's relationship list.
fn extend_trait_names(related: &mut Vec<String>, trait_uses: &[crate::parser::ast::TraitUse]) {
    related.extend(
        trait_uses
            .iter()
            .flat_map(|trait_use| trait_use.trait_names.iter())
            .map(|name| name.as_str().to_string()),
    );
}

/// Builds the propagation metadata for one class-like declaration.
fn owner_info(name: &str, related: Vec<String>, methods: &[ClassMethod]) -> OwnerInfo {
    let hidden_methods = methods
        .iter()
        .filter(|method| {
            method.visibility != Visibility::Private
                && php_symbol_key(&method.name) != "__construct"
                && method.variadic.as_deref() == Some(HIDDEN_ARGS_PARAM)
        })
        .map(|method| (php_symbol_key(&method.name), method.is_static))
        .collect();
    let source_variadic_methods = methods
        .iter()
        .filter(|method| {
            method.visibility != Visibility::Private
                && php_symbol_key(&method.name) != "__construct"
                && method
                    .variadic
                    .as_deref()
                    .is_some_and(|variadic| variadic != HIDDEN_ARGS_PARAM)
        })
        .map(|method| (php_symbol_key(&method.name), method.is_static))
        .collect();
    OwnerInfo {
        name: name.to_string(),
        related,
        hidden_methods,
        source_variadic_methods,
    }
}

/// Recurses into method bodies because PHP permits nested declarations there.
fn collect_method_bodies(methods: &[ClassMethod], owners: &mut Vec<OwnerInfo>) {
    for method in methods {
        collect_owners(&method.body, owners);
    }
}

/// Applies component-wide hidden-tail requirements to every matching declaration.
fn apply_requirements(
    stmts: &mut [Stmt],
    required: &HashMap<String, HashSet<MethodCapability>>,
) {
    for stmt in stmts {
        match &mut stmt.kind {
            StmtKind::ClassDecl { name, methods, .. }
            | StmtKind::InterfaceDecl { name, methods, .. }
            | StmtKind::TraitDecl { name, methods, .. }
            | StmtKind::EnumDecl { name, methods, .. } => {
                if let Some(capabilities) = required.get(name) {
                    for method in methods.iter_mut() {
                        let capability = (php_symbol_key(&method.name), method.is_static);
                        if method.visibility != Visibility::Private
                            && capabilities.contains(&capability)
                        {
                            add_hidden_tail(method);
                        }
                    }
                }
                for method in methods {
                    apply_requirements(&mut method.body, required);
                }
            }
            StmtKind::FunctionDecl { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::NamespaceBlock { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. } => apply_requirements(body, required),
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                apply_requirements(then_body, required);
                for (_, body) in elseif_clauses {
                    apply_requirements(body, required);
                }
                if let Some(body) = else_body {
                    apply_requirements(body, required);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                apply_requirements(then_body, required);
                if let Some(body) = else_body {
                    apply_requirements(body, required);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => apply_requirements(body, required),
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    apply_requirements(body, required);
                }
                if let Some(body) = default {
                    apply_requirements(body, required);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                apply_requirements(try_body, required);
                for catch in catches {
                    apply_requirements(&mut catch.body, required);
                }
                if let Some(body) = finally_body {
                    apply_requirements(body, required);
                }
            }
            _ => {}
        }
    }
}

/// Adds the compiler-private mixed variadic tail unless the source already has a variadic.
fn add_hidden_tail(method: &mut ClassMethod) {
    if method.variadic.is_some() {
        return;
    }
    method.variadic = Some(HIDDEN_ARGS_PARAM.to_string());
    method.variadic_type = Some(TypeExpr::Named(crate::names::Name::unqualified("mixed")));
    if method.param_attributes.len() == method.params.len() {
        method.param_attributes.push(Vec::new());
    }
}
