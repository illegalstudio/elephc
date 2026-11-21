# Unified task API and periodic heartbeat implementation

## Checklist

- [x] Confirm owned worktree, draft PR #958, current head and inherited WIP boundaries.
- [ ] Close the existing native heap/ownership regressions without weakening assertions.
- [ ] Introduce `Elephc\Task\Execution::{Async, Parallel}` with native/eval/strict/target tests.
- [ ] Move public task creation and scope ownership to `Elephc\Task\run` / `TaskGroup`.
- [ ] Implement explicit mode submission and common nonblocking task-result `await()`.
- [ ] Integrate worker completion wakeups without blocking the cooperative scheduler.
- [ ] Implement scope-owned `TaskGroup::every()` registrations with independent intervals.
- [ ] Implement per-registration `stop()` and `setInterval()` plus safe Parallel control proxies.
- [ ] Verify cancellation, failure, queue bounds, stale handles, teardown and AST/oracle parity.
- [ ] Test relevant optimizer modes and every supported target; complete exact-artifact review gates.
- [ ] Update public/internal/generated documentation after the API verification gate.
- [ ] Commit verified checkpoints and push to the existing draft PR with exact remote provenance.

## Authority and current evidence

The maintainer authorized implementation on 2026-10-11: unify the public namespace
as `Elephc\Task`, retain an explicit Async/Parallel mode, clean up the current work,
implement heartbeat and document the fully tested API. Push regular verified
checkpoints to https://github.com/illegalstudio/elephc/pull/958; do not create a new PR.
This reopens the old separate-public-namespace/result-handle choices deliberately,
not worker memory isolation or structured cleanup. Existing independent-review
acceptance remains a completion gate; a draft checkpoint is not consensus.

Owned branch: `spike/runtime-ctx-register` in `hazy-watching-moler`.
Starting local head: `fb95c1bfba6b965dcfaa17ec43b4977b12ef3a49`.
Live PR head checked at task entry: `c933b7c714ea5df714de770b0d1f28cfa91a90be`, draft/open.
The prior main rebase is local; publication requires a verified explicit lease.
Seven vendor deletions, local memory/config/scratchpad state and inherited source
WIP must not be swept into a checkpoint with `git add -A`.

## Required end state

One structured public group owns both execution domains. `spawn()` submits one
execution; `every()` registers recurring actions, each with its own positive finite
seconds interval. Registration is nonblocking and returns a control distinct from
an individual task result. Control is explicit in callbacks and externally usable
inside its live owning scope. Callbacks and task inputs use explicit arguments,
not required userland captures. No public heartbeat accessor or extra event loop.

Async keeps same-context cooperative execution. Parallel keeps isolated worker
arenas and checked copy transfer; worker controls require typed command proxies.
Common await must suspend/wake the coordinator, never conceal blocking join.
Periodic registrations cannot detach or restart an old completed execution.

Maintainer decision (2026-10-12): completed task results remain accessible after
`run()` exits, for both execution modes. A terminal handle owns the parent-side
value/failure/cancellation outcome, not a live group, worker, worker arena or native
job. Repeated awaits preserve ordinary PHP by-value semantics; native worker
results are materialized/copied before release. Scope closure still invalidates
group mutation and periodic control authority. This deliberately extends the old
Async closed-handle rule while preserving Parallel's terminal-result capability.

Working timing defaults from discussion, to pin with tests: first execution after
the interval; monotonic fixed cadence; at most one queued/running invocation per
registration; skip missed deadlines; no overlapping invocation of one action and
no unbounded catch-up. Stop prevents further invocations and drains active work
cooperatively. Define rearming under setInterval, stop/change ordering, scope-close
behavior and worker command acknowledgement in the implementation contract.

## First code checkpoint — execution enum foundation

The first slice adds compiler-owned AST declarations for the execution enum and
the neutral class contract, with matching frontend injection routes and focused
native/eval/diagnostic tests. It does not yet claim unified run/spawn/await or
heartbeat implementation. Production must never parse the PHP oracle to build it.
Generated user-facing docs remain behind the requested full-API testing gate.

## Native completion-wake foundation checkpoint

`crates/elephc-parallel/src/completion_wake.rs` now provides per-coordinator,
native-owned nonblocking socket pairs. Terminal transitions broadcast one coalesced
pending byte to each live subscriber. Draining one subscriber cannot steal another
scope's notification. Handles are monotonic, constrained to PHP's positive signed
integer range, and retired IDs fail closed even if the OS recycles a descriptor.
Exported integer status/FD results use signed 64-bit ABI slots. No PHP pointer or
arena lease crosses threads; no socket/registry is allocated before a subscription.

Create the subscription before inspecting job state; recheck job state after every
wake. Registry operations never acquire a job mutex. The new signal augments the
existing generation/condition-variable path; it does not replace legacy join yet.
The coordinator must release both endpoints only after its waits are settled.

Current final-source evidence (2026-10-11):

- macOS ARM64: 17/17 focused completion-wake/generation/cancellation/wait/worker tests.
  The 2026-10-12 precommit filter expands that evidence to 19/19 with controlled
  bridge panic and rejection of raw non-EPV job envelopes; the bridge build is clean.
- Linux ARM64: 17/17 in image 3f506a232838c660a0258049609b99a32be19beeaff50f93bd691da0c599a96e.
- Linux x86_64: 17/17 in image db9f3dce9005b21801ae553692d7c5c69fac998efd890766faedeccfcbd47312.
- iOS device and Simulator ARM64: focused bridge cargo check succeeds on both targets.
  The local iPhoneOS SDK is absent; no iOS link or device execution is claimed.
- Native Execution enum controls: 3/3, including alias/type transport and AOT/eval
  singleton identity. The initial CLI module-registration failure was corrected;
  the successful rerun is distinct evidence, not a green reinterpretation of that failure.
- Extended enum checks revealed missing EnumDecl support in the AST printer; it
  is corrected without excluding the declaration from the contract gates. The
  2026-10-12 resumed run passes 8/8 tests (including five-target EIR lowering,
  unit/backed enum print round trips and prelude contracts), then 1/1 diagnostic test.

Logs are `/tmp/elephc-task-completion-wake-{host,linux-arm,linux-x86}-integrated-final-20261011.log`,
`/tmp/elephc-task-completion-wake-precommit-20261012.log`, the two
`/tmp/elephc-task-completion-wake-ios{,-sim}-final-20261012.log` checks, and
`/tmp/elephc-task-execution-{target-parity,errors}-resume-20261012.log`.
Only the three native bridge source files form the first publishable code slice.
The enum integration and inherited heap/ABI WIP remain separate until their own
verification gates. This checkpoint does not implement public unified run/await,
worker command proxies or every, and does not close the existing heap regressions.

Kioku risk review completed after a long whole-store verification despite the path
filter. It flags historical broad ctx/serialization/panic packets as drifted.
The current bridge diff does not change worker storage or release/transfer ABI;
the focused worker/quarantine/panic/envelope controls above pass. Wider ctx/C-host
claims remain explicitly un-reverified rather than silently refreshing their packets.

## Publication checkpoint (2026-10-12)

Commit `56721291ed93d4698c472845f10625a264aa0f7b` publishes only the native
completion-wake foundation, this execution plan and scoped Kioku evidence/decision.
The explicit lease against remote c933b7c714ea5df714de770b0d1f28cfa91a90be succeeded.
Live PR #958 now reports that exact head, remains draft and is mergeable; CI started.
No CI acceptance is inferred from startup or the successful classifier job.
The namespace enum/printer/frontend work and inherited source/heap WIP remain local.

Kioku reverify selected stale graph spans for the completion function. The newer
measurement packet ending `307ce56d` pins full current native source blobs instead;
it supersedes the earlier `a7aa1344` packet. This avoids treating an inaccurate
span as proof; it does not repair the Kioku tool or claim a graph correctness audit.

Next implementation requirements remain: unified public run/group/submission,
terminal result retention after scope exit, nonblocking worker await, independent
periodic registrations and native worker control proxies. Existing native heap
regressions must still be fixed and replayed; full API docs and exact-hash reviewer
consensus remain behind the complete verification gate.

## Cooperative worker-await adapter WIP (2026-10-12)

Source inspection found that job outcome publication precedes context release and
OS-thread joining. Merely waiting for terminal phase before calling legacy join
would still block the cooperative coordinator during cleanup.

The new executor reaper consumes exit notices only from finishing workers. Worker
registry entries own its lifetime; final record removal drops that lease outside
the worker registry mutex, closes the queue and joins its consumer. The consumer
retains no PHP arena/pointer and no strong reaper lease. Thread-creation failure
marks the pending join record failed and wakes waiters before removal, so an exit
notice from a dropped spawn closure cannot leave a reaper stuck on Pending.

`elephc_parallel_job_try_wait` returns zero until both outcome and native join are
ready, a signed terminal phase on success, or minus one on invalid/failed joining.
Joined publication notifies completion subscribers again. A gate regression proves
that a published success remains pending while context cleanup is blocked.

