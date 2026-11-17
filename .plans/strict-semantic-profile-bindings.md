# Strict local binding implementation history

This record preserves steps116-118 candidate investigations and verification. The
current approved contract and authoritative checkpoint remain in
[strict-semantic-profiles.md](strict-semantic-profiles.md). Candidate failures
below are chronological and are superseded by the verified118 checkpoint.

### Step116 candidate: stable numeric local stores

Numeric binding contracts are tracked separately from narrowed read views and
physical storage in src/types/checker/numeric_local_contracts.rs. Per-body save/
restore and binding kills own the map. The existing local store contract now
retains its owning StrictFeature, and shared neutral/compiler membership planning
supports Locals independently from Coercions. EIR reuses rooted membership checks
before local assignments and updates. AST effects and exact TypeError summaries
preserve catches for these added write boundaries.

Job3ecf7f9f verifies the compiler build, neutral independence, all-five-target
emission, AST catch retention, existing typed store guards/native ownership, CLI
source diagnostics, native dynamic/compound/prefix/postfix writes, explicit
numeric unions/mixed declarations, scope isolation and unset/rebind. Both IR
modes and heap accounting are covered. The candidate does not complete runtime-
inferred dynamic binding contracts, alias propagation or native/eval scope parity;
stronger Locals remains active work, as do the six remaining guarantees.


Initial job3ecf7f9f passed seven exact gates, including native numeric guards,
independence, all-five-target emission, AST catches and declared-store regression
coverage. The eighth gate exposed a fresh closure-local name collision: the
closure checker clones the enclosing environment, which is not evidence that a
same-named slot is bound in the closure. Contract selection now uses actual
binding/declaration/capture/parameter/global/static evidence before reading that
inherited entry. The public LocalStoreContract type removes the new visibility
warning, and prefix integer result typing reflects the successful guarded store.
Job00d2e431 repeats all eight gates plus the new strict-locals example. Inputs are
frozen while it waits/runs.


### Stronger Locals follow-up boundaries

Keep value membership and physical representation separate. Numeric store plans
now name their owning feature; extend this shared mechanism rather than enabling
Coercions or Overflow. Preserve the type origin of dynamically typed first
stores: explicitly mixed/union producers and declarations retain their advertised
openness, while accidental storage widening must not create such an exemption.
Do not infer semantic openness or a new runtime tag contract solely from boxed
Mixed storage. Array container kind belongs to Locals; key/element constraints
belong to ArrayTypes/ArrayKeys.

Reference rebinding is a binding transition, not an ordinary write. Before
committing numeric contract tracking, check that `=&` does not retain the target
name's previous numeric contract. A replacement binding must derive its contract
from the new source, while an alias to the displaced binding keeps that binding's
own contract. Cover source and destination writes, self-alias, unset/detach,
parameter/capture return escape, globals and foreach with real native controls.
Do not relax existing physical reference-layout or ownership eligibility gates.

Native eval snapshot inventory currently enumerates final function slots. A
future zero Object slot is recognizably absent (verified115); scalar zero alone
cannot distinguish an absent binding from a real integer zero. Stronger native/
eval binding publication must therefore consume source-point binding evidence,
not merely extend the future-slot type switch. Inspect initialized-slot and
binding metadata in EIR lowering and preserve it through passes. Reuse existing
scope local_type_contracts/generic_local_contracts and persistent reference
contracts in Magician. Candidate validation must precede mutation, and a failed
write must consume only owners it actually owns.


Job00d2e431 passed the warning-free compiler build and all nine exact gates in
3m11s. This proves fresh closure scope isolation, prefix-result typing, explicit
numeric unions/mixed, ordinary unset/rebind, native numeric guards with clean
heaps in both modes, five-target emission, AST catches, existing typed-store
behavior and the example. Probe jobac316a73 then compared PHP, permissive native
and Locals native: a target rebound with `=&` incorrectly retained its former Int
contract, and a nonexecuted coalesce-default assignment incorrectly seeded a
contract for a later fresh Float binding. PHP/permissive print2.5 for both; the
Locals candidate rejected them. Self-alias correctly retains the Int contract.

The next candidate clears only a replaced reference target's inferred numeric
contract (preserving self-alias), and saves/restores numeric contracts during
with_preserved_binding_flow diagnostic-only walks. The exact rebind/coalesce
source is now tests/fixtures/strict/numeric_local_rebinds.php and is included in
the native and all-five-target regression gates. Final job85e61f31 runs a fresh
build, ten exact gates and the builtin EIR architecture audit. These latest two
repairs are not yet verified or committed. Inputs are frozen while it waits/runs.


Job85e61f31 passed the build and neutral gate, then the expanded all-target
fixture still rejected the nonexecuted coalesce-default binding. Restoring the
new map in with_preserved_binding_flow was necessary but not sufficient: the
effects walk treated a proven bound non-null parameter's default as a potentially
executing conditional arm. It now checks an intrinsically non-null left value,
or a plain variable with the existing completed-store non-null proof, using a
diagnostic-only default walk. Property and offset probes are deliberately not
classified from ordinary completed-read proofs, because missing/uninitialized
probes must still execute their default. Existing coalesce result typing remains
unchanged.

Candidate acceptance job5705df4f repeats the build and twelve exact gates plus
the architecture audit. The permanent fixture now covers a parameter, literal,
and declared non-null return whose defaults do not execute. Source controls
require real null, missing-array and uninitialized-property defaults to retain
their binding effects; two existing coalesce/native regressions check ordinary
behavior. This latest candidate is unverified and uncommitted. The verification
directory remains target/strict-numeric-locals-pzgxjotu.


