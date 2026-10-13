# Scheduler branch integration with origin/main

## Checklist

- [x] Publish WIP checkpoint ae2c6bd and verified null-default follow-up 516dea0b36.
- [x] Fetch origin/main and record exact integration parents.
- [x] Preserve inherited vendor deletions in a targeted recoverable stash.
- [x] Start an ordinary no-commit merge without rewriting published history.
- [x] Reconcile non-generated conflicts, preserving scheduler/ownership/ABI work
      and upstream behavior/docblocks rather than choosing one side wholesale.
- [x] Regenerate builtin documentation from the reconciled contracts and bindings.
- [ ] Compile and run focused scheduler, binding/ownership, native/eval and target
      controls on the integrated source; previous-parent checks are historical.
- [x] Commit and push the validated WIP merge, then verify PR #958's exact head/CI state.
- [x] Restore the inherited local vendor state without including it in the merge.

## Exact state and recovery

The maintainer requested merge/rebase of origin/main on 2026-10-09. An ordinary
merge was chosen because the branch is published and a force-push is unnecessary.
Our pre-merge parent is 516dea0b36 (full ID available from HEAD); the incoming
parent is afe32d3efe27209b87eb68d4b2adcda8dbc5eefd. There were 55 branch-only and
1,719 incoming-only commits before integration. PR #958 is draft and currently
conflicting; no CI runtime success is inferred from its label classifier.

The seven inherited example-vendor deletions are stored in stash
46b10c28d5f6c08e10fa3fb8a78f317dde039fdc, named
scheduler-main-merge-516dea0b36-20261009-inherited-vendor-only. Apply by exact SHA,
not by a moving stash index, after the merge is committed. Keep this recovery
entry until restoration is verified. The local .codex/, .kioku/, .mcp.json and
scratchpad/ remain outside publication; incoming main tracks none of those paths.

The initial merge reported 962 unresolved paths: 840 generated builtin pages
and registry artifacts, plus 122 other paths. Generated files may use incoming
main as a provisional baseline but MUST be regenerated from reconciled source
before acceptance. The other conflicts include build/package wiring, native/eval
ABI, codegen/runtime, EIR lowering, checker/effects, test infrastructure and manual
docs. This is an integration task, not a request to discard either implementation.

The incoming guide adds mandatory Rust function docblocks and pinned third-party
GitHub Actions; preserve these while carrying scheduler-specific behavior. Do not
change Cargo release versions or changelog content during ordinary development.
The pre-merge 83/24/19 focused results and Linux/iOS probes do not prove this merge.

## Initial resolutions

Generated builtin pages/registries now use the incoming baseline provisionally,
including deletion of obsolete web/trigger_error pages; regenerate after source
reconciliation. Build/package lists retain both upstream mbstring and branch
Parallel contracts/bridge. Third-party action pins from main remain intact.

ABI collision: incoming main already uses invoker argument 3 for lexical scope
(i64, -1 global), while the scheduler branch used argument 3 for scalar strictness
(u64, 0 weak / 1 strict). The integrated ABI must retain both: descriptor,
boxed arguments, lexical scope, then binding policy. Magician announces version 4
and its prebound native-call boundary passes (-1, 0). Emitted AOT/internals,
wrappers, Fiber, curl/PDO/notification and eval producers must all migrate to the
fourth policy argument before this merge can build or be accepted. Native test
stubs are updated to the composed four-argument signature, not one side's ABI.

Runtime feature-bit collision: main assigns bits 12-15 to handler state, object
clone, mbregex and MIME support. Preserve those incoming assignments; the
integrated ctx-register, Async reactor and Parallel bits append at 16-18. Keep
ctx-register active in RuntimeFeatures::none(), since this branch's codegen
requires it. Old mb_strlen detection must move to main's mbstring family, not
drop its new bridge/provider behavior. Runtime cache tests need the composed map.

Incoming main also introduces mutable mbstring runtime cells absent from the
current ctx layout: catalog array, deferred capture head/tail/draining and native
INI activation. They must be routed/isolated or safely refused for workers before
Parallel acceptance; do not retain raw process-global heap pointers by merely
choosing main's .comm definitions. Preserve immutable diagnostic data separately.

After this first pass, 105 paths remain unmerged. The read-only Rustdoc classifier
found no remaining file whose incoming delta is purely Rustdoc, so whole-file
ours selection is not justified. The generated baseline is provisional only;
840 generated conflict resolutions are not source/behavior validation. No merge
commit or merge push has been made. HEAD remains the published follow-up
516dea0b36250ec9cc83e09040cbe296a3defb57 and MERGE_HEAD is the incoming main SHA.

## Integrated source checkpoint (2026-10-09)

All Git conflict stages are now reconciled, including three modify/delete cases that
the stage-3-only inventory did not list. The obsolete array_filter,
array_filter_refcounted and array_find_any_all emitters were removed after verifying
that incoming main's boxed predicate helpers own their production routes. They are
recoverable from the pre-merge parents. A residual marker in array_udiff_uintersect
was found by Cargo and removed; its shared boxed helper retains lexical scope and
now explicitly forwards weak binding policy to the owned-argument boundary.

The integrated library passes cargo check --lib. This followed a real compiler
check reporting 33 API/structure errors, then 5, then zero. The remaining three
warnings identify an obsolete static-closure lifetime helper superseded by main's
prepublished result and operand-owner machinery. No executable scheduler, binding,
ownership or target result is implied by a successful library check.

Changes carried through the merge:

- Descriptor invokers and their consuming caller boundaries independently spill
  scope and binding policy. Magician uses ABI 4; prebound eval arguments are not
  coerced a second time. Every AOT producer still needs executable validation.
- Invoker argument owners, managed reference cells, resolved defaults, PHP return
  ownership status and backtrace state use incoming main's mechanisms. Parallel
  Fiber-suspend guards and physical parameter-order validation remain enabled.
- x86_64 indexed invokers reload argc without borrowing r14; associative argc uses
  rbx. The eval builtin dispatcher spills its eval context while r15 retains
  result-out. Other automatically merged r14 writes remain an audit requirement.
- Internal Fiber/curl/PDO/notification/error/exception callbacks explicitly supply
  global lexical scope and weak binding. Array callback helpers preserve their
  caller scope and explicitly supply weak policy to the composed ABI.
- Descriptor result unboxing retains main's ownership checks and scalar/nullable
  cleanup while preserving direct transfers for owned strings and heap payloads.
  Mixed-to-string conversion consumes the explicit owned-return status rather
  than persisting a Stringable result twice.
- C lifecycle/free entries were ported into main's new lifecycle leaf. They save,
  publish and restore the host context register; repeated init preserves a live
  arena and the incoming startup/mbstring request initialization contract.
- EIR calls use main's source-evaluation roots, result staging and reference-return
  lifetimes instead of legacy run()/method cleanup handlers. Branch snapshot,
  scalar-binding, worker-safety and callback guards are retained. Variadic inferred
  return-carrier normalization is shared with metadata through dynamic_params.
- GC recounting keeps the worker arena base, property GC metadata uses main's
  managed-cell representation, and bounded concat/implode/inet_pton and ASCII
  origin-tracking paths were retained. Runtime feature tests and Linux build
  scripts combine Async/Parallel with the mbstring family, without the removed
  mb_strlen feature field.

Current integrated checks:

- elephc-builtin-contract library tests: 52/52, both default and curl feature.
- Magician native_scope tests: 27/27; ffi native-function registration: 3/3.
- Three isolated extraction tests (PHP control-byte defaults, neutral unions,
  namespaced prelude declarations) pass. Four edited Python files parse; both
  edited Linux shell scripts pass bash -n.
- The abi::tests Magician filter matched ZERO tests and is not validation evidence.
- Targeted diff hygiene passes. Local rustfmt checks are not compile/test evidence
  because the repository disables formatting.

Open integration gates before commit/push:

1. Compile and execute the affected AOT/invoker/Async/Parallel/GC regressions, with
   current supported-target evidence. New emitter tests are not yet executed.
2. Route or safely isolate incoming mutable mbstring, handler, diagnostic, GC,
   resource-inventory and magic-set state; review direct heap-base/r14 uses. Merely
   retaining their main .comm baseline is not safe worker isolation.
3. Verify Fiber boxed arguments on ABI-v4 escape, generator last-key slot sharing,
   pending cleanup exception chains, result retirement and descriptor release.
4. Audit typed by-reference variadic preservation and source-evaluation vs binding
   order across the composed planner/owner mechanisms. Focused historical tests
   do not settle those full ABI contracts.
5. Regenerate builtin docs and registries, run their full extraction/audit/boundary
   gates, and complete Rustdoc/assembly-comment hygiene on the integrated code.
6. Commit/push the validated merge normally, restore stash
   46b10c28d5f6c08e10fa3fb8a78f317dde039fdc by SHA after the merge commit, then verify
   PR #958 head/CI. Full array/I/O implementation and exact-artifact independent
   Codex/Kimi/GLM/DeepSeek consensus remain separate, open acceptance gates.

Kioku acknowledged decision-scheduler-main-integration-composes-lexical-scop-416b820a
after a long CLI capture. Its fact records the earlier first-pass state (105
conflicts), not the current result. Reverify its source after this ledger update;
do not promote that historical decision into an integrated measurement packet.

### Focused invoker test compilation

The root library test build found one remaining synthetic enum-transcriber pattern
that omitted main's `generics` field. It now explicitly refuses unsupported generic
enum transcription rather than dropping that metadata. The obsolete
static_closure_lifetime module/file was removed after confirming it had no remaining
caller; main's result/descriptor roots supersede it. Both it and the earlier three
array helper removals are recoverable from the published pre-merge parent.

The first executable emitter-test run was 26/28. The two failures expected legacy
global _concat_off/_exc_call_frame_top names despite correct context-relative stores
in the emitted assembly. Assertions now check the exact ctx fields and pre/post-call
store placement, not merely a comment or symbol name. The targeted root-package
rerun is still pending at this log point. Future root unit runs must use `-p elephc`
as well as a filter: omitting the package rebuilds unrelated workspace test crates.

That rerun subsequently completed: 28/28 invoker tests pass, including the composed
scope/policy frame, defaults, managed reference owners and all-five-target emitter
controls. The root library test build is warning-free after retiring the obsolete
closure helper. This still does not execute generated PHP programs or prove worker
runtime isolation, I/O completion, target linking or model consensus.

The semantic Kioku recall for scheduler/main/ABI4/context was launched once via the
local CLI and has not yet returned. Do not claim a semantic hit or a successful
integrated measurement capture from a live, empty process.

Four additional focused all-target emitter controls pass: descriptor payload-owner
transfer, per-tag Mixed/string owned-return handling, shared variadic return ABI,
and the consuming argument boundary's independent scope/policy spills. The new C
lifecycle ordering test first failed on Linux AArch64 because its expected publish
sequence ignored ELF PIC/GOT materialization. Its assertion now uses the exact
target-aware publication sequence while still requiring save-before-publish and
restore-before-return for init/shutdown/free. That rerun is pending at this point.

### Request-state isolation and worker teardown (2026-10-09)

The C lifecycle ordering rerun subsequently passed its all-five-target emitter
control. Semantic Kioku recall also completed, returning three relevant packets
with explicit warnings: moved citations/context not reverified, and an expired
Candidate62 measurement. They are historical context, not current integration
acceptance. The CLI exposes a code graph as well as verified memory; prefer that
surface for subsequent discovery when the MCP tools are absent.

Forty-five incoming request cells now have distinct context offsets, defaults,
symbol routing and owner verification. The two initial root tests pass (layout
and initialization/verification on all five emitter targets). Initialization
preserves the pool's state pointer and uses an immutable default reporting mask.
These tests do not execute workers or prove that every consumer is routed.

The current cleanup additions are implemented but their rerun is pending:

- Handler pop drains preceding registrations through the bounded exception
  preservation boundary; feature gates match diagnostic helper emission.
- Native mbstring reset still runs after a caught deferred PHP cleanup throw.
- Stream, Buffer, cycle and inventory cleanup preserve accumulated exceptions
  rather than abandoning sibling cleanup on the first destructor throw.
- Stream teardown marks a native inventory incarnation closed before closing
  its descriptor. Otherwise inventory cleanup could close the same descriptor
  twice, potentially closing a new descriptor reused by another worker.
- Worker inventory draining no longer invokes request-cursor reset. The existing
  resource-id registry is intentionally process-wide and lock-protected; resetting
  its cursor during one worker's cleanup would interfere with other workers.
- The C pool fixture accounts for the inserted 360-byte block, rendezvous-tests
  all 45 new cells with two threads, restores their defaults and arena base, and
  passes/verifies the fifth worker-entry release-status argument.
- Its fatal stream probe allocates a real inventory node, observes incarnation
  retirement before the TLS/native close hook, and checks the global cursor is
  unchanged. This executable probe has not run on the integrated source yet.

Final owner verification still quarantines live owners and pending Throwables.
Later destructors can republish owners; never erase those pointers merely to
make a context reusable. Direct-symbol/register audits, actual native/e2e probes,
all supported targets, docs regeneration and exact-artifact consensus remain open.
No merge commit or merge push has been made; the inherited stash is not restored.

The expanded root context run completed 22/26. All three new request-state tests
and the ordered cleanup/verification control passed. Four broader controls failed:
reserved-register scratch on both architectures, kitchen-sink legacy-state access,
and default-arena materialization count. The x86 findings were real: new hash
reference markers, automatic-key overflow handling and exception-chain traversal
borrowed r14. The write-through return also loaded an r15 spill into r14. Corrected
paths use preserved r15/rbx or the declared r11 scratch. GC status/destructor scans,
managed reference-cell lookup and heap-debug reports now load the active context's
arena/extent instead of raw main-arena symbols.

The ARM findings were paired context saves/restores in worker release and native
callback frames, not scratches. The audit is extracted into a cohesive test leaf:
frame reloads require a matching context save in the same global helper. Negative
controls reject another register's frame slot, unmatched ARM pairs, cross-helper
reloads and a read-through-r14 instruction that overwrites r14 itself. This also
closes the old x86 audit's overly broad allowance for any memory load into r14.
The corrected expanded rerun is active; do not call these four gates green yet.

Kioku usage feedback, observed under concurrent build load: semantic recall returned
useful stale/moved-citation warnings. Code-graph callers reported no references to
per_context_symbol_offset although ABI source has qualified ctx:: calls; this is
insufficient discovery evidence, not proof of no consumers. A learn capture spent
over 21 minutes in its duplicate guard's recall/citation-relocation pipeline, with
an rg scan for an old PHP TaskGroup::spawn signature. Its own process group was
verified and terminated (exit 143), with no matching packet written. Retrying this
one capture uses a precise source range and KIOKU_FORCE=1 to bypass duplicate lookup;
no Kioku source/configuration or other worktree process was changed.

The retry acknowledged decision-integrated-worker-teardown-preserves-request-own-f789481f,
pinned to lines 214-236 rather than the entire ledger. The packet was inspected;
it explicitly leaves native and full target/model acceptance open.

The expanded context gate subsequently passed **30/30**, after routing incoming
serializer/key-conversion concat ABI accesses, migrating GC cache-bin addressing,
and recognizing Linux AArch64's exact :lo12: main-context publication relocation.
The legacy-state scanner now checks exact symbol tokens regardless of the address
register or relocation form; negative controls cover x19 and GOT cases that the
old x9/RIP-only check missed. The all-feature reserved-register audit passes all
five emitter targets. This is not native execution or all-five-target linking.

Against that built unit artifact, focused controls passed: stream ownership 2/2,
heap-debug live reports 1/1, boxed invoker-marker write-through 1/1, PIC ABI 7/7,
and the broader invoker filter 40/40 (overlaps the marker control; do not sum these
as unique tests). Inventory controls first passed 3/5: two assertions assumed legacy
symbol names or the wrong helper emission order. They now require actual ctx head/
tail addresses and delimit drain before insert on both architectures; rerun active.

The C probe passes host syntax checking. Its wrapper is named run_worker_entry to
avoid ELF collision/recursion with the runtime's __rt_parallel_worker_entry label;
invalid-input testing calls the raw five-argument entry and checks that a rejected
call does not alter its release-status sentinel. Native execution is still pending.

Additional inspection point before any full-isolation claim: the Fiber entry leaf
still publishes the main _rt_ctx after a zeroed fake frame. Determine whether worker
Fiber creation is safely diagnosed or must inherit the worker context in its initial
frame; reproduce before treating this as a closed or confirmed executable defect.
The builtin-docs skill has been read for the required regeneration gates; exporter
build/render/audits have not been rerun on this integrated source yet.

The inventory rerun is now **5/5**. All changed-helper controls are green at the
unit/emitter level; the actual staticlib/C context-pool test has been launched
with the exact test name on the macOS host. Its Cargo process must be polled to
terminal before any other heavyweight build or claim of native success. Source
compilation, generated PHP, C host link and execution are separate possible failure
stages; a live Cargo job is not a passing native result.

### Native pool execution and Fiber follow-up