The transitional engine helper `Future::__awaitWithScheduler` subscribes before
rechecking readiness, suspends through existing awaitReadable and releases the
subscription in finally. Rust AST builders and PHP parse oracle agree (8/8 tests).
This helper is plumbing for the upcoming unified Task::await, not the final public
API or an implicit Future/Awaitable conversion. Native PHP regressions use explicit
scope arguments and verify sibling progress and observed worker failure.

Current evidence: 14/14 focused bridge executor/completion tests pass on macOS
ARM64, Linux ARM64 and Linux x86_64. Logs:
`/tmp/elephc-task-reaper-regression-20261012.log` and
`/tmp/elephc-task-reaper-linux-{arm,x86}-20261012.log`.
AST log: `/tmp/elephc-task-cooperative-adapter-ast-20261012.log`.
Native PHP tests are still building under session 11734, log
`/tmp/elephc-task-cooperative-adapter-native-20261012.log`.
iOS checks in session 63513 are queued on the same target build lock; do not restart
either from an observation timeout. No commit/push of this adapter WIP yet.

Open before this slice can be published: terminal/cancellation/transport cleanup
native replays, reaper teardown and resource bounds, iOS checks, source warning
analysis and exact artifact review. Canonical namespace migration, TaskGroup mode
dispatch, terminal snapshots, every registrations and control proxies remain open.

Maintainer correction: new client fixtures must target `Elephc\Task` directly,
not present the legacy Async/Parallel namespace split as the intended API. The
temporary WorkerExchange static-Future fixtures first passed 2/2 native controls
in `/tmp/elephc-task-cooperative-adapter-native-replay-20261012.log`; that proves
the engine adapter only. They are now replaced by canonical TaskGroup/Execution
spawn and Task::await expectations, with no static exchange class or `use` capture.
The AST facade now exists locally (`src/task_prelude/{root,group,handle}.rs`),
with checked Parallel submission delegated through dedicated EIR lowering.
`cargo check -p elephc --features curl --lib` passed without warnings.
The first public-facade native replay is **0/5**, not an accepted implementation:
`/tmp/elephc-task-public-facade-replay-20261012.log` records four SIGSEGV failures
and one null cancellation receiver. LLDB shows the root's named `instanceof`
receiving `NULL_SENTINEL` as an object pointer. A shared target-aware normalization
fix and a catch-assigned-null regression are under focused replay in
`/tmp/elephc-task-instanceof-null-20261012.log`.
That focused regression passed **1/1**. The following public replay improved to
**1/3**: the Async terminal result survives scope exit and repeated await;
Parallel still reports a null `__phase()` receiver, and an escaped group is null
instead of reporting closed authority. Log:
`/tmp/elephc-task-public-after-null-20261012.log`.
Source audit then found duplicate constructor-argument releases in unified
Parallel lowering: `emit_fixed_object_new` already retires Future/coordinator.
Those extra releases are removed, with a five-target EIR count regression added.
The root now uses ordinary `$body($group)` AST invocation instead of an extra
`call_user_func` entry. This is not yet proof of the escaped-group fix.
Current replay: `/tmp/elephc-task-public-ownership-replay-20261012.log`.
That replay passed the two terminal-result tests (**2/3**); only escaped-group
submission remains red. The dedicated constructor-owner EIR assertion now passes
on all five supported targets (**1/1**), after correcting test placement/import
and searching `module.closures` rather than just named functions. Final log:
`/tmp/elephc-task-constructor-owners-targets-corrected-20261012.log`.

Cooperative public replay is **1/2**: Async sibling progress passes; observed
worker failure reports `Can only throw objects`. LLDB identifies Task::await as
the throwing site. Its EIR performs ReleaseLocalSlot on the original Mixed box
before Throw; `take_direct_local_load` must release the source box only when the
load has extracted a separately owned concrete payload, not for a Mixed load.
The narrowed transfer fix and heap-clean nullable-Throwable regression are under
replay: `/tmp/elephc-task-throw-box-regression-20261012.log`.

The escaped group is NOT actually null. Retained CLI reduction
`/tmp/elephc-task-escaped-group.cZe6A2/` proves `instanceof TaskGroup` succeeds
with EIR optimization off. Mixed method dispatch excludes TaskGroup::spawn because
it compares only physical arity (receiver + execution + callable + variadic array)
with unnormalized caller operands (receiver + execution + callable). It selects
no matching runtime class and emits the misleading null-member diagnostic.
Generic source-argument normalization for dynamic variadic/defaulted methods is
an open compiler gate; do not weaken the escaped-group assertion to that message.
The existing EIR `lower_dynamic_method_call_with_receiver` already routes callback
arrays through the descriptor invocation ABI. Investigate reusing it for Mixed
receivers requiring default/variadic source argument planning, instead of adding
per-method physical-arity exceptions in backend dispatch. This is a next action,
not an implemented fix; preserve argument order, lexical access and ownership.

The nullable throw heap regression passes **1/1**. The following public cooperative
harness replay passes **2/2** in
`/tmp/elephc-task-public-cooperative-throw-replay-20261012.log` (not just a CLI probe).
Thus four of the five current public facade tests have positive evidence; the
escaped-group test remains red. The facade/reaper/constructor-wrapper changes are
still local WIP, not part of the already published head.

Kioku feedback from this slice: recall returned the relevant namespace and
result-lifetime decisions, and learn/supersede preserved evolving evidence. The
`callers take_direct_local_load --context 2` query took more than nine minutes
(observed live process) and returned no references, although direct source has
the call from `src/ir_lower/stmt/exceptions.rs`. Index freshness and inclusion rules
have not been diagnosed; this negative is not authoritative. Graft skeleton also
resolved only ambiguous scratchpad review copies. Exact source/runtime inspection
was the necessary fallback; no Kage query was used.

## Scoped compiler repair checkpoint (2026-10-12)

The next publishable code slice contains only object-null normalization for named
and dynamic instanceof, plus correct local-box transfer for throw. The dedicated
native nullable-throw regression passes **1/1** with a clean heap, the final
catch-assigned-null instanceof replay passes **1/1**, and six EIR/assembly checks
cover all five supported targets. PHP CLI matches the two regression outputs
(`retained` and `FFTT`). Diff and assembly-comment hygiene pass. Logs:
`/tmp/elephc-task-{throw-box-targets,throw-box-regression,instanceof-null-final}-20261012.log`.

Public facade evidence remains a working-tree measurement: terminal Async and
Parallel handles pass, sibling progress passes, and worker failure observation
passes. Escaped-group dispatch is still red. Native Linux executable verification
and exact-artifact reviewer consensus are not established by this checkpoint.
Do not stage the local facade/reaper/contract changes, inherited TLS/eval/heap WIP
or vendor deletions. Draft PR head before this checkpoint is 56721291ed93d4698c472845f10625a264aa0f7b;
live base now reports 5fc8dd0bd0e77ffa304cacb4f1ad9253df92a840 and must be reconciled
again before final acceptance. Current CI test workflow at the old head failed;
classifier success and build success do not mean full regression acceptance.

Published checkpoint: `b578c494db586d4baf5090c30b8afbde4b012b54` was pushed by
normal fast-forward to fork/spike/runtime-ctx-register. Live PR #958 confirms
that exact head, OPEN, draft and MERGEABLE. CI test run 38224754985 is queued;
run 38224754816 is in progress; classifier 38224749669 succeeded. No CI acceptance
is claimed. Only six compiler/test files, the plan and the scoped Kioku packet
were committed; local facade/reaper/contract work and inherited changes remain.

Next local implementation now routes opaque Mixed method receivers through the
existing descriptor invocation boundary when a possible target has defaults or
a source variadic. Concrete/known-nullable receiver lowering is unchanged.
`tests/codegen/objects/mixed_method_arguments.rs` adds normal/defaulted, variadic,
spread/named and once-only evaluation plus clean-heap controls. Focused replay is
live under session 19288, log `/tmp/elephc-task-mixed-method-arguments-20261012.log`.
After it finishes, replay the original closed TaskGroup assertion unchanged,
then the five public facade controls and relevant target/optimizer variants.
This dispatch change is uncommitted and unverified, not another completed gate.

## Mixed dispatch replay and synthetic-span recursion (2026-10-12)

The two initial Mixed-method controls pass **2/2**, including clean heap and
default/variadic/spread/named argument binding. PHP oracle agrees. However, the
public three-test replay terminated the Rust harness with SIGBUS; an isolated
closed-group replay reproduced it. This is a compiler crash, not a worker crash
or an observation timeout. macOS diagnostic report
`codegen_tests-e95d5933ce7fa5b1-2026-10-12-105352.ips` shows repeated frames through
lower_dynamic_method_call_with_receiver, function_calls and
lower_desugared_dynamic_method_call. Synthetic callback and call spans are both
dummy, so the parser-desugaring recognizer loops back into its own reconstructed
call_user_func callback array.

The helper now invokes lower_dynamic_call_user_func directly, not lower_expr of
another reconstructed function-call AST. Mixed methods needing runtime binding
also pass through the shared null guard before argument evaluation; a new
clean-heap regression asserts no argument side effect on a null receiver.
The original closed-group assertion remains unchanged. Current isolated replay:
session 98943, `/tmp/elephc-task-public-closed-group-desugar-fix-20261012.log`.
Then run mixed_method_arguments (now three tests) and all five public controls.

