# Complete the Core class and constant surface

- [x] Compare the shared symbol catalog with the pinned PHP baseline.
- [x] Register and test portable Core constants in native code and opaque eval.
- [x] Implement target, build, installation, and executable constants with truthful Elephc values.
- [x] Complete enum interfaces and their shared catalog contracts.
- [x] Implement missing throwable classes, including ErrorException's extended constructor.
- [x] Implement predefined attribute classes and their validation and runtime behavior.
- [x] Implement SensitiveParameterValue and sensitive backtrace redaction.
- [x] Implement Serializable dispatch in serialization and unserialization.
- [x] Implement WeakReference and WeakMap with non-owning object edges and GC integration.
- [x] Add examples, regenerate builtin documentation, and run focused parity and target checks.

## Baseline and completion criteria

The pinned `scripts/docs/php_baseline.json` is PHP 8.5.10. At branch base
`24857c7717`, Core functions cover 62/62, class-likes cover 21/40, and constants
cover 37/89. Existing post-baseline extensions retain their version metadata.
Catalog presence alone does not establish behavioral compatibility. Each class
needs native and eval behavior, PHP-compatible validation, ownership tests where
relevant, and support for all five supported compilation targets.

## Missing class-likes

`AllowDynamicProperties`, `Attribute`, `BackedEnum`, `ClosedGeneratorException`,
`CompileError`, `DelayedTargetValidation`, `Deprecated`, `ErrorException`,
`NoDiscard`, `Override`, `ParseError`, `RequestParseBodyException`,
`ReturnTypeWillChange`, `SensitiveParameter`, `SensitiveParameterValue`,
`Serializable`, `UnitEnum`, `WeakMap`, and `WeakReference`.

Enum interface relationships already exist in parts of Magician; audit the
native checker, instance checks, Reflection, and catalog joins before changing
them. Attribute names already influence some checker and eval behavior, but
their classes are not yet available for construction and Reflection.

## Missing constants

Portable groups: `TRUE`, `FALSE`, `NULL`, `PHP_FLOAT_DIG`, the fourteen
`PHP_OUTPUT_HANDLER_*` flags, the eight `UPLOAD_ERR_*` codes, and the four
Zend debug/thread flags (`PHP_DEBUG`, `PHP_ZTS`, `ZEND_DEBUG_BUILD`,
`ZEND_THREAD_SAFE`). Boolean/null lookup must preserve PHP's special
case-insensitive names without relaxing ordinary constant case sensitivity.

Environment-dependent group: `DEFAULT_INCLUDE_PATH`, `PEAR_EXTENSION_DIR`,
`PEAR_INSTALL_DIR`, `PHP_BINARY`, `PHP_BINDIR`, `PHP_BUILD_DATE`,
`PHP_BUILD_PROVIDER`, `PHP_CLI_PROCESS_TITLE`, `PHP_CONFIG_FILE_PATH`,
`PHP_CONFIG_FILE_SCAN_DIR`, `PHP_DATADIR`, `PHP_EXTENSION_DIR`,
`PHP_FD_SETSIZE`, `PHP_LIBDIR`, `PHP_LOCALSTATEDIR`, `PHP_MANDIR`,
`PHP_MAXPATHLEN`, `PHP_PREFIX`, `PHP_SBINDIR`, `PHP_SHLIB_SUFFIX`,
`PHP_SYSCONFDIR`, and `ZEND_VM_KIND`. Values must describe Elephc rather than
copy paths or build metadata from the Docker image used for the PHP baseline.
`PHP_BINARY` must be resolved in the running program, never from the compiler
process. Document concepts that do not apply to a compiler without Zend.

## Verification

Use focused codegen and error tests, shared catalog/checker parity across all
targets, Magician unit tests, and selected PHP-oracle comparisons. Regenerate
the symbol registries and compatibility documentation through
`update-builtin-docs`; do not count placeholders as completed support.


## Completed verification

