# Execution Scheduler code review scope

Status: Candidate 61 is the current frozen review artifact and is rejected by both dedicated
Codex reviewers, with multiple parent-reproduced blockers. Live fixes are in progress and will
require a new artifact before any acceptance can count. Candidate 62 is the sole live implementation
draft; specification SHA-256 `0621f4c3e5b0b91941e5a42f7ee50acac37f5f4facba62a24c110e7ac29f2bd1`,
with no frozen source tree or reviewer verdict. Specification SHA-256 of the rejected frozen artifact:
`f7bc90281a8143af9ca69ee5313940ea443681b7b2b5444ee645af63e669b549`.
Review commit: `45b1dc6d145b6e3303d470cf9178355715cb70df`.
Code tree: `29e53de7fa35aff375692de2599cec6ddbd7bbff`.
All earlier snapshots and verdicts below are historical. Absolute current-artifact consensus,
supported-target acceptance, and publication-path audit remain open.

## Authoritative Candidate 61 review snapshot — 2026-09-30

The detached review checkout is `/tmp/elephc-scheduler-c61.CEmmpK/checkout` at the commit above,
with parent/base `c26da5a06f49cdca27723ba4a9ba5d064cda79a2`. It contains 1,093 changed paths,
36,371 insertions and 1,749 deletions. A separate temporary index created the review tree; the
shared index and branch were left unchanged. `git diff --check <base> <tree>` passed.

The snapshot contains the compiler/runtime-context foundation and scheduler changes, authored
and generated documentation, tests, examples, CI/build changes, the current specification, and
its checksum. Seven inherited example-vendor deletions were restored only in the temporary index.
Local `.codex/`, `.mcp.json`, `.kioku/`, `scratchpad/`, this mutable manifest and the review log are
outside the immutable artifact. The review commit is an unpublished detached artifact; no branch
commit or push has occurred. Final publication must audit every selected path for relevance and
preserve excluded local material and vendor deletions.

Fresh dedicated Codex backend and scheduler/Parallel reviewers are running against this artifact.
A separate Codex instance orchestrates Kimi K3, GLM 5.3, and DeepSeek V4.1 through the Ollama
launch profile. Logs and full verdict transcripts belong under
`/tmp/elephc-scheduler-c61.CEmmpK/results`. A launched process is not an accepted review.
All four named reviewer identities must accept the same commit/tree/specification hashes with no
unreconciled findings or questions; any source or normative specification edit restarts the round.

The prior turn reran `async_spawn` 7/7 and `state_and_variadics` 53/53 after removing temporary
diagnostics; Rust formatting and diff hygiene passed. Earlier focused callback/Parallel/parity and
two-architecture invoker-slot evidence is enumerated in Candidate 61. The current turn is checking
the remaining named-variadic regressions. Linux x86_64 runtime, current-tree Linux ARM64 runtime,
iOS target paths and generated-doc/assembly audits remain explicit acceptance work.

## Historical review snapshot — superseded

An isolated temporary Git index, initialized from committed HEAD without changing the shared index,
selected the scheduler/ctx source, tests, contracts, generated docs, examples, CI/build files, and
plans described below. `git write-tree` produced tree
`1b97a372b1fd5e9f0c3ff7d61ffa17431b759a17` (1,076 changed paths versus HEAD; 32,135 insertions and
1,590 deletions). Its cached diff passes `git diff --check`. The real index had zero staged paths.
Reviewers can inspect `git diff HEAD 1b97a372b1fd5e9f0c3ff7d61ffa17431b759a17` and the tree's files.
This is a review universe, not yet an approved publication set: its 1,076 paths still need ownership
and relevance audit before any commit or push. This mutable manifest is deliberately outside the
snapshot to avoid a self-referential tree hash. Later source edits require a new tree hash and a fresh
complete code-review round.

## Historical mechanical path audit — superseded

