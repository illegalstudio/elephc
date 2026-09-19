//! Purpose:
//! Coordinates conservative whole-program declaration reachability after AST DCE.
//! Owns the public pruning options and the AST/metadata reconciliation boundary.
//!
//! Called from:
//! - `crate::pipeline::compile()` and codegen fixture pipelines before EIR lowering.
//!
//! Key details:
//! - The pass removes declarations only after a fixed-point reachability proof.
//! - The AST and `CheckResult` are pruned together so lowering cannot resurrect dead methods.

use std::collections::HashSet;

use crate::parser::ast::Program;
use crate::types::CheckResult;

pub mod graph;
pub mod inventory;
mod prune;
mod reconcile;
pub mod usage;

pub use inventory::PreludeInventory;

/// Inputs that add compiler-owned roots to the declaration graph.
pub struct PruneOptions<'a> {
    pub inventory: &'a PreludeInventory,
    pub forced_groups: &'a HashSet<String>,
    pub exported_functions: &'a HashSet<String>,
    pub eval_forced: bool,
}

/// Removes unreachable declarations and reconciles all checker metadata consumed by EIR lowering.
pub fn prune_unreachable_declarations(
    program: Program,
    check_result: &mut CheckResult,
    options: PruneOptions<'_>,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        prune_on_compiler_stack(program, check_result, options)
    })
}

/// The pruning body, running on the stack its entry point established.
fn prune_on_compiler_stack(
    program: Program,
    check_result: &mut CheckResult,
    options: PruneOptions<'_>,
) -> Program {
    let original_builtin_libraries = usage::scan_program(&program).required_libraries;
    let reachability = graph::compute(&program, check_result, &options);
    let declaration_index = graph::DeclarationIndex::build(&program, check_result);
    let program = prune::program(program, &reachability);
    let remaining_builtin_libraries = usage::scan_program(&program).required_libraries;
    reconcile::check_result(
        check_result,
        &reachability,
        &declaration_index,
        &original_builtin_libraries,
        &remaining_builtin_libraries,
    );
    program
}

#[cfg(test)]
mod tests;
