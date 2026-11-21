# Async I/O operations and scoped builtin suspension

## Checklist

- [x] Record the maintainer-approved direction and its separation from scheduler v1.
- [x] Inspect the existing readiness, stream ownership and read-buffer boundaries.
- [ ] Reconcile existing stream close/alias and Mixed ownership blockers first.
- [ ] Define the internal operation/result/identity and lifecycle contracts.
- [ ] Define the trusted may-suspend boundary and scoped activation/deactivation.
- [ ] Specify logical blocking mode versus physical descriptor mode and alias handling.
- [ ] Add deterministic operation lifecycle and fake-queue tests before adapters.
- [ ] Adapt timers/readiness to the operation protocol without changing their v1 semantics.
- [ ] Implement native socket/pipe Read with operation-owned storage and PHP parity.
- [ ] Implement Write with retained input storage and partial-write semantics.
- [ ] Audit every composite helper and callable/eval route before enabling it.
- [ ] Prove cancellation, timeout, shutdown, stream contention and heap cleanup.
- [ ] Validate all supported target paths, documentation, examples and monitoring.
- [ ] Freeze an exact artifact and complete independent reviews before publication.

## Decision and phase boundary

On 2026-10-01 the maintainer accepted the direction discussed from bukka's design:
ordinary PHP I/O calls can become suspendable inside an Elephc Async scope, through
an internal operation/provider boundary. This is a future extension of scheduler v1,
not a claim that the existing implementation already intercepts blocking builtins.

Reference: <https://gist.github.com/bukka/87359261a4bfaa572ce43c93c0554b55>.
The referenced document is a design, with implementation gaps of its own; do not
treat its backend performance or coverage claims as Elephc acceptance evidence.

The approved direction changes the earlier **future design** restriction against
implicit builtin suspension. It does not retroactively change Candidate 62's
hashed specification, frozen historical review artifacts or shipped behavior.
`.plans/execution-scheduler.md` remains authoritative for that candidate. A future
candidate incorporating this work must explicitly replace the relevant blocking
honesty/scope clauses, receive a new hash and restart its complete review cycle.
Do not use two competing specifications to declare one candidate accepted.

No public `Io\\...` classes, userland provider registration, Ring/ior, second
executor, detached tasks or new Parallel semantics are approved by this step.
The remaining PHP-array/copy/ownership work and absolute-consensus publication
gate stay intact. This plan is not an alternative easier acceptance target.

## Inspected baseline

Worktree: `.claude/worktrees/hazy-watching-moler`; branch:
`spike/runtime-ctx-register`; Git HEAD at inspection:
`c26da5a06f49cdca27723ba4a9ba5d064cda79a2`, with inherited dirty changes.
HEAD alone does not identify the reviewed implementation; a complete source
artifact must be frozen for acceptance.

- `src/async_prelude/source.php:510-592`: explicit readiness waits retain the
  source, duplicate its descriptor, register task/deadline/interest, then suspend
  through a trusted scheduler-owned site. Readiness does not perform the syscall.
- `.plans/execution-scheduler.md:951-1054`: one reactor owner, portable ordered
  `poll()` adapter, monotonic deadlines, independent duplicate registrations;
  ordinary PHP calls are currently synchronous and never silently change flags.
- `src/codegen_support/runtime/io/async_poll.rs`: native fd/poll adapter; backend
  results are numeric readiness events, not PHP task execution.
- `src/codegen_support/runtime/io/stream_ownership.rs`: context-scoped descriptor
  ownership/clear/drain helpers. This is not yet an operation busy/lifetime lease.
- `src/codegen_support/runtime/io/fread.rs:18-157`: ARM64 read helper reserves
  concat scratch/heap storage before reading, handles TLS/filter/wrapper routes,
  and returns pointer/length. The x86_64 variant is in the same file.
- `fgets`, `stream_get_line`, `stream_get_contents`, `readfile_wrapper`, stream
  copy/passthrough and `fgetc` reuse `__rt_fread`. Inserting a yield there would
  affect composite callers too; their scratch state must not be assumed safe.

Source line numbers are inspection pointers, not perpetual freshness guarantees.
Before code edits, trace actual callers and verify snippets in the current tree.
The initial broad graph query included scratch review snapshots; only scoped
live-tree results and directly inspected live sources underpin this baseline.

## Layering

1. **PHP operation boundary** preserves parameter validation, source-order
   evaluation, partial results, EOF/error/timeout metadata and callable behavior.
2. **Internal I/O provider** binds the current Async task to one operation and
   converts queue outcomes into trusted scheduler wakeups.
3. **Operation queue** submits, cancels and settles operations using the existing
   readiness reactor initially. It does not run PHP callbacks itself.
4. **Platform backend** reports readiness or completion. Future completion-based
   backends must fit the same contract, not dictate the public PHP API.

The scheduler owns runnable-task policy. The provider owns I/O operation/wait
plumbing. One authoritative loop drives both; no nested Tokio/userland loop.
Parallel workers are not the implicit I/O pool: their isolated heaps and value
transfer contract are a different execution domain.

## Internal contract to implement

