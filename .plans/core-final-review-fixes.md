# Core final review fixes

Baseline: `b8e3e2f63e1524f69ed5febcca02889faf85a92f`, PR #893,
branch `feat/core-align`.

## Checklist

- [x] Close unsupported descriptor and non-reference call reference assignment.
- [ ] Ensure reference-return sources have a managed owner or a bounded active borrow.
- [ ] Preserve the selected reference cell across fallthrough finally rebinding.
- [ ] Align returned property payload guards with bound-closure signature specialization.
- [x] Publish returned reference leases before throwing caller cleanup and root value arguments.
- [x] Protect copied by-value reference-call results during throwing caller cleanup.
- [x] Release successful static extern callable argument temporaries.
- [x] Protect static builtin callable arguments during later argument evaluation.
- [x] Protect descriptor callbacks before argument container construction.
- [x] Retire unused immediately invoked closure descriptors.
- [x] Protect partial statically lowered array_map results across exceptions.
- [ ] Accept valid Mixed class-name strings in get_class_vars.
- [ ] Capture native argument metadata for indirect and eval-originated backtraces.
- [ ] Expose AOT user constants in eval inventories under the correct category.
- [ ] Preserve first-inclusion order in eval file inventories.
- [ ] Add focused regression coverage and synchronize affected documentation.
- [ ] Review every implementation diff and pass compilation and static hygiene checks.
- [ ] Commit thematically, push the reviewed head, and obtain CI results on that head.

## Execution boundaries

Claude Opus 5 receives bounded implementation tasks through the cross-agent
delegation wrapper. Only one delegated writer runs in this worktree at a time.
The coordinator reviews each result and owns commits and pushes.

Do not execute local tests, PHP/compiler repros, or full-suite commands. Test
sources may be added and compile-checked. CI provides executable validation.
Do not switch branches, merge the PR, edit release changelogs, or expand the
agreed Core extension-inventory and collector-buffer limitations.

## Validation ledger

The baseline CI is green. New implementation and validation entries belong
below, and checklist items must not imply that unexecuted tests passed.

### Reference-safety draft review

- Claude Opus 5 completed the first reference-safety draft without executing tests.
- Coordinator: `cargo check --locked -p elephc --tests` passed without warnings.
  This only type-checked production and test sources, it did not execute tests.
- Coordinator: `git diff --check` passed for the first draft.
- The draft is not accepted yet. A second bounded Claude task must publish the
  returned lease above same-frame catch boundaries, reject owner-zero transfers,
  make lowering diagnostics rollback-safe, and strengthen regression coverage.
- A separate read-only Claude review proposed a category-preserving AOT user
  constant registration design. It has not been implemented or validated yet.

### Reference-safety implementation accepted for CI

- Claude's second pass added prepublished reference-assignment owners, a
  target-complete owner-zero error, uniform omitted-default roots, and diagnostic
  rollback. Regression sources cover same-frame destructor timing and repeats.
- Coordinator review corrected unreachable-branch provenance merging, the
  structural cleanup-stack assertion, one unused field, and new punctuation.
- Final `cargo check --locked -p elephc --tests` passed without warnings.
- `scripts/check_asm_comments.py` passed on all five affected emitter/data files.
- `git diff --check` passed. No local tests or PHP/compiler repros were executed.
- The first four checklist items indicate implemented and source-reviewed fixes,
  not executed-test success. Their runtime validation is still pending in CI.
- An additional owned-result gap remains for ordinary by-value use of a
  reference-returning call. It is assigned to the callable ownership batch.
- Read-only backtrace review found hidden-parameter handling and prebound eval
  argument-count loss, so detection changes must include those transport fixes.

### Reference-safety CI and independent review feedback

- Commit `2b309b89e0ec91d35f7aa13147adf6a357c8ff62` was pushed to #893.
- CI run `34596358463`, Linux x86_64 shard 9/16, failed the new fallthrough-finally
  regression with `by-reference return from a path-dependent local reference`.
  The shared local-address helper already handles dynamic representations; the
  additional static refusal in the reference materializer is too restrictive.
- A separate read-only Claude review found the established boxed `array_walk`
  reference-return callback can no longer copy its active borrowed element. The
  return boundary must distinguish an active bounded borrow from an arbitrary
  ownerless address and snapshot the selected pointer independently of its lease.
- CI also confirmed that failure in Linux x86_64 shard 6/16. Other failed shards
  expose a typed `Array` versus boxed `Mixed` reference-return payload mismatch
  and a relay fixture rejected before reaching the guard because it passes
  `int` storage to a `mixed &` parameter. These need implementation and fixture
  corrections, respectively, without weakening reference storage validation.
