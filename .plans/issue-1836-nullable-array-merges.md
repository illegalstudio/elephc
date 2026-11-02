# Missing typed-array values at lazy merges — issue #1836

- [x] Keep all work/artifacts inside Elephc and preserve unrelated changes.
- [x] Freeze the original baseline and reproduce the issue SIGSEGV in both IR modes.
- [x] Preserve implicit null at EIR merges, scalar/callable getters and constant AST pruning.
- [x] Acquire object return payloads before releasing owning merge cells, including nominal widening.
- [x] Share native unpack-source storage classification across literals, positional calls and boxed binders, including suppressed reads.
- [x] Canonicalize raw null/undefined receivers to boxed-null dynamic getter inputs.
- [x] Preserve once-only offset effects, warning suppression and cleanup across throwing offsets and warning handlers.
- [x] Validate the frozen library/CLI with focused tests and all supported-target controls.
- [x] Update internals documentation and Trail learnings; verify Rust preambles and diff hygiene.

Validated base: `6459eb01d3439977e95acc50943f2e99b6927916`. Final source-file hashes and
linked CLI/library/probe/runtime-emitter hashes are in `final-artifacts.json`.
Frozen CLI SHA-256: `d5096308e169ac948014c4e2268dc303b2fe2ea9b70316c9e8184dc792866158`.
This plan and learning-only edits do not change the tested compiler inputs.

Final evidence:
- Warning-free library/CLI build; 333 focused array, merge, optimizer, ownership,
  array-basics and harness tests pass in each IR mode, 666 runs total.
- The original undefined-root isset CI test and explicit-null coalesce review
  example pass as four exact-shape CLI executions with clean heaps. The quiet
  key-effects and throwing warning-handler regressions also pass in both modes.
- 366 program/runtime object combinations compile and assemble across macOS ARM64,
  Linux ARM64/x86_64 and iOS device/Simulator. iOS evidence is compilation and
  assembly only; meaningful library fixtures are function-wrapped.
- All 234 native matrix executions match PHP output and warning counts. Of these,
  228 require clean heaps. Six intentionally retain the known one-Array/72-byte
  destination leak when a caught literal unpack rejects null; the same leak was
  reconfirmed on frozen exact-main6459 on macOS in both IR modes. These six are
  never counted as heap-clean.
- All 22 changed Rust files have module preambles. Backend changes reuse existing
  annotated emitters, with no new assembly instruction strings. Diff hygiene passes.

All receipts remain inside
`.claude/worktrees/.maintenance/elephc-pr-maintenance-20261015/issue1836/ci-failures/`.
The prior published09fc artifacts and validation are preserved separately, as are
invalid candidates and original main baselines. `final-validation-summary.json`
records only the current frozen-artifact results.

The existing explicit-int-cast sentinel defect, Error-versus-PHP-TypeError unpack
message, nullable call-unpack frontend rejection, undefined-root frontend/key
limitations, owned string-key leak and literal destination leak remain separately
reproduced baseline work in `next-baseline-bugs.md`; these checks are not full parity.

Trail reports twenty lexical conflicts in plan/internals prose describing rejected
approaches. The implementation boxes reads before scalar conversion, keeps nullable
ordinary elements, projects native storage only for unpack sources and pins a new
canonical boxed-null getter input across throwing user code. It does not pass the
raw original null to the dynamic getter, flatten Synthetic groups or extend a
proven-null cleanup lifetime to destructor-bearing payloads. New committed learnings
explain the source-level reconciliation without editing previous learnings.