Kioku recall --semantic reported semantic search unavailable and explicitly
fell back to lexical results. The semantic backend was not diagnosed or repaired
by this scheduler slice. The fallback still recovered the current relevant
run-result and direct-every decisions; no semantic success is claimed.

The 30-second per-command semantic retry subsequently completed without the
fallback warning; Ollama's live `ps` showed embeddinggemma loaded on GPU. The
default failure cause is still undiagnosed (model warming/timeout are not proven).
No global configuration or Kioku source was changed.

Closed-group desugaring fix passes **1/1** with the original exact expected scope
error. The subsequent combined replay is **4/8**: closed group plus three
Mixed-method controls pass, but the four live/result Async/Parallel controls now
fail callable-array resolution. Inspection shows the heuristic also routed
already-typed/common-signature methods such as Fiber::resume into a descriptor
inventory which requires emitted public EIR method bodies. Native Fiber methods
are intrinsic, not ordinary emitted bodies.

The predicate now excludes calls with an existing method_signature. The argument
regressions each contain two source classes with incompatible collect signatures,
so they exercise ambiguous receiver binding rather than an already-supported
unique/common signature. Current combined eight-test replay is session 79344,
`/tmp/elephc-task-public-dispatch-ambiguous-only-20261012.log`. This refinement is
unverified and uncommitted. Do not turn the earlier 4/8 into an acceptance claim.

## Dispatch gates passed; heartbeat TDD begins (2026-10-12)

The ambiguous-only replacement passes **8/8** in
`/tmp/elephc-task-public-dispatch-ambiguous-only-20261012.log`: all five initial
public Task controls plus the three truly ambiguous method-binding controls.
The same eight tests pass with EIR optimization disabled and stack register
allocation in `/tmp/elephc-task-public-dispatch-stack-noopt-20261012.log`.
Three target tests pass in `/tmp/elephc-task-public-target-dispatch-20261012.log`,
including five-target emission of closed-group dispatch with dummy prelude spans.
This is source/host evidence, not native Linux/iOS or full API acceptance.

Two initial every Async tests now pin nonblocking registration, explicit callback
control/arguments, first delay and independent intervals. They correctly fail
with Undefined method TaskGroup::every (**0/2**), log
`/tmp/elephc-task-every-async-initial-red-20261012.log`. Async is an implementation
slice, not a reduced end state: Parallel command proxies remain required.

A retained CLI reduction `/tmp/elephc-task-escaped-group.cZe6A2/descendant.php`
exposes another result-lifetime defect: a child submitted by a child while the
facade drains is not cached before closure, and later reports the old Async
Awaitable scope-ended error. TaskGroup::__drain used a foreach snapshot. It now
walks the live indexed task list until all appended descendants are materialized.
The new regression expects done|42 unchanged; replay is session 3366,
`/tmp/elephc-task-public-descendant-drain-20261012.log`. This correction is pending.

Next: finish descendant replay and preserve existing controls, then implement
scope-owned periodic registrations with wakeable control changes and bounded
tracking across repetitions. Native worker proxies must exchange scalar commands,
not PHP arena pointers. Continue final namespace retirement, failure/cancellation,
ownership/queue/target gates, full API docs, exact-artifact review and draft pushes.

The live-descendant correction now passes **1/1** in its focused native replay.
The following complete current baseline passes **9/9** in
`/tmp/elephc-task-public-live-drain-final-20261012.log` (four terminal/scope tests,
two cooperative worker controls, three ambiguous-method binding controls).
The earlier stack/no-opt and five-target emission proofs predate the live-index
drain change; refresh them when publishing the facade. Public every is still
expected-red, not implemented. Native current draft-head CI run 38224754985
completed with failure; do not claim the PR's full CI is green.

Implementation direction for the next heartbeat slice: test a monotonic per-
registration cadence/control state in the native bridge, with wakeable stop and
interval changes. Each registration owns its wait transport; worker controls send
scalar commands only. Couple that state to a scope-owned Async pump, with ordinary
Async invocation and checked Parallel submission, never a blocking callback loop
in the registering task. Keep recurrence tracking bounded, materialize active
outcomes before closure, and reject retired control identities. Do not substitute
an Async-only API or unbounded historical Future/Task accumulation for the full
requested shared hub. The first two Async tests are only the initial behavior pins.

## Native periodic state and first AST pump WIP (2026-10-12)

`crates/elephc-parallel/src/periodic/` now owns a deterministic monotonic cadence
and native registration registry. First admission is delayed; one active claim
prevents overlap; completion skips missed slots on a fixed cadence. An idle
interval change rearms from the command, an active change from completion;
reapplying the same value preserves the deadline. Stop is irreversible.
Each registration owns a separate nonblocking Wake shared with the completion
transport implementation. No PHP pointer, background timer or new thread is
created. Signed-64 identities never reuse a retired registration. Release stops
admission but returns pending while an invocation remains active.

Ten focused native tests pass in
`/tmp/elephc-task-periodic-native-initial-20261012.log`, including synthetic clock
deadlines, interval validation, independent cross-thread wakes and retirement.
This does not prove PHP periodic integration, native target parity or full teardown.

`src/task_prelude/periodic.rs` declares the C ABI and builds PeriodicControl plus
the initial Async pump in Rust AST. The pump uses existing awaitReadable timeout
and a native command FD; the registering task does not execute the callback.
TaskGroup stores controls until scope drain so stop remains available throughout
the owning live scope; cancel stops registrations, and drain retires controls.
The checker records Async every callback reachability. Initial library check
passed, log `/tmp/elephc-task-periodic-ast-check-20261012.log`.
The first PHP integration replay is session3526,
`/tmp/elephc-task-every-async-first-implementation-20261012.log`; shared wake tests
are session32978, queued on the Cargo artifact lock. Do not restart live builds.

Open implementation gates: Parallel every lowering and worker-local proxy creation,
bounded historical tracking, cancellation/failure cleanup, control opacity and
constructor/private helper authority, oracle parity, all targets and fresh facade
replays. The interim source fallback rejects Parallel instead of running it
synchronously; it is not the accepted complete API. Registry/docs generation stays
behind the explicitly requested fully-tested API gate.

Shared wake regression passes **7/7** in
`/tmp/elephc-task-periodic-shared-wake-20261012.log` after exposing only the native
Wake operations needed by periodic state. First PHP pump replay was **0/2** at
type checking: the raw Parallel extern guard knew only legacy compiler-owned
classes, not PeriodicControl. It now permits that exact generated class while
preserving the unconditional rejection of source-located calls. New direct and
string-callback user-FFI error tests protect the boundary.

Current pump replay is session87006,
`/tmp/elephc-task-every-async-trusted-body-replay-20261012.log`. Boundary errors are
session23307, `/tmp/elephc-task-periodic-extern-boundary-20261012.log`, queued on
the same Cargo artifact lock. No native PHP periodic success is inferred yet.

## Periodic foundation verification and publication boundary (2026-10-12)

Current cadence+wake tests pass **18/18** on macOS ARM64, Linux ARM64 and Linux
x86_64. These include the added deterministic two-interval admission test.
Logs: `/tmp/elephc-task-periodic-host-final-20261012.log`,
`/tmp/elephc-task-periodic-linux-{arm64,x86_64}-20261012.log`.
Docker images remain pinned to ARM64 3f506a232838c660a0258049609b99a32be19beeaff50f93bd691da0c599a96e
and x86_64 db9f3dce9005b21801ae553692d7c5c69fac998efd890766faedeccfcbd47312.
iOS device and Simulator bridge cargo checks pass with Rust1.95, using an isolated
`target/periodic-native-checks` directory. Logs are
`/tmp/elephc-task-periodic-ios{,-sim}-check-20261012.log`. No iOS executable proof.

The first two public Async every controls now pass **2/2** after the trusted-body
fix. Direct/string user-FFI denial checks pass **2/2**. The subsequent six-control
PHP replay passes **6/6**, including setInterval/repetition, waking external stop,
scope-expired control, invalid intervals and clean heaps:
`/tmp/elephc-task-every-async-lifecycle-20261012.log`. The two registrations test
does not assume realtime ordering when both deadlines become overdue; native fake-
clock admission tests prove independent deadlines, and PHP checks independent stop.

The next checkpoint contains only the new native periodic module, shared native
Wake visibility/accessor, library module registration, plan and scoped Kioku proof.
The AST/class contract/checker/error/Every PHP work remains local and deliberately
outside this native checkpoint. No complete shared Async/Parallel heartbeat, full
CI acceptance, opacity/fatal cleanup or final documentation/review is claimed.
Pre-push exact draft head is b578c494db586d4baf5090c30b8afbde4b012b54, base is
5fc8dd0bd0e77ffa304cacb4f1ad9253df92a840. Preserve all inherited source changes
and seven vendor deletions; do not stage with a blanket add.

