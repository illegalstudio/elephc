# Persistent web application lifecycle

- [x] Audit current source and the closed PR 456.
- [x] Agree on `--web-lifecycle=request|application`, defaulting to `request`.
- [x] Characterize the unchanged worker reset over HTTP.
- [x] Separate post-fork bootstrap, per-request preparation/dispatch, and shutdown.
- [x] Preserve application ownership while reclaiming request temporaries and cycles.
- [x] Add graceful shutdown and recycling without changing existing isolation modes.
- [x] Validate retained values, exceptions, bounded memory, preload, and default behavior.
- [x] Document the typed API and provide a PHP-compatible example with a native adapter.

## Publication and main integration, PR #2127

The user subsequently authorized opening the PR, following CI and recovering its
queue entry through a verified merge. PR https://github.com/illegalstudio/elephc/pull/2127
is open and ready with `ci:macos`. Publication is tracked in Code Journal plan
`83dcd1ef-2352-45bf-a569-adf7ff19e865`. The previous delivery statuses below are
historical; the implementation is now committed and published.

The original signed feature commit is `a5904e148e46c0558f2f6f6713ecb2ed6925a88c`.
Integrating main `5aa6bdb8b9c41dbd99e9065f5ad974bbacc3f381` required twelve content
resolutions. Preserve its exact allocator classes through 2048 bytes and its
consolidation/reset helpers, while keeping the constant-time live-base guard for
web frees. Adapt application roots/shutdown to the split static-property metadata
and retire native callback bindings after PHP/resource destructors. Hosted Zend
request storage is currently incompatible with retained application state;
projects declaring `[extension.dependencies]` receive an explicit diagnostic for
application lifecycle, documented and tested before native artifact resolution.

Machine queue job `50729bce` passed the warning-free build, seven library checks
(including the new admission and all-target teardown cases), twenty-two native
coroutine/allocator regressions, and all forty application HTTP cases. A
native callback-cycle control confirmed that the final lease retirement detached
an uncollected cycle: only the shutdown hook logged before correction. A final
protected cycle pass and inventory prune now run after callback retirement. The
new HTTP regression and the identical native control log shutdown then destroyed;
all-target ordering checks pass. There are now 41 application HTTP cases with
focused green evidence across the final paths. Remote CI and a clean review
of the integrated head are pending. CI on integration commit `c1f4999c4f` also
exposed an incomplete FakeOps ArgumentCountError hierarchy. The test simulator now
models TypeError, Error and Throwable correctly, and the omission regression checks
all three catches. Both that exact CI failure and private-control catch regression
pass with curl enabled. Queues `508c25aa` and `b12648fb` verified these corrections,
the final native callback cycle and the complete builtin documentation audits. Queue membership is currently absent; native
GitHub HEADGREEN admission and final remote ancestry verification remain required.

## Post-integration review and CI corrections

Claude round 13 (`5ebcdb85-abee-4ce3-b192-18fb8f61bacc`, result event 8096)
reviewed the full integrated source at `565146078b`. Its single accepted P3
finding concerned suspended `__debugInfo`: the upstream debug-info guard sent the
private eight-byte cancellation object through the Throwable formatter. Both ABIs
now divert that marker to the existing PHP fatal path before reading exception
fields. Real finally exceptions retain their warning and trace. Queue `02e3a266`
passed the warning-free curl build, all-target/PIC guard test, both new native
regressions and three existing fatal controls. The two independent native/PHP
probes now agree byte for byte, including exit 255 and empty stderr.

The same queue passed all 21 generic shape-sharing tests, the generic report
oracle, the example corpus and ten SysV alignment checks. Non-coroutine modules
omit unique suspended-owner visitors so identical generic implementations can
still fold. The corpus excludes the application example's non-default profile,
which already has a real application-mode HTTP test. The SysV parser discards
inline comments and models `leave`; its old `gc_mark_reachable` exception was
removed because the helper is now analyzable. Controls still detect actual
misaligned calls and missing frame restoration.

Linux ARM64 web CI exposed two object-edge recount paths that restored the scan
cursor as a physical heap header. Candidate mode stores a libc vector-slot
address there. Both restores now resolve that slot before reading class data.
The identical native ARM64 self-cycle changed from SIGBUS during collection to
exit zero, two collected graph nodes and one destructor (queues `19e564c6` and
`73b503e3`). Queue `a71f3513` passed the all-ARM/PIC restore regression and complete
collector/coroutine subgraph assembly on all five supported targets. Three exact failed ARM64 HTTP cases also pass unchanged: superglobal cleanup,
throwing/resurrecting cycles and saturated incoming edges. The remaining native
container, sparse graph and memory cases are being prepared with target-matched
bridge libraries. A new full-work review remains pending. No failed CI run has been
retried as an infrastructure incident, and queue admission has not been attempted.

## Apple report-entry assembly correction

Claude round14 (`2c0c53ea-64dd-493d-890e-8d76879224da`) returned clean on
`e22e3b9d1c`, with179 unchanged reviewed source/test/API-doc files. Exact-head CI
then passed every Linux job, including all four web shards, and all10 PDO live
jobs. Apple iOS link jobs exposed a gap in the debug-info guard check: it verified
emitted text without assembling that report subgraph. A raw conditional transfer
to the global `__rt_debuginfo_fatal` entry is rejected by Mach-O dead stripping.
The logs for XML, curl and fileinfo, on device and Simulator, show the same error.

The guard now uses the existing target-aware wide equality helper: a local inverted
condition followed by an unconditional cross-entry transfer. A real report assembly
regression failed on the old source (queue `4ee3f0a5`) and passes with the correction
(queue `5034be47`). It covers all five targets, direct/PIC references, dead stripping
and executable/library boundaries,40 combinations. The warning-free build, the
existing all-target marker ordering check and both native fatal/error controls pass.
Another clean full-work review and new exact-head CI remain required before queue
admission. No failed run was retried as a transient infrastructure problem.

## Initial implementation delivery, before PR publication, 2026-10-28

Implementation, focused validation, API documentation and the portable example are
complete. Independent Claude round 12 returned `VERDICT: clean` with no actionable
findings. All findings from rounds 1 through 11 were verified and corrected; none
were declined. Additional review rounds and the collector/coroutine extensions
were explicitly authorized by the user at the boundaries recorded below.

The final review is `7fe5bf95-fa94-478b-b8db-005f8fe035aa`, result event 8009.
All 173 source, test and API-documentation files match their frozen review hashes.
The sorted JSON manifest excluding this execution ledger has SHA-256
`08e3262858d65f840098aa4065085bb171a59ce69e05a078fd41b0e61a63058a`.
The published result precedes process-exit event 8010; both are acknowledged.

