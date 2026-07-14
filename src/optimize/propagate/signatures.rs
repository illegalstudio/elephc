//! Purpose:
//! Program-wide by-ref parameter signature pre-scan for constant propagation:
//! which caller locals a call can write through by-ref parameters, and whether
//! any constructor can bind an argument by reference.
//!
//! Called from:
//! - `crate::optimize::propagate_constants()` (collect + install) and the
//!   targeted invalidation analysis (queries).
//!
//! Key details:
//! - Method summaries union by-ref positions across every same-named method
//!   (classes, traits, enums, interfaces) so dynamic dispatch stays safe.
//! - `function_by_ref_params` falls back to the builtin registry, which is
//!   static and therefore resolvable even without an installed scan.
//! - Function-variant groups union their variants' signatures: a call through
//!   the group name must stay safe whichever variant is linked.

use super::*;

thread_local! {
    /// The by-ref signature scan for the program currently being propagated.
    static ACTIVE_BY_REF_SIGNATURES: RefCell<Option<ByRefSignatures>> = const { RefCell::new(None) };
}

/// Per-program by-ref parameter summaries feeding targeted call invalidation.
#[derive(Debug, Default)]
pub(crate) struct ByRefSignatures {
    /// User `FunctionDecl` signatures: name → `(param name, is_by_ref)` list.
    functions: HashMap<String, Vec<(String, bool)>>,
    /// Union of same-named method signatures across all declarations.
    methods_by_name: HashMap<String, Vec<(String, bool)>>,
    /// True when any constructor parameter or declared property is by-ref, so
    /// `new` can bind an argument by reference.
    any_ctor_by_ref: bool,
}

/// Walks the program and collects user function/method by-ref signatures and
/// the constructor by-ref flag.
pub(crate) fn collect_by_ref_signatures(program: &[Stmt]) -> ByRefSignatures {
    let mut sigs = ByRefSignatures::default();
    collect_from_block(program, &mut sigs);
    // A call through a variant group name can reach any linked variant, so the
    // group takes the union of its variants' signatures.
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    collect_variant_groups(program, &mut groups);
    for (name, variants) in groups {
        let mut union: Vec<(String, bool)> = Vec::new();
        for variant in &variants {
            if let Some(params) = sigs.functions.get(variant) {
                union_params(&mut union, params);
            }
        }
        sigs.functions.insert(name, union);
    }
    sigs
}

/// Installs `sigs` as the active scan for the duration of `f`, restoring the
/// previous scan afterwards.
pub(crate) fn with_by_ref_signatures<R>(sigs: ByRefSignatures, f: impl FnOnce() -> R) -> R {
    ACTIVE_BY_REF_SIGNATURES.with(|slot| {
        let previous = slot.replace(Some(sigs));
        let result = f();
        slot.replace(previous);
        result
    })
}

/// Returns the `(param name, is_by_ref)` list for a named function call: the
/// user scan first, then the builtin registry. `None` means unknown symbol.
pub(crate) fn function_by_ref_params(name: &str) -> Option<Vec<(String, bool)>> {
    let user = ACTIVE_BY_REF_SIGNATURES.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|sigs| sigs.functions.get(name).cloned())
    });
    user.or_else(|| {
        crate::builtins::registry::lookup(name).map(|def| {
            def.params
                .iter()
                .zip(def.ref_params.iter())
                .map(|((param, _), by_ref)| (param.clone(), *by_ref))
                .collect()
        })
    })
}

/// Returns the unioned `(param name, is_by_ref)` list across every method with
/// this name, or `None` when no declaration exists.
pub(crate) fn method_by_ref_params(name: &str) -> Option<Vec<(String, bool)>> {
    ACTIVE_BY_REF_SIGNATURES.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|sigs| sigs.methods_by_name.get(name).cloned())
    })
}

/// Returns true when any constructor parameter or property is by-ref, so a
/// `new` expression can bind one of its arguments by reference.
pub(crate) fn any_ctor_by_ref() -> bool {
    ACTIVE_BY_REF_SIGNATURES.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|sigs| sigs.any_ctor_by_ref)
    })
}