Operation kinds initially cover Timer, Poll, Read and Write. Completion kinds
distinguish Done, Ready, Timeout, Interrupted, Cancelled and Unsupported; native
failure data carries an error domain/code rather than overloading byte counts.
Timer expiry is successful timer completion, not a failed read deadline.
Names/layout/versioning are implementation details to review before wiring.

An operation carries a unique ID/generation, context/scope/task identity,
resource identity/generation, interest, absolute monotonic deadline, payload and
explicit owner/lease information. A descriptor integer alone is not identity:
close/reuse and aliases must not redirect a parked operation to another stream.

Ready never claims that data was read or written. Retry the nonblocking syscall;
EAGAIN can recur after readiness, including when another consumer won the race.
Done reports an actual completed syscall/result. Completions are delivered once;
stale/duplicate generation events cannot wake a new operation or task.

Runtime state must distinguish the logical caller outcome from whether native
code still holds pointers. Cancellation requests cancellation; it does not itself
prove physical settlement. Readiness-only registrations can settle by removing
their watch. A future kernel/worker data operation must retain its native-owned
record, resource/buffers until completion or confirmed cancellation settles it.
Shutdown drains these owners before releasing the context/heap. Never allow a
backend completion to write into a destroyed Fiber frame or freed PHP cell.

Use fake-queue tests to cover inline completion, both cancel/completion orders,
timeout/readiness ties, partial progress, duplicate notifications, orphaned callers
and shutdown. Do not copy a provider pseudocode example as lifetime proof.

## Activation and compatibility

No active provider: retain existing synchronous PHP behavior, with no per-call
queue allocation or offload. Merely linking the capability does not activate it.

Within a live Async scope, eligible ordinary calls may suspend through a trusted
internal boundary. `TaskGroup::spawn()` remains the only public task creation API.
User `Fiber::suspend()` remains unsupported as a scheduler wakeup. Determine a
single may-suspend predicate covering current task, live scope and unsafe native/
shutdown/reentrant regions. Do not infer safety merely from "a Fiber exists".
Root cancellation behavior must agree with the existing root-scope exemption.

Unsupported operations must be classified and visible, not disguised as async.
The initial active-provider policy is explicit refusal before a known unsupported
blocking call, with a documented diagnostic; outside the provider it remains
synchronous. A supported explicitly nonblocking PHP stream must still return
immediately on would-block rather than being silently converted to a blocking
logical read. Exact diagnostics and coverage are acceptance requirements.

Logical PHP blocking flags and physical nonblocking fds must have a reviewed
mapping. Setting O_NONBLOCK on a duplicate changes the shared open-file
description: descriptor duplication alone does not isolate flags. Do not promise
transparent default-blocking reads until alias/external-handle interactions,
restoration on every exit and metadata reporting are proved. Raw integer/FFI
handles and untracked aliases are not automatically safe managed streams.

Keep explicit v1 awaitReadable/awaitWritable semantics: they duplicate descriptors,
never change blocking mode and permit repeated independent readiness waits.
Data-operation contention rules must not silently change those readiness rules.

## Buffers and resource access

Read buffers and write inputs must remain valid across the full wait. Existing
shared concat/line scratch cannot be carried across a suspension just because it
belongs to the same `_rt_ctx`: other tasks execute in that context while parked.
Use operation-owned storage/retained immutable input with explicit result transfer
and normal/unwind release. Audit all composite helper accumulators before enabling
them. Do not globally insert a park into `__rt_fread` as the first implementation.

Introduce resource-aware busy leases: no concurrent close, seek, mode change or
conflicting data operation may invalidate an in-flight operation. Preserve PHP
object/resource alias identity and distinguish explicit close from dropping one
reference. Current Mixed/resource close findings must be reconciled first.
Start with a whole-resource data-operation lease and an explicit contention error;
full-duplex directional leases require a separate proven design, especially TLS.

Cancellation after a read has consumed bytes cannot pretend those bytes were
never consumed. Specify whether completed-but-undelivered data is committed to
the stream buffer, and preserve partial-write side effects. Do not conflate
cancelled caller delivery with backend rollback or lose data via a freed result.

## Delivery order and gates

**A — dependency closure:** stabilize the existing scheduler, Mixed copy/array
transport and affected stream close/ownership semantics. Preserve their exact
review/publication gates; this plan does not bypass them.

**B — protocol:** typed internal operation/results, fake queue lifecycle tests,
scope/context activation and trusted suspension rules. Reuse timers/readiness,
without changing public behavior. No new external dependency.

**C — first ordinary call:** managed native plain socket/pipe `fread`, after
buffer ownership, flag virtualization, contention and metadata rules are proved.
Prove inline success, would-block suspension and other-task progress, short reads,
EOF, timeouts, cancellation, close/reuse and heap cleanup. No TLS, filters or
wrappers implicitly opt in through their use of a shared helper.

**D — write/composition:** `fwrite`, partial writes/backpressure, then individually
audited composite consumers and callable/eval adapters. Signature/effects/
ownership/runtime targets remain authoritative in the shared builtin contract
and EIR path, not PHP-name string dispatch in backend emitters. Suspension and
external effects must not be optimized as pure or duplicated/reordered.

