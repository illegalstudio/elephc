//! Purpose:
//! Gives a compiler run the stack depth its own nesting limit implies.
//!
//! Called from:
//! - `main`, around the whole binary.
//! - Anything driving the pipeline in-process: the codegen test harness, and embedders.
//!
//! Key details:
//! - A reserved memory mapping, not a thread, so a caller keeps its own return value,
//!   non-`Send` data and panics.

/// The stack a compiler run gets.
///
/// Sized against the limit the compiler ALREADY diagnoses. `MAX_COMPILER_NESTING` lets source
/// nest 1024 levels deep, and each level costs one frame in every recursive AST pass -- the
/// parser, the constant folder, the magic-constant walker, the checker, the optimizer's
/// rewriters, EIR lowering. The default 8 MiB main stack runs out around 140 levels, so
/// `$a = [[[…1…]]]` at 200 aborted the process with `has overflowed its stack` instead of
/// reporting the diagnostic written for exactly that input (issue #686).
///
/// One budget for the whole run rather than a per-pass guard because the passes are many and
/// the list grows: one place to size, and a pass added later inherits it. PHP itself compiles
/// these depths, so a diagnostic below 1024 would reject valid PHP rather than protect
/// anything.
pub const COMPILER_STACK_BYTES: usize = 256 * 1024 * 1024;

/// The headroom a caller must already have for [`with_compiler_stack`] to use its stack as is.
///
/// Half the reservation, so the phases NESTED inside a wrapped run -- `main` wraps the whole
/// compile, and every phase re-checks on entry -- keep using the run's own budget instead of
/// reserving a second one each. A phase that really is starting from a small thread stack sees
/// far less than this and reserves.
const COMPILER_STACK_HEADROOM: usize = COMPILER_STACK_BYTES / 2;

/// Runs `body` with at least [`COMPILER_STACK_BYTES`] of stack, returning what it returns.
///
/// This is the contract for running the pipeline in-process: the binary wraps `main` in it, and
/// anything else calling the crate's phases -- the codegen test harness, an embedder -- wraps
/// its own driver the same way. A caller that skips it gets whatever stack its thread happens
/// to have, which for a worker thread is often 2 MiB.
///
/// The stack is a fresh mapping obtained on demand, NOT a worker thread, and that choice is
/// load-bearing three times over: `body` may return a value and may borrow non-`Send` data, a
/// panic inside it unwinds through the caller normally instead of having to be caught and
/// resumed, and there is no spawn that can fail and leave the run silently short of stack.
///
/// The mapping is RESERVED, not committed: pages fault in only as the recursion actually
/// reaches them, so an ordinary compile pays for the depth it uses and nothing more. It is
/// released when `body` returns.
///
/// Already having the budget -- a caller on a stack this size, or a nested call -- reuses the
/// current stack instead of reserving a second one.
pub fn with_compiler_stack<R>(body: impl FnOnce() -> R) -> R {
    stacker::maybe_grow(COMPILER_STACK_HEADROOM, COMPILER_STACK_BYTES, body)
}
