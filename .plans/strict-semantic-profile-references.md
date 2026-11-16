# Strict semantic profile reference and cast admission history (110-111)

This archive preserves the chronological evidence for the completed Coercions and
Casts admission work. Earlier pending snapshots and failed probes are historical;
the final verified sections record their resolutions. The active contract and
remaining work stay in [the implementation plan](strict-semantic-profiles.md).

Coercions is admitted in signed commit ba2bc9f3129563987301ab391d3578e9a8b314a6.
Casts is admitted in signed commit4727b75aced908c09988daed550f7d5e602c4b17.
Property/reference source snapshots, independent original-value candidates,
Reflection binding identity, source retirement, eval capture/unset and native
context lifetime controls are completed by those verified steps.

### Native Coercions contract admission (110, verified)

109 is signed as3bc3f47cf4 and Strings is publicly admitted. The two preserved
eval-owned property native-reference fixtures are now registered. They require
exact Float contracts to reject Int writes at the native assignment before later
callee effects, plus native catches, nested eval, source intersections, source
removal, static properties and orphan lifetime controls. Coercions admission is
provisional only in this uncommitted audit snapshot. The real Magician archive
will be rebuilt before the two exact native tests; no execution result exists
yet. Remaining source/prelude/callable/host coverage must be checked before any
Coercions admission commit.


Native110 job116fb542 exited0 after2m52s. Assignment-point63.71s and reference
lifecycle61.03s both pass with both optimizers and clean heaps: native writes
reject before following callee effects, native catches/nested eval preserve the
original value, and removing source declarations restores permitted kinds.
Shared native parameter/return/builtin PHP fixtures pass public --check and are
now reused by staged tests plus newly registered public CLI execution checks.
A REPL exact-binding recovery case is registered. These public execution/source
and REPL tests remain unverified; Coercions admission is still provisional.


Public110 job6e2399ae exited0 after1m52s. Native shared parameter/return/builtin
CLI matrix6.40s, source evidence/explicit conversion0.59s, REPL exact binding and
recovery48.69s and compiler build39.44s pass. Before final admission, added direct
AOT Reflection controls and one real opaque host matrix spanning eval/native
parameters, returns, methods, descriptors, builtin inputs and Reflection. A
public library all5-target profile test and the extended existing strict example
complete the admission checks. These final additions are not yet executed.


4e82b956 passed all3 selected five-target compiler tests, then failed the new
Reflection fixture. Exact retry9c5a39af proves this is permissive no-IR-opt too:
expected stdout matches, but inline reflectors leak96 blocks8672bytes. Known
invoke fast paths bypass receiver retirement after lower_member_receiver already
constructed it. Both fast paths now root the original receiver and retire it
afterward through a shared helper that detaches the receiver record, pins the
completed result and clears/releases the receiver slot in LIFO order. The native
control adds an owned string result. Added CLI assembly-only checks on all5
targets, reusing a compile-only fixture helper. A separate typed-string missing
value Coercions probe is registered and unexecuted; no logical-kind claim yet.
Opaque hosts and the example were not reached by4e82b956. No110 commit/admission.


Nativec76ea9eb passed Reflection receiver/result owners13.59s (all4 profile/IR
controls), all5-target CLI Reflection emission12.38s, opaque native/eval host
contracts63.72s (both IR modes) and the combined-profile example2.39s. The new
missing-string probe then reproduced three exact-binding bypasses: argument,
return and builtin accepted logical null behind Str storage, yielding
V|NULL|0|NV:0:1 instead of T|T|T|NV:0:1, clean heap. Compiler conversion planning
now keeps runtime kind validation for accepted Str storage when the declaration
excludes null. Nullable declarations and explicit conversions retain their
contracts. The public all5-target compiler fixture now checks that predicate too.
These latest changes await focused verification; no110 admission commit yet.


Final110 verification90c1f7b5 exited0 after3m00s. Public library guards pass
all5 targets, including logical string-kind checks; the missing-string regression
passes all4 AST/EIR modes in1.56s with correct nullable/cast controls and clean
heaps. Shared public native contracts6.25s, Reflection profile/optimizer controls
14.58s and the extended combined-profile example2.03s pass, followed by compiler
build59.95s. Earlier verified opaque hosts and five-target Reflection emission
remain green; the last change is the shared concrete-string declaration guard.
Source, target, owner, REPL, reference, Reflection and explicit-conversion
evidence now supports public Coercions independently. No other guarantee was
enabled. No stored EvalIR shape or profile bit numbering changed. Known
Reflection receivers now retire on normal and exceptional paths through the
shared result-preserving owner helper. CLI/checker docs describe the guarantee.