The first native pool execution failed INVENTORY-INCARNATION-NOT-RETIRED. Added
failure-only C snapshots showed registration was correct (id 99, fd 3, kind 1),
but none of the native close hooks ran. The earlier simple worker probe had already
consumed/released its context under the composed five-argument entry ABI. Reusing
that pointer without a fresh acquire made release a no-op. The fixture now reacquires
after the simple worker, and production worker entry rejects a zero acquisition
back-pointer before publishing the ctx register, on both architectures. A native
negative control checks that rejected reuse leaves release-status output untouched.

After those changes the exact staticlib/C pool test passes on macOS AArch64 (1/1).
Its C compiler/linker and execution succeeded, including native fd hook retirement,
global resource cursor preservation, two-thread marker isolation, pool racing,
dirty-slot refusal and released-entry rejection. This is host-native pool evidence,
not executable Linux/iOS, complete PHP-array/I/O behavior or model consensus.

Fiber inspection confirms both constructors zero the initial saved-register frame,
then Fiber entry explicitly republishes main _rt_ctx. Generators reuse the same
constructor through codegen/block_emit.rs. A new heap-debug PHP regression creates
a Fiber inside a Parallel worker and changes its GC state; the parent must still
print 42|main-enabled. It has been added but not executed: the first test-build
command used a bare --exact name, whereas its real name is
codegen::parallel_execution::parallel_worker_local_fiber_inherits_worker_runtime_context.
Poll that build to terminal; a zero-match result is not evidence and must be followed
by the fully qualified exact name. Do not restart a still-live build.

That first codegen-test build terminated with a real fixture compile error, before
test selection: compile_and_run_with_heap_debug returns ProgramOutput, not String.
The new regression now checks success, stdout and the clean heap-debug summary,
matching existing Parallel controls. It is relaunched with the fully qualified
exact name; its current Cargo process is live. No Fiber runtime change has been
implemented and no successful Fiber-worker execution is claimed at this checkpoint.

The fully qualified Fiber regression then compiled and ran exactly one test, which
crashed on the elephc-parallel-1 thread. LLDB reproduced EXC_BAD_ACCESS at address 8
in __rt_fiber_entry+72: x19 (current Fiber) was zero, while x28 pointed at main rt_ctx.
The entry's first instructions explicitly materialized that main pointer. This is
a confirmed executable isolation defect, not merely a suspicious emitter shape.

The common constructor now seeds the creator's context in the actual switch-save
slot (AArch64 x28 at 72; SysV r14 at 8), after zeroing and before installing entry.
Entry no longer republishes main. Both constructors are updated; Generators reuse
the same path. An all-five-target emitter control checks seeding/order/slot bounds,
and the reserved-register audit has negative controls for main publication in entry.
The corrected Fiber regression rerun is active. A second heap-debug Generator
worker regression was added; neither new executable result is yet claimed green.

The corrected exact Fiber regression subsequently passed **1/1** on macOS AArch64,
including 42|main-enabled stdout and the clean heap-debug assertion. The Generator
exact control is now launched separately; its result must match one executed test.
The all-target constructor/switch unit controls and post-fix Linux/ordinary Fiber
controls still need execution. Kioku acknowledged the earlier native pool measurement
as measurement-integrated-staticlib-context-pool-passes-native--dcd8f70d, with a
20-commit freshness budget and explicit remaining gates.

The exact heap-debug Generator worker regression also passed **1/1** on macOS
AArch64, preserving main GC state and clean ownership. The target-wide coroutine
constructor/switch and context-register unit gates are now running serially.
The EIR architecture inventory passed: 741 registry AOT builtins, zero structural
errors and zero backend-dependent homes. This audit may silently run Cargo via
the exporter freshness fallback; treat it as a possible heavyweight build, not a
pure Python-only operation, and serialize it with test/bridge builds.

Linux preflight was verified broken on Docker Desktop: the daemon reports
/var/lib/docker, but host df cannot access that guest path. Both runner scripts now
measure free space through an ephemeral --rm container using the prepared test
image, before creating the Cargo target volume. The shared helper fails closed on
Docker failure or malformed measurements. Four mocked controls (valid, zero,
malformed, failed command) pass, all four shell files pass bash -n, and the actual
ARM64-image probe reports 21,801,072 KiB free (about 20.8 GiB) at this checkpoint.
No Docker image, unrelated container or volume was pruned. This is tight relative
to the 20-GiB runtime disk guard, so do not infer enough room for both target builds
or disable the guard to manufacture target evidence.

Post-coroutine-fix unit reruns completed: constructor/switch **2/2**, including
creator-context seeding on all five targets; context/register/storage controls
**30/30**, including the new negative entry-publication controls. Both focused
native coroutine regressions are green, but executable Linux coverage is not yet
proved. The full generated-docs workflow is now launched serially: canonical curl
exporter build, render/registries/module/comparison generation, docs/site audits,
EIR target-architecture audit and diff hygiene. Stop at its first actual failure;
do not treat the previously reconciled generated baseline as already regenerated.

The full canonical docs workflow subsequently completed with exit 0. It exports
1,185 builtins, 209 classes and 1,117 constants, renders 2,254 pages, updates zero
module blocks and regenerates the PHP comparison. Docs audit reports zero errors;
site compatibility validates 2,276 generated pages; EIR architecture inventory
again reports zero structural errors and zero backend-dependent homes. Whole-tree
unstaged diff hygiene passes. Inspection found 1,041 generated paths changed from
the reconciled incoming baseline (1,070 inserted and 1,070 removed lines), rather
than treating the original 840 generated conflict resolutions as sufficient.

Kioku acknowledged the coroutine measurement as
measurement-worker-local-fiber-and-generator-context-regress-b5f0f8f5. No Docker
volume matching this worktree's runner hash 8067abf45272ac3f exists, so no owned
target cache was assumed reusable or deleted to increase disk space. Linux native
validation and touched-Rust/assembly hygiene remain separate gates before publishing
the merge checkpoint; full array/I/O and absolute exact-artifact reviews remain open.

### Native Linux context-pool probes without Cargo target volumes

The explicit ignored fixture emitter emit_linux_context_pool_probe_bundles ran
exactly once and passed. It invokes the current compiler's AST/EIR --emit-asm
staticlib route for a scalar export, and generates the real feature-none runtime
in static-library boundary mode. The existing C pool host is reused verbatim.
No mock runtime helpers or borrowed bridge archive were substituted. The generated
bundles live under the test-owned temporary directory:
/var/folders/9d/f4jbjvxs0t5d94xl7cp671t00000gn/T/elephc_linux_context_pool_probes_93696_ThreadId(2)_0.

Both bundles were assembled and linked inside their corresponding existing Docker
images, then executed with exit 0 and POOL-OK. Each run uses --rm, one CPU and 512 MiB,
mounts only its own bundle, and performs cc/ar/cc, not a Cargo rebuild. This checks
the C pool/worker-entry/fatal resource cleanup/slot-race/quarantine surface on Linux
AArch64 and x86_64. It does NOT establish the Rust Parallel bridge, PHP-array ABI,
Fiber/Generator worker execution, full Async/I/O or the entire executable matrix.
The x86_64 image executes through Docker's amd64 support on the ARM64 host.

Provenance (SHA-256):

- ARM64 image: 3f506a232838c660a0258049609b99a32be19beeaff50f93bd691da0c599a96e.
- ARM64 auth.s: d29ae9ffab5ed6fd4b88e58950020226b0f194a12e0b466182e76832b9a10f90.
- ARM64 runtime.s: 350ba200fc6d03eb72737a8fac3ca4f2b0b098fc3c3c6d6c03613a5818490cd2.
- x86_64 image: db9f3dce9005b21801ae553692d7c5c69fac998efd890766faedeccfcbd47312.
- x86_64 auth.s: 2ee837b179e5a96874a0448294a46b591f9f1667facc4587f861e2e61df34d5e.
- x86_64 runtime.s: 451c4c62986baafbb072e23179d3275d1dbe1829133c7c367c1377416ace34ce.
- Shared pool-host.c: ee158d84c120c355a5cef43b5ccac2ad266abf711e6f570387a72de53948d571.

The normal Linux runner's disk guard remains intact; no Docker volumes/images from
other worktrees were deleted. The read-only Kioku recall launched for Linux probe
history is still pending; no result is inferred from silence. This thin native
probe is additional direct evidence, not a redefinition of whole-goal acceptance.

### Incoming Rustdoc policy reconciliation

The assembly-comment audit passes on the 20 currently edited ABI/runtime Rust
files, including the new request-state and register-audit leaves. A lexical Rustdoc
scan first had a syntax error, then was corrected; ordinary comments between a
Rustdoc block and item attributes are ignored rather than reported as missing docs.
The latest-edit scan found missing blocks in ctx owner-verification helpers,
Parallel execution controls, two iOS refusal controls, the stream drain helper and
one ABI test. Those functions now have responsibility-specific Rustdoc comments.
The shared Parallel contract's three sizing functions and four tests are documented,
and its focused library tests pass 4/4. A serial cargo check --tests is still active.
No function signature, attribute, assembly instruction or control-flow body was
changed by these documentation edits; the relevant alignment/diff checks pass.

The broader contribution inventory is 389 Rust paths relative to origin/main.
Its lexical scan found no missing module preambles, but reports additional candidate
missing function blocks across the Parallel bridge, preludes, binder/lowering
helpers and test modules. This is a remaining incoming-policy gate, not a reason
to weaken the acceptance checklist or claim that all 389 files are documented.
Candidates need source confirmation before edits; lexical scans are not a Rust AST
proof. Final exporter freshness/review artifacts must account for later doc edits.
Kioku acknowledged the Linux pool measurement as
measurement-current-linux-arm64-and-x86-64-context-pool-asse-cc00e990. The earlier
Linux-history recall remains a live process, with no returned result asserted.

The documentation pass now also covers descriptor param_validation, numeric_bounds,
pending_owners and container_storage. Responsibility-specific blocks explain
borrowed tag/marker validation, independent ABI4 scope/policy, signed integer bounds,
record clearing before release/alias transfer and key-preserving container leases.
The stale ABI-v3 preamble is corrected. These four files pass the lexical function
docblock check, assembly-comment alignment and diff hygiene; only comments changed.
The pre-existing Hash-to-indexed-only ABI gap remains explicitly documented, not
silently fixed by documentation or covered by focused pool tests.

