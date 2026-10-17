# PR #1822 review follow-up

## Current review gate — runtime array callables

- [x] Reproduce the three latest reviews against head `6e6a8b3d3745a38097d202039e06889e3221a435`.
- [x] Validate false/null value arguments in both direct and descriptor invocation paths.
- [x] Preserve declaration names, wider overrides, and source-box cleanup on rejection.
- [x] Correct folding test descriptions and the two assembly intent headings.
- [x] Finish focused optimizer-on/off, heap, Linux execution and all-target assembly checks.
- [x] Run Trail and hygiene checks and prepare the existing upstream PR update.
- [ ] Verify the exact published SHA and resolve the three addressed threads.
- [ ] Confirm full CI completion on that head.

The sections below retain historical evidence. Current diagnostics and temporary
artifacts stay inside the canonical Elephc project; historical `/private/tmp`
paths no longer contain the earlier evidence.

## Previous review rounds

- [x] Freeze the reviewed head and inspect all review threads and CI.
- [x] Reproduce the four earlier findings against PHP and the starting compiler.
- [x] Guard false arguments on instance, static, nullable-receiver, and parent calls.
- [x] Preserve null-only property declarations separately from void.
- [x] Diagnose unsupported true-only properties, including promoted properties.
- [x] Name the actual object class in property and argument TypeErrors.
- [x] Protect partially constructed receivers on exceptions and model parameter guards.
- [x] Complete regression, parser, error, optimizer, and target checks after the last code edit.
- [x] Publish the follow-up on the existing branch and inspect exact-SHA CI.
- [x] Consolidate the verified review response and resolve the four treated threads.

## Provenance

Reviewed head: `889c6c55770f9480ee8d56e875b0b135e882ac6b`.
Observed base: `4fdabfb8b1cd0b3bc58e789c846cf5970e86490e`.
Branch: `feat/generics-optimizations`; worktree was clean at entry.
All existing head CI checks had succeeded. Three older threads were resolved;
the four new threads are the current correction scope.

## Changes and decisions

False parameter guards consume declared signatures before ABI bool coercion.
The same mixed tag/payload check is used for instance, static, nullable receiver,
and parent constructor calls. Error composition uses the runtime class-name table.
False property diagnostics retain the declared name `false` rather than `bool`.

Null-only properties retain an explicit named null annotation, which resolves to
the existing null runtime representation. Void declarations remain invalid.
True-only property storage is not implemented: the review explicitly permits
rejecting these declarations until the correct guard exists. Ordinary, promoted,
nullable, and union declarations therefore receive an explicit diagnostic.

Late argument guards can throw even for pure method bodies. Both conservative
effects and the exact exception-flow summaries must retain that TypeError, or a
catch can be pruned. Constructor validation happens before allocation; real
constructor failure uses the existing unconstructed-object ownership guard,
which releases fields and suppresses premature destructor invocation.

## Evidence

The initial follow-up exposed pruned static catches and leaked failed constructors.
Temporary logs from the interrupted session were cleared; the final evidence was
regenerated from the unchanged source before publication.

Fresh checks: six new heap-debug regressions pass with optimization on and off;
37 optimizer-effect tests, seven EIR-effect tests, one parser test, two diagnostic
tests, and four existing generic-method/variadic regressions pass. The compiler
build is warning-free. Earlier same-source generics runs passed 199 tests in each
optimization mode; these are separate from the fresh focused results.

All six new PHP fixtures also match PHP on Linux ARM64 and Linux x86_64, on/off:
24 Docker executions with clean heaps. x86_64 executes under QEMU on the ARM64
host. Only PHP's argument-error caller-location suffix is normalized; complete
property values and object-variable arrays are compared. A probe covering the
four fixes emits and assembles on all five supported targets, on/off: ten checks.
iOS evidence is compilation/assembly rather than device execution.

The example matches PHP on/off with clean heaps. Its first version exposed the
separate unsupported `false` declaration-default path; it now initializes the
properties in its constructor. That limitation is documented, and no unrelated
default-value implementation is added here. Assembly-comment alignment and
`git diff --check` pass. No full local suite was run.

## Publication

Code commit: `15b977c4e17d5cf7354f970a4f46f4b03996c094`, pushed to the existing
`origin/feat/generics-optimizations` branch. GitHub confirmed the exact head;
the PR is mergeable and its required CI checks were queued/running, with no
reported failure at inspection. Full CI completion remains pending.

Verified response: https://github.com/illegalstudio/elephc/pull/1822#issuecomment-6149367830.
All seven threads are resolved: the three older threads plus these four fixes.
This subsequent plan-only update does not alter the validated compiler source.