Forty application HTTP cases have focused green evidence across the final feature
paths. The 2,000-request stress kept live PHP memory at 313,976 bytes and RSS at
19,648,512 bytes. Coroutine and ownership regressions, exact default-mode/preload
checks, a warning-free build, builtin documentation regeneration/audits, PHP lint,
85 emitter comment checks and `git diff --check` passed. Native execution was on
Linux x86_64. Complete EIR fixtures and direct/PIC helper graphs assembled for all
five supported targets; other targets' native execution remains for CI.

The source remains uncommitted in `feat/web-isolation` at base/head `650933501a`.
No PR exists, so the Greptile stages do not apply. No commit, push, PR, merge,
installation or remote deployment was performed. The separate pool JSON task
`1f476909` and other pre-existing follow-ups listed below remain open.

The following sections preserve chronological execution history. Earlier pending
statuses describe those stages, not the final outcome above.

## Contract

Application lifecycle is initially available only with worker isolation. Bootstrap runs
after fork and before accepting requests. It registers one nominally typed application.
Each request constructs fresh metadata and superglobals and invokes the same application.
Ordinary PHP globals and statics are worker state in this opt-in lifecycle. Runtime
request state and internal session state remain request scoped. Shutdown runs once after
draining requests on graceful termination and planned recycling. Bootstrap failures never
admit requests. PHP exceptions remain inside generated callbacks and cannot cross Rust.

## Ownership

Use the existing non-moving, reference-counted worker heap. The registration owns the
application, and PHP stores retain or persist values saved during requests. Request
boundaries must release transient owners and collect cycles without bulk arena reset.
Audit every reset operation: clearing a pointer is not a substitute for releasing its
allocation, and releasing all resources must not close application-owned handles.
Stress bounded replacement and discarded values on normal and exceptional paths; use
actual allocator live bytes rather than resetting counters to conceal retained blocks.
Any missing ownership path exposed by removing arena reclamation needs a focused test
and a fix before claiming acceptance.

## Scope

The existing request lifecycle and master preload behavior remain unchanged. The pool
JSON crash tracked in task 1f476909 is separate. PR 456 is historical context, not an
implementation base: it booted before fork and immortalized only boot allocations.

Code Journal task: d7cf8135. Plan: 18edc617.

## Validation progress

The initial unmodified head passed the focused worker static-reset HTTP test. The
implementation passes ten application lifecycle tests and six existing web-mode
regressions on Linux x86_64. Request-boundary assembly assertions cover all five
supported targets. Executable checks cover registration, retained graphs and streams,
fresh superglobals and body snapshots, request/shutdown errors, failed bootstrap,
recycling, preload, the shipped example, and stream-context destructor re-entry.

The final JSON-enabled 2,000-request stress exposed a pre-existing source-conversion
leak in json_decode/json_validate. Its boxed-string conversion now has an owned EIR
call argument, with normal and exceptional cleanup. Twelve JSON heap-debug tests pass
with IR optimization on/off and the PHP 8.5 oracle; seven source-order JSON tests pass.
Five bootstrap failure variants pass, including duplicate registration and early return.
The expanded HTTP stress includes JSON-retained cache values, arrays, object cycles,
stream callbacks and open output buffers. Between requests 20 and 2,000, live heap
remains 310,400 bytes and RSS remains 21,585,920 bytes. The measurements do not reset
live allocation counters or reclaim the arena. This fix is separate from the pool crash
in task 1f476909.

The builtin registry/API documentation regeneration and all three audits pass, as do
PHP syntax checks, assembly comment checks, and git diff --check. Native execution was
validated on Linux x86_64; other supported targets have focused emitter coverage and
still require the normal CI target matrix. Implementation is local to this worktree;
no commit, pull request, installation or upstream release was performed.

## Independent Claude review

- [x] Obtain a full read-only Claude review of all uncommitted work.
- [x] Verify findings, fix actionable defects, and run focused checks.
- [x] Obtain a clean full-work review, with explicit user authorization for additional rounds.

The user requested the me-delegate-review workflow with Claude. Changes remain
uncommitted, and no pull request exists for feat/web-isolation. Greptile stages
therefore do not apply. The host owns all fixes; reviewers may not change files.

Round 1: ffcc5813-448f-492d-8ae6-80ece0d4d6b9, changes requested. All eight
findings were verified and fixed: valid superglobals throughout user callbacks,
complete implicit stream teardown, deferred termination signals during PHP callbacks,
owned four-argument context-option updates, bootstrap exception diagnostics, guarded
adapter phase methods, the compiler stack wrapper, and assembly group comments.
No finding was declined. Verification also fixed an application-only accept/recycle
race and normalized boxed option arrays before publishing the native context root.

All sixteen application tests pass, including the six new review regressions and
six bootstrap failure variants. The final 2,000-request stress has live heap usage
of 309,744 to 310,072 bytes and unchanged RSS of 19,464,192 bytes. Six existing web
regressions and three native resource ownership regressions pass. The small-stack
prelude test and five-target application-boundary emitter test pass. Regenerated
builtin documentation passes every audit. Two pre-existing stream-context limitations
remain separate follow-ups: boxed inputs in legacy stores (3edc480f), and optional
default-context option setters (9539feac).

Round 2: 89636f6f-4c92-4ae3-88f9-d4df21c00ea3, changes requested. Five findings:
ordinary nested foreach request garbage leaks; callback signal masks reach child
processes; suspended Fiber values fail across requests; slow failed bootstraps evade
startup failure accounting; and the commit extern can regress the native phase.
The host is verifying and fixing these before round 3. No finding has been declined.

All five round-2 findings are now fixed. Dynamic-key mutation retires the old Mixed
frame box and transfers its replacement owner through EIR. Coroutine argument
consumption is declared in the intrinsic contract and retained at direct, method-wrapper
and eval boundaries, including invalid-state cleanup. Shared thread-local signal
deferral restores original masks around native, PCNTL and eval process creation.
Slow bootstrap failures have a dedicated supervisor exit code, and commit is monotonic.

