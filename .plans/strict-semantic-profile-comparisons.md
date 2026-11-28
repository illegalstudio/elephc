# Comparisons implementation history

Historical investigation, failed validation attempts and component proofs for steps112-114.
The final verified status and remaining work live in [the active plan](strict-semantic-profiles.md).
Temporary validation paths in the historical notes have been removed after the final evidence was recorded.

### Comparator callback admission audit (112, in progress)

Added two focused Magician probes before production changes: actual integer
callback returns must retire all sort entry/result owners in permissive and
checked modes, and strict comparator results must retain their original integer
kind. The probes cover usort, uasort and uksort; the ownership control also
reaches builtin fallback from a bytecode user caller. Baseline job ebb80c30 is
queued behind the existing Elephc docblock and WordPress verification jobs.
No runtime outcome is claimed yet. Reuse the existing sort entry/root/container
helpers instead of adding a second ownership lifecycle. Keep original source
policy authoritative for native adapters and maintain all five target paths.


Step112 baseline probes reproduced an outstanding Int(0) owner after permissive
usort and acceptance of a boolean comparator return under Comparisons. Existing
compiled single-unit replays established both failures without compiling or
running a heavy suite outside the queue. Generic sort entries now share the
existing collector, root inventory and partial-result builder; callbacks retain
an explicit result owner and checked consumers reject noninteger original kinds.
Direct writeback uses the existing lvalue/replacement finishers.

Job7907229b passed all seven exact unit gates and the default Magician build
(exit0,23s). This includes both caller entry modes, all three user sorts, five
rejected return kinds and failed insertion at each result position, plus ordinary
sort/key-sort/shuffle regression controls. Two earlier fixture versions stopped
at unsupported VM syntax; the corrected probe enters through VM and executes
builtin fallback inside the walker callee. The uksort control uses strcmp rather
than FakeOps permissive numeric-string ordering. Only inherited warnings remain.
Current jobc6848415/session35840 runs exporter/docs audits, restores the default
Magician archive, executes the opaque native-runtime retirement fixture in both
optimizer modes and builds the compiler. No112 commit yet. Native source-policy
metadata for subsequent step113 is being prepared separately and remains
uncommitted, with no native comparator guard claim yet.


The first opaque fixture attempt exited UnsupportedConstruct before any output.
The production eval FFI explicitly rejects pending semantic profiles, including
Comparisons; staged native lowering bypasses only frontend admission. This is
an admission guard, not evidence of a parser defect. The echo expression-list
syntax is supported by the parser. Added a fast parse assertion to distinguish
these layers. Queued job7d812690/session56173 will check parsing and still meets
the production admission guard until Comparisons is admitted. Do not claim a
native heap result from that blocked probe. Move the opaque execution fixture
into the later complete Comparisons admission verification.

Next native integration keeps the invocation policy in ProfiledFunction EIR,
so internal physical consumers cannot acquire user strictness from the emitter.
Runtime adapters must inspect the original boxed comparator result before any
integer cast, retain it through a catchable failure and support every target.
Descriptor, runtime-string and static callbacks must share the checked result
adapter. Comparator-based set operations need the same policy at their runtime
boundary. Public admission remains gated until those native and host controls
are verified. The other seven pending feature admissions and stronger Locals
remain part of the current user request.


Job7d812690 passed the exact parser/integer-kind unit, then stopped at the
expected production pending-profile guard in the checked opaque test (exit1,
2m22s). The shared-owner integration test now uses a separate permissive PHP
fixture; the checked-result fixture remains parse-verified for later admission.
PHP oracle for the permissive fixture:1,2;1,2;1,2;label-1,label-2.

Prepared step113 native integration retains strict_comparisons on ProfiledFunction
EIR, preserves it through optimizer rewrites, and shares one checked return
adapter across static/descriptor/string sort callbacks. A borrowed/owned native
guard checks the original unboxed integer tag before conversion. Comparator set
operations carry an explicit sixth ABI policy argument and retain their ordinary
cast path for internal/permissive invocations. New tests cover five-target EIR
and runtime emission, real cross-assembly, callback result kinds, descriptors,
string dispatch and heap retirement. These changes are not verified or committed.