Kioku notes surfaced drifted gotcha-descriptor-parameter-preflight-must-precede-mars-49526109.
Its C61 guidance says the descriptor has no strict_types policy and implies global
preflight before conversion. Current code validates at each physical parameter
position with independent ABI4 scope/policy and pending-owner cleanup. A replacement
decision was submitted with an explicit supersedes/reason edge, retaining the useful
declared-contract and earlier-owner cleanup rules. Capture is still live: do not
claim the old packet has been retired before acknowledgment. The original serial
cargo check --tests is also live (its package identity/cwd were verified separately
from other worktrees' Cargo processes); no other worktree process was interrupted.

The next coherent Rustdoc pass covers crates/elephc-parallel/src/value.rs and
value/tests.rs. Public ABI summaries now complement the existing Safety blocks:
single-root structural completion, exact floating bits, checked container slots,
nonnull cancellation ids, writer consumption at finish, complete-envelope validation,
and blob-borrowed byte views. Internal writer/parser helpers and all seven test/helper
functions are documented. The diff adds 25 and seven comment lines respectively,
with no deletions; lexical function coverage and diff hygiene pass for both files.
Focused value-codec tests have not been rerun yet; the original serial root
cargo check --tests remains active and must finish before another heavy build.

Graft's value.rs API lookup was ambiguous between three old scratchpad review copies,
so those historical spans were not treated as the current source. Kioku notes also
marked its raw-pointer-contract citation moved/context-unverified and the old EPV1
decision drifted. Current work used the explicit owned worktree file; no historical
packet was promoted into fresh codec/native acceptance evidence. The ABI4 note's
superseding capture remains pending, rather than assumed successful.

The failure-envelope module and its eight helpers/tests are now documented as a
cohesive follow-up: checked wire lengths, optional absent/present-empty sentinels,
copied EPFL fields/frames, exact parse exhaustion, borrowed C views and explicit
raw-pointer safety contracts. These edits add 36 and eight comment lines with no
deletions; their lexical docblock and diff checks pass. No encoding, ABI layout,
failure kind, lifetime representation or control-flow body was changed.

The original serial root cargo check --tests completed with exit 0, including its
trailing whole-tree diff hygiene. Subsequent focused bridge tests passed **3/3**
for value:: and **4/4** for failure::; their parent command still needs terminal
polling for its final git diff --check. Those are codec controls, not full native
Parallel/Async or array ABI acceptance. Remaining contribution-wide Rustdoc,
callback/target gates, publication and exact-artifact consensus are still open.

The parent codec-test command subsequently completed with exit 0, including its
final whole-tree diff hygiene. No root Cargo check or codec-test process is left
running from these serial commands. The Kioku ABI4 superseding capture and the
failure.rs graph/notes lookups remain pending; their silence is not evidence of
completed lineage/index refresh.

### Bridge documentation and durable verification continuation

Current Git reads confirm HEAD 516dea0b36250ec9cc83e09040cbe296a3defb57,
MERGE_HEAD/origin/main afe32d3efe27209b87eb68d4b2adcda8dbc5eefd and an empty
unmerged-stage inventory. The merge is still uncommitted and unpublished.

The job and executor documentation pass covers owned input/result copies,
registry release, FIFO reservations, spawn publication, worker status mapping,
context-release fallback and joining the actual OS thread rather than treating a
terminal job result as proof of thread exit. The prior focused commands returned
8/8 job tests and 5/5 executor tests with exit 0; later executor test-summary edits
are comment-only and their fresh rerun is tracked separately below.

The PHP-wire leaf and its tests now document safe-subset parsing, ordered array
keys, depth/count checks, native EPV1 ownership, copied EPFL failures and the
borrowed thread-local prepared pointer's expiration. Explicit raw-pointer Safety
contracts describe readable source ranges, writable outputs and allocator-matched
buffer release. This pass adds 58 and 14 comment lines respectively, with no
deletions or executable changes; it does not close the broader PHP-array ABI gate.

Kioku lineage is now confirmed from both packet files, not inferred from the earlier
pending capture: decision-descriptor-abi4-binding-validates-each-physical--18d9f519
supersedes gotcha-descriptor-parameter-preflight-must-precede-mars-49526109, and the
old packet records the reciprocal superseded_by and reason. The obsolete no-strict-
policy/global-preflight advice must not be used for current ABI4 binding.

During tool-session continuation the focused PHP-wire/executor command's observation
handle disappeared while its own Cargo process remained live (PID 79102, cwd verified
as this worktree, parent wrapper 79064). Both exact owned processes are being stopped
so verification can restart with a durable output log. No result from that lost
output is counted as passing, and no other worktree's Cargo process is interrupted.

The two owned orphan processes were subsequently confirmed absent. The durable
replacement command ran all six modules of the focused elephc-parallel bridge
library: **44/44 passed**, 0 failed, 0 ignored, exit 0. Its complete log is
/tmp/elephc-parallel-integrated-4XZASn. This includes 12 PHP-wire, 5 executor,
8 job, 4 failure, 3 value and 12 bridge/storage tests; do not add the earlier
overlapping filtered counts to these totals. The bridge entry tests use explicit
mock callbacks and are not native emitted-context or whole-scheduler acceptance.

The bridge root now documents worker-table borrowing, exact release/quarantine
handoff, thread-local scope guards and raw transfer-pointer safety. The PHP-wire
and bridge root changes pass the lexical function-doc scan, and focused diff
hygiene passes. These are comment-only changes, including correcting the root's
stale future-bridge wording. Broad contribution Rustdoc and final exporter
freshness remain open. The next executable gate is the descriptor parameter
binding module against the integrated compiler, with a durable log.

The descriptor parameter-binding test leaf now documents its 16 runtime-selected
callback controls and no longer suggests a global pre-marshalling preflight. Its
examples cover fixed/variadic declared contracts, weak Stringable/numeric coercion,
named array COW, throwing conversion cleanup, defaults and returned payload leases.
The shared transfer classifier and checker are documented at their existing v1
boundaries: recursively copied values, direct-only Cancellation, static target
resolution, capture/effect restrictions, literal shape proofs and TaskGroup escape
diagnostics. Their function-doc and focused diff checks pass; only comments changed.

The integrated executable descriptor gate is live, serial, with durable log
/tmp/elephc-descriptor-binding-integrated-X0qWN6. Cargo first waited for the shared
package-cache lock, then began compiling this worktree's root compiler. Its own
Cargo PID 87257 and rustc child 90737 were verified live; no terminal result or
executable pass is inferred yet. A Kioku measurement capture for the 44 bridge
controls is also live, with log /tmp/elephc-kioku-bridge-4Y5KuM; acknowledgment has
not yet been observed. Existing notes/graph queries with lost observation handles
remain distinguishable from completed retrieval and are not current code evidence.

Live remote inspection now reports main at
4f79c92c9eebf8925a611e42dae6a07f6a2e1027 rather than the current merge parent
afe32d3efe27209b87eb68d4b2adcda8dbc5eefd. A fresh fetch is in progress to inspect
that delta; do not silently rewrite MERGE_HEAD or imply the newer parent was
already tested. Finish the pinned integration and account for the follow-up before
claiming current-main publication. PR #958 is OPEN and draft, with head exactly
516dea0b36250ec9cc83e09040cbe296a3defb57; GitHub reports mergeStateStatus UNKNOWN,
not a verified clean merge or passing CI. No push was performed.

The fetch subsequently completed with exit 0. origin/main and FETCH_HEAD now both
resolve to 4f79c92c9eebf8925a611e42dae6a07f6a2e1027, while MERGE_HEAD remains the
original afe32d3efe27209b87eb68d4b2adcda8dbc5eefd. The newer delta contains 13
commits and one first-parent merge, PR #1694 (monitor source names). The scoped
ABI/backend/type/bridge inventory shows src/ir_lower/tests/eval_ownership.rs as
the changed path in those trees; this is an inventory, not a semantic audit of
the complete monitoring delta. Inspect the full delta before the follow-up merge.
Use the explicit original parent SHA for pinned-integration inventories rather
than treating the now-advanced origin/main ref as that old baseline.

The full fetched delta is seven paths: src/ir_lower/tests/eval_ownership.rs,
src/monitor/{decl_ranges,exact,mod,render,tests}.rs and src/parser/mod.rs. It has
not yet been merged into this in-progress integration. The executable binding
command has advanced from compiling the compiler library to rustc compiling
tests/codegen_tests.rs (child 93644 of Cargo 87257). Keep that existing command
and its durable log; do not restart because the large test binary is slow to link.
No descriptor result or Kioku capture acknowledgment is yet counted as complete.

### Incoming monitoring delta and memory acknowledgment

The complete seven-path incoming diff was inspected against the two pinned main
SHAs. It replaces line-based declaration guessing with lexer-backed namespace and
class ranges, recognizes generator entry suffixes without changing complete PHP
names, and aggregates generic instantiations for exact-profile source attribution.
The parser change is a Rust API forwarding the existing class-name token grammar;
it does not add PHP syntax. New monitor controls cover namespaces, comments/string
braces, contextual names, generic headers, suffix ambiguity and exact totals. The
eval-ownership delta splits two all-target tests into separate target entries while
preserving their substantive assertions. None is merged or counted as passing here
yet; the follow-up still needs ordinary integration and focused monitor/target checks.

Kioku capture is now acknowledged in the durable log and its packet exists:
measurement-integrated-parallel-bridge-library-passes-44-uni-a60c7691. It pins ledger
lines 550-556 and has a 20-commit freshness budget. The body explicitly records both
merge parents and uncommitted state; measured_at is the old HEAD, not a claim that
the integrated source was committed at that SHA. No emitted native acceptance is
inferred from the 44 unit controls.

The next cohesive documentation pass adds 15 responsibility-specific comment lines
across argument snapshot cleanup, declared-string return ownership/previous chains,
return-binding effects and worker-entry effect projection. The lexical function-doc
and focused diff checks pass with no executable changes. Kioku api initially reported
no indexed file for the existing string-return leaf; a single-file scan is live to
repair that coverage rather than treating its absence as evidence about the code.
The supplementary graph query has durable log /tmp/elephc-boundary-graph-54sBQB.

The descriptor gate completed test-binary compilation in 24m30s and is now executing
its 16 selected tests. The first test's helper is building the required elephc-crypto
bridge and has waited on the shared package-cache lock. Cargo 87257, its test runner
22597 and helper Cargo 23768 were observed live; no passing test count is asserted
before the actual output. Preserve this command/log across observation-session loss.

### Named descriptor integration regressions and resumed correction

The original descriptor batch completed: **11 passed, 5 failed**, 0 ignored,
with complete diagnostics in /tmp/elephc-descriptor-binding-integrated-X0qWN6.
Three variadic tests exited 255 with Unknown named parameter ($tail, $value,
$named). Two named array-prefix tests failed static checking with callback
parameter $prefix expects array, got mixed. The batch is failed evidence, not
native acceptance; its Cargo/test processes were subsequently confirmed absent.

The interrupted correction is present after resuming on 2026-10-10. The shared
storage predicate now recognizes boxed Mixed and typed key-preserving Hash
collectors, retaining rejection of raw indexed scalar collectors. The invoker
reads a typed Hash's element contract and forces Mixed elements for a boxed
Mixed collector, rather than inheriting an unrelated input-container element.
Static checking defers an explicitly opaque Mixed value only on descriptor
by-value binding into a boxed target. Direct calls, raw indexed targets and
supplied references are not newly relaxed. The existing dynamic-spread projection
path is preserved; the larger raw Hash/indexed ABI gate remains separate.

The shared storage/source contract unit test passes **1/1** after the correction.
A new heap-debug regression requires invalid input to fail before callback entry
and a valid array to run exactly once; the PHP CLI oracle prints
TypeError|callee|2. The corrected descriptor batch now has 17 tests and is running
serially with durable log /tmp/elephc-binding-postfix-s9qvAm. No corrected native
pass is claimed yet. Assembly-comment alignment and focused diff hygiene pass.

Kioku semantic recall returned the rejected blanket-Hash C61 decision, the current
ABI4 physical-binding decision and the callable-return alias gotcha. These remain
compatible with the shape-aware correction: no uniform Hash transport was imposed.
Its single-file scans repaired the string-return leaf's missing index and the
invoker's stale ABI offsets. Supplementary graph results include historical
scratchpad review copies alongside the current owned file; only current-file
edges were used, never historical spans as current executable evidence.

The resumed post-fix pipeline completed with exit 0: storage/source contract
**1/1**, native macOS descriptor binding **17/17**, followed by whole-tree
git diff --check. Log: /tmp/elephc-binding-postfix-s9qvAm. All five previously
failing cases are green. The strengthened negative test's callee marker confirms
the invalid opaque value is rejected BEFORE target entry; its heap-debug cleanup
also passes. This is focused macOS evidence, not a full array ABI/target claim.

The freshly built library-test artifact was then run directly with the invoker
filter: **40/40**, exit 0, log /tmp/elephc-invoker-postfix-unit-ijqyZa. This checks
cache/default consumers, argument/result ownership and target-aware emitters,
including the supported-target scope/argc/cleanup controls; it is distinct from
native Linux execution. No new build or heavyweight parallel test was started
for this short emitter run. Neighboring strict dynamic and named-variadic checks
are serially running from the freshly built codegen test artifact with log
/tmp/elephc-binding-neighbors-KdEX7W; do not infer their outcome until completion.

A renewed lexical Rustdoc inventory against the ORIGINAL integration parent
still reports candidates in scheduler preludes, runtime/lowering leaves and test
modules. Completed bridge/binder documentation is not full contribution hygiene.
Canonical docs freshness, remaining focused gates, original/follow-up merge
commits, fork push, inherited vendor restoration and exact-artifact review remain
open. The Kioku packets/local scratch directories stay excluded from publication.

The neighbor batch subsequently completed with exit 101: strict dynamic binding
**3/3** and ordinary named variadics **8/8** pass, but the three user-relay controls
are **2/3**, with user_function_relays_named_variadic_arguments rejected because
call_user_func_array() sees its variadic argument list as Iterable rather than array.
This is a separate representation leak, not a reason to accept arbitrary Traversable
objects as call_user_func_array() argument arrays.

Known named/runtime-keyed user-function tails now reuse the existing shared
descriptor_variadic_container PHP-array contract in initial resolution, parameter
specialization and associative callback specialization. Source element declarations
are retained separately. Variadic slots are located by name in callback specialization
rather than assuming the final slot; forwarded dynamic array variables remain marked
as potentially keyed. A known dense array literal does not gain a keyed marker, so
its existing indexed path remains available. No blanket list-to-Hash conversion or
public API was introduced. The relay correction is pending executable verification,
with log /tmp/elephc-variadic-relay-fix-WG3O2k; the 17/17 result above predates this
additional correction and is not promoted to a latest-source acceptance claim.

The relay correction's first pipeline completed: its exact failing relay **1/1**,
named-variadic family **8/8** and descriptor binding **17/17** pass. Durable log:
/tmp/elephc-variadic-relay-fix-WG3O2k. A subsequent one-line dense-literal guard
keeps syntactically known indexed literals out of the keyed-source classifier;
those results predate that last guard. The latest-source named family, all three
relay controls and the boxed by-reference named-tail control are now being checked
serially with a new durable log; no fresh result is assumed from the earlier run.
No merge commit, fork push, vendor restoration or reviewer consensus is claimed.

The latest-source slice log is /tmp/elephc-named-final-slice-didbvC, observation
session 91064. Its Cargo 57174 and rustc child 63553 were verified live after the
dense-literal guard, so continue that command rather than starting another build.
Kioku acknowledged the named-binding fix as
bug_fix-named-descriptor-binding-recognizes-boxed-and-ha-e910e6db; its capture log
is /tmp/elephc-kioku-named-fix-DrIC4m. It cites the storage/boxed-binding code and
the bounded 17/40 evidence, explicitly retaining the relay correction's separate
gate and the open complete-ABI/I/O/target/reviewer requirements.

The final slice completed with exit 101: latest-source named variadics **8/8**
and user-function/method relays **3/3** pass. The boxed by-reference named-tail
control instead fails **0/1** with SIGSEGV. Its existing regression is
tests/codegen/callables/state_and_variadics.rs::test_by_ref_variadic_scalar_and_named_tail_writeback_use_boxed_storage.
The complete log is /tmp/elephc-named-final-slice-didbvC. Automatic LLDB replay
shows EXC_BAD_ACCESS address 0 at fn_replace+204, `ldr x9, [x0]`, with x0=0.
The first failing program call is the direct `replace($position, named: $named)`;
the later descriptor/reference/count variants have not yet executed successfully.

Retained native artifact:
/var/folders/9d/f4jbjvxs0t5d94xl7cp671t00000gn/T/elephc_test_76969_ThreadId(2)_0/test.
The callee saves incoming x0 then loads through that saved address before the
dimension update. The next diagnosis must compare direct-call variadic reference
packaging with this callee entry contract; the trace alone does not establish
whether the regression predates the current correction. Do not remove/disable
the reference test, convert references to copies, or claim full by-reference ABI
acceptance from the passing by-value cases. Cargo/test processes for this final
slice are terminal, and whole-tree diff hygiene passes. Merge publication remains
unperformed while this real runtime failure and the other recorded gates remain.

Kioku acknowledged the reference failure as
gotcha-integrated-by-reference-variadic-collector-crash-24ae769e, pinned to this
trace checkpoint and the existing regression source. The named-binding fix and
remaining reference failure are separate packets, so recall must not treat the
positive named-binding evidence as whole reference-ABI closure.

### Reference collector lifetime and scalar retyping diagnosis

Static LLDB disassembly of the retained failing binary identifies the first
failure exactly: main stores the new collector in frame slot -0x3b0, clears that
slot on successful argument-evaluation handoff at main+1200, then passes its
address to fn_replace at main+1396. The callee dereferences that emptied slot.
This is not evidence that the source-variable reference cells should be copied.

The correction acquires a stable owned SSA collector before retiring the snapshot
slot, and includes that detached synthetic reference collector in ordinary user-
call unwind roots. The backend then stages a managed temporary reference cell
instead of recovering the cleared local address. Existing actual reference places
keep their original handling. A new throwing-callee heap-debug regression verifies
caller writeback and retirement after exceptions. An all-target EIR/emission test
checks the detached Acquire, published/retired collector owner and managed cell on
macOS/iOS device/iOS Simulator/Linux ARM64/Linux x86_64; it passes **1/1** with exit 0,
log /tmp/elephc-reference-collector-unit-W6zNBz. This is emission, not native iOS/Linux.

The first expanded reference batch was **3 passed, 8 failed**, log
/tmp/elephc-reference-variadic-fix-ukAdLf. It removed the initial null-slot crash,
then exposed scalar-retyping corruption/leaks. Positional variadic preparation
was using the fixed-reference marker rather than the existing variadic Mixed-
widening marker. Reusing the correct marker advanced the batch to **10 passed,
1 failed**, log /tmp/elephc-reference-variadic-retype-gy4efR, with the new throwing
heap-debug case and the three reference GC controls green. The remaining failure
is the call_user_func segment of the mixed direct/descriptor regression.

PHP 8.5.10 CLI prints a by-reference warning and keeps the source scalar unchanged
for that call_user_func probe. Elephc's current docs/php/functions.md explicitly
promises source-variable mutation through call_user_func, and the existing test
expects it. This safety correction preserves that documented Elephc behavior;
the PHP semantic discrepancy is an explicit open parity/consensus finding, not
silently declared equivalent or resolved by changing the test expectation.

Known descriptor reference-variadic arguments now use the same widening marker
as direct positional/named tails, while fixed reference parameters retain their
existing guarded storage normalization. A separate heap-debug call_user_func
regression checks the documented retyping behavior and balanced owners. Its
expanded 12-control reference batch is running serially with durable log
/tmp/elephc-reference-variadic-final-5nNfoS, session 88575. No terminal result or
full reference-ABI acceptance is inferred yet. Original/follow-up merge, source
documentation/generation gates, fork publication and exact-artifact consensus
remain open; no history rewrite, push or vendor-state restoration occurred.

The latest reference batch completed with exit 101: **11 passed, 1 failed**,
log /tmp/elephc-reference-variadic-final-5nNfoS. Direct positional/named writeback,
methods/closures, callable retyping, repeated aliases, hidden counts, throwing
callee cleanup and the reference GC controls pass. The new isolated heap-debug
call_user_func retyping test also passes. The original combined regression still
prints a pointer-like integer for its call_user_func segment AFTER an earlier
call through the same callable; its other segments produce the expected values.
Do not report the whole reference batch as green or weaken that combined case.

The next investigation is callable metadata lifetime/unknown-signature argument
preparation. callable_descriptor_signature_for_expr consults callable parameter
signatures or flow-local static bindings; clear_static_callable_locals clears the
latter at argument-evaluation joins. The signature-driven widening marker applies
only when the caller retains a known variadic signature. This explains a possible
route difference between isolated and combined calls, but requires focused EIR/
caller evidence before attributing the remaining failure or retaining stale facts.
No blanket bypass of callable invalidation is authorized by the passing isolated test.

Kioku recorded the PHP/Elephc call_user_func compatibility finding as
gotcha-call-user-func-source-variable-writeback-is-docu-792ec083, with successful
capture log /tmp/elephc-kioku-cuf-parity-KZbiNg. The reference batches and all-target
emission test above are terminal, and whole-tree diff hygiene passes. Their native
signal/ownership findings are distinct from the open PHP semantic decision.

Kioku acknowledged bug_fix-reference-variadic-collectors-detach-evaluation--f396e27b
and the explicit superseding edge from the original null-slot crash packet. Its
fact preserves the current **11/12** result and combined-call failure rather than
claiming reference closure. Capture log:
/tmp/elephc-kioku-reference-owner-fix-bYNbC3. Both the remaining dynamic-signature
case and the documented PHP semantic divergence stay open for later reconciliation.

### Context-dependent reference marker authorization

Source inspection separates two facts: callable identity is flow-sensitive, while
the checker records per-argument boxed-reference storage authorization by scope
and source span. Both record_boxed_reference_output and unresolved-callee alias
recording contribute those sites. The runtime descriptor adapter already supports
Mixed-backed caller storage and addresses its concrete payload for typed refs.
Argument CFG joins may discard static callable identity; retaining a stale target
or return signature is not required to honor the checked storage authorization.

When descriptor argument lowering has no signature but the exact variable/span
has checker authorization, it now prepares the widening reference marker rather
than passing the old scalar slot unchanged. Known fixed reference handling and
runtime target selection remain intact. The context-dependent reference batch is
now **12/12**, exit 0, log /tmp/elephc-context-reference-fix-p8Pphu.

The strengthened heap-debug regression performs an earlier callable invocation
before call_user_func and checks cleanup. A fixed int-reference control confirms
its scalar ABI/writeback still yields 1:6. The next pipeline completed with exit 0:
fixed reference **1/1**, expanded reference family **12/12**, descriptor binding
**17/17**, then whole-tree diff hygiene. Log:
/tmp/elephc-authorized-reference-gates-YOQukf. These focused macOS results do not
close complete array/I/O/target or reviewer acceptance.

A further heap-debug control reassigns a previously invoked reference callable
to a runtime-selected by-value identity target; it requires result/source 5|5 so
storage authorization cannot preserve stale callable identity. That control and
the argument-evaluation-owner library family are being verified serially, log
/tmp/elephc-reference-identity-gates-x7HuAP, session 73843. No result from this
last pipeline is inferred yet. The explicit PHP/Elephc call_user_func behavior
discrepancy remains open, and no merge commit/push/vendor restoration occurred.

The runtime-target-selection heap-debug control passes **1/1**. The following
argument-evaluation-owner library family is **12 passed, 1 failed**, exit 101.
Its failure is not a reference retyping output: assembly emission rejects
`backtrace hidden argument collector PHP type AssocArray { key: Mixed, value: Mixed }`
in mixed_named_string_and_callable_coercions_do_not_double_retire_evaluation_owners
(argument_evaluation_owners.rs:586). The full log is
/tmp/elephc-reference-identity-gates-x7HuAP. This newly exposed collector/backtrace
transport gate must be handled shape-aware; do not drop keys or narrow the
signature merely to make the emitter accept it. The reference handoff test inside
this family remains green on its supported-target emission loop.

The authorization fix itself is current-source validated: the combined reference
case is green, the fixed-scalar reference/heap control is green, all 12 reference
controls and all 17 descriptor-binding controls pass. Callable selection remains
runtime-driven in the retargeting control; no invalidated target identity was
restored. These are bounded checkpoints, not whole array/target/review closure.

Kioku acknowledged bug_fix-descriptor-reference-storage-authorization-survi-711c76a5,
capture log /tmp/elephc-kioku-authorization-fix-i5O7sv. The fact separates storage
authorization from callable identity, cites the passing reference/retargeting
controls and explicitly retains the failing backtrace collector family.

The exact unsupported collector diagnostic is emitted by both target paths in
src/codegen/lower_inst/core_builtins/backtrace.rs (matches around 526-534 and
920-928), currently accepting only PhpType::Array. Inspect their frame metadata
and corresponding runtime readers before widening collector support; both
AArch64 and x86_64 must retain the actual shape and argument/key semantics. The
supplementary literal graph lookup is still pending (session 40680), and is not
needed to treat these direct diagnostic matches as current source evidence.
Whole-tree diff hygiene passes and conflict stages remain empty, but HEAD and
MERGE_HEAD are unchanged and the original/follow-up merges remain uncommitted.

### Backtrace positional collector representation

The AST func_args pass guarantees its HIDDEN_ARGS_PARAM snapshot is positional-
only: source variadic entry values are copied through an is_int(key) filter,
and a synthetic surplus collector does not accept unknown public named arguments.
This internal snapshot is distinct from the user variadic's original numeric/
string-key storage. PHP 8.5.10 CLI confirms debug_backtrace args preserve named
variadic entries, while func_get_args excludes them; mutations of the ordinary
variadic variable do not rewrite those entry-time backtrace values.

The hidden snapshot reader now borrows indexed storage or obtains an owned
insertion-ordered values copy for a Hash snapshot using the existing typed
array_values backend emitter. Both debug_backtrace materialization and printed
argument ranges retire that temporary copy, while the original frame collector
remains borrowed and unchanged. Optional hidden count extraction also reads the
normalized positional view, accounting for its extra owner stack slot before
retirement. This is an internal positional snapshot copy, not a blanket user
Hash-to-list transport workaround or named-key preservation claim.

The focused serial pipeline completed exit 0: argument-evaluation-owner library
**13/13**, including its supported-target emission loops; native optional/variadic
backtrace **1/1**; native printed optional/variadic backtrace **1/1**; diff hygiene.
Log /tmp/elephc-trace-collector-gates-GFL8yu. Assembly-comment alignment also passes.
The shared values emitter's visibility was widened only inside lower_inst to reuse
the existing allocation/retention implementation; no PHP builtin contract changed.

New required heap-debug regression:
test_core_backtrace_regression_named_tail_keeps_entry_snapshot_and_keys.
Its PHP CLI oracle is
{"0":11,"1":12,"named":22}|[11,12]|named|{"named":22}|[]|named|.
Current native source fails **0/1** with
[11,12]|[11,12]|bad|[]|[]|bad|, but heap cleanup is clean. The full evidence log is
/tmp/elephc-named-trace-snapshot-n3qfaN. Neither trace materialization nor printing
currently receives the named tail's entry-time snapshot. This test must stay
enabled and must pass before full backtrace/array acceptance: do not infer closure
from the repaired positional reader or read the already-mutated user variadic.

Next work must capture the original named variadic entries separately from the
positional func_* snapshot, preserve fixed/default argument-count rules and keys,
and make both frame materialization and printing consume that view with balanced
ownership on every supported target. Optional non-variadic hidden-count controls
are additionally running from the current codegen artifact; outcomes are pending.

The hidden-count follow-up completed exit 0: call_user_func-only frame-capture
gate **1/1** and eval-only optional frame-capture gate **1/1**, log
/tmp/elephc-trace-count-prefix-zoqbQ8. The latter rebuilt its required bridge
artifacts serially before native execution; it is not a whole eval parity claim.
Kioku acknowledged the missing named-entry snapshot as
gotcha-backtrace-named-variadic-arguments-require-entry-06389138, with successful
capture log /tmp/elephc-kioku-named-trace-tJDCnP. All build/test processes for the
positional/backtrace and named snapshot probes above are terminal.

Current local origin/main resolves to 91136c41cebbf1f5a0166e1bec7b7425895ae1bc,
which is newer than the previously inspected 4f79c92c9e follow-up. MERGE_HEAD
remains afe32d3efe and HEAD remains 516dea0b36. Refresh/inspect the follow-up
against its exact parent before merging it; the earlier seven-path inventory
does not describe every possible newer main change. No source stage, merge
commit, fork push or inherited vendor restoration is inferred from these tests.

### Named backtrace entry snapshot: terminal-cursor correction (2026-10-10)

The diagnostic run /tmp/elephc-named-snapshot-diagnostic-PW8KkP is terminal
(0/1, exit 101). It confirms that captureNamedTrace has an allocated snapshot
local with AssocArray<Mixed, Mixed> storage in both frame readers; the failure
is not evidence that the local itself was absent. Its native output still
omitted names and heap cleanup was clean. Compiler stderr instrumentation used
for that check has now been removed from named_arguments.rs.

The runtime hash iterator documents and emits two distinct negative cursors:
-2 accompanies the final valid entry, whereas -1 means no entry was yielded.
The new named reader incorrectly stopped on any negative result, skipping the
tail entry (including the only entry in the named-only fixture). It now stops
only on -1. Runtime hash iteration semantics are unchanged.

The structural snapshot regression now requires a StoreLocal initializer, not
only a local declaration and emitted reader label. Additional native heap-debug
regressions cover typed named strings after user-local mutation and named
reference updates whose returned trace cells must not alias caller storage.
The PHP CLI oracle for those two fixtures is respectively
{"first":"one","last":"two"} and {"named":42}|42|42.
Post-correction native and structural results are pending; no acceptance or
publication gate is closed by the source diagnosis alone.

The initialized-snapshot structural check completed 1/1 (five supported-target
lowering/emission loops), log
/tmp/elephc-named-snapshot-initialization-20261010.log. The native cursor-fix run
completed 1/3, log /tmp/elephc-named-trace-cursor-fix-20261010.log. The original
mixed positional/named and named-only fixture now matches PHP exactly, including
printed names, with clean heap-debug teardown.

Both supplementary fixtures stopped before runtime: declaring their return as
array while returning $frame["args"] produces "return type expects array, got
mixed". This is a separate required PHP-array return-boundary finding, not a
named-reader failure. Their escape/ownership controls now explicitly return
mixed so the string/reference reader paths can execute; this does not establish
typed-array return compatibility. Preserve the stronger reproducer by changing
the return annotations of namedValues and namedReferences back to array. The
return boundary remains open in the full PHP-array campaign.

The mixed-return ownership controls completed 2/3, log
/tmp/elephc-named-trace-ownership-controls-20261010.log. Typed strings preserve
both original values and final-entry order with clean heap teardown. The named
reference fixture reaches native execution but reports a bad refcount before
caller output. Keep that regression enabled.

The retained native reproducer is
/tmp/elephc-named-reference-debug.wGXVh1/main.php (compiled through the current
CLI with --heap-debug --keep-symbols). Its executable SHA-256 is
bdc66244974a500ec1b9a77b8e472b98759504471be2e15fed60c6b4d6ab7d92;
assembly SHA-256 is
5f9969a251d8d65362fe8df0bbe2c14afb2dc759a57084dc1cddf7290a3a99f1.
LLDB places the failing decref in main's call-owner cleanup, after the callee
returned normally, not after an initial PHP exception. Conditional hash-release
breakpoints show the callee's named-snapshot cleanup retires the last original
hash owner before the caller's owner guard tries to retire it again.

The shared managed-cell initializer retained only identical source/target PHP
representations. For a Hash-backed collector passed as Array(Mixed), coercion
keeps the raw pointer but that inequality omitted the cell's independent payload
retain. The initializer now retains this existing raw Hash/Iterable collector
borrow as well. It does not cast the hash to an indexed list, discard keys, or
change declared element validation. Validation is running in
/tmp/elephc-named-reference-cell-owner-fix-20261010.log; no pass is inferred yet.

That owner-fix run completed 2/3: the reference test now has clean heap teardown
(52 allocations/frees), but outputs {"named":null}|42|99 instead of PHP's
{"named":42}|42|42. Memory safety improvement is not semantic acceptance.
Private invoker marker tag 11 is not an ordinary persistent PHP reference:
mixed_reference.rs uses nested Mixed tag 7 and high word 1 for that protocol.
The plain mixed clone therefore leaves an invoker marker unresolved.

The reference-value materializer used by descriptor binding is now extracted
without changing its instruction sequence into
codegen_support/callable_invoker_args/reference_values.rs. Its old descriptor
wrapper delegates with the same label allocator. The named snapshot reader
recognizes the private marker after unboxing, uses this same source-address/tag
decoder, then clones the resolved value into independent trace storage and
retires both temporary owners. The native test keeps its PHP oracle unchanged.
Validation is pending in /tmp/elephc-named-reference-value-fix-20261010.log;
the descriptor unit suite and five-target snapshot checks must be replayed too.

Kioku operational feedback: recall and targeted scan/api remain usable, but
callers emit_ref_arg_cell_block --context 2 spent over five minutes building
its shell argument list before starting its search. lsof identified its cand
file (1,825,808 bytes) as stdin and its rawhits output still empty; this merge
checkout has 23,353 modified/untracked paths. cmd_callers appends those paths
and repeatedly executes set -- "$@" "$gr_f", making dirty-checkout argument
construction expensive. Only this agent's nonessential query process (78369)
was explicitly cancelled; no build or external process was stopped. Graft's
subsequent graph query completed, while source readback verified the current
coercion and ownership code. Do not treat the cancelled query as no references.

The decoder-fix native run completed exit 0, 3/3, with clean heap-debug teardown,
log /tmp/elephc-named-reference-value-fix-20261010.log. Both named-reference
expectations now match PHP: the trace observes the callee's value 42, and later
trace mutation leaves the caller at 42. The initial printed/mixed-key snapshot
and typed string controls also remain green. The broader backtrace family is
running separately in /tmp/elephc-core-backtrace-family-20261010.log; no result
for that family or the shared descriptor unit replay is inferred yet.

Debugger artifact provenance correction: the first CLI build was subsequently
recompiled to inspect the owner-only fix. The earlier bdc66244/5f9969a2 hashes
above describe that historical pre-owner-fix build, not current main/main.s.
The heap-clean but semantically failing owner-only executable and assembly are
now preserved as main.owner-fix and main.owner-fix.s in the same temporary
directory, with SHA-256 respectively
c69b20be7ecc236452396c9c32de3150e505d58259023e3d58c34f3b38ebe553 and
6d72cf3f8264cabaa41e6f184e4fcef190936c94b77e926c7be2a68ae5dc7972.

The broader Core backtrace regression family completed exit 0, 15/15, including
the eval/native boundary, nested eval, optional/variadic count controls,
call_user_func/array and literal/first-class/variable-callable capture gates.
Log /tmp/elephc-core-backtrace-family-20261010.log. This is focused native macOS
evidence, not complete PHP-array, supported-target or final review acceptance.

Kioku replacement bug_fix-named-backtrace-snapshots-decode-reference-marke-13a10ec4
now supersedes both gotcha-backtrace-snapshots-must-resolve-private-invoker-cf2f2ae1
and gotcha-backtrace-named-variadic-arguments-require-entry-06389138, preserving
the retired findings and their reasons rather than silently forgetting them.
Neighbor reference/descriptor native controls and the shared decoder/snapshot
unit replay are queued serially; do not infer results from the 15-test family.

The current native by-reference variadic neighbors completed exit 0, 12/12,
including throwing writeback, retyping, named tails, closure/method forwarding,
clean heap, repeated-call growth and replaced-box cleanup controls. Log
/tmp/elephc-named-reference-neighbors-20261010.log. The descriptor binding
neighbors are executing next in /tmp/elephc-named-descriptor-neighbors-20261010.log.
The same fail-fast sequential queue (exec session 58230) then runs the shared
descriptor unit suite and initialized snapshot checks before/after EIR
optimization, in /tmp/elephc-shared-marker-decoder-unit-20261010.log and
/tmp/elephc-named-snapshot-optimized-unit-20261010.log. Read their terminal
footers before promoting any pending counts. Do not restart a live queue.

Final source diff hygiene and assembly comment alignment pass for the new
reference decoder, named reader and managed-cell initializer. HEAD remains
516dea0b36 and the original MERGE_HEAD remains afe32d3efe; no merge commit,
follow-up main merge, fork push or vendor stash restoration occurred here.

### Shared-decoder validation and function documentation (2026-10-10)

The previous sequential validation queue completed exit 0: descriptor binding
neighbors 17/17, shared runtime_callable_invoker module controls 28/28, and
argument-evaluation ownership controls 14/14. The last group checks the named
snapshot initializer before and after EIR optimization and emits every supported
target. Logs are /tmp/elephc-named-descriptor-neighbors-20261010.log,
/tmp/elephc-shared-marker-decoder-unit-20261010.log and
/tmp/elephc-named-snapshot-optimized-unit-20261010.log. The 28-module filter is
not the earlier broader 40-invoker filter; keep their coverage descriptions distinct.

A fresh contribution inventory relative to the original afe32d3efe integration
parent found 395 lexical missing-function-doc candidates. Each candidate's
current source was inspected before adding responsibility-specific Rustdoc:
prelude builders and parse-parity helpers, worker-safety/effect analysis,
runtime guards and teardown/registry locks, instrumentation state accounting,
transfer diagnostics, Mixed value-copy/string boundaries, Async behavior and
heap-debug ownership fixtures. Only comments changed in this documentation pass;
signatures, attributes, bodies, assembly instructions and test expectations remain.
The old legacy itoa diagnostic now explicitly describes its fixed local artifact
directory and the absence of native execution evidence.

The same lexical inventory now reports zero candidates. Its audit source and
initial/final inventories are /tmp/elephc-function-doc-candidates-20261010.awk,
/tmp/elephc-function-doc-candidates-20261010.txt and
/tmp/elephc-function-doc-candidates-20261010-final.txt. This is a source-confirmed
lexical checklist, not a Rust AST proof or whole-repository Rustdoc acceptance.
Focused checks/build, canonical docs freshness and merge publication still need
their own terminal evidence. The full scheduler/array/I/O/review goal stays open.

The broader backend comment audit covered 238 existing contribution files and
found 534 issues: 520 existing comment-column mismatches in 23 files and 14
missing comments in the descriptor suspension guard. Existing comments were
mechanically aligned without changing instruction strings or comment text; the
14 additions describe descriptor-kind/name comparison and mismatch cleanup.
A final rerun reports all comments aligned, log
/tmp/elephc-premerge-asm-audit-final-20261010.log. File inventory is
/tmp/elephc-premerge-asm-audit-files-20261010.txt. New named-reader/reference
decoder leaves were separately checked. Kioku recorded bounded hygiene
measurement-main-integration-rustdoc-and-assembly-comment-in-6d428754.

Root cargo build -p elephc completed exit 0 without warnings, log
/tmp/elephc-premerge-build-20261010.log. Its queued Async/Parallel parse-parity
and prelude-detection library checks remain active in exec session 75149.
Subsequent assembly comment formatting changes only comments/spacing; canonical
exporter freshness and final compilation evidence still need their own run.

The latest explicit fetch now pins origin/main at
9df6d3bd7ad066484a9c21c4eb1f54dbdfa2fbf3. Beyond the earlier inspected monitor
source-name merge and traffic update, it adds PR #1808 canonical loop optimization
and associated CFG/range/liveness/LICM changes. The afe32..9df follow-up inventory
is 38 paths, 2553 insertions/475 deletions; it is not the earlier seven-file delta.
Finish the original ordinary merge without rewriting MERGE_HEAD, then inspect
and integrate this pinned follow-up through a second ordinary merge and rerun
the affected optimizer/ownership/scheduler gates. Neither merge is yet committed.

The default build/prelude validation queue completed exit 0: Async parse-parity
and injection 8/8, Parallel parse-parity/worker patches 8/8, and shared prelude
reference detection 39/39. Logs /tmp/elephc-premerge-async-prelude-20261010.log,
/tmp/elephc-premerge-parallel-prelude-20261010.log and
/tmp/elephc-premerge-prelude-detection-20261010.log. No full local suite was run.

The canonical builtin export/render/audit workflow is executing serially from
the reconciled source with the curl exporter feature enabled (exec session
21463). Its logs use /tmp/elephc-premerge-builtin-* plus premerge-php-comparison,
site-compat and eir-boundary. Generated pages are not manually rewritten.
Check the exporter and every subsequent terminal log before staging the original
integration merge. Docker guard shell syntax and mocked preflight controls pass;
the contribution function-doc inventory is still zero before publication.

GitHub readback confirms PR #958 is open/draft on Guikingone's
spike/runtime-ctx-register with exact head 516dea0b36250ec9cc83e09040cbe296a3defb57.
Both configured publication remotes and the exact inherited-vendor stash remain
unchanged; unmerged index entries remain zero. No other live collaboration agent
is editing this worktree. Untracked project additions intended for explicit staging
are the heartbeat discussion note, Docker guard test, named backtrace reader,
shared reference-value decoder and two runtime-context audit/state leaves. Local
graft graph cards are untracked too and must stay outside publication alongside
.kioku/.codex/.mcp.json/scratchpad; do not use git add -A.

The pinned loop follow-up's recognizer/CFG/driver and branch/range/LICM/liveness
diffs were inspected before integration. It restricts induction summaries to PHP
Int, retains checked overflow modes, avoids generator/handler loop rewrites,
propagates liveness deltas and limits standalone LICM materializations to blocks
dominating every latch. Its own plan explicitly leaves a pre-existing raw-slot
overflow difference between optimizer modes open; do not infer that our complete
array/ABI acceptance follows from upstream's optimizer measurements. The follow-up
also widens existing scoped Linux eval fixture budgets, not the general watchdog.
Its structural and executable tests still need replay on our integrated branch.

Canonical builtin workflow completed exit 0: curl-enabled exporter build,
1185 builtin contracts, 209 classes, 1117 constants, 2254 rendered pages, zero
builtin audit errors, 2276 site-compatible pages and zero enforced EIR structural
inventory errors. Logs /tmp/elephc-premerge-builtin-exporter-20261010.log,
premerge-builtin-render, builtin-modules, php-comparison, builtin-audit,
site-compat and eir-boundary. Generated changes versus the provisional index
remain 1041 paths, 1070 insertions/deletions, consistent with the earlier exporter
refresh; no generated page was hand-edited. Whole-tree diff hygiene passes.

The original merge can now be committed as an explicitly unaccepted WIP
integration checkpoint. Its publication does not close the follow-up 9df main
integration, typed-array return/consumer coverage, I/O provider or exact-artifact
review consensus. Preserve the reserved vendor stash until both merge commits
are finished and its inherited local state can be restored without publication.

### Original integration committed; pinned follow-up under validation

The ordinary integration commit is
027c82fa89bcd77cd1e8fb5c23dfaf9067203603, with parents 516dea0b36 and afe32d3efe.
The staged source/docs manifest excluded .codex/.kioku/.mcp.json/graft/scratchpad
and inherited vendor paths. Cached diff hygiene additionally caught four new
blank EOF lines in existing incoming test files; only those trailing blank lines
were normalized before the commit. This commit is not pushed or accepted yet.

A second ordinary no-commit merge now has MERGE_HEAD
9df6d3bd7ad066484a9c21c4eb1f54dbdfa2fbf3. Its only conflict was adjacent insertions
in src/monitor/exact.rs: our scheduler table and main's sampled/generic declaration
attribution helpers. Both implementations are retained, with their own complete
function scopes. Conflict stages are empty and cached/working diff checks pass.
New loop, CFG, range, LICM and liveness code arrived through the ordinary merge;
the old original parent was not rewritten.

The post-follow-up validation queue (exec session 1110) is active and fail-fast:
EIR pass unit tests, argument-owner controls, monitor unit tests, native canonical
loop controls and the public Async behavior family. Logs use
/tmp/elephc-main-followup-{ir-passes,argument-owners,monitor,loop-native,async-native}-20261010.log.
Do not infer completion of a later group if an earlier one fails. Commit/push the
follow-up only after relevant terminal results; then restore the reserved vendor
stash and verify the exact fork/PR head while keeping final review gates open.

Post-follow-up unit results now prove 223/223 tests under ir_passes::tests:: and
14/14 argument-owner controls. The narrower pass filter does not include every
analysis-local unit outside that tests module. Likewise the attempted library
monitor::tests:: filter selected only the runtime async_monitor hook (1/1), not
the CLI monitor renderer: monitor is wired in src/main.rs and must be tested
through --bin elephc. Correct both scopes after the still-live native loop/Async
queue (1110), without restarting it. Current function-doc inventory remains zero.
Cargo workspace/CLI-target metadata parses successfully (21 workspace members).
The first nextest TOML attempt failed because this shell's python3 lacks tomllib;
its combined shell exit did not prove that check passed. Retry with an installed
newer interpreter or nextest's own configuration loader before reporting success.

The native loop group completed 7/10, exit 101, stopping the fail-fast queue
before public Async execution. Its three failures are structural: ordinary
numeric assignment now emits MixedClone instead of the old Acquire scaffold,
but CheckedIntSink recognizes only Acquire. Loop counters remain boxed, preventing
scalar promotion/coalescing. The overflow/example assertions remain unchanged.
Current evidence is /tmp/elephc-main-followup-loop-native-20261010.log.

CheckedIntSink now accepts non-pinned numeric value copies only within its
already-proven integer use/slot graph. Copies exposed to output, boxed readers,
terminators or unknown uses still reject specialization. Four new focused unit
controls cover copied numeric retirement, observable retired slots, escaped copy
output and cast-justified overflow mode. They are validating in exec session
82447, log /tmp/elephc-copied-integer-sink-unit-20261010.log. Do not publish the
follow-up until unchanged native loop regressions and neighboring ownership
controls pass.

Configuration validation was recovered without global tool installation: a
standalone Rust diagnostic used the already-built TOML crate and from_str's
document parser. It parsed 44 default and 44 CI overrides and verified the
single shared Linux eval-family override in each profile. Log
/tmp/elephc-followup-toml-validated-20261010.log. Python tomllib and cargo-nextest
are unavailable in this shell; full nextest filter/config loading is not claimed.

The copied-integer unit run completed 15/16, exit 101. Positive slot/cast
specialization and the existing refusal controls passed; the new escaped-output
fixture tried to append an instruction after a terminator and failed in Builder
before exercising the pass. The fixture now temporarily takes/restores its
original terminator around that observation; its rejection assertion is unchanged.
New fail-fast queue 66140 runs 16 unit controls, the broader ir_passes:: filter,
then the unchanged native loop family. Logs
/tmp/elephc-copied-integer-sink-unit-retry-20261010.log,
/tmp/elephc-copied-integer-pass-neighbors-20261010.log and
/tmp/elephc-copied-integer-loop-native-20261010.log. Follow-up remains uncommitted.
Kioku recorded gotcha-independent-numeric-value-copies-must-participat-7f76d144
with the original 7/10 native evidence and conservative proof requirements.

The corrected fixture/unit retry completed exit 0, 16/16, including the
escaped-output refusal. The broader ir_passes:: replay completed 242/242,
including analysis-local tests excluded from the earlier 223-test selection.
Logs /tmp/elephc-copied-integer-sink-unit-retry-20261010.log and
/tmp/elephc-copied-integer-pass-neighbors-20261010.log. Native canonical loop
validation remains active in the same queue 66140 and retains all original
coalescing, overflow-mode, execution-order and supported-target assertions.

The unchanged native loop group now completes exit 0, 10/10, log
/tmp/elephc-copied-integer-loop-native-20261010.log. Equal counter coalescing,
cast/fatal overflow modes, zero-trip/conditional behavior and all supported
target emitters retain their original assertions. This reconciles the 7/10
integration failure without weakening the tests. Refusal controls remain green.

The corrected neighboring queue is now active: CLI monitoring is tested through
--bin elephc, followed by argument-owner controls, native Mixed copy fixtures and
the public Async family. Logs /tmp/elephc-followup-cli-monitor-20261010.log,
followup-copy-proof-owners, followup-mixed-copy-native and
followup-public-async-native. Do not infer those results from the loop group;
publication and full final acceptance remain open until their own gates run.

Current neighbor validation runs in exec session 68448 (CLI-monitor Cargo PID
4546 confirmed live, parent queue 4545). Its source is unchanged while compiling.
Kioku replacement bug_fix-checked-integer-sink-specialization-preserves-nu-92c90af7
supersedes gotcha-independent-numeric-value-copies-must-participat-7f76d144 with
the verified 16/242/10 results, while keeping broader neighbors and acceptance open.
The same contribution-relative function-doc inventory remains zero, and the
238-file backend comment audit reports all comments aligned. No source conflict,
commit/push or inherited-vendor restoration is inferred from a still-running job.

The corrected CLI monitor group completed 82/82, including scheduler rendering
and main's source/generic attribution tests. Argument-owner replay is 14/14.
The next native Mixed-copy group completed 29/31, stopping before Async: named
source snapshots produce a poisoned result after later reference mutation, and
partial named variadic conversion leaks 2 blocks/80 bytes. Both reproduce with
ELEPHC_IR_OPT=off, so neither failure depends on the new integer specialization.
Logs /tmp/elephc-followup-{cli-monitor,copy-proof-owners,mixed-copy-native}-20261010.log,
/tmp/elephc-named-snapshot-no-ir-opt-20261010.log and
/tmp/elephc-named-variadic-no-ir-opt-20261010.log.

Retained CLI fixtures and off-mode EIR are in
/tmp/elephc-named-copy-debug.dFHjN8. The named fixed-argument call loads the source
box before change_later but creates no snapshot; a later mutation retires that
box before take_first consumes its pointer. Planned named source lowering now
uses the same by-value snapshot primitive and protected evaluation ledger as
positional lowering. Reference places/variadic reference markers remain on their
existing path. The existing source-order and heap assertions are unchanged.
A new structural check requires the named snapshot before the later reference
call on all supported targets. Native copy validation and the structural replay
are running serially in exec session 22944, logs
/tmp/elephc-named-source-snapshot-fix-20261010.log and
/tmp/elephc-named-source-owner-proof-20261010.log. Outcomes are pending.

The first shared-snapshot correction completes the native copy family 31/31,
exit 0, and both isolated no-IR-opt controls 1/1 with unchanged heap-clean
assertions. Logs /tmp/elephc-named-source-snapshot-fix-20261010.log,
/tmp/elephc-named-snapshot-no-ir-opt-fixed-20261010.log and
/tmp/elephc-named-variadic-no-ir-opt-fixed-20261010.log. Kioku records the
original failure as gotcha-planned-named-value-sources-need-the-shared-posi-84c08f3c.

The first structural replay is 14/15: the new named snapshot check passes on
five targets, but the existing static/method source-evaluation check still
requires an unwind root before the later argument call. That assertion was not
weakened. Earlier named by-value snapshots now retain the existing evaluation
root publication. release_if_owned already calls clear_consumed_argument_snapshot,
clearing its former protected ledger slot when ownership moves to that root;
the two ledgers therefore do not both retire the same live lease.

Fresh fail-fast queue 31839 now tests the 15 owner controls, 31 native copy
fixtures, 12 reference-variadic neighbors, 17 descriptor neighbors and the public
Async family. Logs /tmp/elephc-named-source-{published-owners,published-copies,
reference-neighbors,descriptor-neighbors,public-async}-20261010.log. Results remain
pending. The second merge, fork push and inherited vendor restoration remain
uncommitted/unperformed; full PHP-array/I/O/target/review acceptance is unchanged.

The corrected publication-root structural group completes 15/15, exit 0, log
/tmp/elephc-named-source-published-owners-20261010.log. It includes the unchanged
static/method pre-call unwind-root assertion and the new five-target named
snapshot-before-reference-mutation check. The same queue 31839 continues with
fresh native copies and reference/descriptor/Async neighbors; no later result
is inferred yet. Working diff hygiene and touched function-doc scans pass.

The publication-root native replay is 31/31 copies, 12/12 reference neighbors
and 17/17 descriptor bindings. Public Async is 66/67: the internal generation
reuse probe produces no result through ReflectionMethod::invoke. Its retained
instrumented CLI fixture /tmp/elephc-stale-generation-debug.Khn0n5 prints
start|after-first|after-second|NULL, never entering either reflected callback.
runRoot still returns mixed directly; this is not an internal return-wrapper API
change and its expected stale-generation assertion remains unchanged.

Argument-evaluation guard lowering builds a rethrowing alternate cleanup path
then clears every callable/reflection association at the success continuation.
That continuation has only the already-lowered successful predecessor, so handler
construction must not erase its valid post-evaluation ReflectionMethod target.
A small context/callable_flow.rs leaf now snapshots the actual success facts after
argument effects and their invalidations, and restores only those association
maps after emitting the terminal cleanup path. IR, slot/owner ledgers and mutation
generations are not rewound. The change does not retain pre-evaluation identities
across real retargeting.

Fresh queue 55653 tests 33 native copy/flow fixtures (including a ReflectionMethod
survival case and a reference-call retargeting refusal control), the unchanged
stale-generation probe, then the 15 supported-target owner controls. Logs
/tmp/elephc-argument-success-{flow-facts,stale-generation,owner-proofs}-20261010.log.
Results are pending. No tracking push or full acceptance is inferred yet.

Flow-fact native replay is 32/33. The actual retargeting control and all prior
copy controls pass; the ReflectionMethod survival fixture returns the required
42 but fails its unchanged heap assertion with 33 blocks/8328 bytes retained.
The original stale-generation probe independently passes 1/1 with EIR optimization
off, log /tmp/elephc-flow-facts-stale-isolated-20261010.log. Its assertion was not
changed, and the reflected callback now executes.

The fail-fast queue stopped before owner-proof replay. Reflection cleanup is
isolated in /tmp/elephc-reflection-owner-debug.3x7qfn: a baseline consisting only
of ReflectionMethod construction/unset retains the identical 33 blocks/8328
bytes, without invoking a method or creating an argument guard. The full flow
fixture and baseline have retained PHP, binaries, stdout/stderr, and full EIR/
assembly for the main fixture. Its EIR explicitly releases and zeroes the
ReflectionMethod slot. Attribute the remaining leak to construction/cleanup
only after checking its actual ownership path; do not blame the successful
flow restoration or weaken the new heap assertion. Follow-up remains uncommitted.

Reflection leak diagnosis follows the actual creator lease. Fresh nested
ReflectionClass/Method/Enum owners were boxed with the borrowed-value helper,
which retained a child without retiring its creator reference. Five confirmed
member/prototype/parent/declaring/enum sites now use the existing owned boxer.
The first run improved 33 blocks/8328 bytes to 6 blocks/2376 bytes but remained
red (32/34 controls). That improvement is not acceptance.

The baseline with only ReflectionMethod construction/unset retains the same six
remaining blocks, so the earlier speculative "holder" attribution is withdrawn.
The remaining matching sites are fresh constants/default Hash arrays and fresh
ReflectionNamedType/UnionType/IntersectionType or parameter declaring-owner
objects. Nine additional confirmed sites transfer their creator lease using the
same owned boxer. Borrowed static-property values and global enum singleton
values intentionally retain the ordinary borrowed boxer.

New heap-debug coverage exercises nested parent/prototype/constructor/declaring
owners and parameter declaring-function/union-type owners with expected PHP
names/nullability unchanged. Current queue 89803 runs 35 native copy/flow/
reflection controls, the unchanged stale-generation probe and owner proofs.
Logs /tmp/elephc-reflection-{creator-transfers,transfer-stale-generation,
transfer-owner-proofs}-20261010.log. No result for this expanded fix is asserted.
All touched Rust function docs, diff hygiene and three reflection emitter comment
alignment checks pass. Neither final acceptance nor publication has occurred.

### Reflection getter source-box retirement, 2026-10-10

The expanded creator-transfer queue completed with 33/35 native controls passing.
The two new metadata regressions remained red: nested metadata retained 75 blocks
(17376 bytes), and function/parameter metadata retained two strings (96 bytes).
The unchanged stale-generation and owner-proof replays did not start because the
queue stopped at the failing copy family. These are not passing checkpoints.

Four isolated getters under `/tmp/elephc-reflection-getter-minimal.sxVNq1/`
split the nested retention exactly: `prototype` retained 42 blocks/9096 bytes;
`declaring` retained 33 blocks/8280 bytes; `parent` and `constructor` were clean.
ReflectionClass construction and getMethod alone were already clean. EIR and
assembly show that the two failing getters retain the concrete object extracted
from an owned Mixed property read but never release that source box.

The shared heap-return coercion now retires an owning boxed source after concrete
payload extraction. Borrowed sources remain untouched. An all-five-target EIR
and assembly regression checks extraction-before-release, exactly one source
release, transfer of the result, and preservation of the borrowed receiver.
The native nested-metadata regression now destroys the original reflectors
before reading the returned objects, checking their independent lifetime.

The two strings are a separate duplicate initializer: ReflectionFunction wrote
`__short_name` once in the owner-common branch and again in its namespace-aware
initializer. Removing the earlier write preserves the latter and avoids losing
the first allocation. Both corrections are awaiting fresh test results; the
merge remains in progress and neither acceptance nor publication is claimed.

The first five-target return-owner proof passed (1/1). Rebuilding the final
assertions and replaying `ir_lower::tests::` completed with 297/314 passing and
17 failing. The final return-owner proof passed in that run, as did the focused
argument evaluation owner proofs. The broader result is explicitly red.
The initial fail-fast queue therefore did not start native tests or the build.

Failures require individual reconciliation rather than weaker expectations:
extra snapshot/evaluation leases change several count or producer-shape probes;
three callable-flow probes still see physical arrays where descriptors are
expected; the example corpus helper misses the Async prelude; context-register
handler assembly no longer necessarily contains a global handler symbol; and
effect/destructor-handler assumptions require source-level checks. No cause is
declared settled solely from these observations. Full diagnostics are retained
in `/tmp/elephc-reflection-return-owner-lowering-20261010.log`.

An independent serialized queue now runs native Mixed copy/Reflection controls
with IR optimization on and off, then Async, backtrace, descriptor-param binding,
by-ref variadics and the root build. Those outcomes remain pending. The old CLI
was copied to `/tmp/elephc-before-owned-mixed-return-20261010` before rebuilding,
so focused EIR baseline comparisons can avoid mutating the dirty source tree.

The independent Reflection/copy replay completed with **35/35 passing in both
optimizer modes**. The returned nested objects remain usable after destruction
of their original reflectors and all focused heaps are clean. This result
belongs to the source checkpoint before the positional-callable correction
below, not an accepted final artifact.

The broader `async_scheduler` filter completed with **141/147 passing**. Six
Fiber ownership regressions remain red: array-stored throw state errors,
escaped temporary throw, rejected resume with boxed or typed results, rejected
second start with a Mixed result, and a dropped suspended Fiber owning an object.
The fail-fast queue did not start backtrace, descriptor, reference or build steps.

Two callable-flow failures were reproduced as native bugs using the saved CLI
before the Reflection return-box correction: static joined arrays report
`Unsupported EIR callable_descriptor_invoke callable descriptor without invoker`;
the joined instance array segfaults. PHP produces `7` and `8`, respectively.
Artifacts are under `/tmp/elephc-callable-join-baseline.GNzq9q/`. These are not
merely stale structural assertions and must be corrected before publication.

The positional source-only path omitted `lower_tracked_callable_array_param`,
already present in the specialized and named paths. It now materializes only a
proven by-value callable-array target and snapshots that descriptor before later
source effects. The original PHP local remains an array, by-reference arguments
remain lvalues, and genuine target invalidation is not suppressed. Two native
regressions cover both branches and captured receiver lifetime. A serialized
queue replays unchanged callable-flow proofs, return/evaluation owners, and the
now-37 native copy controls in both optimizer modes. No result is asserted yet.

The positional-callable queue completed successfully: unchanged callable-flow
proofs **7/7**, return-owner filters **4/4**, evaluation-owner proofs **15/15**, and
native Mixed copy/Reflection/callable controls **37/37 in both optimizer modes**.
The three formerly failing callable-flow proofs pass without assertion changes.
No aggregate 314-test result is inferred from these focused reruns. The remaining
lowering failures still need individual reconciliation.

The earlier broader Async replay includes **67/67 passing in the public Async
namespace**, distinct from its six failing Fiber ownership controls and the full
147-test filter. This public result is historical evidence before the positional
correction, not a substitute for its pending current-source Fiber checks.
The current queue runs root build, backtrace, descriptor-param binding, by-ref
variadics and the focused rejected/escaped/dropped Fiber controls. Outcomes are
not asserted until the respective logs and process completion are checked.

### Suspended-coroutine cleanup register restoration, 2026-10-10

The retained LLDB replay for the dropped suspended Fiber stops in
`__rt_object_free_deep` immediately after `__rt_exception_cleanup_frames`:
`x0` is zero and the next instruction loads the Fiber stack-base field through
it. The helper explicitly set x0 to zero to unwind the entire parked activation
chain and never restored the saved coroutine pointer. The x86_64 Fiber arm has
the same caller-saved-result assumption after its cleanup helper call.

The shared suspended-frame cleanup exit now reloads the coroutine object from
its existing saved stack slot on both architectures, including the no-chain
path. Caller and coroutine cleanup-chain ownership/order are unchanged. An
emitter regression checks the common exit for Fiber and Generator prefixes on
all five supported targets. Assembly comment alignment and diff hygiene pass;
the new unit/native checks remain pending, so this is not a claimed fix result.

This runtime edit followed the 37/37 both-mode checkpoint. The live neighboring
queue may rebuild before later steps; no pre-edit build result is claimed as
proof for the modified runtime. The five-target emitter test and dropped-Fiber
native regression must be explicitly replayed against the correction.

The coroutine-pointer correction is now verified: object-deep-free emitter
tests **4/4**, including the common exit on all five targets; unchanged dropped
Fiber native regression **1/1**, output `dropYdone` and clean heap; Mixed copy/
Reflection/callable controls **37/37** after the runtime edit; fresh root build
warning-free. Backtrace controls passed **20/20**, descriptor-param binding
**17/17**, and by-ref variadics including eval **16/16**. The focused nine Fiber
controls completed **4/9**: the dropped-Fiber case is fixed, five state-error/
escaped-throw cases remain red. Logs retain the precise checkpoint; no global
matrix or review acceptance is inferred.

### Fiber helpers must not consume published caller operands, 2026-10-10

An owned `/tmp/elephc-fiber-state-owner.a89J2d/` probe reproduces rejected resume
with a fresh object argument. CLI symbols are stripped by default, so the first
two LLDB breakpoints did not resolve; rebuilding with `--keep-symbols` made the
throw helper visible. `rethrow-lldb.log` shows the first FiberError alive with
refcount one and class ID seven, then the same pointer with refcount zero and
freed-memory poison at the rethrow. The EIR publishes the argument value in a
call-operand slot whose `ReleaseLocalSlot` remains responsible for retirement.
The backend's rejected-resume path freed that value before constructing the
FiberError; allocator reuse let the published slot free the new exception.

Fiber start/resume state guards now release only references that the shared
`value_can_transfer_ownership_to_consumer` predicate permits them to consume.
Receiver transfer additionally preserves the existing expression-temporary
restriction. Direct and Mixed-dispatched `throw` use the same predicate for the
Throwable ownership flag, instead of treating `Owned` alone as permission to
consume it. Published caller leases retain their ordinary success/unwind cleanup;
unrooted transferable temporaries keep the existing eager-release behavior.

Two target-emission regressions check start/resume rejection paths and both
private throw ownership flags on all five targets. The new unit and unchanged
nine-case native Fiber replay are running in a serialized queue. These edits
remain uncommitted and no passing outcome is asserted before that queue finishes.

The first two new emitter fixtures failed their assumptions (0/2), not the
native assertion: their function-scoped start argument was transferable, and
the temporary receiver genuinely transferred ownership. The final fixtures now
use the reproduced main-scope owner shapes and assert their EIR precondition:
each tested method argument is stored in a slot, published, and paired with
`ReleaseLocalSlot`. The throw control uses a caller-local receiver instead of
claiming its temporary is rooted. The unchanged nine native regressions pass
**9/9**, including all five formerly red cases, with clean heaps.

The final two emitter proofs, nine native cases with IR optimization off, broad
Async replay, 37 copy controls and root build are queued serially. Their result
is pending. The old 0/2 fixture log is retained as diagnosis, and the fresh retry
log is distinct; no assertion is weakened to accept double retirement.

### Complete synthesized FiberError payload, 2026-10-10

On resumption, session 93577 had terminated with exit 101. Its emitter retry
passed the throw-owner control but failed the start/resume fixture precondition
(1/2). Because the queue used `set -e`, the following no-optimization/native,
Async, copy and build commands did not execute. The fixture previously chose
the first StoreLocal for an argument; the same value can also have a published
owner slot. It now requires one matching slot with both publication and release,
without requiring every auxiliary store to be an owner.

The emitted IR for the exact fixture also shows that the main-scope Mixed
call result is genuinely transferred (`UnsetLocal` before MethodCall, then
`ReleaseUnlessAliases`), unlike the fresh object argument's published owner.
The start fixture now uses a fresh object as well; its precondition stays strict.
The native Mixed call-result regressions remain unchanged and independently
exercise the transferable case. This is a fixture correction, not permission
to treat all owned expressions as published roots.

The separate owned probe `details.php` in
`/tmp/elephc-fiber-state-owner.a89J2d/` confirms a second real defect: native
output ends at `FiberError|Cannot resume a fiber that is not suspended|64|`
and the executable exits 139. PHP prints the same prefix with `0|none` and
returns normally. Both Fiber state-error helpers allocate only 24 bytes and
stamp ordinary-object heap kind 4. Canonical builtin Throwable allocation in
`src/codegen/lower_inst/objects/throwable_new.rs` allocates 56 bytes with heap
kind 6; the previous reader explicitly selects its raw link by that kind.

Both helpers now allocate 56 bytes, select kind 6, initialize code to zero,
clear the creation line through the existing unknown-line helper, and initialize
both previous-slot words to zero. A five-target emitter regression checks these
stores before throwing. A heap-debug native regression reads class, message,
code and previous and requires PHP-equivalent output plus a clean heap. These
new results are pending; there is no target execution or consensus claim yet.

Kioku recall was used first and returned the Kioku-standard convention plus
neighboring scheduler packets, but no compact Throwable layout evidence. The
available graph lacks unambiguous caller edges for these emitter names because
scratch review copies share them. Exact checked-out source and scoped reference
search supplied the missing layout/dispatch evidence; no Kage integration was
used or changed.

The two subsequent `fiber_` unit replays each completed **9/10**, with the
five-target compact Throwable layout and throw-owner controls green. The
remaining failure still concerned the start fixture: even a fresh object is
transferred before `Fiber::start()` rather than kept in a final published root.
The unit test is now explicitly a resume published-owner proof, with unchanged
strict EIR preconditions; it no longer claims a start root the real lowering
does not produce. The unchanged native start controls cover that separate path.
Because both queues stopped at this unit failure, their later native commands
did not run. Final-source tests are being replayed; no aggregate success inferred.

The following native-first queue (session 10829) terminated with exit **143**
during compilation, without a diagnostic beyond `Compiling elephc`. The runtime
library artifact exists, but that does not prove the executable regression ran.
No owned build remained alive on inspection; a separate Cargo build belonged to
another worktree and was left untouched. The same focused queue was relaunched
with a distinct `native-retry` log. Source hashes and Git merge state are unchanged.

The native retry completed **10/10**, including the new field-reading regression:
`FiberError|Cannot resume a fiber that is not suspended|0|none`, with clean heap
teardown. All unchanged nine state/throw/suspended-frame controls also pass after
the complete-payload correction. Log:
`/tmp/elephc-fiber-complete-error-native-retry-20261010.log` (2m17s compilation,
13.09s test execution). No-optimization, broader Async/copy, final owner-unit and
build outcomes remain pending until their separate logs complete.

The same ten native controls also pass **10/10 with IR optimization disabled**
(14.62s). Broader Async public/heap controls complete **148/148**, including the
new FiberError field probe and all previously red Fiber state/throw/drop cases
(94.17s). This is the focused macOS native filter, not a full suite, Linux/iOS
execution or review consensus. Copy controls and final fixture/build validation
remain separately queued.

Final-source Mixed/callable/Reflection copy controls complete **37/37** (14.81s).
The corrected `fiber_` unit filter completes **10/10** (0.12s after a 1m06s
compile), including complete Throwable initialization and the published resume/
throw owner proofs across all five supported targets. Log:
`/tmp/elephc-fiber-complete-error-unit-verified-20261010.log`. Kioku records the
validated layout correction as `bug_fix-construct-fiber-state-errors-with-the-complete-c-09f0511e`
and caller-owner correction as `bug_fix-fiber-guards-preserve-caller-published-operand-c-e6b6bf15`,
superseding their diagnosis packets. Assembly-comment checks and diff hygiene
pass. The CLI build is still running and its success is not assumed.

Next integration work remains the distinct lowering-module failures recorded
earlier (last broad replay 297/314; not rerun here). Three callable-flow cases
have focused passing evidence, but no new aggregate is inferred. In particular,
the corpus helper omits Async/Parallel injection present in the production
pipeline; ownership/effect/handler assertions must be checked against real IR
before changing expectations. Complete target execution, typed-array return/
remaining PHP-array ABI, internal I/O queue and exact-artifact review gates are
still open. Neither the second merge commit nor its fork push has been made.

The serialized validation queue completed with exit **0**. The fresh CLI
`cargo build -p elephc` passed in 3m05s without warnings or errors (log
`/tmp/elephc-fiber-complete-error-build-20261010.log`). Final diff hygiene and
assembly-comment alignment also pass. No owned build is left running. This
closes the complete FiberError payload and focused Fiber operand-owner checkpoint;
it does not close the distinct broad lowering failures or publication/review gates.

### Lowering harness and contract reconciliation, 2026-10-10

The prior goal turn made verified progress (complete FiberError payload and
native/unit controls), not merely a status restatement. Resumption confirms the
same dirty worktree and in-progress merge. Kioku recall returns the recent Fiber
checkpoint and scheduler guard decisions. The graph traces the corpus frontend
helper, but indexes only six files in the scoped test directory and omits the
container-parameter caller edge; exact source and retained failure logs provide
the missing evidence. No Kage call is used.

The unit frontend now injects Async then Parallel at the same point and with the
same extension eligibility as production. A new regression lowers both structured
surfaces independently on all five supported targets. The scoped lowering replay
completes **304/317**, with that regression and the complete example corpus green;
13 other assertions remain red. Log:
`/tmp/elephc-lowering-main-reconciliation-20261010.log` (88.16s execution).

Independent CLI EIR/assembly probes in
`/tmp/elephc-lowering-reconcile.henFXn/` establish these distinctions:

- Parameter privatization clones a Mixed cell, then the normal PHP binding store
  clones it again. The helper now passes the borrowed parameter to the standard
  binding store, retaining exactly one detached shadow and preserving container
  COW/by-reference exclusion. The unit requires one clone, direct shadow adoption,
  no premature producer release, and the unchanged by-ref return boundary.
- Reference argument evaluation now detaches the mutable Mixed cell before pinning
  its staged snapshot. The test verifies the clone, pin provenance and publication
  before the later reference mutation rather than an obsolete direct LoadRefCell pin.
- PHP catches were correctly pruned in scalar/quiet-destructor controls, but private
  `args.eval_cleanup` handlers remain. Tests now distinguish source catch dispatch
  from private cleanup, and positive destructor controls require an actual PHP catch.
- Exception handlers load/publish through x28/r14 context offsets, not process-global
  symbol strings. Tests require the context-specific assembly before setjmp.
- strlen uses a pinned cast result and its callee summaries conservatively carry
  WRITES_HEAP/REFCOUNT_OP. Tests retain exact ownership and effect assertions.
- Descriptor rejection now branches through argument construction before throwing.
  A CFG-aware test follows only rejection continuations, rejecting writes, cycles
  and normal escapes instead of demanding Throw in the initial guard block.
- Array property mutation assertions are scoped to the mutation method rather than
  counting unrelated generated methods. Boxed call arguments have explicit EIR
  roots; tests check distinct per-box roots, balanced source-pin republication and
  retirement order instead of former backend-frame offsets.

These edits are under fresh scoped validation; no passing outcome is assumed.
Production change is limited to removing the redundant parameter clone. The final
native copies/array/reference/scheduler regressions must be replayed against it.

The first reconciliation build caught an explicit-lifetime omission in the new
CFG test helper; it was corrected without production changes. The fresh scoped
retry then completes **316/317** (142.67s). All parameter-shadow, ref snapshot,
context-handler, destructor precision, effect and boxing-root controls pass.
The remaining failure is a second old direct-Throw assertion for invalid unpack
keys; the rejection guard itself already passes its CFG proof. That invalid-key
assertion now uses the same no-write/no-cycle/no-normal-escape traversal and
requires at least one invalid-key block, avoiding vacuous success.

The native commands following that failed scoped module did not execute. A final
serialized queue replays the entire lowering module, then native copies with PHP
cross-check, descriptor/ref/callable ownership, Async, Parallel and a fresh CLI
build. Its outcomes remain pending. No publication mutation has occurred.

Live PR inspection confirms #958 is open/draft at published SHA
516dea0b36250ec9cc83e09040cbe296a3defb57, still marked CONFLICTING while local merges
are unpublished. It has no review threads and only the successful PR-label
classifier check, not runtime CI acceptance. Local HEAD remains 027c82fa89bc;
MERGE_HEAD and origin/main are pinned at 9df6d3bd7ad0, with zero unresolved Git
stages. Preserve this distinction when publishing the validated follow-up.

The final scoped lowering replay is **317/317** (129.87s), with all 13 original
failures reconciled and the single-shadow clone invariant green on all targets.
No broader suite is implied. The live native queue then starts building copies.

Before calling that a PHP parity result, helper inspection reveals that
`compile_and_run_with_heap_debug` does not honor `ELEPHC_PHP_CHECK`; the ordinary
helpers only print mismatches when PHP succeeds and silently ignore unavailable
or failing PHP. Kioku records this evidence limitation as
`gotcha-heap-debug-copy-tests-do-not-run-the-advisory-ph-37b1f3e5`.

The copy group's helper now optionally executes its exact tagged source in an
isolated, exclusively-created temporary PHP file, requires successful PHP exit
and equal stdout, and removes that owned file before checking process results.
An executed-match marker supports an auditable count with `--show-output`. Missing
PHP, PHP fatal errors and mismatches fail the group; the global advisory helper
is intentionally unchanged. The native queue had already started compilation
before this test-only refinement, so a fresh strict copy replay is required after
it, rather than assuming that enabling the environment flag proved any oracle.

The fresh strict copy replay completes **37/37**, with exactly **37**
`PHP copy oracle verified` markers (19.71s). This is actual successful PHP
execution/output comparison, distinct from the earlier advisory/no-op flag run.

The neighboring descriptor/reference/callable-owner group completes **53/55**
(45.89s): two temporary closure-capture destructor controls fail with
`result|unreached|capture|` followed by an uncaught RuntimeException. That queue
stops before Async, Parallel or CLI build; no results for those steps are claimed.

Exact owned probes `iife_capture.php` and `descriptor_capture.php` in
`/tmp/elephc-lowering-reconcile.henFXn/` reproduce the same **exit 255** with the
previous CLI, which still has the double parameter clone. Thus the new single
parameter clone did not introduce these failures. PHP exits **0** for both and
prints `result|capture|caught|ok:result|capture|caught|ok`, also disproving the
historical tests' `capture|result` ordering as a PHP oracle.

EIR shows a cloned by-value capture stored in a hidden `#capture` PhpLocal; the
descriptor retains the same cell, but its creator's slot remains live until frame
exit. This extra owner delays the captured object's destructor beyond the source
catch. The backend confirms it increments the borrowed capture rather than cloning
the cell again. Initial correction now retires snapshot slots immediately after
ClosureNew, using the descriptor's retained-owner proof for non-final cleanup;
pending static metadata is published after capture-view cleanup. No new opcode or
descriptor ABI is needed for this owner transfer.

A new five-target unit asserts creator-slot retirement after construction, and a
new strict native/PHP copy fixture replaces a captured callable in a later argument
to expose whether direct static calls protect descriptor ownership themselves.
The focused snapshot-unit/copy/capture-destructor queue is running. It is not a
claimed complete fix: retained-callable lifetime and PHP-observable discarded-result
destructor ordering must both be verified, then reconciled before publication.

The first snapshot-retirement unit filter completes **1/3**: the validator
correctly rejects the proposed narrowed effects on ReleaseLocalSlot. That opcode
requires its canonical conservative cleanup contract even where this site has a
retained-owner proof. The implementation and new unit now preserve default effects;
no validator weakening is introduced. The pending callable hint is still published
after retirement, so conservative cleanup cannot erase its newly constructed binding.
The retry is serialized and native steps did not run after that unit failure.

The corrected snapshot-owner unit filter passes **3/3**, including all five-target
construction/retirement paths. Strict copy replay then completes **37/38**: all
existing native/PHP controls pass, while the new captured-callable replacement
probe returns a fatal TypeError (`string, null returned`) instead of `old`.
This is why removing the compensating creator owner is not accepted alone.
The old capture-destructor tests did not execute after this new regression failed.

Direct tracked closure variable calls now independently retain and publish their
selected descriptor before argument evaluation, while retaining static dispatch.
Generic descriptor calls also snapshot/retain a borrowed callback before arguments
can replace its local or mutable cell. Result staging remains outside the callback
owner for unwind-safe retirement. These corrections are under a fresh strict-copy,
capture-destructor and lowering queue; no passing outcome is asserted yet.

The retained selected-descriptor correction passes **38/38** strict native/PHP
copy tests, with **38** executed oracle markers (15.15s). The new argument-side
replacement returns `old` and leaves a clean heap. Independently PHP and the
previous CLI also return `old` for `captured_replace.php`; the old extra creator
owner had been masking the missing descriptor lease.

The two capture-destructor tests now pass their historical expected output
(2/2, 2.36s), showing capture retirement is back inside the source catch. That is
not PHP ordering acceptance: their historical `capture|result` order contradicts
the exact PHP probes' `result|capture`. Expectations and Rustdoc now require the
PHP-observed order. A fresh replay must expose and then fix this remaining
discarded-result/callback retirement ordering rather than preserving a misleading
green check. The broader lowering replay after descriptor pinning remains live.

### Direct-call eligibility and discarded result ordering, 2026-10-11

The interrupted turn ended with a terminal lowering replay **314/318**: all four
failures panic at the newly added `expect("a tracked closure lowers directly")`.
Captured references already have a supported descriptor path; being tracked is
not evidence that the static direct path is eligible. The protected branch now
consults the existing `static_callable_call_lowers_directly` predicate BEFORE
emitting owner bookkeeping. Existing ref-detach/loop cases and corpus pass again:
**318/318**, 87.33s, log `/tmp/elephc-capture-direct-eligibility-lowering-20261011.log`.

The ownership/order implementation now marks the exact result depth discarded
by an expression statement, including full speculative snapshot/restore. Callback
cleanup can release that staged result BEFORE its callback owner while keeping
the emptied outer result record published. The latter preserves strict LIFO and
exception cleanup if either destructor throws. A non-owning null sentinel prevents
the enclosing statement from releasing the already retired SSA result again.
Nested operands and used results do not match that depth and retain their ordinary
result staging. Static, dynamic, immediate and bound closure invocation use the
shared helper after argument/container cleanup and before callback retirement.

The first native build caught use of a nonexistent PhpType::Null; null expressions
use PhpType::Void/I64 in this compiler, so the helper follows the canonical lowering.
This compile failure is retained in its distinct log; it is not a runtime outcome.
An independent `used_result_capture.php` probe prints **capture|result|caught** in
PHP, contrasting with **result|capture|caught** for directly discarded results.
A new strict copy regression pins that nested results are not discarded early.
Fresh native ordering, strict copies and full lowering-module validation are queued
serially; none of those new outcomes is claimed yet. No commit/push occurred.

After the result-slot assertion was updated, the whole lowering module passes
**318/318**, 88.42s. The neighboring native group then completes **54/55**:
`test_core_closure_call_releases_its_bound_descriptor_on_success_and_on_a_throw`
panics on Void versus Mixed result staging. RUST_BACKTRACE identifies the outer
`lower_closure_bind_method` staging an internal descriptor invocation's result.
That inner value is used by the outer cleanup layer, even though both lowering
steps share the enclosing PHP expression depth. It must not be discarded there.

The `Closure::call` implementation now temporarily suspends the discarded-result
marker around that internal invocation, restores it before outer staging/cleanup,
and applies ignored-result cleanup only to the final outer result. This preserves
the layering/type contract rather than disabling the staging assertion. The new
binding/strict-copy/full-lowering/Async/Parallel/build queue is live; outcomes are
pending and no publication mutation has occurred.

The outer-staging correction's neighboring native group now passes **55/55**
(44.83s), including the formerly failing Closure::call control, descriptor
binding, by-reference variadics and callable-owner cleanup. This is focused host
evidence. Its strict-copy, lowering, Async/Parallel and CLI build steps continue
serially on the same production-source state; their outcomes remain pending.

The two native discarded-result destructor controls now pass **2/2** with the
PHP-observed **result|capture|caught** ordering and clean heaps (2.75s).
Strict copies pass **39/39**, with **39** executed PHP comparison markers
(11.92s), including selected-callable replacement and the nested-used-result
counterexample. No discard-mode widening to nested arguments was introduced.

The whole lowering module completes **317/318** (86.54s): one older descriptor
string test still expects a direct SSA Release. Directly discarded results are
now adopted by a published staging slot and retired through ReleaseLocalSlot.
That test now requires one exact adoption slot, one retirement, no second SSA
release, plus five-target assembly generation. The final lowering/binding/
Async/Parallel/build queue is running; those outcomes are pending.

### Final host checkpoint and remaining Parallel blockers, 2026-10-11

The bound-call layering correction completes the focused host checkpoints:
**55/55** descriptor/reference/callable-owner controls (44.83s), **39/39** strict
native/PHP copy comparisons with **39** executed markers (17.59s), **318/318**
whole lowering-module tests (91.58s), and **148/148** Async public/heap controls
(69.56s). Logs use `/tmp/elephc-bound-outer-staging-*-20261011.log`. These validate
the current production source, not the entire supported runtime matrix.

Parallel completes **51/54**, 59.30s, and stops the serialized queue before the
CLI build. Its three exact blockers are:

1. `parallel_parent_scope_rejects_fiber_suspend_through_builtin_callbacks`:
   codegen rejects `array_filter` with `expected 3 args, got 2`. The source uses
   valid omitted-default/named callback forms; shared normalization/lowering must
   satisfy the runtime operation contract rather than padding the fixture.
2. `parallel_worker_resource_ids_remain_unique_under_concurrent_stream_use`:
   checker reports undefined `$ids` at the append inside a worker loop. The exact
   source initializes `$ids = []` before that loop. Preserve the seven workers,
   sixteen streams each and 112-unique-id assertion; diagnose compiler binding/
   optimization instead of weakening this concurrency coverage.
3. `parallel_zero_job_id_drains_as_an_infrastructure_failure`: fatal private
   `Future::$jobId` access. The source already intentionally uses
   `ReflectionProperty::setValue`, so the initial thought that it used an obsolete
   direct property write was wrong. Preserve Future privacy and reflection's
   authorized test route; diagnose reflection lowering/access behavior.

The queue is terminal with exit 101 and no owned heavy build remains live. Diff
hygiene and touched assembly-comment alignment pass, but the fresh CLI build did
not run. No merge commit/push or new accepted review artifact has been created.
Continue with these genuine compiler defects, then replay Parallel and the other
needed gates. The larger PHP-array ABI, internal I/O, target and absolute-consensus
requirements remain intact; this checkpoint is not completion or a global blocker.

### Parallel defaults and reflected construction, 2026-10-11

The prior turn made verified source/test progress. Kioku recall returns the final
host checkpoint and the three exact Parallel blockers. The callback-special builtin
argument path exits before the ordinary MaterializeDefaults strategy. Its first
attempt consulted regular slots of CallArgPlan, but positional plans intentionally
use passthrough_args and leave regular_args empty; temporary diagnostics proved
args=2, params=3 and regular_count=0. The diagnostics have been removed.

Positional default materialization is now one shared helper reused by ordinary
calls and guarded callbacks, driven by the builtin's existing semantic strategy.
The new profiled/unprofiled filter regression passes **1/1** across Async/Parallel
and all five targets. The unchanged native builtin callback-suspension guard also
passes, without padding its source or altering evaluation order.

The concurrent resource worker body passes --check standalone and in a non-Parallel
loop of closures, but fails with the original Parallel::spawn construction. Its
112-unique-id workload remains unchanged. A temporary trace at missing array pushes
did not fire, so the undefined read is elsewhere; that trace was removed and a
temporary variable-inference trace is queued to locate the actual checker caller.

Reflection's inline factory previously retained concrete type only when the reflector
was assigned to a variable. Constructor target resolution is now shared by assignment
and inline inference. Native checks still failed private writes, proving inference
alone insufficient: EIR construction unconditionally returns a boxed Mixed result.
Known reflected construction now extracts a retained concrete object through the
existing heap-conversion boundary and retires its owning source box. Dynamic targets
retain their Mixed path. A new strict-copy regression preserves private reflection
authority; the Future field is not reopened and its ReflectionProperty setter is
not replaced by direct access.

The first build caught the changed constructorless lowering signature at direct/
nullable callers. Both callers now pass their available source expression (Some)
or absent static information (None). A focused Reflection retry is live, followed
by the resource checker trace. No Reflection fix result, resource fix result,
fresh complete Parallel pass, commit or push is claimed yet. Owned probes live in
`/tmp/elephc-parallel-reconcile.2xczOZ/` and logs retain each failed/successful step.

The constructor alignment now passes the zero-job infrastructure test and produces
`7` through the new private Reflection setter regression; the latter still fails
its unchanged heap assertion with 49 live blocks, rooted by a temporary reflector.
Early reflected-construction/property-access paths now stabilize the result and
retire owning receivers through the existing common helper. This is under replay,
not a claimed heap fix yet.

The variable-inference trace locates the actual `$ids` failure in
require_parallel_task_return_transferable re-inferencing a returned worker local in
its parent's environment, including speculative outer-loop storage scans. The
temporary trace is removed. Flow-typed returns now also record reference-array
alias risk while the worker's local scope is active. Parallel return validation
uses that exact checked type when transfer-safe, rejects recorded reference-array
risk, and retains recursive literal validation for rejected/erased shapes. A new
negative test requires refusal of an aliased typed worker-local array return;
object/resource/cycle refusals are not relaxed.

The three original Parallel regressions, strict copies and complete Parallel error
filter are running serially on these changes. Preserve all seven workers and 112
unique resource IDs. No diagnostics remain in production source. No fresh overall
pass, build, commit/push or exact-artifact consensus is asserted until completed.

### Verified three-regression checkpoint and clone follow-up, 2026-10-11

The next lifetime retry exposed a double retirement: lower_property_get_from_value
already consumes an owning ReflectionClass receiver while stabilizing __name.
The extra outer factory retirement was removed, and class-name temporaries are
published across allocation/constructor arguments and retired afterwards. The
ReflectionProperty fast path retires its own unused reflector separately.

The unchanged three original Parallel regressions now pass **3/3**, 27.47s:
callback guard/defaults, all 112 concurrent resource IDs and the zero-job
infrastructure drain. Strict copy/PHP/heap controls pass **40/40** with **40**
executed oracle markers, 17.84s, including the inline private reflection setter.
Logs: `/tmp/elephc-parallel-three-lifetime-{retry,copies}-20261011.log`.

The Parallel error filter completed **79/82**, exposing a too-early successful
signature return in transfer validation plus two private-clone compile expectations.
The early return is now restricted away from closure bodies, so the recorded
reference-array return risk is always inspected. The new aliased worker-local
array refusal passes **1/1**, 0.35s, in
`/tmp/elephc-worker-local-return-alias-refusal-20261011.log`.

Private clone validation has moved to typed runtime CloneWith on main; the checker
currently accepts the expression. A new heap-debug native guard test is retained
before changing either existing compile expectation. Its initial fixture reused one
local for TaskGroup and Future, which the checker rejected; distinct duplicate
locals remove that unrelated fixture issue. The retry still fails codegen with
`array_push for PHP type Void`, so runtime clone refusal is NOT established. PHP
independently confirms a private __clone is a catchable runtime Error. Preserve
scope/job uniqueness and investigate the new codegen failure rather than deleting
the clone gates or claiming them accepted.

All owned validation queues above are terminal. No production diagnostic remains,
no complete Parallel/error/lowering/target replay or fresh CLI build has been
claimed for this final source, and no merge commit/push occurred. Next work starts
with the private-clone fixture's Void array-push path and complete refusal replay;
full PHP-array ABI/I/O/target/review requirements remain active.

### Null append result and private clone allocation ordering, 2026-10-11

The null-only indexed array regression passes natively **1/1**, including the
strict executed PHP oracle and heap-debug cleanliness, in
`/tmp/elephc-null-append-native-20261011.log`. The corresponding all-five-target
emitter test passed **1/1** before a final ARM64 receiver-load ordering adjustment;
that final source still needs replay. Void call results append PHP null rather
than being rejected by codegen, and null element tags remain observable.

The private-clone fixture now compiles and exposes a real ownership failure:
it prints `group|`, then spawning through the original TaskGroup fails with
`cannot spawn after cancellation has been requested`. Log:
`/tmp/elephc-private-clone-null-append-20261011.log`. CloneWith allocated and rooted
a shallow duplicate BEFORE its invocation-scope visibility check; unwinding that
refusal destroyed the duplicate and cancelled state shared with the original.
The PHP probe for a private __clone plus visible destructor prints exactly
`caught|alive|destruct|`, proving no duplicate destructor should run on refusal.

The AOT hook access check is now emitted against the borrowed source before the
shallow-copy adapter. Eval retains its identity callback path first. The existing
shared lexical/runtime invocation-scope guard is reused, not replaced by a blanket
permission, and successful copies keep their unwind root while executing hooks.
New native heap-debug and all-five-target ordering regressions are added; replay
is pending. The original two Parallel compile-time refusal tests remain intact
until the runtime refusal and original scope/job lifetime are verified.

No second merge commit, push, complete target acceptance or review consensus is
claimed by this checkpoint. Kioku semantic recall served the preceding checkpoint,
but fresh clone code is missing from its symbol index and historical scratch copies
pollute symbol results; use the exact worktree source for this correction.

### Private clone fixed; checked return contract reconciliation, 2026-10-11

The new ordinary private-hook destructor regression passes **1/1**, 0.75s,
heap clean (`/tmp/elephc-clone-private-preallocation-20261011.log`). The Parallel
combined TaskGroup/Future clone regression also passes with unchanged output
`group|future|7` and clean heap. Runtime refusal now preserves both original
owners; the two obsolete compile-time-only clone expectations were replaced by
this catchable runtime/lifetime gate, not dropped without execution evidence.

The first full Parallel replay was **48/55**. Seven failures came from separately
validated scalar return contracts being re-inferred against parent storage and
callable state, after the local alias check was introduced. The checked return
contract is now accepted AFTER all recorded worker-local alias risks have been
examined. The replay passes **55/55**, 45.18s, in
`/tmp/elephc-parallel-return-boundary-replay-20261011.log`.

A further source change closes the corresponding tracked-closure-variable bypass:
a variable with tracked closure captures must have the cached safe-return proof;
its declared transferable signature alone cannot override a failed local alias
proof. A new negative fixture exercises the aliased worker-local array through
such a variable. This final change is under the serialized error/lowering/native/
strict-PHP-copy/ordinary-cloning/Async/no-IR/build replay. Do not apply the preceding
55/55 result to it before that replay completes.

The first final error replay is **80/81**: the new tracked-closure refusal worked,
but its custom diagnostic changed an existing hidden-object expectation. It now
reuses the shared Mixed/unknown-shape diagnostic. The next error replay is again
**80/81**, this time exposing the original inline aliased-array refusal as Ok.
Neither failure is an accepted checkpoint. Inspection of method inference confirms
that named/spread argument normalization CLONES the task AST before Parallel safety
validation, while return-site proofs are keyed on the checked statement address.
The exact normalized closure is now type-checked in its own local-storage scope
before transfer validation. This preserves local alias facts without an unreliable
cross-AST address hit or a parent-environment re-inference. Full replay is pending.

### Exact normalized-task replay and broader clone controls, 2026-10-11

On that source the final focused results are **81/81 Parallel errors**, **321/321
lowering**, **55/55 native Parallel**, **41/41 strict PHP copy oracles** (41 actual
markers), **148/148 Async**, and **1/1 Parallel private-clone guard with IR opt off**.
The CLI build completed without warnings. Logs use the final-* names under /tmp;
errors use `elephc-parallel-final-errors-normalized-20261011.log`.

The wider ordinary object-cloning control is **76/81**, not green. Two failures
accept a non-array Mixed override and run the hook; three concern foreach reference
detachment/COW identity. Exact failed assertions are retained in
`/tmp/elephc-scheduler-final-cloning-20261011.log`. The first reducer's EIR confirms
an eager MixedUnbox to array<mixed> before CloneWith, so its runtime tag guard never
sees the original scalar. Reproducer:
`/tmp/elephc-clone-reconcile.b6mG9w/mixed-overrides.php`.

CloneWith now selects the existing PreserveValues argument-lowering strategy in
the central semantic descriptor, retaining public signatures and the shared planner
but allowing runtime validation of original values before allocation. A new EIR/
emitter test requires the override operand to remain Mixed on all five targets.
Full cloning replay and the update-builtin-docs skill's exporter/regeneration/audits
are queued serially. Those results are pending; the prior focused passes/build are
historical for this new semantic-descriptor edit. No new commit or push occurred.

The PreserveValues cloning replay is **78/81**, 42.62s:
`/tmp/elephc-clone-preserved-values-replay-20261011.log`. Both Mixed override
validation regressions now pass. All five original expected outputs were
independently confirmed by running the unchanged fixtures with PHP **8.5.10**.
Three foreach/reference-COW cases remain red; no test expectation was relaxed.

The added emitter fixture initially treated block instruction IDs as instructions,
which failed Rust compilation. It now reads the function's actual instruction
arena; retry and generated-docs checks are live in owned session **55052**.
Logs: `elephc-clone-preserved-values-emitter-retry`, `-exporter`, `-docs` under
/tmp with the 20261011 suffix. Do not assert emitter or docs success before reading
their terminal results.

Two smaller native/PHP reducers in `/tmp/elephc-clone-reconcile.b6mG9w/` separate
the remaining lifetime failure. `foreach-unset.php` prints `{"u":1}` in both
Elephc and PHP: plain unset is not shown to corrupt the source. In contrast,
`clone-after-foreach-unset.php` prints native
`{"u":1}|TypeError:Cannot assign null to property RefBox::$u of type int|{"u":null}`
but PHP prints `{"u":1}|1|{"u":1}`. Corruption occurs while applying clone
overrides, with the reference cell/source entry still shared. Investigate generated
applicator by-value iteration, argument snapshots/cleanup and reference-cell
ownership before changing ordinary unset behavior. Merge/push remains withheld.

The corrected Mixed-override EIR/emitter regression now passes **1/1**, 0.29s,
on all five supported targets, in
`/tmp/elephc-clone-preserved-values-emitter-retry-20261011.log`. The serialized
owned session **55052** has advanced to the curl-enabled builtin exporter build;
regeneration and docs audits are not completed yet. Three native cloning/reference
regressions remain the next correction gate. Goal remains active, second merge
remains uncommitted, exact vendor recovery SHA remains available, and no push or
acceptance/review consensus is claimed.

### Live reference-cell ownership diagnosis and single-retirement fix, 2026-10-11

The curl-enabled exporter and the complete update-builtin-docs workflow completed
successfully. It exported 1185 builtins, 209 classes and 1117 constants, rendered
2254 pages, audited with **0 errors**, validated **2276** pages and found **0**
structural EIR inventory errors. Regeneration changed 741 internal pages plus the
builtin JSON registry: the added semantic-descriptor line moved shared lowering
source links by one line. Generated files are kept; they were not hand-edited.

The reference corruption also reproduces in an ordinary `mixed` parameter's
by-value foreach (`foreach-mixed-parameter.php`), without clone. It is unchanged
with IR optimization off or stack-only register allocation. The earlier conclusion
that plain unset was safe is **not established**: the first JSON print only read
freed storage before its reuse.

LLDB stopped twice at `_rt_hash_entry_deref` with `$x5 == 11`, first in the initial
JSON encoding and then in `values()`'s foreach. Both entries pointed at cell
`0x10004c338`. At the first stop, its header reported RC **0**, kind **0** (freed),
while payload `0x10004c310` still looked readable. At the second stop that same
address was a reused 24-byte Mixed allocation, RC **1**, kind **5**, containing
`[5, 0x10004c290, 0]` rather than a reference cell. This is live evidence of a
dangling hash reference after unset, not proof of a clone-only write.

`lower_unset_local` released every nonzero dynamic-state slot as an ordinary
promoted cell and then `unmark_promoted_ref_cell` released explicit counted state
again. A managed hash alias has state >1 and two legitimate owners (entry + local);
both decrements consumed the entry's owner. The ordinary-cell cleanup now applies
only to state **1**, on both architectures. State **0** skips it and state **>1**
retires once through the existing counted-state helper. No reference permission,
ownership contract or COW predicate is weakened.

New strict PHP/heap and five-target emitter regressions are added. The serialized
owned validation session **34725** runs the new native reducer, full ordinary
cloning controls and the new emitter gate. Logs use the
`/tmp/elephc-foreach-alias-single-retire-{native,cloning,emitter}-20261011.log`
names. These results are pending; no merge commit/push or final acceptance is
claimed. The preceding goal turn made verified implementation/test progress.

### Single-retirement verification and current native Linux probes, 2026-10-11

The new strict PHP/heap allocation-reuse regression passes **1/1**, 1.79s, with its
actual oracle marker and clean heap. Full ordinary cloning passes **81/81**,
40.50s; all three formerly failing reference/COW fixtures are now green. The
all-five-target state-discrimination emitter test passes **1/1**, 0.15s.
Logs: `elephc-foreach-alias-single-retire-{native,cloning,emitter}-20261011.log`.

The final-source native replays pass **42/42 strict copy/PHP/heap controls** with
**42** executed oracle markers, **148/148 Async** and **55/55 Parallel**. The
no-IR new regression and CLI build are still the last queued checkpoints; inspect
their terminal logs before claiming them. All selected assembly comments align
and `git diff --check` passes.

For Linux, a task-owned standalone emitter was compiled against the CURRENT
default-feature libelephc artifact (`libelephc-29f03d77e3be40dd.rlib`, timestamp
2026-10-11T13:50:12) using host Rust 1.95.0. It calls the actual public
`generate_runtime_with_features_mode` with executable boundary, no PIC,
RuntimeFeatures::none() plus object_clone. The same current compiler
(`target/debug/elephc`, timestamp 13:51:36) emits all three PHP programs through
AST/EIR for each Linux target, with heap debug enabled. No mock runtime, foreign
bridge archive, Cargo target volume or target-only fallback was substituted.

Both matching existing Docker images assembled and linked the fresh user/runtime
assembly using cc, libm and pthread, with one CPU/512 MiB/--rm. All three programs
exited successfully with clean heap on **Linux AArch64 and Linux x86_64**. All six
stdout files were independently compared byte-for-byte with their PHP 8.5.10
source executions using diff; every comparison succeeded. Bundles:
`/tmp/elephc-clone-reconcile.b6mG9w/{linux-aarch64,linux-x86_64}`.
Logs: `/tmp/elephc-reference-linux-{arm64,x86}-20261011.log`.
The x86 image reports Rust 1.98.1, ARM64 image 1.95.0; neither runs Cargo here.
An unrelated Docker review build was left untouched.

SHA-256 provenance (runtime, Mixed foreach, clone after unset, clone reference COW):

- AArch64 runtime: ac8c070fea7f00872fb8659886cf8a4759d00acac27d7c1ea778957d76cfd80a
- AArch64 foreach: 477c34fa47318c7b30f767f60bd92058922d815707ba30336caeaf95b6a76e12
- AArch64 clone/unset: 760df09fdd903602837ec71fba100a734ec601c8c8fde51637617d07abc9e8f4
- AArch64 clone/COW: 38c4fc0faef1c69b432642535f67fb1869799f4d1fc7f08d595490ca65fbf897
- x86_64 runtime: 541d2354f0584decb6419b1274850e4161be20f1a3082398e703e6daeeaf5898
- x86_64 foreach: d5fab0d8caf3e0bc040ea4923eb7206906a3507178c6050f8b5cbdeaff9c8aae
- x86_64 clone/unset: 769675f1dcd3cb0b13f51d2733e45e8db4cc53c3a2cc36474e65d83151e31194
- x86_64 clone/COW: 3c825c6e6e301df9cf08959ca68960ae0249d2320d5e18e70779732c9f18dab7

This proves the scoped reference-retirement/clone/COW regressions on Linux, NOT
the Rust Parallel bridge, complete PHP-array ABI/I/O contract, full executable
matrix or iOS SDK/hosted behavior. Those whole-goal gates and exact-artifact
independent review consensus remain open. Live main is still 748bf44eef (only
stats ahead of MERGE_HEAD 9df6d3bd7a). No new commit/push is yet claimed.

### WIP merge publication readiness, 2026-10-11

Final-source lowering passes **323/323**, 228.74s, and Parallel refusal controls
pass **81/81**, 14.27s. The no-IR allocation-reuse regression passes **1/1**,
3.61s, and `cargo build -p elephc` completes without warnings. Together with the
current 81 cloning / 42 strict copies / 148 Async / 55 Parallel / two Linux
three-probe runs / five-target emitter controls / docs audits / hygiene checks,
this reconciles the relevant red integration regressions for the authorized WIP
checkpoint. It is NOT full array/I/O/target/review acceptance.

Publication is authorized by the maintainer's tracking exception documented in
execution-scheduler-publication.md. Stage only the incoming merge, owned source/
tests/plans and generated builtin links; exclude .codex, .kioku, .mcp.json,
scratchpad and the seven inherited vendor deletions. Commit the current ordinary
merge, then integrate the exact current stats-only main 748bf44eef normally.
Restore the inherited vendor state by exact recovery SHA after merge commits,
verify that it is not in the index, push the same fork branch without force and
read back PR #958's exact head/draft/CI state. No new commit/push is asserted by
this preparation entry; record their actual outcomes separately.

### Merge commits and inherited-state restoration, 2026-10-11

The second ordinary merge is committed as
**172338a8929c46b1de7b7dec53c7c56efb5a2dd7**, with parents 027c82fa89 and
9df6d3bd7a. It contains the incoming loop/monitor changes, owned regression fixes,
new leaves/tests and generated source-link updates (837 files, 6561 insertions,
2211 deletions). No excluded tool configuration, Kioku packet, scratch artifact
or inherited vendor deletion was staged.

The live stats-only main was then merged normally as
**b669488089b508c3ccf0a9a075b4a9da57def95f**, with parents 172338a892 and
748bf44eef. Exactly the four expected .github/traffic files changed; src, crates,
tests and Cargo files are byte-identical to the tested integration commit.
748bf44eef is an ancestor of the current branch. No force/rebase was used.

Recovery stash 46b10c28d5f6c08e10fa3fb8a78f317dde039fdc was applied by exact SHA
without popping/removing it. Its seven inherited vendor deletions are restored
as **unstaged** local changes. A diff of their path set against the original
stash change set matches exactly, and the index is empty. They remain outside
publication and recoverable from tracked Git versions. Local .codex, .kioku,
.mcp.json and scratchpad remain untracked and unpublished. Push/PR readback is
the next action, not an asserted outcome here.

### Published exact head and live CI readback, 2026-10-11

The docs-only handoff commit is **f0d4e7dfed2d01dee2e352c2292e86beb8c57281**.
A normal push to fork/spike/runtime-ctx-register succeeded from 516dea0b36 to
that exact SHA. GitHub PR #958 reports the same exact head, **draft=true** and
**MERGEABLE**, replacing its previous conflicting state. Tracking comment:
https://github.com/illegalstudio/elephc/pull/958#issuecomment-6108933034 .

Actual CI now runs on this head, including architecture build/archive jobs,
managed packages, iOS curl/XML checks, frame assertions and PDO live drivers.
The fresh readback shows Builtins docs, RustSec, Action pins, image stubs and PR
classification successful; architecture/PDO/full runtime jobs are still active or
queued. CI run **38138507612** is the exact-head compiler workflow. Do not replace
those pending results with the earlier label-classifier-only success or claim
complete CI. This post-push log remains local pending the next work commit, to
avoid restarting the current CI solely for bookkeeping.

The goal remains active with complete PHP-array ABI/consumer/ref semantics,
scoped I/O queue/provider work, remaining complete target evidence and dedicated
Codex/Kimi/GLM/DeepSeek exact-artifact consensus still required. Heartbeat remains
discussion-only. Vendor deletions stay unstaged and excluded; the source tested
above is exactly the published source despite the stats/docs-only follow-ups.

### First exact-head CI failure and retained array-return blocker, 2026-10-11

On published head f0d4e7dfed, CI Curl feature coverage completes **6/7** cross-
backend parity tests. Catalog coverage and Magician eval bindings pass, but
`non_registry_surfaces_have_complete_backend_contracts` expects 409 exceptional
surfaces and observes 410. The completed job's direct logs were fetched through
the jobs API (the run-level gh log interface refuses an unfinished workflow).
Exact job: **114467274289**; log:
`/tmp/elephc-pr958-curl-parity-job-20261011.log`.
The same local curl-enabled parity command reproduces **6/7**.

The catalog diff against origin/main identifies the exact additional surface:
`elephc\\async\\run`, a PreludeProvided Elephc extension with one Callable body
and Mixed result. No missing/duplicate runtime binding is established by this
count-only failure. The test now expects 410 total / 347 prelude surfaces and
explicitly requires this precise Async contract, its extension marker and its
signature. Language constructs, dedicated syntax, transforms, rewrites and zero
unsupported-route checks are unchanged. Curl/default parity and curl compiler-
prelude parity are under serialized replay; no new success/commit/push is claimed.

The remaining approved PHP-array transport gate is freshly reproduced against
the published compiler: `declared-array-forwarding.php` in the reducer directory
packages named variadic args in a mixed frame and returns `$frame['args']` from a
declared array function. PHP 8.5.10 prints `{"label":42}`; Elephc --check reports
`Function 'forwardedArgs' return type expects array, got mixed`. This is an open
full-array contract blocker, not subsumed by the green reference-retirement tests.
The goal remains active and the PR remains draft, not accepted.

The reconciled parity checks now pass **7/7 with curl**, **7/7 without curl** and
**7/7 compiler prelude parity with curl**. Logs:
`/tmp/elephc-pr958-{curl-parity-reconciled,default-parity-reconciled,curl-prelude-parity}-20261011.log`.
Only the test's audited inventory and explicit Async contract assertions changed;
the implementation/catalog/public signature are unchanged, so the already-green
generated-docs result remains applicable. Hygiene checks pass. The follow-up
commit/push will supersede f0d4e7dfed for CI observation; retain its failed run as
historical evidence, not success on the new head.
