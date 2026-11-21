# Execution Scheduler exact-hash review log

This file is outside the hashed specification. Each entry identifies the immutable specification
hash and preserves the reviewer output used for the gate. Any material specification edit starts a
new three-review cycle.

## Candidate 51 — `5b81ef47e418b64722f45a0bcfb1859916ca414d9fb89ab8705b697730a5b0f5`

### Kimi K3 — ACCEPT

Complete final JSON received 2026-09-25:

```json
{
  "hash": "5b81ef47e418b64722f45a0bcfb1859916ca414d9fb89ab8705b697730a5b0f5",
  "verdict": "ACCEPT",
  "findings": [],
  "questions": []
}
```

An earlier Candidate-51 attempt returned a truncated, self-contradictory REJECT transcript and is not
counted as a review. This later complete exact-hash response is the valid Kimi review for this round.
GLM 5.3 and DeepSeek V4.1 Flash are pending on the same hash.

### GLM 5.2 Cloud — ACCEPT

GLM 5.3 Cloud did not produce a complete verdict on two attempts: it emitted long internal analysis
instead of the required final JSON, and those transcripts are not counted. A short format probe showed
that GLM 5.2 Cloud follows the JSON contract, so it was used as the GLM-family reviewer for this hash.
Complete final JSON received 2026-09-25:

```json
{
  "exact_candidate_sha256": "5b81ef47e418b64722f45a0bcfb1859916ca414d9fb89ab8705b697730a5b0f5",
  "review_verdict": "ACCEPT",
  "findings": [],
  "questions": []
}
```

DeepSeek V4.1 Flash remains pending on this same hash.

### DeepSeek V4.1 Flash — REJECT

Complete exact-hash JSON received 2026-09-25 for Candidate 51 (`5b81ef47e418b64722f45a0bcfb1859916ca414d9fb89ab8705b697730a5b0f5`). It returned five findings: outside-task await of a Cancelled child/reason, cancellation wake of `Future::join()` on a Queued job, root await of a cancelled child versus root wake exemption, `Future::isComplete()` during the Queued-cancellation transition, and post-close `Awaitable::isComplete()` precedence. Candidate 52 records the corresponding resolutions in the hashed plan. This REJECT invalidates both Candidate-51 ACCEPTs for the current cycle; all three reviewers must review Candidate 52's new hash.

## Candidate 52 — `00429ec8bb8fe8d4e68fc0622ab81b9d76bcd82e3fe4a2358924734e5a605aec`

### Kimi K3 — REJECT

Complete exact-hash review received 2026-09-25. The five findings concerned: (1) the normal
`WaitingTask` terminal-completion edge, (2) cancellation-awaits not being failed-outcome observations,
(3) signaling a blocked `join()` when a Queued job becomes Cancelled, (4) cancellation versus
`mark_running()` after executor dequeue, and (5) implementation-visible Async constructors and
`__Scheduler::runRoot()` permitting alternate root construction. Candidate 53 resolves the lifecycle
and mutex items and changes the supported scope-entry design to private compiler-friend construction;
the corresponding source/AST enforcement remains open implementation work. No other model reviewed
Candidate 52.

## Candidate 53 — `9ad60d844287d29e1a300d10abcaf72c3d6b80a1a89f933fb2230acd2ce78321`

### Kimi K3 — REJECT

Complete exact-hash review received 2026-09-25. Its five findings asked for a direct root-drain
statement, explicit non-root `reschedule()` cancellation checks, outside-task `isComplete()` behavior,
historical scope of the first-review checklist item, and a worker-context guard on Parallel
TaskGroup operations. Candidate 54 addresses these and narrows the Async authority change after
checking tests that construct inert internal records. No other model reviewed Candidate 53.

## Candidate 54 — `b67e42f52a6ef256ee0b8e9e6bc4ff0fd25ef0f657bad13fe6993b2e3a525345`

No reviewer verdict counts. Kimi's first cloud call returned Ollama's weekly usage-limit message
(reference `e4afcfcc-ecb9-4dc4-9b7e-248b73d642e3`). Subsequent three-host focused validation
amended the specification evidence and superseded this hash before any model review.

## Candidate 55 — `e380d79dbef836db41fb3dacc858f91dd85787f1b20cd888d0bd9c0aac113462`

No model reviewed this hash. A prior evidence-only hash
(`c0bed738b4f7e981800415e96874c503334216119c31d2deca915c6a3ef413b9`) was amended before review.
An `isset($this->state)` guard added after the 55 snapshot initially preceded the normal closed-group
cleanup path, producing five macOS `parallel_` failures (54/59). Reordering closed-group release first
restored 59/59 and superseded Candidate 55.

## Candidate 56 — `2a75f69181907f5f87a88c9f63103a32f4b0e3ff740c1084c64e057e4b49e37b`

No model reviewed this hash. The current macOS Async 55/55 and Parallel 59/59 slices, relevant
Linux x86_64/AArch64 focused reducers, and both iOS analysis/refusal tests were recorded there. An
earlier evidence-only hash (`7d52899d20306b9739f6cf4772e2c54a12789cc22003b16a6c97037f1005c1a1`)
also had no model review. Adding the five missing Parallel class contracts and their generated docs
superseded Candidate 56.

## Candidate 57 — `9151cdf759509e8a205b9818ecb2cb3e2130fb9891270e9bd84c8a7d5b7b9e84`

Kimi, GLM, and DeepSeek are pending on this exact hash. The current macOS Async 55/55 and Parallel
62/62 slices, Linux x86_64/AArch64 focused authority and catalog tests, iOS refusal checks, and
generated builtin-doc audits are recorded in the hashed plan. Bridge/prelude/diagnostic slice results
amended the prior evidence-only hash (`5be7932ac74133aaa6eea428daa7ea6cb91263dfc3e5ce4e55146d06f644fe7a`)
before any model reviewed it. A second evidence-only hash
(`da2ec65ec0134dd9ce1e6a9a3f3ee3080668c9112181903c3efff3d59c7dee5a`) was amended with the Async
AST/catalog gate before review. Cloud quota and dedicated code review gates remain open.
An Ollama Kimi availability probe on 2026-09-27 again returned HTTP 429 with the weekly usage-limit
message (reference `7cac1894-603f-4a50-a607-587b63fb4420`). This is not a review verdict.
Candidate 57 was superseded before any model review by the clarified `--with-parallel` link-only
contract and its CLI regression.

## Candidate 58 — `96b007bac23d7afe5f0b5886ac756d6d9571f2a76d84396112c2df09f82f82ae`

Kimi, GLM, and DeepSeek are pending on this exact hash. The macOS CLI link-only regression passes;
the specification, CLI reference, linking guide, and Parallel guide agree on the PHP surface
injection boundary. A prior evidence-only hash (`51bba16c7961feb2f5f9d6eabe7936b179f954350628d6a51fdff76e0b97abfd`)
was amended to describe the detector's conservative final-segment matching before any model
reviewed it. The exact-hash status-history wording was then corrected before any model review,
producing this final Candidate-58 hash. Kimi, GLM 5.2, and DeepSeek availability probes on
2026-09-27 all returned HTTP 429 weekly-limit errors; these are not verdicts. A complete Codex
read-only review and publication audit are still pending.