Queued job38a0a3a1 was terminated by signal15 while waiting, without running any
tests; cause unknown. Replacement jobdf0ec42d has a persistent task-owned log in
target/strict-comparator-checks-_r_y5j68, wrapperPID3698701. It queues three exact
Magician regressions, the permissive opaque native heap control, all-target
native guard/EIR/assembler tests, native callback/set-operation runtime control,
and compiler build. Remove this exact task directory after collecting evidence.
Registered compiler sources remain frozen during the job. build.rs recursively
hashes every Rust file under src, including files not registered as Rust modules;
new unregistered source files are therefore not outside compiler build inputs.
No112 or113 commit yet; complete profile and stronger Locals work remain active.


### Eval comparator return and owner foundation (112, verified)

The shared eval sort collector/root/result builder now supports both precomputed
sort keys and user comparators. Callback returns are retired independently and
staged Comparisons consumers require an original integer result. Existing direct
lvalue finishers balance callback/target/replacement ownership. Seven exact unit
gates and the default Magician build passed in job7907229b. The three new unit
gates passed again in current jobdf0ec42d, including the checked fixture parser
assertion. Exporter generation and all builtin-docs audits passed in queued
jobc6848415 without generated changes. Only inherited warnings remain.

This is a component foundation commit, with Comparisons still publicly gated.
The production checked opaque fixture meets that intentional admission guard;
its runtime heap validation remains part of the subsequent complete admission.
The permissive opaque control and native callbacks are still running under
jobdf0ec42d. Step113 native policy/guard changes remain separate and uncommitted.


Step112 is signed commit c30508e838808305aa24ab4f0f2dea937e583c65; signature verified against the configured SSH key. Native changes remain uncommitted. Jobdf0ec42d passed all three new Magician regressions, then stopped at a private value_boxing module import in the native adapter. Switched to its public codegen_support re-export. Current jobb310c30b (wrapperPID3719329, same task log) retries only native/permissive runtime gates and compiler build; no new native result is claimed yet.


Jobb310c30b passed the real permissive opaque ownership control in both optimizer
modes with clean heaps (69.57s), native comparator guard emission on every target,
real ELF/Mach-O cross-assembly on all five targets (0.12s), and the original-source
EIR/optimizer policy control on all five targets (0.76s). The native result-kind
fixture then stopped during catalog lowering: the minimal staged helper omitted
the object-cast prelude needed by a dynamically discoverable settype wrapper.
Switched that fixture to capture_staged_eval_runtime, which prepares those
preludes. It executes native functions, not opaque source with a pending profile.

Current job93fb92d4 (wrapperPID3738975, same task-owned log directory) retries the
native callback fixture, the existing all-target set-comparator boundary test,
permissive integer-coercion and snapshot/throw-cleanup regressions, then compiler
build. No runtime result from these new gates is claimed yet. Prepared a separate
unregistered PHP fixture comparator_native_results.php adding valid integer
returns through mixed, descriptors, dynamic names and checked set operations.
Integrate that fixture after the current frozen-input job, then verify those
positive controls before native foundation commit or public admission.

Read-only preparation for stronger Locals recovered original facta7883b31: its
catch-name collision occurred in a permissive native/eval host control. It is
not yet established to be a strict-locals-specific bug, and its root cause and
fix remain unproven. Keep the original reproduction and separate it from policy
choices about stable numeric bindings.


Job93fb92d4 passed the full native callback fixture without optimizations, including
expected output and clean heaps. With both optimizers enabled it stopped after
I1,2; at an uncaught TypeError from the first boolean-result comparator. No113
commit/admission followed. The baseline AST comparison boundary consulted only
neutral policies for automatic modes; explicit comparator consumers had no role.
Added Comparator metadata for usort/uasort/uksort/array_udiff/array_uintersect,
shared comparison-consumer effects in builtin semantics and precise callback
summaries, and a focused AST catch-pruning regression plus four-mode native
execution control. This is a proposed repair pending verification.