The snapshot contains every tracked worktree change except seven deletions under the two example
`vendor/` directories. It also contains the new scheduler source, tests, plans, examples, and authored
docs. Its documentation portion has 846 generated builtin pages; the three generated index/summary
pages and two generated registry JSON files account for the remaining five generated-document paths
recorded below. The two newly edited compiling guides are included. A shadow-index/worktree diff
over the selected paths is empty, while the shared index has no staged entries.

Untracked local material left outside the snapshot consists of `.codex/hooks.json`, `.mcp.json`, ten
`scratchpad/` files, this mutable scope manifest, and the Kioku store (`.kioku/README.md` plus 100
packets). `kioku verify` currently reports 15 `ok`, 8 `moved`, 56 `drifted`, and 21 `superseded`
packets. The newly verified scheduler packets are useful review evidence, but the entire store must
not be staged blindly; curate any packets destined for publication after the code scope is accepted.

The Candidate-58 macOS `test_with_parallel_links_and_runs_an_unrelated_program` regression passes.
A focused Linux x86_64 attempt did not reach test execution: the repository script built unrelated
test binaries, then a direct `--test codegen_tests` retry spent nine minutes compiling under
emulation. Both were stopped cleanly; the reusable Docker target volume was retained. This is
**not** a Linux pass or failure. No ARM64 rerun was started for this documentation/test-only delta.
Kimi's availability probe still returned Ollama HTTP 429, reference
`7ac0e837-afc8-41fd-8aad-373eafe97ed6`; it is not a model review.

## Historical Codex audit leads on tree `1b97a372b1fd5e9f0c3ff7d61ffa17431b759a17`

Three read-only Codex reviews reported the following source-grounded leads. They are not Ollama
verdicts, and each must be independently reproduced or reconciled before a finding is closed:

- **FFI authority and memory safety:** injected global Parallel externs expose `buffer_alloc/free`,
  `parent_scope_leave`, `job_release`, and other bridge controls. A repeated buffer free can double
  drop `Box::from_raw`; writing past the returned allocation can corrupt Rust allocator memory.
- **Invalid job ID hangs drain:** failed native submission/serialization returns job ID `0`; `phase(0)`
  is `-1`, which the group drain treats as permanently incomplete. By-value cyclic-array captures
  may reach this path because capture checks omit the reference-alias guard applied to arguments.
- **Future duplication:** PHP object serialization can duplicate the private native job ID despite the
  `__clone` guard; the duplicate destructor may release the original Future's job before scope drain.
- **Parallel isolation:** unresolved string callbacks do not project `uses_process_global_storage`,
  allowing an indirect call to a function that reads process-global PHP storage.
- **Worker teardown:** normal worker completion clears fatal interception before context-release GC
  invokes user destructors; a destructor calling `exit()` may terminate the host process.
- **Parallel resource IDs:** worker-local stream operations mutate a process-global resource-ID table
  and cursor without synchronization.
- **Async correctness:** import aliases and fully-qualified callable strings for `Elephc\Async\run()` may
  miss prelude injection; any user-thrown `CancelledException` after group cancellation may be
  misclassified as request-paired; repeated `Fiber::start($value)` can leak the overwritten start
  argument; large valid I/O timeouts may overflow C `poll(int)` into an infinite wait.
- **Target docs:** the Parallel guide omits the explicit iOS device/Simulator compile refusal.

The prior `param_binding.rs` lead was rechecked against the frozen tree and dismissed: result-only
widening to `Mixed` is present at the exact snapshot. Current fixes/evidence are recorded below;
the leads still require independent fresh-hash review.

## Superseded code review snapshot — tree `1f432b8d2ca2a9a6970199113491ef772fd009f7`

An isolated temporary Git index, initialized from committed HEAD without touching the shared index,
captured tree `1f432b8d2ca2a9b6970199113491ef772fd009f7`. The tree contains 1,083 changed paths
versus HEAD (33,365 insertions and 1,607 deletions); its cached diff passes `git diff --check`, and
the real index remains empty. Compared with the superseded review tree, the current code delta is
33 files (1,433 insertions and 220 deletions). The exact review inputs are
`git diff HEAD 1f432b8d2ca2a9b6970199113491ef772fd009f7` and that tree's files.

