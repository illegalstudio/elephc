# Callable descriptor cleanup during exception unwinding

## Problem

Receiver-bound callable descriptors are temporary heap allocations. Their normal
invocation path releases the descriptor after preserving the call result, but a
runtime throw transfers control with `longjmp` before that post-call release.
The current activation-frame cleanup metadata does not describe these codegen
temporary-stack descriptors, so instance-array and invokable-object calls can
leak the fresh descriptor and its retained receiver when the callee throws.

## Boundary

This leak predates the descriptor-resolution size optimization. Existing callable
descriptors borrowed from boxed `Mixed` values must not be retained merely to
normalize invocation, because that would add a new leak on the same unwind path.

## Required follow-up

- Represent fresh descriptor ownership in cleanup metadata visible to
  `__rt_exception_cleanup_frames`.
- Release the descriptor exactly once on both normal return and unwind.
- Preserve borrowed/static descriptor behavior and receiver refcounts.
- Cover instance-array and invokable-object throws with heap-debug regressions on
  every supported target.
