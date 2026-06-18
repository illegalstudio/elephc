//! Purpose:
//! Fixed-point driver for mutating EIR transformation passes. Runs the
//! registered passes over each function repeatedly until none reports a change,
//! re-validating the function after every pass in debug/test builds.
//!
//! Called from:
//! - `crate::pipeline::compile()` via `optimize_module`, after AST-to-EIR
//!   lowering and before codegen, for the EIR backend.
//!
//! Key details:
//! - Passes operate on a single `Function`; `optimize_module` drives every
//!   function-like body in the module. A future cross-function pass (e.g. an
//!   inliner) would add its own module-level phase instead of using this loop.
//! - Validation after each pass and the non-convergence panic are gated on
//!   `debug_assertions`, so they are active in `cargo build`/`cargo test` and
//!   compile out of `--release` builds — exactly "validation after each pass in
//!   test builds". In release, hitting the iteration cap simply stops and
//!   proceeds with the current IR.

use crate::ir::{DataPool, Function, Module};

use super::dead_inst::DeadInst;
use super::identity_arith::IdentityArith;
use super::peephole::Peephole;

/// Maximum fixed-point sweeps before the driver gives up on a function. Real
/// passes are idempotent and converge in a couple of sweeps; exceeding this cap
/// indicates a non-converging pass bug.
const MAX_PASS_ITERATIONS: usize = 64;

/// A mutating EIR transformation pass over a single function.
pub trait IrPass {
    /// Returns the stable, human-readable pass name used in diagnostics. Only
    /// consumed by the debug-build validation/non-convergence panics, so it is
    /// dead in `--release` where those guards compile out.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    fn name(&self) -> &'static str;

    /// Runs the pass over one function, returning true if it changed the IR.
    /// `data` is the module's shared literal pool, used by passes that materialize
    /// new constants (e.g. peephole string-literal concat folding interns the
    /// folded string); passes that need no new literals ignore it.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool;
}

/// Builds the ordered set of transformation passes run on every function. Later
/// v0.25.x passes (DCE, branch simplification, CSE, LICM, …) register here.
fn default_passes() -> Vec<Box<dyn IrPass>> {
    vec![
        Box::new(IdentityArith),
        Box::new(Peephole),
        Box::new(DeadInst),
    ]
}

/// Runs the default pass pipeline over every function-like body in the module.
///
/// The module is destructured so the function tables and the shared literal
/// `data` pool can be borrowed disjointly: each function is mutated in place
/// while passes intern new literals into the same pool.
pub fn optimize_module(module: &mut Module) {
    let passes = default_passes();
    if passes.is_empty() {
        return;
    }
    let Module {
        functions,
        class_methods,
        closures,
        fiber_wrappers,
        callback_wrappers,
        extern_callback_trampolines,
        runtime_callable_invokers,
        data,
        ..
    } = module;
    let all_functions = functions
        .iter_mut()
        .chain(class_methods.iter_mut())
        .chain(closures.iter_mut())
        .chain(fiber_wrappers.iter_mut())
        .chain(callback_wrappers.iter_mut())
        .chain(extern_callback_trampolines.iter_mut())
        .chain(runtime_callable_invokers.iter_mut());
    for function in all_functions {
        run_function_passes(function, &passes, data);
    }
}

/// Runs the given passes over one function to a fixed point. After each pass, in
/// debug/test builds, the function is re-validated and any malformed IR panics
/// naming the offending pass. Non-convergence within the cap panics in debug and
/// stops (keeping current IR) in release.
pub fn run_function_passes(
    function: &mut Function,
    passes: &[Box<dyn IrPass>],
    data: &mut DataPool,
) {
    for _ in 0..MAX_PASS_ITERATIONS {
        let mut changed = false;
        for pass in passes {
            let pass_changed = pass.run(function, data);
            #[cfg(debug_assertions)]
            if let Err(error) = crate::ir::validate_function(function) {
                panic!(
                    "EIR pass '{}' produced invalid IR in function '{}': {:?}",
                    pass.name(),
                    function.name,
                    error
                );
            }
            changed |= pass_changed;
        }
        if !changed {
            return;
        }
    }
    #[cfg(debug_assertions)]
    panic!(
        "EIR pass driver did not reach a fixed point for function '{}' after {} iterations",
        function.name, MAX_PASS_ITERATIONS
    );
}
