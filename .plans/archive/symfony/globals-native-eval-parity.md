# Global symbol-table parity audit

Status: element-access implementation in progress; callback synchronization and
SAPI hydration remain open. This is not a claim of
complete PHP compatibility or of the requested final multi-model consensus.

## Normative baseline

PHP 8.5.6, php-src commit fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633.
Zend/zend_compile.c, especially is_global_var_fetch, zend_delayed_compile_dim,
zend_compile_assign and zend_ensure_writable_variable, treats a GLOBALS element
as access to a global variable, not an ordinary local array. The key is evaluated
and converted to a string. Whole-array write/reference restrictions are distinct.
Source inspected at the pinned commit, not a moving branch.

## Confirmed gaps

1. AOT supports literal identifier keys through src/globals_array.rs internal
   aliases and explicitly refuses bare GLOBALS, computed root keys and keys that
   cannot be PHP identifiers. Those refusals are compatibility limits, not proof
   of complete GLOBALS support.
2. Magician recognizes GLOBALS only as an arrow-superglobal name and in a parser
   fixture. It evaluates the syntax as an ordinary missing local variable. The
   full request's eight warnings occur in a bound closure's once-per-file map.
   No application/library name belongs in the fix.
3. A live context.global_scope_ptr is not the same as live native global storage.
   Explicit eval/include flush AOT globals into the eval scope and reload them
   afterward. Later eval-backed callable invocation captures context/callback and
   calls __elephc_eval_callable_call_array without that native synchronization.
   Arrays can appear updated through shared payload mutation while replaced
   scalar cells do not reach native _eir_global_* storage.

## Executed native reducers

tests/codegen/runtime_gc/eval_globals.rs uses a dynamically included closure
factory, a bound closure with a per-key once guard, and an included counter.
Native code reads the count/map, unsets the marker, then invokes the closure again.

| Route | Expected | Observed |
| --- | --- | --- |
| GLOBALS in evaluated factory and unit | 1:seen:2 | 0:missing:0 |
| global keyword in factory and unit | 1:seen:2 | 0:seen:0 |

An earlier directly compiled-closure version returned 0:seen:0 with GLOBALS in
the dynamically included unit. The opaque factory prevents the guard itself
from remaining on the already-supported AOT route.

Luna performed a bounded read-only audit. Its first conclusion attributed the
failure only to missing GLOBALS syntax. The executed global-keyword control
contradicted that scope; after review it confirmed the separate native callback
import/export gap. Neither audit used builds or edited files. This is diagnostic
agreement between root and one reviewer, not final GLM/Kimi consensus.

## Relevant edges

- src/codegen/lower_inst/builtins/eval/calls.rs: explicit eval/include barriers.
- src/codegen/lower_inst/builtins/eval/sync_inventory.rs: eligible native globals.
- src/codegen/lower_inst/builtins/eval/scope_io.rs: native import/export ownership.
- src/codegen/lower_inst/callables.rs: owned eval callable descriptor environment.
- src/codegen/eval_callable_helpers.rs: native-to-eval descriptor invoker.
- crates/elephc-magician/src/ffi/callables.rs: invocation with the captured context.
- crates/elephc-magician/src/interpreter/scope_cells.rs: existing global aliases.
- crates/elephc-magician/src/context/reference_metadata.rs: reusable reference targets.

## Required test/implementation envelope

- Direct global reads/writes must not shadow or overwrite a same-named local.
- Nested indexed mutation and first-write autovivification must use actual global
  storage; do not inject a fake empty GLOBALS array or a private closure snapshot.
- Native-to-eval calls must import current native globals and export changes on
  normal/error paths, preserving references, COW and cell ownership.
- Also inspect nested eval-to-native calls and foreign eval contexts: mutations
  must be visible at each observable boundary, not only at the outer return.
- Test native mutation/unset between callback invocations, scalar replacement,
  array replacement versus in-place mutation, and reference aliases.
- Test isset/empty/coalescing quiet reads, missing-key warnings, unset, and
  by-reference argument writes separately.
- Audit computed keys, whole-array snapshots and forbidden whole-array writes
  against PHP before claiming the entire GLOBALS surface complete.
- Request reset must isolate later web requests while retaining globals for the
  whole current request. Every generated ABI path must cover all five targets.
- A successful map/count reducer is not stock-Symfony HTTP acceptance. The final
  goal remains the unchanged application's standard welcome page with HTTP 404.

## Implemented checkpoint

The new interpreter/globals.rs resolves GLOBALS element names once, reads from
the context's actual global scope with independent value ownership, and writes
or unsets that global slot. A GlobalVariable writable location preserves local
shadowing and nested first-write behavior. No local GLOBALS array is synthesized.
Direct isset/empty use quiet global lookup. Three focused interpreter tests now
pass; 36 control_flow and seven null_coalesce_assign tests pass too.

The native test test_eval_globals_explicit_barrier_roundtrips_scalar passes with
7:absent after an evaluated global scalar assignment and unset. This verifies
the already-existing explicit eval barrier, not later callback synchronization.
Native callback barriers, references, whole-array reads, non-UTF8 names, and
complete normal/error-path ownership remain open. Nested quiet array reads still
need temporary-owner audit; do not infer heap closure from the semantic tests.

## CLI/HTTP hydration comparison requested by the user

Normative sources additionally inspected at the same pinned commit:
main/php_variables.c (php_hash_environment, php_build_argv, auto-global callbacks
and registration) and sapi/cli/php_cli.c (request setup and forced SERVER creation).
The installed oracle reports PHP 8.5.6. Probes print only shape/configuration and
an explicitly supplied sentinel, not actual environment values.

- PHP resets http_globals, registers CLI argv/argc from SAPI arguments, then
  activates auto-globals. CLI explicitly triggers SERVER after request startup.
  A php -n probe reported register_argc_argv zero while global argc/argv still
  existed; that INI setting must not be treated as disabling CLI's globals.
- GET/POST/COOKIE/FILES registration differs from lazy SERVER/ENV/REQUEST.
  Direct mentions can trigger initialization while PHP compiles the source.
  A GLOBALS-only ENV lookup returned null under the probe, while direct ENV
  access saw the provided sentinel. Do not indiscriminately hydrate every array.
- Elephc CLI source-name seeding currently injects empty associative arrays for
  referenced CLI superglobals. Direct ENV sentinel probe: PHP printed fixture,
  Elephc printed missing. This is a confirmed hydration gap.
- CLI GLOBALS argv probe: PHP printed 3:3 for two supplied arguments; Elephc
  failed count() with true given. globals_alias_type treats argv/argc as Mixed,
  while frame startup writes raw Array<Str>/Int to their global slots. The source
  establishes a producer/consumer representation mismatch; test_cli_globals_argv_and_argc_match_process_arguments
  is red (CLI binary exits with error). Do not patch by assuming globals are immutable.
- HTTP source inspection shows REQUEST merged from GET then POST, always an
  empty SESSION initialized by the prelude, eager ENV copying from a worker cache,
  and manual query/body parsing. Compare these against request_order,
  variables_order, session auto-start, method restrictions, nested names,
  plus decoding, duplicate cookies and per-request environment restoration.
  These are audit items, not a blanket claim that HTTP hydration has been tested.

Probe artifacts: target/globals-hydration-probe/{main,argv,env}.php and their
separate native output directories. The combined probe aborts at argv count;
do not infer its earlier entries were correct from absent JSON output.

The installed PHP oracle is 8.5.6. An isolated direct ENV read with the same
process sentinel printed fixture in PHP and missing in Elephc. A source mentioning
ENV/REQUEST directly later in the file already had them in its initial GLOBALS
key snapshot: compiler-driven auto-global activation must be accounted for when
designing eager/lazy tests. GLOBALS-only ENV access did not produce that effect.

### Process argument representation fix verified

frame.rs now publishes argc and argv as boxed Mixed globals, matching the EIR
global alias contract. The eval synchronization inventory uses that same type
for its fallback process-argument entries. Boxing argv retains its payload; the
builder's original array reference is then released with the existing shared
runtime helper. This does not constrain later PHP writes to their initial types.

Focused native tests passed (two tests, 10.67 seconds):
- test_cli_globals_argv_and_argc_match_process_arguments: 1:1.
- test_cli_globals_process_arguments_allow_type_changes_across_eval:
  1:1:changed:42, matching an executed PHP 8.5.6 oracle.

The earlier red argv checkpoint above is superseded for these cases only.
Cross-target execution, complete ownership closure, CLI ENV hydration and native
callback synchronization remain open. target/release/elephc predates this fix;
the tests used the freshly built debug CLI. No full application replay performed.

## Nested native boundary: executed red checkpoint

test_eval_globals_nested_native_call_observes_each_boundary starts a native
global counter at 1, changes it to 7 in eval, calls a native function which prints
the counter and changes it to 9, then prints from eval and the outer native frame.
PHP 8.5.6 was executed and returned native:7:eval:9:outer:9.
The current debug CLI returned native:1:eval:7:outer:7 (25.18 seconds).
This proves both missing directions: eval's replacement is invisible to the
native callee, and the callee's replacement is invisible on eval resumption.

The two bound-closure tests were also rerun after the process-argument fix:
global-keyword control still returned 0:seen:0; GLOBALS returned 0:missing:0.
These failures are not closed by correcting argc/argv initialization.

Implementation direction from root and bounded read-only Luna audit:
- Generate module-scoped synchronization hooks reusing the existing inventory,
  flush and reload semantics, with an explicit scope argument and own ABI frame.
- Attach hooks during module/context metadata registration, preserving the fast
  metadata-clone path and fallback context initialization.
- Invoke both directions around native function execution and eval-backed
  callable invocation, before reference writeback and error propagation.
- Audit native method and conversion callback paths, not only named functions.
- Do not restore an old snapshot of PHP globals when crossing eval contexts:
  contexts in one request must observe legitimate shared global mutations.
  The reviewer's suggested transaction restore was not accepted as PHP semantics.
- Preserve retain-before-replace, reference aliases and request reset; cover
  nested calls, foreign callback contexts and Throwable paths with tests.

No hook implementation or ABI change has been applied at this checkpoint.

### Hook registration groundwork

Subsequent implementation adds NativeGlobalSyncHooks (two scope-argument C ABI
function pointers), an optional pair on the opaque context, and a registration
entry point rejecting null contexts, incompatible ABI versions and incomplete
pairs without replacing an existing registration. Published/copied AOT metadata
carries this pair so context reuse does not lose module transfer routines.
The focused registration test passed; it verifies admission only, not transfers.
The test ABI-version constructor was updated alongside the production constructor.

Still to implement: generated transfer routine bodies, registration from codegen,
invocation at both execution boundaries, ownership/error-path validation and all
supported-target checks. The native regression tests remain red; this groundwork
does not change their PHP-visible behavior yet. Kage recall timed out after 300s.

### Nested native execution now passes

context_registration.rs emits paired module-wide transfer routines alongside
the existing registration helper, each taking the live global scope explicitly.
Each routine has a 112-byte ABI frame (96 bytes scratch, 16 bytes footer), saves
the scope at scratch offset 64 and reuses the existing inventory/flush/reload.
The helper registers both addresses before publishing reusable AOT metadata.

native_execution.rs copies the hook pair and scope pointer, reloads eval globals
into native storage before function.call, then flushes native changes back before
result classification, argument cleanup and reference writeback. Missing native
metadata leaves standalone interpreter contexts on their existing path.

The freshly rebuilt debug CLI passed
test_eval_globals_nested_native_call_observes_each_boundary in 18.45 seconds:
native:7:eval:9:outer:9, matching PHP. Runtime staticlib build passed; compiler/test
rebuild took 2m36s. The optimized release compiler still predates these changes.

A read-only review incorrectly proposed reversing the two hooks, confusing an
eval-to-native invocation with the opposite direction. That change was rejected:
eval writes 7 before nativeStep, so eval-to-native reload is required first.

Remaining: inverse callback invocation, method/conversion paths, missing-scope
contexts, references, ownership replacement/downgrade, exceptional transfers,
and focused five-target assembly/execution coverage. Hook status handling still
uses the pre-existing generated fatal/throw handlers; this is not error-path
closure. No full application request has been rerun.

Own r151/r152 assembly artifacts were compressed losslessly to index.s.gz
(63 MiB each instead of 832 MiB each), retaining binaries and diagnostics.

### Captured-context callback transfer now passes

The three generated native callers of __elephc_eval_callable_call_array were
enumerated: both architecture descriptor invokers and introspection lowering.
There are no interpreter callers of this ABI in the current graph/source.
Its existing inner dispatch now flushes native globals to the available context
global scope before execution and reloads modifications before returning status,
including normal EvalStatus errors. The hook pair and scope are copied before
execution; scope-less fallback contexts remain an explicit open case.

Rebuilt target/debug/libelephc_magician.a (14.08 seconds), then ran the existing
CLI integration harness directly, avoiding an unnecessary compiler rebuild.
Both bound-closure reducers passed in 38.39 seconds:
- test_eval_bound_closure_global_keyword_control_shares_compiled_storage
- test_eval_bound_closure_global_once_guard_shares_compiled_storage
Both now return 1:seen:2. The prior explicit and nested eval tests also passed
together (two tests, 35.28 seconds) before this inverse callback change.

Added test_eval_native_global_transfers_do_not_accumulate_owners using the
existing same-binary/equal-length-argument heap comparison, one versus four
native calls inside one eval activation. Semantic output is not heap closure.
Methods, foreign/fallback scope selection, PHP references, exceptions, panic
paths and all-target verification are still open before the application replay.

The scalar same-binary transfer heap comparison passed in 19.17 seconds.
A stronger string-replacement variant found a separate compiler admission gap:
nativeAppend with `global $counter; $counter .= 'x';` is rejected as a change
from int to string, despite the outer GLOBALS value being initialized to 'seed'.
Kept as test_eval_native_global_keyword_heap_replacements_do_not_accumulate_owners.
The equivalent explicit GLOBALS read/concatenation/write is tested separately as
test_eval_native_global_heap_replacements_do_not_accumulate_owners to isolate
transfer ownership from that global-keyword inference failure.

The explicit GLOBALS string variant compiled, but failed the heap comparison in
18.35 seconds: one call leaves 10 blocks/736 bytes; four calls leave 13 blocks/880
bytes. That is three additional blocks and 144 bytes, not heap stability.
Before attributing this to the new barriers, compare the same repeated native
global string replacements without eval. The scalar test was insufficient to
detect this case; keep the heap-backed regression red until the root cause is
fixed, not merely the value output.

## Native-only string owner isolation

test_native_global_heap_replacements_without_eval_do_not_accumulate_owners
also fails: one call leaves 2 blocks/96 bytes; four leave 5 blocks/240 bytes.
The same three-block/144-byte delta without eval rules out attributing this
specific accumulation solely to the new bridge transfers.

Inspected nativeAppend's actual EIR and assembly in the failed native-only test
artifact. The concat is acquired/persisted into v4, marked MaybeOwned; StoreGlobal
then uses retaining Mixed boxing rather than consuming that persisted owner.
Only v3 (scratch concat) receives a later Release. Each persisted v4 source owner
is stranded. value_can_transfer_ownership_to_consumer required exactly Owned,
despite string Acquire having materialized its own lifetime-safe payload.

The shared predicate now recognizes string Acquire while keeping its explicit
Release guard and OwnedTemp move-out guard. It does not classify every MaybeOwned
value as transferable. Regression rebuild/run pending at this checkpoint.
Follow with an assembly regression for both representations/ABIs, including an
explicit Release that must prevent stealing the acquired owner.

Verification: the rebuilt compiler passed both global_heap_replacements tests
in 23.08 seconds after a 3m10s Rust rebuild. The native-only and eval/native
variants no longer accumulate the extra three blocks/144 bytes. This establishes
stability for these two reducers, not a globally clean runtime heap. The global
keyword string-admission test remains red and is excluded by this filter.
Luna's bounded ownership review found no double-free in the Acquire/StoreGlobal
case; target-specific explicit-Release emitter tests remain to be added.
Disk free space fell to 1.4 GiB; inspect regenerable artifacts before further
heavy rebuilds. No full Symfony request has been run after these changes.

## Global declaration checker correction

check_global previously kept any same-named local type; otherwise it copied the
top-level initial type or defaulted to Int. Both constrain mutable PHP global
bindings incorrectly. It now replaces the local binding with Mixed for ordinary
PHP globals, preserves superglobal/extern type contracts, and clears stale local
callable metadata. cargo check -p elephc --lib passed (51.58 seconds).

Added test_global_keyword_allows_replacing_a_known_initial_type and
test_global_keyword_rebinds_a_same_named_local. The existing keyword string heap
test covers the absent top-level plain-name case. Runtime execution of these
tests is pending; checker success does not prove EIR declaration-point rebinding.
DateTime's observed parallel Rust build was verified in its own worktree and
left untouched.

Resource checkpoint: the targeted test rebuild was deliberately interrupted
with SIGINT after disk free space fell to 384 MiB (Cargo exit 130 verified).
Only our Cargo PID 81395 was interrupted; its cwd was checked first.
The inactive agent-ad53b5c8fa71a56fe worktree target was then inventoried, checked
for symlinks/open target files, and cleaned with Cargo using exact manifest/target
paths under the user's existing authorization. Cargo removed 26845 regenerable
files, reporting 13.4 GiB; no source was removed. DateTime's separate Cargo process
and target remained untouched. Own r149/r150 binaries were compressed losslessly
to index.gz (28 MiB each), with r151/r152 binaries left directly executable.
The global_keyword_ test run was restarted only after the prior run's verified
termination and recovery of disk space; its result remains pending here.

The resumed global_keyword_ run completed: three passed, one failed (37.65s).
Passed: keyword string heap replacements, bound-closure keyword control, and
replacement of a known initial global type. Failed: same-named local rebinding,
which produced 42:s instead of seed:done. The global remained 'seed'; reads and
writes after the declaration still targeted the old local. lower_global calls
declare_local_with_kind, which immediately returns an existing slot without
changing its binding. Fix requires declaration-point binding semantics, balanced
old-local ownership and control-flow/reference handling, not just a checker type
change. Keep the new regression red until that runtime issue is corrected.
Disk free space after cleanup/rebuild was approximately 10 GiB.