The detailed verified notes through109 moved verbatim into the linked admission
history, keeping all approved requirements, active audits and the full ledger in
this plan while restoring room under Code Journal's100k limit. Next111 closes
the remaining Casts Reflection identity/temporary and opaque-host audits, then
stronger Locals and the remaining approved guarantees. The full user task stays
active; the umbrella remains gated until every guarantee is implemented.


### Casts Reflection binding identity audit (111, pending)

110 is signed asba2bc9f312. New PHP reference fixture distinguishes four cases:
an existing property reference versus one introduced by the temporary Stringable
destructor, each with/without a destructor throw. PHP always converts before the
destructor; without an original reference it publishes into the property and
breaks the newly introduced alias (old:second:new), whereas an original reference
receives the converted value even after property rebinding (new:second:second).
Successful conversion publishes despite a retirement ValueError. Both native
lowering and actual opaque Reflection over native declarations are prepared for
comparison. Casts admission is provisional in this uncommitted experiment only;
no native execution result exists yet. Remaining source/opaque/alias/host Casts
checks remain required before admission.


111 initial8d8f6923 stopped before execution: the AOT parser rejects reverse
static-property reference assignment (Unexpected token Ampersand). The fixture
now creates pre-existing aliases with supported $first =& Class::$text and uses
runtime-selected eval inside each native destructor for reverse rebinding. PHP
confirms identical four-case stdout. Both outer native and opaque Reflection
paths remain unverified, and no binding-identity bug has been claimed yet.


111 retry1a222096 stopped during lowering, before native execution: adding opaque
rebind inside the native destructor requires the staged helper to inject the
real eval/object-cast preludes. The direct case now also uses the eval-capable
fixture helper. This is harness setup, not a confirmed publication failure.

111 real baseline1fc635b0 selected exactly one regression and ran with a clean
heap. Native Reflection setter results differed from PHP only after rebinding
a pre-existing property reference: the old alias and property both became new,
where PHP leaves the property bound to second. Confirmed native eval reference
binding uses static_property_set plus an interpreter-only alias overlay. Work
in progress adds scoped physical native rebinding with a rooted staging cell,
shared declaration validation, and protected cleanup. This result does not yet
admit Casts.

111 current checks: the owning-alias EIR assertion passes for both Casts and
Coercions on all five targets; Clang assembles the new physical rebind, sharing,
and captured-destination helpers on all five targets. The existing protected
commit publication-order test passes. Native alias detachment/write-through
passes for Casts and Coercions with both IR modes, four clean heaps (135.79s).
The full Reflection identity/retirement regression is still running. New
sharing helpers distinguish native cell owners from direct canonical wrapper
owners and exclude validation/collector leases.

111 native Reflection cases now pass with both IR modes. The opaque outer
Reflection baseline remains red with clean heap778/778: its temporary argument
is retired after publication (SDnew) instead of before (SDold). Work in progress
shares exact evaluation-owner provenance through named/positional binding and
adds a consuming native class-setter entry using the same static-call publisher.
No remaining feature has been admitted by this pending work.

111 retry c631b74f stopped at compilation before tests: one native descriptor
CallArgumentModes initializer needed the new optional retirement field. Fixed
via the existing Default initializer. The current872db101 prebuild succeeds;
Clang accepts both capability paths and the consuming class-store ABI on all
five targets. Its opaque Reflection case now also uses reordered named
arguments. No full suite was run.

Future112 public Casts source/eval/callable/library checks are prepared in the
currently unregistered tests/codegen/strict_casts_public.rs and shared scalar
fixture tests/fixtures/strict/cast_values.php. They remain unrun. Current source
inputs are frozen while872db101 executes. Both native static promotion and
Reflection source-owner metadata are gated by the shared neutral profile
predicate, preserving ordinary source-argument ownership outside checked
property-reference profiles.