The snapshot includes all tracked worktree changes and untracked project source, tests, examples,
docs, and plans, except the seven pre-existing vendor deletions. Local-only `.codex/`, `.mcp.json`,
`.kioku/`, `scratchpad/`, and this mutable scope manifest are excluded. The `.kioku` packets are
local discovery/decision evidence and are not staged blindly. A shadow-index diff over the excluded
paths is empty.

### Historical focused validation on predecessor tree `1f432b8d2ca2a9a6970199113491ef772fd009f7`

- `cargo test --test codegen_tests async_`: 135 passed, including Async cancellation, nested
  Async/Parallel composition, Async-only Fiber callback guards, and the unpaired user-thrown
  `CancelledException` regression.
- `cargo test --test codegen_tests parallel_worker`: 12 passed, including fatal containment,
  release-time destructor reentry, unique resource IDs, and worker transfer cleanup.
- `cargo test --test error_tests parallel_transfer`: 37 passed, including raw bridge FFI rejection,
  unknown string-callable global access, custom user-wrapper registry access, and reference cycles.
- `cargo test --lib prelude::tests`: 95 passed; `cargo test --lib opcache_prelude::detect::tests`:
  32 passed; `cargo test --lib process_global_parallel_storage`: 2 passed.
- `parallel_zero_job_id_drains_as_an_infrastructure_failure`,
  `parallel_handles_cannot_be_serialized_or_unserialized`,
  `rejected_second_fiber_start_releases_mixed_arguments`, and the three `parallel_run` tests pass.
- `scripts/check_asm_comments.py` on the modified runtime emitters reports all comments aligned;
  `git diff --check` passes.

These are focused macOS ARM64 results, not cross-target or end-to-end closure. No current-hash
Kimi/GLM/DeepSeek verdict has been received, and the old Codex review outputs do not count for this
tree. The old review findings are now source-fixed and locally covered, but all must be re-audited
on the exact current tree. No commit or push is authorized by this checkpoint.

## Authoritative boundary

- Worktree: `hazy-watching-moler`, branch `spike/runtime-ctx-register`, current committed head
  `c26da5a06f49cdca27723ba4a9ba5d064cda79a2`. The fork's branch head matched it on
  2026-09-27; `origin/spike/runtime-ctx-register` is 413 commits behind this local/fork head.
- Specification and SHA-256: `execution-scheduler.md` and `execution-scheduler.sha256`.
- Current source has uncommitted scheduler changes alongside a broad dirty worktree. A model verdict
  on the specification does not establish code-review consensus or authorize a broad commit.

## Slices the dedicated code review must inspect

1. `src/async_prelude*`, `src/parallel_prelude*`, and their PHP/AST parity; public authority,
   private `runRoot`, `Future`/`TaskGroup` clone guards, cancellation, and scope cleanup.
2. Shared contracts and checker rules under `crates/elephc-builtin-contract/`, `src/builtins/`,
   `src/types/`, `src/optimize/`, and compiler pipeline wiring.
3. EIR lowering/codegen, target ABIs, Fiber switching, runtime context ownership and cleanup,
   reactor I/O, and monitoring under `src/ir*`, `src/codegen*`, `src/monitor/`, and the Parallel
   bridge crates. Review all supported executable hosts and iOS refusal paths.
4. Tests, examples, CLI/linker/build scripts, CI/release packaging, and docs. The `--with-parallel`
   link-only behavior now has a CLI regression and matching user documentation. The generated builtin
   registry delta adds five Async functions and fifteen Async/Parallel classes, with no removed symbol;
   hundreds of page sidebar orders changed as a deterministic consequence. The builtin-doc workflow
   and audits pass, and the current macOS Async/Parallel codegen slices pass 55/55 and 62/62.

## Freeze and publication gates

- Audit every dirty path before staging. `.codex/`, `.mcp.json`, `scratchpad/`, and old Kioku packets
  are not automatically part of the scheduler commit. Preserve unrelated worktree changes.
