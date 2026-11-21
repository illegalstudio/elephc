# Async/Parallel scheduler publication checkpoints

## Checklist

- [x] Confirm the maintainer's 2026-10-09 authorization to commit and push a
      work-in-progress checkpoint for tracking, before final review consensus.
- [x] Reuse the existing fork branch and upstream PR #958 as explicitly requested.
- [x] Run the current checkpoint's focused build/test and generated-doc checks;
      record failures as open, not as acceptance.
- [x] Commit the scoped project changes, excluding local tool configuration,
      scratch/review artifacts and unrelated inherited vendor changes.
- [x] Push normally to Guikingone/elephc:spike/runtime-ctx-register.
- [x] Record the exact pushed SHA and current validation limits in PR #958.
- [x] Resume the invocation-policy ABI migration and remaining scheduler gates;
      the named/default/nullable heap failure is under isolation, not resolved.
- [ ] Obtain complete independent exact-artifact review consensus before acceptance.

## Publication is not acceptance

The maintainer explicitly authorized publication for tracking on 2026-10-09 and
then confirmed that the existing branch/PR should be updated, not replaced by a
new branch or PR. This changes the earlier hold on publication, not the final
correctness/review requirements. Keep the PR in draft while implementation and
acceptance remain incomplete. Do not treat this checkpoint as a frozen accepted
Candidate 62, a release or an instruction to merge.

The verified pre-publication fork and local HEAD are both
`c26da5a06f49cdca27723ba4a9ba5d064cda79a2`. The publication target is
https://github.com/illegalstudio/elephc/pull/958, whose head repository is the
Guikingone fork and whose branch is `spike/runtime-ctx-register`. No force-push
is needed for the checkpoint. Existing main merges/foundation commits remain
in the branch; do not rewrite them during this publication step.

## Scope and evidence

The code checkpoint includes Async/Parallel preludes and bridge/contracts,
runtime context/isolation integration, scheduler monitoring, target/package
wiring, PHP argument/array/string/ownership prerequisites, tests, examples,
generated documentation and English plans. Preserve local-only `.codex/`,
`.mcp.json`, `.kioku/` and `scratchpad/` outside the commit, following the
existing review scope; Kioku packets remain local discovery/provenance evidence.

Historical evidence from 2026-10-01: 101 default-EIR and 76 no-IR-optimization
focused controls passed before the invocation-policy ABI migration. Functional
Linux x86_64/ARM64 probes passed; ordinary iOS assembly emitted and assembled,
but SDK linking/hosted execution was not proved. These are historical results,
not validation of the current ABI-v3 source.

The current source adds `CallableProfile { strict_php, strict_types }`, an
explicit third descriptor-invoker policy argument, internal weak-binding callers
and Magician ABI v3. `cargo check --lib` passed during the interrupted 2026-10-01
turn, but its final runtime tests were not recovered. Fresh validation must
cover this source before any migration-complete claim.

Remaining acceptance gates include the complete policy producer/consumer audit,
native/eval ABI compatibility, all PHP-array transport/consumer contracts,
scoped I/O provider/operation queue implementation, current supported-target
evidence, assembly-comment hygiene and complete Codex/Kimi/GLM/DeepSeek review
consensus against one exact current artifact. The last legacy touched-file
assembly-comment audit reported 52 issues; that gate remains open.

## Fresh pre-publication checks, 2026-10-09

- `cargo test --test codegen_tests string_parameter_boundaries -- --test-threads=1`
  compiled successfully, then reported **20 passed / 1 failed**. All output
  assertions passed; `dynamic_strict_nullable_union_defaults_and_named_tail`
  fails the unchanged heap assertion with **one live block / 40 live bytes**.
  This is the first issue to isolate after the WIP checkpoint is pushed.
- `cargo build --example gen_builtins --features curl` passed. Generated builtin,
  class/constant, module-section and PHP-comparison docs were refreshed. Builtin
  audits report zero errors, site compatibility validates 2,075 pages, and the
  enforced EIR architecture inventory reports no structural errors.