Nineteen application tests pass. The expanded 2,000-request stress includes ordinary
nested foreach reconstruction and repeated four-argument updates of one stream wrapper:
heap 314,712 to 314,832 bytes, RSS unchanged at 21,622,784 bytes. Nested-array alias
cleanup and nullable Fiber transfer/error cases have clean heaps in both optimizer
modes. Shell self-termination is checked in bootstrap, request and shutdown with
15 execution markers, PCNTL fork/exec termination passes, and evaluated shell process
inheritance passes. The shared mask guard and five-target coroutine wrapper emitter
tests pass. The broad pre-existing Fiber/Generator audit remains task 0546ffcd; the
proven resume/suspend fixes and their evidence are attached there. Final round-2 target,
default-mode, prelude parity, build and documentation checks all pass. The native
process bridge and coroutine wrapper have emitter checks for all five targets.

Round 3: b41daffa-c8c8-45ab-9de3-a7056c5a9b66, changes requested. Three verified
findings: full request-end GC scans retained application data; fork children retain
the cooperative worker signal disposition; callback exit restores too much of the
signal mask and loses application changes to unrelated signals.

The two signal fixes pass focused validation. Fork children
reset only dispositions equal to the registered worker handler before unmasking,
preserving PHP handlers and SIG_IGN. Callback exit restores only SIGINT/SIGTERM bits,
preserving other application changes across bootstrap, requests and shutdown.
Queue job 065e2890 built the compiler and web/PCNTL/Magician bridges without warnings.
The nested-mask unit test and all five selected HTTP regressions pass: fork without
exec, application mask changes across callbacks, PCNTL fork/exec, fifteen native shell
child markers across all three phases, and graceful draining of blocking socket reads
with and without a receive deadline. Assembly comment checks and git diff --check pass.

The GC probe measured 2.163 ms/request with a small heap and 30.758 ms/request with
58,661,048 retained bytes. The API documentation now states the current linear scan
cost. No GC behavior or destructor timing has been changed to conceal this finding.
The user has been asked to choose between a broader collector redesign preserving
per-request cleanup, and threshold-based cycle collection with explicitly deferred
cyclic destructors. This decision is required by the me-delegate-review stop rule for
design decisions and broad refactors. The remaining two independent review rounds
have not started. No review finding has been declined.


## Candidate collector redesign (user approved option 1)

- [x] Record the explicit decision to preserve per-request cleanup and cyclic destructor timing.
- [x] Queue all nonfinal managed graph releases, including COW owner retirement.
- [x] Discover candidate forward closures and run existing root/destructor/resurrection phases on that subset.
- [x] Verify stale-address reuse, old and newly detached cycles, throwing/resurrecting destructors and stable memory.
- [x] Assert unchanged retained graphs are absent from later scans and rerun the retained-heap timing probe.
- [x] Resume Claude review with rounds 4 and 5 still available.

The user chose option 1 explicitly. Deferred/threshold cycle collection is not an
accepted substitute. Pending roots and active snapshots use non-owning raw vectors
outside the PHP heap. Candidate header flags are independent of the incoming-edge
counter, and exact allocator live-base checks precede reads of queued identities.
The collector discovers forward closures iteratively through the existing child
layout visitor. Only that closed subset participates in incoming-edge counting,
root marking, destructor snapshots, recounting after callbacks, and final sweep.
References from outside the subset remain external roots. Default request lifecycle
keeps the legacy collector. Work and targeted regression validation are in progress.


The redesign passes all focused GC cases. Nonfinal decrements can be temporary leases
of still-rooted data, so suspect queues alone were insufficient: the 20,000-object
regression observed 40,005 visited nodes after the direct visitor dispatch was fixed.
A generated visitor now reads actual initialized global/static owners and anchors
normalized roots plus direct object children before expansion. It never walks large
array/hash contents. Anchor metadata is cleared before counting reuses its bit field.
The performance regression compares cumulative visited-node deltas across requests.

Queue 1f0139ab passed 24 application tests, including a 2,000-request stress with live
heap 314,600 bytes and RSS 19,582,976 bytes unchanged. All five target assemblers pass
candidate collection in direct/PIC modes with Darwin dead stripping; incoming-cache
and array-leaf suppression emitter checks pass. Queue 241b923f passed all four GC
HTTP tests on the cumulative-counter implementation, exact/saturated fan-in boundaries
4094/4095/4096, a saturated unreachable cycle, and ordinary COW allocation statistics.
An explicit collection may return zero after an earlier automatic safe point already
collected the test cycle; the HTTP saturation regression checks destructor counts and
bounded live memory instead of assuming which safe point performed collection.

The final warning-free build passed in queue 7048db80. Queue 29073c0f measured one
worker, five warmup requests and thirty samples per case using a 268,435,456-byte arena:
0 retained objects, mean 1.638 ms, median 1.603 ms, visited counter 1 throughout;
400,000 retained objects with owned strings, 65,060,592 live bytes unchanged, mean
1.294 ms, median 1.287 ms, visited counter 800,001 throughout. The nonzero initial
counter is bootstrap work; no later collection visited those retained nodes. This
removes the prior measured full-heap cost without deferred cycle cleanup. The API and
memory-model docs now describe candidate collection and the remaining affected-graph
cost. No review finding has been declined. Claude round 4 is the next step.

Round 4: 56daf511-e8c4-4ab1-8c29-218a6f10af87, independent full-work read-only review in progress. Source is frozen; no clean verdict yet.


Round 4: 56daf511-e8c4-4ab1-8c29-218a6f10af87 returned changes requested.
Three findings were verified against source and the supplied probes:
- P1: coarse small-bin fit scans and web double-free chain scans persist between
  requests and make retained-graph shutdown quadratic. Preserve the safety check
  using exact live-base membership; make small bins exact by aligned size.
- P2: a plain read through App -> Kernel -> Service -> items re-traces the retained
  closure (40003/800003 visits for 20000/400000 leaves). The prior shallow fixture
  measurements remain correct but do not establish a general retained-read bound.
  Object-spine-only pruning also cannot prove services stored in a container map.
- P3: op() needs per-call instruction intent and logical group comments; fix three
  additional comment defects and an unreachable cleanup jump.

No finding was declined. Round 5 remains available after verified fixes and focused
validation. The user already authorized a broad collector redesign preserving cleanup;
that authorization remains in force. No deferred cyclic destruction is accepted.
Eight existing default-mode HTTP regressions passed after the candidate redesign
(queue 3ceef9bc). Review scratch was removed by Claude; host scratch remains until
its probes and remaining review are finished.


## Round-4 fixes and final review candidate