## Second review round — 2026-10-13

Reviewed head: `2d14195d242ec585960867c5df836b8f2181b874`.
The worktree was clean. GitHub was refreshed before publication: the same five
new open threads were present; seven previous threads remained resolved.

- Promoted `null`: preserve the named-null declaration exception in parameter
  checking, allow boxed value arguments at the user method boundary, check their
  runtime tag before allocation/coercion, and pass a canonical null ABI word.
  `void` declarations and by-reference mixed storage keep their existing gates.
- Interface `false`: inspect the concrete implementation before argument
  materialization. The guard uses its declaring class/signature, including
  inheritance, and does not reject wider contravariant `bool` implementations.
  Nullable receivers perform their null check first. Throwable intrinsics retain
  their existing path.
- Assembly folding: both normalization passes preserve indented `.ascii` data.
  The unit regression keeps label references canonical but distinguishes literal
  payload bytes that happen to spell those labels.
- Restore complete immediate docblocks for the CLI native fixture runner and
  mixed property guard. Document the EIR `shape()` / `has_nominal_use` approach
  as rejected history and the assembly comparison as the active mechanism.

Baseline probes reproduced the promoted-null declaration error and acceptance
of integer zero through an interface false parameter. An intermediate regression
then caught the additional mixed-to-null checker rejection; that failure is
retained in the temporary evidence rather than counted as a passing result.

Final execution evidence: 203 generics tests passed with optimization on and 203
with optimization off, including ten heap-debug review regressions (four new).
Four new PHP fixtures also match complete PHP stdout, except the existing PHP
argument-error caller-location suffix normalization, on both Linux architectures
in both modes: 16 executions, clean heaps. x86_64 runs under QEMU. All four
fixtures emit and assemble for all five supported targets, on/off: 40 checks.
iOS evidence is compile/assembly only. The updated example demonstrates promoted
null with a default and matches PHP with clean heaps on/off.

Compiler SHA-256 used for executable and target probes:
`58eab4b1ef979f1950111075b8937fa5260ef04b3510c6f60c41a459474ac25b`.
Evidence directory: `/private/tmp/elephc-1822-round2`.
Assembly-comment alignment and diff hygiene pass. No full local suite was run.
The final normalizer unit regression, three review diagnostics, and the existing
ordinary void-parameter rejection pass. The dev build is warning-free. Because
the test and dev builds produce different compiler artifacts, all 16 Linux
executions and 40 assembly checks were replayed with the final dev compiler;
the four fixtures plus the example also match PHP with clean heaps on macOS,
on/off (ten executions). Publication follows on the existing branch.


## Third review round — mixed receiver literal arguments

Reviewed head: `cf91e46213977197c408b14a3aa9c1a604355d2a`.
The generics worktree was clean. One new thread remained open:
`PRRT_kwDORs6r_s6sCqjR` / comment `4263494962`.

The PHP oracle rejects integer/string/object arguments to a promoted null
constructor when it is explicitly invoked through a mixed receiver. The
starting compiler accepted each argument, replaced it with null, and executed
the constructor body again. A counter rose from two to five instead of staying
two. The baseline is retained in `/private/tmp/elephc-1822-round3/baseline.json`.

The mixed candidate path now runs the same declared literal parameter guard as
concrete/interface calls before preparing ABI arguments. The receiver register
is preserved across guard calls. Throwable compact intrinsics retain their
existing bypass. Three heap-debug regressions cover null/false constructor
re-entry and a pure false-parameter method. They compare body counters and
stored values as well as complete error messages.

All 13 review regressions pass on/off after the source edit. Seven existing
mixed dispatch tests, the owned-argument return control and the omitted by-ref
default-argument control pass. The source change is five lines and reuses the
existing target-aware guard. Assembly comments and diff hygiene pass.
The final dev build is warning-free. Three fixtures match complete PHP stdout
(with only the argument-error caller-location suffix normalized) on macOS and
both Linux targets, on/off: six macOS plus twelve Linux executions, clean heaps.
All three probes emit and assemble on all five targets on/off (30 checks).
Linux x86_64 runs under QEMU; iOS evidence is compile/assembly only.
Final compiler SHA-256: `051470ebf8a74f70d33144b91684d7cb0dd5d43b90122d2a27f7b0308b30545f`.
Evidence directory: `/private/tmp/elephc-1822-round3`. No full local suite was run.
Publication uses the existing generics branch; full CI remains its readiness gate.

## Fourth review round — argument conversion documentation

