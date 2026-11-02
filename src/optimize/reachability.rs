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
    // These two stay STD-hashed on purpose. They are inputs handed in by the pipeline and are read
    // a handful of times, not keys in the fixed point's hot path, so there is nothing to gain by
    // making every caller build them with the fast hasher — and this is a public signature.
    pub forced_groups: &'a std::collections::HashSet<String>,
    pub exported_functions: &'a std::collections::HashSet<String>,
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
    // This pass is 14.3% of a Symfony `--web` build (45.10 s of 314.57 s), which is as much as
    // type checking, and it is SIX steps. `ELEPHC_DECL_REACH_TIMES=1` says which one.
    let trace = std::env::var("ELEPHC_DECL_REACH_TIMES").is_ok();
    let mut mark = std::time::Instant::now();
    let step = |label: &str, mark: &mut std::time::Instant| {
        if trace {
            eprintln!("[elephc-decl-reach] {label}={:.2}s", mark.elapsed().as_secs_f64());
        }
        *mark = std::time::Instant::now();
    };

    let original_builtin_libraries = usage::scan_program(&program).required_libraries;
    step("scan_before", &mut mark);
    let reachability = graph::compute(&program, check_result, &options);
    step("graph_compute", &mut mark);
    let declaration_index = graph::DeclarationIndex::build(&program, check_result);
    step("declaration_index", &mut mark);
    let program = prune::program(program, &reachability);
    step("prune", &mut mark);
    let remaining_builtin_libraries = usage::scan_program(&program).required_libraries;
    step("scan_after", &mut mark);
    reconcile::check_result(
        check_result,
        &reachability,
        &declaration_index,
        &original_builtin_libraries,
        &remaining_builtin_libraries,
    );
    step("reconcile", &mut mark);
    program
}

#[cfg(test)]
mod tests;