- Shared baseline contract gate: Core functions 62/62, class-likes 40/40, constants 89/89.
- Focused Core tests: 24 class tests, 27 review regressions, 5 constant tests, and 7 diagnostic tests.
- Native and opaque eval cover weak object and closure keys, identity, cloning, live iteration,
  sensitive traces, deferred attributes, executable-path aliases, and serialization hooks.
- Heap-debug regressions cover cycles, key resurrection, throwing value destructors, and
  callable and receiver owners extracted from locals widened by a future eval barrier.
- Existing callable ownership, ArrayAccess dispatch, and GC exception recovery regressions pass.
- Core declaration checking and capture ownership checks cover all five supported targets.
- Runtime assembly and representative programs assemble for every supported target. Focused
  Linux ARM64 programs also execute successfully with clean heap reports. Apple targets were
  verified by compilation and assembly, without native execution on this Linux host.
- The example matches PHP output and leaves a clean heap. Generated docs, site compatibility,
  builtin/EIR boundary, Rustdoc, assembly comments, and whitespace checks pass.

Existing eval restrictions on the standard stream constants and Reflection metadata retention
remain documented separately. The general property replacement ordering issue found during
WeakMap work is tracked as Code Journal task `21f22b33`.

## PR 1870 review corrections

- [x] Fix and verify every distinct failure in the initial CI matrix.
- [x] Preserve nested serialization reference state and validate legacy hook results.
- [x] Accept tentative Serializable method signatures without mandatory return hints.
- [x] Warn for discarded eval instance method results without reevaluating receivers.
- [x] Regenerate builtin docs and run focused regression and target checks.

The follow-up also protects the eval error-handler C boundary, consumes thrown callable
boxes, and tests callback exceptions with heap debugging. Local verification includes
all 95 serialization regressions, the example corpus, the initial CI failures, and
seven focused ARM64 executables. CI and Greptile outcomes for the published commit
are recorded on PR #1870 and in the shared Code Journal.

The follow-up ARM64 review found an ABI scratch-register clobber in nested hydration.
Load the sharing mode before taking the reference-counter address; both modern
hydration hooks now have a heap-debug regression that also executes on ARM64.

The separate nonempty modern hydration-data owner issue is tracked as Code Journal
task `a1321652`; its parse/insert path is unchanged from main. The focused nested
reference regression uses explicit empty modern payloads to isolate reference state.


## CI follow-up after the serialization review

- [x] Scope whole-module assertions to their fixture functions after Core prelude injection.
- [x] Audit aligned dynamic serialization snapshots without enlarging the ABI allowlist.
- [x] Validate and reduce repeated work in the archived REPL cache regression.
- [x] Schedule each target of the argument-owner probe independently under the existing timeout.
- [ ] Publish focused validation and verify CI and Greptile on the final commit.

Run `39038329288` repeated three structural assertion failures and one ABI-audit
limitation across executable platforms. Linux ARM64 also reached the global
60-second limit in a five-target argument-owner test, and the macOS archived
REPL probe reached 240 seconds while rerunning the broader cache lifecycle test.

Focused validation passes 40 library/CLI cases under the CI profile, including
all five targets, the complete x86 runtime alignment audit, and two negative
audit regressions. The archived REPL cache test passes in 86.48 seconds. The
compiler build is warning-free, Rustdoc checks and `git diff --check` pass, and
no global test timeout or production runtime behavior changes in this follow-up.


## Independent Grok review correction

- [x] Reproduce default ErrorException construction lines on the reviewed head.
- [x] Stamp ordinary Throwable construction lines after defaults for fixed, dynamic, and late-static allocation.
- [x] Verify native inheritance, factories, delayed throws, explicit zero/null overrides, and native-to-eval getters.
- [x] Check all three allocation paths on all five targets and assemble representative programs.

