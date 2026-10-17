# By-reference foreach tail append maintenance

- [x] Rebase the existing PR branch on `origin/main`.
- [x] Inspect the failed CI job and separate infrastructure failures from regressions.
- [x] Reproduce adjacent deletion cases against an exact current-main compiler.
- [x] Rebuild from the relocated worktree and replay focused functional/heap tests.
- [x] Verify the caller ABI sequence for every supported target.
- [x] Run hygiene and Trail checks and prepare a squashed update of the existing PR head.
- [x] Confirm CI on published head `3a6c1ac7f8` (green).
- [x] Strengthen the new review's source-reload assertion after the kind comparison.
- [x] Verify real all-target emission and missing/wrong-slot reload controls.
- [x] Prepare the corrected assertion for existing-branch publication.
- [ ] Confirm fresh CI after this review follow-up.

## Scope and baseline

PR #1811 fixes issue #1789: append after yielding the tail without deleting that
tail key. Tail deletion, key recreation, and string-key replacement are tracked
separately by open issue #1807; do not claim that the key-anchor fix covers them.

An exact `origin/main` build at `d3542732744e46dbe623951d53ee0ddba4f2498e`
misses the append both in the original reproducer and in all three deletion
variants. PHP visits the new elements in each case. Each failing native case has
a clean heap, which is not evidence of semantic correctness.

The historical failing Linux ARM64 eval shard never ran tests: its archived
`nextest-archive-linux-aarch64` artifact was missing, including after retries.
A fresh push must regenerate the complete CI archive rather than mask a failure.

## Reproducibility after relocation

Worktrees, generated PHP/assembly/native fixtures, build helpers, logs, and
temporary directories stay inside the canonical Elephc project. Rebuild local
package entrypoints after relocation before using the shared Cargo target, since
older artifacts can embed the former external manifest paths.

Focused harnesses compile the original test modules against Cargo's root library
and exact locked dependency versions. Run only the selected modules, excluding
their shared helper tests, and record optimizer-on/off results separately.

## Focused validation

The relocated branch builds warning-free with `cargo build --locked -p elephc
--lib --bin elephc`. The original growth module passes 19/19 on host macOS ARM64
with the EIR optimizer enabled and disabled. Both new heap-debug regressions pass
in each mode: 42 selected test executions, excluding shared support tests.

The caller-register test now covers all five supported targets. Its fixture puts
the loop in a named function so iOS static libraries emit the loop rather than
omit executable top-level statements. This is assembly emission, not iOS or
Linux execution; fresh CI supplies native Linux coverage.

The original append probe now matches PHP and has a clean heap. All three #1807
variants retain their pre-existing semantic failure and remain explicitly open.
Assembly instruction comments are aligned and `git diff --check` passes.

## Trail check interpretation

Eight keyword matches are contextual false positives: the plan explicitly rejects
clean-heap-only semantic proof and top-level iOS witnesses; the test uses a named
function and iOS static-library output; the runtime resync preserves the
per-helper ABI. No Synthetic declaration groups are flattened by this change.
The referenced immutable learnings are preserved.


## Source-reload assertion review, 2026-10-16

Reviewed head: 3a6c1ac7f8bbeb487dea14a52f48a21553c19276, whose complete CI is green.
Thread PRRT_kwDORs6r_s6tJpmq is valid: matching `x0,` also matches the heap-kind
comparison, so it does not prove that the table pointer was reloaded.

The assertion now requires the exact memory load after that comparison, adapts
its destination to the resync helper's argument register, and checks the same
frame-relative source address when a scratch address register is used. It does
not accept the kind result comparison as an argument materialization.

An exact relocated source compiler/library rebuild is warning-free. Production
Rust/Cargo source is unchanged from the reviewed head. Both original focused
modules pass: 19 growth tests and 73 reference-cell/ownership controls, on/off
(92 distinct cases, 184 executions), excluding 17 shared support checks. The ABI
case emits its named-function witness for all 5 supported targets in both modes.
iOS static-library evidence is emission only; no Linux/iOS execution is claimed.

A CLI shim deliberately removes the real source reload or changes the frame
slot in emitted assembly. All 10 faults are rejected by the new assertion across
all 5 targets. The old compiled test still passes the missing ARM reload, proving
the original gap. Original and mutated assembly and complete results stay inside
maintenance/pr1811-review/. No full local suite was run. Tail deletion/recreation
remains separately scoped to open issue #1807.


The current Trail keyword matches are contextual. The plan describes the
previous assertion gap and the named-function iOS correction, rather than
restoring them. Pointer/slot checks in the current test and runtime agree with
the saved-source learning; mentions of heap kind or a frame slot are not rejected
algorithms. Foreach reference-cell count/key controls do not flatten Synthetic
AST groups, and no deep-inheritance guard or publication replacement is changed.
Fault injection verifies the distinction directly. Immutable learnings and other
worktrees are preserved; the existing fork PR branch remains the destination.

Additional cross-branch descriptor/null-result slot vocabulary does not alter
the foreach helper or its saved-source contract. These are contextual matches.