- The affected checklist items are reopened until these corrections are reviewed
  and their CI regressions pass. No local test or repro was executed.
- Callable ownership implementation is in progress with Claude Opus 5. The
  coordinator is reviewing intermediate diffs but has not accepted that batch.

### Callable ownership first pass

- Claude completed the first implementation pass for extern/builtin callable
  arguments, early descriptor callbacks, IIFE descriptors, partial static map
  results, and ordinary by-value reference-return lease staging.
- Coordinator: `cargo check --locked -p elephc --tests` passed without warnings
  in 15.65 seconds. No test or PHP/compiler repro was executed.
- The batch remains unaccepted. Typed CUF/CUFA containers and owned results during
  callback/descriptor retirement still need unwind-visible roots. Late fallback
  decisions must not reevaluate callbacks. Regression sources need same-frame
  catch coverage, and the nearby `Closure::call` path needs the shared helper.
- Final CI run `34596358463` failed. The additional non-codegen fixture for
  diagnostic rollback is rejected by the checker for int-to-string reassignment;
  its source must still exercise rollback using supported storage.
- Claude is correcting the reference-return CI failures next. No concurrent
  delegated writer runs in this worktree.

### Reference CI correction, second review

- Claude separated the selected `Pointer` SSA from the optional managed return
  lease, admitted only exact active boxed-walk borrows, promoted ordinary local
  array payloads to their declared Mixed shape, and corrected the invalid fixtures.
- Coordinator review made pointer result/return validation explicit, removed the
  late-rematerialization fallback, preserved compatible object-class storage, and
  made the borrowed-finally regression observe the actual copied return value.
- `cargo check --locked -p elephc --tests` passed after the first review corrections
  in 18.26 seconds. The later final-state check passed in 17.19 seconds with one
  unused-helper warning from the concurrently incomplete callable batch, which
  is excluded from this reference CI commit. No tests ran.
- Assembly-comment checks passed on the changed return/local-cell emitters.
- A separate property-payload guard draft revealed that bound closures currently
  specialize only the callable signature, not the compiled closure's return type.
  That draft was withdrawn to preserve existing bound-string reference controls.
  The property/closure specialization correction remains explicitly required and
  has a bounded Claude task prepared; it is not counted as complete.
- Callable completion is now delegated to Claude. Its read-only design review
  confirmed prepublished owned-result slots and Mixed pointee cloning are needed.
  Proposed example fixtures were screened for genuine same-frame catch behavior
  and valid spread/named ordering before assigning the implementation.

### Reference correction pushed and callable completion in progress

- Commit `184cb9df3ad430eec09c40c11eac6a09f375d1fa` is confirmed on origin.
  CI run `34602101553` is in progress; no runtime success is claimed yet.
- The callable completion draft passes `cargo check --locked -p elephc --tests`
  in 17.25 seconds, with one obsolete `adopt_returned_ref_cell` helper warning.
  The delegate is still completing its regression sources. No tests ran locally.
- Separate read-only Claude sessions are checking property/bound-closure typing
  and preparing an include-order patch proposal. Only the callable delegate may
  write to the worktree, apart from coordinator-owned bookkeeping and reviews.

### Callable ownership completion reviewed for CI

- Claude implemented partial typed-container roots, total descriptor argument
  builders, prepublished owned-result staging, and detached Mixed reference copies.
  New source regressions cover same-frame catches and repeated cleanup.
- The delegation process is no longer running after the interruption; its final
  textual response was unavailable. Acceptance is based on the inspected patch,
  not an inferred success report from the delegate.
- Coordinator review removed obsolete late-adoption code, balanced original
  `Closure::call`/`bindTo` receiver and descriptor temporaries, added extern FCC
  loop coverage, and replaced a static-property reference-return fixture that
  would be rejected before exercising its intended copy behavior. Structural
  coverage now explicitly checks Mixed pointee cloning on all five targets.
- `cargo check --locked -p elephc -p elephc-magician --tests` passed without
  warnings in 20.91 seconds before the last source-test additions. Final source
  compilation and CI remain required; no test was executed locally.
- Checked items mean implemented and source-reviewed, not executed-test success.
  Bound-property closure specialization and descriptor receiver ownership remain
  assigned to a separate correction rather than counted as complete here.
- Final source compilation passed without warnings in 15.99 seconds, including
  the source-test corrections and the uncommitted include-order proposal.
  `git diff --check` passed. Runtime verification is left to CI.