Grok delegation `14b44edf-751e-438e-9b98-064b8b1b031f` found one actionable
defect on `c5cbece4b4`: a default ErrorException line read zero because ordinary
property initialization cleared the inherited creation-line word. Five focused
codegen tests and the all-target allocation test validate the repair. Review
outcomes for the final published head remain tracked in Code Journal and on PR1870.

The broader opaque-eval construction-location gap affects existing Exception as
well and is task `5b678980`. Separate owner-retention controls using compact
Exception and stdClass are tasks `f53ab0fe` and `6436dcd1`. The new native and
bridge regressions each retain their clean-heap assertions.


## Second independent Grok correction

- [x] Reproduce protected `code` assignment clearing construction metadata.
- [x] Reserve ordinary Throwable metadata after PHP properties and the optional dynamic hash.
- [x] Keep property/GC payload sizes distinct from full allocation sizes.
- [x] Update allocation, location readers, uncaught reporting, and shallow-copy storage on both architectures.
- [x] Add scalar, compound, dynamic-name and eval Reflection write regressions.

Grok delegation `74583fbf-67ea-42f2-86fc-b1000645293d` found that the first
repair still reused the high word of `code`. The second repair gives ordinary
Throwables a separate metadata word. Dynamic-name scalar writes have an existing
boxing-owner leak even for a plain user class (Code Journal task `0a0ba2ab`), and
Reflection metadata retention remains task `7215b48c`. Those functional probes
check line preservation; the native scalar/compound and native-to-eval location
probes separately require a clean heap.


## Third independent Grok correction

- [x] Reproduce explicit ErrorException locations diverging between getters and fatal reports.
- [x] Share a borrowed effective-location reader between getters and uncaught reporting.
- [x] Preserve explicit line zero and empty filenames without inventing synthesized locations.
- [x] Correct stale stdout and location-storage documentation.
- [x] Validate regressions, stack alignment, and all-target runtime assembly.
- [ ] Obtain clean Grok and Greptile 5/5 on the next published head.

Grok delegation `903c8eeb-8a94-49e9-856b-2e7cbb21ac81` confirmed the metadata
tail repair and one further medium-severity fatal-report mismatch. PHP reports
`custom.php:73` for explicit constructor arguments, while the old report used
the construction path and line. The effective-location reader returns a borrowed
filename, line, and a distinct known-origin flag, so getters and reports agree.

PHP 8.5.10 source and PHP-oracle controls also confirm that a supplied filename
with an omitted/null line records zero, and null constructor arguments preserve
previous location overrides. The injected constructor now follows those rules;
native and opaque-eval regressions cover defaults, reinitialization, and ownership.

Explicit eval `__construct` calls now fall back to the existing constructor
signature when no ordinary method signature is registered. The named-argument
reinitialization regression passes with a clean heap after that shared binding
repair. Final focused validation passes 12 Core cases, 7 existing uncaught-report
cases, 6 Magician argument/ownership cases, 4 report-emitter checks, the x86 runtime
stack audit, and complete runtime/program assembly for all five targets.

## Fourth independent Grok correction

- [x] Reproduce ErrorException reinitialization clearing code and previous state.
- [x] Preserve omitted messages, zero code, and null previous arguments with PHP's actual argument-count rules.
- [x] Reuse the ordinary constructor body and parent initializer instead of bypassing its state logic.
- [x] Keep compiler-provided argument collectors stable when opaque eval captures frames.
- [x] Verify native and opaque-eval state, previous-object identity, excess arguments, and clean heaps.
- [x] Assemble the repaired constructor and complete runtime for all five supported targets.

Grok delegation `bac13ecc-a309-4c75-8049-b6ba5cfd4965` found that repeated
constructor calls unconditionally replaced the inherited message, code, and
previous exception. PHP preserves the message only with no supplied arguments,
the code when zero, and the previous exception when null. Supplying a later named
argument fills earlier defaults, so the message then becomes empty. The injected
body now resolves those values before calling the existing parent initializer.