- [x] Replace coarse buckets with eight exact aligned small-block bins on every target.
- [x] Keep web duplicate-free protection using exact live-base membership before retirement.
- [x] Preserve opt-in heap-debug full-list validation and cache-drain accounting.
- [x] Replace repeated retained-closure scans with validated non-owning rooted paths.
- [x] Verify object spines, map entries, rehash, replacement, retagging and immediate cyclic cleanup.
- [x] Give every candidate instruction its call-site intent and add logical instruction groups.
- [x] Pass all 28 application HTTP tests, including 2000-request memory stability.
- [x] Complete final default-mode/allocator checks.
- [x] Complete round 5 and resolve its finding in later user-authorized rounds.

The root-path cache is an immutable sorted spanning forest of (node, parent, logical slot)
records, three words per entry in reusable libc storage. Every use validates exact live
bases and the current typed owning edge until it reaches a freshly enumerated root.
Hash records use logical indexes and reacquire the current entry buffer. Detached,
replaced, retagged, reused or unprovable paths enter the immediate candidate collector.
No mutation barrier or persistent raw request-heap pointer is treated as ownership.
Bootstrap builds the forest once. Boundary collections can refresh it when unproved
live state remains. A refresh visits the rooted graph; native capacity follows the
largest recorded graph. Opaque callback edges retain the ordinary fallback path.

Queue 64c18f9b passed warning-free build and six allocator regressions, including a
50000-free/1000-later-allocation probe-budget harness. Queue 11a5df1f passed all five
GC HTTP cases then present, and direct/PIC assembly on all five supported targets,
including the allocator and proof helpers. Queue 358dbb94 passed all 28 application
tests with the added invalidation and 100000-object shutdown regressions. Between
request 20 and 2000, PHP live memory remained 313976 bytes and RSS 19546112 bytes.

Queue 75c14276 measured one worker, a 268435456-byte PHP arena, five warmups and
30 samples per path. With 400000 objects each owning a 40-byte string, live memory
on reads remained 65061248 bytes. Object-spine reads averaged 0.378 ms, maximum
five cumulative GC node visits per request. Map-held service reads averaged
0.389 ms, maximum 12 visits. The corresponding no-read requests averaged 0.376
and 0.385 ms. Both retained workers shut down in about 114 ms. The empty object
fixture averaged 0.382 ms on reads. These local measurements are not latency
promises; cache construction and affected graphs still require work. Native
execution is Linux x86_64; the other supported targets have assembly validation,
with executable CI still required. No PR or commit exists, and none is being created.

Queue b1cd46c0 passed the warning-free build, 14 focused allocator/GC codegen tests and eight default-mode HTTP regressions. The web duplicate-free guard also passed with heap-debug explicitly disabled. All 64
changed/new assembly-emitter files pass comment alignment, and git diff --check
passes. No review finding has been declined. Claude round 5 remains pending.

Independent full-work Claude round 5 started as 273a4f01-bf26-46b1-a15b-ad92efac50b1. No verdict yet. Production source, tests and API documentation are frozen.


## Round 5 outcome and reviewed-finding correction

Claude round 5, 273a4f01-bf26-46b1-a15b-ad92efac50b1, returned changes requested
with one verified P1. A single new retained value made the boolean cache-refresh
flag rebuild all rooted proofs on every write. The reviewer measured about 70 ms
and 800000 visits per write with 400000 unrelated retained objects. Its full-work
review found current typed proof validation, membership/reuse handling, incoming
mask retirement, cleanup scopes and earlier fixes sound. No finding was declined.
The provider exited by signal 9 after publishing the explicit result; its sender
is unknown. The delegation remains completed, with all events acknowledged.

The fix changes the flag to a saturating work debt. Each actually traced surviving
snapshot member earns one unit. A request boundary rebuilds only after debt reaches
max(256, the previous forest node count); rebuilding clears the debt. Ordinary
candidate collection, cyclic destructors and error cleanup remain immediate. Cold
unproved services earn reusable proofs from work already spent on their closures.
API and internals docs state the amortized bound and possible individual rebuild
latency. No incomplete pointer-store barriers or deferred destruction are introduced.

A new HTTP regression makes 30 requests retain both new objects and the preceding
Request snapshot while keeping 20000 unchanged objects. It checks retained values,
per-request cycle destruction, bounded total visits and small steady-state deltas.
The total includes one linear cold collection of constructor-queued leases. The
rehash/replacement test now verifies cache warmup after the earned budget as well
as immediate detached-cycle destruction.

Queue dc0a7a53 passed warning-free build; its initial regression assertion exposed
the one-time constructor queue work, so the test was corrected to enforce O(N+R)
total work and a small per-request bound after that single cold pass. The final
queue 44cae79c passed all 29 application HTTP tests and five-target direct/PIC
assembly. The 2000-request stress had live PHP memory 313976 bytes and RSS 19632128
bytes unchanged between warm and final measurements. All 64 changed/new emitters
pass comment alignment and git diff --check passes.

Queue 49b263b9 independently reran the reviewer's exact two fixtures, one worker,
256 MiB arena, five warmups and 20 samples/path. With 400000 retained objects, storing
one new Leaf averaged 0.371 ms (30..102 GC visits/request), and retaining Request
averaged 0.373 ms (4 visits/request). Both workers shut down in about 114 ms. The Leaf
fixture deliberately grows its short retained history; the Request fixture live
PHP memory stayed 65060360 bytes during writes. These are local measurements and
not latency guarantees. Previous 14 allocator/GC and 8 default-mode HTTP checks
passed in queue b1cd46c0; the last correction touches only application cache work.

- [x] Correct the only round-5 finding with a focused regression and matching probes.
- [x] Revalidate application acceptance, stable memory and every target assembler.
- [x] Obtain user authorization to continue beyond the five-round review limit.
- [x] Obtain a clean independent review of the final correction.

The me-delegate-review five-round stop rule now applies. No sixth review has been
started. Task d7cf8135 and this plan remain active; the subsystem journal doc remains
draft. All source edits remain uncommitted. There is no PR, push, installation or CI
claim. Host-owned scratch is being removed after its evidence is recorded.

Final cleanup completed: two older task-owned debugger probe server groups were verified by exact executable path and stopped gracefully; the task-specific scratch directory was removed after a zero-process check. No active build job or delegation remains.

User authorized continuation on 2026-10-28 (puoi continuare). Independent full-work Claude round 6 is being started with read-only checkout/shared-Git mounts. A verified 123-file source snapshot is saved in task-owned target/web-review6.IwgxTQ until review completion. No source/test/API-doc edits are being made during review.

Round 6 started: 7070ab4c-dc6c-4110-a8dd-82590742e777. Supervisor PID3279311 and guest PID3279312 both inherit verified read-only checkout/shared-Git mounts. Guest reached ready7707; no verdict yet.


