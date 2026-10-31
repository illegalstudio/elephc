//! Purpose:
//! Fixed-point driver for mutating EIR transformation passes. Runs the
//! applicable registered passes over each function repeatedly until none reports
//! a change, re-validating the function after every pass that runs in debug/test builds.
//!
//! Called from:
//! - `crate::pipeline::compile()` via `optimize_module`, after AST-to-EIR
//!   lowering and before codegen, for the EIR backend.
//!
//! Key details:
//! - Preparation first removes proven redundant receiver leases while their original
//!   lowering shape is intact, before any cross-function inlining.
//! - `optimize_module` runs the whole EIR pipeline to a module-level fixed point:
//!   each round runs the cross-function small-function inliner (`super::inline`)
//!   and then the per-function passes (each driven to its own fixed point), and
//!   the round repeats until neither the inliner nor any function pass reports a
//!   change. This lets the two layers feed each other: inlining exposes constants
//!   and dead code for the function passes, and the function passes shrink callees
//!   (or expose calls) so the next round can inline more. The first round
//!   reproduces the previous "inline once, then optimize" behavior; later rounds
//!   only add optimization, never change semantics.
//! - Validation after each applicable pass and the per-function non-convergence
//!   panic are gated on `debug_assertions`, so they are active in `cargo build`/
//!   `cargo test` and compile out of `--release`. In release, hitting either
//!   iteration cap simply stops and proceeds with the current IR.
//! - Tail recursion lowering follows that fixed point and reruns scalar passes on its loops.
//!   Ownership finalizers run only after these phases. Their rewrites must
//!   not expose new integer sinks to the earlier representation-changing passes.

use crate::ir::{DataPool, Function, Module};

use super::array_fill_loop::ArrayFillLoop;
use super::array_get_bounds::ArrayGetBounds;
use super::abs_int_sink::AbsIntSink;
use super::string_len_sink::StringLenSink;
use super::json_decode_count_sink::JsonDecodeCountSink;
use super::json_encode_len_sink::JsonEncodeLenSink;
use super::sqrt_add_int_sink::SqrtAddIntSink;
use super::count_chars_count_sink::CountCharsCountSink;
use super::str_split_count_sink::StrSplitCountSink;
use super::parse_url_count_sink::ParseUrlCountSink;
use super::inet_pton_len_sink::InetPtonLenSink;
use super::strip_tags_len_sink::StripTagsLenSink;
use super::strpos_int_sink::StrposIntSink;
use super::base64_decode_len_sink::Base64DecodeLenSink;
use super::mb_catalog_count_sink::MbCatalogCountSink;
use super::mb_detect_order_single_sink::MbDetectOrderSingleSink;
use super::mb_numericentity_len_sink::MbNumericentityLenSink;
use super::ereg_search_array_result_sink::EregSearchArrayResultSink;
use super::ereg_search_literal_state_sink::EregSearchLiteralStateSink;
use super::gz_len_hoist::GzLenHoist;
use super::gz_literal_roundtrip_len_sink::GzLiteralRoundtripLenSink;
use super::hash_algos_count_hoist::HashAlgosCountHoist;
use super::hash_const_sink::HashConstSink;
use super::iconv_get_encoding_len_sink::IconvGetEncodingLenSink;
use super::iconv_mime_literal_sink::IconvMimeLiteralSink;
use super::iconv_position_const_sink::IconvPositionConstSink;
use super::mb_convert_variables_len_sink::MbConvertVariablesLenSink;
use super::mb_parse_str_literal_sink::MbParseStrLiteralSink;
use super::mb_strlen_const_local_sink::MbStrlenConstLocalSink;
use super::openssl_cbc_len_sink::OpensslCbcLenSink;
use super::openssl_cipher_iv_length_const_sink::OpensslCipherIvLengthConstSink;
use super::mb_strstr_len_const_sink::MbStrstrLenConstSink;
use super::mb_strimwidth_len_sink::MbStrimwidthLenSink;
use super::strstr_len_sink::StrstrLenSink;
use super::token_get_all_count_hoist::TokenGetAllCountHoist;
use super::array_map_sum_add_int_sink::ArrayMapSumAddIntSink;
use super::array_filter_count_hoist::ArrayFilterCountHoist;
use super::array_map_sum_cache::ArrayMapSumCache;
use super::array_sum_int_bounds::ArraySumIntBounds;
use super::array_sum_int_direct::ArraySumIntDirect;
use super::array_sum_int_sink::ArraySumIntSink;
use super::array_sum_loop::ArraySumLoop;
use super::branch_simplify::BranchSimplify;
use super::borrowed_receiver_roots::BorrowedReceiverRoots;
use super::bounded_mixed_counter::{BoundedMixedCounter, LoopCounterVersioning};
use super::checked_int_sink::CheckedIntSink;
use super::checked_numeric_chain::CheckedNumericChain;
use super::concat_reset::ConcatResetElim;
use super::const_fold::ConstFold;
use super::count_hoist::CountHoist;
use super::cse::Cse;
use super::dead_inst::DeadInst;
use super::dead_store::DeadStore;
use super::fresh_mixed_transfer::FreshMixedTransfer;
use super::identity_arith::IdentityArith;
use super::immutable_local_loads::ImmutableLocalLoads;
use super::integer_range::IntegerRange;
use super::licm::Licm;
use super::loop_optimize::LoopOptimize;
use super::mem2reg::Mem2Reg;
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

    /// Returns whether this pass can possibly transform the current function.
    /// The default keeps existing passes unconditional; candidate-driven passes
    /// override it to avoid both their analysis and post-pass validation when no
    /// relevant opcode or storage shape exists.
    fn is_applicable(&self, _function: &Function) -> bool {
        true
    }

    /// Selects ownership finalization after all representation-changing passes.
    fn run_after_fixed_point(&self) -> bool {
        false
    }

    /// Selects canonicalization of lowering-time owners before the first inlining round.
    fn run_before_inlining(&self) -> bool {
        false
    }

    /// Runs the pass over one function, returning true if it changed the IR.
    /// `data` is the module's shared literal pool, used by passes that materialize
    /// new constants (e.g. peephole string-literal concat folding interns the
    /// folded string); passes that need no new literals ignore it.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool;
}