## Verified116 acceptance

Job5705df4f passed a build without new warnings, all twelve exact gates and the
builtin EIR architecture audit in 2m52s. Native checks covered both IR optimizer
modes with clean heap accounting, independent PHP arithmetic/coercion/cast/
comparison controls, exact large integers, explicit mixed/numeric unions,
reference rebinding, self-alias, unset, closure scope isolation and nonexecuted
coalesce defaults. Existing coalesce mutation/missing-string-array regressions
also passed. The expanded EIR/library emitter gate covers all five targets.
The owned verification directory is removed after evidence capture. Stronger
Locals and six other strict families remain active work; no full local suite,
push or PR has been requested or performed.


## Step117 investigation history

### Step117 candidate: nullable numeric bindings and shared updates

Baseline882e459f on signed116 compared PHP, default native and Locals native in
both IR modes with heap debugging. Direct aliases already reject a Mixed float
under Locals and retain integer1; direct global writes do too. A fresh integer
closure local shadowing an outer float is a passing control. Untyped reference
parameters still replace the caller's inferred Int with float (knowledgea7dce035),
and opaque eval still does the same (knowledgea6343333), all with clean heaps.
The default direct-alias control narrows2.5 to integer2 through the inherited
scalar-reference backend coercion; preserve existing default behavior while adding
strict checks. These two missing contract transports remain completion work.

The first nullable-reference fixture was refused in both profiles by the existing
concrete-versus-nullable reference storage guard. Supported baselinesf49cbc10 use
actual nullable parameter storage instead, and also test nullable by-value writes/
updates and a compound assignment followed by a typed return. They run the exact
prebuilt116 compiler before any new compiler build.

The117 candidate admits null as an explicit alternative of an otherwise numeric
binding. Successful guards project concrete source alternatives so an accepted
integer update remains an integer read view, without removing null from the
binding's contract. For a guarded Locals update, raw nullable reference storage
receives the accepted result after ordinary PHP promotion and a TypeError guard,
rather than taking the legacy narrowed-sink fatal path. Existing reference-layout
eligibility is unchanged. Numeric overflow remains independently ArithmeticError
when Overflow is enabled. The example now uses an optional quantity parameter and
shows a legal null replacement.

Queued job2916a299 follows baselinef49cbc10 and builds the candidate, then runs
nine exact gates: nullable and ordinary five-target emitters, nullable native
values/references with both IR modes and Overflow settings, source kinds/null,
ordinary numeric/native/union/rebind controls, example, and an existing nullable
reference regression. The architecture audit follows. Candidate production files
are frozen for this Rust verification. No117 commit exists yet. The possible
compound-return read-view case is being baselined before further changes.
Owned temporary artifacts are target/strict-local-references-xu51mw4b and must
be removed after evidence capture. The complete task remains active.


### Verified117 acceptance

Baselinef49cbc10 confirmed that signed116 allowed nullable value writes/updates
to float and hit the legacy fatal narrowed sink for supported nullable reference
updates. Candidate diagnosisd356faea proved correct TypeError versus
ArithmeticError behavior with clean heaps. The first matrix assertion was too
strict when Overflow had already proved an integer result; that redundant guard
can be absent. Diagnosisf6523b3c found a compound-return read-view regression:
acceptance kept int|null after an operation that cannot produce null. The shared
completed-store proof now preserves its non-null view after guarding.

Job8d7c8fcc passed a build without new warnings, ten exact gates and architecture
audit in2m39s. Both IR modes, both Overflow settings, MIN/MAX, ordinary/reference
updates, repeated prefix/postfix results, null/large-int writes, typed compound
returns, examples and existing nullable reference/coalesce controls pass with
clean heaps. Both five-target gates pass. Untyped reference parameter transport
and opaque eval contract transport remain unfinished, as recorded in facts
a7dce035 and a6343333.


## Verified118: native caller reference transport

The original native_untyped_local_contract.php baseline is now registered and
passes with TypeError before mutation under Locals, preserving integer1, while
the default still stores double2.5. Exact integer9007199254740993 survives both
paths. Job8e574690 first passed the new five-target validator/emitter, native
regression, existing property metadata, protected C-ABI checks and architecture
audit in4m54s. Jobs18ed5c3f and4b25fd6f verified parameter, nullable, explicit
numeric union/mixed, unset, normal/exception cleanup, closure-parameter and
capture controls. A signature-unknown descriptor still bypassed the contract;
its checker alias recorder now receives the source-point environment and records
the same numeric contract used by resolved calls.

Job7f65ab1a stopped on an overly optimistic four-contract assertion: only three
were generated. Probe24539791 proved a real missing already-boxed numeric union
contract: a string replacement was accepted. The checker now records numeric
reference membership before deciding whether physical storage needs widening.
The permanent lifetime fixture rejects that string, preserves double2.5, permits
explicit mixed writes and covers runtime-selected descriptors.

Final jobbd8c3124 passed a build without new warnings, eight exact gates and
the architecture audit in2m46s. Both IR modes have clean heaps. All five targets
validate/emit the exact local callback ABI and retain existing physical property
metadata. The extended example and prior ordinary/nullable numeric tests pass.
The task-owned target/strict-reference-contract-bghxkxqg directory is removed
after evidence capture. Native/eval scalar contract transport and broader
Locals remain active, alongside the other six unintegrated families.