111 verified native reference/Reflection checkpoint872db101 exited0 after4m49s.
The real native/opaque Reflection fixture now matches all four PHP identity and
throwing-destructor cases with both IR modes, four clean heaps (142.79s). Opaque
invocation also verifies reordered named argument ownership. All-five-target
Clang assembly includes both static binding and consuming class-setter entries.
Public scalar, source-diagnostic, callable/eval, library, example and REPL tests
are now registered for the final Casts admission pass. No commit yet.

Public db4bad2c currently passes source evidence/independence, native+opaque
scalar domains (63.04s), native/eval callable hosts (60.26s), all-five-target
library emission (0.76s), and example (0.42s). REPL, selector control and final
build are still running. Casts admission additionally retains the explicit
historical alias audit: tests/codegen/strict_cast_argument_aliases.rs is prepared
but not registered while sources are frozen. PHP oracle verifies current-cell
mutation versus unset/rebind selection for native and eval callees, using
explicit mixed eval-owned properties and statics to isolate identity from
implicit conversions. Expected: Ninteger:7|integer:7;Ninteger:42|string:7.9;
Einteger:42|string:7.9;Ninteger:9|integer:9;. No compiler result exists yet.

Public db4bad2c also passes the real cached REPL (47.26s), then stops at
protected static selector generation: the new binding label mode needed the
getter ABI's scope-offset mapping. Added binding to both target mappings. This
was a compiler unreachable guard for non-public slots, not a numeric cast
failure. Pending-profile regression tests now select still-staged Comparisons
instead of candidate Casts. Job1ae14843 queues those narrow controls, the newly
registered argument-alias audit, and the final compiler build.

### Remaining Casts alias and source-coercion evidence (111, still pending)

Jobd65948d3 confirms the mixed alias failures again with a clean207/207 heap,
and typed float controls also lose their declaration: actual
Ninteger:1|integer:1;Einteger:1|integer:1; versus PHP
Ndouble:1|double:1;Edouble:1|double:1;. Current eval property scalar validation
is Coercions-only. Extending stable captures alone would not preserve the
weak PHP declaration rules needed by independent Casts. Native and eval
property sources must inspect the same original RHS before publication; do not
sequentially pre-coerce eval sources before native union validation.

The proposed-static-binding probe separately returns Tobject:X:old before any
Stringable hook (clean48/48); PHP returns Sstring:new:new even though __toString
replaces its unconstrained source with an array. The initial failure is the
weak native guard's static-only Stringable class list. Pendingf740df6b adds
ObjectPlan.eval_stringable, uses the existing live Stringable relation query,
and shares relation-ABI materialization with ordinary object_is_a lowering.
The established string conversion ladder already has an exception-safe erased
object fallback. A new EIR test covers all five targets with/without eval.
No native result for this guard fix yet. It is NOT yet established that the
temporary native source registration rejects reentrant mutation too early.

The re-entry probe f740df6b confirmed that issue (STobject:X:old versus PHP
Sstring:new:new, clean59/59). Implemented validation of a proposed
NativeReferenceTypeSource passed by value snapshot without installing it in the
live weak registry. Add a versioned validation ABI while preserving v1. The
coordinator must snapshot/authenticate existing native sources plus the proposed
source before ANY PHP callback. Proposed sources use a reserved token distinct
from registered generations. For Casts-only eval declarations, an optional
protected external-candidate callback can snapshot all live eval property
source declarations and coerce each from the same original RHS; existing
Coercions stays exact. Compare the external candidate with independently
coerced native/proposed candidates before commit. Weak eval normalization must
not feed into native union selection, and prospective constraints must not
become live during __toString. Version two preserves the existing version-one
entry and operation layout, adds an optional external source provider, and
authenticates copied proposed sources without registering them. Jobfd456b9d
passed all four Rust coordinator controls, all-five-target assembly, the real
proposed-source re-entry regression, both Reflection identity/retirement IR
modes (164.33s), and the previous exact eval/native assignment-point regression
(72.03s). The complete job exited0 after8m44s. No commit or Casts admission yet.