## Round 6 outcome and corrections

Claude round 6, 7070ab4c-dc6c-4110-a8dd-82590742e777, returned two verified
findings. P1: each queued descendant revalidated its full cached path, both on
success and after a shared ancestor changed. The 4000-node native list probe
repeated about 32 million path visits and 1.1 seconds per full read or prepend.
P3: rebuild budgets counted graph records but omitted scalar/string/empty hash
slots inspected by the visitor. Sparse tables could therefore be rescanned
without earning their true scan cost. No finding was declined.

- [x] Check snapshot membership before proof validation.
- [x] Memoize successful and unproved prefixes for exactly one callback-free prepare.
- [x] Advance the memo epoch on every prepare, including destructor recounts.
- [x] Charge structural slots, including sparse scalar hashes and native retained edges.
- [x] Add deep cyclic read/prepend/drop, sparse hash and pure native count regressions.
- [x] Finish focused validation and obtain an independent clean full-work review.

The forest retains its original three-word record layout. A parallel native word
stores each record's epoch/outcome. Prefix stamping is iterative, without a
recursive native stack. Rebuild thresholds use the last complete structural scan
cost, while every surviving candidate earns matching scan units. Cycle collection
and destructor timing remain immediate. Native diagnostics include actual scan
work and expose the current rebuild cost to adapter-specific test fixtures.

Queues 9e9ec7cd and 03f18dfa passed the revised 4000-node native probes, deep cyclic
HTTP regression and five-target direct/PIC assembly. Reads now use about 32000
counted units and 2.9 ms; repeated prepends use about 52000 to 80000 units and
1.9 to 2.6 ms. Counts include the new structural/memo work. Queue 53c8721d passed
the 300-request sparse scalar hash regression and pure native callback count test.
The sparse fixture preserves 16384-entry capacity after deleting all but one key.
Both tests verify immediate per-request cyclic destruction where applicable.
Sources remain uncommitted and the task remains active pending the next review.


Final round-6 validation: queue bc407627 passed all 31 application tests, pure native
edge counting, the 50000-free allocator probe, five-target direct/PIC assembly and
all-target collector/release callback coverage. The earlier queue 61e37cb8 passed
build but exposed an obsolete aggregate-unit allowance in the retained-write test.
Its steady per-request bound already passed. The test now bases its one cold pass
on structural scan cost, preserving the per-request <1000 bound and exact cleanup.
PHP live bytes were 313976 at both warm/final samples; RSS was 19451904 at both.
All 59 edited/new codegen/runtime emitter files pass comment alignment and diff
whitespace checks pass. A fresh snapshot is being saved for independent round 7.


Independent full-work Claude round 7 started as 20d07d8e-b7ea-4f23-bd46-b887e7bb0eaa.
Supervisor 3365469 and guest 3365470 inherit verified read-only checkout/common-Git
mounts. The fresh snapshot covers 128 changed/new files. Guest ready/status events
7718/7719 confirm review has begun. Production sources, tests and API docs are frozen.


## Round 7 outcome and callable graph correction

Claude round 7, 20d07d8e-b7ea-4f23-bd46-b887e7bb0eaa, completed with one verified P1.
Native callable descriptors were raw kind-0 allocations and tag-10 ownership edges
were absent from every GC phase. A request-local Holder storing its own closure or
arrow function leaked 4328 bytes per request and never ran its destructor. A direct
self-cycle control was reclaimed every request with flat live bytes. Host queue
22caf521 confirmed all 15 responses; no eval was involved and no finding was declined.
All round-6 fixes and earlier lifecycle, allocator, signal and cleanup paths were
reviewed as sound. Sources stayed frozen through the result.

- [x] Register a distinct managed callable kind and stamp every runtime descriptor producer.
- [x] Share typed capture traversal across discovery, marking, incoming counts and scan cost.
- [x] Add callable owning edges to all container/reference/object visitors on both ABIs.
- [x] Queue nonfinal callable releases and reclaim doomed descriptors with balanced child suppression.
- [x] Validate closure/arrow/value/ref/nested/bound receivers, retained closures and Fiber callback owners.
- [x] Rerun focused application/CLI/all-target checks and an independent full-work review.

The existing user-approved collector redesign covers this cleanup defect. Static
and foreign descriptors remain excluded by exact live-base validation. No delayed
destruction or request arena reset will mask closure cycles. This work is separate
from the eval literal-capture issue recorded on feat/simplexml.


Round-7 correction is implemented locally: canonical managed callable heap kind9,
initialized capture storage at all descriptor producers, capture owner enumeration,
tag10 edges in containers/references/proofs, nonfinal queueing and protected sweep.
The original native HTTP probe now keeps 69944 live bytes for every closure/arrow
request and destroys each preceding Holder. Native Fiber-owned callable/transfer/
pending-throw fields now share a C-ABI owner wrapper with eval children. Borrowed
visible start args and caller/frame pointers are excluded, matching final teardown.

Three CLI regressions pass (queue016827e9): twelve closure/capture shapes with dead
weak observers and clean heap, rooted callbacks plus throwing/resurrecting/mutating
destructors, and a saturated4100-capture exact incoming recount. A512-request HTTP
fixture also passes all twelve shapes, bounded memory, immediate destruction and
cleanup after a throwing destructor. A second retained-callable HTTP fixture hit a
worker SIGSEGV before responding. This is being diagnosed via owned reproduction;
no clean review or final acceptance rerun is claimed yet. Queue c4ad8ae1 runs GDB on
the exact retained-callable source; original core PID3459655 confirmed SEGV_MAPERR,
with38GiB available and no OOM events. Sources remain uncommitted and task active.


Round-7 correction validation is green. Queue c8c35283 passed both callable HTTP
regressions (including512 requests and throwing cleanup), direct/PIC assembly on all
five targets, guarded descriptor-reference cleanup on every target and pure native
slot counting. Queue3929bcf2 passed a warning-free build, all33 application tests,
three callable GC tests, five existing reference-capture ownership tests, three
incoming-cache/reference/saturation tests and eight exact legacy web regressions.
The latter cover worker/pool/request process trees, globals/statics/context/handler/
resource resets and pool recycle. Over2000requests, live PHP313976 and RSS19693568
bytes were unchanged. All66 changed/new emitter files pass comment alignment;
PHP fixture lint and git diff --check pass. Only Linux x86_64 executed locally;
other targets have assembly evidence and still depend on native CI execution.

