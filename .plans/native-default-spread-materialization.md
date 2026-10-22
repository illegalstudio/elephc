# Native default array spread materialization

- [x] Preserve PHP82/85 complete-value oracles for final-free, occupied, prefix-then-overflow, string-key-only and empty spreads.
- [x] Reproduce native invocation failure on the current PR1568 candidate and a main487cae6 compiler control with only unrelated destructor-order changes.
- [x] Preserve the original five-argv native regression without weakening values or heap checks.
- [x] Implement materialization through the existing native callable/default protocol.
- [x] Resolve native free-function/method Reflection missing-default witnesses against PHP.
- [x] Validate exact outputs, fresh-process clean heaps, focused target support and the example/docs.
- [ ] Publish with explicit Reflection/TLS dependency provenance and observe exact-head CI.

Worktree and code stay inside Elephc. This is an independent baseline, not an
excuse to loosen the seven PR1568 maximum-key regressions. Evidence is preserved
under `.claude/worktrees/.maintenance/elephc-pr-maintenance-20261015/pr1568-review-20261020/candidate1/`.

`native-call-main-control.json` pins main487cae6 plus unrelated PR1874 runtime
traversal; the unchanged callable invoker aborts with `cannot materialize this
default value`. Opaque free-function and static-method Reflection separately
return a missing-default ReflectionException. Literal/interpreter next-key
semantics and native callable default materialization are distinct paths.

No PR is published for this incomplete follow-up. Audit existing PR1256 overlap
before selecting the final publication branch; never publish a replacement for an
existing PR. Keep the existing PR1256 upstream exception pending until answered.

## Implementation and final local evidence

- Generate existing EIR native default helpers for spread trees and register owned expression metadata for callable/Reflection factories. Reflection retains lazy availability until materialization.
- Native hash spread probes checked append availability before retaining source values, on AArch64 and x86_64; empty/string-only operands and object projection keep their own key rules.
- Mark HashSpread MAY_THROW and pin independent in-flight owners for exception cleanup.
- Candidate5, on main40c2 plus Reflection ownership dependency PR1953: all 80 fresh-process cases pass under profiles8.2/8.5 with IR on/off, exact output and clean heaps.
- Ten supported-target program/runtime pairs assemble; all six macOS/Linux executions pass five complete-value shapes with clean heaps. iOS device/Simulator evidence is assembly only.
- The new example matches PHP with a clean heap in both IR modes. Compiler/bridges build warning-free, assembly-comment alignment and whitespace checks pass.
- Five per-target native helper/EIR effect tests and five prior callable factory tests passed before the rebase; their source paths were unchanged in main487-to40c2. Full current target CI remains required after publication.

PR1256 concerns array-valued constant folding and its parser rewrite; this change fixes independently reproduced main callable/default failures using the existing executable-default protocol. It does not replace or update PR1256, or claim to close the whole ArrayLiteralMixed/property-default issue1340. The pending upstream exception is unchanged. The old combined dirty worktree is retained in stash f314c9657bffa0422aa17cdb503effabfd0f7287 and binary patches.