The compiler-owned collector records the actual supplied argument count without
being exposed as a PHP variadic. A desugaring regression prevents eval frame
capture from snapshotting that collector a second time. All 14 focused
ErrorException/inherited-constructor cases and three argument-capture unit tests
pass. The new native and opaque-eval regressions agree with PHP and leave clean
heaps, including previous-object aliasing and the six-argument limit. The compiler
build, complete all-target assembly, documentation audits, and diff hygiene pass.


## Fifth independent Grok and CI correction

- [x] Reproduce dynamic associative constructor unpacks failing EIR arity.
- [x] Reproduce a later named argument losing an explicitly supplied message.
- [x] Preserve keyed reads and compute supplied optional positions for compiler collectors.
- [x] Discover builtin method dependencies from lexical and explicit static calls.
- [x] Reproduce the CI parent-symbol failure locally and verify all five dependency paths.
- [x] Complete focused native/eval and CI failure regressions and target checks.
- [ ] Obtain fresh Grok and Greptile reviews on the replacement published head.

Grok delegation `1a49c824-40d3-4777-b56b-b5787d6dd4d5` returned NOT CLEAN on
`8ce1067919`. A hidden variadic had excluded the ordinary associative binding path;
`new ErrorException(...$args)` forwarded one array instead of seven physical
arguments. The dynamic prefix plus later named path also supplied count zero,
restoring the old message despite an explicit replacement. The shared EIR helpers
now bind the visible parameters and derive the count from present keys and
explicitly supplied parameter positions.

CI run `39062619662` exposed a second integration defect: after removing the
constructor bypass, static parent calls needed their builtin method dependencies
emitted even when no parent object was instantiated. Curl-free literal eval and
web/profile fixtures linked an ErrorException body without Exception's parent
constructor. All 19 failed jobs inspected before cancellation shared this one
undefined symbol. The dependency scan now includes static calls. The all-target
EIR/assembly regression passes, as do all three previously failing CLI profile
cases and the curl-free eval regression.

The heterogeneous dynamic-prefix checker limit reproduces with a plain user
class on unchanged planner/checker paths and remains task `1edb64c8`. It is
separate from the confirmed count-zero case, whose typed previous prefix compiles
and printed `kept:7` instead of PHP's `fresh:7` before the repair.

Final focused verification passes all24 ErrorException, inherited-constructor,
optional-frame, curl-free eval, and literal-eval codegen regressions in84.538s.
The two associative native/eval cases also pass independently with clean heaps.
Three CLI web/profile regressions pass. Complete runtime and representative
associative constructor programs assemble for all five targets, including an
array returned by a function and a later named-argument prefix. The final compiler
build is warning-free; builtin docs generation and all documentation, site, EIR
boundary, Rustdoc, and whitespace checks pass without generated registry changes.

An exploratory plain-function argument collector reaches an existing Iterable
representation diagnostic and is tracked separately as task `e93eafd7`; it is not
claimed as covered by the constructor repair. Final review and CI results are
recorded against the immutable published SHA in Code Journal and on PR1870.


## Sixth independent Grok and Greptile correction

- [x] Reproduce numeric unpack keys disappearing, unknown names being accepted, and lost overflow.
- [x] Extend the existing EIR iterator binder for visible defaults and compiler-owned collectors.
- [x] Preserve positional insertion order, duplicate/name-order errors, and the actual argument extent.
- [x] Keep fixed, instance, lexical parent, class-string, and late-static constructor dispatch coherent.
- [x] Retain a reusable class-name root across nonmatching dynamic constructor candidates.
- [x] Finish the native/eval regressions, all-target checks, and documentation audits.
- [ ] Obtain a clean Grok review and Greptile 5/5 on the final published SHA.

Grok delegation `0becf7d6-53a9-4444-8c63-86cd66883156` returned NOT CLEAN on
`8a7d6737f2`; Greptile independently rated that head 4/5 for the same binding gap.
The string-key probes could not represent the PHP argument stream. The replacement
walks runtime arrays in insertion order, binds integer entries positionally, tracks
named occupancy, supplies missing defaults, and retains overflow in the hidden
collector. The constructor receives its actual PHP argument extent and enforces
its six-argument limit through the normal body.