- The 851 already modified generated documentation/registry paths stem from five added Async
  functions and fifteen Async/Parallel class contracts; the deterministic generator and audits have
  been rerun. The existing tracked deletions under `examples/namespaces/vendor/` and
  `examples/spl-autoload/vendor/` are outside this scheduler publication scope. The local
  `examples/async-scheduler/main.key` and `main.speedscope.json` artifacts remain on disk and are
  ignored by that example's `.gitignore`.
- Form a reviewable commit or manifest of the intended files, record its exact hash, and run a
  dedicated Codex-controlled review with Kimi, GLM, and DeepSeek against that same code snapshot.
  A code edit or finding restarts all three code reviewers.
- Reconcile focused native tests, generated-doc audits, iOS checks, model findings, and CI before
  pushing only the intended branch to `fork`. The Ollama weekly cloud quota currently prevents the
  required model rounds; no consensus or push is claimed.

## Last exact code-review snapshot — superseded by 2026-09-29 source fixes

The frozen code tree is `21795865986de993a6627977498ee6dcd998fcce`, based on committed HEAD
`c26da5a06f49cdca27723ba4a9ba5d064cda79a2`. It contains 1,082 changed paths versus HEAD
(33,514 insertions and 1,608 deletions). `git diff --check HEAD <tree>` passes, and the shared Git
index remains empty. Its review archive is `scratchpad/review.21795865986d.yYEXUo`; the archive is
not part of the tree. The mutable scope manifest, review log, `.codex/`, `.mcp.json`, `.kioku/`,
`scratchpad/`, and seven pre-existing vendor deletions remain excluded. The temporary index was
initialized from HEAD and only used for this exact snapshot; no commit or push was created.

### Fresh Codex review of predecessor tree `1f432b8d2ca2a9a6970199113491ef772fd009f7`

Three read-only Codex reviewers found four confirmed defects, all addressed in the current tree:

- Cross-class calls to private cancellation marker methods made the PHP prelude invalid. The marker
  now requires a state-owned `stdClass` token; the token is not exposed by the helper, and PHP/AST
  declarations remain parity-checked.
- Runtime string callbacks could obtain descriptors for compiler-only `elephc_parallel_*` externs.
  Runtime extern descriptor generation now excludes that PHP symbol prefix case-insensitively; a
  codegen regression invokes `call_user_func()` through a typed runtime-string parameter and expects
  the undefined-callable failure.
- Callable effect summaries used exact spelling after a case-insensitive PHP name match. Active
  function effects now fall back to `php_symbol_key()` comparison; an uppercase callback regression
  proves the global-storage access is rejected.
- Worker analysis omitted process-global user wrapper/filter registration and several wrapper-aware
  path operations. Registration/unregistration, filter registration, stat/read/path mutation,
  `readfile`, `opendir`, and both `rename()` path operands now mark process-global access; tests cover
  direct registration, unregistration, filter registration, URL stat, unknown paths, and rename
  destinations. The Parallel guide documents the restriction.

The predecessor review's large-finite-timeout finding was retracted after exact-tree verification:
`src/codegen_support/runtime/io/async_poll.rs` already clamps finite waits to signed `INT_MAX` on
both AArch64 and x86_64 before calling `poll(int)`. The reviewed file SHA-256 is
`a293b5ea11414b0ad0b1ef83c6397514edd8b77c52929100e7b5c0d8ac941bbe`.

### Focused validation on the current source/tree

- `cargo test --test codegen_tests async_`: 135 passed, including runtime GC/cancellation and
  Async/Parallel composition.
- `cargo test --test codegen_tests parallel_worker`: 12 passed.
- `cargo test --test codegen_tests parallel_bridge_extern_is_not_dispatchable_through_a_runtime_string`:
  1 passed.
- `cargo test --test error_tests parallel_transfer`: 44 passed.
- `cargo test --lib async_prelude::tests`: 8 passed; `cargo test --lib runtime_extern_descriptor_tests`:
  1 passed.
- A PHP CLI oracle confirmed a state-created cancellation is paired, while an arbitrary `stdClass`
  token cannot forge the marker. `git diff --check` and scoped `rustfmt` pass.