## Code review tree — `21795865986de993a6627977498ee6dcd998fcce`

This frozen tree is based on `c26da5a06f49cdca27723ba4a9ba5d064cda79a2`; its archive is
`scratchpad/review.21795865986d.yYEXUo`. Focused final-source results recorded at freeze: Async
codegen 135/135, Parallel worker codegen 12/12, Parallel transfer errors 44/44, dynamic-string
Parallel extern regression 1/1, Async prelude parity/injection 8/8, compiler-only extern descriptor
unit test 1/1, and `git diff --check` clean. No commit or push was created.

The fresh Codex review of predecessor tree `1f432b8d2ca2a9a6970199113491ef772fd009f7` confirmed
the private cross-class cancellation marker, variable-string Parallel FFI descriptor exposure,
case-sensitive callable-effect lookup, and missing global wrapper/filter registry/path effects. The
current tree fixes all four. Its proposed large poll-timeout finding was retracted: both AArch64 and
x86_64 clamp to `INT_MAX` before the C `poll(int)` call.

Current-hash Ollama reviews were attempted independently in read-only mode. Kimi K3 returned weekly
limit reference `65e47ebd-6a98-416d-9487-88d88390e2a2`, GLM 5.3 returned
`3fbae6ef-90cc-4b50-9a91-e63b84820e7d`, and DeepSeek V4.1 Flash returned
`c751d1ec-20dc-47b7-8ae4-609aca07ddd0`. These HTTP 429 responses are availability failures, not
verdicts. Fresh Codex reviewers are auditing tree `21795865986de993a6627977498ee6dcd998fcce`;
three-model consensus and publication approval remain open.


## Code review tree — `8c81734c125262bdd97a9686afee3df7bee4e36e`

Frozen 2026-09-29 from HEAD `c26da5a06f49cdca27723ba4a9ba5d064cda79a2`; 1,084 changed paths,
34,140 insertions, and 1,621 deletions. The archive is `scratchpad/review.8c81734.dbtp8y`. The
shared index remains empty. `.codex/`, `.mcp.json`, `.kioku/`, `scratchpad/`, the mutable review
manifest/log, and seven inherited vendor deletions were excluded. Key reviewed blob IDs were checked
against both the frozen tree and archive.

The tree includes fixes for the predecessor Codex findings plus Fiber temporary ownership errors,
process-global Parallel mutations, wrapper-dispatched filesystem calls, and callback-capable externs.
Focused evidence: Async 141/141; generators 92/92; Parallel execution 47/47; Parallel transfer
errors 63/63; AArch64 and x86_64 runtime emitter tests pass; safe scalar extern worker test passes;
`cargo build --bin elephc`, scoped rustfmt, `git diff --check`, and assembly-comment checks pass.

No reviewer has reviewed tree `8c81734c...` yet. Fresh Codex and Ollama rounds must all target this
exact tree; any source change invalidates the round. Cross-target Linux/iOS runtime acceptance, CI,
review consensus, and push remain open. No commit or push was created.

## Current exact code-review tree — `5f7727d1c7b0e47a0d1ff644d4cc3652ae8b4b3d`

Frozen 2026-09-29 from committed HEAD `c26da5a06f49cdca27723ba4a9ba5d064cda79a2`; 1,087 changed
paths, 34,875 insertions, and 1,696 deletions. `git diff --check HEAD <tree>` passes. The isolated
archive is `/tmp/elephc-scheduler-review.o7Kvv5/snapshot.tar` with SHA-256
`7c212f92703735bad22c436ec95c0f28972518f6a68801010971b0221726d052`; its extracted root is
`/tmp/elephc-scheduler-review.o7Kvv5/snapshot`. The live shared index remains empty. The temporary
index was initialized from HEAD, selected the whole implementation/tests/docs/plans snapshot, and
restored the seven inherited vendor deletions only in the temporary index. `.codex/`, `.mcp.json`,
`.kioku/`, `scratchpad/`, the mutable review scope/log, and those vendor deletions are excluded. No
commit or push was created.

Current focused macOS evidence: Generator 98/98, Fiber 155/155, Async 147/147, Parallel execution
47/47, AArch64 runtime emitter 1/1, x86_64 runtime emitter 1/1, `cargo build -p elephc --bin
elephc`, scoped rustfmt, assembly-comment alignment, and `git diff --check` pass. The new suspended
stack local cleanup regressions pass for both Generator and Fiber.

Linux x86_64 Docker execution did not reach tests: the host-backed QEMU attempt stopped with target
signal 7 while compiling `icu_properties`; existing Docker volumes were preserved. iOS target
checks and CI remain open. Three independent read-only Codex reviews are running on this tree.
Kimi K3 has an Ollama review request running for the Async/Fiber/Generator runtime slice; GLM and
DeepSeek reviews are pending. No reviewer verdict or consensus is claimed yet. Any source edit
invalidates this tree and all reviews.

### Independent Codex reviews on tree `5f7727d1c7b0e47a0d1ff644d4cc3652ae8b4b3d`

**Scheduler/API reviewer — BLOCK.** Static-only findings (not reproduced):

1. P2: `getenv()` is absent from the process-global storage rejection path. The reviewer traced
   `src/optimize/effects/calls.rs:62-82`, `src/types/checker/parallel_transfer.rs:208-232`, and
   `docs/beyond-php/parallel.md:105-108`; a worker callback can read process-shared environment
   state despite the documented restriction. Suggested fix: classify `getenv()` and audit other
   process-global accessors, then add a compile-rejection regression.
2. P2: public `TaskGroup::spawn(callable $task, mixed ...$args)` accepts an arbitrary variadic list,
   while `__TaskStart::start()` throws `Error` for more than seven arguments at runtime. The reviewer
   traced `src/async_prelude/source.php:254-272` and `1074-1078`; suggested fix is either arbitrary
   arity transport or explicit validation/documentation of the seven-argument limit.

**Runtime reviewer — BLOCK.** Independently confirmed the `getenv()` issue by tracing
`src/ir/runtime_fn.rs:1200`, `src/optimize.rs:485`, `src/optimize/effects/calls.rs:64`,
`src/types/checker/parallel_transfer.rs:227`, and `docs/beyond-php/parallel.md:105`. It is also
static-only and not yet reproduced. No other finding was returned in its assigned runtime/ABI scope.

The Parallel/isolation reviewer later returned the additional static findings below. These BLOCK
reports are findings on this exact tree, not acceptance; no source edits were made during the exact-
tree review round.

**Parallel/isolation reviewer — BLOCK.** Three additional static findings, not reproduced:

1. P1: Enum singletons use process-global `_enum_case_*` slots while their objects are allocated
   in the current `_rt_ctx`. The reviewer traced `src/codegen/enum_singletons.rs:159-171,394-416`,
   `src/codegen_support/runtime/data/user.rs:113-125`, and the per-context symbol table in
   `src/codegen_support/runtime/ctx.rs:1244-1307`. An enum value in a worker can leave a context
   arena allocation rooted in a global slot, causing context drain to quarantine backing and
   eventually exhaust the executor's worker-context pool. Suggested fix: make singleton ownership
   context-local or refuse worker materialization until lifecycle support exists.
2. P1: Process-global configuration readers pass the Parallel guard. The reviewer traced the
   missing getter classification in `src/optimize/effects/calls.rs:62-85`, the checker gate at
   `src/types/checker/parallel_transfer.rs:227-231`, and the global timezone buffer read in
   `src/codegen_support/runtime/system/date_default_timezone.rs:101-108` plus declarations in
   `src/codegen_support/runtime/data/fixed.rs:200-205`. A worker can observe or race a parent's
   `date_default_timezone_set()`. Suggested fix: classify process-global readers/transitive
   accesses or isolate that state per context.
3. P2: Unclassified extern functions without `callable` parameters are permitted in worker tasks.
   The reviewer traced extern inventory at `src/types/checker/driver/mod.rs:394-408` and the
   callback-only safety condition at `src/types/checker/parallel_transfer.rs:258-274`. An extern
   `setenv()` target could mutate process environment from a worker. Suggested fix: reject
   unclassified externs in workers unless an explicit worker-safe contract/allowlist exists.

The reviewer supplied PHP repro sketches and neighboring tests but did not execute them. It found
no additional packaging/linking or target-diagnostic blocker. Together, the three Codex reviewers
have independently blocked this exact tree; the `getenv()` finding is independently reported by
both Scheduler/API and Runtime reviewers. No consensus is possible before triage/reproduction.

### Source-checked reproductions against the frozen source

The PHP reproducers are temporary, outside the worktree, under
`/tmp/elephc-scheduler-repro.8mjtp1/`; each was compiled with the current `target/debug/elephc`
binary from this source and run locally. No tracked source changed.

- `parallel_getenv.php`: worker sleeps, reads `getenv()` after the parent calls `putenv()`;
  executable returns `after` (expected isolated snapshot: `before`). Confirms both Codex getenv
  findings as observable process-state leakage.
- `parallel_timezone.php`: worker sleeps before `date_default_timezone_get()`, while the parent
  changes the timezone after spawn; executable returns `Pacific/Honolulu`, not the worker's
  `UTC` snapshot. Confirms cross-context timezone visibility.
- `parallel_setenv.php`: a first-class `extern setenv(...)` worker mutates the process environment;
  parent-side `getenv()` prints `worker`. Confirms unclassified scalar externs can mutate process
  state through a worker.
- `parallel_enum.php`: seven concurrent enum singleton reads return the expected case names, so a
  single scope does not expose the leak directly.
- `parallel_enum_pool.php`: one scope materializes seven distinct enum case singletons and succeeds;
  a second scope then exits 255 with `Fatal error: Uncaught Elephc\\Parallel\\TaskFailure:
  Parallel worker context was unavailable`. Confirms context exhaustion/quarantine caused by
  global enum singleton slots retaining context-owned allocations.
- `async_eight_args.php`: the public variadic spawn call compiles, then outputs
  `Error:Elephc Async tasks accept at most seven arguments`. Confirms the advertised variadic shape
  is not an arbitrary-arity runtime contract; whether this is a product bug or an undocumented v1
  limit remains to be decided against the frozen spec.
- `parallel_json_state.php`: worker A decodes malformed JSON, waits, then reads `json_last_error()`;
  worker B decodes valid JSON between those steps. Output is `0|0`, while the PHP CLI oracle for the
  first operation returns `4`. `_json_*` state remains `.comm` storage and is absent from
  `PER_CONTEXT_SYMBOLS`.
- `parallel_stream_context.php`: the parent creates a context with HTTP method `GET`; a worker
  creates one with method `POST`; after joining, the parent reads its original context and gets
  `POST`. `_stream_context_options` is the process-global single slot used by both create and
  get-options lowering, and is absent from context routing.
- The same reproducer confirms the stream-context pointer persists across the worker boundary long
  enough for the parent to observe the worker's arena-owned options hash; the notification-callback
  pointer uses an analogous process-global slot but has not been executed independently yet.
- `parallel_strtotime_race.php`: two worker loops call `strtotime("tomorrow", 0)` and
  `strtotime("tomorrow", 86400)` respectively, checking `86400` and `172800` over 20,000
  iterations. The executable reported `1|3` mismatches, confirming `_strtotime_clock` cross-worker
  interference on macOS ARM64.
- A standalone Linux ARM64 musl C probe (container `elephc-test-linux-arm64`) forced thread A to
  retain `localtime(0)`'s returned pointer while thread B called `localtime(1704067200)`; A then read
  `tm_year == 124` rather than `70`. This confirms the underlying libc static-buffer race on a
  supported Linux target. Elephc's emitter retains that pointer across a hash allocation before
  reading it; a generated-PHP Linux reproduction is still pending.

The context-isolation Codex follow-up also found a public safe-Rust FFI soundness issue in the
Parallel value codec: `ParallelTransferBlob` has public raw pointer/length fields, and safe
`elephc_parallel_value_validate()` reaches `from_raw_parts` without a validity precondition. A safe
Rust call with `NonNull::<u8>::dangling().as_ptr()` and length 8 is a source-level UB trigger; it was
not executed. The same API review found safe writer functions that dereference caller-provided raw
writer pointers. The proposed boundary is to mark raw-pointer FFI entry points `unsafe extern "C"`
with explicit safety contracts (preserving C ABI), or replace them with safe owned/slice handles.

### Source triage of additional Ollama leads

- DeepSeek's AArch64 process-exit syscall lead is retracted: the snapshot's `Emitter::syscall(1)`
  passes through `Target::emit_linux_syscall()` and `map_syscall(1) == 94`, while macOS uses syscall
  1 directly. The emitter deliberately maps Darwin syscall numbers for Linux ARM64.
- DeepSeek's `_stream_grow_scratch` race is not counted yet: the symbol is indeed process-global and
  absent from `PER_CONTEXT_SYMBOLS`, but `stream_filter_append/prepend` are classified as global and
  wrapper-aware paths containing `://` are rejected in Parallel workers. Reachability of the filter
  scratch from an allowed worker path remains to be established.
- Kimi's Parallel bridge OOM lead is source-grounded but not allocation-failure reproduced:
  `elephc_parallel_php_buffer_alloc()` returns null after `try_reserve_exact()` failure, the prelude
  does not test that pointer before `__elephc_ptr_write_string()`, and the emitted byte-copy helper
  writes through the destination without a null guard. Severity/recovery path remain under review.
- Kimi's jmp_buf-size lead is not confirmed: the local Linux ARM64 Alpine container reports
  `sizeof(jmp_buf) == 312`, but an isolated `setjmp()` call changed bytes only through offset 175
  and `longjmp()` returned successfully with sentinel bytes beyond that range. The reported struct
  size alone therefore does not prove that this call overruns the 224 bytes available in the frame;
  the libc-specific call path needs separate evidence.