Late-static construction now reuses planned fixed-class branches. Repeated class-name
probes borrow a guarded root instead of consuming an owned temporary on the first
nonmatching branch. The dispatch planner has a dedicated module. Native probes match
PHP for sparse keys, multiple unpacks, explicit locations and previous identity,
lexical parents, factories, and all three binding errors, with clean heap reports.

Direct constructor calls retain their normal EIR binding and dispatch. Dynamic class-string
construction and constructor callables keep the descriptor binder. Its nullable integer
coercion now stages both payload and tag before later arguments and the captured receiver,
preserves the pair across temporary-box cleanup, and rejects unknown names for hidden
collectors. Native callback probes match PHP with clean heaps. This completes the ABI work
tracked by Code Journal task `5e4a396e` as part of the Core correction.

Eval binding also creates catchable `Error` objects for unknown names, duplicate slots, and
positional entries after named unpack entries, preserving ordinary argument cleanup.
The published `8a7d6737f2` PDO workflow passed; its main CI was cancelled after the
new review finding. Replacement exact-head CI and review results remain pending.


Final local validation passes 28 focused codegen cases, 82 Magician argument/scope cases,
all 28 descriptor-invoker unit cases, and five focused library checks. The latter include
all-target direct/late-static/callable construction, nullable ABI width, boxed binding,
parent reachability, and complete x86 runtime call alignment. Native/eval stream and
constructor-callable regressions require PHP-equivalent output and clean heaps. Combined
stream/callable programs compile and assemble for all five targets. The compiler build is
warning-free; builtin documentation generation/audits, EIR boundary, Rustdoc, assembly
comments, and whitespace checks pass without generated registry changes.

Final review and CI acceptance are recorded on PR1870 and in Code Journal against the
immutable published SHA. No clean-review or green-CI claim applies to an older head.


## CI catchability fixture follow-up

CI run `39069667333` passed 1721 Magician tests and ignored one, with a single
failure expecting `RuntimeFatal` for positional values after named call-array
entries. The updated test catches `Error` and checks the PHP message. Both owned
and legacy array traversal preserve the shorter `call_user_func_array` wording;
actual unpack syntax retains its `during unpacking` suffix.

All 13 focused call-array unit cases pass, including direct and context-level
errors. Four native/eval constructor stream/callable checks pass after the
message-context change. The extended native/eval callable cases also pass with
PHP-equivalent call-array diagnostics and clean heaps. Build, generated docs,
registry/site/EIR audits, Rustdoc and whitespace checks pass. Final exact-head CI
and review results remain recorded externally on PR1870 and in Code Journal.


## Nullable reference-marker owner correction

Greptile's review of `120e062efc` rated it 3/5 for a temporary-box leak that also
applied to `260cf07cb6`. Reference markers read into by-value nullable integer
arguments materialize owned Mixed cells. The converter already produced the
two-word TaggedScalar ABI, but its support predicate still rejected that target,
so the caller skipped cleanup. Six supported variadic-reference forwarding calls
printed the correct result while retaining six 40-byte boxes.

The support predicate now agrees with the nullable converter, enabling the existing
conversion root and release path. The same six-call probe has 112 allocations and
112 frees after the repair. The new GC regression also covers string and null
reference sources, positional and named forwarding, and a clean final heap.

All six focused callable/ownership regressions pass in 27.337 seconds. All 33
focused library checks pass, including all 29 invoker cases, owned-marker cleanup
on every target, the existing constructor/boxed/parent target checks, and full
x86 runtime call alignment. The reference-marker program compiles and assembles
for all five targets. Compiler/exporter build, generated documentation, registry,
site/EIR boundary, Rustdoc, assembly-comment and whitespace checks pass.


## Seventh independent review and ARM64 execution correction