Regular-file offload, DNS, TLS, curl, database bridges, process waits, public
provider customization and Ring/ior remain future separately specified slices,
not a universal nonblocking promise. Cover them with explicit supported/unsupported
diagnostics until their own integration is accepted.

Executable evidence: macOS AArch64 and both Linux architectures. iOS device and
Simulator retain explicit analysis/host-contract refusal until a host-driven
scheduler contract is separately implemented. All five targets must have tests
for their intended path. Include optimization on/off, direct/namespaced/runtime
callables, monitoring wait attribution and dormant-path cost checks. Documentation
must state exactly which call/stream combinations suspend and which do not.

Independent Codex/Kimi/GLM/DeepSeek review must cover the same frozen specification
and complete code artifact; prior scheduler votes never transfer. Do not push or
announce completion from a narrow successful operation probe.

## Progress: closed boxed stream validation, 2026-10-01

The initial prerequisite fix is implemented in
`src/codegen/lower_inst/builtins/io/resource_handles.rs`: after unboxing resource
tag 9, reject a negative closed-resource payload before any descriptor table or
syscall access. The error uses the existing catchable TypeError unwinder on both
architectures. It does not change the negative identity sentinel, consume another
Array owner, or activate an I/O provider.

Three self-contained regressions in `closed_stream_resources.rs` cover read,
write through an alias and second close, including PHP 8.5.10 message and clean
heap assertions. The focused default-mode run passes 3/3; the pre-existing
descriptor/sparse groups pass 23/23 after this change. A no-IR-opt replay also
passes 3/3. The resource alias probe prints `same|written|closed` on both Linux
targets, artifacts `/tmp/elephc-linux-emitted.r4Bh0L` and
`/tmp/elephc-linux-emitted.HhdxCv`, compiler SHA-256
`b1da846778891e190711312729c883fbcf3bf1d9fcefa213ddfa2dda7b18c5d2`.
These Linux runs are functional evidence, not Linux heap-debug coverage.

The older probe's `still-writable` label meant only "no TypeError was thrown";
it never asserted that the post-close write actually wrote bytes. Do not infer
an OS descriptor stayed open or attribute the defect to resource refcount close
policy from that label alone. The confirmed cause here is missing validation of
the closed marker. Mixed copying, raw resource carriers, closed `is_resource`
results and dynamic properties remain open; the full dependency gate is not done.

The builtin documentation workflow passed exporter build (curl enabled), all
generation steps, audit (zero errors), site validation and target/EIR boundary
audit. A backup of inherited generated docs is
`/tmp/elephc-io-docs-before.vWOA9k`. Comparison shows only the pre-existing Async
run source-line pointer refreshed from 1134 to 1215 in one internals page and
the JSON registry; PHP pages and symbol registry are byte-identical. Assembly
comments and `git diff --check` pass. No review consensus or publication is claimed.

## Progress: Mixed value bindings and call snapshots, 2026-10-01

Semantic PHP stores now use explicit `MixedClone` rather than sharing a mutable
box; internal spills retain their original ownership operations. By-value Mixed
parameters receive private owning shadows, matched by
`FunctionSig::param_is_callee_owned`. Direct-call borrowed-return classification
respects that callee owner. References keep their binding identity while an
ordinary value assigned through one receives an independent cell.

By-value closure captures hold a private snapshot in a cleanup-tracked slot;
the descriptor retains it and optimized static calls use borrowed slot loads.
Global assignments also snapshot values. The initial extended matrix passed
15/15 with clean heaps, including globals, reference reads/writes and captures,
and the no-IR-opt replay passed 15/15. Binding/sparse/closed-stream regressions
passed 26/26 and selected Async/Parallel authority/fail-fast/alias checks 9/9 on
that checkpoint. Those results do not transfer past the subsequent changes.

New regressions then reproduced static-store sharing and argument evaluation
order: a later argument mutating the source changed an earlier argument's
value. Static stores now publish a copied value and release their intermediate
owner, returning a borrowed static load to assignment-expression consumers.
The positional argument path snapshots by-value Mixed values at evaluation.
Both focused regressions pass their behavior/heap checks, but the combined
post-snapshot checkpoint is **38/43**, not accepted:

- three closed-stream exception regressions leak one 40-byte owner;
- the resource identity/position/lifetime regression leaks one 40-byte owner;
- the closure parameter regression leaks three blocks/1168 bytes.

The additional named-argument source-order regression fails its unchanged
oracle: `{"x":2}|{"x":2}` instead of `{"x":2}|{"x":1}`. It remains red.
Builtin exception paths currently bypass normal owning-argument cleanup; some
normal builtin/closure paths also retain the new snapshots conservatively.
Do not remove those snapshots or weaken assertions merely to recover green:
complete their scoped lifetime/cleanup and apply value-copy policy through the
shared named/spread planner, excluding true by-reference arguments. Argument
evaluation exceptions must be covered too, not only the eventual callee call.