Published native checkpoint is `bce43fd9e8509b1b8b095f4a6e70da1582ca0693`, pushed
by normal fast-forward to the existing fork branch. Live PR #958 confirms OPEN,
draft, MERGEABLE and that exact head. CI runs38239707037/38239707071 are in progress;
classifier38239704414 succeeded. Full CI acceptance is not inferred.

The subsequent current working-tree combined replay passes **15/15** in
`/tmp/elephc-task-public-periodic-baseline-20261012.log`: nine original facade/
cooperative/ambiguous-dispatch controls plus six Async every lifecycle controls.
The native-only commit does not contain this AST facade; those results remain
separate local integration evidence. Current index is empty, inherited WIP and
vendor deletions remain untouched. No docs or final API acceptance are claimed.

Next implementation is Parallel every: inspect actual static worker-entry lowering,
wire transfer and Future/group retirement before building its reusable submission
path. The callback must receive a worker-local control backed only by a scalar
native identity, and repeated submissions must not accumulate unbounded historical
Task/Future entries. Preserve nonblocking joined-cleanup waits and structured
cancellation/failure drain. Then harden controller construction/private helpers,
refresh all target/optimizer/teardown gates, retire public legacy namespaces and
generate/review the complete API documentation on the accepted exact artifact.

## Parallel recurrence preparation WIP (2026-10-12)

`src/ir_lower/expr/parallel.rs` now separates prepare_parallel_submission from
one-off submission. Preparation freezes the checked input/captures and emits the
worker entry without starting a job. The existing one-off path retains its scope
pin through registration and retires the original task descriptor after submission,
preserving its previous cleanup ordering.

`src/ir_lower/function/parallel_factory.rs` builds a zero-visible-argument EIR
closure with one borrowed hidden serialized-string capture and a typed ParallelSpawn
against the already checked entry. Its five-target emission test passes **1/1** in
`/tmp/elephc-task-parallel-template-targets-20261012.log`. This callable is not yet
connected to public every; compiler warnings report that unwired state. Do not
suppress those warnings or claim a completed Parallel pump from emitter evidence.

Native periodic release now stops admission and marks a running registration for
deferred retirement. Completion closes that pending identity/endpoints atomically.
Nineteen native cadence+wake controls pass on the host in
`/tmp/elephc-task-periodic-deferred-retirement-20261012.log`; cross-target reruns of
this latest change are still required.

The legacy Future/group AST and PHP oracle add a periodic scalar-id attachment,
joined-readiness retirement check, scoped factory submission and array-splice
retirement of completed periodic Futures. Future scope finish completes associated
cadence before releasing its native job. New private access exceptions are limited
to synthetic calls from the designated generated group/pump methods; source PHP
does not gain those entry points. These helpers are not yet wired into public every
and require oracle/private-boundary/cleanup verification.

The current library check succeeds with **three unused-factory warnings**, not a
warning-free acceptance gate. The one-off/facade/Async-every regression replay is
session65266, `/tmp/elephc-task-parallel-template-oneoff-regression-20261012.log`.
The first Parallel-every isolation/cooperative-parent behavior pin is added and
still expected-red until integration. It checks parent GC state remains isolated,
without assuming realtime ordering of parent/worker output.

Required next integration: inject only a scalar control identity into the copied
input and reconstruct a worker-local PeriodicControl. Current transfer rules reject
objects except direct Cancellation, so do not serialize a parent control object or
relax general object transfer. Classify only approved scalar stop/setInterval
commands as worker-safe; raw user extern access must remain denied. Connect the
prepared factory to a scope-owned Parallel pump, retire history only after joined
cleanup, and use deferred cadence retirement when cancellation leaves active work.
Then refresh the fifteen baseline controls, Parallel behavior, private/ownership/
capacity and all-target gates before the next facade publication or API docs.

## Parallel pump integration under replay (2026-10-12)

The first post-extraction baseline was **3/15**, failing at type checking rather
than native execution: elephc_task_periodic_complete was declared by both owning
preludes, and __spawnPeriodic lacked its narrowly scoped private Future constructor
exception. The duplicate Task extern was removed; the Parallel prelude owns that
declaration. Constructor access is granted to the exact generated spawner method.

Parallel every now has dedicated call-site EIR lowering. It evaluates registration
inputs, prepares frozen worker bytes/entry, captures them in the submission factory,
and registers a structured cooperative pump. The pump claims cadence, submits a
fresh scoped Future, awaits joined cleanup through the existing completion adapter,
and retires completed Future history. Deferred release handles active cancellation.
Current compiler library check is warning-free:
`/tmp/elephc-task-parallel-every-integration-check-20261012.log`.

PeriodicControl is treated as a direct scalar capability input, not a generally
transferable PHP object. Parent lowering extracts __nativeId before serialization;
worker reconstruction creates a local proxy with __fromNativeId. Nested/returned
controls and all ordinary objects remain rejected. Only native stop/setInterval
commands join the worker-safe extern classification; raw user FFI guards remain.
Construction/helper opacity, untyped callback contextualization, dynamic/named mode
and complete cancellation/teardown/privacy verification remain open.

First native integration replay is session17001,
`/tmp/elephc-task-every-parallel-first-integration-20261012.log`. Capability-boundary
unit and Parallel AST/PHP oracle replay is session93090,
`/tmp/elephc-task-parallel-every-parity-transfer-20261012.log`, queued on Cargo lock.
No successful Parallel every execution or full API acceptance is claimed yet.

Latest deferred-retirement cadence+wake checks pass **19/19** on both Linux
ARM64 and Linux x86_64, preserving the same pinned images and native-only filters:
`/tmp/elephc-task-periodic-deferred-linux-{arm64,x86_64}-20261012.log`.
Host19/19 was already measured before integration. This validates native state/
transport, not the current PHP worker reconstruction or the unchecked lifecycle
paths. First Parallel native test remains live; its Cargo child is compiling the
compiler binary. Private/oracle/capability replay remains queued on that lock.
No new checkpoint was staged or pushed; draft head remains bce43fd9e8.

## First Parallel execution replay findings (2026-10-12)

First worker behavior test did not reach execution: preparation panicked while
removing the injected control's param_attributes entry from an empty optional
metadata table. Parameter metadata removal now checks table presence, with a
focused unit pin; caller-visible params still remove the one injected control.
Failure log: `/tmp/elephc-task-every-parallel-first-integration-20261012.log`.
Replacement native replay is session91422,
`/tmp/elephc-task-every-parallel-signature-replay-20261012.log`.

Capability boundary unit passes **1/1**. AST/oracle unit fails **0/1** because the
synthetic builder's unused-parameter walker skipped ClosureCall's callee variable,
inserting `_unused = $factory` although `$factory()` uses it. walk_expr now records
both callee and argument reads; a builder regression checks no extra consumption.
Do not add artificial unused statements to the oracle to conceal the missed read.
Prior combined log: `/tmp/elephc-task-parallel-every-parity-transfer-20261012.log`.
Fresh builder/signature/oracle replay is session23171,
`/tmp/elephc-task-parallel-every-builder-signature-regression-20261012.log`.
Native replay has compiled and its bridge helper waits on that Cargo target lock;
both are live. No completed native Parallel every behavior is established yet.

The signature replacement native test subsequently passes **1/1** in
`/tmp/elephc-task-every-parallel-signature-replay-20261012.log`. It establishes
one real isolated worker callback, successful scalar stop proxy, parent Async
progress and unchanged parent GC state. The long harness time includes waiting
for Cargo/bridge build locks, not a measured scheduler stall.

The fresh optional-metadata, closure-callee read and Parallel AST/oracle regressions
pass **3/3** in
`/tmp/elephc-task-parallel-every-builder-signature-regression-20261012.log`.
No artificial unused statement was added to the PHP oracle.

Two additional Parallel lifecycle tests now check three non-overlapping fresh
workers with setInterval/stop through an application file counter and clean heap,
and canonical failure propagation with clean heap. The counter file is created
and removed inside the isolated temporary native fixture directory; no repository
data is modified. Current three-test replay is session54520,
`/tmp/elephc-task-every-parallel-lifecycle-20261012.log`, compiling. These are
unverified behavior pins, not accepted repeated-execution or cleanup evidence.
Refresh the combined baseline, all supported target emitter/native evidence,
scope/constructor/helper opacity, transport/cancellation/resource bounds and the
legacy namespace migration before documentation and final exact-artifact review.

## Periodic failure propagation restored; ownership remains red (2026-10-12)

Parallel lifecycle replay is **2/3**: isolation/cooperative parent and three fresh
worker repetitions with interval change/stop and a clean heap pass. The failure
fixture returned without propagating its error and retained13 blocks/1184bytes.
A catch-all retained CLI reduction confirmed before|returned|after, not an empty
message inside a matched catch. LLDB followed the worker error into Future::join,
the pump and TaskGroup::__drain, which threw the canonical TaskFailure; root did
not rethrow it.

A separate two-catch failure selector reproduced lost| with a clean heap. Its
assembly folded Object-vs-null strict equality to false from projected PHP types,
despite the skipped first catch leaving the widened object local null. Backend
strict comparison now normalizes object null encodings and compares at runtime in
both operand orders/equality variants. Dedicated native regression passes **1/1**
with clean heap and PHP oracle second|first; assembly-comment and diff checks pass.
Log: `/tmp/elephc-task-periodic-failure-null-compare-regression-20261012.log`.