Job203e6ef8 now queues stable Casts source/property argument captures and a
shared eval-source candidate planner used by fake RuntimeValueOps and the
version-two native external provider, without double adaptation. New neutral
weak_scalar_binding plans implicit scalar preference and range rules separately
from checked explicit numeric casts. Immutable source decisions precede PHP,
and each candidate uses the same original RHS. Physical eval sources remain
distinct rather than deduplicated by declaration. Declared mixed contributes
an identity constraint, unlike an untyped property (reconfirmed with PHP and
existing CJ fact2f43cec8). Native/eval cross-source tests cover both mixed and
numeric-union conflicts plus removal of a source. This implementation remains
unverified until the queued job completes.
Preserve implicit PHP scalar preference/range rules separately from explicit
checked-cast kernels, and keep every owner/Throwable boundary protected.

Job203e6ef8 stopped at compilation after16s: the new candidate finisher passed
Result<Option<RuntimeCellHandle>> to the existing owner cleanup helper, which
accepts Result<RuntimeCellHandle>. Corrected the owned result before mapping to
Some and removed redundant exact validation after the shared planner. No runtime
test ran in that job. Casts with eval_bridge now explicitly selects native
reference validation even when there are no statically declared user properties.

Joba00480ba queues the corrected build, four neutral scalar planner tests, weak
and existing exact fake-reference controls, all-five-target weak-provider and
assembler checks, native argument-cell selection, independent native/eval mixed
and union constraints, proposed source reentry, the previous exact native/eval
assignment control, and a final compiler build. It also adds the PHP-verified
weak_property_reference_sources.php fixture: distinct instances of one property
each invoke Stringable conversion even after the first source retires, and a
throwing E_DEPRECATED precision handler escapes as TypeError with the original
RuntimeException as previous while storage remains unchanged. Oracle output is
SS:new:new;PTypeError:RuntimeException:1:1 (CJb882f71e and458a9819). The bounded
previous-exception builder roots constructor operands and retires every owner.
These new runtime paths are implemented but not yet verified. No111 commit yet.

Joba00480ba passed build, all four neutral weak-scalar controls, weak and exact
fake reference controls, the five-target external-provider policy test, and
Clang assembly for both Coercions and Casts. The real argument fixture then
confirmed current-cell mutation and float constraints fixed, but unset/recreate
still reused the old captured cell: Ninteger:7 and Einteger:7 versus detached
integer42 with replacement string7.9. Heap250/250 clean; job exited1 after5m05s.
CJ9110a87e records the concrete cause: the eval unset fallback wrote null
through the existing backing hash reference after clearing only alias metadata.

Added an eval-owned backing-hash removal fallback to the existing protected
typed-unset boundary. It reuses the property hash-slot classifier, installs the
COW table before hash_unset, and performs no table writes after value retirement.
Fixed native slots retain their existing dispatch; ordinary native objects do
not acquire eval backing-storage permission. New source fixture controls cover
two distinct same-declaration sources, retirement during Stringable conversion,
precision-handler exception chaining, clone/COW aliases and destructor replacement.
The full PHP oracle output is now
SS:new:new;PTypeError:RuntimeException:1:1|42:42:7.9D:replaced.

Added a bounded execution-context lease for the external provider. Active native
callers borrow their existing context; a foreign/fallback heap context retains
one ABI owner through PHP callbacks. The current candidate remains rooted through
context cleanup, and any conversion/teardown Throwable is exported after releasing
the context without dereferencing it again. Pure Stringable probing and execution
recover an escaped object's owning context. Three ABI lifetime tests cover final
host release, an existing native activation and nested outer teardown. These are
implementation safeguards, not a claimed reproduced UAF. New job0dba0d5f/session92441
queues these tests, all-five-target unset emission/assembly, prior candidate
controls, all native alias/callback fixtures and final compiler build. No new
runtime result for these additions yet; no111 commit/admission.

Job0dba0d5f passed build and two lease controls, then stopped at an invalid test
setup: begin_teardown correctly refuses a context that still has ABI owners.
Corrected the fixture to release its host owner before entering outer teardown,
then acquire the nested activation. Job9292c9c6 passed all three lifetime tests,
all scalar and fake exact/weak controls, both five-target unset tests and both
coordinator policy/assembler controls. Its native alias fixture then failed after
the first case with heap memory exhausted (3m11s, exit1). The new unset fallback
passed the hash only in rax, but the x86_64 COW helper consumes rdi and returns
rax. Added explicit ABI argument materialization and a five-target structural
check for the x86 input register (CJ948b621d). The separate inspected older direct
dynamic-property unset path is tracked as task1fffe7bc for reproduction; it is
not claimed fixed here. Current joba34a3a4e/session72535 queues the corrected
unset execution and all remaining reference/callback controls plus compiler build.
No new runtime result for that correction yet; no111 commit/admission.

