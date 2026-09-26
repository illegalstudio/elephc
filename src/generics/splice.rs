//! Purpose:
//! Splices one monomorphic declaration per requested generic instantiation into the program.
//!
//! Called from:
//! - `crate::pipeline::compile` (between type checking and lowering)
//!
//! Key details:
//! - Lowering walks the AST, not the checker's tables, so an instantiation that exists only
//!   inside the checker is invisible to codegen. This pass is what makes it real.
//! - The spliced declaration is an ORDINARY `FunctionDecl` with no type parameters, so every
//!   pass after this one handles it without knowing generics exist.

use crate::generics::{instantiated_name, Bindings};
use crate::parser::ast::{Program, Stmt, StmtKind};

/// Appends a monomorphic declaration for every instantiation in `requested` that the program
/// does not already carry, and returns the new program plus how many were added.
///
/// A zero count is the fixpoint signal: no call site asked for anything the program does not
/// already declare, so re-checking would find nothing new.
pub fn splice_instantiations(
    program: Program,
    requested: &[(String, Bindings)],
) -> (Program, usize) {
    if requested.is_empty() {
        return (program, 0);
    }
    let templates: Vec<(String, Stmt)> = program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl {
                name, type_params, ..
            } if !type_params.is_empty() => Some((name.clone(), stmt.clone())),
            _ => None,
        })
        .collect();
    if templates.is_empty() {
        return (program, 0);
    }
    let mut declared: Vec<String> = program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();

    let mut program = program;
    let mut added = 0usize;
    for (base, bindings) in requested {
        let target = instantiated_name(base, bindings);
        if declared.iter().any(|name| name == &target) {
            continue;
        }
        let Some((_, template)) = templates.iter().find(|(name, _)| name == base) else {
            continue;
        };
        let Some(instance) = instantiate_declaration(template, &target, bindings) else {
            continue;
        };
        declared.push(target);
        program.push(instance);
        added += 1;
    }
    (program, added)
}

/// Substitutes type parameters at every type position the AST walker reaches.
///
/// Reuses `magic_constants::walker`, whose statement and expression matches carry no wildcard
/// arm, so a type parameter is substituted wherever it appears — a typed local, a
/// `buffer<T>` element type, a nested closure's parameter or return type — and not merely in
/// the signature. That exhaustiveness is the whole reason to borrow this walker rather than
/// write a second one: the walk that misses a node leaves `T` behind for the checker to report
/// as an unknown type.
struct TypeParamSubstitution<'b> {
    bindings: &'b Bindings,
}

impl crate::magic_constants::walker::Pass for TypeParamSubstitution<'_> {
    /// Magic constants are already substituted by the time a template is instantiated, so this
    /// pass leaves them exactly as it found them.
    fn transform_magic(
        &self,
        _span: crate::span::Span,
        mc: crate::parser::ast::MagicConstant,
    ) -> crate::parser::ast::ExprKind {
        crate::parser::ast::ExprKind::MagicConstant(mc)
    }

    fn transform_type(
        &self,
        ty: crate::parser::ast::TypeExpr,
        _span: crate::span::Span,
    ) -> crate::parser::ast::TypeExpr {
        ty.substitute_type_params(self.bindings)
    }
}

/// Substitutes type parameters at every type position inside `body`.
///
/// The checker and the AST splice both instantiate the same template and must produce the same
/// body, or the signature the checker resolved would describe a function the backend does not
/// emit. One helper, used by both, is what keeps them in step.
pub fn substitute_in_body(body: Vec<Stmt>, bindings: &Bindings) -> Vec<Stmt> {
    crate::magic_constants::walker::walk_program(
        body,
        &mut TypeParamSubstitution { bindings },
    )
}

/// Clones a template declaration under `target`, substituting its type parameters everywhere.
fn instantiate_declaration(template: &Stmt, target: &str, bindings: &Bindings) -> Option<Stmt> {
    let StmtKind::FunctionDecl {
        params,
        param_attributes,
        variadic,
        variadic_by_ref,
        variadic_type,
        return_type,
        by_ref_return,
        body,
        ..
    } = &template.kind
    else {
        return None;
    };
    let mut instance = template.clone();
    instance.kind = StmtKind::FunctionDecl {
        name: target.to_string(),
        type_params: Vec::new(),
        params: params
            .iter()
            .map(|(name, declared, default, by_ref)| {
                (
                    name.clone(),
                    declared
                        .as_ref()
                        .map(|ty| ty.substitute_type_params(bindings)),
                    default.clone(),
                    *by_ref,
                )
            })
            .collect(),
        param_attributes: param_attributes.clone(),
        variadic: variadic.clone(),
        variadic_by_ref: *variadic_by_ref,
        variadic_type: variadic_type
            .as_ref()
            .map(|ty| ty.substitute_type_params(bindings)),
        return_type: return_type
            .as_ref()
            .map(|ty| ty.substitute_type_params(bindings)),
        by_ref_return: *by_ref_return,
        // The body goes through the exhaustive walker, so a type parameter named in a typed
        // local, a `buffer<T>`, or a nested closure's signature is substituted too — not only
        // the ones in this declaration's own parameter and return types.
        body: substitute_in_body(body.clone(), bindings),
    };
    Some(instance)
}

/// Returns the instantiated function names the program now declares.
///
/// Reachability prunes a declaration nothing references, and no call site names an
/// instantiation: the source still says `identity(5)`, and lowering is what resolves that to
/// `identity<int>`. Each instantiation exists BECAUSE a call site asked for it, so it is
/// reachable by construction, and these names are handed to the pruner as roots.
pub fn instantiation_roots(requested: &[(String, Bindings)]) -> Vec<String> {
    let mut roots: Vec<String> = requested
        .iter()
        .map(|(base, bindings)| instantiated_name(base, bindings))
        .collect();
    roots.sort();
    roots.dedup();
    roots
}

/// Removes every generic template declaration from the program.
///
/// A template is not a function: its annotations name types that have no representation, so
/// lowering must never see it. Called once the fixpoint has produced every instantiation the
/// program reaches.
pub fn strip_templates(program: Program) -> Program {
    program
        .into_iter()
        .filter(|stmt| {
            !matches!(
                &stmt.kind,
                StmtKind::FunctionDecl { type_params, .. } if !type_params.is_empty()
            )
        })
        .collect()
}