Kimi's first slice also flagged the ARM64 `__rt_fiber_resume` state-error path reading ownership
flags at `[sp,#24]`/`[sp,#32]` although the frame stores them at `[sp,#8]`/`[sp,#16]`. The claimed
uninitialized `x20` subfinding is contradicted by the same snapshot's `mov x20, x1` before the
state guard; the offset defect remains under review for reachability and severity.

### Source follow-up fixes after frozen tree `5f7727d1c7b0e47a0d1ff644d4cc3652ae8b4b3d` — 2026-09-29

The following source changes invalidate that review hash; no fresh immutable tree or model verdict
exists yet.

- **Rust FFI soundness:** public Parallel value-codec exports that dereference raw writer/blob
  pointers are now `unsafe extern "C"`, with documented validity/ownership preconditions. The C ABI
  symbols and calling convention are unchanged. Compile-fail doctests prove safe Rust cannot call
  the writer or validator with unchecked raw pointers. Kioku packet:
  `bug_fix-parallel-value-codec-raw-pointers-require-unsafe-cb0e8597`.
- **Runtime-state isolation:** `Effect` and `ParallelCallableSafety` now carry a distinct
  `uses_process_global_runtime_state` bit; generic EIR `READS_GLOBAL`/`WRITES_GLOBAL` remains
  separate, preserving the Kioku decision that ordinary context-owned registries/resources must not
  be mislabeled as PHP globals. Worker call graphs fail closed for environment/timezone access,
  `json_*`, `stream_context_*`, `strtotime`, local/gm date conversion paths, and unclassified native
  calls. `atoi` remains the only explicit user-declared native worker-safe exception. The Parallel
  guide documents these v1 restrictions.
- **Enum singleton poisoning:** the Parallel callable-effect analysis collects enum case names and
  marks direct `E::Case` reads plus enum static case factories as process-global runtime access.
  Workers now fail at compile time instead of publishing worker-arena singleton pointers into global
  enum slots and quarantining contexts.

Current focused evidence on the edited worktree:

- `cargo test -p elephc-parallel`: 42 unit tests and 2 compile-fail doctests passed; the crate build
  also passed.
- `cargo test --test error_tests parallel_transfer`: 75 passed, including new environment/timezone,
  JSON/time scratch, stream-context, unclassified external-call, direct enum-case, and enum-factory
  refusal regressions.
- `cargo test --test codegen_tests parallel_spawn_accepts_static_first_class_function_closure`:
  1 passed, retaining the reviewed `atoi` worker-safe exception and builtin/user-function workers.
- These are focused macOS checks only. The Linux x86_64 Docker QEMU failure recorded above remains
  unresolved; no iOS or current exact-hash model review has been run.

The earlier Codex/Ollama outputs are evidence about the old snapshot only. After remaining source
triage, create a new immutable review tree and restart all dedicated code/model reviews against that
same hash; absolute consensus and publication scope remain open.

### Candidate 59 and Async spawn arity correction — 2026-09-29

The Async reviewer found that `TaskGroup::spawn(callable, mixed ...$args)` statically accepted eight
task arguments, then `__TaskStart::start()` raised an Error only after the scheduler had created the
child. The underlying `Fiber::start()` ABI supports at most seven task arguments. Candidate 59 now
states that bound explicitly (spec SHA-256
`77cfcacab7a99b73d8728178ac66254d058f0dc7a7bb2e975466fc7e3d2b82c0`).

- Type checking rejects statically known `Async\TaskGroup::spawn()` calls with more than seven task
  arguments. The public prelude method also checks the expanded variadic array before asking the
  scheduler to create a child, covering dynamic argument spreads; the internal Fiber-start check
  remains defensive. Source PHP and generated prelude AST are kept in parity.
- `cargo test --test error_tests async_spawn_rejects_more_than_seven_statically_known_task_arguments`:
  1 passed.
- `cargo test --test codegen_tests async_spawn_checks_dynamic_argument_spreads_before_creating_a_task`:
  1 passed; the task body is not run on overflow.
- `cargo test --lib async_prelude::tests`: 8 passed. `cargo build -p elephc --bin elephc` passed.

This spec amendment also invalidates any earlier specification hash/review verdict. The code tree
still needs a new freeze and a complete exact-hash reviewer round; no model consensus is claimed.

### Parallel OOM wire-buffer guard — 2026-09-29

Kimi's source lead about `elephc_parallel_php_buffer_alloc()` returning null was addressed in both
`TaskGroup::__completeJobValue()` and `TaskGroup::__failJobValue()`: each throws a worker-side Error
before `__elephc_ptr_write_string()` can write through a null pointer. The executor's existing
post-callback `ensure_terminal()` publishes a terminal infrastructure failure if transfer cannot
complete. PHP and generated AST forms remain covered by the Parallel prelude parity test.

- `cargo test -p elephc-parallel php_transfer_buffer_allocation_fails_closed_on_capacity_overflow`:
  1 passed; `usize::MAX` proves the native allocator fails closed with null.
- `cargo test --lib parallel_prelude::tests`: 8 passed.
- `cargo test --test codegen_tests parallel_worker`: 12 passed, including fatal containment and
  result/failure wire-transfer heap-cleanliness regressions.

The first broad unknown-static-call guard also treated PHP syntax constructs as opaque native calls
and rejected worker `exit()` cleanup regressions. The classifier now keeps `isset`, `unset`, `empty`,
`exit`, and `die` distinct from native externs; the complete `parallel_worker` slice passes after
that correction. The exact source tree is still unfrozen and all model reviews remain pending.

The DeepSeek test-ABI lead was also reproduced by source inspection: the executor test's
`worker_entry` callback had four parameters but was cast to the five-argument `WorkerEntry` C ABI.
The test shim now accepts the fifth release-status pointer, and a compile-time type assignment pins
the signature. `cargo test -p elephc-parallel` passes **43 unit tests plus 2 compile-fail doctests**.

### Ollama lead reconciliation — 2026-09-29

- **Kimi Fiber resume ownership offsets: retracted.** The AArch64 helper reserves 48 bytes, stores
  `receiver_owned`/`resume_value_owned` at `[sp,#8]`/`[sp,#16]`, then pre-decrements SP by another 16
  bytes with `stp x20, x21, [sp, #-16]!` before the guard. The reported `[sp,#24]`/`[sp,#32]` reads
  therefore address those same two slots relative to the new SP. `x20` is loaded from `x1` before
  the guard and is callee-saved. No source correction is warranted.
- **Kimi job-status collision: no hang path found.** `finish_job()` can return status 1 for an
  invalid envelope, the same integer as success, but the invalid branch does not transition the job
  out of Running. The owning executor calls `ensure_terminal()` after every worker callback; it
  observes a still-Running job and publishes an infrastructure failure before `job_wait()` can
  remain blocked. This remains covered by the worker terminal/status tests.