Grok delegation `0a28781f-5c49-41e2-b16d-86b4a6046a9c` completed NOT CLEAN
on `260cf07cb6`. The nullable reference-marker owner finding was already fixed
in `2f75f88f75`; its final verdict did not review that later head. Greptile then
rated `2f75f88f75` 4/5 for the ARM64 nullable result-register restoration.

The planned dynamic-new miss now calls the single argument-container fallback
in `object_construction.rs`. Removing the duplicate raw helper preserves unknown
names, positional unpack entries, and cleanup for nonmatching planned classes,
Exception, ErrorException, and typed variadic constructors. Nullable cleanup
restores TaggedScalar into the integer-result and tag registers independently
of the string ABI. The all-target regression checks the post-release pair.

Actual Linux ARM64 execution exposed two additional ownership/ABI edges. Descriptor
entry wrappers now reserve the shared caller-stack gap around instance, static,
and late-bound calls with overflow arguments. Temporary by-reference Array(Mixed)
cells retain borrowed associative storage even though the PHP types differ. The
full dynamic constructor and nullable reference regressions pass under QEMU with
clean heaps after these repairs. A boxed static/late-bound method regression
covers nine visible integer arguments.

The preceding `2f75f88f75` CI run also hit a GHCR Docker-login infrastructure
failure before its affected shard ran tests. Replacement exact-head CI, a new
independent Grok review, and Greptile 5/5 remain mandatory publication gates.
Their final outcomes are recorded on PR1870 and in Code Journal against the
immutable published SHA.

Final focused verification passes all35 selected library checks and all9 native/eval
codegen cases. After the last ownership refinement, the three new executable
regressions pass again. The compiler/library/exporter build is warning-free and
generated docs, registry/site/EIR audits, Rustdoc, assembly comments, and whitespace
checks pass without generated drift.

Combined nullable, dynamic-constructor, and static/late-bound descriptor programs
compile and assemble with clang on all five supported targets. iOS device and
Simulator use the supported static-library output mode.


## Integration with current main

After `8ace75e93c` was published, main advanced through five independently
reviewed PRs to `4b5681d5fc4da297073dbae64d0b7c811913803e`. GitHub reported
conflicts and did not start replacement CI/PDO workflows. The integration
preserves both Core and BCMath prelude registrations in the compiler and test
harnesses, keeps Core attribute classes alongside RoundingMode in the catalog,
and combines the current JSON state with legacy Serializable bookkeeping.
Generated module/comparison documentation is regenerated from the merged catalog.

Focused validation checks the corrected Core constructors and callables together
with weak GC/resurrection, Serializable, enum interfaces, BCMath Number/RoundingMode,
JSON locations, incoming-edge/scalar cleanup, and tail-call ownership boundaries.
Final Grok and Greptile acceptance must cover the resulting published integration
SHA against `4b5681d5fc4da297073dbae64d0b7c811913803e`.

The integrated compiler/library/exporter build passes without warnings. All55
selected library checks,18 integration codegen cases, and10 Core/BCMath diagnostic
cases pass. Both prelude registrations remain active. Generated docs and
registry/site/EIR audits pass; the comparison page reports Core40/40 classes and
89/89 constants, with overall coverage163/329 classes. Compiler/clang syntax
checks pass for all5 targets, and Rustdoc, assembly comments and whitespace pass.


## Eighth independent review and eval callback correction

Grok delegation `7ca6cc4a-e657-4a1a-9f8a-d8678caa3a3d` completed NOT CLEAN on
`f5cb1cbab915d0b7eba6dc53f4eeec41d0ed5e7c` against main `4b5681d5fc`. The
separate active eval-to-native descriptor wrappers still omitted the ARM caller
gap. An all-target regression fails before the repair and passes after both
instance/static wrappers adopt the shared ABI padding. A valid opaque-eval
callback fixture matches PHP and passes with clean heaps on x86_64 and under
Linux ARM64 QEMU, including ErrorException and a nine-argument static method.