The 15-case results above are historical checkpoints. Current source is a
work-in-progress with the explicit failing gates above. All changes remain
unpublished; the full PHP-array/stream identity, target, protocol and review
requirements are intact. No ordinary I/O suspension has been activated.

## Progress: cleanup and planned named values, 2026-10-01

Registry builtin calls now install a cleanup handler when their operands own
temporaries, stabilizing non-void results before normal cleanup and releasing
arguments before rethrow on the exceptional edge. Direct static closure calls
release owning arguments using their actual parameter signature. Named arguments
use the same snapshot primitive as positional ones, taking parameter indices
and reference modes from `CallArgPlan` rather than rebuilding name matching;
scalar binding remains after source evaluation on this named path.

`RuntimeFnId::Fread` now declares Independent result storage: read bytes are in
concat reservations, never the resource argument cell. This resolves the normal
resource snapshot retention without changing physical read storage or introducing
a PHP-name dispatch rule. The six previously failing controls all pass, and the
combined binding/sparse/closed-resource/Mixed group completes **44/44** with the
unchanged heap assertions. Targeted rustfmt, diff checks and the regenerated
builtin audit/site/EIR boundary checks also pass on this checkpoint.

An additional self-contained regression,
`mixed_value_copies_argument_snapshot_is_released_when_later_argument_throws`,
then exposes a distinct leak: correct `caught|{"x":1}` output, but three live
blocks/1168 bytes. A later argument throws before the eventual callee cleanup
handler is installed. This test remains red and acceptance is unproven.
Next implementation must protect argument preparation itself: cleanup-tracked
snapshot slots, an evaluation-scope handler, zero-safe release of snapshots not
yet initialized, and transfer to ordinary call cleanup only after successful
preparation. Nested evaluations, reference operands, named/spread planning and
coercions that consume/replace a carrier must participate in the same ownership
ledger. Do not weaken the test or call the 44-case checkpoint complete scope.

No current-source Linux/iOS acceptance, independent review, new I/O protocol or
ordinary builtin suspension is claimed by these focused host results.

## Progress: evaluation ledgers and string binding, 2026-10-01

Argument preparation now installs a nested evaluation handler and transfers
Mixed snapshots through zero-initialized owned slots. Successful preparation
clears the slot without releasing its transferred SSA owner; exceptional
preparation releases initialized snapshots before rethrow. The later-argument
1168-byte leak and three additional zero/nested/named preparation controls
passed in a 48-test host checkpoint. Subsequent source changes invalidate
that checkpoint as current acceptance.

A new named coercion control revealed a separate optimizer defect: an unused
pure-body call with a Stringable argument was removed, and an observable call
lost its catch. PHP 8.5.10 prints `caught|payload` for the unused-call reproducer
in `scratchpad/c63_unused_stringable_binding.php`; the old compiler printed
only `payload`. Callable body effects omit parameter binding, and exact
body-only exception summaries bypass the general may-throw fallback.

The current draft adds a conservative entry-time string binding effect to
function/method summaries and open exception domains to those boundaries.
Closed-world `__toString` effects participate in the existing fixed point;
dynamic declaration contexts remain conservative. This deliberately sacrifices
argument-specific purity precision, not scalar-only callable analysis globally.
Closure binding, variadic conversion, return binding and exact argument-specific
proofs remain to audit. The first fresh 50-case run completed with **48 passed,
2 failed**: both new coercion controls now catch the exception and preserve
the original value, but each retains one 48-byte string allocation. Conversion
of the first named argument replaces its consumed Mixed snapshot with a new
owner; a later conversion throws before eventual call cleanup. The evaluation
ledger must also protect converted strings and other owning argument producers,
not just MixedClone snapshots. That follow-up is implemented and a fresh run
is pending; no heap-clean acceptance is claimed yet.

The first generalized producer transfer repaired both Stringable controls but
regressed four existing array/COW controls (**46/50 passed**). A concrete array
local load can satisfy `value_is_owning_temporary` because later storage widening
may require an owned unbox; that predicate is not proof that every current
load is a movable independent owner. Moving such a pointer into a new owned
slot can consume the caller's container. The correction is narrowed to owning
string producers and explicit converted owners; the array binding contract is
unchanged. The four regressions remain mandatory controls in the next run.

## Verified checkpoint: coercion cleanup, 2026-10-01

- Fresh Cargo codegen group: **50/50 passed**, default EIR optimization.
- The same fresh binary with `ELEPHC_IR_OPT=off`: **50/50 passed**.
- Focused Async spawn/root-unwind and actual Parallel worker round-trip:
  **10/10 passed**. This is not the entire scheduler acceptance suite.
- Both Stringable controls preserve `caught|payload` and pass the unchanged
  heap-clean assertion; all four array/COW regressions from the attempted
  generalized owner transfer pass again.
- Current compiler SHA-256:
  `10291797961cd5f2f23a47816321bbaefe296ca681244860cb863e527dd4913f`.
  Rebuilt runtime emitter SHA-256:
  `6f3ccd873430ef6fa193a6f2d45358e82790238e450fc22028d1c54aa74c3705`.