Current combined baseline is **18/19** in
`/tmp/elephc-task-periodic-null-compare-full-baseline-20261012.log`: the periodic
failure message now propagates correctly, but its clean-heap assertion still
reports13 retained blocks/1184bytes. Do not weaken that assertion or claim ownership
closure from successful propagation. The new backend comparison still needs its
five-target emitter/native follow-ups before publication.

Read-only LLDB inspection using the active ctx arena (not obsolete global heap_off)
identifies a retired Elephc Parallel Future (jobId0, refcount1), TaskFailure and
its retained data. Exact retained diagnostics live in
`/tmp/elephc-periodic-failure.BPU0rw/{heap-identities-context,future-refs-bounded}.log`.
The Future reference trace ends with one owner remaining, but its source is not
yet established. A separate nullable receiver method-throw reduction is heap clean
(`/tmp/elephc-periodic-failure.BPU0rw/receiver-throw.php`); do not infer a generic
method receiver leak from the periodic failure. No speculative extra decref was
introduced. Next: inspect the pump's exceptional retirement argument/local owners
against EIR and dynamic refcount increments, then rerun the unchanged failure pin.
Current index is empty; draft head remains bce43fd9e8. Opacity, modes/namespace,
capacity/cancellation, complete targets/docs/CI and exact review remain open.
Legacy namespace references in backing-engine implementation are transitional;
this does not establish completed public namespace migration or heartbeat support.
Historical first canonical replay: the public Task run/group facade was not yet
implemented, and the tests intentionally failed before the facade was added.
That obsolete first command was session 83131,
`/tmp/elephc-task-canonical-api-red-20261012.log`; current proofs are recorded above.
Both iOS bridge checks now completed successfully; SDK linkage is still unproved.

## Owned method receiver unwind and remaining envelope string (2026-10-12)

A narrower generic reduction with nullable receiver, throwing method and a
same-frame finally that retires the object from an owning array retained one
24-byte Item. Publishing an owned method receiver through the existing call
operand owner stack fixes that reduction. Both ordinary and nullable method
paths now root before argument evaluation and retire after result stabilization;
Fiber switch ownership transfer remains exempt.

Focused replay is **1/2** in
`/tmp/elephc-task-periodic-owned-receiver-regression-20261012.log`: the independent
finally-retirement regression passes with clean heap; periodic Parallel failure
still propagates correctly but retains one string block/272 bytes, down from
13 blocks/1184 bytes. The clean-heap assertion remains unchanged.

Read-only current-binary LLDB inspection identifies the remaining string as the
serialized seven-field worker failure envelope (RuntimeException, message, code,
source file, line and trace), not a remaining Future/TaskFailure object. Evidence:
`/tmp/elephc-periodic-failure.BPU0rw/heap-strings.log`. Inspect envelope decode and
throw ownership next; do not add an unproven decrement or claim cleanup closure.

The read-only envelope persist/free trace narrows the remaining leak to a detached
string produced by a concrete read from final Mixed storage in
Future::__takePreparedValue. The argument snapshot copies it but its eager owning
source check omits retirement before the later unset widens the slot. EIR with
optimization both on and off lacks the source release; this is not an optimizer
removal. A normal source encode/decode/unset reduction is heap clean, so it does
not establish general unserialize leakage.

Snapshot/capture argument paths now use value_needs_release_after_use after copying:
the existing builder finalization removes provisional releases for concrete borrowed
locals and preserves them for detached Mixed-to-string reads. A five-target EIR
regression requires exactly one release of the envelope decoder's projected read.
Current native receiver/failure/repetition replay is session86776, log
`/tmp/elephc-task-periodic-envelope-snapshot-regression-20261012.log`, compiling;
the new correction and target regression remain unverified until those runs finish.

Focused unit checks for task_execution, call_coercion_owners and
fiber_operand_owners are queued as session18399 on the same Cargo artifact lock
(`/tmp/elephc-task-periodic-envelope-all-target-checks-20261012.log`). No second
compiler is consuming this worktree's target concurrently. The receiver regression
also checks root publication before the call and emits assembly for all five
supported targets; these are structural/emitter checks, not native Linux/iOS
execution. Poll the same handles, do not restart builds from elapsed time.

Live draft PR head remains bce43fd9e8509b1b8b095f4a6e70da1582ca0693, base
5fc8dd0bd0e77ffa304cacb4f1ad9253df92a840. Published CI run38239707037 is red,
including non-codegen and Linux Curl/Eval shards. This is independent of the new
unpublished facade corrections; no broad green claim is justified. Two attempts
to retrieve one failing job's log ended with an HTTP stream CANCEL, so failure
causes remain unclassified rather than presumed unrelated.

## Envelope snapshot cleanup native replay green (2026-10-12)

The snapshot correction's focused native replay passes **3/3**: the generic
nullable receiver/finally retirement, periodic Parallel failure propagation and
three worker repetitions with interval change/stop all have clean heaps.
Log: `/tmp/elephc-task-periodic-envelope-snapshot-regression-20261012.log`.
Current CLI EIR also shows exactly the formerly missing projected string release
after its snapshot Acquire, before the decoder call. No assertion was weakened.

The larger focused native baseline (public Task results, every lifecycle,
Mixed receiver arguments, both exception reductions and Async/Fiber heap cleanup)
is now running; no full API acceptance is inferred from the three-test proof.
Five-target unit/emitter checks remain under session18399 until terminal output.
The published CI failure log fetch also fails with unexpected EOF with HTTP/2
disabled for that command; failure attribution remains open, no persistent client
configuration was changed. Namespace retirement, control opacity, runtime modes,
capacity/cancellation, documentation and exact-artifact review remain required.

The focused all-target unit/emitter replay completed **20/20** in
`/tmp/elephc-task-periodic-envelope-all-target-checks-20261012.log`: all Task
execution/constructor/closed-group controls, projected envelope cleanup, owned
receiver publication, call coercion owners and Fiber transfer exclusions pass.
This verifies structural/emitter behavior on five targets, not Linux/iOS native
execution or final API acceptance.

## Periodic authority copy/construction guards started (2026-10-12)

PeriodicControl constructor and native proxy factory are now private. Only
compiler-origin TaskGroup every/create-control paths receive the cross-class
constructor exception; source spans are explicitly excluded. Private clone and
throwing serialize/unserialize guards prevent ordinary copying/restoration of
scope authority. New diagnostic/native heap tests pin manual construction, forged
native IDs, clone, serialize, unserialize and direct factory access.

These guards are **not yet validated**. The larger ownership baseline began before
these edits, so it cannot serve as final-source acceptance of them. Wait for its
terminal result, then replay focused diagnostics and native every lifecycle on
the final guard source. Internal helper privacy (including dynamic dispatch),
Reflection bypass/authority reconstruction, Task/TaskGroup opacity and remaining
namespace retirement are still open; constructor/copy guards do not close them.

## Expanded ownership result and private periodic helpers (2026-10-12)

The expanded pre-helper-privacy baseline finished **93/98**, not green, in
`/tmp/elephc-task-periodic-ownership-combined-baseline-20261012.log`. All public
Task/every and Mixed-method controls pass; five array-held Fiber start/resume/throw
exception cleanup cases remain red. The minimal retained state-error reduction
reproduces two live blocks/336 bytes. EIR shows an owned ArrayGet Fiber receiver
without a receiver root, while assembly passes the receiver-transfer flag as zero.
Inspect backend scoped-owner classification before changing transfer or refcounts;
no extra decrement or weakened heap assertion is justified.

Constructor guard diagnostic passes **1/1** in
`/tmp/elephc-task-periodic-constructor-guards-20261012.log` (normal and forged-ID
arguments inside a real Task run). Import-only fixtures do not inject the full
facade and test Undefined class instead; the fixture now enters run explicitly.
An older CLI direct-copy probe predates the guards and is not their acceptance
evidence: it emits no guard markers despite a clean heap.

All PeriodicControl native cadence helpers are now private. TaskGroup's controls
collection is element-typed so its drain does not dispatch __close through opaque
Mixed. Private-helper access is granted only to compiler-origin every/parallel
pumps and drain, with a method allowlist and source spans excluded. New direct,
dynamic-method and call_user_func negative heap controls remain to run on final
source, alongside every lifecycle and five-target emission. Reflection access
and Task/TaskGroup opacity remain open.

The reproduced transfer refusal is caused by scanning every SSA Release in the
function, including the disjoint method.null.fatal branch. Ownership-transfer
queries now inspect releases reachable after the consumer, conservatively
including every implicit exception-handler target. Traversal stops when a loop
re-executes the value's producer (a new dynamic owner); absent metadata remains
conservative. Scoped published-slot cleanup still blocks transfer independently.
No runtime refcount operation was added or removed.