## Declaration-point binding design evidence

Pinned php-src additionally inspected: Zend/zend_compile.c 5089-5120 emits
ZEND_BIND_GLOBAL for a normal compiled local. Zend/zend_vm_def.h 8170-8242
finds/creates the global entry, acquires its reference, installs the new local
binding before destroying the old local owner, and handles destructor exceptions.
This is an executed binding operation, not a static local-kind annotation.

Executed PHP 8.5.6 oracles:
- Conditional global, false then true: local:seed|seed:changed.
- Reference to the local before global rebinding, followed by unset and local
  reassignment: local:global:global:new-local. The earlier alias stays local;
  unset of the bound local does not remove the global variable.

Bounded read-only Luna review agrees that changing local_kinds alone is invalid
for conditional declarations. Reuse local ref-cell promotion/alias/cleanup,
but add a real global target binding; existing AliasLocalRefCell addresses two
locals, while ordinary native globals are still direct Mixed symbols. Include
branch/loop execution, prior/later aliases, unset/reassignment, arrays/COW and
destructor exceptions in the implementation tests. This design is not implemented.

An optimized compiler rebuild is now running to prepare a full stock --web
replay with the already verified callback/ownership corrections. Compiler/runtime
source edits are paused during that rebuild; the same-named-local regression
remains open, not silently accepted as fixed.

## User-requested binding implementation resumed

The user explicitly requested correcting this now, so the optimized compiler
rebuild was stopped (only own Cargo PID 87237, cwd verified, exit 130 verified).
No Symfony replay took place. The binding fix takes priority over that replay.

First implementation slice: shared global ref-cell final destruction now uses
the payload representation tag at offset 8 to release either a boxed Mixed value
(tag 7) or the existing associative-array payload. A focused emitter test passed
on all five target configurations in one run (Rust build 1m15s). This is assembly
coverage, not five-target runtime execution.

Shared global load/store helpers were renamed without web-only naming and now
accept boxed Mixed payloads as well as hashes. Ordinary StoreGlobal performs
boxing before the shared-cell store; all existing helper consumers were updated.
cargo check -p elephc --lib passed in 29.01s after this generalization.
No ordinary-global cell producer/binding opcode is wired yet, so these preparatory
changes do not close the same-named-local/conditional/reference regression tests.

Implementation still required:
- Explicit EIR operation materializing/acquiring a global reference-cell target.
- Preserve the ordinary fast path where no reference binding is needed, with a
  complete module-level inventory for symbols that use shared cells.
- Bind the runtime local pointer at the executed declaration, with a durable
  owner share and cleanup; never just mutate local_kinds statically.
- Predeclare uniform Mixed local storage for names with a runtime global binding
  before parameters/body emission; reuse global_decls traversal, not a new walker.
- Handle prior aliases, conditional representation flags, unsetting/rebinding
  either the local alias or the global name, and request/CLI global cleanup.
- Adapt process argument initialization and both eval synchronization directions
  for shared Mixed global cells, retaining before replacing an existing owner.
- Keep PHP's destructor exception ordering, plus the already-added TDD cases.

## Runtime binding implementation and six green reducers

Implemented GlobalRefCell in EIR, validation, effect/name metadata and target-aware
codegen. It creates the global's shared Mixed cell if absent and acquires an owner
share. Shared-cell inventory recognizes the opcode. CLI global cleanup, process
argument initialization and web reset now distinguish shared cells from values.
Shared Mixed reload from eval retains the replacement before releasing the old
cell payload, including same-pointer borrowed values.

lower_global now calls bind_global_local at the declaration's actual position.
The body collector seeds runtime_global_bindings before parameter/body lowering;
ordinary names in that set get uniform Mixed storage before any branch. Binding
uses the existing runtime ref-cell representation flag, pins the prior payload
until after installation, and stores the acquired share in a hidden RefCell owner.
Local reference aliases now get an owner share as well, so a prior alias can keep
its old cell when the source name is rebound. External/superglobal/main paths
keep their existing dedicated storage behavior.

global_keyword_ passed all six then-present tests in 40.10 seconds after a 2m31s
Rust rebuild: known-type replacement, same-named local, conditional binding,
prior alias plus local unset, keyword callback control and string heap stability.
This supersedes the 42:s result for the original local-rebinding reducer.

Two additional oracle-backed tests exposed remaining boundaries (8-test run,
six passed, two failed, 44.01 seconds):
- Global name unset/recreation: new:detached, expected old:new.
- Conditional binding observed inside eval: :eval:eval|:eval:eval, expected
  seed:eval:seed|eval:eval:eval.

Added UnsetGlobal EIR/codegen to remove the name's owner before destruction while
other reference-cell owners stay live; ordinary global unset lowering uses it.
Eval's global inventory now accepts GlobalRefCell/UnsetGlobal as storage evidence,
and a missing shared Mixed scope entry detaches its global name instead of
overwriting the cell with a null box. cargo check passed (26.91 seconds).
The updated eight-test runtime run is active; no result recorded yet.

Further checks remain mandatory: eval local/global scope identity (including
literal eval AOT versus Magician), global unset/recreation within one eval,
repeated binding after local unset, destructor exceptions, reference/COW regressions,
and five-target execution/emission coverage. No Symfony replay or complete parity
claim at this checkpoint.

Updated runtime result: seven of eight global_keyword_ tests passed (48.21s,
after a 3m10s Rust rebuild). Global-name unset/recreation now correctly returns
old:new. The remaining failure is conditional binding inside eval, still
:eval:eval|:eval:eval instead of seed:eval:seed|eval:eval:eval. Inspect the actual
literal-eval EIR route before attributing this only to Magician: eval AOT scope
materialization may merge same-named local/global values or omit GLOBALS aliases.

AliasLocalRefCell effects were subsequently made conservative for its owning
hidden-slot case (WRITES_HEAP/REFCOUNT_OP); a reference acquisition must not look
like a pure slot copy to the optimizer. This metadata adjustment requires the
next focused runtime rerun alongside the remaining eval correction.

## Literal eval scope collision isolated and corrected

Saved the standalone reducer at target/global-binding-eval.php and its original
EIR at target/global-binding-eval.ir. The selected route is a compiled
__eir@evalaot_scope function, not Magician: it stored `shared` using EvalScopeSet
and fetched a nonexistent `GLOBALS[shared]` key using EvalScopeGet. Meanwhile
calls.rs flushed/restored the real global under `shared`, overwriting the lexical
local. This explained both the empty GLOBALS output and false-branch global write.

GLOBALS aliases now bypass lexical eval-scope load/store selection and lower to
native global access. In compiled scope materialization, globals sharing a name
with a lexical sync local are no longer merged into that same scope key. The
normal Magician eval/include global scope remains separate and unchanged.
Added test_eval_literal_global_reads_do_not_alias_local_variables.

The complete eval_globals module ran 13 tests: 12 passed, one failed (81.99s;
Rust rebuild 2m33s). Pure lexical/global separation returns seed:eval:seed.
All existing argc/argv, explicit/nested eval, callbacks, native global binding,
alias and unset tests pass. The conditional eval reducer now returns
seed:eval:seed|seed:eval:eval instead of seed:eval:seed|eval:eval:eval.
Thus the remaining problem is immediate write-through for an actually bound
native reference DURING the compiled eval body, not namespace collision.
Post-change EIR is saved as target/global-binding-eval.after.ir.

Next implementation must carry live native reference access into the compiled
scope function and avoid replacing it afterward with a stale snapshot. Preserve
native function/global mutations observable mid-eval and PHP copy-on-write; do
not mutate a shared boxed value in place or route the literal through Magician.
Consider the read-only scope-parameter optimization too. Existing AOT fallback
policy excludes declarations/global/ref-assign/closures; do not silently widen or
restrict that policy as a substitute for implementing supported-scope semantics.

## Live compiled-scope bindings: 13 native tests pass

Implemented native_scope.rs: compiled scopes distinguish ordinary snapshots,
call-bounded native reference bindings and native global-name bindings. Native
reference entries own a tag-11 marker box borrowing the caller's live storage;
they do not claim heap ownership of a potentially stack-backed parameter address.
Scope getters/setters access original storage while the compiled eval executes,
and reload skips those live bindings instead of overwriting them with snapshots.
Before returning to PHP, marker entries become owned value views again. The eval
return cell is parked at scratch offset 88 while those views use offset 40.
The literal remains compiled EIR; no fallback to Magician was added.

Rust scope ABI stores NativeScopeBinding metadata (bits 32/64) and clears it on
ordinary replacement. Core scope helpers already preserve caller flags; both
backends reject an invalid combined binding kind. Owned-to-borrowed replacement
of the same cell now returns/releases the old scope owner in Rust and core
assembly, instead of silently losing that reference.

Executed before the following capability-symbol addition:
- Full eval_globals module: 13 passed, zero failed, 73.83 seconds after 2m26s Rust
  rebuild. Conditional immediate visibility now matches PHP completely for the
  reducer: seed:eval:seed|eval:eval:eval.
- scope_native_binding_flags_roundtrip_and_plain_replacement_clears_them passed.
- set_borrowed_same_cell_returns_the_previous_scope_owner passed.
- Independent CLI parameter-alias probe printed 11, matching PHP; heap clean,
  one allocation/one free, peak 48 bytes. Source retained in
  target/alias-param-probe.php. Added the permanent
  test_local_alias_of_reference_parameter_preserves_owner; that added test still
  needs execution (the prior 13-test run did not contain it).

After the September 8 resume, added __elephc_eval_scope_bind_native as an explicit
capability entry point. New compiled-binding call sites use it; old bridges
without support fail to link instead of discarding flags. The core backend uses
an actual tail-jump wrapper, not adjacent empty symbol aliases that could be
split by runtime-cache/dead-strip partitioning. Rust forwards to the validated
setter. The FFI capability flag roundtrip test passed again (19.31s build).
The five-target capability emitter test is currently running.

Do not infer full GLOBALS or Symfony acceptance: read-only eval parameter
optimization with reference-bound reads, interpreter observers during compiled
eval, typed/native parameter references, literal-only global-name discovery,
destructor/exception paths and full cross-target runtime coverage remain audit
items. CLI ENV hydration remains open. No Symfony HTTP replay since r152.

Capability follow-up: native_scope_binding_capability_on_all_targets passed
(five target configurations, 1m24s Rust test build). The native binding FFI
roundtrip passed with the new explicit entry point. Rebuilt the actual top-level
debug Magician static archive (14.78s), not only its test rlib.
Global-name binding entries now use a null placeholder rather than storing a
borrowed old value pointer that could dangle after an in-eval global assignment;
compiled access resolves the binding by name and does not consume that placeholder.

An optimized compiler rebuild is active with opt-level 1, 256 codegen units,
LTO disabled, one Cargo job and incremental compilation disabled. Compiler source
is frozen during this build. Next: check the new compiler on the scoped-global
probe (both scope-only and bridge-linked paths), then full Symfony --web r153
using the existing timing/inventory and one HTTP worker acceptance procedure.
The added permanent reference-parameter alias test still needs execution; its
equivalent earlier CLI probe was clean, which is a narrower piece of evidence.

## Optimized probes and full r153 retry

Optimized compiler reconstruction completed in 12m07s. Both fresh optimized
probes returned seed:eval:seed|eval:eval:eval with process exit zero. Assembly
confirms the literal scope remains compiled; the bridge variant additionally
contains the declaration fallback call to __elephc_eval_execute. Heap summaries
remain non-clean: scope-only 5 blocks/224 bytes, bridge variant 11 blocks/736
bytes. Do not report these probes as full ownership closure.

Full Symfony r153 stopped after 78.42 seconds because the locked PCRE2 10.47 r4
artifact was absent from the shared native cache. It had already emitted an
832 MiB index.s; no new language error was reported before this integrity failure.
Restored the package with native install --locked --offline --target macos-aarch64.
Native doctor then reported healthy. Manifest and lock hashes stayed unchanged:
- elephc.toml: 44ea7fbfddc221f79a6dbb7d7def247b0829c7900badf094b18d966a5a66d548
- elephc.lock: 3ceab0a43fd72883bbd94aa1714e024c960eca3199c1ac761992e3bc45bde1c5

Preserved failed-attempt assembly as index.s.gz; its uncompressed SHA-256 is
ec300eb02f6225758027ff154b497d07fa14e16097ef3f073efe52fb54239389.
The retry uses the same output directory and parameters; logs are separated as
target/symfony-web-r153.recovered.compile.stdout/stderr. It is currently active.
Run native doctor before expensive future retries to catch this cache condition
without spending another frontend/codegen pass.

Application entry, Kernel.php and composer.lock had no local modifications.
The dev cache is inherited dirty state (848 KiB) and was preserved. A later cold
cache verification must preserve that state reversibly and must not warm the
application with PHP as a substitute for Elephc runtime acceptance.

### r153 recovered build and HTTP results

Recovered build succeeded: 181.20 seconds, binary 115 MiB, runtime cache hit,
two assembly slices with zero reuse. Current assembly SHA-256 differs from the
failed attempt: 8d269a2d1e31bd6b87cee3212203df391c841b0fafcbf44462bc59bea6327de9.
Do not call that deterministic-source nondeterminism proven: native artifact
availability changed between attempts. Keep both assembly artifacts for analysis.

Untraced request: HTTP 000 / curl 52, empty body after 28.202626 seconds. Worker
exhausted its 32 MiB heap: 761382 allocations, 553802 frees, live/peak 33426624,
bump 33554400, failed request 32 bytes. Unlike r152, there were no undefined
GLOBALS warnings. No standard welcome page was returned.

Trace replay of the SAME binary: HTTP 000 / curl 52 after 32.872833 seconds;
worker SIGSEGV during ResolveBindingsPass on Router metadata, after
getMethod(setConfigCacheFactory). It did not report the same heap fatal, so keep
the outcomes distinct. Own parents 12543 and 14329 were stopped through SIGTERM;
both server sessions completed. No r153 server remains running.

Bounded append-key audit: eval_array_append_key allocates owned iteration keys,
one/candidate/comparison cells and fails to release internal temporaries; truthy
returns a Rust bool, not another cell. This is a concrete helper-level leak, not
proof that it accounts for the whole request. Also inspected hash_append.rs:
the native helper likewise scans existing integer keys; it has no demonstrated
persistent next-free-element contract. Do not adopt max-key or saturating_add as
PHP compliance. Next work: ownership-focused reducer and cleanup, plus separate
red parity tests for append after unset, negative keys and integer overflow.

## Binding regression checkpoint, September 8

The previously open same-named-local binding defect is corrected in the current
working tree. Re-executed the existing integration binary sequentially with the
runtime_gc::eval_globals:: filter: 14 passed, zero failed, 103.58 seconds.
This includes the newly added reference-parameter alias ownership test, which
requires a clean heap summary. Conditional declaration, prior aliases, local
unset, global-name unset/recreation, mutable types, scoped eval and nested native
calls all passed their respective regressions.

Re-executed PHP 8.5.6 CLI oracles for conditional binding during eval and global
name recreation: seed:eval:seed|eval:eval:eval and old:new respectively.
Scoped git diff --check passed for the compiler/runtime and runtime GC tests.
No full suite or new Rust build was started: free disk was initially 1.8 GiB.
These checks close the original binding reducer, not all GLOBALS semantics or
the Symfony HTTP gate. The r153 HTTP failure above remains the latest full run.

Append-key ownership follow-up: eval_array_append_key now releases iteration
keys, arithmetic/comparison temporaries, rejected candidates and displaced
candidate owners, including failure cleanup. Before correction, the dense
one-versus-four append reducer grew from 18 blocks/1120 bytes to 72 blocks/3744
bytes. After rebuilding the actual Magician static archive, both
test_eval_append_key_scan_releases_temporary_cells and
test_eval_append_key_scan_releases_ignored_and_smaller_keys passed (38.85 seconds
for the two-test run). This proves no differential accumulation for those
branches, not a clean entire process or persistent next-free-index parity.

## r154: heap corruption caught by a hardware watchpoint

Full build with unchanged 32 MiB heap and updated debug Magician archive passed:
205.61 seconds wall, 115 MiB executable. Timings: native code generation 47.13s,
assembly 101.28s, linking 1.58s; one of two assembly slices reused. Peak memory
footprint 5400285720 bytes. Native doctor was healthy before compilation.
Preserved r153 assembly losslessly as index.s.gz (verified against the original
before removing the uncompressed copy); the older failed assembly archive was
renamed index.failed-attempt.s.gz without overwriting it.

HTTP remains 000/curl 52, zero bytes after 28.141228 seconds. First r154 worker:
761394 allocations, 553817 frees, live 33426496, failed allocation 128 bytes.
Append cleanup did not materially move this request boundary. No welcome page.

Attached LLDB to replacement workers of this same parent, no source changes or
heap-size increase. ASLR base was 0x102ecc000; allocator failure breakpoint
0x108506d34. The immediate allocation is string persistence, while the recorded
hash_new_origin is only the most recent hash creation and not proof of cause.
Retained breakpoint trace in target/symfony-web-r154.heap-breakpoint.log.

At failure, a sequential heap walk encounters an invalid header at 0x10a2ab080:
size 149, references zero, kind 576. The previous block header 0x10a2ab030 has
payload size 64, references 3, kind 4295032838 (object kind 6 with metadata).
Its payload starts 0x10a2ab040 and contains class id 91. Current assembly's
_class_name_91 identifies ReflectionException, not an application subclass.
The partial census covers only 5706512 bytes: DO NOT treat it as a full heap
accounting report. Binary snapshot target/r154.heap.bin contains the 32 MiB arena.