The full native fixture now uses comparator_native_results.php and includes
positive original integer results through mixed, descriptors, dynamic names and
checked set operations. Job6f08103b (wrapperPID3838754, same task-owned log) queues
neutral contract roles, catch-pruning, four-mode native execution, five-target
EIR effects, existing default set-operation boundary/coercion/throw controls,
the full native callback fixture, affected eval owner/mode regressions, exporter
and docs audits, restored default Magician archive and final compiler build.
Sources are frozen; queue position2 behind full-date and generics Claude jobs.
Comparisons remains publicly gated; the full user-requested implementation and
all remaining feature admissions stay active after this component work.


### Native comparator return foundation (113, verified)

Final job6f08103b exited0 after5m46s. All twelve exact single-test gates passed:
neutral contract roles; AST catch retention; native execution under all four
AST/EIR combinations (1.62s); all-five-target EIR source policy and may-throw
requirements (0.71s); all-target set-operation boundary; default integer-result
coercion (0.95s); snapshot/throw cleanup (4.43s); full native callback positive
and negative return cases in both optimizer modes with clean heaps (10.17s);
and four affected Magician owner/mode gates. The full native cases cover direct
integer, mixed integer, descriptor and dynamic-name integer returns, both checked
set operations, rejected bool/float/string/object returns and destructor timing.

Exporter build and all six generation/audit commands passed, without generated
docs/registry changes. The default Magician archive was restored and final
compiler build passed. The inherited AtomicU64 deprecation remains. Earlier
jobb310c30b passed permissive opaque sort ownership in both modes with clean
heaps (69.57s), native guard emission and actual assembly for all five targets.
Assembly comments and diff hygiene pass. The neutral Comparator role and shared
consumer effects preserve TypeError independently of pure callback summaries.

This completes the native component, with public Comparisons admission still
gated. Next: admit the candidate profile, switch pending-feature controls to the
next unfinished guarantee, validate native value classes and public callable,
eval, REPL and library hosts, then continue stronger Locals and the remaining
six unimplemented feature admissions. Remove the completed task-owned validation
directory after preserving the verified evidence in Code Journal.


Step113 is signed commit 7a97ec5e6839749ad2358db51423c236a6ed7d58; signature verified against the configured SSH key. Final job6f08103b has twelve exact passing tests and exit0. Native public admission is next, with no remaining work on the verified component itself. The full implementation remains active. Completed task-owned directory target/strict-comparator-checks-_r_y5j68 is being removed after these evidence records.


### Public Comparisons candidate admission (114, in progress)

Candidate INTEGRATED_FEATURE_BITS now includes Comparisons. The two remaining-
feature denial controls use Nullability instead; DynamicCode still refuses opaque
source independently. Added public prefold diagnostics and flag-independence
controls, native/opaque scalar and comparator ownership, callable/builtin alias/
Reflection/eval re-entry, all-five-target standalone library emission, example
and REPL recovery tests. Source evidence includes GMP, DateTimeImmutable and
SimpleXMLElement parameter types, to reject value-class ordering before lowering
without requiring a foreign native package at link time. The runtime scalar
control rejects ordinary Stringable objects before conversion hooks.

CLI docs describe the candidate comparison contracts. No public runtime result
or114 commit is claimed yet. Jobf734b075 (wrapperPID3940145) queues fresh default
Magician/compiler builds and eight exact admission gates. Task-owned persistent
log/script directory:target/strict-comparisons-admission-zd5v5z2e. Source inputs
are frozen while queued/running. Initial position2 follows Claude's full-date
and generics verification jobs. Remove this exact directory after collecting
final evidence. Continue the complete approved implementation after admission;
stronger Locals and six remaining feature integrations are still required.


### Stronger Locals baseline preparation (115, pending)

While114 public admission is queued, job93965f46 (wrapperPID3990340) runs
bounded baseline probes against the freshly built public compiler, without
changing compiled inputs. It reproduces permissive native/opaque catch-name reuse
from knowledgea7883b31 and measures numeric reassignment and overflow with Locals
alone in both EIR optimizer modes. It compares reference PHP output and records
compile/runtime statuses and heap reports. No outcome or root cause is claimed.
Task-owned target/strict-locals-baseline-s90tj50e holds the probe and persistent
log; per-case temporary directories clean up on success and failure. Remove the
parent after evidence is recorded.