The retained-callable crash is isolated to null invocation after clearing an untyped
property, with r12=0 in GDB at descriptor+56. It is task d48fb4f1. Explicitly typed
Closure replacement retains4096 through make/read/read and reclaims the captured
object after drop. A separate conditional reference-local issue is task1a6bda71;
reference PHP returns4096 in all12 original cases, and unrelated fixture cases now
use unique locals. Neither issue changes the verified callable graph correction.

Three additional cleanup/boundary regressions are queued as b459bd93, position1
behind another agent's emit-php-library build. Independent read-only round8 can
start on the validated frozen sources while those checks wait. No code or test edit
is planned during review. No commit, PR, push, installation or CI result is claimed.


Independent full-work Claude round8 started as dd08908b-fb45-41a0-954b-3ce6c7cbcdf0.
Supervisor3531513 and guest3531514 have verified read-only checkout/common-Git mounts.
The snapshot covers138changed/new files. Ready event7745 confirms the guest is reviewing.
Queueb459bd93 subsequently passed both exact throwing-destructor/reference cleanup
regressions and all-target object-child boundary coverage. No production, test or API
doc edits have occurred since the snapshot. Review verdict remains pending.


Round8 (dd08908b-fb45-41a0-954b-3ce6c7cbcdf0) concluded changes-requested with
three accepted findings: missing Generator/SPL native cycle edges, quadratic
reference-cell owner normalization and opaque callable capture proof slots.
Native probes reproduced all three classes. Generator cycles retain9008bytes
in CLI and grow across400HTTPrequests;3000reference closures plus100safe points
took2502ms versus18ms for value captures; closure-only services rescan120037work
units per request versus4for a direct property. Next changes extend the native
owner boundary and guarded teardown, reuse exact-base validation for constant-time
reference lookup, and revalidate typed logical capture slots. Add focused native,
HTTP and all-target regressions before independent round9. No findings declined.


Round8 corrections passed queue3e248d72: two CLI native-owner heap-debug tests
(nine Generator/SPL shapes, external roots, throwing/resurrecting cleanup),3000
by-reference closures with100automatic collections,400HTTP native cycles with
error recovery, and value/reference closure-only20001-node service proofs across
30requests, detach and replacement. Direct/PIC assembly passes on allfive targets
including reference-owner helpers, and the constant-work owner predicate unit passes.
Build is warning-free; all68changed/new emitters pass comment checks, PHP fixture
lint and diff hygiene pass. An initial fixture compilation tried dynamic dispatch
through a mixed property and required PCRE2 outside a managed project. Calling
the same generator through typed locals keeps the fixture focused on native owners.
No native package was installed. Queue7bf2e324 reruns the original raw probes and
all35application HTTP regressions plus directly affected existing ownership tests.
It is position1behind task10a9b8b7 validation. Round9 may review frozen sources;
only execution notes in this ledger and host scratch results may change.


Independent round9 is44f5607a-6f8f-4d66-9854-2a710d028565; snapshot143files.
Supervisor3806666and guest3806667 have verified read-only checkout/common-Git mounts.
Queue7bf2e324 started: original Generator CLI now2:dead:dead and clean heap;
400HTTPrequests have immediate prior destruction and constant RSS21540864bytes.
Reference-capture safe-point probe improved2502ms to14ms (value8ms). Remaining
probes and35application regressions are running; review verdict still pending.


Queue7bf2e324 passed all35application HTTP tests and directly affected existing
ownership tests:4callable GC,5reference-capture owners, borrowed elements/reference
returns, temporary receiver ownership, throwing object/array/hash/capture teardown
and all-target eval-object boundary checks. Original SPL now2:dead:dead with29
allocs/frees and clean heap. Closure-only service work is4units per steady request,
matching direct property. Over2000requests PHP live313976andRSS19521536bytes stay
unchanged. All142source/test/API-doc hashes remain frozen for round9. Verdict pending.


Round9 result44f5607a-6f8f-4d66-9854-2a710d028565 has two accepted findings.
Incomplete-object property hashes are now traced for class sentinel-2and payload
capacity>=32in allfour visitor modes, with opaque proof slots. Queue47d32931
passed a warning-free build, unknown/blocked class cycles, nested known/throwing
destructors with clean CLI heap,200HTTP cycles/error recovery, and allfive targets
direct/PIC assembly. The original CLI reproduction is now clean19allocs/frees.
Its explicit gc_collect_cycles result remains0because unset already collects.

The remaining P1 is coroutine cancellation and frame ownership. Probe B confirms
acyclic Generator/Fiber locals are abandoned when the suspended stack is unmapped:
CLI destructor0/0and8440livebytes, HTTP generator early-break grows4152bytes/request.
It is not corrected by tracing native slots. No finding was declined and the task
is not complete. Queue5f7355b8 is confirming original incomplete HTTP flatness and
a frame-local cycle.

Concrete proposal for the remaining runtime extension:
1. Introduce an internal forced-unwind marker that no PHP catch clause matches.
   On final release of a suspended coroutine, resume into that unwind so existing
   exception activations release locals and execute every finally before unmapping.
   The entry boundary absorbs only the marker; real exceptions remain protected
   cleanup failures. Never resume not-started or already terminated coroutines.
2. Reject new yield/suspend operations during forced close using PHP-compatible
   Error/FiberError behavior. Preserve existing global/handler/magic-guard state,
   and account for reentrant cleanup, nested coroutines and resurrection.
3. Run forced close for doomed coroutine nodes during the collector destructor
   phase, before final sweep. Pinning and a fresh-epoch recount must observe
   finally mutations, newly retained objects, exceptions and resurrection.
4. Generate a pure typed frame-owner enumerator from the same authoritative slot
   inventory and initialized-state predicates as normal/exceptional frame cleanup.
   Publish its callback with the activation metadata, preserving backtrace fields
   and updating every activation producer/consumer together. Avoid raw stack scans.
5. Walk saved suspended activations as real Fiber/Generator ownership edges, with
   count/mark/prime/pure-cost modes and opaque proofs. Exclude borrowed slots and
   dead/moved temporaries; normalize references through exact-base ownership checks.
6. Validate allfive targets together. Add CLI heap-debug and HTTP early-break
   memory tests, forced-finally execution, real errors, forbidden yield/suspend,
   retained and detached frame cycles, nested cleanup and resurrection. Re-run
   directly affected default web/CLI and preload behavior, then independent review.

This changes default CLI coroutine destruction toward PHP parity and extends the
activation/GC ABI beyond the already authorized collector patch. The review skill
requires the user's decision before that additional broad refactor. The concrete
proposal is complete; the incomplete-object fix is being finished independently.
No cancellation/frame-enumerator implementation has been started.