### Failure-envelope length overflow and Linux ARM64 compile checkpoint — 2026-09-29

DeepSeek's failure-message length truncation lead was source-confirmed: the failure encoder narrowed
the required message length with `as u32` while optional/frame lengths used checked conversions.
All encoded lengths now pass through `wire_u32_len()` before serialization; the failure kind/frame
format rejects overflow instead of emitting a truncated, unparsable envelope. A unit test covers
the u32 boundary without allocating a multi-gigabyte buffer.

- `cargo test -p elephc-parallel`: 44 unit tests and 2 compile-fail doctests passed.
- The first Linux ARM64 filtered run did not reach tests: it found a compile error in the new length
  guard (`Option::map_err`). That was corrected to `Option::ok_or`; the same preserved
  `elephc-target-linux-arm64-parallel-v1` volume is being reused for a fresh ARM64 run. This is not
  yet a Linux pass. The container was stopped by the script; `ELEPHC_KEEP_DOCKER_TARGET_VOLUME=1`
  kept the volume, and no Docker volumes were removed.
- Fresh Linux ARM64 rerun on the corrected, unchanged worktree:
  `ELEPHC_DOCKER_TARGET_VOLUME=elephc-target-linux-arm64-parallel-v1 ELEPHC_KEEP_DOCKER_TARGET_VOLUME=1 ./scripts/test-linux-arm64.sh parallel_worker` passed **12/12**.
  The script rebuilt bridge staticlibs and `elephc`, installed the managed libxml2 package, and then
  executed the focused tests. The temporary container exited; the explicit target volume remains.

## Candidate 59 exact-hash outcome — 2026-09-30

Kimi K3, GLM 5.3, and DeepSeek V4.1 all returned BLOCK for specification hash
77cfcacab7a99b73d8728178ac66254d058f0dc7a7bb2e975466fc7e3d2b82c0. The candidate is rejected:
it left Candidate 58 as a competing review authority, narrowed the accepted variadic Async API to
seven arguments to match an implementation detail, and omitted exact diagnostics/order, the
Awaitable method name, and Parallel arity.

The replacement proposal is Candidate 60: preserve unbounded Async spawn arguments with the
existing direct Fiber path through seven values, then capture the callable and variadic array in a
child-Fiber trampoline for larger argument lists. A probe confirms the trampoline adds one visible
closure frame to debug_backtrace() relative to a direct Fiber callback; reviewers must explicitly
assess that trade-off. If rejected, pursue descriptor-level Fiber argument transport rather than a
public cap.
No Candidate 60 code or model review has occurred yet.

## Candidate 61 immutable review and reproduced blockers — 2026-09-30

Candidate 60 was superseded before review. Candidate 61 reconciled the unconditional zero-argument
Async runner, normative Awaitable naming/raw-suspend behavior and associative variadic EIR change.
Its specification SHA-256 is `f7bc90281a8143af9ca69ee5313940ea443681b7b2b5444ee645af63e669b549`.
The detached review commit is `45b1dc6d145b6e3303d470cf9178355715cb70df`, tree
`29e53de7fa35aff375692de2599cec6ddbd7bbff`, parent
`c26da5a06f49cdca27723ba4a9ba5d064cda79a2`. The checkout and independent model transcripts are
under `/tmp/elephc-scheduler-c61.CEmmpK`. The shared index/branch were not changed; the detached
commit is an unpublished review artifact. The seven inherited vendor deletions and local tool/store
material remain excluded. The specification checksum and frozen diff hygiene passed.

Two independent Codex reviewers returned BLOCK on that exact artifact. Parent reproductions with
a freshly rebuilt compiler confirmed:

- **Positional array regression:** `product(int ...$xs)` calling `array_product($xs)` refuses the
  new Hash argument; PHP returns 6 for `(2, 3)`. The backend audit identified the complete affected
  array-consumer families, recorded in `execution-scheduler-php-array-abi.md`.
- **Declared-array return transport:** `collect(mixed ...$xs): array { return $xs; }` called with
  `foo: 42` refuses Hash-to-Array return transport; PHP emits `{"foo":42}`.
- **Wrong aggregate/helper ABI:** a float variadic sum `(1.5, 2.5)` returns 2.5 instead of 4;
  ordinary integer-valued associative `array_sum()` returns 0 instead of 5.
- **Invalid dynamic parameter binding:** `relay(callable, mixed)` supplying `'abc'` through
  `call_user_func_array()` to an `int` variadic prints `0|accepted`; PHP catches `TypeError`.
  The static constant call is already checker-rejected; it is not sufficient regression coverage.
- **Escaped cloned Async authority:** an explicit `: mixed` root returning `clone $tasks` permits
  post-scope `cancel()` and `cancellation()`; both must report the ended scope.
- **Caught Parallel failure:** after catching a throwing child's `join()`, a retained token reports
  not requested and a late spawn succeeds (`caught|not-cancelled|spawned`). Fail-fast must already
  request cancellation and reject late admission. The initial `: void` root probe hit a separate
  backend limitation, so the verified reproducer uses an explicit integer root return.
- **Namespace alias detection:** `use Elephc\Async as A; A\run(...)` fails with unknown
  `Elephc\Async\TaskGroup` because injection missed the ordinary namespace alias.
- **Public documentation:** the Parallel guide retained the superseded direct-suspend/deadlock
  description instead of the immediate Async-specific Error.

Named by-reference variadic mutation also leaves the caller at 1 instead of PHP's 9. Source history
shows reference collectors were already forced to indexed storage and excluded from shape
specialization; this is inherited evidence, not claimed as a newly introduced migration regression.
The throwing Fiber-capture destructor probe matched PHP-visible output and is not an established
finding; additional ownership instrumentation would be needed before elevating it.

Source fixes for shared Async group liveness, Parallel failed-join cancellation/outcome observation,
scoped namespace alias/relative-call detection and Parallel documentation are now implemented with
PHP/generated-AST parity edits and regressions. Their post-edit test execution is still pending.
The callback parameter verifier is being corrected. Blanket Hash storage must be replaced by a
complete PHP-array transport/consumer contract; isolated consumer patches cannot establish closure.
These live edits invalidate Candidate 61 acceptance and require a fresh complete reviewer round.

External reviewers were launched by a dedicated Codex instance via the Ollama profile on the same
detached artifact. Kimi performed source inspection but terminated without a verdict at the session
usage limit (reference `8aff3ee7-ea04-420d-a440-7cc9fbb69f2b`). DeepSeek's initial session and one
bounded resume both ended without a final message because the provider emitted malformed DSML
tool-closing tokens; exit 0 and empty output are not acceptance. A tool-free native Ollama chat
diagnostic returned `PROTOCOL_OK`; the dedicated driver may use complete exact-hash source packets
and coverage slices through that route on the next corrected artifact. GLM is still inspecting the
artifact; its provisional eval descriptor ABI lead remains to be reproduced. All complete logs and
protocol diagnostics are retained under the results directory. No reviewer consensus or push occurs.