Preparation confirms which existing mechanisms to extend: LocalStoreContracts
already carries scoped source-point declaration membership to both statement and
expression stores; conversion_boundaries guards rooted candidates before storage
adaptation. Keep the owning Locals policy and diagnostic independent from
Coercions. Reuse generic_local_contracts for eval aliases and persistent reference
sources. Do not pin array elements as a side effect of stable whole-value bindings,
and do not replace scoped read/narrowing proofs with final frame-slot types.


### Public host admission follow-ups (114, unverified repair)

Jobf734b075 passed fresh Magician/compiler builds and six exact gates, including
public native/opaque scalar/comparator domains in both modes with clean heaps
(73.66s), source/value-class diagnostics, flag independence, all-five-target
libraries and the example. The host fixture then used unsupported native reference
array elements. Replaced reflected usort with reflected array_udiff so Reflection
still exercises a comparator consumer. Jobb3a45f2c found the other fixture limit:
array_map currently takes one array. A unary user proxy preserves the original-kind
callback test without extending that builtin. Job8b9316ca then found a genuine
SIGSEGV after ABCDEFG, at the first-class usort alias with mixed parameters.

Diagnostic jobbe8e0fc4 reproduces the minimal crash with both permissive and
Comparisons profiles. Symbolized core4051900 shows mixed_deref -> mixed_clone ->
crashBadCompare with raw integers2/1 passed as boxed pointers. No OOM evidence;
free memory42Gi. The extracted core copy was removed. Candidate helper
callback_binding::direct_callback_requires_argument_boxing selects the existing
owned descriptor invocation boundary for these raw-to-mixed argument adapters;
static ABI-compatible fast paths and method adapters retain their own routes.
New native regression covers integer/string payloads, invalid results, explicit
throws and both profiles/optimizer modes. Library emission also exercises a raw
integer receiver with mixed callback parameters. Job4dc8fa0a (wrapperPID4067231)
queues the fresh compiler build, this regression, public hosts/REPL, five-target
libraries and two existing boxed-sort ownership controls. No repair result or114
commit yet. PHP reference confirms permissive always-true comparison sorts2,1 to
1,2; the new test currently expects2,1 in that one permissive assertion, so correct
that expectation after the frozen running job reports, then rerun the exact gate.

Baseline job93965f46 measured integer-to-float and overflow stores with and without
Locals in both modes: all still print double, with clean heaps. Knowledgea9725473
retains this stronger-Locals gap. The simplified catch reuse control succeeds if
its first TypeError catch actually runs and unsets the name. Refined jobbe8e0fc4
omits that prior detachment and reproduces the original permissive collision in
both modes: PHP printsTVN, native printsT then exits255 with Typed local $error
expects TypeError. Knowledgeffd0deed records the precise trigger; no catch fix.
Keep both task-owned target directories until the evidence is persisted and remove
them afterward. Full implementation and all remaining profiles remain active.


### Boxed callable reference contract follow-up (114, candidate)

Job4dc8fa0a eliminates the original scalar-pointer crash through the initial
integer/string orders, then fails a late callable usort with a reference-layout
Error. Job71173afd corrects the PHP permissive boolean order to1,2 and uses a
fresh dynamic-string receiver, but the same refusal remains. Knowledge1e04fa0d
supersedes the reassignment hypothesis16eb283c. Diagnostic jobc43df1eb stops at
__rt_tagged_reference_argument_error in _eir_main_callable_invoker_2191: source
marker tag4 addresses raw array storage while the builtin wrapper requires tag7
boxed storage. No reference-array contents had reached the throwing callback.

Candidate uses_boxed_reference_payload in types/reference_layout is shared by
checker output planning and descriptor-argument lowering, covering the PHP array
union and other Mixed-backed contracts without confusing inline tagged integers.
Job090f9343 passed both existing five-target reference-marker/layout refusal gates,
but still hit the late native refusal. The contextual first-class-builtin checker
returns before ordinary callable reference planning; check_builtin_callable now
explicitly asks its existing normalized builtin-argument walk to prepare the
actual callable wrapper signature before that early return. Direct builtin
specializations keep their own ordinary storage plan. No runtime layout guard is
removed. New boxedLibraryOrder forces post-catch dynamic dispatch in every target's
library emitter test.