Hardware write watchpoint at 0x10a2ab080 with new-value condition 149 caught the
actual overwrite. PC after the store is 0x1070ac448. Preceding instructions:
str x1, [x9, #56]; str x2, [x9, #64]. Registers: x9 0x10a2ab040 (object payload),
x2 149 (persisted string length). The second store is exactly beyond the
64-byte payload and writes the NEXT allocator header. This is demonstrated
memory corruption; whether it explains all remaining leaks is not established.
Log: target/symfony-web-r154.heap-watchpoint.log. Next investigate allocation
size versus compiled ReflectionException inherited-property layout; add a native
regression before fixing the shared allocation/layout contract, not vendor code.
Magician reflection throw paths use values.new_object("ReflectionException")
in reflection/class_lookup.rs and statements/native_static_dispatch.rs.

Own parent 63220 was stopped with SIGTERM after evidence capture; server session
completed. No Symfony acceptance claim. Other worktrees and app sources untouched.

### Throwable declared-layout repair (in progress, 2026-09-08)

- [x] Add two CLI regressions for protected file/line writes through a bound
  closure and an inherited method, including previous-object identity.
- [x] Establish RED against the current compiler: both tests fail; message bytes
  become NULs and source getters return their old synthetic location. Log:
  target/throwable-layout-regression.log (0 passed, 2 failed).
- [x] Check both expected outputs against PHP 8.5.6 with no php.ini.
- [ ] Validate shared complete-allocation migration and property offsets.
- [ ] Cover generated exceptions, eval construction, inherited extra properties,
  ownership and all supported target emitters before rebuilding the web fixture.
- [ ] Repeat the stock HTTP request and check whether the heap overwrite is gone.

Root cause is wider than allocation capacity: the declared property sequence is
message, code, string, file, line, trace, previous, with offsets 8, 24, 40, 56,
72, 88, 104. Old native paths allocated 56 bytes, put previous at 40 and the
creation line at 32. The generic property and GC walkers use the declared layout.
The migration shares allocation/zeroing and named layout offsets across both
architectures; a size-only patch would preserve previous/string aliasing.
The php-src baseline is tag php-8.5.6, Zend/zend_exceptions.c, especially
zend_default_exception_new and property-based getters. This work does not close
the existing missing backtrace or include-source-provenance semantics.

Kage context timed out after 300 seconds again. The existing packets and this
ledger remain available; no index deletion or concurrent reindex was attempted.

Follow-up evidence: the original two CLI regressions are GREEN after complete
allocation and previous-slot migration. Existing `throwable previous` test filter:
27 passed, 0 failed, 1 ignored (126.63s), target/throwable-layout-existing.log.
Two library tests also pass: declared builtin metadata matches the constants,
and allocation zeroes every slot on all five supported targets.

Expanded regressions found two distinct follow-ups: native subclasses with extra
properties needed source stamping after their ordinary defaults (implemented),
and source-level dynamic construction can call the eval bridge with a NULL
context. The latter creates a temporary fallback context with empty call-site
metadata. Passing source only to an already-owned eval context is insufficient.
The direct source probe expects file:line:file:line in PHP; after the subclass
fix it reaches file:line:wrong-file:wrong-line. It must be repaired before claiming
source getter migration complete. Planned transport: explicit source metadata
on the native construction call, applied before constructors in whichever
context (owned, callback owner, or temporary fallback) resolves the class.

The repeated-owned-fields regression is GREEN with a clean heap. The generated
ReflectionException regression initially used a constant missing class and was
rejected by the checker rather than exercising runtime allocation; its class
name now includes runtime argc so it tests the intended runtime path.

Saved r154's 832 MiB index.s losslessly as a 39 MiB index.s.gz, gzip-tested and
compared byte-for-byte before removing only the uncompressed copy. The binary,
debugger logs and heap snapshot are intact. No new full Symfony build yet.

Follow-up: explicit construction-site transport is implemented by the additional
`__elephc_eval_try_new_object_at` ABI; the old ABI remains available. Source fields
are initialized before user constructors, under their internal declaring scope.
Compiled builtin getFile/getLine dispatch now reads the declared property slots
when invoked through eval. All seven throwable_layout CLI regressions pass
(31.97s; target/throwable-layout-fix-expanded.log).

The opaque-eval construction heap reducer is still RED. Its initial loop mixed
two causes: an empty loop alone retains two cells per additional iteration.
stdClass showed that same slope, so it does not establish a construction leak.
The regression now repeats statements in one conditional branch instead, keeping
source and activation constant. A debugger census shows no surviving object
payload, but retained scalar cells including argument indexes. RuntimeOps::arg_array
creates each boxed index without releasing it after the borrowing array setter.
A separate native-constructor regression isolates this path before correction.
Other Throwable defaults and loop temporaries remain under investigation.

Argument-owner follow-up: the isolated three-int constructor reducer initially
retained nine cells per call. Releasing RuntimeOps::arg_array indexes reduces
that to six; releasing reader indexes in both generated constructor/method ABIs
reduces it to three. Explicit zeroed owner slots now keep array_get results alive
until the common completion tail, after reference writeback, including native
exception exits. A read-only audit found no offset or bypass defect, but executable
alias/by-ref/failure coverage is still required. The remaining live scalar cells
are literal arguments: eval_const allocates them, while eval_expr_is_owning_temporary
previously classified only arrays/new/clone. Const is now classified as owning.
Evidence logs: eval-constructor-index-red.log, eval-constructor-index-partial.log,
eval-constructor-index-fix.log and native-constructor-owners-census.log in target/.

The builtin EIR audit was stopped after discovering it internally launches
`cargo run --example gen_builtins` with inherited defaults. Only that audit and
its verified Cargo/rustc descendants were stopped; the other-worktree datetime
tests remain untouched. Resume the docs workflow with the single-job,
non-incremental environment applied to the Python scripts too.

Literal classification closes the three-int constructor reducer: GREEN after
the archive rebuild. The same-binary instance/static method argument reducer and
three global replacement/transfer regressions also pass. Expanded run: five pass,
one fail (target/eval-argument-owners-expanded.log). Its alias test mixed in eval
binary comparisons, whose temporary operands are a separate measured leak; it
has been rewritten as a runtime-include content regression expecting
payload:payload, without claiming heap stability of comparisons.

The Throwable reducer remains RED after generic literal cleanup (five cells per
construction). Builtin constructors no longer erase the allocator's property
defaults; replacing message/previous releases the previous field owner. Fresh
default arguments were also marked borrowed in native function and method
binders: those defaults and nested object-default constructor arguments now carry
their owed release. These latest changes await executable validation.

Latest focused results: seven pass, one fail in
target/eval-argument-property-owners-fix.log (119.04s). Constructor literals,
instance/static argument ownership, return-alias content, string-property
replacement and three globals regressions pass. The remaining default-argument
case retains one persisted native string conversion per call. A second owner
slot per argument now tracks by-value string conversion payloads separately from
reader cells; builtin property initialization and reference argument conversions
are deliberately not registered in this bank because their transfer rules differ.
The latest bank and property changes are awaiting validation.

The Throwable reducer previously fell from five cells to one empty string per
construction (target/throwable-default-owners-validation.log); all seven layout
tests still pass. A debugger census located the orphan empty string in native
property initialization: eval's string setter overwrote its old payload. The
generic string-property RED/GREEN regression verifies its repair, using decref
rather than raw heap_free_safe so shared owners remain valid.

The readonly audit found matching omissions for arrays, objects and Mixed cells.
All three new reduced tests are RED before repair:
target/eval-heap-property-owners-red.log, 0 pass / 3 fail, 45.43s. The object test
expects drd and produced r (neither old value's destructor was called). Their
setters now retain the new value, publish it, then decrement the old owner, so a
destructor can observe the replacement. These changes also await validation.

## Consolidated ownership validation, 2026-09-08

- Exception and associative-key reducers: 2 pass, no accumulation, 31.08s
  (target/eval-key-throwable-owners-fix.log).
- Imported native owner unset: drd now passes; raw-local missing-entry reload
  releases its old owner through the existing representation-aware cleanup.
  Timing inside the include remains a separate unproven claim.
- Initial wider reference validation exposed five regressions (13 pass / 5 fail).
  In-place variable writeback wrongly demoted the existing scope cell to Borrowed;
  preserving identical cells and retaining replacements fixed four. Mixed ref
  slots additionally need their own owner after preparation succeeds. Unchanged
  writeback consumes that owner; changed shared cells are cloned before transfer.
  Both dedicated Mixed regressions pass, including preservation of a shared source.
- Full targeted replay: 18 pass / 0 fail, 202.12s
  (target/eval-owner-byref-exception-recheck.log).
- Non-string default reducer exposed retained raw constructor conversions.
  Constructor casts retain arrays/objects/iterables while method casts borrow them;
  only constructor-owned conversions are now added to cleanup. This reducer and
  caller/property COW preservation test pass (19.93s and 17.82s respectively).

GLM 5.2 and Kimi K2.7 cloud reviews returned NO LOCK on their respective snapshots.
Some findings needed additional code context; their non-string conversion concern
led to the failing reducer above. No absolute consensus or Symfony acceptance is
claimed. Reports/bundle hashes are retained under target/eval-ownership-review*.

The handler layout audit verified that a 224-byte slot does not contain the full
glibc AArch64 jmp_buf object (312 bytes) after its 24-byte header. The common slot
is now 336 bytes. The standalone layout test is RED before / GREEN after. Four PDO
adapters also required migration from hardcoded offsets; the interrupted agent
left only a partial scalar edit, completed by root with all four frame layouts
derived from the common size. Five-target PDO emitter test passes; four exception
and argument-owner layout tests pass. This is ABI/layout evidence, not a Linux
executable reproduction of an overwrite.

Two Rust builds hit ENOSPC before tests ran. Old PR #794 Cargo debug artifacts
(636 MiB), our unused incremental cache (594 MiB), and lossless compression of
owned diagnostic assembly recovered space without removing sources or reports.
The parallel DateTime/DOM builds were preserved. Space later returned to ~11 GiB.

An optimized compiler build for r155 is now running, logged to
target/symfony-r155-compiler-build.log. No new full Symfony build or HTTP result yet.
Builtin-docs regeneration, remaining unit checks and final Kimi K3 review remain
open. Existing edits under examples/symfony-app/config/reference.php were noticed
and left unchanged; they are not part of this ownership repair.

## Native heap corruption and profiler checkpoint, 2026-09-08

- r155 compiled in 112.92s to a 123 MiB executable after restoring the locked
  PCRE2 artifact. Its request still failed with heap exhaustion (HTTP 000).
- r156, with retained symbols, compiled in 91.92s to 174 MiB, reusing seven of
  eight assembly slices. It also exhausted its 32 MiB heap before responding.
- Hardware watchpoint evidence in target/symfony-web-r156.heap-watch.log catches
  __rt_array_set_str writing a string length beyond its allocation, reached from
  ResolveBindingsPass::processValue. An empty array changed element stride from
  eight to sixteen bytes without reducing capacity. The indexed setter now uses
  the same capacity recalculation helper as string push, on both architectures.
  The five-target emitter regression is RED before and GREEN after; PHP output
  probes passing on the old compiler are not evidence of memory safety.
- Kimi K3's array-spread entry-owner finding has a RED/GREEN heap regression:
  test_eval_array_spread_releases_owned_entry_cells. Remaining array literal
  failure-path findings and renewed same-hash consensus are still open.
- An external worker-aware CPU capture is available at
  target/symfony-web-r156.cpu.html and its .cpu.speedscope.json companion.
  This sampled profile is not an exhaustive call trace or allocation/leak proof.
- The r157 compiler rebuilt successfully in 11m13s. The isolated r157 Symfony
  build has started with web mode, retained symbols, regex and the same 32 MiB
  heap. Its build and HTTP acceptance results remain pending.

### r157 measured result

Build succeeded in 134.43s (174 MiB with symbols), six of eight assembler slices
reused; maximum RSS 2,838,888,448 bytes, reported peak footprint 4,718,924,168.
HTTP still fails: curl 52, HTTP 000, 43.209847s, heap exhausted. A fresh-worker
LLDB census now scans the entire 33,553,744-byte heap extent with no invalid
header, unlike r156. This validates removal of the observed header corruption,
not freedom from every possible memory defect.

At exhaustion, hash_new is called through hash_clone_shallow, hash_ensure_unique,
hash_to_mixed, Definition::setArguments and AbstractRecursivePass::processValue.
12,598 live refcount-one hash blocks of payload size 1072 occupy 13,706,624 bytes
including allocator headers. A second census confirms 5,793 of these hashes are
empty with capacity 16 and value tag 7; another 2,835 have one entry and 1,581
have two. Allocation sizing, redundant conversion copies and lost owners are
separate hypotheses to test, not established leaks from these counts alone.
Evidence: target/symfony-web-r157.heap-census.log, .heap-shapes.log and
target/r157.heap.bin. The debugger detached after each capture. The r157 server
remains available on loopback port 19083 with one worker.

Kage context timed out after 300s during this continuation. Local packets remain
readable; the old ref-literal function-boundary ownership packet is explicitly
superseded and must not be treated as current validation.

### r157 ownership isolation

A native-only setter/getter round trip with a string-keyed array, fluent return,
replacement and object destruction is heap-clean: one activation performs
13 allocations/13 frees, four perform 37/37, both end at zero live blocks and
the same 2752-byte peak. The CLI probe is target/hash-setter-owner-probe.php;
the equivalent permanent test is
test_native_array_setter_roundtrip_releases_conversion_owners. Its focused Cargo
validation is running in target/native-array-setter-owner-test.log. This is a
negative control, not a reproducer of the full request failure.

A conservative scan of the saved live heap's aligned payload words finds
incoming pointers for 5204 of the 5793 empty capacity-16 refcount-one hashes;
571 have none. Many Mixed reference cells also have no incoming native-heap
pointer. This does NOT establish unreachable allocations: native stacks, static
roots and Rust-held eval scopes were not scanned. Next isolation must cover
native/eval boundaries and scope lifetime instead of treating pointer absence
as a proven leak or changing the native setter despite its clean reducer.

### Nested eval argument result leak: RED and repair pending validation

The native negative-control test passed (one test, 3.95s). Opaque eval isolates
a different result: getter-only and setter-only repetitions remain stable, but
setArguments(getArguments()) leaves one additional 48-byte cell per repetition.
The permanent test test_eval_nested_native_array_calls_release_argument_results
is RED: 20 live blocks/2768 bytes versus 22/2864, with identical source and one
eval activation (target/eval-nested-array-argument-red.log).

eval_call_arg_values marked only a small whitelist of expression forms owned;
method-call result cells were excluded, so bound-argument cleanup skipped them.
Named and positional arguments now use the existing expression-result alias
rule when no prepared reference target exists. Prepared lvalues remain on their
existing reference-target ownership path. The shared whitelist's array-literal
and foreach consumers are unchanged by this repair.

Validation is running in target/eval-nested-array-argument-green.log. The CLI
requested a fresh release Magician archive automatically; the earlier debug
archive build alone did not complete this gate. Keep the existing test process
and its single-job release build running. No GREEN, by-reference replay, or
new Symfony HTTP success is claimed yet.

### Nested argument repair validation and r158

The new regression is GREEN after the release archive rebuild: one pass,
107.12s including the implicit archive rebuild. Six focused reference/exception
tests also pass (92.47s): Mixed/string constructor arguments, writeback before
throw, instance object-reference and static string-reference arguments, and the
unchanged Mixed ref-slot heap regression. Evidence is in
target/eval-nested-array-argument-green.log and
target/eval-nested-array-argument-ref-recheck.log.

The isolated r158 Symfony web build is running with the same 32 MiB heap and
retained symbols. Managed PCRE2 remains healthy. The r156/r157 assembly sources
were gzip-compressed, gzip-tested and byte-compared before removing their plain
originals; the recoverable .s.gz files are about 41 MiB each, with binaries and
diagnostic logs preserved. Global free disk nevertheless declined to ~3.3 GiB
during other active work, so continue monitoring before additional builds.

### r158 acceptance result: still failing

The build succeeded in 94.17s wall time, seven of eight assembler slices reused,
runtime object cache hit, 162 MiB executable with symbols. The optimized Magician
archive was rebuilt during the preceding test, so neither the size difference
nor runtime timing can be attributed solely to the ownership fix.

With one worker on loopback port 19084 the request fails after 26.255441s: curl
52 / HTTP 000, heap exhausted, requested allocation 1072 bytes, 796724 allocs,
601816 frees, 33424208 live bytes, bump 33554112 of 33554432. The focused fix is
real but insufficient for HTTP acceptance. Do not equate passing the reducer
with repairing the full container workload. Evidence: target/symfony-web-r158
build/server/curl logs. The r157 server was stopped; r158 remains available.

Free disk reached ~1.5 GiB. Lossless compression/verification of r158 assembly
is in progress; defer another full build until space is safe. The 32 MiB test
heap is a diagnostic budget, not a user-specified runtime acceptance limit;
before adjusting it, distinguish retained owners, container over-allocation and
the real cold-container working set with measured evidence.

### Controlled heap-budget comparison r159

The CLI confirms heap size is a compile-time fixed budget (default 8 MiB).
The 32 MiB budget was diagnostic, not a user requirement. r159 rebuilds the same
entry in web mode with a 64 MiB heap through the standard CLI option, preserving
regex capability and symbols. This controlled doubling tests whether boot can
cross the current exhaustion point; it does not excuse known leaks or establish
a bounded working set on its own. Build/HTTP results are pending.

The remaining plain r155 assembly was also gzip-tested and byte-compared before
removal, retaining its .s.gz. Free disk recovered from ~1.8 to ~2.6 GiB before
starting this single full build; r158 artifacts and server remain intact.

### r159: larger diagnostic heap exposes the next semantic failure

64 MiB build succeeded in 128.54s (128.99s wall), binary still 162 MiB with
symbols, seven assembler slices reused. Request failed in 50.772727s with HTTP
000, but NOT heap exhaustion: undefined console.command.* keys precede failure
to declare the framework YamlLintCommand. Its parent source exists. A repeated
request with ELEPHC_EVAL_TRACE set captures the same failure at
class_decl_error / validate_modifiers (target/symfony-web-r159.trace.stderr,
line 62952, ~15 MiB log); HTTP 000 in 50.775329s. r158 was stopped; the traced
r159 server remains on loopback 19086 with one worker. No debugger is attached:
the attempted mid-request attach lost the race with worker exit.

The runtime validators apply method override/signature checks to constructors.
This is a hypothesis, not yet the established cause: the initial small parent /
eval-child reducer passed declaration but failed when executing parent::__construct
with an unsupported StaticMethodCall error. A second reducer instantiates the
native parent first and tests declaration only. Do not claim the original
validate_modifiers failure is reproduced by the separate execution error.

r159 assembly was losslessly compressed and verified before deleting the plain
copy; disk recovered to ~2.5 GiB. Kage recall again timed out after 300s; source
and trace evidence remain authoritative. No full runtime acceptance or memory
leak closure is claimed from increasing the diagnostic heap.

The declaration-only v2 reducer now passes both PHP and Elephc (stdout ok), even
after native parent instantiation. Thus constructor signature incompatibility
has NOT been isolated as the YamlLintCommand root cause. Next diagnostic should
identify the precise failing check inside validate_eval_class_modifiers rather
than implement an assumed constructor exception based on the class shape alone.

### r160: granular class-modifier diagnostic

The existing ELEPHC_EVAL_TRACE diagnostic now reports the validation substage
and method name when validate_eval_class_modifiers fails. Validation order and
rules are unchanged. Release Magician archive rebuilt successfully in 81s.
The existing final AOT-parent override rejection and valid Override-attribute
tests both pass (2 passed, 71.84s, target/class-modifier-trace-regression.log).
Scoped diff whitespace check passes. Kage recall timed out after 300s.

r160 is compiling with the same 64 MiB diagnostic heap, symbols and regex
capability as r159, in its own output directory. Exact failing substage and
HTTP acceptance remain pending; no constructor exception is implemented.

r160 completed in 104.85s (105.26s wall), 162 MiB with symbols, runtime cache
hit and six assembler slices reused. Maximum RSS 3356835840 bytes; peak memory
footprint 4765700536 bytes. HTTP still fails: curl 52 / HTTP 000 in 39.803961s.
The new trace identifies member __construct, stage eval_parent_override for
YamlLintCommand. Assembly was gzip-tested and byte-compared before removing the
plain copy; about 2.0 GiB disk free afterward. The old r159 server was stopped.

The minimal opaque fragment target/eval-constructor-both-dynamic.txt now
reproduces the declaration failure with BOTH parent and child declared by eval:
parent constructor has optional nullable parameters, child has no parameters.
PHP accepts; Elephc rejects. The earlier AOT-parent reducer did not cover this.
PHP-8.5 Zend/zend_inheritance.c, do_inheritance_check_on_method, exempts concrete
constructors from signature and visibility checks, but preserves final and
abstract/interface prototype constraints, including inherited prototypes.

Two new table regressions in interpreter/tests/expressions/classes_traits.rs
are RED (target/constructor-override-red.log): valid constructor rejected and
final PRIVATE parent constructor incorrectly overridable. Additional cases cover
visibility narrowing, by-ref/variadic changes, abstract and interface prototypes
through a concrete intermediate class. PHP crosschecks confirm those boundaries.
No semantic fix yet: prototype handling must be preserved, not blindly skipped.

The eval-parent validator now exempts concrete constructors from signature and
visibility checks only when no abstract constructor prototype remains in its eval
ancestry. It preserves the oldest abstract prototype and immediate parent
visibility; inherited interface contracts retain their independent validation.
Both eval and AOT private-method shortcuts now exclude constructors, preserving
final-private rejection. The two regression tables are GREEN (27.04s rebuild,
target/constructor-override-green.log). Broader classes_traits replay is pending.
PHP also confirms the oldest abstract signature remains authoritative through a
widened abstract redeclaration, while immediate-parent visibility still applies.

Luna read-only review initially misread the private shortcut, corrected its
verdict after the PHP counterexample/source predicate, then found the eval patch
directionally correct. This is NOT the requested three-model consensus or final
audit. AOT concrete-constructor signature/visibility exemption remains open and
is not claimed fixed. No post-fix full web build has run yet.

### r161: constructor failure cleared in the real request; reflection is next

classes_traits replay passed all 17 tests. Release archive rebuilt in 80s.
r161 full web build succeeded in 159.31s wall, runtime cache hit but ZERO of eight
assembler slices reused. Maximum RSS 2680389632 bytes; peak footprint 4662841640.
The single-worker request returns HTTP 000 / curl 52 after 41.678163s. Crucially,
trace line 62952 now confirms class_decl_ok for YamlLintCommand. The new fatal is
ReflectionClass::getConstants, invoked while ReflectionClassResource processes
the native LoaderInterface reflection object (line 66501). No heap exhaustion
diagnostic on this request. This does not close memory-leak or HTTP acceptance.

The ordinary-method extra clone introduced in constructor validation was removed
after building r161; the same 17 tests pass again against current source
(target/constructor-override-no-clone-replay.log). A later small CLI probe caused
the optimized archive to rebuild against this no-clone revision.

A directly constructed ReflectionClass over a small native empty interface passes
getConstants inside opaque eval (PlainContract:0), with reflection already_bound.
The failing real object instead traces declared=false during foreign reflection
rebinding and then dispatches to the native getConstants fallback. A second
probe obtains the interface reflection through another class's getInterfaces;
its result is pending. Probe PHP source files have disappeared between calls
twice, including under a fresh /tmp directory; cause unestablished. Do not
attribute these removals to the compiler without isolating the execution step.
r161 assembly was gzip-verified/byte-compared before removing the plain copy;
about 1.8 GiB free. r160 server stopped; r161 listens on loopback 19088.

The getInterfaces-derived CLI probe fails earlier than the real request:
getName sees runtime_class ArrayIterator for the retrieved interface reflection
object. Evidence: target/reflection-interface-constants-related-probe.run.stderr.
Do not claim this is the getConstants root cause yet. The /tmp source survives
both this compilation and its binary execution when checked immediately after
each; earlier disappearance is not reproduced by either isolated step.

### Associative object return conversion: independent RED/GREEN defect

LLDB proves the related-reflection probe double-boxes an object: outer Mixed
contains tag 6 and a pointer to another Mixed(tag 6, actual ReflectionClass).
The actual object retains class id 75 under a write watchpoint. It is not an
overwritten class id or a demonstrated runtime-cache mismatch. Logs:
target/reflection-interface-wrong-identity.log and
target/reflection-interface-identity-watch.log.

emit_eval_result_as_type normalized indexed array<Object>, but associative
object-valued hashes merely unboxed the outer container. New
src/codegen/lower_inst/builtins/eval/associative_results.rs clones that hash,
walks insertion order, retains raw objects, replaces Mixed cells and per-entry
tags, releases old cloned-cell owners, then stamps the hash-wide object tag.
Existing raw-object entries are preserved. Both architectures are implemented.
Luna found no ABI/ownership defect, but notes the same declared-object-type
assumption as the indexed converter (no independent post-unbox tag validation).

Two CLI regressions in oop/reflection_regressions.rs are RED before the fix and
GREEN after it (target/assoc-object-return-{red,green}.log): interface-map object
identity with aliases, trait maps and empty maps. GREEN run 43.88s after a 3m11s
debug compiler rebuild. A subsequent extraction into a directly testable emitter
adds associative_object_results_preserve_ownership_on_all_targets for the five
targets; that unit test has NOT run yet. The extracted code was compiled by the
later foreign-reflection CLI run. Release compiler still lacks this conversion;
no full Symfony build after r161 has run.

### Foreign reflection: exact runtime failure reproduced across native frames

Two successive eval calls inside the SAME native frame both pass, preserving the
same context pointer. A native function returning eval-created ReflectionClass
to main reproduces the exact r161 failure: first frame getConstants succeeds,
main gets declared=false then native getConstants RuntimeFatal. PHP prints
0 followed by ForeignContract:0. Artifacts:
target/reflection-foreign-frame-probe.{build,run}.{stdout,stderr}; source
/tmp/elephc-reflection.U6XDTI/foreign_frame.php and foreign_create.txt.

Rebinding now accepts canonical emitted AOT reflection rows as well as eval
declarations, consistently for class/method/property/constant reflectors.
Native class/method source-location requests return None from the eval handlers
to preserve the native getters; the previous eval adapter used the global entry
file and would regress declaration paths. This boundary was missed in Luna's
initial review and corrected after reading the actual callback.

Expanded tests remain RED, with useful narrowed failures:
target/foreign-reflection-green.log (2 failed, 57.02s):
- constants and names work for native classes, interfaces, traits, enums and
  case-folded class names; class_alias still reports ForeignAlias rather than
  ForeignClass. Existing context.resolve_class_like_name should be investigated
  before canonical AOT row lookup (Luna recommendation; not applied yet).
- ReflectionObject retains ForeignFile.php and lines 2:5, but ReflectionMethod
  still reports main.php (lines 4:4 are correct). Its construction routes through
  Magician whenever an eval context exists (owner_dispatch.rs), whose native
  materializer has only the global source-file callback. The direct AOT source
  emitter already reads module.declared_class_source_files for method owners.
  Do not weaken this regression or treat generated native getters as universally
  correct: method construction is a different provider from ReflectionObject.

No requested three-model consensus, final Kimi audit, or Symfony HTTP acceptance
is claimed. Scoped diff whitespace checks pass. No active build was left after
the failed foreign-reflection run; r161 single-worker server remains on 19088.

Further validation: associative_object_results_preserve_ownership_on_all_targets
passes its five-target emitter assertions (target/assoc-object-return-targets.log,
1 matching test passed, 80s build). The reflection-filtered Magician unit replay
also passes all 84 tests (target/foreign-reflection-unit-replay.log). These do not
replace Linux executable coverage or a post-r161 HTTP request.

Attempting context.resolve_class_like_name before native canonical lookup, both
when constructing native class reflectors and when rebinding them, did NOT fix
the static alias case (target/foreign-reflection-alias-green.log, unchanged
ForeignAlias output). Those two small additions are WIP, not verified closure.
Luna's follow-up found the actual producer: src/autoload/alias.rs rewrites static
class_alias into a synthetic subclass, removing the original call and losing
alias provenance. Do not infer aliases from empty subclasses or merely disguise
their names in Reflection. Proper alias identity/activation needs investigation
with generic tests (including final classes/interfaces and parent identity).

ReflectionMethod construction follows lower_eval_native_object_new whenever an
eval context is present (owner_dispatch.rs35–57), whereas ReflectionObject has a
separate native materializer path. The direct AOT reflection_source_file helper
already uses declared_class_source_files for method declaring classes, but this
is not the provider used by the failing method test. A generic source-metadata
solution must also consider trait method origins, not assume every method's
physical file equals its declaring class's file. No source-file fix was applied.

### r162: optimized end-to-end replay and nested isset blocker

The optimized compiler rebuilt in 10m00s (single Cargo job, opt-level 1,
256 codegen units, LTO off); the separate standard optimized Magician archive
rebuilt in 53.85s. Compiler startup/version check passes. The first r162 build
failed after 64.87s because the locked PCRE2 artifact had disappeared from the
shared cache. `native install --locked --target macos-aarch64` restored it and
native doctor passed; no manifest change. Original failure logs were retained.

Retry succeeded: 102.08s compiler timing / 102.34s wall, 162 MiB with symbols,
runtime cache hit, 3/8 assembler slices reused. Maximum RSS 3270033408 bytes,
peak footprint 4670869872. Assembly gzip-tested and byte-compared before removing
the plain copy. r161 server stopped; r162 runs one worker on loopback 19089.

HTTP still fails: curl 52 / HTTP 000 in 26.415841s. Worker exits 255 and stdout
reports uninitialized AutowirePass::$container. This occurs during AutowirePass,
before the old ReflectionClassResource failure point, so do not claim the old
getConstants gate was crossed in the real request. It is fixed in focused tests.

AutowirePass clones itself before parent::process initializes container. Its
createTypeNotFoundMessageCallback tests isset($this->typesClone->container),
which must be a quiet false on the clone. A framework-free inherited clone
probe reproduces the error (target/typed-clone-isset-probe.php). A second probe
with NO clone also reproduces it on isset($root->child->value), proving that
cloning is not necessary (target/typed-nested-isset-no-clone.php).

The native isset classifier used a shallow syntactic type for compound receiver
expressions. It therefore missed the existing PropInitialized branch and emitted
a strict read. isset_object_expr_class now delegates to the existing shared
instance_callable_object_class_and_nullability helper. This also has Reflection
and unset consumers; the complete caller graph was checked before editing.

Five new focused regressions in objects/property_access/isset_chains.rs are RED
before this change (target/native-isset-chain-red.log, 5 failed): inherited clone,
ordinary nested leaf, uninitialized/null prefix, method-produced owning receiver,
and private-property magic getter. The last fails at checker visibility, the
other four at uninitialized reads. A replay after only the shared-type change
is currently running as unified session 32178; log
target/native-isset-chain-typed-receiver.log. Do not claim all chains corrected.

An LLDB batch attempt stalled at launch with an unresolved breakpoint spelling;
its own debugger/debugserver were stopped, and its target exited. No debugger
remains attached. Another Cargo build was identified in the php-dom-libxml2-catalog
worktree and left alone. Kage recall for this task is pending (cell 75).

### Native quiet-property probes: latest focused evidence (2026-09-08)

The earlier sessions above are terminal. The expanded nine-test replay in
target/native-isset-chain-magic.log completed with seven passes and two failures.
The inherited clone, uninitialized/null prefixes, nullable method receivers,
private getter result typing and factory-owned callback lifetime now pass.
Still failing: a subclass-added __isset is ignored when the nominal base only
declares __get; a Mixed global receiver reads an inaccessible typed slot before
calling either magic method. These are not full Symfony acceptance results.

A tenth regression, test_native_isset_chain_typed_receiver_survives_global_release,
isolates the latter receiver behind a typed function parameter. It reaches both
callbacks but fails independently on destruction timing: Elephc emits
"has:get:ready", PHP emits "has:get:drop:ready". Evidence:
target/native-isset-typed-lifetime.log, one failed test, 2.72 seconds test execution
after a 23.45-second test build. The reference-retention mechanism remains to be
localized; the test does not establish which acquisition or release is wrong.

Do not substitute a static method_exists query for runtime magic dispatch:
src/codegen/lower_inst/builtins/member_queries.rs lower_member_exists currently
folds an Object-typed receiver against its nominal class. Such a substitution
would preserve the subclass bug. Mixed dispatch has a separate dynamic path.

The live r162 HTTP build still fails on AutowirePass::$container initialization.
No rebuilt Symfony request has validated these quiet-property changes yet.

Lifetime localization controls (same day):
- test_native_typed_receiver_global_release_without_property_probe fails with
  "clear:done" instead of PHP's "clear:drop:done". No property probe or magic
  callback exists in this reducer (target/native-isset-lifetime-control.log).
- test_native_global_object_release_without_argument_owner passes with
  "drop:done" (target/native-global-release-control.log, 1.66 seconds).
  Assigning null through a global binding can therefore release the object;
  the failure requires additional receiver/argument lifetime handling in these
  measured cases. Investigate the typed call boundary before changing the
  newly introduced magic-fetch lifetime pin. Exact leaking reference not yet
  identified. Both controls remain executable regression tests.

CLI cross-check of the lifetime failure: target/global-argument-lifetime.php
compiled with target/debug/elephc also emits "clear:done" (exit zero), so this
is not limited to the in-process test harness. EIR captured in
target/global-argument-lifetime.eir and assembly in
target/global-argument-lifetime-asm/global-argument-lifetime.s show:
- main loads the global Mixed cell without retaining it;
- the nominal guard unboxes and increments the object payload;
- consumeLifetime borrows the typed parameter and calls clear;
- main explicitly decrements the guarded object after the call.
Thus an absent post-call Release instruction is not the cause in this reducer.
The runtime refcount transitions still need measurement before editing cleanup.
Kage context cell 153 terminated with the 300-second tool timeout; no recall
packet was returned by that attempt.

Root cause found in shared-global storage discovery: uses_shared_ref_cell scanned
module.functions and closures but omitted class_methods and generated wrapper
bodies. A global bound only in a method consequently used direct boxed storage
in main, while GlobalRefCell in that method interpreted it as a reference cell.
The scan now includes every function collection used by the EIR printer and
validator. No refcount-helper behavior was changed to compensate for that layout
mismatch. Focused receiver regression replay is running in session 70308, log
target/global-method-refcell-green.log; results are not yet claimed.

LLDB on the symbol-preserving CLI reducer confirmed object refcounts of 3 and 2
at the initialization releases, then still 2 at the release after the call.
The debugger runs terminated; no debugger remains attached. The first stripped
binary could not resolve the breakpoint and was not used as refcount evidence.

Shared-cell correction validation completed: session 70308 exited 101, with
102 passing and six failing receiver-filter tests in 223.27 seconds after
2m17s Rust build. Both newly RED typed lifetime tests now PASS. The six other
failures require baseline comparison; do not label them preexisting yet:
eval_expression_static_receiver_property_writes (4|5: versus 6|5:4),
coalesce_receiver_release_is_balanced_for_owned_and_borrowed_receivers (four
outstanding allocations), reflection_class_tracked_local_receiver_uses_static_metadata
(private ReflectionClass::$__name), mixed_receiver_rejects_non_object_for_typed_method_parameter
(missing expected stderr), mixed_receiver_normalizes_mixed_indexed_array_for_typed_method_parameter
(bad versus ok), regression_525_nullable_receiver_read_twice_heap_clean (100 live blocks).

The complete isset_chains replay then finished with 10 PASS / 2 FAIL in 17.10s
(target/native-isset-global-layout.log, session 47232 terminal 101). Only the
subclass-added __isset and Mixed inaccessible-property probe remain red in that
file. The global storage fix closes the independent typed receiver lifetime
regressions; it does not by itself implement Mixed magic property dispatch.

Baseline isolation completed for the six receiver failures. Only the five newly
added function-collection chains in uses_shared_ref_cell were temporarily removed;
all other compiler changes were retained. The six-filter test replay failed on
all six with identical outputs and heap counts (23.04 seconds execution,
target/global-method-refcell-six-baseline.log, session 14212 terminal 101).
These failures therefore precede the shared-global scan correction, not
necessarily the earlier property/reflection work in this dirty tree. All five
chains have been restored in source. The current debug/test binaries were built
for the baseline without those chains and must be rebuilt before validating the
restored source. Kage risk cell 211 timed out after 300 seconds with no report.

Polymorphic member-probe correction in progress: the new regression
test_native_member_exists_uses_runtime_subclass_of_typed_receiver was RED with
"0:0|0:0|" versus PHP "0:0|1:1|" (target/member-exists-subclass-red.log).
Object-typed method_exists/property_exists now use the existing runtime member
registry instead of folding against the nominal class; feature discovery also
requests that registry for Object operands. Class-string constant folding is
unchanged. The quiet magic-prefix lowering now checks runtime __isset presence
when only a descendant declares it, using existing typed RuntimeFnId::MethodExists
and native subclass method dispatch. No extra boxing or interpreter fallback
was introduced for that lookup. Full isset_chains replay is running as session
93717, target/native-isset-runtime-method-lookup.log. No GREEN claim yet.
The update-builtin-docs skill has been read for this builtin-lowering change;
exporter regeneration and documentation/architecture audits remain required.

Runtime member lookup replay completed (session 93717 terminal 101): 12 PASS /
1 FAIL in 15.71 seconds, including GREEN for both polymorphic member_exists /
property_exists and the descendant-only __isset regression. The sole remaining
isset_chains failure is the Mixed inaccessible typed-property read, before magic
callbacks. The restored shared-global scan is included in this tested build.

Builtin documentation workflow completed for this change: exporter build,
extract_builtins.py --render --force, audit_builtins.py, validate_site_compat.py,
and audit_builtin_eir_boundary.py --enforce-target-architecture all exited zero.
Logs use target/member-probes-builtin-* prefixes. Existing generated documents
were backed up to target/member-probes-builtin-docs-before-regeneration.tar.gz.
Comparison against the extracted backup found no changed generated page or
registry content, preserving the preexisting dirty documentation. These audits
do not establish full builtin semantic parity or executable cross-target coverage.

Next end-to-end checkpoint started: release compiler rebuild in session 12050,
target/quiet-property-fixes-release-compiler.log, single Cargo job, incremental
disabled, release optimization level 1 / 256 codegen units / LTO disabled (same
profile as r162). This includes the quiet typed-property fixes, global storage
scan and polymorphic member lookup. Do not start another compiler build while
this handle is live. The remaining Mixed regression is explicitly still open;
this checkpoint tests whether the independently reproduced AutowirePass failure
is crossed, not whether all PHP property semantics are complete.
Free disk at launch was 3.6 GiB. Existing DateTime tests and a curl builtin exporter
were identified as outside this task and left running. r161 and r162 output
directories each occupy 202 MiB, including retained compressed assembly evidence.
The native mixed_property_get helper handles stdClass only; redirecting arbitrary
private-property reads to it would silently omit native magic callbacks and is
not a valid correction for the remaining Mixed case.

r163 checkpoint completed: compiler release build succeeded in 10m18s (session
12050 terminal zero). Native doctor reports installed PCRE2 10.47 and healthy
lock/cache. Symfony --web compiled successfully in 115.61 seconds, 115.92 wall,
161 MiB with symbols, runtime cache MISS and zero assembler slices reused,
peak footprint 4667413016 bytes (target/symfony-web-r163.stderr; build session
62573 terminal zero). No new Magician source change required an archive rebuild.

Server session 41040 listens on 127.0.0.1:19090 with one worker and eval tracing;
r162 remains on its separate port. Curl returned HTTP 000 / empty reply in
0.091120 seconds. Worker 43757 exited 1 with uninitialized
Symfony\\Component\\DependencyInjection\\Container::$parameterBag. This request
loaded the existing generated ContainerTvPT0Dp cache, so it is NOT proof that
the cold AutowirePass gate was traversed. The generated class overrides
getParameterBag with lazy isset-based initialization at lines 421-431, while
the native base accessor reads the field directly. Trace also contains an
earlier caught TypeError before a second generated container construction.
Investigate actual dispatch/caught error before attributing the fatal solely
to a missing constructor or quiet probe. Symfony/vendor sources were not edited.
The 881 MiB generated index.s was gzip-compressed to index.s.gz and integrity
checked; original assembly can be recovered by decompression.

r163 dispatch reducer: target/dynamic-override-probe.php returns 42 under php -n,
but its release-compiled binary fatals reading the native base's uninitialized
property. A class declared by eval overrides read() to lazily initialize it;
a separately compiled typed consumer calls the base method instead. Formal
regression test_native_typed_call_honors_dynamic_override_initializing_property
is RED (target/dynamic-override-red.log, 15.86 seconds).
lower_method_call gated lower_eval_owned_method_call on caller-local eval state,
although that helper explicitly passes a null caller context and resolves the
object's dynamic owner. Gate changed to module eval-bridge capability, retaining
native fallback for objects without a dynamic owner. Targeted test now running:
session 67072, target/dynamic-override-green.log. Not validated yet. Nullable,
interface and Mixed dispatch siblings, argument ownership and overhead still
need review. This reducer does not explain the preceding caught TypeError in
the full Symfony request; that remains separate evidence to investigate.

Typed dynamic-override regression GREEN (session 67072, 16.72 seconds), but
expanded coverage found four failures out of five (session 22305 terminal 101,
target/dynamic-override-siblings.log, 84.45 seconds): nullable/interface/Mixed
receivers still call the base, and a private parent method name collision calls
the child's public method instead of the parent's private implementation.
The owner probe has now moved to the common lower_method_call entry, before
native receiver dispatch branches, with private/final methods excluded from
dynamic override lookup. The old typed-only probe was removed. Replay session
21211 is active, target/dynamic-override-common-dispatch.log. No all-green claim
yet; by-ref arguments, lexical protected visibility, exception cleanup and the
per-call allocation overhead of this broader probe remain audit surfaces.

Common dispatch replay GREEN: all five dynamic-override tests passed in 79.69
seconds (session 21211 terminal zero, target/dynamic-override-common-dispatch.log).
This includes typed, nullable, interface, Mixed and private-name-collision cases.
Added two follow-up differential tests for a by-ref integer argument and a
protected override invoked from an inherited native method. Replay session 18600
is active, logging to target/dynamic-override-reference-scope.log.
No new release compiler or Symfony rebuild has included this dispatch correction.

Reference and protected-scope follow-ups GREEN (session 18600, two passed in
31.52 seconds). A native-call heap reducer initially passed with empty eval;
that was insufficient capability coverage. With a real dynamic class declaration,
128 otherwise native calls leave 397 allocations / 263 frees, RED in
target/dynamic-override-native-heap-active.log. The owner dispatch prepared its
argument pack before knowing whether the receiver had a dynamic owner.
Added a non-allocating raw-identity owner query and moved the native miss branch
ahead of argument packing in lower_eval_owned_method_call. Focused replay is
running in session 33783, target/dynamic-override-native-fastpath.log. The new
FFI symbol requires an updated Magician archive for the next release/HTTP run.
Residual fixed-cost heap state and dynamic-owner argument cleanup are not yet
proven balanced; the measured 134-block mismatch is not all attributed blindly
to the 128 calls.

Fast-path evidence: native-call heap reducer now reports 141 allocations / 135
frees, removing the 128 per-call live blocks (session 33783 terminal 101).
The no-method-call dynamic declaration control reports 10 / 4, the same six
residual live blocks (target/dynamic-owner-fastpath-controls.log). The strict
heap tests remain RED; no baseline subtraction weakened their assertions.
Reference argument, interface and private-collision checks passed with the
owner preflight enabled (session 74705: three passes, one baseline heap failure).

Current debug compiler emitted the dynamic-override CLI reducer for Linux x86_64,
Linux AArch64, iOS device and iOS simulator. iOS requires --emit staticlib even
with --emit-asm; initial executable-mode requests were rejected and rerun with
that flag. Artifacts/logs are target/dynamic-owner-<target>*. This is emission
evidence, not Linux execution or iOS host-link validation. The outstanding six
baseline allocations and dynamic-owner argument-marker cleanup remain open.

Release preparation for r164: standard release Magician archive rebuilt in
54.56 seconds (session 1725 terminal zero, target/dynamic-owner-release-bridge.log).
nm confirms the new __elephc_eval_object_has_dynamic_owner export is present.
Compiler release rebuild now active in session 35489, logging to
target/dynamic-owner-release-compiler.log with the same single-job opt-level-1 /
256-codegen-units / no-LTO profile as r163. Do not restart while live. Multi-target
probe assembly files were gzip-compressed and gzip-tested; source artifacts
remain recoverable as target/dynamic-owner-<target>/dynamic-override-probe.s.gz.
No Symfony request has yet exercised the common dynamic-owner dispatch fix.

2026-09-09 interruption recovery: session 35489 is missing, no Cargo/rustc process
is running, and its log stops after the library warnings without a successful
build footer. Restarted the same compiler build only after those terminal-state
checks. New session 94837 logs to target/dynamic-owner-release-compiler-resumed.log.
Free disk is 4.2 GiB. During the previous run the unused r161 binary was compressed
to target/symfony-web-r161/index.gz (28 MiB) and gzip-tested; it can be recovered
by decompression. r164 has not yet been compiled or tested over HTTP.

r164 checkpoint (2026-09-09): resumed compiler build succeeded in 5m40s, session
94837 terminal zero. First Symfony build stopped at 76.22 seconds because the
locked PCRE2 artifact had disappeared again (target/symfony-web-r164.stderr).
native install --locked --target macos-aarch64 restored it without manifest/lock
edits. Retry session 85494 succeeded: 118.83 seconds compiler / 119.16 wall,
164 MiB with symbols, runtime cache hit, zero assembler slices reused, peak
footprint 6396401112 bytes (higher than r163, remains a performance audit item).

Server session 73723 listens on 127.0.0.1:19091 with one worker and eval tracing.
Curl still returns HTTP 000 (empty reply, 0.121468 seconds). The request now
reaches generated service loading instead of the previous base-property fatal.
It fails binding the native constructor arguments for
Symfony\\Component\\DependencyInjection\\Argument\\ServiceLocator, at generated
getContainer_EnvVarProcessorsLocatorService.php line 21. Trace signature is
Some((3, 3, 2, true)), supplied argument count 3, error stage bind. Source
constructor parameters are Closure, array, ?array; the actual arguments are a
first-class method closure through ??=, an associative service map and an
associative type map. Isolate these PHP shapes without framework-specific code
before selecting a fix. The generic "unsupported NewObject" message is not a
complete root-cause diagnosis. Worker 78648 exited 1. Logs use r164.trace prefixes.
Generated r164 index.s was compressed to index.s.gz and gzip-tested successfully.

r164 constructor reducer: target/constructor-closure-map-probe.php reproduces
the bind failure; target/constructor-closure-only.php fails with just Closure,
so associative arrays are not necessary. PHP accepts both. Formal regression
test_eval_closure_argument_is_accepted_by_native_constructor is RED in
target/native-constructor-closure-red.log (17.59 seconds).
eval_method_parameter_class_accepts used a local declaration-only class walk
and then the raw native relation callback, omitting the shared dynamic Closure
handling already used by is_a/instanceof. It now consumes dynamic_object_is_a
before falling back to native metadata. This reuses existing class relation
semantics, rather than introducing a constructor- or framework-specific rule.
Replay session 93306 is active, target/native-constructor-closure-green.log.
Actual callable execution/storage, first-class method closures, rejected
non-Closure objects and callable signatures still require focused coverage.

Constructor Closure acceptance GREEN: session 93306 completed with one passing
test (31.07 seconds including the harness archive refresh). A stronger regression
now stores the closure in a promoted private Closure property and calls it from
native code, for both an eval arrow function and a first-class native method
closure. Session 91657 is active, target/native-constructor-closure-invocation.log.
The last release archive predates this relation-check correction; it must be
rebuilt before the next Symfony link, but no compiler source changed in this fix.

r165: closure storage/invocation test GREEN (session 91657, 17.69 seconds).
Release Magician rebuilt in 1m00s, native doctor healthy, then Symfony compiled
in 120.73 seconds / 121.03 wall, 164 MiB, five of eight assembler slices reused,
peak footprint 6550230392 bytes (target/symfony-web-r165.stderr). Server session
10001 listens on 127.0.0.1:19092. Curl returns HTTP 000 in 0.155498 seconds;
worker 82067 exits 255 with Class "" not found.
The previous constructor gate is demonstrably crossed: trace lines 165-169 show
three arguments bound, generated load success=true, then native Container::get
returns tag Object with a nullable generic object return contract. The next
native Container::getEnv invocation throws. Suspect the nullable generic-object
method-dispatch path (class name empty) before inferring an empty PHP class name
in the application. Needs a reducer. r165 index.s compressed and gzip-tested;
logs and binary retained. No valid Symfony HTTP response yet.

r165 generic nullable reducer confirmed: target/nullable-generic-object-probe.php
returns yes under PHP but Class "" not found under the release compiler. No eval
or framework is needed. Three new tests in property_access/nullable_generic_calls
give two RED (nonnull ordinary and nullsafe call) and one GREEN (actual null
throws the expected member-call Error), target/nullable-generic-calls-red.log.
lower_nullable_receiver_method_call now routes an empty nominal class name
(the internal generic object representation) through the existing runtime
candidate dispatcher before named-class lookup. Replay session 59182 is active,
target/nullable-generic-calls-green.log. Undefined-method diagnostics in the
shared candidate dispatch remain a separate unproven edge case.

Nullable generic call replay GREEN: three passed in 4.89 seconds (session 59182,
target/nullable-generic-calls-green.log). The missing-method follow-up is RED:
expected Call to undefined method GenericMissingReceiver::missing(), got Call
to a member function missing() on null (target/nullable-generic-missing-method.log,
session 14656 terminal 101). The shared narrowed nullable dispatcher emits the
null diagnostic both for empty candidate sets and nonmatching real objects.
Keep the null branch separate from the nonnull missing-method branch. Existing
emit_dynamic_object_class_name and the dynamic throwable string-result machinery
can construct the actual-class diagnostic without a generated message per class.
No new release build includes the nullable generic dispatcher fix yet.

Missing-method diagnostic patch applied: removed the premature empty-candidate
null fatal from raw/nullable narrowed dispatch. Nonmatching nonnull receivers
now format Call to undefined method <runtime class>::<method>() through existing
runtime class-name lookup and concatenation, persist the message, and throw a
catchable Error. Added emit_error_from_string_result by extracting the existing
emit_error_value tail; null and caller-selected TypeError branches stay separate.
No per-class message table was introduced. Replay session 44123 is active,
target/nullable-generic-method-errors.log. Magic __call and the broader Mixed
dispatch missing-method semantics remain unproven, not silently claimed complete.

Nullable/generic method suite GREEN: session 44123 passed all four original
cases, then the expanded replay passed all six in 8.09 seconds (session 15534,
target/nullable-generic-complete-replay.log). Added nonnullable generic-object
missing-method and nonmatching-candidate coverage. Both now report the real
receiver class; the actual-null error and nullsafe argument laziness stay green.
Release compiler rebuild started in session 37679 for the next Symfony checkpoint; log
target/nullable-generic-release-compiler.log. No HTTP result with this correction
yet. Six tests are focused evidence, not full method-dispatch or magic-call parity.

r166 checkpoint: compiler release build completed in 14m32s. Initial Symfony
assembly failed with ENOSPC after 167.56 seconds; no process remained active.
The failed output held 1.8 GiB of main/slice assembly. Preserved all these files
as gzip archives and verified them. Cargo package-scoped dev cleanup removed
942 elephc files (1.7 GiB) and 1440 elephc-magician files (1.2 GiB), after dry runs
and process checks. Release compiler/archive, sources and logs were retained;
dev artifacts must be rebuilt when needed. Logs: target/r166-*-clean*.log.

Retry used ELEPHC_ASM_JOBS set to 2 and a fresh target/symfony-web-r166-retry
directory. Build succeeded in 181.35 seconds / 181.74 wall, 164 MiB with symbols,
runtime cache hit, two assembler slices, zero reused, peak footprint 4790702376
bytes. This configuration is slower than the 1-2 minute target; memory is lower
than r165, but differing cache/code state prevents attributing all timing change
solely to the job limit. Retry index.s compressed and integrity-checked.
Server session 25492 listens on 127.0.0.1:19093. Curl returns HTTP 000 in 0.122526s.
The empty-class error is gone; execution now reaches the first-class callback
to native Container::getService and fails at invoke with RuntimeFatal, followed
by "eval callback dispatch failed". Worker 68761 exited 1. The constructor bind
and generated service load both succeed before that error. Next: reduce callback
unpacking/union parameters/visibility using ordinary PHP fixtures.

Read-only constructor_parity_review audit found no precise static defect in the
audited generic/null/missing-method branches; this was narrow static evidence,
not global or requested multi-model consensus. Separately, streamed r164/r165
assembly fingerprints found 3 changed equal-sized segments among 5763: FileLoader
findClasses, RouteCollection addNamePrefix, and main plus its trailing data.
The two isolated function diffs show changed load_local order/stack offsets:
target/r164-r165-unstable-functions.diff. Reviewer hypothesis points to LocalSlotId
creation around foreach; compare local descriptors/IR before blaming frame layout.
No determinism fix applied and no whole-main attribution proven.

r166 callback reducers: target/inherited-callback-unpack-probe.php reproduces
the exact native callback invoke failure with an eval-declared subclass of a
native base, a first-class inherited method closure, and a native consumer.
PHP prints storage:id:method:1. Public-method and explicit-four-argument variants
also fail; protected access and unpacking are not necessary. Original protected
source was restored after variant builds. Formal inherited_callbacks regression
is running in session 46947, target/inherited-callback-red.log.
The first alternate probe (protected-callback-unpack-probe.php) exposed a separate
earlier failure: a native Closure return arrives with runtime tag 10 and is
rejected by its Callable return contract. Do not conflate it with this callback.

LLDB at __elephc_eval_value_method_call, conditional on method-name length 7,
confirmed method resolve and a correctly forwarded CallbackBase scope. The
boxed receiver's first word was 0x000000016d657469 rather than object tag 6
(low bytes spell item), while the argument array had tag 4. Neither the native
method body nor its prep-fail entry was reached before callback failure. This
points to invalid/reused captured receiver-cell storage, not an absent method;
nm confirms native body and bridge are present. Investigate closure receiver
ownership and establish the exact overwrite/free before choosing a retain fix.
Debugger sessions completed; no debugger is intentionally left attached.

Closure receiver ownership correction: EvalClosureObjectTarget exposes its
optional receiver; common closure materialization retains that cell and stores
the returned retained handle. Closure binding now reuses the same constructor.
The dynamic Closure destructor releases the owned receiver before unregistering
the closure, so the closure lifetime still pins its context during nested object
destruction. Allocation/identity/retain failure paths release the new closure
object. No retention was added without its normal destruction counterpart.
The inherited callback reducer is GREEN (session 11905, 33.06 seconds including
archive refresh), target/inherited-callback-retained-receiver.log.
Added variable-receiver and temporary-receiver destructor timing regressions;
expanded replay is session 9208, logging to target/inherited-callback-lifetime.log. Their
outcomes are pending. Escaping receiver temporaries, cycles, bound closure clones
and error-path cleanup remain audit surfaces; no HTTP replay yet.

Temporary receiver follow-up: first-class method/invokable creation now reuses
the ordinary method receiver cleanup, including errors during method-name or
target validation. The receiver retained by the closure is independent of the
original temporary. Expanded inherited_callbacks suite GREEN: five passed in
122.52 seconds, session 80144, target/inherited-callback-temporary-cleanup.log.
Read-only reviewer found no precise retain/release or nested-finalization defect
in the audited paths; self-referential targets/cycles remain unvalidated. This
is not the requested full multi-model consensus.

r167: release Magician rebuilt in 1m32s; native doctor healthy. Package-scoped
Elephc dev cleanup removed 83 files / 849.7 MiB after a dry run and idle-process
check, keeping release artifacts and logs. No Magician cleanup followed the
later dry run because the build had already finished. Symfony build succeeded
in 197.50 seconds, 164 MiB, two assembler slices with one cache hit, peak footprint
4891185544 bytes. Server session 49421 listens on 127.0.0.1:19094.
Curl returns HTTP 000 in 0.111976 seconds; worker 20485 exits 1. The callback
failure is crossed. New failure is an InterfaceDecl RuntimeFatal while loading
vendor/symfony/dependency-injection/EnvVarProcessorInterface.php line 21. Its
surface is an instance getEnv(string,string,Closure):mixed plus a static
getProvidedTypes():array. The generic "unsupported InterfaceDecl" diagnostic
does not yet establish which declaration guard rejected it. Trace is r167.trace.
r167 index.s compressed and gzip-tested, source recoverable from index.s.gz.

r167 interface diagnosis: execute_interface_decl_stmt rejects already-known
runtime interfaces. Generated getContainer_EnvVarProcessorService explicitly
include_once-loads the interface source. The generic two-file reducer under
target/native-interface-once/ prints once|ok under PHP but redeclares its interface
under Elephc after an earlier native require_once. Native IncludeOnceMark/Guard
only touch hashed module-global flags; eval uses canonical-path context/request
sets, with no observed synchronization. Do NOT relax redeclaration checks.
The full Symfony file's expected FNV guard _include_once_b13866395699f262 is absent
from nm r167, despite a matching realpath, so discovery-vs-activation may be an
additional issue; registry synchronization alone is not yet proven sufficient.

Four CLI-backed regressions added in type_builtins/includes/native_eval_once.rs
cover native-to-eval, eval-to-native, an interface repeat, and a skipped native
interface include first executed in eval. Session 89315 is active, logging to
target/native-eval-include-once-red.log. A read-only reviewer is investigating
declaration visibility/provenance, without builds or modifications.
For disk headroom, unused r151-r160/r162/r163 index binaries were gzip-compressed
after lsof found no users, then integrity-checked. They remain recoverable; live
r164-r167 servers were not touched. Free space rose from ~1.6 to ~3.0 GiB.

All four native_eval_once tests are RED (session 89315 terminal 101, 99.33 seconds):
both side-effect-only orders print load twice, and both interface cases fail
declaration, including the skipped native branch. Reviewer confirms discovery
hoists interface metadata separately from runtime include guards and eval's
canonical-path registry. Wrote .plans/native-eval-inclusion-spec.md to capture
the required separation and fail-closed acceptance cases; proposed, not locked.
Initial SHA-256: e5b947cfa89453be8a9f9bf1b023ca0fa638e9feb2a6022119e4933f3e0254ef.
No redeclaration check was relaxed and no discovered file was marked loaded.

Ollama inventory confirms glm-5.2:cloud, kimi-k2.7-code:cloud and kimi-k3:cloud are
available. Started a bounded GLM review of the exact initial spec and supplied
source packet, session 75409; output target/native-eval-inclusion-glm-review.md.
The reviewer has provided text only, not autonomous filesystem access. No local
model weights or dedicated Codex instance were launched. Review/locks pending.

Initial spec reviews: GLM 5.2 and Kimi K2.7-code both returned NO LOCK for hash
e5b947cfa89453be8a9f9bf1b023ca0fa638e9feb2a6022119e4933f3e0254ef. Their concerns
about underspecified ownership/state/file entry are valid review inputs, but
their assertions that declared_once insertion eliminates conditional runtime
includes and that these CLI tests use cfg(test) stubs are not accepted facts.
No declared_once.contains use was found; resolver exports its set as manifest
input. Production-mode archives are exercised by the CLI-backed regressions.
GLM final text was separated from CLI thinking/terminal controls into
target/native-eval-inclusion-glm-final.md; original output is preserved.

Measured php -n 8.5.6 oracle in target/include-state-oracle/: opening an include-once
file then throwing leaves it registered; unconditional class/interface/function
declarations after the throw are already visible. A ParseError ALSO leaves the
file registered and a second include_once returns true, but its class is absent.
A missing file remains unregistered and retries on each call. Output recorded in
target/include-state-oracle/php.stdout. Separate file registration from successful
compilation/early binding and body execution; do not implement blanket rollback.
Kimi K3 intermediate review started with the same spec, peer reviews, source and
oracle evidence, session 95556, target/native-eval-inclusion-kimi3-review.md.
This is not the final whole-project Kimi audit and no consensus is established.

Kimi K3 initial review completed NO LOCK, agreeing with the oracle and rejecting
the peers' unsupported conditional-elimination/test-stub claims. Archived v1 as
.plans/native-eval-inclusion-spec-v1.md; its hash matches the reviewed original.
Revision 2 now explicitly separates registration, early binding/activation and
body execution; specifies typed source identity, one logical request registry,
compiled file-entry/caller-scope boundaries and native-only dependency constraints.
New spec SHA-256 d84fab6cb405c37708947dbb955d6327cada0c34f7f30cd21103ff79bd3c6087.
GLM revision-2 review is active in session 45312, with output at
target/native-eval-inclusion-v2-glm-review.md. No previous review locks apply to
this new hash. No inclusion/activation implementation change has been made yet.

Revision 2 GLM review returned NO LOCK with concrete requests for explicit native
reset, early-binding prologue and descriptor installation/lookup ordering. Checked
src/codegen/web.rs: current reset calls only the dynamic include-state reset, not
the native once comm guards. Revision 2 archived with matching d84fab6c hash.
Revision 3 clarifies RequestIncludeState ownership/reset (including native-only
programs), eligible early bindings before body execution, conditional activation
events, source descriptor installation before PHP execution, lookup before source
byte parsing, and exact include return types. Current SHA-256:
459e29a0c61812134bd5fd53565f56f6dccce0290509874f1af122134353883a.
GLM 5.2 LOCKED this exact hash (target/native-eval-inclusion-v3-glm-review.md).
Kimi 2.7 sequential review is now running in session 57603; output
target/native-eval-inclusion-v3-kimi27-review.md. Kimi K3 and root consensus are
not yet established; no implementation readiness is inferred from GLM alone.

Kimi K2.7-code review completed LOCK for the same revision-3 hash
459e29a0c61812134bd5fd53565f56f6dccce0290509874f1af122134353883a.
Kimi K3 final spec review (not the final implementation audit) is now running,
output target/native-eval-inclusion-v3-kimi3-review.md. GLM and Kimi 2.7 locks
cover the specification only; runtime gates and final whole-code audits remain open.

Kimi K3 revision-3 review is active in session 61028. Added a read-only source
inventory in .plans/native-eval-inclusion-implementation-map.md identifying the
resolver capture boundary, provenance-only maps, IR module construction, native
guards, candidate native scope helpers and web reset boundary. No implementation
was started while the final specification review is pending.

Revision-3 specification consensus obtained: Kimi K3 LOCKED the same hash as GLM
and Kimi 2.7; root accepts that contract. Evidence and limits recorded separately
in .plans/native-eval-inclusion-consensus.md to preserve the frozen spec hash.
This is not final implementation consensus or HTTP acceptance.

First implementation slice: resolver SourceUnit snapshots preserve canonical
PathBuf identity, SourceMode and shared lexer input before include-site rewriting.
A shared collector survives isolated resolver scope clones; deterministic keyed
collection rejects conflicting content/mode without replacing the first snapshot.
IncludedDeclarationSources now returns those snapshots. This does not activate
types or mark files included, and is not yet wired to IR/runtime source entries.
Three collector tests passed (target/source-unit-snapshots-tests.log). That first
--lib command also visited other workspace libraries with zero matching tests;
subsequent runs are package-scoped. A real resolver capture regression was added;
focused four-test replay is session 64037, target/source-unit-snapshots-focused.log.

Source snapshot replay completed: four resolver tests passed. Autoload now returns
its own pre-transformation source text plus nested include snapshots, and merges
them through the same checked SourceUnit::insert_into operation. Conflicting
content/mode is rejected rather than overwritten. Two autoload tests were added.
An initial assertion used mixed-case declaration-map keys incorrectly; fixed the
test to use the existing php_symbol_key normalizer, without changing name logic.
All six package-scoped source_units tests pass (session 16463 terminal zero,
target/source-unit-autoload-replay.log). Source collection is not yet connected
to IR source entries or runtime activation; the four cross-engine includes remain
open. Main-source capture and pipeline/test-harness catalog wiring are next.

Main frontend now returns its finalized AST plus the immutable original source
snapshot using the invocation's actual source mode. CLI pipeline merges main,
explicit-include and autoload snapshots with checked conflict detection into
Module.source_units. This is compiler metadata only, not published runtime source
entries or activation state. cargo check -p elephc --lib --bin elephc passed
(target/source-unit-pipeline-check.log). Added a binary frontend unit test proving
the snapshot retains __FILE__ while the finalized AST substitutes it; session
77383 is active, target/entry-source-snapshot-test.log. Other lowering/test-harness
catalog wiring and typed SourceId assignment remain pending.

Entry frontend snapshot test GREEN (session 77383, one test). Added the public
checked Module::add_source_units merge and used it from CLI and the primary
in-process codegen harness. That harness now collects include provenance/snapshots
with invocation defines and merges include + autoload + synthetic entry metadata,
instead of discarding include provenance. Six nullable-generic regression tests
pass through this updated path (session 35801, 6.27 seconds,
target/source-unit-harness-replay.log). Project helpers in support/projects.rs
still need the equivalent metadata flow; this test replay is not coverage of
cross-engine include semantics or runtime source registration.

Project harness metadata flow updated: generate_project_asm now receives source
inputs and merges main/include/autoload snapshots plus declaration provenance.
The multi-file success/failure and stdin helpers use collecting resolver APIs;
the defines-aware helper also propagates defines through autoload collection.
Seven existing include_once regressions pass (session 6028, 4.41 seconds), then
six focused failure/ifdef/shared-scope/stdin regressions pass (session 26025,
3.80 seconds). Logs: target/source-unit-project-harness-tests.log and
target/source-unit-project-paths-tests.log. This is metadata plumbing validation;
the four native/eval inclusion regressions and Symfony HTTP are still open.
Next: freeze typed source identities and wire runtime source state/entry consumers,
including any remaining library lowering entry paths rather than only the CLI.

Typed source identity implemented: SourceCatalog checks/merges immutable inputs,
assigns SourceId values in canonical PathBuf order and exposes read-only lookup.
Module::bind_source_units freezes the catalog once; conflicting input leaves it
unbound and subsequent binding cannot renumber existing IDs. Diagnostic Debug
output lists identities rather than dumping retained source bodies. CLI and both
codegen harness paths use this binding API. Nine source_units tests pass, including
order independence, duplicate/conflict handling, atomic binding and distinct
non-UTF8 path bytes (session 98123, target/source-catalog-tests.log). Consumer
type-check is running in session 17235, target/source-catalog-consumers-check.log.
IR inclusion opcodes and runtime state do not yet consume SourceId; no HTTP fix
is claimed from the catalog tests alone.

SourceCatalog consumer check completed successfully (session 17235): compiler
binary and codegen integration-test target type-check with the frozen catalog API.
No runtime inclusion behavior has changed from this metadata-only step.

Native web once-guard reset implemented in src/codegen/web.rs. Reset inventories
IncludeOnceMark/IncludeOnceGuard operands across all seven EIR function collections,
deduplicates them deterministically, and clears only emitted comm storage after
PHP-visible global/static cleanup and before heap reset. It does not require the
eval bridge and does not infer inclusion state from symbol prefixes.
The first test attempt failed in the fixture itself (no selected EIR insertion
block), not on the product defect; both original red/green-named logs are therefore
invalid regression evidence. After correcting the fixture, disconnecting the reset
call produced the expected "native guard must reset" failure in
target/native-include-web-reset-baseline.log (session 10241, exit 101). Restoring
the call passed the same test in target/native-include-web-reset-fixed.log
(session 9184, exit zero). This test emits the reset for all five supported targets,
checks a method-only guard without Magician, excludes unrelated prefixed storage,
and checks ordering before heap reset. It is emission evidence, not a repeated
HTTP-request execution test. Assembly-comment check and scoped diff check pass.
Cross-engine registration/activation and r167 HTTP remain open. Free disk after
validation is 1.6 GiB; do not start a full application build without rechecking.
Kage context timed out after 300 seconds; local source/plan evidence was used.

Remaining source catalog inventory correction: lower_source_at is currently in
src/ir_lower/tests/mod.rs, not src/codegen/mod.rs. It still calls the resolver
without collecting source metadata, discards autoload provenance and returns an
unbound module. Wire this path before claiming catalog coverage of lowering tests.

Lowering-test helper now uses collecting resolver/autoload APIs, merges declaration
provenance, and binds original entry/include/autoload source snapshots. New test
lowering_source_catalog_preserves_original_entry passes in
target/lowering-source-catalog.log (session 70153). Focused lowering replay excluding
corpus/exhaustive tests gives 33 passed, six failed (session 16054,
target/lowering-source-catalog-replay.log). Temporarily reverted the ENTIRE helper
change while retaining the new test, excluded that test from baseline replay:
32 passed and the same six failed (session 50920,
target/lowering-source-catalog-baseline.log). The new test alone then failed with
"lowering must retain source inputs" (session 33950,
target/lowering-source-catalog-red.log). Restored the exact tested patch; git diff
compared byte-for-byte equal to target/lowering-source-catalog-helper.patch.
The six open failures are builtin_runtime_calls_use_descriptor_result_representations,
eval_bridge_keeps_virtual_dispatch_conservative, inaccessible_instance_calls_remain_throwing,
instance_calls_union_closed_world_override_effects,
property_reads_refine_typed_slots_and_magic_getters, and
mixed_parameter_array_push_uses_explicit_opcode. Their missing EIR/type assertions
remain open; no expectations were weakened. This only isolates them from the helper
change, not from all earlier branch work.

Additional production gap found: src/pipeline/eir_output.rs currently performs a
separate lowering before native output's source-catalog/provenance binding. Unify
module preparation for textual and native output, and provide the frozen catalog
before lowering emits SourceId operands. The current post-lowering binding does not
yet meet that requirement. Symfony r167 HTTP and shared runtime inclusion remain open.

Textual EIR output now consumes the same prepared Module as native output: moved
the emit_ir branch after shared lowering/catalog/provenance binding in pipeline.rs
and removed duplicate lowering from pipeline/eir_output.rs. A debug assertion pins
the emitter's prepared-catalog contract. Cargo check of the compiler binary passed
(session 48988, target/shared-output-lowering-check.log); the real CLI regression
test_cli_emit_ir_prints_eir_only passed (session 25880,
target/shared-output-lowering-cli.log), including no assembly/object/binary outputs.
The catalog is still bound AFTER lowering; providing it before SourceId emission
remains the next step. IncludeOnceMark inventory spans resolver production, AST
rewriters, EIR lowering/validation and backend/reset consumers; preserve those together.

Disk dropped to 285 MiB after CLI validation, then filled before a compressed CLI
backup could be created. No backup was produced. Ran scoped cargo clean for elephc
dev artifacts in THIS worktree only (session 66792, exit zero): 163 files/1.4 GiB
removed. Source, test logs, bridge artifacts and target/release/elephc preserved;
the just-tested debug compiler must be rebuilt. Free disk afterward: 1.4 GiB.

Pre-lowering source identities wired: Module::with_source_catalog constructs an
empty module with a frozen catalog; lower_program_with_source_catalog passes it
into program::lower before metadata or any body emission. Existing public lowering
APIs retain their no-catalog behavior for compatibility while remaining consumers
are migrated. The CLI constructs/validates SourceCatalog before invoking lowering,
so textual EIR and native output both see it from the start. The lowering-test
helper now uses this same early-catalog API rather than binding afterward.
Three focused source_catalog tests passed (session 62683,
target/prelower-source-catalog-tests.log), including constructor immutability and
the helper's retained entry snapshot. Compiler library/binary check passed with
four existing unused-import warnings (session 48232, 41.05 seconds,
target/prelower-source-catalog-cli-check.log). Scoped diff check passed.
Remaining codegen test helpers still bind catalogs after lowering and need migration
before inclusion opcodes require typed source operands. Native/eval once state is
still separate; no new Symfony HTTP attempt or closure is claimed.

Codegen fixture helpers migrated to early catalogs: compiler.rs and projects.rs
construct SourceCatalog before calling lower_and_validate_ir_for_codegen_fixture;
that helper now requires the catalog and invokes lower_program_with_source_catalog
before EIR optimization. Existing declaration provenance merging remains unchanged.
The package-scoped codegen integration target type-check passed in 28.30 seconds
(session 21497, target/prelower-codegen-harness-check.log). Scoped diff check passed.
No executable fixture replay in this step: free disk is 1.2 GiB and the previous
full integration rebuild consumed that margin. Search of src/tests now finds
bind_source_units only at its definition and catalog immutability unit tests.
This does not establish that every old low-level lower_program caller supplies a
catalog; inventory synthetic/manual AST callers when typed inclusion is made mandatory.
Next semantic change: carry canonical source identity from resolver AST markers
through SourceId EIR operands, validation, codegen and web-reset inventory. Do not
recover file identity by reversing hashed guard labels. Shared registry, native
file entries, activation and Symfony HTTP acceptance remain open.

AST inclusion identity now retains PathBuf source_path rather than a pre-hashed
label. Resolver stores the canonical path, and conditional/magic/name/alias/AST
optimizer rewriters preserve it. Binding-decision, mixed-storage and strict-PHP
visitors were updated; synthetic statement fixtures now carry paths too. EIR
lowering still uses the legacy label adapter at this intermediate step, so emitted
native guard behavior remains unchanged and collision/runtime interop is NOT fixed.
The FNV adapter is now explicitly documented as non-unique and temporary.

Initial check caught three remaining field consumers, fixed before replay. Library,
CLI and test-target checking passed in 1m44s (session 31673,
target/include-source-path-check-replay.log). Eleven source_units tests passed
(session 50797, target/include-source-path-tests.log), including the new resolver
path-preservation test through conditional/magic/name/alias/folding rewrites.
Targeted lowerer test lowers_include_source_path_markers passed (session 73778,
target/include-source-path-lowering-test.log). Additional all-statement smoke failed
with Heap(Hash) versus Heap(Mixed), target/include-source-path-stmt-smoke.log;
its provenance has NOT been established and it remains open. No expectation was
weakened. Scoped src/tests whitespace check passes; global diff check reports two
unrelated trailing spaces in examples/symfony-app/config/reference.php, untouched.

For the next step, LoweringContext has a mutable DataPool reference, not a Module
reference. Its single constructor call is in function.rs::lower_body_into_function
(eight callers). Provide read-only source lookup without cloning the entire catalog
and source map once per PHP function. SourceId EIR operands and shared request
registration/activation/native source entries still need implementation.

Typed inclusion operands implemented. Module shares its immutable SourceCatalog via
Arc; all eight lowering-body entry paths receive that handle, and nested closures
inherit the parent's handle. No per-function catalog or source-text copy. Four
catalog tests passed before the operand conversion (session 48323,
target/shared-source-catalog-tests.log), including Arc pointer identity across handles
and module clones.

IncludeOnceMark/Guard now carry Immediate::Source(SourceId), not string data IDs.
Missing path lookup produces diagnostic-only UnresolvedSource, which validation
rejects before codegen; module validation also rejects out-of-catalog SourceIds.
The textual IR prints the ID/path catalog, never raw source bodies. Backend guards
and native web reset resolve the same SourceId/catalog lookup. Native storage names
encode exact canonical path bytes as hex, avoiding both lossy non-UTF8 collisions
and module-local ID name collisions across linked modules. The old FNV adapter
src/resolver/include_once.rs was removed; identity is not recovered from symbols.
Synthetic AST fixture catalogs were updated, and old no-catalog lowering APIs now
explicitly document rejection of resolver-produced inclusion markers without inputs.

Library/CLI/test target check passed (session 79761,
target/typed-source-inclusion-check.log). Nineteen targeted tests passed (session
96955, target/typed-source-inclusion-tests.log): immutable catalog sharing, source
lookup from real resolver output, typed guard lowering, unknown-ID and missing-path
rejection, non-UTF8 symbol separation, and native web reset emission on all five
targets. Assembly-comment and scoped whitespace checks passed. This is NOT shared
native/eval inclusion state or an executable Symfony acceptance result.

Release compiler rebuild launched with the existing low-memory opt-level-1,
256-codegen-unit/no-LTO profile and one Cargo job: session 73479,
target/typed-source-release-build.log. Poll that handle before any restart. Once
built, run native inclusion reducers before the four still-open native/eval tests.
Shared request registration, declaration activation, compiled source entries and
stock Symfony HTTP acceptance remain open; the larger statement smoke's Hash/Mixed
failure also remains unclassified.

Release compiler rebuild completed (session 73479, exit zero): 9m45s, binary 41 MiB.
Executed native include reducer under target/typed-native-includes: ordinary require,
include_once skip return, repeated ordinary require and canonical ./ alias. Compile
610.34 ms including runtime-cache miss; native stdout exactly matches PHP:
load|7:1|load|done. This exercises the new SourceId backend with a real binary.

Found a separate identity loss in Magician: eval_include_key converted canonical
PathBuf through to_string_lossy into String, and both context/global include sets
stored that lossy key. Added regression include_keys_preserve_distinct_non_utf8_paths:
RED with two distinct byte paths both rendered as /nonexistent-source/replacement.php
(actual replacement character in target/include-key-byte-red.log, session 97563).
Kept canonical PathBuf keys in include_exec, context storage and global storage;
path-taking context methods retain ergonomic string/path input without lossy storage.
GREEN (session 79199, target/include-key-byte-green.log), then 17 include-filter tests
passed (target/include-key-byte-replay.log). Non-test production check passed in
5.20s (session 9272, target/include-key-byte-production-check.log), important because
global include bookkeeping is cfg-disabled in unit tests. Release Magician rebuild
passed in 53.93s (session 27168, target/include-key-byte-release-build.log).

PHP oracle target/include-key-byte-probe.php creates two UTF-8-named symlinks pointing
at distinct non-UTF8-named sources. macOS rejected source-file creation with Illegal
byte sequence, so that host run is invalid inclusion evidence. Existing Linux image
elephc-test-linux-arm64 lacked php; tools-php supplied PHP 8.5.9 ZTS and produced AB
as expected (target/include-key-linux-php-version.log, target/include-key-linux-php.stdout).
Containers were ephemeral, network-disabled and limited to 128 MiB/one CPU. This is
Linux PHP-oracle evidence, not Linux Elephc executable coverage of the path-key fix.

Recompiled and executed all four native/eval reducers with the refreshed compiler
AND release bridge, sequentially (session 92223), under target/typed-cross-engine.
native-first and eval-first both exit zero but print load|load|done instead of
load|done. interface-first and skipped-interface both exit one with unsupported
InterfaceDecl. All four remain RED; typed identities did not unify registration
or activate declarations. Request-state authority still needs implementation.
Read-only lifecycle concern for that work: web reset currently calls the eval reset
callback, which clears include keys before releasing global function/autoload context
owners. Audit whether their cleanup can execute PHP before choosing the shared-state
reset point; no destructor-order reproducer or fix is claimed here.

Eval request inclusion ownership unified internally. New context/request_includes.rs
owns RequestIncludeState behind a shared Arc/Mutex. Generated contexts take the current
worker-request handle; no context-local set is retained, and the old global HashSet
was removed. Queries and registration always use the shared object. Unit contexts
default to isolated requests, with explicit shared handles in lifecycle tests.
Nineteen include-filter tests passed (session 64635,
target/shared-request-include-tests.log), including surviving-context reset and state
survival after one context drops without leaking into another request. Non-test
production check passed in 5.93s (session 93994,
target/shared-request-include-production-check.log). Native cells are not bound yet.
This latest request-owner change is not yet in the release Magician archive; that
archive currently contains the preceding PathBuf-key fix.

Compiler rebuild optimization implemented: src/main.rs used to redeclare the compiler
modules already built in src/lib.rs, compiling the core twice. Moved the exact dispatch
body to cli_entry.rs (only main-to-run rename), added its previously binary-only modules
to the library, and made main call elephc::run_cli. The existing dedicated 64 MiB compile
stack, native/monitor dispatch and INI diagnostics were preserved verbatim (checked
against the retained original body). CLI check passed in 23.85s (session 81514).

Same release build settings, one measured rebuild: 5m04s/304.59s real (session 63900,
target/thin-cli-release-build.log), versus preceding 9m45s core-duplicated rebuild:
about 48 percent less wall time. Not a statistically controlled benchmark, and not
the Symfony application's build time. Reported maximum resident set size was
3017490432 bytes; the separate 65340016-byte footprint line is not the rustc peak.
Compiler binary is 42 MiB. Before/after help/version stdout AND stderr compare equal.
The new CLI compiled the native inclusion reducer in 565.43 ms and its binary still
matches PHP exactly (target/thin-cli-native.stdout). Architecture regression passed
(session 18092, target/thin-cli-architecture-test.log), then 54 CLI-filter tests passed
(target/thin-cli-parser-tests.log). Pipeline/frontend/CLI unit tests now live in the
library target rather than duplicated binary-module test trees. Scoped diff checks pass.
Next: bind native inclusion cells to RequestIncludeState, with explicit lifetime/reset
rules; keep declaration activation and native compiled source-entry gates open.

Native inclusion cells connected to the shared request owner. RequestIncludeState
now accepts an unsafe, process-lifetime pair of native lookup/reset callbacks.
Lookup returns null or the actual aligned writable guard cell; mapping is immutable,
access is serialized by the PHP execution owner, and callbacks cannot execute PHP
or reenter. No cell addresses or shadow bits are cached per eval context. Known
native sources read/write their actual cell; unknown paths remain dynamic rows.
Installing a hook transfers only paths already opened dynamically, never merely
compiled/discovered sources. Same-owner registration is idempotent; incompatible
owner replacement fails explicitly. Multi-owner/library context behavior is not
claimed from these single-program probes.

Generated callbacks use a compact three-word table (raw path pointer, byte length,
cell pointer) and one lookup loop with memcmp, plus one reset loop. No per-file
unrolled dispatch body. Startup and eval-context entry install the hook, with fatal
status checked. Native-only programs do not link/register the bridge. Web reset now
enumerates emitted catalog cells, including those only reached dynamically, and
eval inclusion reset runs after global function/autoload context cleanup. Broader
PHP destructor/global-lifetime semantics remain outside this narrow ordering evidence.

Checks: request-owner native-cell tests passed (session 36870); production bridge
check passed (session 3292); compiler check passed (session 99810); three native
emission/reset tests passed (session 46990), covering all five targets and constant
lookup code size from one to 100 sources. Twenty-three include-filter bridge tests
passed (session 39370), including FFI invalid callback/ABI and conflicting-owner
checks. Logs: target/native-include-state-owner-tests.log,
native-include-hook-ffi-check.log, native-include-hook-codegen-check.log,
native-include-hook-emitter-tests.log, native-include-hook-bridge-tests.log.

Sequential release builds completed (session 83415): compiler 5m14s, bridge 55.40s.
Four reduced native/eval programs were rebuilt/run with these artifacts (session
48735). THREE now match PHP exactly: native-first, eval-first and interface-first
all exit zero with load|done. skipped-interface still exits one with unsupported
InterfaceDecl because discovery is still mistaken for active declaration metadata.
Evidence is under target/typed-cross-engine/*hook*; previous red logs preserved.
No new Symfony build/HTTP success is claimed, and compiled source-provider entries
are still absent (the new callback registers state access, not file execution).

Native callback ABI harness added: generated callbacks plus real C/memcmp, no PHP
runtime stubs. Host macOS ARM64 passed (session 73252); optional fixture export lives
under target/native-include-abi-fixtures via ELEPHC_INCLUDE_ABI_FIXTURE_DIR. Linux
ARM64 and x86-64 compiled/linked/executed that same harness in existing containers,
network disabled and capped to 128 MiB/one CPU, exit zero. First ARM64 attempt used a
no-exec tmpfs and was invalid execution evidence; the exec-enabled rerun passed.
Logs: target/native-include-callback-linux-arm64-exec.log and
target/native-include-callback-linux-x86_64.log (empty on success). Tests cover distinct
cells, exact byte/length lookup including non-UTF8, unknown paths and reset. This is
Linux callback ABI execution, not full PHP/Symfony execution there. The default
temporary-directory test also passed after ensuring cleanup only owns newly created
directories (session 91406, target/native-include-callback-temp-test.log).
Scoped whitespace and assembly-comment checks pass. Next: separate compiled symbol
metadata from active declaration bindings and implement real native file entries.

Declaration activation TDD expanded in tests/codegen/type_builtins/includes/
declaration_activation.rs (six permanent CLI-backed tests). Controlled probes under
target/declaration-activation establish these outputs in class/interface/trait/enum/
function/constant order:
- skipped inclusion: PHP 0:0:0:0:0:0, native 1:1:1:1:0:0;
- entered file, skipped declaration branch: PHP 0:0:0:0:0:0, native 0:0:0:0:1:1;
- file throws before declaration statements: PHP 1:1:1:0:1:0, native 1:1:1:1:0:0.
The last oracle confirms enum and global constant activation must not be lumped
with simple class/interface/trait and top-level function early binding.

Primary-source cross-check: php-src tag php-8.5.6, Zend/zend_compile.c, downloaded
to target/php-8.5.6-zend_compile.c (SHA256
a5ec78d970c9830ef470632aec6496a37bd6a625d4e053e284dcd26060614208).
zend_compile_class_decl around lines 9358-9454 distinguishes top-level early binding
and deferred declaration opcodes, excludes interfaces/traits dependencies from the
simple early-binding path, and adds enum interfaces before that decision. Function
declaration handling around 8414-8431 distinguishes nested declarations with an
execution opcode from top-level definitions. Do not infer all binding behavior from
these excerpts; dependency-sensitive binding and genuine duplicate timing need gates.
Reference: https://github.com/php/php-src/blob/php-8.5.6/Zend/zend_compile.c

Test-target checks passed (sessions 63848 and 54745). Formal six-test execution
(session 90873, target/declaration-activation-red-tests.log) reported one pass/five
failures, but only FOUR initial failures were semantic: dynamic-name interface
lookup hit a stale debug bridge and its automatic rebuild ran out of disk space.
Do not count that initial infrastructure failure as a semantic result. Cleaned ONLY
elephc-magician dev artifacts in this worktree (session 42478): 123 files/556.8 MiB;
release compiler/archive, source and logs preserved. Then ran the already-built
integration binary directly with ELEPHC_MAGICIAN_LIB_DIR pointing to target/release,
no Rust rebuild. Dynamic-name lookup now compiles/runs and fails 1 versus expected 0
(session 7133, target/declaration-activation-dynamic-name-red.log). Thus all five
semantic regressions are now independently evidenced; loaded-classlike control passes.
Free disk afterward about 705 MiB. This step adds failing gates, not an activation fix.

Read-only implementation implications: class-like literal queries currently constant-
fold from module metadata in member_queries.rs; bridge/dynamic queries reach native
metadata existence helpers too. Magician has_interface itself tests dynamic bindings,
not its declared-name metadata list. The resolver removes discoverable top-level
class-like declaration statements and replaces top-level included functions with
FunctionVariantMark at their textual position. Conditional function and define
availability also need examination. Preserve per-declaration sites/identity and
binding mode: source registration alone cannot represent declaration activation.

### 2026-09-09 — file-level function activation patch (validation pending)

- Revalidated branch reconcile/dirname-symfony; no cargo/rustc process at start,
  about 819 MiB free disk. No Symfony or vendor edits.
- strip_discoverable_declarations now collects marks generated from this file's
  direct function declarations and prepends them to its executable body, including
  across namespace blocks. It deliberately does not hoist preexisting child include
  marks or descend into conditional statements. Source include guards still enclose
  the activation prefix. This uses existing native function variant binding, not eval.
- Added three CLI regression tests with prefix test_file_function_early_binding:
  throw before declaration, cross-namespace visibility, and parent throw before child
  include. Existing PHP early.php oracle still prints 1:1:1:0:1:0.
- Focused diff hygiene passed. cargo check --lib --tests is tracked by session 92259
  and target/file-early-binding-check.log; executable tests are not yet rerun.
  Kage recall remains pending in tool cell 198. No full Symfony rebuild launched.
- Remaining activation failures and compiled source entry implementation remain open;
  this narrow patch is not proof of Symfony HTTP acceptance or full PHP parity.

Validation continuation: check session 92259 finished successfully in 1m53s with
unused-import warnings. Kage cell 198 terminated with a 300-second tool timeout;
packet text fallback found no matching function-activation packet. A combined PHP
oracle in target/file-early-binding-oracle/main.php prints 1:0 then 1; the previous
release compiler prints 0:0 then 0 (session 45627), establishing a native red baseline
for parent/child isolation and cross-namespace activation. Focused executable tests
are building sequentially in cargo session 60904, log target/file-early-binding-tests.log,
using the existing release Magician archive to avoid the stale debug bridge rebuild.

Focused executable validation finished: session 60904 exit 0, three new tests pass
in 7.27s after a 2m12s test build. Cargo still builds its Magician dev dependency;
the release override prevents an additional CLI-triggered bridge build, not Cargo's
normal dependency compilation. The pre-patch executable oracle above provides the
red baseline. Session 71060 now replays all 76 include-focused tests from the same
built test executable; log target/file-early-binding-includes-tests.log.

Next activation mechanism confirmed in source: src/ir_lower/stmt/mod.rs maps
FunctionDecl and all class-like declaration statements to lower_noop, whereas
FunctionVariantMark emits an actual active implementation pointer store. Conditional
declarations therefore need explicit binding semantics, not merely a reordered
metadata registry. No change to that path has been made yet.

Include-focused replay completed (session 71060 exit 101): 70 passed, 6 failed in
118.48s. All discovery, function-variant, basic include and path/error tests passed.
The six failures are the five already-reproduced activation cases plus the known
skipped-native-interface/eval InterfaceDecl failure. Combined early-binding output
is now 1:1:1:1:1:0 versus PHP 1:1:1:0:1:0: the function position is corrected,
leaving enum activation wrong. Native/eval once sharing remains 3/4 passing. No
additional failure appeared in this focused replay; no full-suite or full HTTP
claim. This turn's builds/tests are terminal, no owned process remains pending.

### 2026-09-09 — conditional declaration call gate

New permanent CLI regression test_conditional_function_activation_matches_callable_availability
checks existence before/after an entered argc-dependent branch and calls the function.
PHP oracle target/conditional-binding-oracle/main.php prints 0:1:42. Current debug
native compiler produces 1:1: followed by undefined-function Error (session 72676,
exit 255). Focused Cargo test session 39070 exits 101 on the compiled binary failure;
log target/conditional-binding-oracle/test.log. The harness reports empty stderr,
so retain the direct reducer output as the diagnostic evidence.

Important refinement: the native .s DOES contain _fn_conditionalBindingProbe with
a compiled body. This is not a missing-body compiler limitation. Declaration lowering
recurses through If in program/function_declarations.rs, but statement execution is
a no-op and a call classified late-bound-undefined becomes an unconditional Error
through expr/late_bound_call.rs, called by function_calls.rs. Next work must reconcile
checker call classification, runtime activation, and existence against one binding;
merely compiling nested bodies or replacing existence with true cannot fix this.
No runtime implementation change in this step; test and oracle only. Symfony HTTP
acceptance remains open, and no full application build was launched.

### 2026-09-09 — conditional function producer/consumer mismatch isolated

Read-only source confirmation: Checker::collect_function_decls in
src/types/checker/driver/functions.rs iterates only the top-level Program and
registers FunctionDecl/FunctionVariantGroup. In contrast, EIR
program/function_declarations.rs recursively lowers If and other statement bodies.
expr/function_calls.rs tests ctx.functions membership; a missing signature in a
frame without an eval barrier routes to lower_late_bound_undefined_call, which
emits an unconditional throw. is_late_bound_undefined_function is only a generic
eligibility predicate (nonempty, not a hidden extension), not a runtime lookup.

Implementation constraints for the next patch:
- Preserve a separate declaration identity and public binding; do not simply
  recurse checker collection and expose all conditional functions unconditionally.
- Reuse function variant binding for actual native entry pointers, with markers
  at executed declaration sites and shared lookup for exists/direct/indirect calls.
- Preserve distinct declarations within the SAME file: the current include variant
  registry keys only canonical path plus public name and cannot identify both sites.
- Do not introduce eval as a fallback for these already-compiled function bodies.
- Cover skipped/entered branches, call before declaration, same-name alternatives,
  repeated activation/redeclaration, namespaces, argument side effects and web reset.
This is an implementation map refinement under the locked inclusion specification,
not a claim that runtime binding is fixed. No production code changed in this step.

### 2026-09-09 — prerequisite: inactive native binding call semantics

Before reusing the variant dispatcher for conditional declarations, tested its
missing-binding behavior. PHP target/inactive-function-call-oracle/main.php prints
caught|done and never evaluates the print argument. Current native binary prints
argument-ran, then exits 1 with undefined-function fatal instead of entering catch
(session 16628). Permanent regression
test_inactive_compiled_function_call_throws_before_arguments fails as expected in
session 76241, target/inactive-function-call-oracle/test.log (4.94s execution).

Source mechanism: src/codegen/function_variants.rs emits the null-pointer check in
the public dispatcher, after caller argument evaluation, and directly writes/exits
instead of emitting PHP Error unwinding. emit_variant_mark also overwrites an active
slot without redeclaration checking. Therefore the old dispatcher cannot be reused
unchanged as the semantics of arbitrary conditional declarations. Native binding
resolution must happen in EIR before argument lowering, using the existing Error
throw machinery, while the dispatcher remains only the ABI tail-call mechanism.
LoweringContext currently has function signatures but no variant-group set; this
metadata must be propagated or represented explicitly without testing symbol-name
prefixes. No production workaround added. Tests and source evidence refine the
implementation dependency order; HTTP acceptance remains open.

### 2026-09-09 — native binding guard implementation in progress

Module now carries an Arc-shared set of runtime-bound public function names,
collected from FunctionVariantGroup declarations before body lowering. All eight
lower_body_into_function callers propagate it (closures inherit their parent).
Direct function lowering guards these bindings before argument adaptation/lowering
using function_exists and the normal EIR If/Throw Error path. No native assembly,
framework source or bridge fallback was added. This addresses the inactive included
function prerequisite; conditional declaration normalization is still pending.

cargo check --lib passed in 23.24s, session 48906, with four existing unused-import
warnings; focused diff hygiene passed. Executable regression is building in the
next Cargo session, log target/runtime-binding-guard-test.log. Runtime success is
not yet established; the previous test log remains the red baseline. Review still
needed for metadata collection through synthetic nodes, synthesized guard span
interactions, first-class/indirect call surfaces and all target behavior.

Validation: session 99227 exit 0; inactive direct function call regression now
passes in 3.19s after a 1m18s test build. The previous native red reducer evaluated
the argument then exited; this green test requires caught|done with no argument
side effect. Existing function-variant group replay session 52015 exit 0: all 21
tests pass in 10.05s, including loaded/unloaded namespaced calls and include order.
Added test_runtime_function_binding_guard_in_method_and_closure to exercise both
inactive and subsequently active calls from non-main scopes; focused build/run
is tracked in target/runtime-binding-guard-scopes.log. No Symfony acceptance claim.

Scope gate completed, session 17773 exit 0: method/closure test passes in 6.17s,
requiring method|closure|42:42. Independent PHP scopes.php oracle also confirms
catch-before-include and successful calls after include (its body prints body-ran).
All owned builds/tests terminal. Evidence is macOS host executable coverage only;
no new Linux/iOS execution has been performed for this guard. Missing conditional
function normalization, indirect-call lookup timing, redeclaration checks and full
HTTP acceptance remain open. The guard fixes direct-call activation timing for
existing native variant groups; it does not close the whole declaration campaign.

### 2026-09-09 — same-file conditional declaration identity gate

Added test_conditional_function_alternatives_keep_distinct_implementations with both
runtime branch selections (expected 0:left and 0:right). PHP alternatives.php oracle
prints 0:left. Current native compilation exits 1 in the assembler with duplicate
_fn_branchBindingProbe and duplicate epilogue labels (session 44937); full log
target/conditional-binding-oracle/alternatives-build.log. Focused regression run
tracked by session 89977 and alternatives-test.log.

Existing resolver FunctionVariantKey is canonical-file plus case-folded public name;
it cannot represent two declarations in one file. variant_local_name also hashes
file/public-name/include-exclusivity context, with no declaration-site identity.
Therefore conditional normalization must allocate declaration-specific identities,
not just reuse the include registry key or hoist both functions under their public
name. Name resolution must retain the public name for calls/recursion, reflection
and magic constants while bodies receive unique internal identities. This step adds
the failing identity gate; no production normalization implemented yet.

Formal test session 89977 completed exit 101 (2.58s execution), reproducing the
duplicate-symbol compilation failure. The first branch case fails before the loop
can exercise the second; do not count the second branch as executed native coverage.
Focused diff hygiene passed. No owned build remains active.

### 2026-09-09 — conditional alternatives with different signatures

Added test_conditional_function_alternatives_preserve_selected_signature: one branch
declares an int parameter, the other a string parameter plus an optional suffix.
PHP signatures.php oracle returns 42. Current native compile fails on duplicate
function and epilogue symbols; target/conditional-binding-oracle/signatures-build.log.
Focused formal test is tracked in signatures-test.log.

Source constraint: ensure_function_variant_group_signature compares all FunctionSig
values for equality and rejects differing signatures. Consequently merely hoisting
conditional declarations into the existing include variant groups would introduce
a compile-time rejection for valid PHP. The runtime binding must select both the
implementation and its argument contract, using the shared argument planner/ABI;
do not delete the signature check while retaining a representative static ABI.
This is an explicit regression gate for the pending implementation, not a fix.

### 2026-09-09 — clarify the actual interface worker failure

Recentered on the observed HTTP blocker rather than expanding conditional function
work further. execute_stmt DOES handle InterfaceDecl. execute_interface_decl_stmt
rejects a name already found in dynamic bindings or native existence callbacks;
without a specific note, statement failure is mislabeled unsupported InterfaceDecl.
PHP local duplicate-interface oracle reports Cannot redeclare interface CollisionProbe.

Added duplicate_interface_reports_redeclaration_not_unsupported_syntax. RED session
10955 exits 101 with the actual clause ': unsupported InterfaceDecl statement'.
Patched the existing duplicate-name rejection to record Cannot redeclare interface
plus the interface name before returning RuntimeFatal. No rejection is suppressed,
no stub/interface body bypass introduced, and no activation behavior changed.
Green rerun tracked by session 67333 and target/interface-redeclaration-diagnostic-green.log.
The native metadata-versus-active-binding defect still must be implemented; better
diagnostics alone do not close the HTTP gate. Release bridge has not been rebuilt.

Green session 67333 exit 0 (17.50s test build); targeted redeclaration diagnostic
passes. Existing interface-filtered Magician tests also complete exit 0, log
target/interface-redeclaration-neighbors.log. Focused diff hygiene passed.

### 2026-09-09 — interface activation must distinguish parent linking

Traced _interface_table: emitted as a 16-byte name/length table by const_registry.rs;
rt_interface_exists on both AArch64 and x86_64 maps any name match directly to true.
No declaration binding is consulted. Main consumers found by graph are the registry
emitter and these two lookup arms; literal probes bypass them via metadata folding.

PHP oracle target/interface-activation-oracle/main.php includes a file that throws
before a plain interface, an interface extending an already-active parent, and a
conditional interface. PHP returns 1:0:0; current native returns 1:1:0 (session 39998).
Thus even with a loaded parent, interface inheritance is linked at its declaration
site, unlike the plain top-level interface. A single source-included bit would
incorrectly activate the child. Added permanent
test_interface_early_binding_distinguishes_parent_linking; focused run session 78112,
target/interface-activation-oracle/test.log. Per-declaration early/link-time activation
is required by observed PHP semantics, not an optional refinement. No production
activation patch in this step.

### 2026-09-09 — runtime binding metadata wrapper coverage and independent review

Name resolver flattens NamespaceBlock but preserves Synthetic wrappers. The initial
runtime-bound-function scan only inspected Program's first level, unlike statement
lowering. Extracted runtime_bindings::collect_runtime_bound_functions under
src/ir_lower/program/, traversing Synthetic, NamespaceBlock and IncludeOnceGuard.
Names remain case-normalized and deduplicated; collection never activates a binding.
Added runtime_binding_collection_preserves_transparent_wrappers unit test, session
89082, target/runtime-binding-wrapper-test.log. Focused diff hygiene passes.

Started one read-only GPT Luna subagent binding_guard_review to audit the recent
guard/metadata propagation; no agent builds or edits authorized. Review specifically
covers argument timing, refs, synthesized source spans, methods/closures and metadata
coverage. No multi-model consensus or final code audit is claimed from this one
review. The overall native declaration activation/HTTP goal remains open.

Wrapper test session 89082 completed exit 0: 1 passed after 1m46s lib-test build.
Root reviewed clear_static_callable_locals called by lower_if: it clears callable,
reflection and fiber associations even though the new successful guard arm is empty.
A first-class abs callback created before a guarded call still works afterward:
PHP/native target/inactive-function-call-oracle/callable.php both print body-ran|42,
native session 52808 exit 0. This rules out the simple callback-breakage hypothesis,
not every association/performance case. Added permanent callback regression; focused
build session 36217, target/runtime-binding-existing-callback.log. Luna review remains
active and was given these observations; no independent verdict yet.

Callback regression session 36217 completed exit 0, 3.97s execution. Luna review
reported standalone first-class creation bypassing the direct-call guard and lacking
variant descriptor selection; association clearing looked like optimization loss,
not a proven semantic bug. Root rechecked the proposed case with argc-dependent
include (not constant false): PHP creation.php prints caught|done; current CLI rejects
it at checking with Undefined function for first-class callable: inactiveCallProbe
(creation-build.log). No null descriptor/runtime failure was reached, so the review's
suggested downstream outcome remains unproven. Missing first-class binding semantics
are confirmed at an EARLIER stage. Added
test_runtime_function_first_class_creation_tracks_activation (inactive creation caught,
then successful creation/invocation after include); formal test execution pending.
Next fix must include checker first-class resolution plus creation-time native binding
lookup/descriptor selection. No full consensus or HTTP success claimed.

### 2026-09-09 — first-class native variant binding patch

Checker first-class resolution now canonicalizes names and ensures known function
variant group signatures before lookup, just like direct call resolution. EIR
first-class function creation invokes the existing pre-argument binding guard.
Descriptor selection uses the group's public dispatcher rather than the representative
variant's entry. Refinement to Luna's initial report: callable_function_by_name DOES
fall back to a representative variant (context.rs:275); the defect is selecting that
representative implementation, not universally lacking descriptor metadata.

Added active-variant test choosing right.php (22) over left.php (11); retained inactive
then active creation gate. cargo check --lib passed in 36.17s (session 54014), focused
diff hygiene passed. Executable filter test_runtime_function_first_class_creation
running in target/first-class-binding-tests.log. Read-only Luna follow-up requested
for descriptor invocation/public entry consistency and binding lifetime. No runtime
pass or consensus claimed yet. Different-signature conditional functions remain open.

Luna follow-up found the descriptor fix internally consistent: public group spelling
is used for both entry_label and named invocation, representative signature is valid
under the existing equality restriction, and creation is guarded. No additional
concrete binding-lifetime defect identified in that read-only review. Root PHP oracle
target/first-class-binding-oracle/main.php returns 22. Runtime tests remain tracked
by Cargo session 61649; no final multi-model consensus or HTTP acceptance follows
from this focused review.

Runtime session 61649 completed exit 101: active variant selection PASSES; creation
before then after include still FAILS when running the binary (empty stderr from
harness). Test build 1m53s, two tests 4.58s. Thus static review was not sufficient
for runtime closure. Root is replaying creation.php through rebuilt CLI under
creation-after output dir to isolate exception/creation behavior. Assembly-comment
alignment check for descriptor_metadata.rs and focused diff hygiene both passed.

Direct replay session 43677 exits 255 with Uncaught Error: Call to undefined function
inactiveCallProbe() at creation.php:4, despite its surrounding catch(Error).
The guard now fires at creation, but exception handling is not preserved on this
surface. Investigate first-class creation effects/try-handler retention (do not
assume the descriptor is the remaining problem). The regular direct-call guard
catch test previously passed. No root build/test currently pending.

### 2026-09-09 — first-class creation exception effects

Root found the missing handler mechanism: exception_flow::expr_throws grouped
FirstClassCallable with Closure as throwing nothing. Consequently AST catch pruning
removed the Error handler before EIR inserted the creation-time binding guard.
callable_target_effect separately classified function/static-method creation as PURE,
allowing unused creation to be erased entirely.

Patched creation summaries: function targets may throw Error; method targets retain
unknown Throwable possibility (receiver evaluation/autoload). Coarse creation effects
now include may_throw, preserve receiver effects, and conservatively model static
autoload side effects/global writes. Invocation-body exceptions are not mistaken for
creation exceptions. Focused diff hygiene passed. Existing two native first-class
binding regressions running in session 69377, target/first-class-binding-catch-tests.log.
No executable pass yet; this fixes optimizer modeling, not the broader activation
registry or full HTTP goal.

Session 69377 completed exit 0: both first-class native variant tests PASS in 4.94s
after 1m58s test build. The previously uncaught creation-time Error is now caught,
and subsequent creation/invocation and active variant selection succeed. Replaying
the focused first_class test family sequentially from the already-built executable,
log target/first-class-binding-neighbors.log. This broader focused replay is still
pending; no full-suite, cross-target or HTTP claim.

Added test_runtime_function_first_class_unused_creation_keeps_error to ensure the
coarse effect fix preserves discarded creation; not part of the already-running
binary and not executed yet. Neighbor replay session 69532 triggered one normal
Magician dev archive auto-build (12.77s) despite the CLI archive override, through
an in-process test path. That build completed; disk about 1.5 GiB and only the test
runner remained active at check. No cleanup or parallel build started.

Continuation: session 69532 is confirmed still live (not a stale log). Neighbor
replay advances through mixed native/eval first-class callable cases; no failure
line observed at this checkpoint. Free disk about 1.4 GiB. Do not restart the run
or start another Cargo build until it completes. The unused-creation test remains
unbuilt and must be run afterward. This turn is a verified wait, not a new fix.

### 2026-09-09 — first-class regression family outcome and baseline comparison

Neighbor session 69532 completed exit 101: 108 passed, 2 failed, 110 total in 373.69s
(the earlier list count 111 also matched another name; use actual run count).
Failures are builtin sort callable with Mixed receiver and associative spread callable
argument expecting Heap(Array) but receiving Heap(Hash). New unused-creation test
session 45368 PASSES in 3.32s.

Luna read-only triage suggested representation/signature defects, not exception
effects; root verified rather than relying on that inference. Preserved release CLI
target/release/elephc (Sep 9 15:21:37, before current changes) compiles the exact two
PHP reducers under target/first-class-neighbor-oracles/ and reproduces BOTH errors:
sort-baseline.log has unsupported sort for PHP type Mixed at runtime_call line 4;
spread-baseline.log has the same InstId21/ValueId9 Heap(Array)/Heap(Hash mismatch.
PHP oracles print 123 and 60 respectively. This is old CLI versus current focused
test-harness evidence on identical PHP inputs, not a complete historical suite run.
Both failures demonstrably predate the current first-class binding/effect fixes.
No new failure observed in the 110-test replay; other pending campaign defects and
all-target/HTTP acceptance remain open. All owned test/build sessions now terminal.
