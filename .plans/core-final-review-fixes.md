# Core final review fixes

Baseline: `b8e3e2f63e1524f69ed5febcca02889faf85a92f`, PR #893,
branch `feat/core-align`.

## Checklist

- [x] Close unsupported descriptor and non-reference call reference assignment.
- [x] Ensure accepted reference-return sources have a transferable managed owner.
- [x] Preserve the selected reference cell across fallthrough finally rebinding.
- [x] Publish returned reference leases before throwing caller cleanup and root value arguments.
- [ ] Protect copied by-value reference-call results during throwing caller cleanup.
- [ ] Release successful static extern callable argument temporaries.
- [ ] Protect static builtin callable arguments during later argument evaluation.
- [ ] Protect descriptor callbacks before argument container construction.
- [ ] Retire unused immediately invoked closure descriptors.
- [ ] Protect partial statically lowered array_map results across exceptions.
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