Joba34a3a4e passed the real argument selection fixture (82.16s, both IR modes,
clean heaps) and independent native/eval mixed and union constraints (68.42s).
The callback fixture then stopped at eval parsing: it used reverse property-to-
variable reference syntax that the existing eval parser does not support. Changed
those two setup bindings to the already supported property =& variable direction,
with the same PHP oracle output, and added a fast parse assertion to the existing
fake candidate control. No parser language extension was added.

The builtin architecture audit passed:828 registry AOT builtins,484 compiler
routes, zero structural errors and no backend-dependent homes. I omitted
ELEPHC_DOCS_PREBUILT=1 and the audit implicitly ran cargo outside the queue;
this was an execution-protocol mistake, not a source change. It finished before
the attempted stop. Job5bb61656 then explicitly rebuilt the default Magician
archive through the queue and passed callback/retirement/COW/destructor/previous
exception controls (62.72s), proposed-source reentry (63.33s), exact native/eval
assignment points (71.53s), and compiler build (2m04s). Whole job exit0,5m54s.

Final reference-boundary review restores the direct interpreter's current-context
visibility now that publication delegates to the native coordinator. Checked
RuntimeValueOps reference replacement installs a context-only native activation;
the registry includes its current borrowed context even during outer teardown.
Inactive tearing-down contexts remain excluded. A unit control covers this
borrowed-stack case. This is a bounded restoration of the earlier current-context
validation path, not a claimed reproduced native crash.

Current job834c4b03/session74052 queues final public Casts source, scalar, callable,
five-target library, example and REPL controls; Reflection binding/retirement;
native/eval aliases and callbacks; existing exact and default native property
alias tests; the current-context controls and compiler build. CLI docs now list
Casts with the accepted features. No111 commit yet. Full plan remains active
after Casts, including stronger Locals, Comparisons and all remaining flags.

Read-only preparation for the subsequent Comparisons audit: builtin comparison
selection already lives in the neutral comparison_policy contract (identity
searches and explicit comparator alternatives). Verify native value-class
ordering cannot bypass the original-kind gate. Also probe comparator return
kinds and ownership before admission: eval_user_sort_compare currently delegates
to borrowed eval_int_value, which coerces to int and releases only its cast
temporary; the native sort adapter accepts Bool directly and casts Mixed results.
No native failure or fix for those observations is claimed yet. Keep the public
admission gated until focused source/runtime/callback proofs close that audit.

### Checked Casts admission (111, verified)

Final job834c4b03 exited0 after16m33s. All17 selected tests passed, each with an
exact single-test filter. Public source evidence/independence, native and opaque
numeric domains (71.61s), callable hosts (72.10s), five-target library emission,
example and real REPL recovery (53.05s) passed. Reference argument selection
(91.63s), independent native/eval mixed and union sources (87.51s), callbacks,
precision exception chaining, clone/COW and destructor replacement (80.61s) passed
with both IR modes and clean heaps. Native/opaque Reflection identity and source
retirement passed all four cases in both IR modes (161.67s). Static rebinding
alias owners passed Casts and Coercions with both modes (169.55s). Existing exact
native/eval reference controls (70.47s), default typed native aliases (44.03s),
current-context activation and pending-feature controls passed. Default Magician
and compiler builds passed. Only the two previously documented inherited Rust
warnings remain; no new warnings were introduced. Earlier five-target structural
and real assembler checks cover the native coordinator and unset fallback.

Casts is now the ninth accepted semantic feature. Source numeric conversion
contracts are enforced through shared kernels; implicit declaration rules remain
independent. Version-two source validation preserves version one, independently
compares native/eval/proposed candidates, and publishes only after all checks.
Physical static rebinding, exact argument retirement, eval capture/unset, context
activation and ownership helpers close the reproduced admission regressions.
The complete umbrella and remaining seven feature admissions are unfinished.
Continue stronger Locals and the Comparisons audit before nullability, mixed,
initialization, arrays, indexing and final cache/host/combination acceptance.