Current fresh host evidence includes named-variadic regressions 3/3 in addition to the previously
rerun Async spawn 7/7 and general variadics 53/53. Docker's filesystem has only 947.4 MiB available;
no existing cache/volume was deleted. A minimal host-generated-assembly/container link path is being
evaluated to obtain Linux runtime evidence without another large compiler build under QEMU.

GLM subsequently terminated without a final verdict at the same Ollama session quota (reference
`6be66768-f53e-4014-af06-90c7c09a70f3`). All external processes are terminal and none returned
ACCEPT. Their complete availability/protocol bundle and partial source transcripts are copied to
the durable local `scratchpad/reviews/c61/` directory; the immutable checkout remains clean.

The host-generated Linux x86_64 assembly plus runtime linked and executed successfully in the
existing container image, reproducing `2.5|0` for the wrong-sum probe without building Rust inside
Docker. Host cross-builds of `elephc-crypto` and `elephc-parallel` now pass for both
`x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`; the target standard libraries were
installed on the host. Their artifacts live under the host worktree target directory. This is build
and target-probe tooling evidence, not current-source scheduler runtime acceptance.

Kioku decision `decision-candidate-61-uniform-hash-variadic-storage-rejec-912d9a3e` supersedes
`bug_fix-associative-storage-for-variadic-captures-ec31b8d7`: the earlier narrow-test packet must not
be recalled as an accepted universal collector fix. The active array ABI plan now records a concrete
array-only raw-pointer storage direction reusing Iterable representation and common runtime shape
dispatch, with the full consumer/return/reference implementation gate still open.

Candidate 62 is now the sole live implementation draft; specification checksum
`0621f4c3e5b0b91941e5a42f7ee50acac37f5f4facba62a24c110e7ac29f2bd1` verifies. Its source is not
frozen and no model acceptance exists. Candidate 61's archived checksums remain unchanged.

The first native live-fix checkpoint compiled production code, then exposed a misplaced Rust test
inside an existing PHP raw-string fixture; the original fixture was restored and both test modules
were explicitly formatted. The descriptor binding filter then executed: invalid fixed and named
binding tests passed, but five valid-positional/Stringable tests failed. This proved a pre-existing
double variadic packing at the ClosureCall descriptor fallback, rather than a reason to weaken the
parameter validator. The forwarding boundary is being corrected and the tests remain unchanged.

Direct execution of that compiled checkpoint proved cloned Async authority 1/1 and the Async
namespace-alias/relative fixture group passed. Parallel fail-fast/alias/escaped-failure tests exposed
the new Future's cross-class private `__cancelAll()` call; the source now uses the existing guarded
public `cancel()` entry. The new private attach-scope edge is restricted to compiler-generated
TaskGroup code. These Parallel fixes required a fresh test run, recorded below.

### Live correction checkpoint (not a frozen review artifact)

After correcting the canonical lowercase friend-method keys, the compiled live checkpoint passes
`caught_join_failure` 1/1, `private_scope_helpers` 1/1, escaped failed Future retention/heap cleanup
2/2, and escaped successful Future cache heap cleanup 1/1. The earlier clone authority and Async
alias fixtures also passed. A combined namespace-alias rerun lost its completed process output;
the handle is absent and no matching process remains, so it is not counted as a fresh pass and must
be rerun at the next checkpoint.

The descriptor binding group expanded to 14 meaningful regressions. Its latest completed execution
is 11 passed, 3 failed: typed mixed named/positional closure forwarding SIGSEGV; ordinary and named
Array-prefix `__toString()` throw paths produce correct PHP-visible messages but leak 7 heap blocks
(312 and 1352 bytes respectively). Normal Stringable coercion, invalid types, numeric bounds,
Array defaults/COW, untyped inference control and Mixed/string return aliases passed. Assertions
have not been weakened.

LLDB on the typed-closure reproducer confirms `__rt_mixed_unbox` dereferencing address `0x14`, the
literal integer 20: associative tail coercion must wrap the actual raw tag/payload in a borrowed
Mixed view rather than assume every low word is a cell pointer. The throw-path leaks trace to raw
argument-array and Object-to-Mixed boxes allocated implicitly in backend ABI materialization; their
owners are invisible to EIR's exception-safe cleanup. The source correction now moves those
allocations into explicit EIR ownership/argument normalization and reuses alias-aware cleanup on
success. Sources are held for a fresh `descriptor_param_binding` run, which is currently compiling.

The ordinary user-call ABI borrows parameter pointers. Callee entry independently retains only
owned/mutating local parameter slots; invoker argument leases remain caller-owned through return
or unwind. Return aliases must be stabilized before releasing those leases, and consuming carrier
conversions must update the ledger's actual pointer/kind. An owned mutating-parameter Array return
remains a separate focused ownership probe to execute before acceptance.

After this checkpoint, run both prelude parity tests, wrapper-layout and scoped detector unit tests,
then the affected scheduler/heap regressions. The full PHP-array ABI/consumer implementation,
current-source target matrix, fresh immutable review round and publication are still required.
No branch commit, consensus or push has occurred.

The latest source-ready correction includes the raw Hash Mixed view, explicit pre-invoke EIR
container boxing, effective user-function Mixed parameter boxing, and alias-aware normal cleanup
for exception-safe calls. Both agents have held production sources for verification. The first
rebuild was terminated by SIGTERM (exit 143) without a Rust error or test result; the machine still
had 908 GiB available, and the observed Cargo processes subsequently disappeared. The same focused
command was restarted only after that terminal/process check. Its live exec handle is 98687:
`CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 RUST_MIN_STACK=67108864 cargo test --test codegen_tests descriptor_param_binding -- --test-threads=1`.
It is still compiling at this checkpoint; 11/14 remains the latest completed binding result.

That process subsequently completed: 10/14 passed. Typed named/positional closure forwarding now
passes, but Array default and named Array-prefix COW regressions fail with bad refcount, and both
Stringable throw regressions fail during Mixed-to-string register materialization. These are
current failures, not acceptance. Backend investigation identified duplicate normal-path release
of the same EIR-prebuilt argument box (backend release followed by explicit EIR release). The
constructor failures exposed regular user-call boxing being applied to native constructor ABI;
the correction restricts it to resolved user functions. Both corrections require a fresh run.

Read-only AST helper feasibility probes suggest native compiler-owned helpers can reuse Mixed
iteration once loop-carried local storage is kept stable. This does not close the array ABI gate:
sparse integer writes currently create null gaps instead of preserving sparse PHP keys, generic
unset and reference-aware mutation remain incomplete, and by-value Mixed cell detachment must be
rechecked after the current EIR corrections. No helper architecture has been adopted merely from
this older-binary probe; the complete consumer/return/reference scope remains authoritative.

The completed checkpoint executable was also run with `namespace_alias --test-threads=1`: 5/7
passed; the Async and Parallel alias fixtures both hit the same Mixed-to-string materialization
error as the Stringable throw tests. This replaces the earlier missing-output namespace rerun with
an actual negative result. Rerun this group after the native-constructor ABI correction; no new
source compilation or broader pass is inferred from this older executable.