/// Returns true when `name` is a user-declared function (by-ref arguments to
/// user callees may be retained past the call, unlike builtin arguments).
pub(crate) fn is_user_function(name: &str) -> bool {
    ACTIVE_BY_REF_SIGNATURES.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|sigs| sigs.functions.contains_key(name))
    })
}

/// Element-wise ORs `params`' by-ref flags into `union`, extending it when
/// `params` is longer.
fn union_params(union: &mut Vec<(String, bool)>, params: &[(String, bool)]) {
    for (index, (name, by_ref)) in params.iter().enumerate() {
        match union.get_mut(index) {
            Some((_, merged)) => *merged = *merged || *by_ref,
            None => union.push((name.clone(), *by_ref)),
        }
    }
}

/// Converts an AST parameter tuple list into `(param name, is_by_ref)` pairs.
fn params_signature_with_variadic(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    variadic: Option<&str>,
    variadic_by_ref: bool,
) -> Vec<(String, bool)> {
    let mut signature: Vec<_> = params
        .iter()
        .map(|(name, _, _, is_ref)| (name.clone(), *is_ref))
        .collect();
    if let Some(variadic) = variadic {
        signature.push((variadic.to_string(), variadic_by_ref));
    }
    signature
}

/// Records one method declaration into the by-name union and the ctor flag.
fn collect_method(method: &ClassMethod, sigs: &mut ByRefSignatures) {
    let signature = params_signature_with_variadic(
        &method.params,
        method.variadic.as_deref(),
        method.variadic_by_ref,
    );
    if method.name.eq_ignore_ascii_case("__construct")
        && signature.iter().any(|(_, by_ref)| *by_ref)
    {
        sigs.any_ctor_by_ref = true;
    }
    match sigs.methods_by_name.get_mut(&method.name) {
        Some(existing) => union_params_owned(existing, signature),
        None => {
            sigs.methods_by_name.insert(method.name.clone(), signature);
        }
    }
}

/// Element-wise ORs an owned signature into an existing union entry.
fn union_params_owned(union: &mut Vec<(String, bool)>, params: Vec<(String, bool)>) {
    for (index, (name, by_ref)) in params.into_iter().enumerate() {
        match union.get_mut(index) {
            Some((_, merged)) => *merged = *merged || by_ref,
            None => union.push((name, by_ref)),
        }
    }
}

/// Records by-ref properties (bindable through `new`) into the ctor flag.
fn collect_properties(properties: &[ClassProperty], sigs: &mut ByRefSignatures) {
    if properties.iter().any(|property| property.by_ref) {
        sigs.any_ctor_by_ref = true;
    }
}

/// Recursively collects declarations from a statement block.
fn collect_from_block(body: &[Stmt], sigs: &mut ByRefSignatures) {
    for stmt in body {
        match &stmt.kind {
            StmtKind::FunctionDecl {
                name,
                params,
                variadic,
                variadic_by_ref,
                ..
            } => {
                sigs.functions.insert(
                    name.clone(),
                    params_signature_with_variadic(params, variadic.as_deref(), *variadic_by_ref),
                );
            }
            StmtKind::ClassDecl {
                properties,
                methods,
                ..
            }
            | StmtKind::TraitDecl {
                properties,
                methods,
                ..
            }
            | StmtKind::InterfaceDecl {
                properties,
                methods,
                ..
            } => {
                collect_properties(properties, sigs);
                for method in methods {
                    collect_method(method, sigs);
                }
            }
            StmtKind::EnumDecl { methods, .. } => {
                for method in methods {
                    collect_method(method, sigs);
                }
            }
            StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) => {
                collect_from_block(body, sigs);
            }
            StmtKind::IncludeOnceGuard { body, .. } => collect_from_block(body, sigs),
            _ => {}
        }
    }
}

/// Recursively collects `FunctionVariantGroup` name/variant pairs.
fn collect_variant_groups(body: &[Stmt], groups: &mut Vec<(String, Vec<String>)>) {
    for stmt in body {
        match &stmt.kind {
            StmtKind::FunctionVariantGroup { name, variants } => {
                groups.push((name.clone(), variants.clone()));
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => collect_variant_groups(body, groups),
            _ => {}
        }
    }
}
