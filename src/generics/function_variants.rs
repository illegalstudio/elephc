//! Purpose:
//! Specializes include-dispatch metadata once generic function binding reaches its fixpoint.
//!
//! Called from:
//! - `crate::generics::monomorphize`, before template removal.
//!
//! Key details:
//! - Each concrete public function gets its own dispatcher and activation marks.
//! - Marks stay at their original runtime positions, including inside deferred includes.

use std::collections::{HashMap, HashSet};

use crate::generics::{instantiated_name, Bindings};
use crate::magic_constants::walker::{walk_program, Pass};
use crate::parser::ast::{ExprKind, MagicConstant, Program, Stmt, StmtKind};
use crate::span::Span;
use crate::types::CheckResult;

/// Specializes dispatch metadata in the AST and the checker's retained method bodies.
pub(super) fn specialize(program: Program, check_result: &mut CheckResult) -> Program {
    let templates: HashSet<_> = program.iter().filter_map(|stmt| match &stmt.kind {
        StmtKind::FunctionDecl { name, type_params, .. } if !type_params.is_empty() => {
            Some(name.as_str())
        }
        _ => None,
    }).collect();
    let mut groups = HashMap::new();
    for stmt in &program {
        let StmtKind::FunctionVariantGroup { name, variants } = &stmt.kind else { continue };
        if !variants.iter().any(|variant| templates.contains(variant.as_str())) { continue; }
        let mut bindings = Vec::new();
        for (base, arguments) in &check_result.requested_instantiations {
            if base == name && !bindings.contains(arguments) { bindings.push(arguments.clone()); }
        }
        groups.insert(name.clone(), bindings);
    }
    if groups.is_empty() { return program; }
    let mut pass = Specialize { groups };
    let program = walk_program(program, &mut pass);
    // EIR lowers class and enum methods from these checker copies, including flattened
    // trait methods. Rewriting only the AST would leave marks naming stripped templates.
    for class in check_result.classes.values_mut() {
        for method in &mut class.method_decls {
            method.body = walk_program(std::mem::take(&mut method.body), &mut pass);
        }
    }
    program
}

/// Carries the requested concrete type bindings for every generic public dispatcher.
struct Specialize {
    groups: HashMap<String, Vec<Bindings>>,
}

impl Pass for Specialize {
    /// Preserves magic constants, which earlier pipeline phases have already substituted.
    fn transform_magic(&self, _span: Span, mc: MagicConstant) -> ExprKind {
        ExprKind::MagicConstant(mc)
    }

    /// Expands only generic dispatch metadata while retaining each statement's source profile.
    fn transform_stmt(&self, stmt: Stmt) -> Stmt {
        let (name, variants, mark) = match &stmt.kind {
            StmtKind::FunctionVariantGroup { name, variants } => (name, variants.clone(), false),
            StmtKind::FunctionVariantMark { name, variant } => (name, vec![variant.clone()], true),
            _ => return stmt,
        };
        let Some(bindings) = self.groups.get(name) else { return stmt };
        let concrete = bindings.iter().map(|arguments| {
            let name = instantiated_name(name, arguments);
            let variants: Vec<_> = variants.iter()
                .map(|variant| instantiated_name(variant, arguments)).collect();
            let kind = if mark {
                StmtKind::FunctionVariantMark { name, variant: variants[0].clone() }
            } else {
                StmtKind::FunctionVariantGroup { name, variants }
            };
            Stmt { kind, ..stmt.clone() }
        }).collect();
        Stmt { kind: StmtKind::Synthetic(concrete), ..stmt }
    }
}