- `git diff --check` passed. The publication manifest excludes local tool config,
  Kioku packets, scratch/review artifacts and vendor paths.
- The expanded invocation/bridge assembly-comment check reports **71 issues**
  across its selected files, including multiline instructions with comments on
  closing lines and legacy alignment/missing-comment issues. This replaces the
  narrower historical 52-issue figure for this selected scope, not for the
  entire repository. Do not claim the assembly-comment gate green.
- PR #958 has been converted to draft for this in-progress tracking publication.
  No full local suite, current complete target matrix or new independent model
  review consensus has been run or claimed.

## Published checkpoint

Commit `ae2c6bd8bd18bcf55152bc65539f0828c34be019` was pushed normally to the
Guikingone fork. GitHub reports this exact SHA as PR #958's head and the PR is
draft. The PR body now preserves the original foundation description as
historical and adds the current scheduler scope, fresh results and open gates.
The seven inherited deletions under examples/namespaces/vendor and
examples/spl-autoload/vendor remain outside the commit and unchanged locally.
The initial unpublished staging mistake was amended before any push; no remote
history was rewritten.

The first observed check on this SHA is the successful PR-label classifier,
not a compiler/runtime CI validation. Full CI evidence must be inspected on the
exact head after the required jobs run; do not report the classifier as test
coverage. Follow-up source edits will need their own focused revalidation.

## Follow-up worktree progress (not part of the published checkpoint)

The 40-byte leak was isolated to an omitted null default: explicit null,
nullable string, union values/rejection and named-tail rejection each replay
heap clean independently. emit_null_default_to_result allocates a fresh Mixed
cell, but push_default_value_arg acquired it as if borrowed. Transferring its
existing owner fixes the combined regression. The 23-control group, including
new omitted nullable/mixed default and later-binding-failure cases, passes.

A new omitted by-reference nullable default regression initially left three
allocations / 120 bytes live. The callee expects the existing pointer-holder
reference ABI; a trial passing the boxed value directly was rejected by the
regression and removed. The corrected route preserves the holder and registers
its newly created default owner in the invoker ledger. The isolated reference
regression now passes with the original output and heap-clean assertion.
Raw non-Mixed reference defaults and the full supplied-reference/return contract
still require their separate audit; do not treat this as full reference closure.

The updated 83-control default-EIR group and 24 string controls with EIR disabled
pass. Selected Magician native/ABI unit controls pass 19/19 after updating its
FFI registration-test stub to the explicit third ABI argument. These are focused
controls, not complete invocation or PHP-array acceptance. GitHub reports the PR
CONFLICTING with main, so integration conflicts and actual runtime CI remain
open independently of this focused worktree repair.

Fresh Linux x86_64/ARM64 emitted-assembly probes pass for the omitted default
and its by-reference nullable mutation route (functional, not Linux heap-debug
suite coverage). Provenance: /tmp/elephc-linux-emitted.VtQJK3,
/tmp/elephc-linux-emitted.uL6ywn, /tmp/elephc-linux-emitted.H8RfLw,
/tmp/elephc-linux-emitted.3XN2fe. Compiler SHA-256:
be6f8c3e790a2c5e4cc087af1018b1f1c0d484133051135f9708bd1baacfe98e;
runtime emitter: 41edc33b7d28b35216be06fa8d7a01c7c7ba142aa0342b7886f8ad7c70e32bc4.
The follow-up commit will carry the two ownership fixes, three new regressions,
the FFI stub correction and this evidence ledger only. No broader acceptance
gate is closed by publishing it.

Ordinary iOS device/simulator by-reference-default assembly emits and assembles
with the known host-sysroot warning; no SDK linking/hosted execution is claimed.
The maintainer additionally requested integration of origin/main on 2026-10-09.
Prefer a merge on the published branch to preserve history without force-push;
keep inherited local vendor changes recoverable and outside that integration.