These are focused macOS ARM64 results. Cross-target Linux/iOS validation, CI, and exact-current-tree
Codex/Ollama reviews are still pending. Reviewers must use tree
`21795865986de993a6627977498ee6dcd998fcce`; any further source edit restarts all reviewer rounds.
No consensus, commit, or push is claimed at this checkpoint.

## Current exact code-review snapshot — 2026-09-29

The immutable review tree is `5f7727d1c7b0e47a0d1ff644d4cc3652ae8b4b3d`, based on committed HEAD
`c26da5a06f49cdca27723ba4a9ba5d064cda79a2`. It contains 1,087 changed paths (34,875 insertions,
1,696 deletions). `git diff --check HEAD <tree>` passes and the shared Git index is empty. The
read-only archive is `/tmp/elephc-scheduler-review.o7Kvv5/snapshot.tar` (SHA-256
`7c212f92703735bad22c436ec95c0f28972518f6a68801010971b0221726d052`); the extracted review root is
`/tmp/elephc-scheduler-review.o7Kvv5/snapshot`. A key source blob was verified identical between
the tree and archive. The temporary index started from HEAD and restored seven inherited vendor
deletions only inside that temporary index; the live index was not changed. `.codex/`, `.mcp.json`,
`.kioku/`, `scratchpad/`, this mutable scope/log, and those seven vendor deletions are excluded.

Focused macOS ARM64 evidence on the frozen source: Generator 98/98, Fiber 155/155, Async 147/147,
Parallel execution 47/47, AArch64 runtime emitter 1/1, x86_64 runtime emitter 1/1, and
`cargo build -p elephc --bin elephc` pass. Scoped rustfmt, assembly-comment alignment, and
`git diff --check` pass. The suspended-stack local cleanup regression now proves heap-clean drops
for both Fiber and Generator.

Linux x86_64 runtime acceptance did not reach tests: the host-backed Docker target attempt stopped
with QEMU signal 7 while compiling `icu_properties`; existing Docker volumes were preserved. iOS
target checks and CI remain open. Three read-only Codex reviewers are auditing this exact tree.
Kimi K3 has an Ollama review request in progress for the Async/Fiber/Generator runtime slice; GLM
and DeepSeek passes are pending. No reviewer verdict or consensus is claimed yet. Every source edit
would invalidate this review round. No commit or push was created.

## Current exact code-review snapshot — 2026-09-29

The frozen code tree is `50b9b592c54c4130a9d7e9c73c0a4998b3804fa2`, based on committed HEAD
`c26da5a06f49cdca27723ba4a9ba5d064cda79a2`. It contains 1,084 changed paths versus HEAD
(34,190 insertions and 1,621 deletions). `git diff --check HEAD <tree>` passes; the shared Git index
is empty. Its archive is `scratchpad/review.50b9b59.cC3ykF`; nineteen key source/test/doc blob IDs
match between tree, archive, and live worktree. The mutable scope manifest, review log, `.codex/`,
`.mcp.json`, `.kioku/`, `scratchpad/`, and seven inherited vendor deletions are excluded. A separate
temporary index was initialized from HEAD; no commit or push was created.

This snapshot supersedes `8c81734c125262bdd97a9686afee3df7bee4e36e`: PHP-oracle wrapper probing
expanded the Parallel guard to all wrapper-dispatched file/metadata calls and both `copy()` and
`rename()` operands; registry restore/introspection and callback-capable extern effects are covered
too. The current `parallel_transfer` filter is 66/66 (the source/metadata expansion adds the final
wrapper cases; rerun the filter against this exact tree before handing off).

Focused runtime evidence on the current source: `async_` 141/141, generator 92/92, Parallel
execution 47/47, `parallel_transfer` 66/66, Fiber throw state-error/escaped-Throwable heap tests
2/2, and ARM64/x86_64 runtime emitter tests 1/1 each. Scoped `rustfmt --check`,
`git diff --check`, assembly-comment alignment, and `cargo build --bin elephc` pass.