The review also identified repeated DelayedTargetValidation being accepted.
Pinned PHP8.5.10 and its official Zend header establish the underlying layout:
TARGET_CONSTANT=64, TARGET_ALL=127, IS_REPEATABLE=128. DelayedTargetValidation's
flags127 are correct. The implementation retained the older repeatable bit64,
so the constant target was mistaken for repeatability. The shared contract now
owns the complete Attribute constant list and masks, consumed by the prelude,
AOT/eval declaration validators, and both ReflectionAttribute factories. Native
and interpreted regressions reject repeated DelayedTargetValidation and
Deprecated; object/reflection tests assert the PHP8.5 values.

The eighth verdict covers the older published head. A fresh independent Grok
review, Greptile5/5 and CI acceptance are required on the final repair SHA.


## Runtime executable defaults and final CI corrections

Greptile rated integration head `f5cb1cbab9` 3/5. Executable-path defaults
were incorrectly resolved from the empty metadata placeholder. A shared scalar
ABI marker now preserves the runtime value across aliases, descriptor invokers,
eval binding, nested array defaults, and Reflection metadata. Initial declaration
checking retains known constant types. Fresh strings transfer into Mixed/hash
storage or retire after indexed-array copying; constant declarations transfer
owned executable-path results into their global box.

The reported native WeakMap nested assignment already wrote through correctly.
A new regression covers direct and compound nested writes and requires a clean
heap. It also exposed a computed Mixed value retained after the boxed writer
stored it. Nested writes now reuse shared operand roots and retire parent, key,
and value on normal and exception paths. The regression passes on native x86_64
and actual Linux ARM64. Existing nested-write, evaluation-order, and nullable
property cleanup regressions pass as well.

The three failed non-codegen shard5 jobs in CI `39073868167` share the same
x86 stack-audit failure. The fatal report reserves location scratch space before
a terminal exit syscall; the static CFG previously fell through to the fallback
label with another frame depth. Explicit `ud2` terminators preserve the actual
nonreturning behavior without expanding audit allowlists. All8 SysV audit tests
pass after the correction.

A generic opaque-eval grammar restriction accepts only one subscript dimension
in assignments, reproduced with ordinary arrays and tracked as task `95e8d65f`.
A separate literal-default callable control with a later eval barrier retains a
descriptor and captures; task `97836066` tracks exact base attribution. Neither
is counted as fixed by the executable-path regression.

All30 descriptor-invoker unit tests and the shared scalar/nested-array ABI decode
regression pass. The final executable-default regression passes with clean heaps on x86_64
and actual Linux ARM64. The same program compiles and assembles on all five
targets, including iOS device and Simulator static-library output. The final
compiler/library/exporter build is warning-free. Complete documentation and
registry/site/EIR audits pass without generated drift. Publication gates are
recorded in Code Journal against the immutable published SHA.
A fresh clean Grok verdict, Greptile5/5, and green CI on that same SHA remain
required before this plan is complete.


## WeakMap clone and entry reference validation

Pinned PHP8.5.10 exposed shared nested array values across WeakMap clones and
confirmed that native entry references must remain valid after their weak key
or map is destroyed. A private managed-reference helper now lowers entry
references without changing offsetGet reflection. Clone separates ordinary
entry zvals and preserves live reference sets. Singleton managed Mixed reference
cells clone their boxed value storage. Replacing an entry retains PHP's
detachment of the old external reference.

The opaque-eval clone regression also exposed an x86_64 stack alignment defect
while replacing a refcounted static symbol. Its cleanup now preserves the new
value in the shared aligned stack slot before releasing the previous value.
The regression exercises the actual Rust release callback.

Grok9's proposed ErrorException message-preservation change was rejected after
three exact PHP8.5.10 counterexamples: a later named filename, named code, or
filename-only unpack fills the earlier message default and clears it. Existing
constructor behavior matches the oracle. A new independent review must cover
the repaired published SHA, alongside Greptile5/5 and green CI.