The backend owner subsequently held sources stable after removing the duplicate success release
and restricting regular ABI boxing to resolved user functions. Targeted rustfmt and diff hygiene
passed; assertions remain unchanged. The fresh serialized Cargo binding checkpoint is now running
under exec handle 54698. No outcome is recorded until that process actually finishes. The unchanged
Candidate 62 specification companion checksum also verifies from `.plans/`.

Fresh checkpoint 54698 completed with 12/14 passing. Both Stringable throw tests now pass; the
double-release bad-refcount failures are gone. Array default and named Array-prefix COW still fail
their unchanged heap assertions with one live 80-byte block each (7 allocations/6 frees and
18/17 respectively); their PHP-visible output assertions pass. These two leaks remain blockers.

On this same compiled checkpoint, `namespace_alias --test-threads=1` completed 7/7 passing,
including both Async and Parallel fixtures. `scratchpad/c62_descriptor_array_return.php` was
cross-checked with PHP, compiled by current `target/debug/elephc --quiet --heap-debug`, then run:
user-function mutated Array return `[99,22]|[11,22]`, borrowed return `[11,22]|[11,22]`, and
closure mutated Array return `[88,22]|[11,22]` all match PHP. Heap report is 20 allocations/20
frees, zero live blocks and bytes. This particular return-owner reproducer is clean; it does not
prove every returned COW shadow or associative return path.

Additional runs of this checkpoint pass cloned TaskGroup authority 1/1, caught join-failure
fail-fast/late-spawn rejection 1/1, escaped failed Future functional/heap regressions 2/2, and
private scope-helper access rejection 1/1. Test executable SHA-256 is
`9021edd3dfc1732a307e3dd5464a1bebea069fb5cb33d9027579c91c416e25e2`; compiler executable is
`6116ece70b451b4c8c0bef7326469f7d806caa34dbcdc48c0cd552d5bc64e990`. These identify tested
binaries, not a complete immutable source review artifact. The backend owner may now edit source
to address the remaining two Array leases; subsequent source changes require a new checkpoint.

Kioku measurement `measurement-candidate-62-binding-12-of-14-with-two-array-lea-b9b23fbe`
supersedes the prior 10/14 checkpoint packet. Semantic recall completed with relevant rejection
context and a drift warning on the earlier measurement; it was significantly slower than lexical
recall during concurrent host builds. This observation does not establish an embedding or Kioku
root cause. No stale measurement has been used as current acceptance evidence.

The compiler checkpoint above also freshly compiled the stable-Mixed AST helper probe with heap
debug. PHP output is `4|6|{"1":2,"2":3}|{"b":2,"c":3}|{"b":2,"a":1}|{"a":1}|{"a":1,"changed":9}`.
Native output is `4|6|[null,2,3]|{"b":2,"c":3}|{"b":2,"a":1}|{"a":1,"changed":9}|{"a":1,"changed":9}`,
with 101 allocations/95 frees and six live blocks/280 bytes. Sparse-key loss and by-value boxed-cell
aliasing therefore remain current defects, not merely historical probe results. The composite
probe's leak report is real but is not attributed to one primitive without isolation.

The scheduler correction owner is now implementing boxed sparse-write and nested fetch-for-write
promotion for both architecture emitters, plus focused exact-shape/heap regressions. Backend owner
continues investigating the two 80-byte callback Array leases. The broader raw erased-array storage
contract, deletion, by-value detachment, references and consumer inventory remain open; this bounded
primitive step does not replace or close them. Heavy builds remain parent-serialized.

The backend owner's LLDB inspection corrects the provisional lease attribution: each remaining
80-byte block is a first-class callback descriptor (payload kind 11, name `consume`), not an Array
allocation. Actual Arrays and Mixed boxes are released. Unknown return-alias handling conservatively
skips releasing a raw Callable argument when the call returns boxed Mixed, retaining the descriptor.
The proposed correction releases the caller's raw descriptor lease independently, since a returned
Mixed box retains its own payload lease; Mixed-to-Mixed alias guards must remain intact. A returned
callable-in-Mixed lifetime control is required before accepting this correction. No additional Array
decrement is justified by the leak report.

The local Linux runtime-emitter tool was rebuilt against the checkpoint's current default-feature
rlib `libelephc-6673dddd655db46d.rlib`, not the older `0071390b50be5c34` artifact. Linux probes use
that emitter and the already-tested compiler binary; in-flight source edits are not part of their
evidence. The first x86_64 return probe is running under handle 12010.

That first local harness invocation failed at link: the standalone emitter unnecessarily enabled
eval-scope reflection helpers, whose metadata was correctly absent from this non-eval program.
The local emitter now makes eval-scope explicitly opt-in; no missing symbol was stubbed and no
production emitter was changed. Rebuilt emitter SHA-256 is
`3a4fe6ccb52a31bd816e19691c943385360dfed30e52deea990ebaf3667646a0`. The x86_64 probe then
assembled, linked and executed successfully in the existing Linux image, producing all three
PHP oracle lines. Artifacts and result log are `/tmp/elephc-linux-emitted.MUgYZ4`. This is a focused
target-functional result, not Linux heap-debug or full scheduler acceptance. Linux AArch64 follows.

Linux AArch64 also assembled, linked and executed the same return probe with all three oracle
lines, artifacts `/tmp/elephc-linux-emitted.phG9zV`. Both results use checkpoint compiler
`6116ece70b451b4c8c0bef7326469f7d806caa34dbcdc48c0cd552d5bc64e990` and emitter above,
before the subsequent descriptor-lease and sparse-key source corrections. No current-source or
full-target acceptance is transferred to those in-progress changes. `git diff --check` passes.

The cloned Async TaskGroup probe was then compiled/executed on the checkpoint. macOS and Linux
AArch64 print both expected scope-ended errors; AArch64 artifacts are
`/tmp/elephc-linux-emitted.vjZQnf`. Linux x86_64 instead links successfully then exits 255 with
`Uncaught TypeError: Callback argument $tasks must be of type Object("elephc\\async\\taskgroup")`.
Its user/runtime assembly and result log are `/tmp/elephc-linux-emitted.lCJwYj`. This is a new
target-specific execution blocker, not a target pass. Backend owner must diagnose object callback
binding/ABI against these artifacts; the validator must not be weakened. The successful ordinary
Array-return probe on both Linux targets does not cover this generated Async object callback route.

Boxed sparse-write slice is now source-ready and held: both `mixed_array_set` and nested
`mixed_array_fetch_for_write` architecture branches check index strictly greater than payload length
before conversion/growth, reusing cell-owned Array-to-Hash promotion. Seven focused codegen/heap
regressions are in `tests/codegen/mixed_sparse_writes.rs`; library emitter-order regression is
`boxed_sparse_writes_promote_before_widening_on_both_architectures`. Targeted formatting and
`git diff --check` pass, but neither new test group has executed yet. Parent will compile once
backend callback corrections are also held. Raw erased result metadata, by-value Mixed cell
detachment, generic deletion, references and the complete consumer inventory remain open.