New unit pins cover disjoint fatal cleanup, normal cleanup, exceptional cleanup,
unknown current instruction and producer re-execution in a loop. A five-target
Fiber emission pin requires transfer of an array-read receiver while the rooted
Throwable remains caller-owned. Focused final-source native replay is session60820,
`/tmp/elephc-task-periodic-private-controls-fiber-paths-20261012.log`, compiling.
It includes public Task/every, both authority-negative fixtures, the five failing
Fiber cases and both generic exception reductions. The new helper-opacity and
transfer corrections remain unverified until this replay and unit checks finish.

Do not launch another heavyweight Cargo target while session60820 is building.
After it is terminal, run the focused lib filter for
codegen::context::operand_owners::tests and ir_lower::tests::fiber_operand_owners,
then the Task five-target controls and constructor diagnostic against final source.
The last small no-release fast-path edit must be included in that final-source
validation; the running command started before it. Retain the old93/98 evidence
as red, and do not label the new path analysis successful from inspection alone.
At this checkpoint the host has27GiB free with other worktrees building; no
foreign process was stopped or target/cache deleted. Index remains empty and
publication waits for relevant green evidence rather than sweeping inherited WIP.

## Fiber regression closure evidence and opacity integration follow-up (2026-10-12)

The next native replay finished **17/22**, not green, in
`/tmp/elephc-task-periodic-private-controls-fiber-paths-20261012.log`. All five
previously failing array-held Fiber cases now have clean heaps, as do public
Task result and Async every controls. Five failures are opacity integration:
three Parallel every checks infer a fake PeriodicControl construction, private
static factory access is diagnosed at compile time rather than caught at runtime,
and inaccessible runtime callable arrays still abort the process.

The Parallel checker now supplies a type-only PeriodicControl variable in a cloned
environment with an unlexable NUL-prefixed name, instead of a constructor call.
That hint is never lowered and cannot shadow source PHP names or allocate authority.
Private factory rejection moves to an explicit diagnostic test; native copy tests
still require clone/serialize/unserialize refusal with clean heap. The shared
callable-array no-match path now throws TypeError through ordinary PHP unwinding,
with an independent inaccessible-callback heap regression. Direct dynamic-method
error class/message precision remains open because that path currently delegates
through the callable-array boundary; do not claim complete PHP parity from the
common Error catch or weaken the negative guard.

Focused final-source unit/emitter checks are running under
`/tmp/elephc-task-periodic-private-controls-final-unit-20261012.log` (owned operand
paths, Fiber flags, Task lowering and call coercion); native and diagnostic
follow-ups must run after the unit build is terminal to serialize Cargo work.

Published PR bce43fd9e8509b1b8b095f4a6e70da1582ca0693 remains draft/open/mergeable
against base5fc8dd0bd0e77ffa304cacb4f1ad9253df92a840 with141 failed checks and no
pending checks. Job-specific REST log retrieval succeeds (allow terminal escapes
only into a local file); gh run view's whole-run archive retrieval was failing.
The sampled macOS unit job includes web arena reset, GC destructor, frame unwind
and unserialize registry structural assertions plus resource/hash heap cleanup.
These are classified as concrete failures, not yet reproduced or dismissed as
stale snapshots; log `/tmp/elephc-task-published-unit-job-20261012.log`.

PHP oracle `/tmp/elephc-periodic-failure.BPU0rw/short-callable.php` confirms an
incomplete callback array raises TypeError (exact two-member shape required).
The existing short-boxed-callback test previously expected the obsolete process
fatal; its expectation now requires TypeError while retaining failure/OOB
protection. A separate caught short-array regression requires a clean heap after
releasing both owner objects. Current generic no-match diagnostics still differ
from PHP's caller-specific wording and direct dynamic-method Error taxonomy;
retain that parity follow-up as open, not acceptance from a broad Error catch.

Final source has no further production edits after launching session31788 for the
focused unit/emitter filter. After it ends, replay the22 native controls plus
inaccessible/short callable regressions, then both PeriodicControl diagnostics.
No new shared-target build should overlap the unit compiler. Index is still
empty and no opacity/namespace checkpoint has been published.

Focused final-source unit/emitter checks complete **26/26** in
`/tmp/elephc-task-periodic-private-controls-final-unit-20261012.log`: the new
release-path unit pins and looped array-Fiber flags, existing call-coercion roots,
Task/periodic envelope and receiver tests pass on all supported emitter targets.
No native Linux or iOS execution is inferred from this result.

Final-source native replay is now running in
`/tmp/elephc-task-periodic-private-controls-final-native-20261012.log`:22 prior
controls plus inaccessible and both short-callback controls. Wait for its terminal
result before another Cargo build, then rerun both authority diagnostics.
The four sampled structural CI controls also reproduce **0/4** on the current
unit binary (`/tmp/elephc-task-ci-structural-current-replay-20261012.log`). Source
inspection shows assertions tied to old global labels; context-aware replacements
must preserve ordering/ownership invariants and get their own focused proof.

The four structural assertions now compare context-aware ABI accesses and forbid
the legacy global names. Web reset still requires last-bin reset before allocation
total read, frees update and live reset; cumulative peak remains untouched. GC
still requires destructor recount/unpin/sweep ordering and collector reset before
rethrow. Frame unwind still publishes the detached cursor before callback on all
five targets and both PIC modes. Registry still checks the65536 bound before
addressing, retains before deferring release, and rejects shared-global storage.
Only cfg(test) assertions changed in these files; runtime emitters are unchanged.
These edits are unverified until the four-test replay completes after the active
native build. Their earlier0/4 and published CI failures remain retained evidence.

## Final opacity replay and shared magic-hook cleanup (2026-10-12)

Native replay is **23/25**, not green. Public Task/every, private helper denial,
inaccessible and caught-short callbacks, and all five Fiber regressions pass.
The uncaught short-callback expectation checked stderr, but the shared uncaught
exception reporter writes stdout and exits255; a retained CLI probe proves
100 stdout bytes and0 stderr bytes. The assertion now checks the actual TypeError
diagnostic on stdout, without allowing success or dropping the OOB protection.

The copy-guard test still leaves3 blocks/160 bytes. Read-only LLDB identifies both
live objects as PeriodicControl plus its Mixed wrapper, not by size alone.
Independent generic probes are: private clone clean; throwing __serialize leaves
one40-byte object; throwing __unserialize leaves one40-byte object and one40-byte
Mixed box. The serializer's EIR owns an acquired argument snapshot but publishes
no unwind root. Serialize now joins builtin callback operand rooting, with a
generic regression. The hydration helper now roots the stable decoded result box
around its PHP __unserialize call, preserving normal transfer and clearing/releasing
it on exceptional unwind. ABI offsets are verified from the decoder frames:
AArch64 box at sp96/fp-minus16; x86_64 box at rbp-minus96. The helper's emitter pin
requires publication before invocation and detachment after it on all targets.
These fixes remain under replay; nested property-hook failure and __wakeup ownership
are additional coverage gates, not claimed closed by the empty-data reduction.

The four context-aware CI assertions first failed to compile because Emitter::output
consumes its emitter. Tests now retain the Copy Target before taking output; runtime
bodies remain unchanged. Their four-test runtime replay still needs to run.
Current focused serialize/unserialize/copy and short-callback replay is session86126,
`/tmp/elephc-task-periodic-serialization-guard-regression-20261012.log`, compiling.

Maintainer explicitly authorized targeted old inactive Cargo output cleanup.
Only this worktree's target/debug/incremental directory (7.1GiB, last changed
2026-10-10, no live worktree Cargo/rustc or artifact lock) was removed. It is
regenerable from sources; current compiler/staticlibs, sources, logs, other worktrees
and Docker were preserved. No broad target/dependency clean was performed.

The serialization guard replay completed **3/5**, log
`/tmp/elephc-task-periodic-serialization-guard-regression-20261012.log`.
Throwing __unserialize and both short-callback controls pass. Throwing __serialize
still leaves one40-byte object; PeriodicControl copy rejection now leaves only its
one80-byte object, down from3 blocks/160 bytes. This artifact predates the final
Serialize result-ownership refinement: the shared RuntimeFnId contract now marks
the separate wire-string result Independent, enabling the existing non-aliasing
callback root rather than adding a one-off lowering exception. Final-source
five-test replay is session15665, log
`/tmp/elephc-task-periodic-serialization-final-contract-20261012.log`.
Do not infer its outcome from the preceding reduction or mark the gate closed.

Post-refinement `git diff --check` and focused assembly-comment alignment pass.
The builtin EIR boundary audit was deferred after discovering it internally
starts a Cargo exporter; only that queued, owned audit and exporter were stopped
before the native replay completed. No audit result or generated documentation is
claimed. Replay that audit after the focused tests, and retain the four corrected
CI structural tests, hydration emitter pin, Serialize ownership unit, and both
PeriodicControl authority diagnostics as pending final-source checks.

Final-source hook guard replay completes **5/5**, exit0, log
`/tmp/elephc-task-periodic-serialization-final-contract-20261012.log`.
Throwing Serialize input snapshots, throwing Unserialize decoded results, both
short-callback guards, and PeriodicControl clone/serialization rejection pass on
the native macOS host. The checked cleanup cases report a clean heap. This is
focused host evidence, not Linux execution, a full scheduler replay, or closure
of the nested hydration / wakeup / Reflection / API-mode gates.