Reviewed head: `8309f9de5072ceb1ffdb67edb23bf94c7917d383`.
One new thread remained open: `PRRT_kwDORs6r_s6sD8kG`, comment `4264031250`.
The immediate Rustdoc for `materialize_direct_call_arg_for_param` was missing.
Restore a concise description of its ABI conversion and the ordering constraint
that literal false/null guards must run first. This changes documentation only.

The reviewed head's CI completed with 184 successful checks and one skipped check,
including the aggregate Build & Test gate. The earlier eval shard timeout did
not recur on this head. Verification for this documentation-only change checks
diff hygiene and confirms that removing Rustdoc lines yields identical Rust
source to the reviewed head. No additional runtime suite is required.

## Fifth review round — deep inheritance and ordinary call lookup cost

- [x] Freeze the new upstream head and inspect all current reviews and CI.
- [x] Reproduce the deep hierarchy argument bypass against PHP.
- [x] Remove the fixed ancestor depth limit and cache parameter-only candidates.
- [x] Verify focused generics/review tests in both IR modes and all supported targets.
- [x] Rebase/squash and prepare the corrected version for fork publication.

Reviewed head: 9568ffa2e55f217becf15a8ebc524fa99f8bbd7d.
Its CI had 184 successful checks and one skipped check. Two threads remained:
PRRT_kwDORs6r_s6s04Wg (deep ancestry) and PRRT_kwDORs6r_s6s04Wn
(whole-program searches at ordinary virtual calls).

At 70 parent links, the reviewed compiler accepted true through inherited false
instance and late-static methods, while PHP threw TypeError. Traversal now
follows parents with a visited set instead of a 64-iteration cutoff.
The guard path skips ordinary signatures, caches declared literal parameter
candidates by method/staticness/argument count, and no longer resolves emitted
method bodies just to discover a guard. Runtime implementation diagnostics,
widened overrides, nullable receiver precedence and existing Mixed dispatch
remain covered.

Final focused verification: 158 generics tests and 18 review tests pass with IR
optimization on and off (176 distinct end-to-end tests, 352 executions).
The standalone harnesses also pass their 17 shared support checks.
The final dev build is warning-free. Both new fixtures match normalized PHP
stdout and have clean heaps on macOS ARM64, Linux ARM64 and Linux x86_64 in both
modes (12 executions). Linux x86_64 runs under QEMU. Both fixtures emit on all
five supported targets in both modes (20 combinations); iOS device and simulator
also pass static-library emission and assembly of program/runtime objects
(eight combinations, compile/assembly evidence only).
Assembly-comment alignment, module preambles and diff hygiene pass.
No full local suite was run.

The first in-process test run hit storage-full while writing a runtime assembly
cache, poisoning subsequent cache users; that failed log is retained separately
and is not counted as passing evidence. A standalone null method parameter is
also rejected on the reviewed baseline. The null regression therefore uses the
already supported promoted-null constructor surface; the declaration gap is
not claimed fixed here.

Trail check reports eight keyword collisions: docs/index/dispatch mention the
static signature only to skip unnecessary guard discovery, while actual checks
still select the runtime implementation; regression tests assert rejection of
the baseline bypass; block_emit's isolated compiler cache is unrelated to the
standing rule against renaming the graph cache or overwriting dirty worktrees.
The plan also recounts the failing baseline rather than reintroducing it.
These are not semantic conflicts. The learning is preserved unchanged.

Evidence directory: /private/tmp/elephc-1822-review, including frozen compiler
hashes, exact fixtures, complete logs and target-results.json.

The ordinary-call probe (129 implementations, 512 virtual call sites, IR
optimization disabled) emits byte-identical assembly before and after.
Three alternating warm trials measured medians of 198.7 ms and 193.8 ms;
this small local observation is informational, not a general speedup claim.

Rebased on origin/main 132b4da6f7. The aggregate Rust/Cargo source fingerprint
is unchanged after rebase: da739e2e32ea32972d3e2ee19c10f4d420c386a5fb348fe80727dd054c755ece.
The upstream head remains 9568ffa2e55f217becf15a8ebc524fa99f8bbd7d.

## CI maintenance, 2026-10-16

The existing head 8d88cfab09 has no open review thread in the current snapshot.
The current pull-request CI run 38787959662 has two independent failures:

- macOS non-codegen shard 7 passed 1511/1512 cases; the all-target native-default
  helper test hit the 60s timeout on both attempts (final 60.024s). That exact
  library test now gets the existing 180s all-target budget in both nextest
  profiles. The global 60s guard and the test assertions are unchanged.
