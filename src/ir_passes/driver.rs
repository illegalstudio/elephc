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
//! - `optimize_module` runs the whole EIR pipeline to a module-level fixed point:
//!   each round runs the cross-function small-function inliner (`super::inline`)
//!   and then the per-function passes (each driven to its own fixed point), and
//!   the round repeats until neither the inliner nor any function pass reports a
//!   change. This lets the two layers feed each other — inlining exposes constants
//!   and dead code for the function passes, and the function passes shrink callees
//!   (or expose calls) so the next round can inline more. The first round
//!   reproduces the previous "inline once, then optimize" behavior; later rounds
//!   only add optimization, never change semantics.
//! - Validation after each pass and the per-function non-convergence panic are
//!   gated on `debug_assertions`, so they are active in `cargo build`/`cargo test`
//!   and compile out of `--release`. In release, hitting either iteration cap
//!   simply stops and proceeds with the current IR.

use crate::ir::{DataPool, Function, Module};

use super::branch_simplify::BranchSimplify;
use super::const_fold::ConstFold;
use super::cse::Cse;
use super::dead_inst::DeadInst;
use super::dead_store::DeadStore;
use super::identity_arith::IdentityArith;
use super::licm::Licm;
use super::peephole::Peephole;

/// Maximum fixed-point sweeps before the driver gives up on a function. Real
/// passes are idempotent and converge in a couple of sweeps; exceeding this cap
/// indicates a non-converging pass bug.
const MAX_PASS_ITERATIONS: usize = 64;

/// Maximum module-level rounds of `inline → per-function passes` before
/// `optimize_module` stops. Inlining over the acyclic candidate call graph plus
/// the monotonically-simplifying function passes converge in a few rounds (deep
/// inline chains need one round per call-graph level); the cap is a generous
/// backstop, after which the current IR is kept as-is.
const MAX_MODULE_ITERATIONS: usize = 10;

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

/// Builds the ordered set of transformation passes run on every function:
/// identity arithmetic folding, peephole rewrites, constant folding,
/// common-subexpression elimination, loop-invariant code motion, dead
/// instruction elimination, dead store elimination, and branch simplification.
/// The cross-function small-function inliner is not a member here; it runs as a
/// module-level phase in `optimize_module`, interleaved with these passes.
///
/// Constant folding runs after peephole so the scalar load/store forwarding has
/// already moved constants stored to local slots onto their `load_local` uses,
/// exposing constant-operand operations for it to fold. CSE then runs after
/// folding so it deduplicates pure computations over the already-canonicalized
/// constants (the constants themselves are left for the backend to
/// rematerialize). LICM then hoists loop-invariant pure computations into loop
/// preheaders. The redundant or relocated instructions these leave behind are
/// cleaned up by dead instruction elimination, and any folded branch condition is
/// collapsed by branch simplification — all converging through the fixed-point
/// loop.
fn default_passes() -> Vec<Box<dyn IrPass>> {
    vec![
        Box::new(IdentityArith),
        Box::new(Peephole),
        Box::new(ConstFold),
        Box::new(Cse),
        Box::new(Licm),
        Box::new(DeadInst),
        Box::new(DeadStore),
        Box::new(BranchSimplify),
    ]
}

/// Runs the whole EIR optimization pipeline over the module to a module-level
/// fixed point.
///
/// Each round runs the cross-function small-function inliner and then drives the
/// per-function passes to their own fixed point on every function-like body, and
/// the round repeats while either layer reports a change. Interleaving lets the
/// inliner and the function passes feed each other: inlined bodies expose new
/// constants/dead code, and the simplified functions expose new (smaller) inline
/// candidates. The first round reproduces the prior "inline once, then optimize"
/// behavior; later rounds only optimize further, never changing semantics, so the
/// result is identical with `--ir-opt` on or off except for performance.
///
/// Inside each round the module is destructured so the function tables and the
/// shared literal `data` pool are borrowed disjointly. The combined process
/// converges (inlining is bounded over the acyclic candidate graph; the function
/// passes only simplify), with `MAX_MODULE_ITERATIONS` as a backstop.
pub fn optimize_module(module: &mut Module) {
    let passes = default_passes();

    for _ in 0..MAX_MODULE_ITERATIONS {
        let mut changed = false;

        // Module-level cross-function inliner.
        changed |= super::inline::inline_small_functions(module);
        #[cfg(debug_assertions)]
        if let Err(e) = crate::ir::validate_module(module) {
            panic!("inline_small_functions produced invalid module: {:?}", e);
        }

        // Per-function fixed-point passes over every function-like body.
        if !passes.is_empty() {
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
                changed |= run_function_passes(function, &passes, data);
            }
        }

        if !changed {
            return; // module-level fixed point reached
        }
    }
    // Module-level non-convergence: keep the current (valid, more-optimized) IR.
    // Each per-function run still converged; only the inline/simplify interleave
    // hit the round cap, which is a generous backstop for deep inline chains.
}

/// Runs the given passes over one function to a fixed point, returning whether the
/// function was modified at all (so the module-level loop can detect convergence).
/// After each pass, in debug/test builds, the function is re-validated and any
/// malformed IR panics naming the offending pass. Non-convergence within the cap
/// panics in debug and stops (keeping current IR) in release.
pub fn run_function_passes(
    function: &mut Function,
    passes: &[Box<dyn IrPass>],
    data: &mut DataPool,
) -> bool {
    let mut modified = false;
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
            return modified;
        }
        modified = true;
    }
    #[cfg(debug_assertions)]
    {
        panic!(
            "EIR pass driver did not reach a fixed point for function '{}' after {} iterations",
            function.name, MAX_PASS_ITERATIONS
        );
    }
    #[cfg(not(debug_assertions))]
    {
        modified
    }
}