The four new WeakMap regressions pass with clean heaps on native Linux x86_64
and actual Linux ARM64. All24 reference-cell ownership tests,13 symbol ABI
tests,8 SysV audits,5 reference-cell emitter tests and the five-target WeakMap
lowering regression pass. The updated example matches pinned PHP8.5.10 and
leaves a clean heap. Compiler/library/exporter builds and generated documentation,
registry/site/EIR, Rustdoc, assembly comments and whitespace audits pass.


## Synthetic reference parity follow-up

CI's curl contract gate exercises all injected preludes and exposed missing
RefAssign cases in the synthetic PHP printer and call audit. Both now traverse
and render reference sources; the parameter-read walker also records their
reads and targets. A property/nested-entry reference round-trip test verifies
that calls in index expressions remain visible to the audit. The executable
WeakMap semantics and target ABI repair are unchanged.

All7 prelude parity tests pass with curl enabled, and all5 printer tests pass.
The compiler/library/exporter build is warning-free. Regenerated documentation
has no drift; registry/site/EIR and whitespace audits pass.


## WeakMap reference receiver resolution

Greptile rated0ab94c46e7 at4/5 because property and method results typed as
WeakMap missed the narrow isset receiver probe. A regression reproduces the
object-as-array EIR validation error before the repair. Reference lowering now
uses the shared instance receiver resolver, including property, instance-method
and static-method return types. The fixture checks aliasing, one-time operand
evaluation and reference lifetime after receiver/key cleanup against PHP8.5.10.
The five-target lowering fixture includes these receiver forms.

The same fixture exposed a fresh RHS leak in typed ArrayAccess property writes
even without entry references. The property writer now shares the existing
boxed/local writer's evaluation order, unwind roots and operand retirement.
The receiver regression keeps a clean-heap assertion; a second PHP-matched
fixture covers throwing value evaluation and invalid keys passed to offsetSet.

All6 WeakMap regressions pass with clean native heaps. The five-target EIR
fixture and all7 prelude parity tests pass. The compiler/library/exporter build
is warning-free and generated docs, registry/site/EIR, Rustdoc and whitespace
audits pass without generated drift.


## Native WeakMap write-context completion

Grok10 confirmed three further native gaps on462a8cc962: direct by-reference
arguments, increments with active entry references, and static property entry
references. Adjacent review added closure-returned receivers, settype and append.
The shared receiver resolver now handles static properties and known closure
signatures. Direct reference arguments bind and lease the private managed cell
through the existing call planner and evaluation ledger.

Pinned PHP8.5.10 distinguishes these writes: ++/-- and array append preserve
connected references; plain assignment, += and assigning ??= detach them. Missing
entry reference reads and increments throw Error. Increment lowering recognizes
the parser's implicit one and old/new expression snapshots, then uses ordinary
local increment semantics through a scoped entry alias, including string values.
Append and literal settype use the same alias. Typed EIR casts update Mixed
reference cells for settype instead of overwriting a local value slot directly.

Receiver and key snapshots use ordinary synthetic locals, with slot-only owners
in the existing evaluation ledger. This keeps their loads borrowed while the
ledger owns the slots, preventing call cleanup from consuming an OwnedTemp load
a second time. All paths retire the entry alias before its key and receiver.

Final focused checks pass16 WeakMap cases with clean native heaps, all24
reference-cell ownership cases, all7 prelude parity tests and the parser origin
regression. Fourteen native ARM64 cases pass; all4 increment/static/string cases
pass again after ordinary increment lowering is reused. The expanded closure,
parenthesized-call, literal-closure and container-element receiver test matches
PHP2:3:4:5 on native x86_64 and ARM64. The full five-target EIR fixture passes.
The final compiler/library/exporter build is warning-free, and generated docs,
registry/site/EIR, Rustdoc and whitespace audits pass without generated drift.