Backend sources are now held after the concrete Callable-to-Mixed cleanup correction and x86
object lookup string-ABI correction (`rax`/`rdx`, not C ABI `rdi`/`rsi`). Two added binding controls
cover returned callable lifetime/heap and accepted/rejected object parameters; PHP oracles were
checked and assertions retained. Fresh combined checkpoint is running under handle 25672:
`CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 RUST_MIN_STACK=67108864 cargo test --test codegen_tests -- descriptor_param_binding mixed_sparse_writes --test-threads=1`.
Expected groups are 16 binding tests and seven sparse-write tests. No result is inferred while the
process compiles; the Linux object/Async route still requires replay with rebuilt runtime emission.

Read-only by-value Mixed detachment analysis also identified an ownership coupling: cloning an
incoming cell changes actual return ownership even for `identity(mixed $x): mixed`. Source-level
return-alias summaries may currently classify this as borrowed. Any entry-shadow correction must
reconcile emitted return ownership, direct-call cleanup and descriptor return flags; blindly inserting
MixedClone would risk new leaks or premature release. Resource clones can retain the same cell while
adding an independent lease, so pointer equality alone does not establish owner transfer.

Combined checkpoint 25672 completed: 21/23 passing. All fourteen previous descriptor binding
regressions now pass, including both formerly leaking normal paths, and all seven new sparse-write
regressions pass. The two new controls are not runtime evidence yet: the object control is rejected
by the checker (`RequiredGroup` expected, Mixed given), while the returned callable control attempts
to invoke a variable typed Mixed. Their intended runtime routes must be corrected without dropping
their output/heap assertions. Production remains held while the library emitter/prelude/wrapper
checkpoint compiles under handle 23172.

The rebuilt compiler/runtime pair replayed the cloned-group probe on Linux x86_64 successfully:
both scope-ended errors now match macOS/AArch64. Artifacts, input copy, bridge hashes and result
log are `/tmp/elephc-linux-emitted.x49KvV`; compiler SHA-256
`3f852fc03b7f049ab690a3e0b70bffbfd07332230ac1b17317439e874d8a315f`, emitter
`0bffb9096752d766fd967d187051479cd0e7e096b8a698723086d29a15b71f35`. This verifies the
specific x86 object-string ABI correction on the generated Async route, not all object bindings.

The same rebuilt pair passes cloned TaskGroup revocation on Linux AArch64 as well, artifacts
`/tmp/elephc-linux-emitted.1G4Rpw`. A new boxed sparse-write probe matches PHP's exact
`{"2":3}|{"3":{"x":7}}|[1,2]` on both Linux x86_64 and AArch64, proving missing numeric gap
keys, nested sparse insertion and retained dense append for these routes. Artifacts are
`/tmp/elephc-linux-emitted.JmkaY3` and `/tmp/elephc-linux-emitted.M6IWMt` respectively.

The Parallel fail-fast probe also links/runs on Linux x86_64 and prints
`caught|cancelled|rejected`: an observed failed join requests scope cancellation and prevents late
spawn. Artifacts are `/tmp/elephc-linux-emitted.fE5B7M`. AArch64 replay follows. All these are
focused executable evidence with recorded program/runtime/library hashes, not Linux heap-debug,
full ABI/consumer coverage or independent review acceptance.

Linux AArch64 Parallel fail-fast replay also prints `caught|cancelled|rejected`, artifacts
`/tmp/elephc-linux-emitted.mrIpeP`. The library checkpoint 23172 completed 21/21 passing:
Async/Parallel source-to-built-AST parity and locked surfaces, injection/strict-PHP boundaries,
two-architecture sparse-promotion ordering, and callable wrapper variadic layout preservation.
The same library binary then passes scoped detector units 37/37, including import-scope resets,
absolute versus alias references and literal callable-string semantics. Additional callback
controls and the full PHP-array ABI remain open.

The two new controls were corrected to reach supported runtime dispatch without dropping assertions:
the returned callable-in-Mixed is invoked through `call_user_func_array($callback, [])`, and the
object control uses an opaque runtime-resolved function name instead of a statically rejected known
callback relay. Both scratch sources pass the current compiler's `--check`; PHP oracles remain
`42` and `42|TypeError`. Only the two new Rust fixtures changed, not production. Fresh binding
checkpoint is running under handle 71735 with the same serialized Cargo command and 16 tests.
No new pass is inferred until it finishes. All production/test Rust sources remain held during it.

The next Mixed value-copy slice is being prepared read-only: isolated source/heap probes must cover
ordinary local copy, by-value parameter mutation, identity-return copy subsequently mutated, closure/
method/forwarded values, reference controls and object/resource identity. A shared cell-copy policy
must distinguish semantic value bindings from internal spills and reference-cell ownership, and
reconcile actual returned shadow ownership. Entry-only cloning is not the complete correction.

Checkpoint 71735 completed 16/16 passing. Both added controls now reach runtime and pass their
unchanged PHP-visible and heap assertions. Together with the prior unchanged production checkpoint,
all sixteen binding regressions and seven sparse regressions are green; the library selected 21
and detector 37 also passed. This closes the identified callback descriptor leak and object lookup
string-ABI defects for those tests, not the full shape-erased array/copy/reference contract.

The next required shared Mixed value-copy implementation is authorized and in progress: backend
owner handles lowering, parameter shadows and actual-return metadata; scheduler owner handles the
dedicated copy/reference/object/resource/heap regressions from isolated PHP oracles. Production
source changes after this checkpoint invalidate transfer of its green results. All heavy builds
remain parent-owned and wait for both source owners to hold their changes.

Both corrected descriptor controls also execute with their PHP oracles on Linux x86_64 and
AArch64. Returned callable `42` artifacts are `/tmp/elephc-linux-emitted.hHFHG6` (x86_64) and
`/tmp/elephc-linux-emitted.WSsz2m` (AArch64); accepted/rejected object `42|TypeError` artifacts
are `/tmp/elephc-linux-emitted.MnTFW4` and `/tmp/elephc-linux-emitted.8biQm4`. These are
functional target controls; the macOS regression owns the associated heap assertions.

The checkpoint compiler's iOS device/Simulator gates were also rerun for Async clone and Parallel
fail-fast probes. `--check --emit staticlib --target <target>` succeeds for both programs on both
targets. Actual `--emit staticlib` refuses Async with the exact host-driven Fiber scheduler contract
diagnostic, and Parallel with the exact host-driven worker-thread contract diagnostic, each naming
the requested target and suggesting `--check` or macOS/Linux. Thus four analysis runs and four
intended refusal runs completed. An initial standalone `--emit-asm` attempt only reached the generic
iOS CLI executable-mode guard and is not counted as feature-specific refusal evidence.
No iOS runtime execution is claimed, and these results precede the next Mixed value-copy source changes.