- macOS eval shard 6 failed before tests because cargo-nextest.sha256 was missing
  after artifact extraction, including its retry. The artifact still exists;
  its producer workflow writes/uploads the checksum with the executable. The
  content validator correctly rejects missing/corrupt tools and checksums, so
  this check remains strict. A fresh producer/consumer run is needed; the old
  cancelled duplicate run is a separate source of stale cancelled contexts.

Rebased on current main with every Rust source identical to the reviewed head.
TOML parses, each profile has exactly one intended allowance, the global cap
is preserved, and the existing artifact validation script passes its valid,
corrupt, missing-binary, missing-checksum and invalid-version fixtures. No
compiler source changed, so no broad local Rust suite was rerun. Full fresh-SHA
CI remains required; the artifact extraction failure is not claimed resolved
until that run validates its downloaded bundle.


## Sixth review round — runtime array callables

- [x] Freeze current upstream head and collect all three open review threads.
- [x] Rebase the existing PR worktree on current `origin/main`.
- [x] Reproduce array-callable literal parameter bypasses before repairing them.
- [x] Validate declared parameters before runtime array-callable ABI conversion.
- [x] Correct emitted-body folding test documentation and assembly group headings.
- [x] Run focused functional, heap, optimizer and supported-target checks.
- [x] Run Trail/hygiene checks and prepare the squashed existing upstream PR update.
- [x] Resolve addressed threads against the exact verified published head (`b45db3b04a`).
- [ ] Verify full CI completion on that head.

Reviewed head: `6e6a8b3d3745a38097d202039e06889e3221a435`.
Open threads: `PRRT_kwDORs6r_s6tH9zz` (P1 runtime array-callable null arguments),
`PRRT_kwDORs6r_s6tH9z-` (folding test Rustdoc), and
`PRRT_kwDORs6r_s6tH90G` (receiver and late-static assembly intent headings).

`emit_runtime_array_instance_method_call` already has the selected implementation
and its source-visible signature, but materializes arguments without the literal
parameter guard used by ordinary and mixed-receiver methods. The null ABI
conversion assumes that guard has run. Keep validation before loading the
receiver register and before argument conversion so rejection cannot execute the
constructor body or overwrite a property. Use the selected implementation's own
signature, preserving inherited diagnostics and wider overrides.

All worktrees, probes, build helpers, compiler artifacts, and temporary files now
stay inside the canonical Elephc project. Historical temporary evidence paths
above are records of earlier runs; those directories are no longer available.
Current evidence lives in `.claude/worktrees/.maintenance/elephc-pr-maintenance-20261015/`.


All three new heap regressions fail against the rebuilt pre-fix compiler. The
null case executes the constructor for false, true, integer, string and object
arguments; the false case produces property errors or executes after coercion;
the pure false method accepts true and integer zero. PHP rejects these arguments.
Typed-receiver array controls already match PHP and have clean heaps, isolating
the defect to runtime mixed-array selection. Baseline logs are retained as failed
evidence rather than included in passing counts.

The repair calls the shared literal guard using the selected source-visible
signature before loading the receiver register. It reuses the existing ABI-aware
argument checks and implementation-name diagnostics. No general argument
materialization rule or callable signature is changed.


The direct-array check alone did not cover descriptor-generated invokers. Their
shared indexed/associative decoder now validates declared literal value parameters
before coercion. The diagnostic reads the current descriptor name because bodies
can share a signature; static dispatch lookup still retains receiver/called-class
identity while error names retain the declaring implementation.

Null coercion also exposed the older fresh-box lease leak: Void was absent from
the coercion eligibility check. Recognizing it must also handle its zero-slot
result in temporary-source release; the ordinary release helper assumes a pushed
result. The final null probe matches PHP with114 allocations and114 releases.
Intermediate leaks and the first zero-slot refcount failure remain failed evidence.

Linux x86_64 execution exposed that its string pointer and integer object result
both use rax. Dynamic TypeError construction now restores the diagnostic into
separate ABI scratch registers, preserving the allocated exception object. This
is validated by execution, independently of successful assembly. The standalone
runtime test helper also includes the same handler/clone helpers requested by
emitted builtin candidates; missing-helper link failures are harness evidence,
not suppressed or counted as passing tests.


## Final sixth-round verification

The final warning-free compiler/library build passes25 literal-contract review
cases,95 existing callable controls and3 folding controls with optimization on
and off:123 distinct focused cases and246 executions, excluding shared support
tests. The final seven PHP witnesses match complete normalized PHP stdout and
have clean heaps on macOS ARM64 and both Linux architectures in both IR modes:
42 native-program executions. Linux uses the existing Alpine/musl images;
x86_64 executes under Docker emulation on the ARM64 host. Only PHP's existing
argument-error caller-location suffix is normalized.