Final round9 evidence: original incomplete object HTTP live bytes are flat65600
through200requests. A separately compiled frame-array cycle confirms the missing
suspended-frame ownership edge:0:alive and4616livebytes versusPHP1:dead
(queuee74ec8cc). This makes both force-close and typed frame enumeration necessary.
The unrelated generator property-inference panic is tracked as taske0dea841.
The user has been asked through the conversation's input panel whether to approve
the additional full coroutine runtime extension. Dependent cancellation changes
remain unstarted pending the answer. Incomplete-object correction is validated,
all owned probe processes have stopped, and no build job remains active for this task.


The user replied "procedi" after the explicit full coroutine runtime proposal.
This authorizes force-close with PHP finally/error semantics, typed suspended-frame
ownership enumeration, all-target activation metadata changes and continued Claude
review. The prior decision wait is resolved. Implement and validate the complete
correction without requesting the same authorization again.


Coroutine extension implementation is in place. Normal PHP activations grow from
40to48bytes with a pure owner visitor at+40; compact transferred-owner guards
keep their32-byte self-data layout, and48-byte call-operand records publish the
same visitor contract. Generated visitors reuse cleanup inventories, borrowed/
moved exclusions, runtime reference-state predicates and zero initialization.
Each suspended coroutine walk owns an independent native slot-address set, so
relay guards do not double-count local owners and recursive walks do not interfere.
Raw reference-cell payloads normalize separately from managed cell nodes. Cost
mode counts activation steps and slots without tracing values or native allocation.

Native Fiber/Generator destructor-table entries now force-close suspended bodies
using an internal class-3marker. The entry boundary absorbs only that marker;
finally execution and existing exceptional frame cleanup release all owners.
Real errors propagate after protected cleanup, and markers are removed from PHP
previous-exception chains. Force-closed yield/suspend are guarded before mutation;
other Fiber starts and nested close retain PHP behavior. Existing
collector destructor pinning/recounts handle finally mutations before sweep.

Initial CLI testing exposed two integration gaps, both fixed: AST exception DCE
needed coroutine destruction to open implicit throws, and descriptor Fiber wrappers
needed a protected argument-box owner guard. Queue d9f8d026 passed three CLI heap-
debug tests with clean heaps, including real errors and private-marker behavior.
Queue6a26f3f3 passed1500-slot dedup/rehash/pure-cost, two HTTP tests (512temporary
requests plus retained suspended frames), native Generator/SPL cleanup regressions
and allfive targets direct/PIC. Queue909d08b1 passed new nested cleanup and public
Fiber switch checks; its shared-reference fixture hit a pre-existing unsupported
Mixed writeback shape, so that fixture now supplies an explicitly Mixed boundary.
The same ownership scenario remains covered. Queue401c281e is running a warning-
free build, five CLI cancellation cases, slot metadata, complete target assembly,
all38application HTTP tests and directly affected prior ownership regressions.
No clean independent review is claimed; round10 follows successful validation.


Final PHP oracle cross-check corrected an over-restrictive implementation assumption:
PHP8.5.12allows starting another Fiber from a force-closed Fiber's finally. Only
suspending the force-closed Fiber itself raises FiberError. The new public-switch
guard has been removed entirely; prior notes about forbidding public switches are
superseded by this verified behavior. The nested cleanup test now expects the other
Fiber to start and complete normally before the abandoned nested Fiber closes.
Allsix cancellation sources have reference PHP outputs recorded. A finally-rescue
regression confirms PHP's1:4096:alive|1:dead behavior; native verification follows.
Public native forced-close Error objects now acquire ordinary object handles.


Queue8094699e completed exit0: warning-free build; all six current CLI cancellation fixtures and their PHP reference outputs;1500-slot identity/rehash/pure-cost; complete EIR coroutine source assembled for allfive targets; allfive-target direct/PIC helper subgraphs; all38application HTTP tests; two exact optimizer reachability regressions; eight exact existing web reset/process-tree/pool recycle tests. The2000-request memory fixture kept313976livePHPbytes and19537920RSSbytes unchanged. All80edited/new emitters pass assembly-comment checks, portable PHP lint and git diff --check pass. Coroutine cancellation/frame findings19b5a615and19d4a9f0are superseded by verified discovery4fe80afb. Draft subsystem doc64d2fd2e is updated to revision14. No clean review is claimed. Preparing a full-work round10 freeze; sources/tests/API docs stay unchanged during that independent review.

Independent full-work Claude round10 is9a625303-66ba-4df1-af57-cb00f0ec389b. The frozen snapshot covers162files. Supervisor175807andguest175808both have verified read-only checkout/commonGit mounts. Ready event7889is acknowledged. Production/test/API doc hashes match the snapshot. Native validation job8094699efinished exit0before review. Only this execution ledger and host scratch may change until the result. No verdict yet.


Round10 completed9a625303-66ba-4df1-af57-cb00f0ec389b (result7923, acknowledged), changes-requested. All161production/test/API-doc hashes stayed frozen; only execution notes changed. Five accepted findings: P1unprotected owning yield-from temporary (61a133c8missing inner finally and672livebytes), P1warning activation32byte layout misread as48byte visitor (source confirmed;200/20depth probes passed,2000hit existing stack limit), P2cleanup_previous exposes private marker (6fbbe7b7dtor:leaked), P2fresh Fiber start boxing ownership (6e63a8e4/a550d38d4152bytes; HTTP5185c38965600to891848through200requests), P3forced-close suspend rejection omits monitoring unpark. No findings declined. Mixed-source UAF hypothesis did not reproduce in2df09a42; emitted EIR acquires the callback parameter, so use explicit EIR wire-box owners rather than changing borrowed start-buffer semantics. InstalledPHP also confirms local delegated aliases close after outer finally, while a fresh yield-from temporary closes before it. Host owns all corrections now that freeze ended. CodeJournal output feedback0adc7fadrequests rolling tails after capture-limit saturation. No current-turn work log or completion yet.


Round10 corrections are implemented using existing EIR call ownership, not new borrowed-start-buffer semantics: GeneratorYieldFrom operand roots and result staging; explicit source-evaluated Mixed boxing for Fiber start; complete48byte warning owner record; specialized compact error-handler saved-value/callable/initialized-result inventory; native zero-before-release; private marker filtering in cleanup_previous on both ABIs; existing monitoring unpark hook before rejected forced suspension. Queue22143f35 passed warning-free build and four new CLI heap-debug regressions, all clean. Two further native C fixtures poison the warning stack and install an actual unpark callback, while new HTTP coverage alternates delegation and fresh start arguments over512requests and retains argument data across requests. Queuee020ccf4 began after waiting behind Codex PR2124integration andPR2123docs; it now runs native6plus earlier cancellation6, all39application HTTP, allfive-target direct/PIC and exact raw before/after probes. Before-fix evidence is preserved in scratch/round10-before/. No clean review or completion is claimed.