/// Builds the ordered set of transformation passes run on every function:
/// proven receiver-root preparation before inlining,
/// identity arithmetic folding, peephole rewrites, scalar local promotion,
/// immutable-local discovery, checked-arithmetic int-sink specialization,
/// proven bounded Mixed counters, guarded loop-counter versioning, canonical loop
/// optimization, integer range specialization, boxed numeric-chain fusion, constant
/// folding, common-subexpression elimination, loop-invariant
/// code motion, dead instruction elimination,
/// dead store elimination, branch simplification, and concat scratch-state
/// optimization.
/// The cross-function small-function inliner is not a member here; it runs as a
/// module-level phase in `optimize_module`, interleaved with these passes.
///
/// Scalar promotion runs after peephole forwarding and before the issue-623
/// passes, making loop-carried definitions explicit as block parameters.
/// Integer range analysis then consumes those block parameters and comparison
/// edges, rewriting only checked operations whose complete interval fits in I64.
/// Constant folding runs after peephole and the two issue-623 passes: immutable
/// scalar local loads become pure operands, and checked operations observed only
/// through integer sinks become allocation-free `IChecked*ToInt` computations or fused
/// `ICheckedNumericChainToInt` regions.
/// CSE can then deduplicate those computations using canonical immutable loads,
/// while LICM can move both the loads and their dependent arithmetic into loop
/// preheaders. Canonical loop optimization shares equivalent scalar induction
/// variables before range analysis, keeping checked updates checked, and exposes
/// invariant header arguments plus simpler integer tests for subsequent passes. Branch cleanup composes
/// SSA forwarding edges and merges straight-line loop update blocks.
/// The redundant or relocated instructions these leave behind are
/// cleaned up by dead instruction elimination, and any folded branch condition is
/// collapsed by branch simplification, all converging through the fixed-point loop.
/// Fresh numeric ownership transfer then runs separately, without restarting
/// checked-integer sink discovery after removing assignment clone barriers.
fn default_passes(module: &Module) -> Vec<Box<dyn IrPass>> {
    vec![
        Box::new(BorrowedReceiverRoots::new(module)),
        Box::new(IdentityArith),
        Box::new(Peephole),
        Box::new(Mem2Reg),
        Box::new(ImmutableLocalLoads),
        Box::new(CheckedIntSink),
        Box::new(ArraySumIntSink),
        Box::new(ArrayMapSumAddIntSink),
        Box::new(ArraySumIntDirect),
        Box::new(ArraySumIntBounds),
        Box::new(ArrayFilterCountHoist),
        Box::new(ArrayMapSumCache),
        Box::new(AbsIntSink),
        Box::new(StringLenSink),
        Box::new(StrposIntSink),
        Box::new(StripTagsLenSink),
        Box::new(InetPtonLenSink),
        Box::new(ParseUrlCountSink),
        Box::new(StrSplitCountSink),
        Box::new(CountCharsCountSink),
        Box::new(MbCatalogCountSink),
        Box::new(MbDetectOrderSingleSink),
        Box::new(EregSearchArrayResultSink),
        Box::new(EregSearchLiteralStateSink),
        Box::new(GzLiteralRoundtripLenSink),
        Box::new(GzLenHoist),
        Box::new(HashAlgosCountHoist),
        Box::new(HashConstSink),
        Box::new(IconvGetEncodingLenSink),
        Box::new(IconvMimeLiteralSink),
        Box::new(IconvPositionConstSink),
        Box::new(MbConvertVariablesLenSink),
        Box::new(MbParseStrLiteralSink),
        Box::new(MbStrlenConstLocalSink),
        Box::new(OpensslCbcLenSink),
        Box::new(OpensslCipherIvLengthConstSink),
        Box::new(MbNumericentityLenSink),
        Box::new(MbStrimwidthLenSink),
        Box::new(StrstrLenSink),
        Box::new(TokenGetAllCountHoist),
        Box::new(MbStrstrLenConstSink),
        Box::new(Base64DecodeLenSink),
        Box::new(SqrtAddIntSink),
        Box::new(JsonEncodeLenSink),
        Box::new(JsonDecodeCountSink),
        Box::new(BoundedMixedCounter::new(module)),
        Box::new(LoopCounterVersioning::new(module)),
        Box::new(LoopOptimize),
        Box::new(CountHoist),
        Box::new(ArrayGetBounds),
        Box::new(IntegerRange),
        Box::new(ArraySumLoop),
        Box::new(ArrayFillLoop),
        Box::new(CheckedNumericChain),
        Box::new(ConstFold),
        Box::new(Cse),
        Box::new(Licm),
        Box::new(DeadInst),
        Box::new(DeadStore),
        Box::new(BranchSimplify),
        Box::new(ConcatResetElim),
        Box::new(FreshMixedTransfer),
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
/// behavior; later rounds only optimize further.
///
/// Producing the same answer with `--ir-opt` on and off is the CONTRACT here,
/// not a proven property, and `--ir-opt=off` is the way to find out which of the
/// two a program is getting. It has been broken: dead-store elimination treated
/// a block that throws out of the middle of a `try` as reaching only the block
/// its `br` named, so stores the catch could still observe were neutralized and
/// the handler read a slot nothing had written, a different answer every run,
/// and only with the passes on. The rule that a `may_throw` instruction and a
/// `throw` terminator both reach the handler lives in `dead_store`; any future
/// pass whose analysis walks `cfg::successors` inherits the same blind spot,
/// because that CFG is built from terminators and has no exception edges at all.
///
/// Inside each round the module is destructured so the function tables and the
/// shared literal `data` pool are borrowed disjointly. The combined process
/// converges (inlining is bounded over the acyclic candidate graph; the function
/// passes only simplify), with `MAX_MODULE_ITERATIONS` as a backstop.
pub fn optimize_module(module: &mut Module) {
    let (preparers, remaining): (Vec<_>, Vec<_>) = default_passes(module).into_iter()
        .partition(|pass| pass.run_before_inlining());
    let (finalizers, passes): (Vec<_>, Vec<_>) = remaining.into_iter()
        .partition(|pass| pass.run_after_fixed_point());
    run_module_function_passes(module, &preparers);
    optimize_module_to_fixed_point(module, &passes);
    // Tail recursion changes the CFG and parameter mutability after the ordinary
    // fixed point, so rerun scalar passes before finalizing ownership.
    if super::tail_call::eliminate_tail_recursion(module) {
        #[cfg(debug_assertions)]
        if let Err(error) = crate::ir::validate_module(module) {
            panic!("tail recursion produced invalid module: {:?}", error);
        }
        run_module_function_passes(module, &passes);
    }
    // Buffer resolution hoisting needs the module-wide `pcntl_async_signals()` answer and runs
    // once on the converged loops; the scalar passes then clean up what it leaves behind.
    if super::buffer_access::hoist_buffer_resolution(module) {
        #[cfg(debug_assertions)]
        if let Err(error) = crate::ir::validate_module(module) {
            panic!("buffer resolution hoisting produced invalid module: {:?}", error);
        }
        run_module_function_passes(module, &passes);
    }
    run_module_function_passes(module, &finalizers);
}

/// Interleaves inlining with the ordinary function passes until both converge.
fn optimize_module_to_fixed_point(module: &mut Module, passes: &[Box<dyn IrPass>]) {
    for round in 0..MAX_MODULE_ITERATIONS {
        let mut changed = false;

        // Module-level cross-function inliner.
        let specialized = super::numeric_closure_calls::specialize(module);
        #[cfg(debug_assertions)]
        if specialized {
            if let Err(error) = crate::ir::validate_module(module) {
                panic!("numeric_closure_calls produced invalid module: {:?}", error);
            }
        }
        let inlined = super::inline::inline_small_functions(module);
        changed |= specialized;
        changed |= inlined;
        #[cfg(debug_assertions)]
        if let Err(e) = crate::ir::validate_module(module) {
            panic!("inline_small_functions produced invalid module: {:?}", e);
        }
        if round > 0 && !inlined && !specialized {
            return; // module-level fixed point reached
        }

        // Per-function fixed-point passes over every function-like body.
        changed |= run_module_function_passes(module, passes);

        if !changed {
            break; // module-level fixed point reached
        }
    }
    // Module-level non-convergence: keep the current (valid, more-optimized) IR.
    // Each per-function run still converged; only the inline/simplify interleave
    // hit the round cap, which is a generous backstop for deep inline chains.
}

/// Applies one phase to every function table while sharing the module literal pool.
fn run_module_function_passes(module: &mut Module, passes: &[Box<dyn IrPass>]) -> bool {
    if passes.is_empty() {
        return false;
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
    let all_functions = functions.iter_mut()
        .chain(class_methods.iter_mut())
        .chain(closures.iter_mut())
        .chain(fiber_wrappers.iter_mut())
        .chain(callback_wrappers.iter_mut())
        .chain(extern_callback_trampolines.iter_mut())
        .chain(runtime_callable_invokers.iter_mut());
    let mut changed = false;
    for function in all_functions {
        changed |= run_function_passes(function, passes, data);
    }
    changed
}

/// Runs the given passes over one function to a fixed point, returning whether the
/// function was modified at all (so the module-level loop can detect convergence).
/// After each pass that runs, in debug/test builds, the function is re-validated
/// and any malformed IR panics naming the offending pass. Non-convergence within
/// the cap panics in debug and stops (keeping current IR) in release.
pub fn run_function_passes(
    function: &mut Function,
    passes: &[Box<dyn IrPass>],
    data: &mut DataPool,
) -> bool {
    let mut modified = false;
    for _ in 0..MAX_PASS_ITERATIONS {
        let mut changed = false;
        for pass in passes {
            if !pass.is_applicable(function) {
                continue;
            }
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
