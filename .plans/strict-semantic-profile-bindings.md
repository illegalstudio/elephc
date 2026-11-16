# Strict local binding implementation history

This record preserves step116 candidate investigations and verification. The
current approved contract and authoritative checkpoint remain in
[strict-semantic-profiles.md](strict-semantic-profiles.md). Candidate failures
below are chronological and are superseded by the verified116 checkpoint.

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