- Linux x86_64 and ARM64 execution of the unused-call reproducer both print
  `caught|payload`; complete input/assembly/bridge hashes are in
  `/tmp/elephc-linux-emitted.NbNd1C` and
  `/tmp/elephc-linux-emitted.RQlYnW`, respectively. These are functional
  emitted-assembly probes, not Linux heap-debug acceptance for all 50 tests.
- iOS device and simulator frontend/EIR staticlib assembly emission passes
  for that reproducer. Targeted assembler checks produce Mach-O objects with
  LC_BUILD_VERSION platform 2 (device) and 7 (simulator), in
  `/tmp/elephc-string-binding-ios.YqadLH`. The iOS SDKs are unavailable;
  clang reports the host sysroot mismatch. No iOS SDK link, hosted execution
  or full scheduler acceptance is claimed.
- Targeted rustfmt and `git diff --check` pass; the Candidate62 spec checksum
  remains unchanged. No commit, push, freeze or review consensus is claimed.

Still required: precise binding effects for closures/variadics/returns and
argument-specific worker-safety proofs (the conservative closed-world union
may over-reject a string-typed worker when an unrelated Stringable uses global
state), broader producer/array ownership and raw-resource/dynamic-property
controls, the full PHP-array ABI inventory, and then the scoped operation
provider/fake-queue protocol. This checkpoint only closes the two newly
reproduced direct-call Stringable failures and validates existing controls.

## Binding follow-up: worker entries and variadic elements, 2026-10-01

New exact-route probes establish four additional boundaries:

- `c63_string_worker_precision.php`: a first-class string-typed worker with
  the argument `'safe'` was rejected solely because an unrelated Stringable
  accesses PHP globals. A checked worker-entry projection now recomputes
  root bodies against full transitive callee summaries, excluding only their
  own incoming binding effects. The positive runtime control passes and the
  negative body-invokes-global-Stringable control remains rejected. The enum
  singleton negative control also passes. The projection must retain enum
  context and external callback effects; nested calls never use stripped
  binding summaries. Composite/dynamic entry targets remain conservative.
- `c63_binding_routes.php`: PHP prints `closure|variadic|method|`, whereas
  the old compiler prints `closure|method|`. Adding the variadic declaration
  to effect/exception summaries preserves the call but still omits actual
  element conversion. The first fresh group is **25/26**, with the new
  variadic test red despite clean heap output. Current lowering now consumes
  `FunctionSig::param_type_exprs` for declared elements before physical capture
  boxing and protects explicitly new partial collectors during preparation.
  Known Array/Hash local loads are not moved into that ledger. This lowering
  correction is under test, not accepted yet.
- `c63_immediate_binding.php`: immediate fixed and variadic closures both
  lose conversions when unused. Closure effects and exact throw summaries
  must include bindings; assigned-closure controls alone missed this path.
- `c63_return_binding.php`: PHP wraps a throwing Stringable return conversion
  in TypeError with the original RuntimeException as `previous`. The old
  compiler removes the unused call. Do not merely preserve RuntimeException
  or assert the parameter-binding oracle on a return-binding route.

The corresponding source changes require fresh full focused ownership controls,
both EIR modes, typed variadic/named/spread controls and target replay. No new
candidate acceptance or operation-provider completion follows from these probes.

The first variadic lowering/descriptor checkpoint completes **18/18**, including
the formerly failing direct variadic Stringable test, all sixteen descriptor
binding controls and the worker precision control. The standalone source route
now prints `closure|variadic|method|` with a clean heap summary. This is a
historical checkpoint: subsequent closure changes require fresh validation.

Closure invocation effects now consume the same fixed/variadic binding barrier
as named functions. Exact immediate-closure throw summaries keep the binding
domain open. Checked worker closure roots exclude their incoming binding only;
closure invocations inside their bodies keep the full conservative effects.
Direct static closure calls with owning arguments reuse the exception-safe
user-call cleanup instead of relying on unreachable post-call releases.
New regressions cover unused immediate fixed/variadic closures, a throwing
closure body with an owning Mixed argument, and a named partial collector whose
second Stringable conversion fails. The full focused group is running.

## Preparation checkpoints and source-order correction, 2026-10-01

The immediate-closure descriptor owner required its own cleanup boundary: it is
not a static call argument. The handler now covers argument preparation and the
body; successful heap results are stabilized and transferred with
`take_owned_temp` before descriptor release. The preparation-failure control
and an additional capture/argument/void normal-return control pass unchanged
heap assertions. The resulting **56/56** group passes both default and disabled
EIR optimization; ten Async/Parallel runtime controls and four worker-safety
negative controls also pass at that historical checkpoint.

Compiler SHA-256 for that checkpoint:
`7235919352b851839e7cc14e2e6bf5153708c88863237d66c2ab89c01cb0f58e`.
Runtime emitter SHA-256:
`18770f0776529ac69bc40985cfe2db20cf2a2a16cc2fbc1908001717c52d33ac`.
Current-source emitted-assembly execution prints `immediate|variadic|` and
`closure|variadic|method|` on both Linux architectures. Exact input, assembly,
bridge and executable hashes are retained in:

- x86_64 immediate: `/tmp/elephc-linux-emitted.ABNUZ0`;
- x86_64 binding routes: `/tmp/elephc-linux-emitted.2itLkK`;
- ARM64 immediate: `/tmp/elephc-linux-emitted.rzSFHU`;
- ARM64 binding routes: `/tmp/elephc-linux-emitted.2ysQb2`.

iOS device/simulator staticlib emission and explicit-target assembler checks
for the immediate source pass; objects declare platforms 2 and 7 in
`/tmp/elephc-binding-ios.tIQaNj`. SDKs remain unavailable and clang reports
the host-sysroot mismatch. No iOS link/hosted runtime or complete Linux
heap-debug acceptance is inferred.

Further source-order probes invalidate this as current acceptance:
`c63_binding_evaluation_order.php` produces PHP's `eval|bind|;eval|bind|`, while
the old compiler produces `bind|eval|;bind|eval|`. Regular and variadic sources
must all be evaluated before declared conversion, and regular slots bind before
the variadic tail. A distinct source-only argument helper now separates these
stages, leaving specialized builtin lowerers' already-bound operand contract
intact. Concrete refcounted arguments acquire independent temporary leases
rather than moving speculative array local loads. A typed-array COW control
and the source-order control both pass individually.

The first complete expanded group is **48/58**, exposing ten regressions, not
an accepted checkpoint. Persisting immortal data-section strings introduced
unnecessary owners and obscured literal provenance, including dynamic Phar
references without the corresponding link requirement. Such persistent strings
now remain immutable borrowed data; no optional libraries are force-linked to
hide this lowering defect. A seven-case replay then passes six controls, leaving
one 72-byte array lease in the static UserFunction call route. That route omitted
call-argument cleanup entirely; UserFunction/ExternFunction bindings now use the
ordinary lowering entry point. The latest full group is pending.

Two counterexamples remain independently open: `c63_variadic_array_binding.php`
must print `rejected`, but the cast-based direct binder prints `accepted` with
an Array-to-string warning; `c63_return_binding.php` must preserve the TypeError
and previous RuntimeException chain. A cast is not a complete declared-parameter
or return-binding contract. Reference cells, strict caller mode, specialized
builtin routes, the full PHP-array ABI inventory and operation-provider gates
remain required before freeze/review acceptance.

## Expanded host group and worker regression, 2026-10-01

Common static UserFunction/ExternFunction lowering and persistent-string
provenance repair restore the **58/58** host group in both EIR modes. The four
worker-safety negative controls also pass. The corresponding compiler hash is
`9c7381933e32a86b99110951e4dc1a8e4195faffe40cafca57eacee9497b2753`, emitter
`825add087a311ffcf0b0627a6001f713fe1635c0c53bfd032ba9cbdd415a6c61`.
Source-order execution prints `eval|bind|;eval|bind|` on Linux x86_64
(`/tmp/elephc-linux-emitted.6XSMjq`) and ARM64
(`/tmp/elephc-linux-emitted.BFhUNU`). iOS staticlib emission/assembly-only
artifacts are in `/tmp/elephc-order-ios.cS21Nw`; SDK/hosted execution gates
remain open as above.

However, the runtime scheduler replay is **9/10**, not acceptance: the Parallel
nested-array argument is read as missing (`worker|1||...`) instead of producing
`worker|42|ok|...`. The new snapshots select the owning-Mixed-cell conversion
path in `coerce_container_to_mixed_payload`. That path supplied the cell's
borrowed array payload directly to consuming ArrayToMixed/ensure_unique, then
released the cell itself. Owning the cell is not ownership of a separate payload
lease: those two consumers can consume the same reference twice.

Indexed storage conversion now explicitly unboxes/retains a payload reference
before ArrayToMixed for both borrowed and owning cells, then releases an owning
cell after conversion. That correction does not repair the worker counterexample:
the worker remains red in both EIR modes, while the two source-order/COW controls
pass. The audited ownership defect is distinct from the observed layout failure.

The minimal `c63_parallel_array_binding.php` source preserves the worker route.
`c63_parallel_array_diagnostics.php` receives the expected complete value when
the worker parameter stays Mixed, proving serialization/transfer content is
intact. Runtime `unserialize` materializes PHP arrays as Hash, including dense
numeric-key lists. Indexed storage widening cannot reinterpret that Hash header.
Deferring to the old ABI alone also fails because its Array branch does not
unbox a Mixed cell at all.

The current EIR parameter boundary uses MixedUnbox to acquire an owned raw
Array/Hash payload view without converting or renumbering it. Existing
`__rt_array_get_mixed_key` dispatches physical kinds 2/3 and can read this view;
the rest of the PHP-array consumer inventory remains explicitly unproved.
The worker replay is running against this change. No prior host/target result
transfers to this latest source change.

## Verified raw-view checkpoint, 2026-10-01

The raw Array/Hash payload view repairs the Parallel regression without
renumbering keys. Fresh current-source checks complete:

- **62/62** binding/ownership/closed-stream/sparse/nullable-array controls,
  with default EIR optimization and again with EIR optimization disabled;
- **10/10** focused Async spawn/root-unwind/actual Parallel worker controls;
- **4/4** worker isolation negative controls, including the Stringable body
  and enum singleton cases;
- Linux x86_64 and ARM64 minimal worker executions: `worker|42|ok`, with
  complete provenance in `/tmp/elephc-linux-emitted.V1EyAx` and
  `/tmp/elephc-linux-emitted.7PES6h`;
- targeted rustfmt and `git diff --check`; no staged changes or publication.

Compiler SHA-256:
`8aba2bffb92f93dddd9a2283ec86bfb8336749cc21497126a8307b5f4e223079`.
Runtime emitter SHA-256:
`0814da8a0f3b1ed3d0b1a625cdf5e44447c0198a755e07b2f50bbd32f0613957`.
The prior iOS emission artifacts are historical, not this checkpoint's full
SDK/link/hosted acceptance. Linux probes are functional, not the entire host
heap-debug group replayed on Linux.

This is not completion of the PHP-array ABI or the I/O provider. Direct string
parameter validation still accepts a boxed array through a cast; declared string
returns still need the correct TypeError/previous chain. Strict mode, reference
cells, specialized builtin preparation, all array consumer families and exact
independent-review consensus remain open. Do not freeze or push this checkpoint
as an accepted Candidate62 artifact.

## String-boundary follow-up, 2026-10-01 (not a frozen candidate)

Direct Mixed/Union string arguments now validate independently of explicit casts,
including call-site strict_types and declared variadic elements. Descriptor
validation happens at each binding position, not in a whole-list preflight:
earlier Stringable conversions remain observable before a later invalid value.
Declared exact-string returns reject invalid payloads and wrap an exception
from implicit Stringable conversion in TypeError with the original previous;
an explicit source cast preserves its original exception. Runtime false uses
the empty string. Return effects remain visible to optimizer/worker analysis.

The return regression exposed two additional ownership defects: gettype/get_class
return independent metadata strings, and boxed Throwable::getPrevious must move
its already-retained object lease into the Mixed box rather than retain again.
Both backend architectures have the getter fix. The generated builtin-docs
workflow, site compatibility and EIR architecture audit completed with no errors.

Before the latest method/closure edits, 71 focused controls passed in both EIR
modes. Linux x86_64/ARM64 probes confirmed return|TypeError|RuntimeException,
converted|rejected, invalid variadic-array rejection, and worker|42|ok. Those
artifacts use compiler 4e34dafe98515fc2b6dec7be8a1acddfd005a63bdd1e1da452c2f2ad8e710c2b;
they do not validate subsequent source edits.

The expanded finally regression passes. Separate method and closure return
regressions exposed two leaked blocks each; the closure repair makes regular
Mixed ABI boxes explicit in EIR and its regression passes. The method repair
also adds an exceptional argument/receiver cleanup edge and is being verified.
A broader Async selection exposed a public root-failure leak and obsolete
direct __TaskStart constructor tests. The latter now use existing factories
without changing heap assertions; current replay is required for all of them.

Still open: dynamic invocation strict_types policy, nullable/union/reference
return boundaries, all PHP-array consumers, specialized builtin preparation,
I/O queue/provider implementation, current full target evidence and independent
exact-artifact review consensus. No freeze, commit, push or completion claim.

## Verified string/throw checkpoint, 2026-10-01

The method/closure repairs pass unchanged heap assertions. The method repair
also closes the previously exposed public Async root-failure leak. Capsule
factory replays exposed an additional double release: Fiber is absent from the
intrinsic-class list used for dispatch scoping, but its private switching ABI
already consumes a transferred receiver on escape. Excluding Fiber from user
method boxing/cleanup restores exactly one consuming owner. The debugger located
the failure at the outer Fiber unset after the capsule had been released.

Latest-source evidence (these selections overlap; do not sum their counts):

- 74/74 binding/ownership/string/closed-stream/sparse/nullable-array controls,
  both default EIR and ELEPHC_IR_OPT=off;
- 22/22 string-boundary/root-failure/capsule/recorded-Fiber-failure controls;
- the prior 9/9 public spawn/Parallel/root controls were before only the final
  Fiber exclusion; the latest 22 selection rechecks public root failures;
- 10 functional Linux probes, five each on x86_64 and ARM64: return chain,
  descriptor binding order, invalid variadic array, Parallel array worker,
  and capsule failure through all releases. Linux heap-debug group not replayed;
- iOS device/simulator ordinary return-boundary assembly emits and assembles;
  object build platforms 2/7, no SDK/link/hosted acceptance (host sysroot warning);
- builtin-docs generation/audits, EIR architecture audit, targeted rustfmt,
  diff whitespace and unchanged Candidate62 spec checksum pass.