Job1627fcc1 (wrapperPID4188133) queues fresh build, two exact five-target reference
gates, native scalar/string/throw ownership, public hosts/REPL/libraries, existing
boxed-sort COW/throw/callable controls and source/independence, followed by the full
builtin exporter/generation/audit workflow and restored default Magician/compiler
builds. Source inputs are frozen; no114 commit or complete admission result.

The baseline directory strict-locals-baseline-s90tj50e is removed after its ten
clean baseline records and refined failing records were retained in knowledge and
this plan. The exact no-prior-detachment catch source is now an unregistered
future fixture tests/fixtures/strict/catch_scope_contract.php (PHP syntax checked).
Do not accidentally stage it into114 before its separate regression and repair.
For the catch repair, inspect current snapshot initialization/value provenance;
a future frame-slot Object class is not proof that a binding existed at this eval
site. All stronger Locals and remaining feature/umbrella work remains active.


### Callable array-preservation correction (114, unverified)

Job1627fcc1 passed the two existing five-target reference-layout gates, then the
new native regression failed source checking at the second and fourth usort of
the same integer receiver. Preparing the wrapper's broad Mixed type had overridden
the reviewed preserves_array_reference output, and that invented a non-array
retyping requirement on storage already used by a builtin. Candidate now gives
the semantic array-preservation contract priority and boxes its PHP-array union
without discarding the logical array type. No alias-escape eligibility or runtime
refusal guard was relaxed. Jobda103dc8 (wrapperPID38088, callable_reference2.log)
repeats the same nine exact gates and, only after success, docs exporter/audits and
restored default builds. Inputs are frozen. No114 commit or final result yet.


### Boxed receiver dispatch timing (114, candidate)

Jobda103dc8 passes both five-target reference gates and the complete new scalar/
string/invalid/throw regression (6.27s, both profiles and optimizer modes, clean
heaps). Public callable hosts then fail during EIR emission: usort receives Mixed
in main. The caller-storage plan is applied by argument lowering after
lower_boxed_usort has already selected the raw sorter from the previous slot.
Candidate lower_boxed_usort now applies the checker-authorized whole-local
reference promotion before inspecting the receiver representation. Source-order
argument evaluation and the existing sorter publication/cleanup graph stay shared.
Job8f891928 (wrapperPID61639, boxed_dispatch.log) queues the same nine focused
gates and conditional docs exporter/audits/default restoration. No114 commit or
complete public admission result. The future catch fixture remains unregistered
and excluded from this step; every remaining user-requested feature stays active.


### Primary public gates passed; method ownership follow-up (114)

Job8f891928 passed all nine exact gates and all six builtin generation/audits,
restored default Magician/compiler builds, and exited0 after6m43s. Public hosts
passed in102.05s, REPL recovery95.37s, scalar/string/function callback cleanup5.47s,
five-target libraries1.39s, with both optimizer modes and clean heaps where tested.
No generated docs changed. The initial public native/opaque domain gate and
example had already passed in jobf734b075. No114 commit yet: an additional bounded
method audit found four orphan40-byte Mixed integer argument cells (160 bytes)
when instance/static mixed-parameter comparators throw from their checked
integer-versus-float comparison. Both strict modes printMSD with17allocs/13frees;
both permissive modes printPPD with13/13 and clean heaps (jobb4c1c083).

Candidate argument-boxing predicate now reuses instance/static target metadata
as well as free-function parameters to select the owned descriptor consumer.
New method fixture covers integer and heap-string argument boxes, four failure
catches, receiver destructor and source argument counts. Library emission includes
raw integer instance-method and raw string static-method callbacks. Job8cf9054e
(wrapperPID135397, method_repair.log) runs the new four-profile/mode executions,
existing scalar-argument regression, five-target libraries, source-arity method
adapters and both existing string-method controls, then docs audits and restored
builds. Inputs frozen; method repair unverified. Finish this before committing114,
then continue the catch-source/stronger Locals and all six remaining profiles.