Final round10 validation: e020ccf4passed five boundary cases including actual monitoring callback, then stopped on a C-fixture unused-but-set warning. Fixed by a volatile read and removed the exact failed fixture temp directory. c9888cb1passed that poisoned-stack native fixture, original cancellation6, allfive-target direct/PIC helpers, and38of39application HTTP. The expanded retained start-argument test encountered a pre-existing property/dynamic start wrapper missing its argument staging(c25862a1ArgumentCountError on/open, tracked c3991c29). It now starts through a typed local before storing the same Fiber, preserving cross-request ownership coverage. Queue4da2d44fpassed this remaining HTTP case, six boundary tests with IR-opt off, exact before/after CLI/HTTP probes, complete delegation/start EIR programs on allfive targets, five existing yield-from regressions and two default web resets. All39application HTTP tests have green evidence on unchanged production code.2000request live313976andRSS19648512stable. Fresh-start HTTP now65600at every request through200; delegate16allocs/freesclean; marker30/30cleanwithnoneprevious. All85emittercomments/diffchecks pass. Native Linuxx86_64only, other target execution remainsCI. Native array_map object control is explicitly unsupported by the existing checker and did not establish a new defect; PHP itself retains that cycle at GC. Preparing independent full-work round11, no clean review claimed.


Independent full-work round11started as5a593337-df0d-4a25-a18d-8e927a0482cd with170-file snapshot. Supervisor537487andguest537488both have verified read-only checkout/commonGit mounts. All production/test/API docs are frozen from this snapshot; only execution notes and host scratch may change. Five round10 findings accepted/corrected, no declined findings. Final verdict still pending, task/plan active, no current-turn work log yet.


User resumed with riprendi. Round11had completed5a593337-df0d-4a25-a18d-8e927a0482cd with one P2 code-read finding (result7957, process_exit7958after published result, both acknowledged): eval catch_types_match_thrown accepts Throwable unconditionally and can intercept the private force-close marker returned by compiled pause() called from eval. All other native/layout/owner fixes were independently reviewed as sound. The optional native probe7948was unanswered before that completion, so host is verifying now in queuedc0806c8a, waiting first behindCodex PR1386exactsourcevalidation. No duplicate review was launched; freeze ended only after explicit completed result was recovered. The existing values.object_is_a runtime hook already delegates to __rt_exception_matches on both ABIs and can reject the marker without adding an FFI ABI. Investigating routing Throwable through the ordinary class/interface matcher, retaining dynamic eval class handling. Native baseline output remains pending; no source edit yet. PHP reference additionally verifies a return in finally can override this unwind and allow parent tail execution, both compiled-call and eval cases; do not change finally override semantics.


Round11correction validated in340cfcdd: Throwable uses ordinary dynamic/native interface matching (existing AOTexception matcher rejectsprivateobject), no new FFI/predicate. Seven interpreter catch tests pass; original repro now finally/doneand20allocs/freesclean. Both newCLIevaltests pass with IRopt on/off, including static/instance calls, finallyreturnoverride, actual native/eval-declared exceptions. The latter exposed six224byte interface-copytemporaries in is_a helper; keys, values and array now release on normal/error paths (isolated dynamiccase84/84cleanfrom84/78).300HTTP cancellations keepboundedmemory andzero caught/tail counters. All40application cases have focusedgreen evidence acrosslatest feature paths. update-builtin-docs skillloaded;04a27308queued behindPR1386ownershipbuildandPR1862generatorvalidation for3metadata unitchecks, gen_builtins--featurescurl and allgenerateddocs/audits. Nextstepfreshfull-workClaude round12afterdocsverification. No cleanreview/completionclaimed, no currentturnlogyet.


Round11final verification complete:340cfcddpassed7interpreter exceptiontests,2newCLIeval/coroutine cases withIRopt on/off,300HTTP cancellations andall5isolatednativecatches/overrideswithcleanheaps.04a27308passed3metadata relationtests and the complete update-builtin-docs workflow(curl export, registry/Markdown/module/PHP comparison regeneration, builtin/site/EIR-boundary audits). No additionalgeneratedbuiltin-filedelta since round11;publicsignaturesunchanged. All85emittercomments/diffchecks andportablePHP lintpass. Preparingfreshindependentfull-workround12;allpriorfindingsaccepted,none declined.


Independentfull-workClaude round12is7fe5bf95-fa94-478b-b8db-005f8fe035aa with174-file snapshot. Supervisor980779andguest980783inheritverifiedread-onlycheckout/commonGitmounts. Allsource/test/APIdocs arefrozen;onlyexecutionnotes/hostscratchmaychange. Latest Magician matcher/helpercorrectionsfullyvalidatedincludingbuiltin-docsaudits. Noresultorcleanverdictclaimed.


## Final independent review, round 12

Delegation `7fe5bf95-fa94-478b-b8db-005f8fe035aa` completed with no actionable
findings and exactly `VERDICT: clean`. Checkpoint 8008 and result 8009 cover the
Magician catch/interface owner correction, forced coroutine close and private
marker, every activation publisher, typed frame/slot visitors on both ABIs,
descriptor argument guards, optimizer destruction effects, allocator guards,
application phases, server draining/bootstrap supervision, process signal context,
CLI and API documentation. No native probe was needed during this source review.
The six separate pre-existing tasks named by the reviewer remain unrelated; no
new causal link was found. Full result is retained by Code Journal delegation.
All production hashes were rechecked after completion. Reviewer processes exited.

Code Journal feedback `0adc7fad-95a0-41ec-8ec8-27d4a36aee04` records that local
delegation diagnostic capture stops advancing after its 2 MiB limit, even while
provider progress timestamps advance. The explicit protocol result remained usable.


Final hygiene: all 173 frozen production hashes matched after the review. No
reviewer or task probe process remained. Removed only the exact task-owned
`target/web-review6.IwgxTQ` directory (311 files, 345,147,331 bytes). The known failed
C-fixture directory is absent, the example contains only its three source files,
and `git diff --check` passes. No task-owned temporary artifacts remain. Code
Journal task d7cf8135 and plan 18edc617 are done; subsystem doc 64d2fd2e is current.