Compiler SHA-256: 3ef21eda8b31f97f8da96e0061bd94822436bb32e5ea11c7b665f17f5f146100.
Runtime emitter SHA-256: f874a8e76f87e4b414ad39d577736003bbb72e131116a87ebc5007d5ec687f51.
Linux x86 artifacts: /tmp/elephc-linux-emitted.{Cmhl08,j4J2KP,eVC8ZL,V7Gu29,sZOWuQ}.
Linux ARM artifacts: /tmp/elephc-linux-emitted.{ZNAIT5,JXwYkT,bfM7Yf,StpixD,jkorJj}.

New string guard and shared parameter validation leaves pass assembly-comment
checks. Checking the whole touched legacy invoker/Throwable files still reports
52 issues, including multiline instructions whose explanatory comments are on
the closing line and four misaligned comments; do not claim that gate closed.
The open boundaries listed above remain open. Kioku recall found relevant
checkpoints, but callers coerce_to_return_type used 100% CPU for six minutes
without output and was terminated; use targeted live source when this happens.

## Invocation strictness inventory, 2026-10-01 (implementation gate open)

Current oracle/reproducer: scratchpad/c65_strict_dynamic_binding.php declares
strict_types=1 and invokes a runtime-resolved string callback through
call_user_func_array with integer 42. PHP prints rejected; Elephc prints 42.
This is a demonstrated current defect, not a green strictness checkpoint.

Two additional regressions were reproduced and repaired. Static array_map
inlining interleaved effectful input sources with callbacks and inherited strict
string binding from the enclosing PHP source. It now only inlines safely
reorderable literal elements, like the existing reduce/walk restrictions. Other
inputs go through ordinary runtime array_map, which evaluates the complete
input before invoking weak internal callbacks. Both new regressions pass with
unchanged heap assertions. Broader current-source validation is in progress.

The descriptor invoker ABI currently takes exactly (descriptor, boxed argument
container); strict_php in an EIR callable immediate controls extension visibility,
not scalar strictness. Neither descriptor construction-time strictness nor an
ambient inherited strictness flag can represent PHP's physical invocation site.

Next implementation must carry an explicit invocation binding policy, distinct
from visibility. Required policy routes and proof points:

- Explicit direct/dynamic/call_user_func/call_user_func_array calls use the PHP
  invocation site's strict_types; changing the callee file must not change it.
- Internal callbacks such as array_map/reduce/filter/walk/sort, stream notification,
  PDO/cURL callbacks and Fiber entry must use their reviewed PHP internal-call
  policy rather than accidentally inherit a creator or caller profile.
- AOT/eval native invocation must preserve eval's existing binding phase and
  avoid a second scalar conversion or variadic packing. The public Rust function
  pointer alias NativeFunctionInvoker is currently a two-argument C ABI.
- Fixed/indexed/named/variadic/ref arguments, defaults, unions and nullable types
  need policy-aware validation. Strict float accepts integer widening; strict
  string does not invoke Stringable. Do not silently only enforce string types.
- The policy must not occupy an unspecified argument register, mutate a shared
  descriptor, alter boxed-cell/array payload bits, or survive as ambient state
  across nested calls, throws, Fiber switches or Parallel context isolation.
- All supported targets and every producer/consumer must migrate together;
  version/cache/host registration compatibility must be explicit and tested.

Live inventory of descriptor invoker-slot loaders / native aliases names 15
files: codegen/lower_inst/callables.rs; codegen_support/callable_descriptor.rs;
wrappers/callback/descriptor.rs; wrappers/fiber.rs; runtime/curl/callbacks.rs;
runtime/pdo/pdo_call_{collation,agg_final,agg_step,scalar}.rs;
runtime/io/notification.rs; and Magician context/{reference_metadata,native_function}.rs
plus ffi/native_functions{.rs,/registration.rs,/public_abi.rs}.
Also audit invoker emitters, native_execution, tests and external ABI consumers;
this textual inventory is a starting point, not exhaustive graph certification.

## Verified internal-callback checkpoint, 2026-10-01

The array_map source-order/internal-weak repair is verified on the latest source:
101/101 default-EIR controls including existing array_map consumers, and 76/76
binding/ownership controls with EIR optimization disabled. The two new regressions
preserve heap-clean assertions. Selections overlap and must not be added together.
The current compiler is 945d9540cef02f863c6459a12be2d3d8da11eae4a1b1f3830c6b86671ae91349;
runtime emitter is 6d687e89b7cfa1f432473c49ff7d9116f755af0d94c40940e6576e0c40fd2930.
Linux x86_64 and ARM64 replay the PHP-oracle output
42|1|source1|source2|callback1|callback2|12, with artifacts respectively
/tmp/elephc-linux-emitted.VGEe5e and /tmp/elephc-linux-emitted.V4unQi.
Ordinary iOS device/simulator assembly also emits and assembles, with the known
host-sysroot warning; SDK linking/hosted execution and Linux heap suite remain
unverified. Targeted rustfmt and diff whitespace checks pass. No assembly emitter
or descriptor ABI was changed in this repair.

The explicit dynamic strict-call reproducer remains red (PHP rejected / Elephc42).
Next work is the complete explicit binding-policy ABI migration and its consumers,
not descriptor construction-time profile capture or string-only validation.
Array ABI, I/O provider and absolute exact-artifact review gates remain open.