The first emitter follow-up failed compilation on indexed moves from an Emitter
array; destructuring fixes only the test. The next replay completes **3/6**,
`/tmp/elephc-task-hook-ownership-context-emitter-final-retry-20261012.log`.
Frame unwind, hydration-root emission and registry pins pass. Web/GC probes did
not match the actual direct bin reset / address-then-zero-store sequences; tests
now pin those exact target-aware instructions and preserve reset-before-counter
and reset-before-rethrow ordering. The Serialize contract unit also caught a
real placement mistake: the added variant was inside the Fresh branch, despite
the earlier notes describing Independent. Backend inspection confirms a borrowed
concat-buffer slice persisted on store, not a fresh heap allocation. Serialize
now actually belongs to Independent. The earlier five native successes are
historical pre-refinement evidence; replay them and normal serialization results
before publication. Corrected six-test replay is session59997, log
`/tmp/elephc-task-hook-ownership-context-emitter-corrected-20261012.log`.

Runtime execution-mode regression is reproduced by the current CLI:
`/tmp/elephc-task-runtime-execution-regression-20261012.php`, log
`/tmp/elephc-task-runtime-execution-baseline-20261012.log`, rejects a function-returned
Execution enum at spawn. Required output after repair is AP|42|42, with each
selector called once. Mode selection must not bypass Parallel transfer checking;
named/spread source-order and once-only evaluation remain full-API gates.

Context assertion checkpoint published as
`06a9a6a51bd7803bb3a9ced657aa51632d21c6f3` by normal fast-forward push to the
fork's existing PR branch; PR958 live head confirms that exact SHA, open/draft.
Only cfg(test) changes in web reset, GC destructor, unwind-frame and decoder
registry assertions are included. Their final direct-binary replay is **4/4**,
`/tmp/elephc-task-ci-four-context-controls-final-20261012.log`; runtime bodies and
inherited WIP are not swept into the checkpoint. Exact-head CI is pending.

The combined final-source emitter/ownership replay is **6/7**,
`/tmp/elephc-task-runtime-execution-and-context-emitter-20261012.log`: the four
context assertions, hydration-root emitter and Serialize Independent contract pass.
New runtime-mode dispatch fails during assembly generation: the generic merge-temp
type fallback classifies ScopedConstantAccess as Str, including enum cases in the
selector's ternary. Merge typing now consults the enum metadata, matching the
existing scoped-constant lowering contract. This correction and positional runtime
dispatch remain local/unverified. Both native spawn/every mode tests and a normal
Serialize concat-result stability/heap regression have been added. Replay them
with the five hook/callable controls after the next successful emitter build.

Corrected final-source ownership/context/runtime-mode emitter replay completes
**7/7**, exit0, `/tmp/elephc-task-runtime-execution-enum-merge-emitter-20261012.log`.
The runtime spawn selector's ternary now preserves Execution enum objects and the
lazy dispatch generates valid assembly for all five supported targets. This is
emission/structural evidence, not execution on Linux/iOS. The final native replay
is session88137, log
`/tmp/elephc-task-runtime-modes-serialization-native-20261012.log`, with15 focused
runtime-mode, hook cleanup, normal serialization, post-scope result and repeated
Parallel cadence controls. The new object-transfer rejection and both control
authority diagnostic tests follow after that Cargo process terminates.

Final-source native replay completes **13/15**,
`/tmp/elephc-task-runtime-modes-serialization-native-20261012.log`. All six normal /
exceptional serialization and callable guards, periodic copy refusal, repeated
Parallel cadence/failure, post-scope results and ordinary serialization controls
pass. Both new runtime-mode cases fail heap-debug bad-refcount checks (spawn prints
AP|42|42 first; every prints A before failing). No dynamic-mode acceptance is claimed.
The authority diagnostic replay passes **3/3**,
`/tmp/elephc-task-runtime-mode-authority-diagnostics-20261012.log`: constructor,
private worker proxy factory, and runtime-mode object-transfer rejection.

Retained CLI/LLDB/EIR reduction identifies the prematurely freed object as
TaskGroup: native payload classID53 maps to Elephc\Task\TaskGroup in the emitted
class table, not inferred from its88-byte size. Its release happens in the user
closure before the wrapper tries to retain it for structured drain. EIR shows
repeated OwnedTemp slot loads as owned references, with receiver-root release
plus later slot retirement. Reusable mode/hub slots now use synthetic PhpLocal
ownership and explicit ReleaseLocalSlot retirement, rather than take_owned_temp.
LLDB logs are `/tmp/elephc-task-runtime-execution-{zero-refcount,premature-free}-20261012.log`;
pre-fix EIR `/tmp/elephc-task-runtime-execution-before-owner-fix.eir`.
Current fifteen-test replay is session5696,
`/tmp/elephc-task-runtime-modes-php-local-owners-native-20261012.log`.
Recheck all-target emission after this ownership refinement; the previous7/7 is
pre-refinement evidence. Public named/spread modes and full API opacity remain open.

The corrected synthetic-PhpLocal native replay completes **15/15**, exit0,
`/tmp/elephc-task-runtime-modes-php-local-owners-native-20261012.log`. Runtime-selected
spawn returns AP|42|42 and periodic sequential scopes return Aasync|Pparallel, with
each selector evaluated once and both heaps clean. Normal concat-backed Serialize
results, throwing hook cleanup, copy refusal, cached outcomes and repeated Parallel
cadence remain green after the real Independent contract placement. Last ownership
change requires a fresh seven-control all-target emitter replay before claiming
current multi-target evidence; log
`/tmp/elephc-task-runtime-php-local-final-emitter-20261012.log`.

Final-source emitter replay is **7/7**, exit0, in that log after the synthetic
PhpLocal fix. Final-source authority diagnostics also pass **3/3**, exit0,
`/tmp/elephc-task-runtime-mode-final-authority-diagnostics-20261012.log`.
The builtin EIR boundary audit is running in
`/tmp/elephc-task-runtime-mode-builtin-eir-audit-20261012.log` (session67695);
it invokes the Cargo exporter, now serialized after all focused builds. No
generated public/internal documentation was regenerated before full API acceptance.

Live CI at published head06a9a6 reports failing Curl executable shards on both
Linux architectures. Two retained job logs are
`/tmp/elephc-task-ci-06a9a6-curl-{x86,arm64}-20261012.log` (jobs114932873498 and
114932609073). They show actual callback/eval/output failures including SIGSEGV,
heap-memory-exhausted and eval-runtime-failed diagnostics, not missing global-label
assertions. This sample does not classify every shard or prove a common root cause;
retain the bridge/context integration acceptance gate and correlate inherited
unpublished Curl/eval gateway WIP before further publication. Tracked origin/main
is889f45203e28ca22f6c26c5c3020c5dae933fbd0 at this check, but PR base was5fc8dd0;
refresh live remote/ancestry before the next rebase, not during the active exporter.

The builtin EIR boundary audit completes exit0, with741 AOT registry bindings,
zero backend-dependent homes and zero structural inventory errors. This verifies
the requested target-architecture boundary, not generated-doc sync or API coverage.
No generated doc/registry files changed. Final validation of this local slice is
native15/15, emitter/ownership7/7 and authority diagnostics3/3. Published head remains
06a9a6a51bd7803bb3a9ced657aa51632d21c6f3 (test-only checkpoint); API/ownership and
inherited Curl/eval source changes remain local. Next API gate is shared named/spread
planning with source-order/once-only effects, followed by constructor/Reflection
opacity, public legacy retirement and focused Linux execution/integration proof.

## Shared named/unpack submission replay (2026-10-12)

The initial CLI reductions for named Parallel submission and static indexed full
unpacking compile but terminate with `Task Parallel submission was not lowered`.
TaskGroup interception now sends every checked source form through the shared
argument plan. Source expressions are staged once in reusable PHP locals before
executor selection, with callable metadata preserved and unwind owners published.
The Async branch uses the ordinary checked method helper rather than recursively
intercepting a synthetic TaskGroup call. The Parallel branch keeps its checked
worker preparation. An outer result root protects returned handles while snapshots
retire in LIFO order. Static associative unpacking uses shared key projection and
releases overwritten value snapshots at the overwrite point.

Native focused replay is **4/4**, exit0, log
`/tmp/elephc-task-named-submission-typed-tail-20261012.log`: named spawn/every in both
domains with reordered source effects; static indexed and associative unpacks;
duplicate-key object destruction before submission; runtime indexed argument tails.
Every fixture reports a clean heap. The original named CLI reduction now produces
`IE42`, allocs626/frees626, zero live blocks; retained fixed logs use the
`/tmp/elephc-task-named-arguments-fixed-20261012` prefix.
Existing positional runtime spawn/every controls also pass independently.

An initial dynamic-tail fixture declared its producer return merely `array`; the
Parallel checker correctly rejects its unproven Mixed element shape. Retain that
case as a negative diagnostic, not as a reason to weaken transfer safety. The
positive fixture uses a runtime indexed local with statically proven integer
elements. The initial replay log remains retained as pre-correction evidence.