Linux x86_64 runtime acceptance is still open: `scripts/test-linux-x86_64.sh fiber_` stopped before
tests because Docker's 93.9 GiB filesystem had 0 bytes free (`No space left on device`) while
building in its temporary target volume. The script removed that temporary volume. Existing Docker
target volumes were inspected but not deleted; cleanup requires explicit direction. Kimi/GLM/
DeepSeek and fresh Codex exact-tree reviews have not yet run on this hash. Cross-target Linux/iOS
validation, CI, absolute review consensus, and push remain open.

## Superseding exact code-review snapshot — 2026-09-29

The immutable code tree is `8c81734c125262bdd97a9686afee3df7bee4e36e`, based on committed HEAD
`c26da5a06f49cdca27723ba4a9ba5d064cda79a2`. It contains 1,084 changed paths versus HEAD
(34,140 insertions and 1,621 deletions). `git diff --check HEAD <tree>` passes; the shared Git index
is empty. Its archive is `scratchpad/review.8c81734.dbtp8y`; all sixteen key source/test/doc blob
IDs match between the tree, archive, and live worktree. The mutable scope manifest, review log,
`.codex/`, `.mcp.json`, `.kioku/`, `scratchpad/`, and the seven pre-existing vendor deletions are
excluded. The tree was created using a separate temporary index initialized from HEAD; no commit or
push was created.

### Corrected review leads from tree `21795865986de993a6627977498ee6dcd998fcce`

The prior read-only Codex reviews are evidence about that exact predecessor tree, not acceptance of
the new tree. Their leads have been fixed locally and now have focused regressions:

- Owned temporary Fiber receivers are released on `start`, `resume`, and `throw` state errors,
  returns, and escaped exceptions. `resume()` transfers an owned Mixed cell even when its input
  was already Mixed; rejected and escaping `throw()` paths release owned Throwable arguments too.
  Receiver transfer is derived from EIR definition metadata, not by changing `Ownership::Moved`.
  Internal generator-to-Fiber calls explicitly pass borrowed/private ABI flags on both targets.
- User stream filters cannot be attached inside a worker; user wrappers and wrapper-dispatched
  filesystem calls are rejected. The wrapper path set was cross-checked against PHP's user-wrapper
  callbacks, including `file_get_contents`, `file_put_contents`, `file`, `hash_file`, `copy` on both
  operands, `scandir`, stat/metadata families, and wrapper mutations.
- Process-global writes now cover BC scale, working directory, timezone, environment, umask, iconv
  default encodings, and PCNTL calls whose typed effects write process state. `WRITES_PROCESS`
  remains a coarse side-effect bit and is not blanket-mapped to shared storage.
- Extern functions with callable parameters seed a process-global callback barrier into the
  transitive callable-effect fixed point. First-class extern tasks use their declared signature;
  scalar extern functions remain permitted. Both direct and transitive callback routes have
  rejection coverage.

### Focused validation on this snapshot

- `cargo test --test codegen_tests async_`: 141 passed, including Async/Parallel composition,
  Fiber ownership errors, and heap-debug cleanup.
- `cargo test --test codegen_tests generator`: 92 passed, including generator send/throw and
  delegated Fiber runtime calls.
- `cargo test --test codegen_tests parallel_execution`: 47 passed.
- `cargo test --test codegen_tests parallel_spawn_accepts_static_first_class_function_closure`:
  1 passed; scalar `atoi` extern tasks remain accepted.
- `cargo test --test error_tests parallel_transfer`: 63 passed, including wrapper-dispatch paths,
  process-global mutators, and extern callback re-entry.
- ARM64 and x86_64 runtime emitter tests pass; scoped `rustfmt --check`, `git diff --check`, and
  `scripts/check_asm_comments.py` pass for touched emitter files. `cargo build --bin elephc`
  completed without warnings.

These remain focused host macOS ARM64 results plus target-specific emitter coverage. Linux x86_64 /
AArch64 runtime acceptance, iOS target checks, fresh exact-tree Codex/Kimi/GLM/DeepSeek reviews, CI,
and publication audit are still pending. Every further source change restarts all reviewer rounds.
No consensus, commit, or push is claimed at this checkpoint.
