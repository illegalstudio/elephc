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
Legacy namespace references in backing-engine implementation are transitional;
this does not establish completed public namespace migration or heartbeat support.
Historical first canonical replay: the public Task run/group facade was not yet
implemented, and the tests intentionally failed before the facade was added.
That obsolete first command was session 83131,
`/tmp/elephc-task-canonical-api-red-20261012.log`; current proofs are recorded above.
Both iOS bridge checks now completed successfully; SDK linkage is still unproved.