All seven fixtures emit and assemble program AND shared-runtime objects for all
five supported targets in both modes:70 combinations and140 objects. iOS is
compile/assembly evidence, not device execution. The crypto bridge used by
descriptor support was freshly built from this source for each Linux architecture;
no foreign compiler or stale bridge archive was used for those executions.

The final compiler SHA-256 is `5e468fd5fdcf32caf9df6e3ffee1967889b2ad48b77d73ecb9923f5550cc435e`.
All current compiler snapshots, fixtures, complete JSON results and failed
intermediate logs remain under the project's ignored maintenance directory.
Assembly instruction comments and diff hygiene pass. No full local suite was run.
Fresh exact-head CI remains the complete readiness gate after publication.


Twenty current Trail keyword matches are contextual. Both direct and descriptor
paths receive checks, and runtime signatures preserve wider implementations.
Historical notes describe rejected baseline behavior and the earlier publication
correction. The current worktree is internal, and publication updates the original
PR's existing upstream branch as explicitly requested by the user. Synthetic AST
declarations, test parent registrations, unrelated worktrees, and the Trail graph
cache are preserved. Native on/off and all-target evidence above verifies the
actual implementation; immutable learnings remain unchanged.


## Seventh review round — timeout suffixes and meaningful folding assertions

Reviewed head: `b45db3b04ac2a180dbcd9292dddf6446890ad6c3`.
Two new threads: `PRRT_kwDORs6r_s6tMEk9` (Nextest suffix mismatch) and
`PRRT_kwDORs6r_s6tMElI` (static symbols used for instance methods).
The worktree was clean and already rebased on current main `d354273274`.

Both default/CI profiles now match `_on_(all_targets|every_target)$`, retaining
the scoped 180-second budget and the unrelated 60-second global guard. TOML
parsing, timeout values and suffix controls pass. Official Nextest 0.9.148,
downloaded and checksum-verified inside Elephc, validates the extracted matching
profile settings against two positive and four negative fixture names in both
profiles. A first attempt to load the entire repository config into the tiny
fixture correctly rejected its missing unrelated binary names; that harness
failure remains separate from the final scoped filter evidence.

The old `touch` assertion searched static method symbols, so it passed without
examining the real instance entries. Correcting those names also exposes a stale
transitive-fold expectation: these bodies call distinct nominal helper entries.
Their helpers can share emitted code without making the callers byte-identical.
Keep that fixture as a non-folding control with both real entries present and
neither a thunk. Add an inherited, non-generic static helper fixture where both
instance methods call the same implementation; both entries must exist and one
must jump to the other. No transitive callee canonicalization is introduced.

All 21 folding regressions pass with optimization on and off: 42 executions.
Both fixtures also emit and assemble on all five supported targets in both modes:
20 program objects with explicit instance-entry and sharing assertions. macOS
and Linux executable emission shares the positive case; iOS staticlib emission
intentionally keeps both bodies under the existing library-mode exclusion.
The first target harness incorrectly expected executable folding in a library;
its failed assertion is retained, and the final harness checks the actual mode.
iOS evidence is compilation/assembly only. No full local suite was run.

Production Rust/Cargo sources are unchanged from the reviewed head. The reused
compiler remains SHA-256 `5e468fd5fdcf32caf9df6e3ffee1967889b2ad48b77d73ecb9923f5550cc435e`;
the updated original integration module was rebuilt against its matching root
library. All scratch crates, tools, probes and logs stay inside Elephc, under
`.claude/worktrees/.maintenance/elephc-pr-maintenance-20261015/pr1822-round7/`.
The existing PR branch remains the publication destination; the complete CI
matrix on the final published SHA remains the readiness gate.


The current Trail check reports 20 contextual keyword matches. Historical plan
entries describe rejected baselines and the corrected publication destination;
README/CLI docs describe emitted-code reports rather than vacuous test assertions.
The active guard docs cover both invocation paths and runtime signatures.
Synthetic declaration and test-parent wording refers to unrelated, preserved
code. The new learning's explanation of a conservative non-folding control is
not a prohibition on conservative folding, and the literal-method index is not
an assertion about static symbols. Runtime compiler caches remain separate from
the preserved Trail graph cache. No immutable learning or unrelated source is
rewritten to silence these matches; the new positive/negative tests exercise the
actual symbol and body contracts.