A further CLI probe with the mode and callback inside a runtime full pack panics
in worker lowering (exit101), log
`/tmp/elephc-task-dynamic-full-pack-20261012.log`. The checker now expands static
indexed packs through the shared projection, then diagnoses dynamic packs hiding
required executor/callback fields. Dynamic input tails remain allowed. This guard
and its spawn/every regressions are pending replay; the four native successes
above precede this final guard. Five-target named/unpack emitter replay is running,
`/tmp/elephc-task-named-all-targets-20261012.log`. Do not count it as Linux/iOS
execution or API closure. Fully dynamic callable transport is not implemented.

During concurrent builds disk availability dropped below1GiB. Under the explicit
cleanup authorization,23 enumerated old inactive compiler/test/library artifacts
from this worktree's `target/debug/deps` were removed after checking open handles.
Current compiler/staticlibs/test binary, sources, other worktrees and Docker were
not touched. These outputs are regenerable, not source or acceptance evidence.
The later large external free-space increase is not attributed to this cleanup.

Final-guard native replay completes **5/5**, exit0, log
`/tmp/elephc-task-named-final-guard-native-20261012.log`. The four submission
fixtures remain heap-clean after the checker guard; the fifth proves that a later
named-input throw destroys earlier owned inputs before the catch and leaves the
same TaskGroup usable for a subsequent real Parallel submission (`DC42`).
The formerly panicking full dynamic pack now exits1 with the intended source
diagnostic, not a Rust panic; fixed CLI log
`/tmp/elephc-task-dynamic-full-pack-fixed-20261012.log`.

The named/unpack emitter control completes **1/1**, generating assembly for all
five targets, exit0, `/tmp/elephc-task-named-all-targets-20261012.log`. This precedes
only the final diagnostic wording refinement (`interval` to `seconds`); rerun the
scoped compiler unit binary for exact final-source provenance. The Cargo command
did not include `-p elephc`, so it also compiled workspace library harnesses with
zero matching tests; subsequent library checks must specify `-p elephc`.
Final dynamic-unpack diagnostic tests and positional runtime controls are under
replay in their dedicated logs, not counted as completed here.

Final-source verification completes: **5/5** named/unpack native heap regressions,
**2/2** dynamic-unpack diagnostics, **2/2** positional runtime-mode native controls,
and **1/1** named/unpack emitter unit covering all five supported targets. Logs:
`/tmp/elephc-task-named-final-guard-native-20261012.log`,
`/tmp/elephc-task-dynamic-unpack-diagnostics-20261012.log`,
`/tmp/elephc-task-final-positional-runtime-controls-20261012.log`,
`/tmp/elephc-task-named-final-emitter-20261012.log`.
`git diff --check` also passes. These focused checks are not Linux/iOS execution,
full scheduler acceptance, current CI verification or absolute review consensus.

## Next reproduced authority gate: reflected periodic proxy factory

The retained native probe `/tmp/elephc-task-reflection-proxy-20261012.php` creates
one Async registration, obtains `ReflectionMethod(PeriodicControl::class,
"__fromNativeId")`, and invokes it with native ID1. It successfully manufactures
a second PHP proxy and calls stop on the real registration: stdout
`forged:stopped`, exit0, allocs823/frees823, zero live blocks. Compile/stdout/stderr
logs use `/tmp/elephc-task-reflection-proxy-20261012` as their prefix. This is an
authority violation despite the clean heap; no opacity completion is claimed.

Kioku locates the known reflected-target path at
`src/ir_lower/expr/reflection_method_calls.rs`: `invoke`/`invokeArgs` resolve the
class and method, then static dispatch forwards to ordinary static-method lowering.
The direct-source private-access throw site is absent on this reflected call.
Protect compiler-owned capabilities consistently across reflection invocation,
closure extraction, construction, property access and eval/native gateways; do
not globally change private-method Reflection semantics for ordinary PHP classes
or patch only this literal probe. Task/TaskGroup constructor/helper privacy and
legacy public namespace retirement remain distinct open gates.

No new checkpoint is published from this slice: its source/tests still depend on
the inherited unpublished facade/runtime work, and the reflected authority gate
is open. The named/spread implementation and exact focused evidence are durable;
continue with the authority boundary, then focused Linux execution and review.

## Opaque Reflection policy implementation under replay (2026-10-12)

The shared ClassContract now declares ClassReflectionPolicy, defaulting to ordinary
PHP behavior. Only PeriodicControl opts into OpaqueCapability for this slice:
reflected construction, actual storage reads/writes and non-public method calls
are refused; public methods and metadata remain available. The neutral operation
and denial contract is consumed by native lowering and Magician, not copied into
separate backend name lists. Contract tests pass2/2. Generated documentation is
still deferred by the API acceptance gate; update-builtin-docs has been read and
the final exporter/audit workflow remains required before publication.

Known native reflected methods/storage now refuse after evaluating source argument
effects, rooting operand owners and retiring them in LIFO order. Dynamic allocation
looks up runtime class IDs before candidate/fallback dispatch so an alias cannot
manufacture a control by changing its class-string. The lookup ID and class string
are both detached from temporary ABI storage before the ordinary Error path.
Magician applies the same contract before invoking reflected native/eval methods,
reading/writing normal/raw storage and constructing reflected classes.

First six-test native replay is **3/6**, not acceptance, log
`/tmp/elephc-task-reflection-policy-native-20261012.log`. Storage/allocation refusal,
public methods and ordinary PHP private reflection pass, with clean heaps. The
ordinary fixture also prints41|42|1 on reference PHP. The alias fixture initially
requested a dynamic ReflectionClass constructor already rejected by the compiler;
it now mutates an existing ReflectionClass selector through its physical __name
storage to exercise the actual runtime-ID guard, including class_alias.

The first private factory invoke is refused; an intervening source argument call
invalidates static reflected-target flow facts, and reusing the ReflectionMethod
then routes invokeArgs to its unsupported placeholder instead of the known-target
path. The fixture now recreates its known ReflectionMethod before the invokeArgs
check. This does not close the separate lost/dynamic-target Reflection behavior.
Eval fails during assembly generation: static call materialization receives0 args
for1 parameter. A focused diagnostic now identifies the current EIR function;
resolve that mismatch before claiming eval opacity. The current replay is
`/tmp/elephc-task-reflection-policy-native-second-20261012.log`.
Focused assembly-comment alignment and diff hygiene pass; five-target emission,
raw-property and dynamic/alias eval coverage remain pending.

Second replay is **4/6**, log
`/tmp/elephc-task-reflection-policy-native-second-20261012.log`: known invoke and
invokeArgs both deny after recreating the reflected target, and storage/allocation,
public methods and ordinary private Reflection remain heap-clean. The alias
fixture now hits class_alias's documented top-level-only compiler restriction;
move that declaration to top level, retaining the hostile selector mutation.
The richer eval failure names Elephc\Parallel\run. Backend callable guards provide
source-visible zero/one arguments, but their signature can acquire a hidden
func_args collector under eval. Both static guard emitters now use the existing
source_visible_signature/source_method_entry_symbol adapter contract, not local
collector synthesis or a truncated physical ABI. Third replay is
`/tmp/elephc-task-reflection-policy-native-third-20261012.log`.

The native fallback for a definite ReflectionMethod with no tracked target now
raises a catchable unsupported-path Error after source argument evaluation, instead
of its silent null placeholder. A regression keeps the lost-target reuse as an
explicit refusal and separately re-creates a known target to test invokeArgs's
capability denial. This does not add full native dynamic Reflection dispatch.
The fallback change was written while the third build was active; that artifact
may precede it. Replay the final source before accepting the updated fixture.

Third native replay is **3/6**, log
`/tmp/elephc-task-reflection-policy-native-third-20261012.log`; do not count it as
the final unsupported-target implementation. Its factory stdoutIAIIBdone proves
the compiled artifact predates that new refusal. The eval ABI mismatch is gone,
but assembly now fails on ARM64 branch fixups in an approximately2.4-million-line
generated helper artifact. A retained macOS sample
`/tmp/elephc-task-reflection-eval-sample-20261012.txt` locates the test thread in
assemble_from_stdin/Child::wait, not a scheduler execution loop. Assembler diagnostics
include distant b.ne/cbz branches to common method fail/body labels.

The method helper now uses existing wide zero/nonzero/equal branches and a shared
new unequal-wide comparison helper. Its ABI unit pins the short inverse branch
plus unconditional transfer on ARM64 and native jne on x86. Scope-pointer checks
also skip over a wide jump rather than branching directly to a distant fail path.
Assembly-comment alignment passes. Recheck final emission and actual assembly;
valid textual emission by itself does not prove branch reachability.

The alias probe also exposes an existing compiler limitation: generated class_alias
lowering treats a final target as an illegal base class. Keep an explicit error
regression instead of weakening final-class validation or calling alias support
complete. The positive runtime-selector probe now uses ordinary/case-variant
PeriodicControl names. Final alias support/identity is a separate PHP-parity gate.
Current focused library replay is
`/tmp/elephc-task-reflection-policy-emitter-20261012.log`; final native replay follows
after that Cargo owner terminates. All source work remains unpublished.
