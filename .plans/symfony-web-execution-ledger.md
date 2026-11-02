# Framework compilation: Symfony, Symfony Demo and Laravel

Last consolidated: 2026-10-21. This is the single current-status entry point for
the framework campaign. Update the relevant section instead of appending session
narration or creating another checkpoint plan. Logs, probes and review outputs
belong under `target/`; historical notes are read-only archives.

## Acceptance checklist

- [x] Capture the stock application's native PHP/Xdebug request baseline.
- [x] Preserve typed source snapshots before include folding and declaration stripping.
- [x] Share actual include-once state between native and dynamic execution.
- [x] Serve the actual default Welcome 404 through the standard Symfony --web entry
  on macOS ARM64, including 500 sequential full-content comparisons with PHP.
- [ ] Separate all compiled declaration metadata from request-active bindings.
- [ ] Execute known compiled source units through native entries, preserving caller
  scope, references, return values and exception propagation.
- [ ] Compile Symfony, Symfony Demo and Laravel through their standard `--web`
  entry points and validate every available route against native PHP.
- [ ] Compile both Symfony `bin/console` entries and Laravel `artisan` without
  errors or warnings; validate the entire command surface and file side effects.
- [ ] Remove all compile warnings from both `--web` and CLI console builds; do not
  suppress them or alter Symfony/vendor sources.
- [ ] Validate repeated requests, ownership/cleanup, autoload and target behavior.
- [ ] Meet the build-time objective of approximately 1–2 minutes or less and the
  final executable-size limit of 250 MiB. Measure throughout implementation.
- [ ] Compare rendering performance with native PHP with OPcache enabled through
  interleaved runs, both at idle and under concurrent workloads.
- [ ] Optimize compilation under load as an acceptance condition: reduce total CPU
  work, memory footprint and output volume, and compare bounded codegen/assembler
  parallelism without stopping unrelated jobs.
- [ ] Complete independent implementation audits, resolve findings to the requested
  consensus, and obtain the final Kimi K3 audit.

Do not modify `examples/symfony-app`, vendor or generated framework sources to make
tests pass. No framework/package-manager/layout special cases or semantic stubs.
Compare encountered behavior with PHP/php-src, add regressions, and implement the
standard behavior through AST/EIR/native code. Reduced tests are not HTTP acceptance.

## Active specifications

- [Native/dynamic inclusion specification](native-eval-inclusion-spec.md), frozen
  revision 3, SHA-256 `459e29a0c61812134bd5fd53565f56f6dccce0290509874f1af122134353883a`.
- [Exact-hash consensus record](native-eval-inclusion-consensus.md): root, GLM 5.2,
  Kimi K2.7 and Kimi K3 locked the contract. The frozen spec's old header is retained
  deliberately; the separate consensus record is authoritative. This is not a
  completed-code audit or proof of execution.
- [Class alias draft](20260908-native-class-alias-spec.md): still open and unlocked.
  Keep alias identity semantics in that draft; do not implement aliases as subclasses.

## Latest end-to-end evidence

The presentation is planned for the end of November 2026. Acceptance remains full
framework behavior; a prepared demonstration is not a substitute for passing the
route, console, warning, ownership and resource gates.

The shared worktree is on `reconcile/dirname-symfony`, HEAD `befb1dbdc8`, with a
pending merge of `cf039847e9`. The index has no unmerged paths, but thousands of
staged and unstaged changes remain. Preserve that boundary and do not infer a
committed-tree result from a scratch compiler. Claude's referenced session ended
on 2026-10-20 after its API quota was exhausted; its last workers left partial
reflection, callable and code-size work. Other worktrees still have live tests.

Rechecked 2026-10-20, using the existing artifacts under
`/Users/guillaumeloulier/PhpstormProjects/oss/elephc-scratch/`:

| Surface | Compilation evidence | Execution evidence / open gate |
| --- | --- | --- |
| Symfony web, `sfweb/fs23/index` | Exit 0; 240.18 s wall; 4,961,009,664 bytes maximum RSS; 2,161,675,600 bytes peak footprint; 215,098,360-byte executable; 1,194,082,187-byte assembly; 604 warning lines | Fresh route comparison: 7/8 responses byte-identical to PHP. Both return 404 for the missing route, but PHP returns 194,072 bytes and Elephc 993 bytes. Other tested GET/POST responses match. |
| Demo console, `console-builds/e207` | Exit 0; 692.79 s wall; 16,301,572,096 bytes maximum RSS; 11,338,628,528 bytes peak footprint; 1,423 warning lines | The inherited e206 command comparison reported 90/106; this is not proof for e207 or current source. The later diagnostic entry `probe208b` fails to compile `ReflectionReference::__construct`; it is a reducer, not stock-entry acceptance. |
| Laravel web, `lvweb/fr8` | Exit 1 after 234.83 s; 1,670 warning lines | Backend rejects `ReflectionClass constant metadata array element`. Current-source retry and console acceptance remain open. |

The warning counts above count PHP/compiler lines in the preserved logs; they do
not establish a clean Rust compiler build. No controlled performance comparison
with PHP/OPcache has been repeated on the current tree.

The latest inherited reflection batches completed with 563 passed / 20 failed
in table mode and 576 passed / 7 failed without it. These runs identify open
regressions, not an accepted baseline. Their logs are `refl-on.txt` and
`refl-off.txt` in the referenced Claude session's scratchpad.

A generic reducer reproduces the enum-array metadata failure on frozen d225 and
the freshly rebuilt reference compiler: `tests/fixtures/reflection_enum_array_defaults/main.php`.
The candidate now supports enum leaves inside indexed and associative reflected
constant/default arrays. Its closed-world output matches `php -n` (three identity
reports and `safe:safe`), but the initial ownership check still found two enum
objects live at exit. A separate runtime-name lookup reduction isolates one excess
reference per dynamic `constant()` call: both target emitters explicitly retain a
borrowed enum and then box it with a helper that already retains its child. Removing
that duplicate retain fixes the leak: `enum-focused-round5.log` records three passes
(closed-world reflected arrays, repeated dynamic lookups in both modes, typed reads
and shutdown callback), all with zero live blocks. The fourth test remains red for
the independent default-mode metadata failure below.

The default-mode runtime-name reflection path remains separately broken:
`reflected_enum_case_arrays_match_php` throws `Failed to retrieve the default value`.
The compiled default initializer exists, but the bridge's parameter metadata reads
only value defaults, not compiled initializers. Literal-class reflection succeeds.
`closed_world_reflected_enum_case_arrays_match_php` preserves an independent gate
for the native table path; splitting the tests does not waive the default-mode
failure. Direct enum access/typed argument/shutdown callback cleanup already passes
`enum_case_roots_are_released_at_shutdown` with zero live blocks, and the related
interface/nested-trait table closure passes
`literal_table_reflection_retains_related_classlikes` in default and closed modes.

Do not broadly reroute runtime class names to the native table solely because no
PHP is loaded at runtime: aliases and conditional activation also affect class
identity/existence. The current table constructor alone does not prove those
semantics. No such routing change has been made.

Two further ReflectionParameter constructor gaps were measured: runtime method names
are rejected by the checker; literal method selectors in a closed-world build are
rewritten into three legacy metadata operands for a two-parameter table constructor.
The extra operand is NOT a hidden argument count. Both regressions were red before
the patch (`parameter-pair-before.log`). An attempted rerouting of the PHP callable
pair into the table constructor was invalid: it accepts only an internal row, and
execution crashed after the feature gate was widened. Those lowering/checker/gate
changes have been withdrawn. The candidate instead limits the row constructor to
synthetic Reflection bodies and retains the native metadata materializer for public
calls. The literal/named constructor regression passes on the frozen round-8 compiler
(`parameter-pair-after-round8.log`, 1 passed, PHP stdout and native exit checked).
Runtime method names remain
unsupported and their test stays red. `parameter-pair-lldb.log` preserves the
invalid-route crash, not a current pass. The native compiler rebuild in rounds 6
and 7 has no Rust warnings; warning-free framework compilation remains separate
and open. Their reduced sources/logs are retained alongside the enum probes.
The current rebuild and all new logs are isolated under
`target/framework-audit-20261020.Wcomr7/`.

Round-8 provenance: `candidate-compiler-round8.sha256`, compiler source manifest
`source-state-round8.json`, and `test-source-round8.sha256`. Only assembly-comment
coverage/alignment was changed after freezing that compiler; its runtime behavior
is unchanged. The focused enum fixture emits for all five supported targets (iOS
requires `--emit staticlib`); Linux ARM64 and x86_64 assembly both assemble to ELF
objects with Clang. These are emission/assembly checks, not cross-target execution.
The complete `reflection_table_tests` module finished sequentially against the same
compiler (`reflection-table-round8.log`): 18 passed, 2 failed, none ignored, 392.83 s.
Only the known default-metadata and runtime-method-name cases fail. Neighboring
method/parameter, property, constant, closure, subclass, error, flat-code-size and
heap-stability tests pass. This is not the full compiler suite or framework acceptance.

A Laravel diagnostic build in `laravel-current-web/` used the older round-4
unoptimized compiler before the final boxing/constructor fixes. At the 2026-10-21
resumption its process and handle were gone, with no executable or terminal exit
marker. Keep it as incomplete diagnostic evidence, not compilation acceptance.
The first sampled stack (`compiler.sample.txt`) was in type checking. A later
sample (`compiler-stage2.sample.txt`) spent 71/81 samples in validator dominators,
mostly bitset intersections. The type-check retry loop already batches its errors;
do not assume an old collect-all environment switch removes either bottleneck.
The historical collect-all switch was not found by the current CLI/help and
environment-literal searches; its continued existence is unverified.

The optimized iteration compiler completed: `compiler-opt1-build.log` (278.85 s,
4,978,409,472-byte maximum RSS), frozen as `candidate-compiler-opt1/`; its three CLI
smoke tests passed. Command-line Cargo override only, no repository configuration
change: `profile.dev.package.elephc.opt-level=1`, one job, no incremental compilation,
lowered scheduling priority. This is a compiler rebuild, not application build time.

The validator now processes dominator sets in reachable reverse postorder, derived
from the SAME predecessor graph including implicit exception edges. Numeric block-id
order required 255 sweeps on a reversed 256-block chain (red regression). The
candidate needs two. All 19 validator tests and 15 mem2reg neighbors pass; an
independent set-based oracle checks 512 generated CFGs, including dead islands,
nonzero entries, cycles and exception edges. The manual benchmark is separately
run, not waived: three interleaved 2,048-block trials go from 2,047 to two sweeps,
median internal time 1,432,381 us to 3,776 us. Process instruction counts fall from
about 29.1 billion to 93.1 million; RSS stays near 12.5–12.7 MB. This is not an
application-wide speedup or a fix for quadratic dominator storage. Evidence:
`validator-scheduling-before.log`, `validator-scheduling-after.log`,
`validator-mem2reg-neighbors.log`, `validator-paired-*.log`, and the two frozen
benchmark binaries/hashes. No validator checks were disabled.

The rebuilt RPO compiler is frozen in `candidate-compiler-rpo/`, with hashes in
`candidate-compiler-rpo.sha256` and `source-state-rpo.json`. Its CLI enum-array
smoke test passes. The first fresh Symfony web attempt (`symfony-web-rpo/build.log`)
ran 124.47 s, with 604 warnings, and stopped on the missing locked PCRE2 recipe-11
artifact in the selected native cache. The exact locked dependency was installed
(`native-install-r11.log`, 7.86 s); manifest/lock hashes are unchanged. The retry
(`symfony-web-rpo-linked/build.log`) ran 433.51 s with the same 604 warnings, then
failed on a Mach-O ARM64 branch out of range in `__rt_source_include_lookup`. Its
assembly is 1,194,114,709 bytes. No executable was produced.

The assembler cause is now isolated: the explicit one-worker setting bypassed the
existing size cap, compacting, encoding and object cache, creating one oversized
Mach-O atom. Separately, the previous code spawned one worker per size-required
slice, even when that exceeded the requested worker count. The candidate keeps
small one-worker inputs unchanged, slices large inputs at the existing size cap,
and schedules slices through a bounded pool while preserving link order and panic/
write-error outcomes. All 99 linker tests pass (two manual probes separately
ignored). The actual 1.19 GB source was then processed by the manual probe: six
slices, ONE worker, 153.53 s, no assembly error, max RSS 2,359,345,152 bytes
(`assembly-single-worker-probe/probe.log`). This only reassembles; link/runtime
acceptance is still pending. The existing splitter remains specific to macOS ARM64
executables; other target/artifact fallbacks were not changed by this fix.

The runtime archives copied beside the earlier debug compiler were unoptimized.
Fresh release archives for the ten referenced bridges have now been rebuilt from
current source (`optimized-runtime-bridges-build.log`, 149.63 s, exit 0). Use them
with the next frozen compiler instead of attributing debug-runtime binary size to
production output. Do not assume an old copied archive is fresh merely because
copying updated its mtime.

The 604-warning census is 326 unused-variable, 199 local-retype, 16 unreachable,
61 tentative-return notices, and two non-iterable-foreach warnings. PHP 8.5.10 and pinned php-src
`Zend/zend_inheritance.c` confirm that inherited source contracts are not checked
again for notices, and a new interface consults the actual inherited declaration's
attributes. The checker now follows that declaration for notices, retains existing
hard-error validation locations, and emits a notice once under its real owner.
Three new cases were red before (`tentative-inheritance-before.log`); all five
focused cases now pass, including the userland hard-return negative control.
Interface neighbors pass 35/35. Iterator neighbors pass 20 with three failures:
the expected static constructor errors for invalid RegexIterator/recursive inputs
are absent. The pre-change frozen CLI also accepts their combined reproducer
(`iterator-frozen-baseline-check.log`); these remain open, not waived. Runtime
negative behavior of those invalid calls has not been checked here.

The new optimized compiler build combining the bounded assembler and notice fixes
is in `compiler-bounded-assembly-build.log` (901.65 s under host contention, no Rust
warnings). It is frozen in `candidate-compiler-bounded/` with the ten freshly built
release bridge archives; hashes and source inputs are recorded in
`candidate-compiler-bounded.sha256` and `source-state-bounded.json`. All 5,385 input
hashes were rechecked before the console attempt, with no changes.

The standard Symfony web entry now compiles successfully (`symfony-web-bounded/`):
353.24 s wall, 5,365,989,376-byte maximum RSS, 2,298,794,640-byte peak footprint,
215,032,552-byte executable. Six assembly slices use one worker and all six object
cache entries; runtime cache is cold. This is not yet the 1–2 minute performance
target. There are 543 warnings (all 61 false tentative-return notices are gone),
not warning-free acceptance. Three fresh native GET / requests return HTTP 200 and
the same 695 bytes/SHA-256 as PHP (`native-bounded-home-prod/response.json`). This
fixture renders a custom Twig home, not the route-less Welcome page.

The console acceptance campaign uses separate APFS-cloned application copies
`console-isolated.zi471L/` (native) and `console-oracle.1VqPCG/` (PHP). No inherited
generated cache is copied; PHP warms each copy's own paths. A Seatbelt profile
allows writes only within that executor's application and /dev/null; its positive
and negative canary yielded [7,false,false]. The source bin/console and vendor
files are unchanged. Native compilation uses the standard entry with the generated
container preload selected through the ordinary INI option, one assembler worker,
and a 256 MiB runtime heap. `probe-console.py` executes all 33 advertised commands,
including cache and secrets operations and all default error-page outputs, rather
than treating help as execution. PHP completed all 33 with exit zero; error:dump
writes 40 HTML pages and secrets:reveal returns the known disposable value on
stderr (normal console output-channel behavior). The original fixture's 2,127
recorded input hashes remain unchanged.

Native console compilation succeeds: 379.84 s, 4,178,640,896-byte maximum RSS,
1,686,638,312-byte peak footprint, 174,410,344-byte executable, five slices/one
worker, no assembly cache hits, 456 warnings. The first 33-command matrix is NOT
acceptance: debug:config exits 1 with a generic Magician Match-expression failure;
secrets:reveal exits 2; secrets:encrypt-from-local exits 1; cache:clear crashes with
SIGSEGV. Further exit-zero failures include secrets:set failing to read its stdin,
secrets:decrypt-to-local reporting a pointer-sized count and a type error, and
secrets:remove failing to find the supposedly created secret. debug:twig omits
the yaml_dump callable's parameter names. The advertised command list is complete,
but command output/side effects still require comparison, not merely exit checks.
Evidence: `console-isolated.zi471L/native-matrix/` versus the separate PHP matrix.

The cache:clear crash is localized by `logs/cache-clear-lldb-detail.log` inside the
native copy: __rt_pcntl_dispatch_pending treats C int -1 (zero-extended to
0x00000000ffffffff) as a positive 64-bit queue result, reads uninitialized siginfo,
and indexes the handler array with a stack address. Both ARM64 and x86_64 emitters
have this signed-width error. The emitters now compare the C int status at its
actual width (w0/eax). Both regressions failed before: the actual C callback probe
died by signal and the target-emission test found the wide compare. Both now pass
(`pcntl-status-before.log`, `pcntl-status-after.log`): the native probe exercises
error, empty, record and reentrant cases with mask/guard checks, and emission checks
cover all five targets. Native Linux execution remains for its native runner; this
host executed macOS ARM64. Assembly comment and scoped diff hygiene checks pass.
The optimized CLI rebuild completed in 10m34 under load, without Rust warnings
(`compiler-pcntl-status-build.log`); the frozen compiler is
`candidate-compiler-pcntl/` (SHA-256 c0b964119f94c4198c868f084b6f4d29ebfb074d0a59218621781d5efd2255cb),
with source manifest `source-state-pcntl.json`. Console recompile succeeds in
295.61 s, 4,252,614,656-byte max RSS, same 174,410,344-byte output size, five slice
cache hits, 456 warnings (`logs/build-pcntl.log` in the native copy). This is NOT
a controlled speedup measurement against the cold-cache build.
cache:clear still segfaults, but at a DIFFERENT site: the PCNTL boundary is passed;
now a mixed array read dereferences ASCII-looking payload 0x7474682e656d6974
(`logs/cache-clear-pcntl-lldb.log`). Read-only relocation-free text-anchor matching
against the cached objects verifies 24 independent anchors and attributes its
caller to PhpDumper::collectCircularReferences, rather than relying on a guessed
symbol slide (`map-native-pc.js`, cached object asmslice-v0.27.1-a951ca6b4bf91a172.o).
The call at object offset 0xa5a050 is confirmed by its relocation as
__rt_array_ensure_elem_for_write. A framework-independent reducer now reproduces
the relevant nested-reference mutation: a local ['a', []] is updated at [1][$key]
and appended by reference to a shared result on each loop iteration. PHP 8.5.10
preserves both keys in both shared paths; the frozen compiler produces null at
index 1. With --heap-debug it fails with "bad refcount". Adding trace output to
the recursive graph reducer instead triggers SIGSEGV during its second append.
Evidence: reference-nested-update.php, reference-nested-update-build/,
reference-nested-update-heap-build/, and recursive-path-trace-build/ in the audit
directory. A basic escaped-reference/destructuring probe passes, so neither
destructuring alone nor all reference appends are broken. The likely defect is
the consuming mutation's early boxed-owner release: it does not clear the cell,
while a loop back-edge can promote the same slot and make storeback retire that
already released value again. Focused regressions are being established before
reusing the existing representation-aware deferred retirement. All three native
regressions fail against the old library (12m14 rebuild under load, 5.58 s test
execution; reference-loop-before.log): indexed and associative/dynamic paths lose
their nested arrays, and heap-debug detects the invalid refcount. Both consuming
local-mutation paths now reuse deferred slot retirement, with a five-target EIR
regression and PHP differential/native/heap-debug regressions; validation of the
changed compiler is running serially. reference-loop-after.log stopped on a raw
string placement error in the new cfg(test) module, before executing its tests;
that test-edit error is corrected and rustfmt parse-only checks pass. The fresh
attempt reference-loop-after-dev.log uses the unoptimized development profile
for faster focused TDD. It passes: one five-target EIR test and all four native
regressions, with PHP differential checking; heap-debug is clean on the indexed
ownership case and the recursive walk returns the expected paths (4m52 build,
6.29 s native test execution). Both loop-cleanup EIR neighbor tests also pass,
and all 66 codegen::references tests pass with PHP checking (89.30 s;
reference-loop-unit-neighbors.log and reference-loop-neighbors-after.log).
Reserve the optimized compiler for measured application
builds rather than paying its full crate rebuild on each tiny reducer edit.
The cause of the
full console crash is not closed until the rebuilt command succeeds.
The frozen development compiler candidate-compiler-reference-loop-dev/ (SHA-256
a6e6d39efff57cd9155248c864934dc9b34e611fbec405d33d6153279eb5b02c) also passes
the standalone CLI probes. The nested indexed, associative-parent and dynamically
typed-key cases now match PHP and all three report zero live heap blocks. Source
inputs are pinned in source-state-reference-loop-dev.json; only context.rs and
the reference-loop unit module differ from the previous compiler source manifest.
Direct appends and direct string-key writes still return PHP-equivalent values
but leave respectively 4 blocks/256 bytes and 7 blocks/1360 bytes at exit. These
separate ownership leftovers are confirmed unfixed after this repair, not assumed
clean. Evidence: reference-mutations-before/ and reference-mutations-after-dev/.
Their next ownership hypothesis is backend container writeback: ArrayPush and
HashSet keep the previous slot occupant on the assumption that the mutator already
consumed it. An unboxed array retained from Mixed storage and its old Mixed box are
distinct owners; store_container_writeback_to_local in src/codegen/context.rs does
not currently distinguish that case. Audit dynamic raw/ref-cell paths and destructor
reentrancy before changing this policy; pre-clearing a reference observed by a
destructor is not automatically equivalent to publishing the replacement first.
The first destructor-observability probe, reference-loop-destructor.php, is blocked
earlier by an invalid hash allocation request (roughly 30 GB, pointer-sized value).
Both frozen pcntl and reference-loop-dev compilers reproduce it with the same
253 allocations / 42 frees checkpoint and only about 36 KB live on an 8 MiB bounded
heap. The allocator rejects the request; it does not allocate 30 GB of host memory.
This is a separate pre-existing representation defect, not proof of destructor
parity and not a regression introduced by this repair. Keep it scheduled with the
other ownership/representation probes instead of discarding this measured failure.

The optimized compiler rebuild is successful (4m35 under a different, lighter host
load; no speedup claim), frozen as candidate-compiler-reference-loop/ with SHA-256
2a42968fd44eca035886dad2e3f7b74f82290b597da435c64d0d8197b9f9c0b7. Its 5,386-input
source digest matches the development compiler, and its standalone mutation probes
confirm the same three clean nested cases and the same two still-leaking simple
cases (reference-mutations-after-opt/). A fresh real console build is running in
console-isolated.zi471L/build-reference-loop/; log logs/build-reference-loop.log.
It succeeds in 175.57 s, 4,147,707,904-byte max RSS, 1,842,760,728-byte peak
footprint, 449 warnings, five slices/one worker and zero slice cache hits. The
executable is still 174,410,344 bytes (SHA-256
ecaf608e7d6f5ea85f8c68615c8a19b659a5e57e998554f75807ce68418a3ad5). This is an
instrumented run under a different load, not a controlled speedup or proof that
seven warnings were fixed. Two ten-second sample windows are retained as
compiler-profile-refloop-01.sample.txt and -02.sample.txt: the former observes
EIR optimization/validation/hash-map insertion, the latter slice rendering/ARM64
encoding and waiting on the assembler. The phase table reports 19.50 s optimizing
EIR, 33.00 s codegen and 67.63 s assembling. A third sample attempt was too late
because the process had already completed; no third capture is claimed.

The rebuilt real cache:clear NO LONGER SEGFAULTS. It now exits 1 after 2.626 s with
"unsupported Match expression" from dynamic execution of http-kernel/Log/Logger.php
line 62 (native-reference-loop-cache-clear/). The nested-array crash is passed,
not the command acceptance gate. Investigate whether this logging failure masks
an earlier exception, and keep the command non-green until its normal behavior
matches the separate PHP oracle.
The existing ELEPHC_EVAL_TRACE switch was used on the frozen executable, with
writes still confined to the disposable app. The trace has 276,377 lines / about
55 MB (native-reference-loop-cache-clear-trace/). The same dynamic Logger constructor
and match succeed initially (trace lines 631–639), then fail after warmup/reboot
(276361–276366). Immediately before the failure an ArrayAccess probe receives the
new container object while evaluating the superglobal-based match subject. Match
parsing/evaluation already exists; do not add another match implementation or
misclassify the generic outer-expression diagnostic as missing syntax.

The independent late-global-main.php / late-global-reader.php probe loads a class
from an opaque argv include inside a native function. Main assigns 2 into a native
superglobal. The loaded constructor sees 2 while the loader frame is active but
falls back to 0 after that frame returns, while native main still sees 2. PHP sees
2 on every read. Evidence: late-global-main-capture-build/; the initial variant
without a native superglobal read inside the loader already misses main's write.
This demonstrates a scope/global-view divergence, not yet the precise full-app
memory-corruption mechanism. Check native-global synchronization and retained
scope/value ownership before editing; the probe also has heap leftovers.
The permanent regression test_eval_retained_class_reads_live_superglobals_after_loader_returns
was added in tests/codegen/eval_global_scope.rs. Before repair it returns
2:2 / 0:2 / 0:3 instead of PHP's 2:2 / 2:2 / 3:3
(retained-global-class-before.log). LLDB scope-set/free tracing confirms that only
the loader installs a global scope; its scope is freed after the last local
instance is destroyed, and later native construction uses a fresh fallback context.

The module already publishes paired native/eval global-transfer hooks. The new
context helper with_native_global_sync creates an owned scope when a fallback
context has hooks but no scope, refreshes native globals before entry, and exports
changes on ordinary/error return. The constructor and method native entry paths
now use it. Re-running the unchanged native regression harness against the freshly
built runtime archive passes the previously failing test
(retained-global-class-after-bridge.log). All three hook registration, scope
ownership/reuse and error-return unit checks pass (retained-global-hook-units.log).
All seven eval_global_scope integration tests pass, including the new constructor
write/method exception regression (retained-global-all-after.log). Under a host load
average around 100, execution took 400.43s; this is not a controlled compiler
performance comparison. The optimized runtime
archive built in 2m15s. candidate-compiler-native-globals intentionally combines
the frozen reference-loop compiler with this runtime-only archive replacement;
native-global-bundle-provenance.json records both source manifests and hashes.
It is a diagnostic bundle, not a fresh full-compiler build. Do not yet claim the real command or every other native
entry family is repaired; static, magic-access/string, callable and function
siblings still require the same boundary audit and focused regressions.
The fresh diagnostic console built in 385.11s under heavy host load (4,106,797,056
bytes max RSS, 174,393,832-byte executable, 449 warnings, all five assembly slices
reused). It REGRESSES startup: cache:clear segfaults while rendering a TypeError
from the entry closure, before the earlier logger stop. The old reference-loop
binary still reaches its logger failure on the same application state. Evidence:
native-global-cache-clear/, native-global-cache-clear-trace/ and logs/cache-clear-native-globals-lldb-detail.log
under console-isolated.zi471L. Native address attribution verifies all 24 anchors
and identifies the secondary crash as VarDumper Caster::castObject invoking
__rt_object_to_hash with an invalid string representation; this is not yet a repair.
The broader native_global_boundary_matrix probe also records missing static/magic
property/string global transfers, while its native-function bounce passes.

A new constructor regression exposes an asymmetric reverse boundary: native
constructor dispatch does not export/import globals around the compiled body,
unlike registered native function dispatch. With the new outer global transfer,
that stale snapshot can overwrite the native constructor's writes. The reducer
prints two empty lines versus PHP's constructor/constructor when a runtime child
is also present (native-constructor-inherited-global-before.log); the direct-only
variant passed. A reverse helper and constructor wiring are under validation.
The inherited-native-method/error sibling has a separate permanent regression.
The paired reverse transfer now covers native constructors and instance methods.
All nine integration regressions pass against PHP (native-global-nine-after.log,
479.81s under load), and all four hook ownership/order/error-return units pass
(native-global-four-hook-units.log). The optimized archive built in 2m50s without
production warnings. Runtime source snapshot: 7272cee8c06307dc4a8fb07dcf3ca55f097d43cb705b3247588643eaa8ae4eb0;
archive SHA-256: 00ab5dde061081803cfa0e8c2e89c01b93d2c4f6796f4094af4c40eaf0051991.

Diagnostic runtime-only relinking reuses all five unmodified AOT objects and the
runtime object, with input hashes recorded by relink-runtime-only.js. The control
with the outer-only archive reproduces the startup crash; changing only Magician
to the paired archive makes cache:clear exit 0 with no stderr, in 5.044s, and a
second matrix run passes it again in 5.250s. The linked binary is 174,273,624 bytes,
SHA-256 927e86718cbb004273c9be6d08396e2480d6382bdab689729f5d67ee1506c355.
Relinking alone took 1.654s; this is an iteration improvement, NOT full compilation
time or fresh CLI acceptance. Its explicit BCMath/PCRE archive selection differs
from the earlier full link, so the control is a separate A/B pair, not byte-identical
reproduction of that full executable. Both controls retain the same user objects.

All 33 console commands were rerun in native-global-paired-matrix/ using regular
file input for the public dummy value. debug:config framework and cache:clear now
exit 0 with empty stderr. Three nonzero exits remain among secrets commands;
decrypt-to-local also still emits an error despite exit 0. Existing-key generation
must be compared with PHP on the same state before calling it a compiler failure.
Other output/content and all side effects still need comparison, including the
previous debug:twig parameter-name discrepancy. Static/magic property/string sync
gaps remain explicitly scheduled for the next boundary tranche.

A fresh full compiler build completed with a bounded release configuration
(root opt-level 1, 256 codegen units, debug assertions off), one Cargo job. Earlier
application timing used a development-profile opt-level-1 compiler; validate
release timings before extrapolating those numbers to production performance.

## Default Welcome HTTP acceptance checkpoint

The route-less disposable fixture symfony-welcome.3oTPuv now serves Symfony's actual
default Welcome page through its unchanged public/index.php --web entry point.
It copies the original Kernel/config/vendor and omits only the custom application
controller; it does not modify examples/symfony-app or framework sources. No custom
response or route is substituted. Native PHP first verified three Welcome 404s.

The fresh compiler release-current, SHA-256
712527bb0e88020ae7669e98aa0bf3ce2c850c47a688134cc55c8b76d02ed4dc,
was built from source snapshot 7272cee8c06307dc4a8fb07dcf3ca55f097d43cb705b3247588643eaa8ae4eb0
with the paired runtime archive. Cargo completed in 18m25s under contention, max
RSS 4,470,915,072 bytes. CLI startup and a compiled release-ok smoke program pass.

Normal CLI compilation of the route-less public/index.php --web succeeds with
standard generated dev-container OPcache preload, regex capability and a 256 MiB
native heap. It takes 429.24s wall / 201.88s user / 9.78s system, max RSS
5,220,941,824 bytes. Six assembly slices, one worker, no cache hits. The application
binary is 213,802,344 bytes (203.9 MiB), SHA-256
ba06b9b28e2647a2d4d6db927938f9801d6bd8eb88877bb3da70aa047cae80ea.
The size gate passes here; build time and all 527 warnings remain open.
Main-thread timing records EIR optimization 4.66s, native codegen 64.95s,
assembly 233.37s and linking 25.04s. Child tool CPU and contention are separate;
this fixture/profile differs from the earlier production console timing.

Three native requests return Welcome 404 with the correct HTML content type.
After normalizing ONLY the template's documented random hue (welcome.html.php:135),
their full HTML matches all three PHP responses, SHA-256
65c649cdc0da021eec20e1eac9606debca33578f98e8168327159237edd8cb5f.
100 sequential native requests also all return Welcome 404 and match that full
normalized HTML. Native p50 was 5.904ms including capture overhead; this is not an
interleaved OPcache-enabled PHP performance benchmark. The own server/worker RSS
sum rises from 119,744 to 124,400 KiB across those requests; do NOT call this leak
freedom. A separate 500-request run also returns the exact normalized Welcome
HTML on every request, but its worker RSS grows from 46,720 to 69,168 KiB (about
44 KiB per request). This trend remains UNFIXED; RSS alone does not prove the
allocation mechanism. Generic 40,000-byte direct and ob_start/ob_get_clean web
controls each pass 500 requests and plateau after request 50 (6,320 and 6,464 KiB
server/worker RSS sums). Next: reduce dynamic-include/autoload lifetime versus
the complete request path, and attribute allocations before changing ownership.
The verbose eval trace probe timed out before readiness at 15 seconds; it is not
a runtime crash and did not reach the Welcome include. Do not infer native-only
template execution from successful rendering. All test-owned
servers are stopped by the probe; unrelated processes are untouched.

Artifacts: symfony-welcome-web-release-build.log, welcome-100-comparison.json and
the php-welcome-dev, native-welcome-dev and native-welcome-dev-100 directories under
target/framework-audit-20261020.Wcomr7/symfony-welcome.3oTPuv. This closes the specific
default Welcome HTTP gate on macOS ARM64, not the full framework campaign, every
route, cold-cache behavior, native-only inclusion, every target or final audits.
Old generated console assembly from build-pcntl, build-reference-loop and
build-native-globals was losslessly compressed to console.s.gz (about 96 MiB each
instead of 895 MiB). Their binaries and logs remain unchanged; gunzip restores
the assembly. The current Welcome assembly remains uncompressed for profiling.
Free disk fell to roughly 43 GiB during other ongoing work: avoid another heavy
Cargo build without a fresh resource check. Do not stop unrelated valid jobs.
The reason the bridge reports a
queue read error during cache clearing remains to be investigated separately.

The console's error:dump outputs do pass a stronger side-effect check: all 40 HTML
files match PHP byte for byte. Its list JSON has 27 differing leaves, currently
mostly PHP_SELF/source-path substitutions in help text, not missing commands.
Secrets failures persist when providing the dummy value from a regular file:
secrets:set stops warning about stdin, but reveal still cannot find it and decrypt
still reports a pointer-sized count. The encrypted file and list file do exist;
the list is valid PHP containing the expected null-valued key. Keep this separate
from the missing php://stdin wrapper. Minimal generic array-return probes pass
both when the concrete class is compiled and when it is dynamically included;
they do not yet reproduce the vault fault. Direct standalone SodiumVault list and
decryption also match PHP, with one and two possible concrete vault types; this
rules out the simple primitive-crypto and two-candidate-dispatch hypotheses, not
all native/eval or lifetime interactions. Evidence: `native-vault-file-input/`
inside the native copy and `dynamic-array-return/` in the audit directory.

The diagnostic console relink with paired global callbacks now has a concrete
lifetime trace, not a repair: `logs/hash-owner-lldb.log`, `hash-free-lldb.log` and
`hash-watch-lldb.log` in that native copy show the included vault list hash retained
for by-value eval foreach, then decremented by native object-slot cleanup, then
freed when foreach releases its copy. Its caller still uses the original list;
the allocator free-list pointer becomes the later `count()` result. The native
cleanup address maps to the exception landing pad of
`EnvVarProcessor::getEnv` (`L_f02e6e3bea60_604`), not a uniquely identified ordinary
statement in that method. The native producer/consumer edge still needs a generic
reproducer before changing ownership. Existing generic and standalone vault
probes pass, so neither suppressing foreach cleanup nor adding a blind retain is
justified. This divergence is UNFIXED; the relink is a diagnostic artifact, not
current-source framework acceptance.

The historical two non-iterable-foreach warnings are reproduced independently of
any framework: `indirect-iterable.php` calls an untyped array-returning method
through an object in an array. PHP and native both print a:one,two, but the frozen
compiler's --check calls the result int and claims the loop can never run. A
smaller forward-method-iterable.php reproduces this with a typed object parameter.
The checker now retains the latest warning per body/site, clears it when that loop
is revisited with an iterable type, and rechecks affected free-function bodies
only after method and function returns settle. This includes declared void
functions and nested closures; the pass preserves Mixed returns widened for
dynamic callers. No scalar-warning rule is suppressed. Three initial false-warning
tests failed before the fix; two additional wrapper/widening controls failed on
the first candidate. All nine driver tests now pass (seven foreach controls),
`foreach-warnings-final-driver.log`. Source is still lazy-petting-popcorn;
Cargo output alone lives in the separate scratch target-f directory.

The forwarding-array segfault is repaired on the current candidate. Untyped array
property storage is canonicalized in ClassInfo before the first function/body
inference, and again for a slot first initialized by a write (including null
constructor defaults). EIR metadata reuses the same rule. This keeps getters,
free-function returns and their callers on Hash/Mixed storage without widening
unrelated literal-array returns or changing declared/static property policy.
The first native candidate passed sixteen cases but exposed a separate three-round
return-inference cap: a chain of 32 forwarding functions retained scalar seeds.
The driver now permits a full declaration-length chain and exits early when stable.

Current focused evidence: 10 driver tests and 41 array EIR tests pass; the latter
include generation on all five supported targets. All 17 native integration tests
pass on macOS ARM64, including the original wrapper, a null-defaulted constructor,
string keys, copy separation, a returned hash after its object is unset, an
unrelated object-array/class-constant control, and 32 forwarding functions. Logs:
array-return-storage-final-driver.log, array-return-storage-final-neighbors.log,
array-return-storage-final-native.log. PHP oracles and the original signal-11,
EIR and LLDB captures remain under forward-wrapper-* and *-oracle.json. The
independently exposed vault-list use-after-free is NOT closed by this repair.
Native Linux/iOS framework acceptance and fresh framework builds remain separate.

The compiler-source snapshot is source-state-array-return-storage-final.json,
aggregate c2b21b437228a91805b8198c43563bea4707730d0f6d47e0e83fb6f623e5fe62.
The validated debug compiler is frozen in candidate-compiler-array-storage-debug/,
SHA-256 820035be314f4d912be8aae0a85195fb8d37265c19d9b06465a57698412cc1f3.
The package-only opt-level-one CLI rebuild completed in 508.11 s wall / 356.47 s
user CPU under load, with 4,899,749,888 bytes maximum RSS. Cargo artifacts remain
in scratch target-f; this compiler rebuild is not application build-time evidence
or a controlled optimization benchmark. The frozen 71,345,512-byte CLI is in
candidate-compiler-array-storage-opt1/, SHA-256
c665a20224354b090d928ccb8aa569ec66c4d265a441908942995252b2ef60f3.
Its 17 bundled staticlibs are from the prior release-current bundle: bridge source
inputs are unchanged. Manifest: candidate-compiler-array-storage-opt1.json.
No app/vendor sources were changed.

Fresh --check on the original entries succeeds with 456 console warnings and
454 HTTP-entry warnings, no foreach diagnostics and no errors (HTTP excludes
--web mode). Logs: array-storage-console-check.log, array-storage-http-entry-check.log;
summary: array-storage-symfony-check.json. These are not native runtime acceptance.
The isolated console's fresh build emitted 938,612,713 bytes of assembly, then
failed closed on the exact missing locked PCRE2 10.47 recipe-11 artifact in the
isolated cache. It ran 332.73 s wall / 195.98 s CPU with 3,303,243,776 bytes maximum
RSS; no executable was produced. logs/build-array-storage-opt1.log preserves the
failure. The locked macos-aarch64 install reports success (one package), but installed
under toolchain fingerprint cca6698e76e480fd64322066f67e752ee2d2e345a6c82bb33f64be5e60b7423b;
the compile required 148367798efc83969c4f67a6cc4256737f2ae16b55a998d95500e8b0f60ce0a7.
The pre-retry guard caught the mismatch before rebuilding. The raw PATH differs
between shell working directories, and toolchain.rs deliberately fingerprints it;
the other four allowlisted environment hashes match. Doctor is healthy from both
directories when the same parent environment is reused. No receipt was copied or
retagged. The retry now runs installation, doctor and standard-entry compilation
from one captured environment through build-console-array-storage.py, logging to
console-array-storage-retry.log and logs/build-array-storage-opt1-ready.log. The
failed assembly is compressed losslessly before the retry; its digest is preserved
in console-missing-pcre2-assembly.json. The manifest and lock hashes are unchanged.
This local cache recovery is separate from the passing array return regressions.
The shared-environment retry completed successfully: 266.14 s wall / 313.02 s user
CPU, 4,163,338,240 bytes maximum RSS, 456 warnings. The --keep-symbols executable
is 223,216,504 bytes (under 250 MiB), SHA-256
21a9eab2b528e0f77321e1b2b1423339b85d6e09319ba14c2d6059c73ff6e53e.
This exceeds the 1–2 minute build-time target. Its initial --version/list/about/
router smoke is recorded in native-array-storage-smoke/ in the isolated fixture;
all four smoke commands exit 0 without stderr. The actual list JSON contains
exactly 33 command names and the existing matrix covers all of them (no missing
or unknown case names). A fresh copied PHP fixture, php-console-array-storage.5ok8dky1,
ran all 33 commands; only secrets:generate-keys exits 1 because the copied keys
already exist, while the public test value is revealed correctly. Native execution
of the same 33 cases is now running in native-array-storage-matrix/, with writes
confined to the native fixture by the existing sandbox profile. Full output and
side-effect comparison remains to be performed; exit-code smoke is not acceptance.
The completed 8 MiB native matrix has eight exit-code differences from PHP. Six
commands exhaust the heap, while stdin and vault-list corruption remain separately
reproduced. This was NOT a matched resource profile: direct Mach-O data inspection
proves the prior paired-symbols console has heap_max 268,435,456 bytes and the new
one has 8,388,608. The old binary still passes debug:container and cache:clear on
the current fixture. Do not classify the six exhaustion results as a source-code
regression without matching the profile. Evidence: console-heap-profile-comparison.json
and native-array-storage-old-control/. A standard-entry rebuild with the established
256 MiB heap is running through build-console-array-storage-256m.py, output directory
build-array-storage-opt1-256m and log logs/build-array-storage-opt1-256m.log.
The 8 MiB binary and its complete failing matrix are retained. Its assembly is
losslessly archived as build-array-storage-opt1/console.s.gz, verified identical
to the first missing-PCRE2 assembly; console-8m-assembly-archive.json records the
hash. This reclaimed roughly 895 MiB without discarding evidence. The owned
256 MiB compiler process was briefly stopped and resumed while archiving; no
unrelated job was stopped. Free space has recovered to about 12 GiB as other work
also completed. The 256 MiB build has now exited 0 and its heap_max was verified
directly in Mach-O data; the 33-command matched-profile matrix is running in
native-array-storage-256m-matrix/. Next compare all
33 commands again on the 256 MiB candidate, then investigate real behavioral and
lifetime differences; heap limits alone do not establish leak freedom.

The matched 256 MiB build completed in 198.31 s wall / 288.21 s user CPU, with
4,192,829,440 bytes maximum RSS and 456 warnings. Its 223,216,648-byte executable
has SHA-256 c35a0a132e8ae2a65ea7b3fbb7b647bcca6022683f88da1a0bd2b70f0c17e5a2;
heap_max was verified in Mach-O data. All 33 commands were replayed. The six
8 MiB exhaustion cases now execute successfully, including debug:container and
cache:clear (the latter exit 0 with no stderr). Only secrets:reveal and
secrets:encrypt-from-local differ in exit status from PHP, but secrets:set still
warns about php://stdin and secrets:decrypt-to-local still exits 0 while reporting
a pointer-sized count and an error. Those false successes are not passes. The
existing-key warning from secrets:generate-keys matches PHP's status 1. All 40
error:dump HTML pages match PHP byte for byte. Complete stdout/stderr parity for
the remaining commands is still pending; inspect console-array-storage-256m-comparison.json
and native-array-storage-256m-matrix/report.json before making broader claims.
A stricter text comparison is now preserved in console-array-storage-output-comparison.json:
stdout differences remain for list, about and debug:twig; stderr differences are
in the secrets family. Exact app/executable path substitution was applied, but no
whitespace or arbitrary content normalization. A structured list comparison now has zero differing leaves after exact app and
invocation-path normalization; slash escaping defeated the first textual path
replacement. Evidence: console-array-storage-list-structured-diff.json. About
reports differing cache sizes (fixtures have different generated-cache states)
and PHP profile 8.5.0 versus the host oracle's 8.5.10; do not call it full output
parity. debug:twig omits yaml_dump's inline and dumpObjects parameter names, while
PHP prints yaml_dump(inline, dumpObjects); yaml_encode retains those names on both.
Keep that precise introspection defect in the open callable/Reflection gate.
These observations are recorded in addition to exit-code results.


The generic php://stdin slice is implemented, with eight focused host-native
regressions passing in stdin-final-native.log. Native dispatch recognizes the URL
once, duplicates fd 0, optionally seeks, uses bounded/unbounded stream readers,
persists the result and closes only its duplicate. Eval mirrors the semantics and
recognizes native STDIN payload zero separately from eval-owned resource ids;
standard descriptor cloning preserves close-on-exec. Cases cover 131,073-byte
input, scratch reuse, shared native/eval cursors, byte caps, empty input, seekable
files, non-seekable pipes, invalid length, closed stdin and exact URL punctuation.
Source: source-state-stdin-final.json, SHA-256
97b511569d8e1ceed494557440a6afb413d945070e4bf4b0fcbcf1e2284aed0e.
The final five-target emission test passes (stdin-final-targets.log); this is not
Linux/iOS execution evidence. Assembly-comment alignment and focused diff hygiene
pass. No fresh Symfony acceptance has yet run for the final source.

The broader stream-neighbor run completed 368 passed, eight ignored, one failed
(stdin-stream-neighbors.log). The wrapped named-params notification callback
SIGSEGV reproduces with the pre-stdin frozen c665a202 compiler, independently of
stdin changes (stdin-neighbor-callback/comparison.json). PHP exits zero with its
connection-refused warning and `closed`; the existing `W9;closed` test expectation
also needs event-semantics review. Keep this failure open and scheduled after the
vault-list lifetime defect; do not call the whole stream suite green.

The first optimized stdin compiler built in 447.77 s and Magician in 45.34 s,
but then failed the additional punctuation reducer: folding colon/slash accepts
control bytes 0x1a/0x0f as URL separators. PHP rejects all three malformed paths
without consuming input; a11f2498 instead consumed the payload. Evidence:
stdin-punctuation-oracle.json and stdin-punctuation-native-before.json, plus an
isolated execution of the exact emitted probe. Preserve that compiler as
candidate-compiler-stdin-before-punctuation, not an accepted candidate. Both ARM64
and x86_64 now compare punctuation exactly; the eighth native case proves this on
the host. The sequential stdin-final-acceptance.py job is now running (exec session 83774):
build-stdin-final-candidate.py, final CLI stdin smoke, standard console rebuild,
then six secret commands in both native and PHP isolated copies. It uses the
established heap and shared root-parent environment. Results/logs are
stdin-acceptance-results.json and stdin-acceptance-*.log; semantic/file comparison
is still required after the job, even if its commands exit zero. The c35a0a13 Symfony console still has the earlier stdin behavior.

Builtin-docs exporter, extraction, module sections and site compatibility pass.
Comparison generation passes with the already installed tomli exposed as tomllib
for shell Python 3.9; no script edit was needed. The audit stays red on inherited
census expectations (481 versus 443 non-registry contracts), and the boundary
audit on unreferenced RuntimeFnId::MixedSpreadArray; structural registry errors
are zero. These contract/catalog/runtime-function inputs did not change in the
stdin slice. Logs: stdin-docs-*.log and stdin-docs-results.json (the comparison
retry is stdin-docs-comparison-tomli.log). Preserve pre-existing generated bytes
in stdin-builtin-docs-before.tar.gz (2,437 files); 34 metadata/line-reference deltas
are listed in stdin-builtin-docs-changes.json. No hand-edited generated pages.
The PHP-text ratchet remains red at 80 sites versus 58 allowed, with no findings
on stdin-changed paths (stdin-php-text-findings-scope.json); do not add inherited
sites to the allowlist. An initial exporter invocation omitted scratch/job env and
hit ENOSPC; every later Cargo/audit command uses scratch target-f and one job.

The mechanism and punctuation trap are recorded in the Trail learning
2026-10-21-php-stdin-reads-require-duplicated-descriptors-guillaume.md. Kioku wrote
bug_fix-php-stdin-bounded-readers-share-cursor-across-na-52befb81.md before its
indexing scan exceeded four minutes and its verified process tree was terminated;
that initial whole-file packet now needs narrow citations/reverification for the
final source. Diagnosis: stdin-kioku-learn-timeout.json. Kage recall had also timed
out; Trail, PHP oracles and this ledger are the explicit fallback. No commit/push.

 Fixture input fingerprints (1,839 files including
the extensionless console entry) are console-array-storage-fixture-inputs.json.

The older scalar runtime tests now assert the exact source path and line: PHP and
the frozen compiler already emit that suffix, so the previous message-only
expected output was stale. Their runtime scalar warning rule is retained.

A current-source --check of the original Symfony bin/console succeeds with the
same 456 warning lines as the frozen compiler, with no added or removed warning
lines and no foreach warning on either side. It does not establish a framework
warning reduction or native command acceptance. --web cannot be combined with
--check; the CLI rejects that invocation, so its immediate zero-warning log is
not a successful check. Comparison: symfony-console-foreach-check-comparison.json.
The plain public/index.php entry also passes --check on both binaries with the
same 454 warning lines; this explicitly excludes --web mode. Details are in
symfony-entry-foreach-check-comparison.log. The current debug compiler is frozen
as candidate-compiler-foreach-warnings/elephc, SHA-256
498ce12ecafee310ba8a3914988697baf3fdf7262dc541dfc862447f905e634a;
its compiler-input inventory is source-state-foreach-warnings.json, aggregate
de3c90b48a9747b09c0565e89f7f20becd514abf27f3f58a514d297db2e43c12.
All five changed compiler files belong to this worktree. The preserved Symfony
and Laravel source fingerprints remain unchanged (2,180 and 6,244 files).
Kage risk and properly scoped context calls each timed out after 300 seconds;
Kioku recall, the shell Trail graph and this plan provide the fallback evidence.
The Kage learning capture did not return and was cancelled; no packet write is
confirmed. The reusable findings are saved in the uncommitted team note
`.trail/learnings/2026-10-21-foreach-warning-refresh-must-preserve-dynamic-guillaume.md`.

The old 404 mismatch also needs its oracle corrected: a controlled PHP 2-by-2
comparison isolates `variables_order`, not the presence of a PHP router script.
Host PHP.ini uses GPCS and produces a roughly 194 KB debug page despite the process
environment selecting prod. With EGPCS (also PHP's `-n` default), PHP and frozen
`fs23` return identical 993-byte prod 404 pages, on three repeated requests. These
captures are `php-missing-*` / `native-fs23-missing-prod`. Do NOT infer full globals
parity: `global-order-probe/` separately measures two unfixed divergences. PHP
reports `ini_get('variables_order')` as EGPCS and no APP_ENV in `$_SERVER`; Elephc
reports false and exposes APP_ENV in `$_SERVER`. Both expose it in `$_ENV` and
`getenv()`. The current app fixture also has a custom Twig home page: PHP serves
695 bytes in HTTP 200 in both tested environments; this is not the route-less
Welcome page. Its README's dev-mode routing claim is stale. Fixture source files
were not edited to obtain these results.

Fixture provenance is also open: `examples/symfony-app` already has changes to its
Composer manifests and lockfiles, plus existing probes. Symfony Demo lives in the
external scratch directory, without a Git checkout; Laravel lives in
`agent-a5d6b658a31ea8606/examples/laravel-app`. Preserve these pre-existing files,
use the standard framework entry points, and snapshot their sources before retries.

Native PHP/Xdebug baseline (2026-08-29): HTTP 404, 39,542 response bytes,
72,544 trace events, 570 distinct targets, 1,421 edges and 215 include targets.
Normalized artifacts: `target/symfony-native-xdebug/ledger/`. The generic tools are
`scripts/normalize_xdebug_trace.py` and `scripts/classify_execution_ledger.py`.
Their historical capability classifications require revalidation against current
code; keep the full edge inventory rather than assuming a trace proves support.

## 2026-10-22 checkpoint: the console secrets family is an eval-boundary use-after-free

Re-measured on the current tree with `candidate-compiler-stdin-opt1` (its source manifest still
matches HEAD's worktree files byte for byte). The 33-command console matrix on
`build-stdin-opt1-256m/console` still differs from PHP on exactly two exit codes —
`secrets:reveal` (native 2, PHP 0) and `secrets:encrypt-from-local` (native 1, PHP 0) — and on the
secrets stdout/stderr text. `secrets:list --reveal` segfaults; `secrets:decrypt-to-local` reports a
pointer-sized secret count.

`ELEPHC_EVAL_TRACE=1` establishes that `Symfony\Bundle\FrameworkBundle\Secrets\SodiumVault`,
`DotenvVault`, `Command\SecretsListCommand` and the generated
`Container3yplunE\App_KernelProdContainer` have **no native symbols at all** in `console.s` and run
entirely in Magician. The dump is produced by `cache:warmup`/`cache:clear` at runtime, so compiling
that generated container ahead of time is the single largest AOT/eval reduction available for this
campaign (see the new objective below).

Two defects are now isolated:

1. `eval_declared_return_control_value` returned a **borrowed** `return $var;` handle without taking
   a lease, while `finish_activation_scope` releases every owned local immediately after. It now
   retains the borrowed value exactly like the `Throw` arm does. 98 magician return/ownership tests
   pass. This is necessary but NOT sufficient: `secrets:list --reveal` still faults.
2. The payload of `$secrets` is already freed when `SodiumVault::list(true)` reaches its activation
   drain; a temporary trace in `finish_activation_scope` shows the hash header 16 bytes before the
   payload holding the allocator's `0xf2ee…` free poison. The working sibling (`list(false)`, which
   returns early) and the failing one differ only by the internal
   `foreach ($secrets as $name => $value) { $secrets[$name] = $this->reveal($name); }`. The next
   suspects, in order, are `eval_non_object_array_set_var_stmt`'s `values.retain(cell)` receiver
   lease plus its operand-release loop, and `__rt_mixed_array_set`'s publish of the
   `__rt_hash_ensure_unique` result back into the owning cell.

Notes for the next session: `--heap-debug` masks the fault (different allocator layout), so a green
heap-debug run proves nothing; `settings set target.disable-aslr true` is required to make heap
addresses reproducible for an lldb watchpoint on a payload header; and the diagnostic trace must
guard any raw `payload-12` read because tag-8 nulls carry `0x7ffffffffffffffe` payloads.

### Sharpened mechanism (measured with lldb watchpoints on the hash header)

`__rt_mixed_clone` (the by-value foreach copy) increfs the payload to 2. By the time the loop body
reaches `__rt_mixed_array_set` for the `$secrets[$name] = $this->reveal($name);` write, the header
already reads 1, so `__rt_hash_ensure_unique` takes the in-place branch, the cell keeps the same
payload, and the write is correct. The owner is lost **inside the nested `reveal()` call**
(`loadKeys()` -> `LazyString::__toString` -> `Container::getEnv` -> `getParameter`), not in the
foreach or the write. The iteration-copy release then drops the payload to 0 and the activation
drain reads the allocator poison.

The healthy sibling confirms the expected shape: a foreach whose subject is a *temporary*
(`foreach ($this->list() as ...)`) logs exactly one `_rt_incref` and two `_rt_decref_hash` (the
iteration copy plus the owned subject) and ends with a correct free. So the accounting rules are
right; a stray owner is being released somewhere on the native/eval boundary during `reveal()`,
and the freed-block address is later reused by the vault-list hash.

`gdb`/`lldb` recipes that worked: break at `__rt_mixed_array_set` by address (symbol lookup by name
fails after `run` unless the breakpoint is created once the process is stopped), filter
`*(u64*)$x0 == 5 && $x2 == 23`, then watch `payload-12` (32-bit) and print 30 frames per write.
`eval_declared_return_control_value` is shared by methods, free functions and closures, so the
new lease covers all three return paths.

### The stray owner, located

Arming the watchpoint on every by-value-foreach copy whose source hash has one entry keyed
`ELEPHC_ACCEPTANCE_PROBE` yields, for the failing payload, this exact sequence:

1. `_rt_incref` from `copy_value` in `execute_foreach_stmt` — the iteration copy, refs 1 -> 2.
2. `_rt_decref_hash` from **`_rt_cleanup_invoke` <- `_rt_object_free_deep` <-
   `_rt_release_slot_table` <- `elephc_xslice_L_f02e6e3bea60_604`** — refs 2 -> 1.
   That slice is `method___Symfony_N_Component_N_DependencyInjection_N_EnvVarProcessor___getenv`,
   i.e. the cleanup of the native frame reached through
   `reveal()` -> `loadKeys()` -> `LazyString::__toString` -> `Container::getEnv`.
3. `_rt_decref_hash` from `__elephc_eval_value_release_v3` in `execute_foreach_stmt` — refs 1 -> 0.
4. `_rt_heap_free`, then the block is reused by an object allocation.

So a native frame's slot table releases an **object**, and freeing that object's deferred cleanup
decrements the interpreter's array. The interpreter never gave up an owner, so the object carries a
borrowed reference it does not own. `__rt_release_slot_table` is emitted by
`src/codegen/frame.rs:1861` from the `offset | kind << 32` table built in
`src/codegen_support/runtime/exceptions/cleanup_preserve.rs` (kinds include
`__rt_decref_object` -> `__rt_object_free_deep` -> `_rt_cleanup_invoke`).

Next step: break on `_rt_object_free_deep` while inside that slice and read the object's class id /
pending cleanup list to name the object; then trace how eval handed it a borrowed array. The
`EnvNotFoundException` built inside `getEnv` is the prime suspect because its trace/cleanup path is
the only object created there that the interpreter also knows about.

Measured object shape for that suspect (macOS ARM64, `--keep-symbols` build): every object freed by
`EnvVarProcessor::getEnv`'s slot table at that point has words
`[0x1d0, <string ptr>, <line-ish small int>, 0, 0, 0, 0, 0x108c025c7, 0xd1, 0xe0, 0, <cell ptr>]`.
The `0x108c025c7` word is a code address and the small int reads like a source line, so the freed
object is part of the pending-throwable/backtrace machinery rather than a plain service object.
No property slot in its first 512 bytes reaches the vault-list payload, directly or through one
`tag 7` reference wrapper.

**Caveat that invalidates address-based correlation across a whole run:** the same heap address is
reused for many different arrays. Breaking on `_rt_decref_hash` with `x0 == <payload>` logs seven
unrelated decrements (from `Input::bind`, `_rt_array_predicate_boxed`,
`eir_cbody_…_eval_rethrow`, the native `getEnv` frame, and the foreach release). Only a watchpoint
armed *inside one window* is meaningful; global address matching is not.

**Correction, measured:** the object freed by `EnvVarProcessor::getEnv`'s slot table is an
`EnvNotFoundException` (message readable in the dump: `EnvNotFoundException for variable
"SYMFONY_DECRYPTION_SECRET"`), but it is exactly `PAYLOAD_SIZE = 0x78` bytes, so the word at
`obj + 0x78` that appeared to hold the vault-list hash is the **first word of the next heap block**
(`0x10b06ab90 + 0x78 = 0x10b06ac08`), not a property. The exception therefore does not reference
the array by property. What it owns is its `trace` at `TRACE_OFFSET = 0x58`, an indexed array of
8 frame hashes (`len=cap=elem=8`), each frame a `tag 5` cell over a hash with
`count/cap/meta=7/head/tail/entries` (verified header:
`[6, 0x10, 7, 3, 5, 0x10b0792f0, 0, 0x8000000000000000]`).

**Do not trust a raw 512-byte pointer scan on these blocks.** Reading past a block into its
neighbour produced a false "the trace references the payload" result, exactly like the earlier
`obj + 0x78` artefact. A correct walker must decode each block by its real layout (cell →
`[tag, payload, high]`; hash header → `count, cap, meta, head, tail, entries` with 64-byte entries;
indexed array → `len, cap, elem_size`, data at +24) and never read past an allocation. The last
attempt decoded the trace root and the frame cells correctly but then found no occupied hash
entries, which is the next thing to fix in the walker (entry occupied-flag offset for these frame
hashes is not the one assumed).

**Resolved and measured:** `hash_set`'s entry layout is
`[0] occupied, [8] key, [16] key_len, [24] value_lo, [32] value_hi, [40] value_tag, [48] prev, [56] next`
(`src/codegen_support/runtime/arrays/hash_set.rs`). With the value payload read at `[24]` the
walker decodes the exception's trace correctly: 8 frames, each carrying `file/line/function/class/
type/args`, and it walks 164 nodes. **The trace does NOT reference the vault-list payload** — the
earlier FOUND was another past-the-allocation artefact. `eval_backtrace_frame_array` retains both
its `object` and every `args` entry, so the captured trace is owned correctly.

Remaining standing hypothesis, unchanged: the object freed by `EnvVarProcessor::getEnv`'s
`_rt_release_slot_table` reaches the vault-list hash through a native `__rt_decref_hash` on a
property slot that was stored **without an owner** — the property value is stored as a raw hash
pointer (`__rt_throwable_capture_trace` does exactly that for `trace`), so any code that stores a
raw hash pointer into an owned slot must increment there. The scan must check **direct raw hash
equality at the property offsets of the actual freed object** (`p == payload`), which it does, and
it found nothing within 512 bytes — so either the object is bigger, the slot holds a boxed cell of
an unscanned tag, or the freeing object is a nested one reached from `__rt_eval_object_release_children`.




The next tool must be address-reuse-proof: a debug owner registry (map `payload -> live
cells/functions`) fed by `__rt_mixed_clone`/`__rt_incref` and drained by every release, plus a
generation counter so a reused address cannot inherit an old entry; it reports the surviving cell
and the releasing function when a payload still has owners. Alternatively recurse the object graph
in lldb.

## The ownership path, restated with the caller

`Container::getEnv` does `$this->getEnv ??= $this->getEnv(...)` then
`$processor->getEnv($prefix, $localName, $this->getEnv)`, so the object slot in the **native**
`EnvVarProcessor::getEnv` frame holds the first-class callable bound to the Container, and that
frame's `_rt_release_slot_table` frees it. The watchpoint sequence over the failing payload is, in
time order: `_rt_incref` (the by-value foreach copy, 1 -> 2), `_rt_decref_hash` from
`_rt_object_free_deep <- _rt_release_slot_table <- getEnv` (2 -> 1), the foreach's iteration
release (1 -> 0), then `_rt_heap_free`; the return clone increfs the freed block afterwards.

Next lead: the freed object is the bound callable, the `ServiceLocator` or the `EnvVarProcessor`
created inside `Container::getEnv`; some array reachable from it shares the vault-list payload
without an owner. The deterministic tool is a **native ring-buffer diagnostic**: record every
`__rt_object_free_deep` (object pointer, class id, caller) in a fixed BSS ring, flush at exit behind
a flag, and correlate against an owner registry kept by eval's `copy_value`/`release`. About one
compiler rebuild and one console build per iteration (~10 minutes); no address-reuse ambiguity.no address-reuse ambiguity.

## Concrete store-path finding (checked, not yet fixed)

`src/codegen/eval_property_helpers/dynamic_properties.rs:123`
("Writes a fallback hash with an independent retained owner for the borrowed input cell") calls
`__rt_incref` on the **boxed value cell** (`[sp, #24]`) before `__rt_property_hash_set`, so the
fallback store is balanced by the box release on the freeing path.

The path with no such audit yet is the **declared eval-reachable slot body**
(`emit_aarch64_set_slot_bodies` / `emit_x86_64_set_slot_bodies` in
`src/codegen/eval_property_helpers.rs`): a declared `array`/hash property whose slot is *raw*
stores the payload pointer, while an interpreted `dynamic_property_set` (the trace shows
`loadedDynamicParameters`, `dynamicParameters`, `buildParameters` — all `Ok(5)`) hands the helper a
boxed cell. If a slot body stores the unboxed payload **without** `__rt_incref`, the owning
object's later `__rt_object_free_deep -> _rt_cleanup_invoke -> __rt_decref_hash` decrefs exactly one
owner it never acquired — which is the measured stray release, and the victim is whatever array the
allocator recycled into that block afterwards.

That matches every observation: the decref is native, hash-typed, exactly one owner short, and the
victim changes with the allocation order. Audit both slot-body emit paths (and their x86_64 twins)
for the missing `__rt_incref` on unboxed raw hash/array stores, then rebuild the compiler and the
console and rerun the secrets matrix.then rebuild the compiler and the
console and rerun the secrets matrix.

**Audited and cleared:** the declared-slot store paths are balanced, so they are not the culprit.

- `PhpType::Array/AssocArray` (`emit_aarch64_store_heap_property_slot`, `src/codegen/eval_property_helpers.rs:1695`)
  unboxes the assigned box (`__rt_mixed_unbox` -> `x1` = payload), moves `x0 = x1`, then
  `emit_incref_if_refcounted` (`src/codegen_support/abi/values.rs:69`) increfs the **payload**, and
  the freeing path releases that raw payload through `__rt_decref_array`/`__rt_decref_hash`.
- `PhpType::Mixed|Union` increfs the **box** and the free releases the box via `__rt_decref_mixed`.
- The dynamic-fallback store retains the box before `__rt_property_hash_set`.

So a declared `array` property store and its free are balanced. The remaining unbounded unknown is
which object is freed by `EnvVarProcessor::getEnv`'s slot table and how it reaches the hash. The
deterministic tool stays the native ring-buffer diagnostic for `__rt_object_free_deep` plus an
owner registry fed by eval's `copy_value`/`release`.owner registry fed by eval's `copy_value`/`release`.

## Sharpened identity of the offending slot (next audit target)

The xslice in the decref chain is a **shared exception-cleanup region**, so the frame being cleaned
may belong to any function of that module — not only `EnvVarProcessor::getEnv`. The chain
`_rt_object_free_deep` under `_rt_release_slot_table` means a *slot of an EIR function frame* held
an object whose array property was decref'd. `EnvVarProcessor` holds no arrays, but the compiled
functions that eval invokes (the `eir_cbody_…` bodies and `EnvVarProcessor::getEnv` itself) receive
their arguments **staged from eval**, and a staging path that hands native frames borrowed argument
cells would make the frame's slot-table release steal exactly one owner. The generated container is
the prime suspect as the *victim object*: its `parameters`, `buildParameters`,
`loadedDynamicParameters` and `dynamicParameters` are all hash properties, set from eval
(`dynamic_property_set … Ok(5)`), and it is passed around as an object across the eval/native
boundary constantly.

Audit next: the eval→native staging of **argument and receiver cells**
(`eval_native_method_with_values`, `with_eval_call_arguments`, `__elephc_eval_*_call` bridges) —
every cell handed to a native frame whose slot table releases Mixed slots must either be retained or
explicitly excluded from `_rt_release_slot_table`. The same audit applies to the receiver.explicitly excluded from `_rt_release_slot_table`. The same audit applies to the receiver.The same audit applies to the receiver.

## Staging audit progress (measured, narrowed)

- The slot table that releases a native/EIR frame's Mixed slots is built by
  `function_cleanup_locals` (`src/codegen/frame.rs:1658`). Its filters are exact:
  epilogue-cleanup kinds, non-borrowed slots, no ref-cell-only slots, return slot excluded, and
  crucially  `!local_slot_is_parameter(local.id) || ctx.owns_parameter_slot(local.id)`.
- `owns_parameter_slot` (`src/codegen/local_analysis.rs:350`) makes a **by-value parameter slot
  owned by the frame** when  `stored_slots.contains(slot) || ever_ref_cell_slots.contains(slot) ||
  by_ref_argument_param_slots.contains(slot) || prologue_boxes_owned_mixed || (eval_can_replace_locals
  && local.kind == PhpLocal)`, where `eval_can_replace_locals` is true for any function whose locals
  contain `EvalScope` / `EvalGlobalScope`.
- So a compiled frame invoked from the *interpreter* with an eval-replaceable local does publish its
  by-value parameter in the release-table — the frame releases that parameter at exit, and the
  contract is that its prologue retained it. If the interpreter's staging of that argument cell
  skipped the retain because the slot was classified borrowed, the frame steals exactly one owner:
  `2 -> 1` at exit, which is the measured stray decref.

Next concrete step (one read): check the EIR parameter prologue retain — whether the prologue's
retention decision uses the same `owns_parameter_slot` predicate as `function_cleanup_locals`, and
what the interpreter passes for a by-value `Mixed`/object parameter of a compiled function invoked
from eval. A mismatch between the *retention* predicate and the *cleanup* predicate is a one-line
fix; otherwise the staging retain is missing for this class of parameter.otherwise the staging retain is missing for this class of parameter.

## Audit result: prologue and cleanup agree on parameters

`src/codegen/frame.rs:501` retains an owned by-value parameter in the prologue:

```rust
if !param.by_ref && ctx.owns_parameter_slot(slot) && !converted_to_owned_mixed {
    retain_owned_parameter_local(ctx.emitter, offset, &local_ty);
}
```

and the cleanup table includes the same slot under the same predicate — one clause is exact:
`converted_to_owned_mixed` is `!by_ref && local_ty == Mixed && decl_ty != Mixed`, and for that
case the prologue instead *re-boxes* the incoming value (`emit_box_current_value_as_mixed`), which
is itself the owner, so the cleanup release is balanced with the re-box. Parameters are therefore
consistent between retention and cleanup. `emit_box_current_value_as_mixed` must be re-audited only
if evidence points at it.

Narrowing that stands: `local_kind_needs_epilogue_cleanup` covers only
`PhpLocal | HiddenTemp | OwnedTemp | NamedArgTemp`, so a dedicated receiver slot is *excluded* from
the release table — meaning the freed object in the getEnv chain is a `PhpLocal`/temp, i.e. a real
PHP variable of that compiled body, not the receiver. `EnvVarProcessor::getEnv`'s PHP locals are
`$i, $next, $key, $array, $env, $backedEnumClassName, $backedEnumValue, $processor, $default…`, of
which `$processor` (a `ServiceLocator`/`EnvVarProcessor` object) and `$this->container` are arrays-free,
but the shared xslice may serve *other* bodies of the same module. Next: enumerate which compiled
bodies share the slice and which of their locals is an object with a hash property.which of their locals is an object with a hash property.

## Corrected identity: the freed object is the EnvNotFoundException (NOT a Container)

`EnvVarProcessor::getEnv`'s compiled frame slot released an object whose runtime class id is `0x1d0`
(464). `_instanceof_name_class_464` reads only `Symfony\Component\DependencyIn…` on 40 bytes, which
is AMBIGUOUS between `Container` and `Exception` in this namespace. The object dump settles it: word
[1] is the message string ("Environment variable not found: SYMFONY_DECRYPTION_SECRET"), [2] the
code, [11] the trace cell, [13] the previous cell — the **Throwable layout** (`Exception`,
`PAYLOAD_SIZE = 0x78`). The freed object is the EnvNotFoundException, NOT a Container. Correction of
the earlier Container claim in this same ledger: the chain is:

    container property (hash) over-released earlier
      -> its block recycled by the vault-list hash
      -> EnvVarProcessor::getEnv's slot table frees the container
      -> __rt_object_free_deep decrefs that property = our live vault list

CORRECTION of this paragraph: no container property is involved — see the Container identity
correction above. The measured fact stands on its own:
`__elephc_eval_value_property_set` -> the eval overlay fallback
(`emit_dynamic_property_set_fallback` -> `__rt_property_hash_set`), not only the declared slot
bodies. Audit next, in priority order: `__rt_property_hash_set` / `__rt_property_hash_get`
fallback-Storage helpers and the eval-side `DynamicPropertyOverlay` bookkeeping around
`context.dynamic_property_records` — one of these must release the *previous* stored value without
owning it, or store the new one without retaining it.one of these must release the *previous* stored value without
owning it, or store the new one without retaining it.

Measured while reading the helpers: `__rt_stdclass_set` is a thin wrapper — it lazily allocates the
property hash and forwards to `__rt_property_hash_set` (`stdclass.rs:335`) with `value_tag = 7`
(boxed Mixed), and the eval fallback retains the box before calling it. `__rt_hash_set`
(`src/codegen_support/runtime/arrays/hash_set.rs`) fills
`[0] occupied, [8] key, [16] key_len, [24] value_lo, [32] value_hi, [40] value_tag, [48] prev,
[56] next` and detects an existing key by comparing `[8]`/`[16]`; the read of its overwrite path
must confirm the replaced boxed value is released (`__rt_decref_mixed`), which was NOT visible in
the audited range — a missing release there is a leak, not this UAF, but it belongs to the same
helper audit.helper audit.

## Narrowing continues: receiver/overlay read helpers are clean; the decisive probe is the container's refcount

- `__rt_hash_set` overwrite path releases the replaced payload (`__rt_decref_any` for heap-backed,
  `__rt_decref_mixed` for boxes) — balanced.
- Reads from the eval overlay retain (`instance_property_access.rs` lines 331 / 453 / 570), so an
  eval read of a container property does not hand a borrowed cell to the scope.
- The receiver is held in a reserved register, not a frame local (`frame.rs` uses only eight
  `receiver` references, all about the nested-call register) — so the freed object really is a
  PhpLocal/HiddenTemp of `EnvVarProcessor::getEnv`'s compiled body. Candidates: the temporary that
  carries `$this->container` across a call to the generated container's methods (`hasParameter`,
  `resolveEnvPlaceholders`), which is an *object with hash properties*.

**Decisive probe (one lldb run):** when `__rt_decref_hash` fires with the armed payload, walk to the
`__rt_object_free_deep` frame, read the container's refcount at `[obj - 12]`: if it reached 0 the
free is legitimate and the bug is an earlier over-release of the *array* property; if it is still
>0 inside the guard (a stolen owner), the container is being freed while the interpreter owns it and
the audit moves to the `HiddenTemp` staging of `$this->container` in the compiled body. This single
measurement separates the two remaining hypotheses and needs no compiler rebuild.two remaining hypotheses and needs no compiler rebuild.

**Measured answer (lldb, armed payload + walk to `__rt_object_free_deep`):** at the free of the
container the refcount word at `[obj - 12]` read `0x80000000` — the
*in-progress-allocation guard bit* that `__rt_object_free_deep` itself sets
(`orr w9, w9, #0x80000000; str w9, [x0, #-12]` in
`src/codegen_support/runtime/arrays/object_free_deep.rs`). The real owner count had reached zero, so
the container death is LEGITIMATE.

Consequence: the stray decref that hits the vault-list payload is the container releasing its own
`hash` property, and that property's array was **already over-released before the container died**
(the container still held a pointer whose refcount had reached 0, and the vault list recycled that
block).

The container's native array properties are `parameters`, `buildParameters`, `envCache`,
`resolving`, `loadedDynamicParameters`, `dynamicParameters`; the last two are written from eval
trough the overlay, the others natively. The remaining audit is therefore the life cycle of one
specific array across the eval boundary during boot: whichever container array an eval read holds in
a scope cell while native code replaces it, or a `getDynamicParameter` array stored into
`dynamicParameters` and released twice (the eval `finally { unset($this->resolving[...] }` path and
the `getDynamicParameter` overwrite are the two PHP-visible write points).the `getDynamicParameter` overwrite are the two PHP-visible write points).

## Mechanism located: the slot at PAYLOAD_SIZE [0x78] holds OUR payload pointer

Re-reading the earlier object dump of the freed `EnvNotFoundException` (0x10b06ab90) with the
Throwable layout (`FILE=0x38, LINE=0x48, TRACE=0x58, PREVIOUS=0x68, PAYLOAD_SIZE=0x78`):

    word[15] = obj + 0x78 = 0x10b06ad30  ==  the armed vault-list payload

`0x78` is exactly where `emit_property_hash_slot_helper` places an **eval-overlay property hash slot**
(the "one-property tail offset" that test asserts). The stray `_rt_decref_hash(x0 = 0x10b06ad30)` is
therefore the release of the exception's **dynamic property hash**, whose pointer had been replaced
by the recycled vault-list block.

So the real, earlier over-release is: **an exception's dynamic property hash** (created at
0x10b06ad30 by an earlier `EnvNotFoundException` during boot and discarded by the catch) was released
while still owned, its block then recycled as the vault list, and the fresh exception (created during
`reveal()`) whose overlay slot points there decref'd our live array at its free — which is what the
foreach then tripped over (`array_len` read the free-list pointer).

Fix candidates, in order: (1) the eval overlay must NOT store a raw hash pointer in the slot at
PAYLOAD_SIZE while the interpreter also tracks its children in `object_owners` (one owner, one
release); (2) `__rt_object_free_deep` must skip the PAYLOAD_SIZE slot for eval-owning objects when
`__elephc_eval_dynamic_object_owns_properties` already releases it.

**Discrimination trap to avoid:** a `_rt_decref_hash` breakpoint conditioned on the armed payload
fires for *several different arrays that share the address over the process lifetime*. On the run
above, the first hit whose chain contained `__rt_object_free_deep` freed an object at
`0x10b06ab90` with class id 464 — but every later address-reuse makes "first hit" ambiguous. The
discriminating condition is time: only the decref that happens AFTER the vault-list array exists is
relevant, and the vault-list array is identifiable by
`count==1 && entries[key 'ELEPHC_ACCEPTANCE_PROBE']` at `__elephc_eval_value_copy` (candidate #2 in
a fixed order). The trigger must therefore be armed on the *copy* and the probe must fire on a
`__rt_decref_hash` of that live payload only — which is what the watchpoint sequence already
separated (cref from foreach copy, decref from the getEnv chain). For an EnvNotFoundException-shaped
free, the chain to re-measure is the SAME watchpoint run, not a separate first-hit.

lldb recipe that identified the class: `nm build-…/console | grep _instanceof_name_class_<id>`,
then `memory read -f x -c 6 -- <symbol address>` at a `main` breakpoint (ASCII name bytes).

## Audited and cleared (do not redo)

- **Property GET from a declared slot** (`emit_aarch64_box_property_slot`,
  `eval_property_helpers.rs:1322`): for `Array`/`AssocArray` it loads the payload and boxes through
  `__rt_mixed_from_array_kind`, which tail-calls `__rt_mixed_from_value` — that helper increfs the
  heap child. The box the eval side receives is therefore an extra owner, and releasing it returns
  to the object's count.
- **Property SET** (store paths, see the "Audited and cleared" entry above): the array store increfs
  the payload, the Mixed store increfs the box, the dynamic fallback retains the box.

Both directions of the object property bridge for arrays are balanced. What remains unbounded is the
eval→native call staging (arguments and receiver) and the identity of the object freed by
`EnvVarProcessor::getEnv`'s slot table.






## 2026-10-22 milestone: the secrets SIGSEGV is fixed by the borrowed-return lease

Rebuilt the console with the lease-fixed Magician (`candidate-compiler-diag1`, contains
`eval_declared_return_control_value` retaining borrowed `return $var;`), binary
`console-isolated.zi471L/build-diag1/console` (325 s user, no warnings):

- `secrets:list --reveal` **no longer crashes** (was SIGSEGV in `_rt_str_persist`). Clean exit 0,
  table rendered. The whole `array_len = free-list pointer` chain is gone.
- `secrets:set ELEPHC_PROBE2 <file>` works (writes `prod.ELEPHC_PROBE2.2986ad.php` and the list).
- `secrets:list` shows the secret; `secrets:reveal` still exits 2 ("does not exist") while PHP
  prints the value — the remaining divergence is now narrow: `SodiumVault::list(true)`'s
  `foreach ($secrets as $name => $value) { $secrets[$name] = $this->reveal($name); }` followed by
  `return $secrets` loses the keys in eval, but only with the console's full shape (reveal() calls
  `loadKeys()` which includes another file and `(string)`-casts a LazyString). A reduced pure-eval
  probe (`/tmp/evalrepro`: class W in eval'd code, `include data.php`, foreach-with-mutation,
  return, verify keys) passes on both PHP and Elephc — so the repro needs the extra include/`(string)`
  machinery inside the loop.
- Root cause of the old crash confirmed *after the fact*: measured refcount `0x80000000` at the
  container free was the free-path's own guard bit; the whole chain was the missing activation lease
  on the borrowed `return $secrets;` from `SodiumVault::list`, exactly what
  `eval_declared_return_control_value` now fixes (98 magician tests green, native regressions green).

Next: shrink `/tmp/evalrepro` toward the console shape (include of a *string-returning file* inside
the mutation loop + `(string)` of a lazy key) and fix the remaining keys-lost-in-eval divergence.and fix the remaining keys-lost-in-eval divergence.

## Reducer progress 2 (2026-10-22): shapes that PASS in eval

- `/tmp/evalrepro/probe.php`: class defined via `eval()` on read-back code, `include` of a
  string-returning file inside the mutation loop, `(string)` cast, noise loop over env-like arrays,
  receiver created by `new` in eval — **passes identically** on PHP and Elephc
  (`full=1:SECRET_A:V-SECRET_A17`, `exists-after-localVault: YES`).
- `/tmp/evalrepro/probe2.php`: the **nullsafe** shape (`$extra = $nullable?->noise();` after
  `$full = $nullable?->listAll(true);`, then `array_key_exists`) — **passes** on both.

Console observations retained: `secrets:list --reveal` renders a row for the secret (so that
`list(true)` HAD the key in `SecretsListCommand`), while `SecretsRevealCommand`'s
`array_key_exists($name, $secrets)` fails → the divergence is order/context dependent, and the
console's remaining untested differences are: (1) the receiver is a **native object with an eval
class overlay** (`runtime_class=stdClass` from DI) rather than an eval-created one; (2)
`sodium_crypto_box_seal_open()` (native bridge call) executed inside `reveal()` during the loop;
(3) the include path built from `$this->pathPrefix.'list.php'` (dynamic member concat).
Probe (3) first — it is a one-line change to `/tmp/evalrepro` (build the include path from a
property: `$this->dir . '/data.php'` with `private string $dir` set from the constructor).Probe (3) first — it is a one-line change to `/tmp/evalrepro` (build the include path from a
property: `$this->dir . '/data.php'` with `private string $dir` set from the constructor).

## 2026-10-22 measured trace of the failing secrets:reveal under ELEPHC_EVAL_TRACE=1 (console rebuilt with the lease fix)

With `ELEPHC_EVAL_TRACE=1` the rebuilt console *still* hits the double release in
`SodiumVault::list` when `SecretsRevealCommand` calls it (exit 1, `releasing dead array cell`).
The sequence in `/tmp/rv.trace` (lines 4495-4708), all fn=`SodiumVault::list`:

1. `foreach_subject value="value" owned=false borrowed=true` — the subject `$secrets` is the
   **borrowed include-result cell** (`0x10dff1680`).
2. `copy_array` (iteration copy) -> payload refs **2**.
3. loop body `array_set receiver=0x10dff1680` — the COW write through the receiver.
4. `release_array cell=0x10dff1680 refs=1` — the array_set statement's operand release
   (`eval_non_object_array_set_var_stmt` releases the retained receiver lease).
5. `release_array cell=0x10e02a940 payload refs=1` — the iteration-copy release.
6. `drain_cell cell=0x10dff1680 payload refs=4075746031` — the payload was **already freed** when the
   activation drain ran, and the panic fires.

So after the lease fix, the remaining bug is that between (5) and (6) the payload was freed once too
often — the include-result payload has exactly two owners (the scope cell `$secrets` and the
iteration copy), both released, and the *returned* value aliasing them had died. The next read is
`eval_include_expr`/`eval_owned_expr` for `include` inside a function with a declared `array` return:
whether the include result handed to `return`/foreach-subject carries its owner, and whether the
`include`-result slot in `list()` is classified as a store (`has_store`) by `function_cleanup_locals`
— `SodiumVault::list` has eval-replaceable locals (EvalScope), so its `PhpLocal` parameter/locals all
land in the release table via `owns_parameter_slot`'s `eval_can_replace_locals` clause.

## Open hypothesis: a lifetime pin defeating ensure_unique, and a build difference to check

`array_set`'s after-trace shows the payload pointer UNCHANGED by the COW write although the payload
had 2 owners — `__rt_hash_ensure_unique` decided in place. Its owner computation is
`refcount - pins <= 1` (`hash_ensure_unique.rs`), and hash pins live at `PINS_OFFSET = 48`
(`hash_pin.rs`: "each pin contributes one physical reference and one non-owner count").

Build difference worth checking first: the OLD console (`build-stdin-opt1-256m`) links
`__rt_hash_pin`/`__rt_hash_unpin` (nm visible) while the NEW one (`build-diag1`, plain flags, no
`--keep-symbols`) shows ZERO hash_pin symbols — the linked helper set differs between the two
builds, which can change both the symptom (old: SIGSEGV; new: only poisons under trace) and the pin
behaviour. Verify with `nm build-diag1/console | grep -c hash_pin` (0) vs the old build.

Next iteration, one command at a time:
1. `nm` comparison of the linked `__rt_*` sets between the two consoles (both `--keep-symbols`).
2. Rebuild diagnose-with-symbols, then run the pin probe (`__rt_hash_pin`/`__rt_hash_unpin`
   conditional on `$x0 == armed payload`) around the `SodiumVault::list` window: a pin between
   `copy_array` and `array_set` explains `ensure_unique` mutating in place while the payload had 2
   logical owners.
3. If confirmed, the fix belongs in the include-result path: the eval include must hand the caller a
   payload whose pin state matches a compiled `array` property-get, so an ordinary in-place write
   inside a by-value foreach cannot corrupt the iteration copy.

## Build-comparison result (measured): build-diag1 is NOT a comparable build

`nm` link-set comparison of the two consoles:
- `build-stdin-opt1-256m`: `hash_pin` x2, `ensure_unique` x3, `mixed_array_set`/`mixed_from_array_kind` present.
- `build-diag1`: **ZERO** `hash_pin`, **ZERO** `ensure_unique`, **ZERO** `mixed_array_set`/`mixed_from_array_kind`.

The diag1 console is missing the eval-bridge array helpers entirely: it was built by hand without the
exact flag/environment of the script console (no `--keep-symbols`, same cache), so the linker dropped
or replaced the eval-bridge runtime section. The "validation" of the lease fix on build-diag1 is
therefore NOT valid against the console's own runtime: its eval code runs against a different array
implementation. The SIGSEGV-fix claim made for build-diag1 must be considered UNVERIFIED.

Next iteration, first action: rebuild the console from the SAME script conditions as
`build-console-stdin-256m.py` but with `candidate-compiler-diag1` (--keep-symbols, matched env and
cache, 256 MiB heap), verify `nm` shows the full helper set (`hash_pin` 2, `ensure_unique` 3,
`mixed_array_set` present), and only then rerun `secrets:list --reveal`, `secrets:reveal`,
`secrets:decrypt-to-local`, `secrets:encrypt-from-local` against PHP.

**Measured on that comparable build (build-diag1-sym, nm-verified `hash_pin` 1, `ensure_unique` 3,
`mixed_array_set` 1): the lease fix alone does NOT fix the console.** With a fresh
`prod.list.php = []` and `echo value | secrets:set ELEPHC_RETRY -` (exit 0, list written),
`secrets:reveal ELEPHC_RETRY` still exits 2 ("does not exist") while PHP prints
`retry-secret-value` (exit 0). The COW/in-place + pin hypothesis on
`SodiumVault::list($reveal = true)` therefore stands in the REAL runtime: the
`foreach ($secrets as $name => $value) { $secrets[$name] = $this->reveal($name); }` mutates in place
because `refcount - pins <= 1`, and the iteration copy then frees the shared payload.

Fixed facts for the next session: the SIGSEGV crash mode is gone from the comparable build (the lease
fix removed the activation over-release of `SodiumVault::list`'s return), and the remaining bug is
the in-place COW violation. Next probe: pin watch (`__rt_hash_pin`/`__rt_hash_unpin` conditioned on
`$x0 == armed payload`) over the `SodiumVault::list(true)` window on build-diag1-sym.

## Active objective: maximize AOT, minimize eval

In addition to route/command parity, the campaign now targets compiling as much of the framework
as possible ahead of time instead of interpreting it in Magician. Concretely: make the generated
Symfony/Laravel container, its service factories and the runtime-warmed cache files part of the
AOT input (warm the cache as a build step, then compile the emitted PHP), then shrink the
`eval`/`include` fallbacks to genuinely opaque runtime input (`eval()`, dynamically written files,
reflection-driven includes). Every eval boundary removed also removes a class of ownership bug
like the one above.

## Immediate implementation order

1. Finish and freeze the current stdin compiler plus rebuilt Magician archive;
   rebuild the isolated standard console at the established 256 MiB heap and
   validate stdin secrets:set plus adjacent secret commands against PHP. The prior
   matched-heap array-storage artifact already passes cache:clear byte-for-byte;
   preserve that evidence and recheck it after further ownership changes.
2. Resolve the vault-list lifetime/count defect next, then the independently
   reproduced wrapped named-params notification callback crash and its PHP-event
   expectation. Continue remaining console behavior/side-effect failures and
   callable parameter metadata without treating an exit-zero warning as a pass.
   Recheck the two still-red table-reflection cases rather than assuming that the
   already-tested enum-array/default and native constructor fixes close Reflection.
3. Remove causes of false diagnostics, including the independently reproduced
   non-iterable foreach warning; do not suppress valid diagnostics to reach zero.
   The diagnostic and forwarding-array tranche now passes 10 driver, 41 EIR and
   17 native tests. Rebuild and recheck the actual framework entries with the new
   compiler. Preserve the canonical property layout and full return-chain
   inference; do not restore a hash-to-packed pointer relabel, blanket-widen
   unrelated function returns, or reduce the inference rounds to a fixed three.
4. Recompile all three standard web entries and both console families sequentially.
   Enumerate and compare routes, commands, exit statuses and isolated side effects.
   Report primary PDO/database blockers without changing PDO implementation here.
5. Fix remaining warning and lifetime causes exposed by those retries. Preserve
   framework/vendor sources. The current custom Twig home is not proof of the
   requested stock Welcome page; keep that separate acceptance gate explicit.
6. Continue measured code-size, build-time and memory reductions. The latest Demo
   console historically spent 368 s in native code generation alone. The newer
   Symfony console spends 96.97 s generating code and 57.70 s assembling despite
   five slice-cache hits; the web counterpart spends 96.12 s and 74.98 s with six
   hits. These are profiler targets, not controlled optimization gains. Shared
   runtime tables, bounded helper emission and split-plan/render costs must
   preserve behavior before their gains are accepted.
   Validate under load as well as at idle. Record wall/CPU time, peak RSS/footprint,
   generated bytes, cache state and concurrent work for paired measurements; a
   quieter machine alone is not an optimization.
   Missing-native-artifact preflight is also a measured resource target: the
   latest console emitted 938.6 MB of assembly and spent 332.73 seconds before
   discovering its absent locked PCRE2 artifact. Investigate an earlier read-only
   validation after final dependency planning, while preserving --emit-ir/--emit-asm
   workflows and dead-code/native-requirement pruning. No implementation is claimed.

Relevant producers/consumers: `src/resolver/declarations.rs`, `engine_includes.rs`,
`src/ir/source_units.rs`, `src/ir_lower/program.rs`, `stmt/mod.rs`,
`src/codegen_support/runtime/data/const_registry.rs`, runtime `rt_interface_exists.rs`,
`src/codegen/lower_inst/builtins/member_queries.rs`, `src/codegen/web.rs`, and
Magician `interpreter/statements/trait_declarations.rs` / `ffi/symbols.rs`.

## Historical regression inventory requiring current-source revalidation

| Boundary | Current evidence | Remaining work |
| --- | --- | --- |
| Native/dynamic once state | Three of four `native_eval_once` cases pass; native state callbacks executed on macOS ARM64 and Linux ARM64/x86_64 | Skipped native interface first included dynamically still fails; native source entry/provider remains absent |
| File-level functions | Three early-binding regressions pass; parent exception does not activate an unentered child | Conditional functions and same-file alternative implementations need distinct identities and activation |
| Direct native variant calls | Inactive calls throw catchable Error before arguments; methods, closures and 21 existing variant tests pass | Indirect-call timing, genuine redeclaration and complete request-lifetime audit |
| First-class native variant callables | Inactive creation is caught, later creation works, selected variant is invoked, unused creation retains Error | Full target audit and broader binding implementation still open |
| First-class neighbors | 108 passed, two failures; exact PHP reducers reproduce both errors with the older release compiler | Existing `sort` Mixed-receiver and associative-spread Heap(Array)/Heap(Hash defects remain open |
| Class-like activation | Interface event/table tranche is implemented but has not completed executable validation. The pre-fix probe was native `1:1:0` versus PHP `1:0:0`; event generation distinguishes plain/parent-linked/conditional interface sites | Validate corrected `1:0:0`; add class/trait/enum tables, declaration events, active queries, reset and non-existence consumers |
| Activation representation | Six AST/EIR tests pass: namespace/source preservation, no symbol reservation, reachability use, typed identity validation, explicit unsupported non-interface backend on all five targets | Interface runtime emission needs executable/cross-target validation; all non-interface native bindings remain pending |
| Interface diagnostics | Redeclaration diagnostic regression and 57 interface-filtered Magician tests pass | Diagnostic correction is not an activation fix |

Primary regression module: `tests/codegen/type_builtins/includes/declaration_activation.rs`.
Evidence logs: `target/first-class-binding-catch-tests.log`,
`first-class-binding-unused-test.log`, `first-class-binding-neighbors.log`,
`file-early-binding-includes-tests.log`, and `interface-activation-oracle/test.log`.
Activation representation checks: `target/classlike-activation-tests.log` (six pass).
Interface runtime attempt: `target/interface-activation-overlay-test.log` first
exposed a missing runtime feature; `overlay-build.log` exposed a case-normalized
cell mismatch. A later executable run still returned `1:1:0`: the activation table
lost the child marker after `throw` was eliminated, exposing a provenance gap. The
table now uses real declaration spans (not only surviving EIR events), and nested
control marker rewriting is broader. That correction is unexecuted: the subsequent
CLI build was terminated before linking at 398 MiB free, then its 944 MiB partial
debug artifacts were cleaned. This is not a functional pass.
Old-compiler comparison reducers/logs: `target/first-class-neighbor-oracles/`.

## Wider backlog retained, not declared closed

- Conditional functions: availability/callability mismatch, duplicate native symbols
  for same-name alternatives, and different signatures requiring the selected ABI.
- Conditional `define`, deferred enums, aliases, autoload order and source identities.
- Remaining GLOBALS/global-reference/hydration cases; successful reduced callbacks
  do not prove complete PHP symbol-table or request-lifetime parity.
- Reflection alias identity and method filename, closure/callable return representation,
  constructor parity, inaccessible mixed-property reads, GC/error-path ownership and
  strict heap leftovers documented in the historical implementation journal.
- Build reproducibility and per-function output nondeterminism; complete native/eval
  trace classification and missing capabilities must be rechecked, not inferred from
  old builtin-count tables.
- Compiler executable module duplication was removed: one measured release rebuild
  fell from 9m45 to 5m04. This is compiler rebuild evidence, not Symfony application
  compilation time or a statistically controlled benchmark.

## Validation and resource rules

Use one build/test job at a time: `CARGO_BUILD_JOBS=1`, `CARGO_INCREMENTAL=0`,
`RUST_MIN_STACK=67108864`; use `ELEPHC_ASM_JOBS=1` for reducers. Check active processes
and free disk before builds. Do not stop unrelated processes or discard dirty work.
Prefer focused tests and existing binaries; do not run the full suite by default.
Do not confuse emission checks, host execution, cross-target execution and HTTP proof.
Every runtime-affecting change needs coherent supported-target behavior.

Recheck owned build/test processes before launching another. Historical disk incidents: two CLI
rebuilds exhausted the available free space, even after debug cleanup. Historical
Symfony outputs `r130` through `r166-retry` were moved to the system Trash, preserving
`r167`, root logs and the release compiler; the Trash is on the same volume, so this
did not release its occupied bytes. Empty only those recoverable outputs or reclaim
space outside the worktree before another Cargo build. The AST patch was
completed and corrected after its first check exposed 27 omitted walkers; focused
AST/EIR tests pass. Kage recall for this stage timed out after 300 seconds; current
code/tests were used.
Recheck live state before resuming; do not infer completion from a log.

## Historical evidence

The following snapshots were archived without changing their bytes. References
inside them describe their historical locations and measurements, not current state.

- [Detailed implementation and test journal](archive/symfony/globals-native-eval-parity.md).
- [Original execution-map plan and trace inventory](archive/symfony/symfony-web-execution-ledger.md).
- [r149 debugger notebook](archive/symfony/symfony-r149-double-free-debugger.md).
- [Pre-implementation source inventory](archive/symfony/native-eval-inclusion-implementation-map.md).
- Superseded inclusion specs v1/v2 and five completed review-request documents are
  retained in `archive/symfony/`. Do not resume them as active plans.

Maintain only this current plan and distinct active specifications at `.plans/` root.
Record new measurements in `target/` and summarize their consequence here; do not
grow another append-only session journal or create one plan per build/review.
`globals-native-eval-parity.md` remains only as a short pointer for an existing Kage
citation; it is not an additional active plan.

`__rt_hash_pin`/`__rt_hash_unpin` conditioned on
`$x0 == armed payload`) over the `SodiumVault::list(true)` window on build-diag1-sym.

Pin probe result (2026-10-22): the probe DID NOT FIRE — the break on `___elephc_eval_value_copy`
(0x107257fa0, resolved, not pending) never hit during `secrets:reveal ELEPHC_PROBE5` on
build-diag1-sym (the run exits 2 without the callback reaching candidate 1), and consequently neither
pin nor unpin probes were armed. Open debugging items for the next session: (a) check whether this
build's eval `copy_value` goes through `___elephc_eval_value_copy` at all (the earlier
`ELEPHC_EVAL_TRACE=1` run on the old binary logged `copy_array fn=…::list`, so the hook exists); (b)
verify the lldb-attached run actually stops at breakpoint 2 (no "stop reason = breakpoint 2" was
seen); (c) fall back to arming on the FIRST candidate instead of candidate #2, since
`secrets:reveal` may only run a single counted copy.

Note: `secrets:reveal` still exits 2 / "does not exist" on build-diag1-sym (nm-verified full helper
set), so the in-place COW finding stands and remains the blocker.
Note: `secrets:reveal` still exits 2 / "does not exist" on build-diag1-sym (nm-verified full helper
set), so the in-place COW finding stands and remains the blocker.

## lldb probe abandoned in favour of a Magician-side diagnostic (2026-10-22)

The `___elephc_eval_value_copy` bridge (address resolved from nm) is **never hit** during
`secrets:reveal` on build-diag1-sym even with the `ELEPHC_` key filter fixed and arming on the first
candidate — the run exits 2 without the breakpoint ever firing. Instead of continuing to fight lldb,
the next probe must be **Magician-side**, built from the known-good recipe: add to
`ElephcRuntimeOps::copy_value` / `pin` hooks a trace that records, for the tracked payload (created
by `copy_value` with the ELEPHC_ key check), every `__rt_hash_pin` count change, and print
`payload / copy_state / pins / owners` at the end of `SodiumVault::list`'s activation. This is
build-independent (the eval copy hook demonstrably fires in the Magician-instrumented build) and
closes the address-reuse ambiguity by construction.

Deliverable of this session that remains valid: the comparable console
(`build-diag1-sym`, nm-verified), the lease fix (SIGSEGV mode eliminated), and the pinned
COW-in-place hypothesis for the remaining `secrets:reveal` divergence.
closes the address-reuse ambiguity by construction.

## copy-hdr instrumented run (build-diag1-sym + Magician with the copy-hdr trace + lease fix)

With `ELEPHC_EVAL_TRACE=1`, `secrets:reveal ELEPHC_PPROBE7` exits 2 WITHOUT any panic, and the
copy-hdr header dump shows: **11 array copies seen, every one with `pins=0`** (counts 1..69,
cap 8/10/0x3a/0x80, meta 7, kind 0x8003, rc 2..4). The second `SodiumVault::list` (the reveal
command's, line 1789) ends `success=true` at line 1894, yet the returned array loses its key
(`[ERROR] does not exist`), and the command terminates through `ErrorListener::onConsoleTerminate`
— a silent data loss, no release panic (the earlier 'releasing dead array cell' traces came from
the temporary drain instrumentation, which the revert removed).

Conclusion so far: no pins are observed on any array copy in the failing window; the in-place COW
needs a different explainer, OR the divergence is not the COW at all but the loop body's writeback
(`$secrets[$name] = $this->reveal($name)` in-place on a hash that the copy shares) combined with
`return $secrets` aliasing the *split* payload. Next instrumentation: the same copy-hdr dump in
`release`/`drain` paths (the format just added to copy_value), to watch the second list's payload
(0x10dff1920-class pointer) across its own activation and find the release that drops it to 0 while
the returned cell still aliases it.
the returned cell still aliases it.

## State of the failing sequence (read from the existing /tmp/prhd.trace, no new build)

The second `SodiumVault::list(true)` (trace line 1789) runs its foreach (count=1) and finishes
success=true at 1894; the reveal command then reports "[ERROR] does not exist" and terminates via
`ErrorListener::onConsoleTerminate`. No release panic this run.

Expected ownership after the loop, from the audited code: include gives the scope one owner;
`copy_value` shares the payload (2 owners); `array_set` COW-splits when `refcount - pins > 1`
(so the receiver takes a NEW hash, the shared old hash stays owned by the iteration copy); the
statement releases the retained receiver lease; the loop releases the iteration copy; the drain
releases the scope. With the split working, the returned `$secrets` (the receiver) keeps the new
hash — keys preserved. Nothing here loses a key, so one of these steps does not do what the code
says. The next measurement is therefore the COW branch itself: instrument
`emit_hash_ensure_unique` (add a one-line `__rt_diag_warning`-style trace with the payload pointer,
old refcount, pins, and the taken branch: split vs in-place) plus the same copy-hdr dump at
`drain_cell`. That single pair of traces will show whether ensure_unique split or mutated in place
on the failing payload.

Keep the copy-hdr implementation that exists (validated this run; 11 copies logged, pins=0, no
panic) and add the ensure_unique trace in the SAME rebuild — one Magician build, one console build.
one Magician build, one console build.

## Measurement already available at hook level: array_set did NOT split

The earlier instrumented `array_set` trace (before/after payload, in `ElephcRuntimeOps::array_set`)
ALREADY shows the decisive fact: on the receiving cell the payload pointer was UNCHANGED by the
in-place write (before == after), while `copy_array` reported refs 2 at the copy. So
`__rt_hash_ensure_unique` took the in-place branch on a payload that had 2 logical owners — the
pins/refcount subtraction, or the write targeting a deref'd reference layer rather than the shared
cell, is the defect. The native emitter instrumentation was attempted and REVERTED (adding a call
in the pure-asm helper risks register preservation); the safe replacement is the same before/after
dump in the Rust hook `ElephcRuntimeOps::array_set` (safe, build-independent), reading
`[payload-12] refcount`, `[payload+48] pins`, `[payload] count` before and after the native call —
this names which side of ensure_unique lies.

Fix candidates if confirmed: `__rt_hash_ensure_unique` subtracting PIN miscount under eval, or the
eval write bypassing the receiver retain that a compiled property-write performs (compare
`eval_non_object_array_set_var_stmt` with the compiled array-store slot in
`emit_aarch64_store_heap_property_slot`, which increfs the payload BEFORE the write — the eval
write's `values.retain(cell)` retains the CELL, not the payload).

## Final measurement of the session (array-set hook, build-diag1-sym, ELEPHC_EVAL_TRACE=1)

In the failing `SodiumVault::list(true)` window, `[array-set] recv=0x10d4e3830` fired twice and the
receiver cell kept the SAME payload pointer (0x10d4e2f30) after the native write — the COW did not
split despite a second logical owner (the foreach iteration copy). Caveat noted: the probe's `rc`
field read the incoming VALUE box's cell refcount, not the hash payload's owner count. Even so, the
receiver payload pointer being unchanged while a second logical owner exists confirms candidate (a):
`__rt_hash_ensure_unique`'s owner computation (`refcount - pins <= 1`) concluded <= 1 because the
iteration copy's share is registered as a lifetime PIN (`__rt_hash_pin` — one physical reference but
a non-owner for COW), not as a COW owner, or the pin is stale.

Fix direction: find which path registers the foreach iteration copy on the payload (a
`__rt_hash_pin` in the foreach machinery would make the in-place write correct FROM ensure_unique's
view but WRONG for PHP semantics if the loop then releases its physical share), and make the eval
`array_set` path perform the split exactly as the compiled `array` store does
(`emit_aarch64_store_heap_property_slot` increfs the payload before the write).

## Pin hypothesis REFUTED by code reading (2026-10-22)

All `__rt_hash_pin` call sites are inside the **mbstring internal machinery**
(`strings/mbstring/capture_hash.rs:69`, `query_remove.rs:54`, `query_enter.rs:47/74/99`,
`query_register/storage.rs:63`) — none in the foreach/array-write paths. The foreach iteration copy
cannot be registered as a pin by the array-write machinery.

Next iteration, first action (corrected probe): the `[array-set]` trace's `rc` field read the
incoming VALUE box's cell refcount and `before_payload` read the receiver cell — both are the wrong
targets to judge ensure_unique. Re-measure on the failing `SodiumVault::list(true)` write with:
`payload = [receiver + 8]` (the shared hash), `hash_rc = [payload - 12]` (u32), `pins =
[payload + 48]`, taken before and after `__elephc_eval_value_array_set`, keyed on `$x0 == payload`
armed from `copy_value` with the `ELEPHC_` key check, and identified by
`fn=Some("SodiumVault::list")` in the surrounding eval trace. If `hash_rc` is >= 2 and the branch
was in-place, `__rt_hash_ensure_unique`'s owner math is the defect; if `hash_rc` was 1, the second
logical owner was registered somewhere else and `copy_value`'s `mixed_clone` never incremented the
hash payload for the eval caller.

## FINAL decisive measurement (2026-10-22, correct hook, build-diag1-sym)

With the corrected `[hs]` probe (hash payload refcount read at `[payload-12]` before/after
`__elephc_eval_value_array_set`, key gated on the armed payload), the failing
`SodiumVault::list(true)` write measured **`hrc_before=1`** with `pins=0` and **no split** on every
relevant array_set (1894 hook calls total, zero with hrc>=2).

That is the defect: at the moment of the in-place write the shared hash had **ONE owner** — the
`foreach` iteration copy never registered its own payload owner. The loop-end release of that copy
(`execution_foreach_stmt`'s `values.release(iteration_array)`) then drops the payload to 0 and frees
it while the scope cell `$secrets` still aliases it — exactly the keys-lost/poisoned-hash sequence
measured before.

Concrete fix location: the foreach by-value iteration copy for a **borrowed subject cell** —
`execute_foreach_stmt` -> `values.copy_value(array)` (which returns a cell that owns the payload)
is NOT the path taken for this shape: the eval loop reaches `copy_scope_value`/`copy_array_value`
(the earlier trace's `raw_word_value <- copy_array_value <- copy_scope_value` frames), which shares
the payload **without an owner count**. Make that path register a payload owner (retain the child
like `__elephc_eval_value_copy`/`mixed_clone` does) or make the loop's end-release not decref the
shared payload — one of the two matches PHP semantics for by-value foreach.

After the fix: remove the temporary `[hs]` array-set probe and the `copy-hdr` trace, rebuild
Magician + console, and rerun the 33-command matrix against PHP.

## Owner-missing hypothesis ALSO refuted; the mechanism is now elsewhere (2026-10-22)

Reading the eval loop path: the by-value iteration copy goes through `copy_array_value`
(`runtime_hooks/ops/collection_calls.rs:167`, comment: "Detaches the array's boxed value while
retaining its payload for ordinary COW writes") -> `raw_word_value(tag, payload_word)` ->
`__elephc_eval_value_from_raw_word` -> `__rt_mixed_from_value` — and that helper DOES incref the
heap child (`mixed_from_value` calls `__rt_incref` for heap-backed payloads). So the iteration copy
does register a payload owner; the "no second owner" conclusion from the `[hs] hrc_before=1` probe
is refuted as well.

Remaining unresolved: why `[hs] hrc_before=1` was measured for writes whose copy had incremented.
Most likely the `[hs]` probe read the wrong payload: it read `[receiver + 8]` — the receiver cell's
payload after previous in-place splits could already be the CLONE (hrc 1, correct), while the write
that fails is a LATER write on a receiver whose payload was freed for another reason. The next
measurement must follow the SAME payload from `copy_array_value` (armed by payload address in the
eval hook, one line in `copy_array_value`'s Rust) to the failing write — keying on the copy's
returned payload address rather than on receiver cells.

Session close status: SIGSEGV mode fixed (validated comparable build); secrets divergence still
open with two refuted hypotheses; the reduced evidence base is in this ledger and the
`/tmp/evalrepro` reducers.
## Reconciling measurement and code: the writeback may be the loss (2026-10-22, end of session)

`copy_array_value` DOES incref (raw_word_value -> __rt_mixed_from_value), so `hrc_before=1` at a
later write is self-consistent if an EARLIER write already split the receiver: receiver = clone
(hrc 1, correct), original hash = iteration copy (hrc 1). Releasing the copy then frees the original
hash — correct, unless the SCOPE CELL still points at that original hash.

That is the remaining shape of the bug: `eval_non_object_array_set_var_stmt` computes
`in_place = scope_entry(...).cell() == replacement` and, when false, calls
`write_back_owned_variable_ref_target(scope, name, replacement, ...)`. If that write-back does not
publish the replacement into the scope (or publishes it into a borrowed entry that is later
overwritten), `$secrets` keeps the ORIGINAL hash; the iteration copy releases it; `return $secrets`
returns freed storage and the caller's `array_key_exists` fails. This explains the asymmetry between
the working `secrets:list` (the row came from the same `list(true)` but read before any later
allocation) and the failing `secrets:reveal`.

Next action (first thing after resume): verify `in_place` for the failing write by tracing
`scope_entry(...).cell()` vs `replacement` in `eval_non_object_array_set_var_stmt` (Rust, safe), and
check `write_back_owned_variable_ref_target`'s publish path for a scope entry that is `Borrowed`
(it returns early for in_place only). One line of trace + one rebuild decides it.

## Writeback hypothesis cleared by reading (end of session)

`write_back_owned_variable_ref_target` (`statements/reference_writeback.rs:361`) retains the value,
publishes it with `set_owned_scope_cell` and releases the replaced owners — the write-back itself is
sound. So `in_place` is likely true and the write lands in the scope cell. The remaining unexplained
step is why the caller's `array_key_exists` fails after a successful `list(true)`; next action is the
one-line `in_place` trace (`scope_entry(...).cell()` vs `replacement`) in
`eval_non_object_array_set_var_stmt` to see whether the scope cell is the receiver at all.

## Root cause of the secrets SIGSEGV / key loss — FIXED (2026-10-22)

The ownership hunt ended in the object layout, not in eval's array COW. Measured on
`build-current5` (probe trace + hardware watchpoint on the payload refcount, ASLR disabled):

`secrets:reveal ELEPHC_W11 --env=prod` armed `SodiumVault::list`'s `$secrets` payload, rc=2
(scope owner + foreach iteration copy). Immediately after the native `EnvVarProcessor::getEnv`
call the rc was 1, with this exact stack:

```
_rt_decref_hash
_rt_cleanup_invoke
_rt_object_free_deep
_rt_release_slot_table
elephc_xslice_L_f02e6e3bea60_604   (EnvVarProcessor::getEnv)
```

`__rt_object_free_deep` releases an object's dynamic-property tail with
`ldr x11, [x0, x9]` where `x9 = _class_object_payload_sizes[id] - 8`. For the freed
`EnvNotFoundException` (class id 464) the tables report payload `0x80` / tail flag `1`, so it read
`[obj+0x78]`. That slot had never been initialised and still held a recycled block's stale pointer:
the `$secrets` hash payload. Releasing it dropped the hash 2 -> 1; the foreach end then released the
iteration copy (1 -> 0) and freed the hash the scope cell still owned. `secrets:list --reveal`
dereferenced the same freed hash and SIGSEGV'd.

Why it was never initialised: allocation and free disagreed about the layout.

- `lower_builtin_throwable_new` allocated `8 + props*16 + allow_dynamic_properties*8`.
- `_class_object_payload_sizes` / `__rt_object_free_deep` use `8 + props*16 + tail`, where tail is
  `has_property_hash_storage()` (`allow_dynamic_properties || eval_property_storage ||
  clone_override_property_storage || scope_dynamic_property_storage`).
- `Program::reserve_eval_subclass_property_storage` marks every non-final, non-builtin class
  `eval_property_storage = true` in any program that links the eval bridge. So every compiled
  throwable subclass in such a program was allocated 8 bytes short and freed past its end.

Fix (one authority, so the two sides cannot drift again):
- `src/types/schema.rs`: `object_payload_size(class_name, info)`.
- `src/codegen_support/runtime/data/user.rs`: `class_object_payload_size` delegates to it.
- `src/codegen/lower_inst/objects/throwable_new.rs` (+ the Fiber dynamic-new table call site):
  `emit_throwable_allocation` takes the class name and uses the shared size.
- Regression: `throwable_payload_size_matches_the_free_layout_for_eval_owned_subclasses`
  (`src/codegen_support/throwable_layout.rs`).

Evidence after the fix (`build-current6`, compiler `target-f/release` 13:46 + instrumented Magician):
- `secrets:reveal ELEPHC_W11 --env=prod` -> `v10`, exit 0 (PHP: identical).
- `secrets:list --reveal --env=prod` -> same table and comments, exit 0 (was SIGSEGV 139).
- 33-command matrix (`probe-console.py` on `console-oracle.1VqPCG` vs `console-isolated.zi471L`,
  reports under each app's `matrix-fix2/`): 33/33 identical exit codes, 29/33 byte-identical stdout.
  Remaining differences are pre-existing and unrelated:
  - `list --format=json` and the `secrets:list` hint embed `$_SERVER['PHP_SELF']`; native reports its
    absolute executable path where `php bin/console` reports `bin/console` (executor path).
  - `about`: PHP version reports 8.5.0 vs the oracle's 8.5.10, Intl locale `n/a`, and the timestamp.
  - `debug:twig`: `yaml_dump` prints without its `(inline, dumpObjects)` parameters.
  - `debug:config`: byte difference only in a memory figure (normalised equal).

## Open console divergence: `debug:twig` prints `yaml_dump` without parameters (2026-10-22)

`debug:twig` prints `yaml_dump(inline, dumpObjects)` under PHP and `yaml_dump` (no parentheses)
under Elephc; `yaml_encode` matches. Both are `TwigFilter`s over a first-class callable of a
3-parameter method in `YamlExtension` (`encode`/`dump`).

Isolation evidence: reflecting the same closures from a compiled caller, from eval'd code, on a
compiled class, on an eval'd class, and on the real `Symfony\Bridge\Twig\Extension\YamlExtension`
loaded through the fixture's autoloader (compiled `twigdiag.php` in `console-isolated.zi471L`)
reports `type=Closure needsEnv=false needsCtx=false params=3` for BOTH filters under both runtimes.
So the closure and filter metadata are correct; the empty list comes from the compiled
`Symfony\Bridge\Twig\Command\DebugCommand::getMetadata()` path (it ends up shifting one parameter
too many, or reflecting a 1-parameter callable). Not reproduced yet in a reducer; left open.

## Console matrix after the throwable-layout fix (2026-10-22, `build-current6`)

`probe-console.py` run twice into `matrix-fix2/` after resetting both app copies to the same
baseline (`config/secrets/prod/prod.list.php` emptied, leftover probe secrets and `.env.*.local`
removed, `ELEPHC_W11` re-sealed as `v10` with each copy's own keys). Reports:
`console-oracle.1VqPCG/matrix-fix2/` and `console-isolated.zi471L/matrix-fix2/`.

- 33/33 identical exit codes (`secrets:generate-keys` exits 1 on both: existing keys).
- 29/33 byte-identical stdout; the secrets family is fully identical
  (`secrets:reveal`, `secrets:list`, `secrets:decrypt-to-local`, `secrets:encrypt-from-local`,
  `secrets:remove`, `secrets:set`, `secrets:generate-keys`).
- Remaining byte differences, all pre-existing:
  - `list --format=json`: `$_SERVER['PHP_SELF']` is the canonical absolute `bin/console` path
    natively and the relative `bin/console` under `php bin/console`; the command list itself is
    identical (33 names, same order).
  - `about`: PHP version `8.5.0` (compiled profile) vs `8.5.10`, Intl locale `n/a`, timestamp.
  - `debug:twig`: `yaml_dump` printed without `(inline, dumpObjects)` (open item above).
  - `secrets:list` stderr hint embeds the same `PHP_SELF` path; `debug:config` differs only in a
    memory figure (normalised equal).

## Web entry: the recorded 194 KB 404 was a different fixture (2026-10-22)

`probe-routes.py` against the pristine `examples/symfony-app` with the PHP built-in server
(`php -d variables_order=EGPCS -S ... public/index.php`) returns:

| route | status | bytes |
| --- | --- | --- |
| GET / | 200 | 695 |
| GET /plain | 200 | 33 |
| GET /greet/World | 200 | 12 |
| GET /echo?x=1&y=two | 200 | 70 |
| POST /echo | 200 | 71 |
| GET /no-such-route | 404 | 993 |

The 404 body is the prod fallback page (sha256 `25d34096…`), byte-identical to the body the
native build produced in `native-fs23-missing-prod`. The 194,072-byte 404 in the older fs23
evidence belongs to a different fixture, not to this application. Route-by-route native
comparison follows once `symfony-web-current` finishes linking.

## Web route parity on the standard entry (2026-10-22, `symfony-web-current`)

Built from the pristine `examples/symfony-app/public/index.php` with the current compiler
(`candidate-compiler-diag1/elephc`, release 13:46): 229,006,856-byte executable, 935,144,301-byte
assembly, 454 warning lines, ready at 14:14. Runs through `probe-routes.py`:
`php-routes-prod/` (PHP built-in server) vs `native-routes-prod/` (`index --listen`), both with
`APP_ENV=prod APP_DEBUG=0`.

All six routes match status AND SHA-256 byte for byte:

| route | status | bytes |
| --- | --- | --- |
| GET / | 200 | 695 |
| GET /plain | 200 | 33 |
| GET /greet/World | 200 | 12 |
| GET /echo?x=1&y=two | 200 | 70 |
| POST /echo (a=1&b=two) | 200 | 71 |
| GET /no-such-route | 404 | 993 |

Dev mode (`APP_ENV=dev APP_DEBUG=1`) exposes a missing builtin: the debug error renderer calls
`highlight_file()`, which Elephc does not implement, and Magician reports
`Fatal error: Call to undefined function
symfony\component\errorhandler\errorrenderer\highlight_file()` (the namespaced spelling), then the
worker exits 1. `highlight_file`/`highlight_string` are not in the builtin catalog. Prod responses
are unaffected. Open item; it also shows Magician does not fall back to the global function name
for an unqualified call inside a namespace when the function is unknown to the catalog.

## AOT of the generated container (2026-10-22, `symfony-web-preload`)

`--ini opcache.preload=<abs>/var/cache/prod/App_KernelProdContainer.preload.php` makes
`pipeline.rs` inject an implicit `require_once` of that file at the top of the entry; the resolver
inlines the container plus the ~180 `ContainerXvDiqOo/get*Service.php` units it requires, so they
are compiled into the binary instead of being interpreted when `Kernel::initializeContainer`
`require`s them at runtime. `PHP_SAPI` is `cli-server` in `--web`, so the preload file's
`in_array(PHP_SAPI, ['cli','phpdbg','embed'])` guard lets it through — exactly like PHP's web SAPI
(and it correctly short-circuits in the CLI console, matching PHP).

Evidence:
- Executable: 282,544,312 bytes versus 229,006,856 without preload (+53.5 MB of compiled units).
- All six routes still match PHP byte for byte (`native-routes-preload/` vs `php-routes-prod/`).
- Cold request: 0.122 s -> 0.065 s (second request 0.058 s both ways), measured on the same
  machine, one worker, `APP_ENV=prod APP_DEBUG=0`.
- `ELEPHC_EVAL_TRACE=1` on the preload binary shows the preload-only `Preloader` class dispatching
  natively (`method_call_start method="getDefaultProperties"`), i.e. it is compiled; the same run
  produced **zero** `dynamic_method_start` (interpreted dispatch) lines across 78k trace lines,
  where the pre-preload build logged 352 interpreted method dispatches for one `/` request.
  The traced request itself cannot be completed for a line count: the preload file's
  `Preloader::preload()` pass emits trace lines per reflected class (~600 lines/s), so a full
  traced request takes tens of minutes. The untraced request is unaffected.

Remaining eval boundary (web, prod):
- the entry script itself (`public/index.php`) and its statically-unknown dynamic includes;
- Twig templates compiled at runtime under `var/cache/prod/twig/*.php` and `require`d from
  `TemplateCache`/`FilesystemLoader` (the render path still travels the interpreted include);
- anything the preload list does not name (e.g. the console-only `DebugCommand`).
The preload file body also runs once per worker (its `require_once`s collapse, and the
`Preloader::preload()` pass is the startup cost inside the first request) instead of once per
process as in PHP; hoisting that to process start is a follow-up.

## Compilation and size measurements (2026-10-22, release compiler 13:46)

Both builds use one assembler worker (`ELEPHC_ASM_JOBS=1`), a warm runtime cache
(`XDG_CACHE_HOME=<audit>/runtime-cache`, reported `Runtime cache: hit`), no host contention, and
NO `--keep-symbols` (the production shape). `/usr/bin/time -l`:

| build | wall | user | sys | max RSS | executable |
| --- | --- | --- | --- | --- | --- |
| `bin/console` (`console-timed/`) | 31.45 s | 53.27 s | 1.73 s | 4.48 GB | 178,258,472 B (170 MiB) |
| `public/index.php --web` + container preload (`symfony-web-timed/`) | 40.43 s | 71.30 s | 2.04 s | 5.21 GB | 216,830,248 B (207 MiB) |

Both are under a minute of wall clock on one assembler worker, and both executables are below the
250 MiB acceptance limit. The user-time column is the honest one-core cost: the console is 55 s
(single-core equivalent) and the web build 73 s, because codegen itself runs on ~3 threads.
`strip -x` on the 216.8 MB executable changes it by 8 bytes: it is already stripped, so leaving
`--keep-symbols` off is the size lever (it costs +65.7 MB on the web build and +51 MB on the
console build used during the campaign).

Per-phase table for the web preload build (`--timings`, `symfony-web-timings/`, 1.18 GB / 44.17 M
assembly lines):

| phase | wall | CPU | share |
| --- | --- | --- | --- |
| Generating native code | 13.16 s | 39.84 s | 32.4% |
| Checking types | 7.82 s | 7.81 s | 19.2% |
| Lowering program to EIR | 5.73 s | 5.72 s | 14.1% |
| Assembling object file | 5.83 s | 10.00 s | 14.3% |
| Pruning unreachable declarations | 2.80 s | 3.81 s | 6.9% |
| Optimizing EIR | 1.48 s | 1.51 s | 3.6% |
| Discovering autoloaded symbols | 0.96 s | 0.96 s | 2.4% |
| Propagating constants | 0.89 s | 0.89 s | 2.2% |
| Linking native output | 0.58 s | (<0.1 s) | 1.4% |
| **Total** | **40.64 s** | **71.63 s** | 100% |

The emitted assembly contains only 159 comment lines, so no comment-stripping win exists: the
volume is 44 M instructions. Native-code generation is the dominant cost and the honest target
for any further single-core reduction; the rest of the pipeline (front end + EIR) is 20 s of the
40 s wall clock.

Both production-shape binaries were re-checked against PHP after linking: `console-timed/console`
(`secrets:reveal` -> `v10`, exit 0; `secrets:list --reveal` exit 0) and `symfony-web-timed/index`
(6/6 routes byte-identical, `native-routes-timed/`).

## `debug:twig` yaml_dump parameters — FIXED (2026-10-22)

Root cause: resolving a method first-class callable by its bare name. `YamlExtension::getFilters()`
builds `$this->dump(...)` while Symfony's var-dumper also defines a global `dump(...$vars)`.
`eval_reflection_callable_new` (and `eval_reflection_function_method_target`) looked the closure up
in the function tables by its bare display name BEFORE trying its method target, so
`(new \ReflectionMethod($cb, '__invoke'))->getParameters()` answered with the global's single
variadic (`vars`) instead of the method's three parameters; `DebugCommand` then shifted the one
value parameter and printed `yaml_dump` with no argument list.

Reduced to a 20-line eval program (`/tmp/nativeeval/refl9.php`): a global `function dump(...$vars)`
plus a class method `dump($value, $inline = 0, $dumpObjects = 0)`; PHP reports 3 parameters for the
method closure, Elephc reported 1 (`vars`).

Fix: when the closure target names a METHOD (`ObjectMethod`, `StaticMethod`, `InvokableObject`,
or a `ForeignContext` wrapping one), skip the by-name function-table lookups and go straight to the
method metadata. Applied in both resolution sites
(`reflection/function_construction.rs`, `reflection/function_metadata.rs`).
Regression: `execute_program_method_closure_reflection_ignores_a_same_named_global_function`
(`interpreter/tests/dynamic_calls/first_class_objects.rs`).

Verification (`console-fix1`, production shape without `--keep-symbols`):
- `debug:twig --filter=yaml` -> `yaml_dump(inline, dumpObjects)` and `yaml_encode(inline,
  dumpObjects)`, identical to PHP.
- Full Magician suite: 2175 passed, 0 failed, 2 ignored.
- 33-command matrix reset to the same baseline (`matrix-fix3/`): 31/33 commands byte-identical,
  3 remaining textual differences, all environmental or execution-path:
  - `list --format=json` (and the `secrets:list` stderr hint): `$_SERVER['PHP_SELF']` is the
    canonical absolute `bin/console` natively versus the relative `bin/console` under
    `php bin/console`; the 33 command names and their order are identical.
  - `about`: the baked PHP profile reports `8.5.0` versus the oracle's `8.5.10`, `Intl locale` is
    `n/a`, and the timezone line carries the wall clock of each run (PHP against itself differs
    there too).

## Final artifacts of this pass (2026-10-22)

Both are production-shape builds (no `--keep-symbols`), one assembler worker, warm runtime cache,
and link the fixed Magician (closure-name collision) plus the shared throwable layout.

| artifact | build | size | verification |
| --- | --- | --- | --- |
| `console-fix1/console` | 31 s wall (no symbols) | 178,291,352 B (170 MiB) | `matrix-fix3`: 31/33 byte-identical; `debug:twig` identical; secrets family identical; `secrets:list --reveal` stdout+stderr identical |
| `symfony-web-final/index` | 40.40 s wall / 71.02 s user, 5.21 GB RSS | 216,863,144 B (207 MiB) | `native-routes-final`: 6/6 routes identical status+content-type+SHA-256, including the 404 |

Residual console differences (documented, not app behaviour):
- `$_SERVER['PHP_SELF']`: canonical absolute `bin/console` natively (so `autoload_runtime.php` can
  `require` it from any cwd) versus the invoked relative spelling under `php bin/console`. This is
  the only content difference in `list --format=json` and in the `secrets:list` hint.
- `about`: baked PHP profile `8.5.0` vs the oracle's `8.5.10`, `Intl locale n/a`, and the timezone
  line's wall clock (PHP against itself differs there too).

## `spl_autoload_functions()` and the missing preload file — FIXED (2026-10-22)

The matrix's only side-effect divergence was `cache:warmup`/`cache:clear`: PHP regenerated
`var/cache/prod/App_KernelProdContainer.preload.php`, Elephc did not. Symfony's
`PhpDumper::getAutoloadFile()` walks `spl_autoload_functions()` to find the Composer loader.

Root cause: compiled `spl_autoload_functions()` never asked the runtime. `lower_spl_autoload_functions`
emitted a compile-time array of autoload RULE INDEXES (nonsense to userland, and empty when no
rule exists), while compiled `spl_autoload_register` registers through
`__elephc_eval_register_spl_autoload` into the interpreter's request-global queue. Two answers,
one queue.

Fix:
- Magician exports `__elephc_eval_spl_autoload_functions(ctx)`, which builds the queue as an owned
  boxed indexed array (`interpreter::registered_spl_autoload_functions`).
- `lower_spl_autoload_functions` calls it whenever the module has the eval bridge, unboxes the
  returned box, gives the caller its own owner of the array payload, releases the box, and stores
  the payload. Without the bridge the answer is an empty array (the compiler's own rules are not
  PHP callables).
- Regression: `test_spl_autoload_functions_reports_runtime_registrations`
  (`tests/codegen/spl/autoload.rs`), oracle `true|1|array|load|2`.

Evidence:
- `console-fix2 cache:clear` on a fresh container writes the 190-line preload file; the pre-fix
  `console-fix1` on the same fresh cache does not.
- After the matrix both copies' preload files are 190 lines and identical once the generated
  container id and app path are normalised; `config/secrets/prod` names, `prod.list.php` and
  `.env.prod.local` are identical.
- `matrix-fix4` (`console-oracle.1VqPCG/matrix-fix4` vs `console-isolated.zi471L/matrix-fix4`):
  29/33 commands byte-identical including `debug:twig`; the four byte differences are
  `list`/`secrets:list` (`PHP_SELF` execution path), `about` (PHP profile version, intl locale,
  wall clock) and `debug:config` (a memory figure; equal after normalisation).
- Final web build `symfony-web-final2/index` (222,775,816 B): 6/6 routes identical
  (`native-routes-final2`). The build took 70.2 s wall in that run (99.9 s CPU) versus 40.4 s
  earlier — the runtime object and slice caches were invalidated by the Magician change; the
  figure to quote is the warm-cache one, and both are measured with one assembler worker.

## Dev error page: PHP-exact highlighter (2026-10-22, in progress)

`highlight_file`/`highlight_string` are absent, which is why `APP_DEBUG=1` error pages die with
`Call to undefinefunction symfony\component\errorhandler\errorrenderer\highlight_file()`.

Implemented `crates/elephc-magician/src/interpreter/highlight.rs`: a byte scanner that reproduces
PHP's highlighter output — class-to-colour mapping (#0000BB names/variables/numbers/tags,
#007700 keywords/operators/casts/heredoc markers, #DD0000 strings, #FF8000 comments, inline HTML
with no span), span coalescing, whitespace inheriting the previous colour, `?>` closing PHP even
inside a `//` comment, `&`/`<`/`>` escaping only, and tab expansion to four spaces.
`highlight_document` wraps it in `<pre><code style="color: #000000">`.

Verified against reference PHP on 32 corpus cases (inline HTML, open/echo/close tags, `//`/`#`/
block/doc comments, single/double-quoted strings with `{$…}`/`$a[…]`/`$b->c` interpolation,
heredoc and nowdoc with indentation, numbers, casts, attributes, qualified names, operators,
`__halt_compiler`, CRLF, tabs, empty input). All pass:
`cargo test -p elephc-magician --lib highlight::tests` -> 32 passed.

Next step: register `highlight_string`/`highlight_file` in the shared contract, add the AOT home
files and `RuntimeFnId`s, the Magician `eval_builtin!` bindings, and the AOT bridge call (the
builtin forces the eval bridge; Magician returns the boxed string and the lowering transfers its
owner, as `spl_autoload_functions` does for arrays). Until that lands, the module carries a
documented `#[allow(dead_code)]` so the crate stays warning-free.

### Dev parity after wiring the highlighter (2026-10-22)

`highlight_string`/`highlight_file` are now ordinary builtins end to end: shared-contract entries,
`RuntimeFnId::{HighlightFile, HighlightString}`, AOT home files, `EvalValuesHook::Highlight` /
`EvalDirectHook::Highlight`, the interpreter renderer, and `__elephc_eval_highlight_{string,file}`
(borrowed subject cell, owned boxed `string|bool` answer, which is exactly the `Mixed` repr of the
declared union, so the AOT lowering is `store_if_result` with no unboxing). The eval bridge is
forced for both, like `spl_autoload_functions`. The highlighter also gained the fixed
fully-qualified-name rule (`catch (\Throwable` keeps the backslash in the NAME's run): 33/33 oracle
tests, plus `tests/codegen/strings/highlight.rs` (AOT, eval, and `highlight_file` byte-identical to
reference PHP 8.5).

Dev measurement with the fresh compilers (`php-routes-dev3` vs `native-routes-dev10`, `APP_ENV=dev
APP_DEBUG=1`):

| route | PHP bytes | native bytes | result |
|---|---|---|---|
| `GET /` | 695 | 695 | identical |
| `GET /plain` | 33 | 33 | identical |
| `GET /greet/World` | 12 | 12 | identical |
| `GET /echo?x=1&y=two` | 70 | 70 | identical |
| `POST /echo` | 71 | 71 | identical |
| `GET /no-such-route` | 194,074 | 180,875 | 23/25 frames, text identical apart from the frames |

The dev worker survives the 404 (no SIGSEGV, no `Uncaught` worker exit). The only remaining 404
difference is the `require_once('/…/vendor/autoload_runtime.php')` include frame (it appears twice,
once at the end of each exception's trace) and its three `getTraceAsString()` lines. Everything
else — the trait throw site (`CompiledUrlMatcherTrait.php:70`), the closure frame name and `::`
type, every kernel frame, the highlighter excerpt of each frame, the timestamp — matches. The
remaining differences after normalising the port, the random Sfdump id and the clock are exactly
those two frames.

Fixes that took the page from 130,705 B to this state:

1. **AOT closure frames** (`src/codegen/lower_inst/core_builtins/backtrace.rs`): a compiled closure
   body reported the internal `__eir_closure_*` symbol with no class. The frame now reuses
   `core_closures_php_source` (`{closure:SCOPE:LINE}`, nested scopes included) and reports the
   lexical class plus `::`/`->`, with the closure's own `static function` flag threaded through
   `ir_lower` (`Function.flags.is_static`). Regression:
   `test_compiled_closure_trace_frames_match_php_names_and_call_types`.
2. **Suspended compiled frames in a throwable trace**
   (`crates/elephc-magician/.../core/debug_backtrace.rs`): when eval frames were live, the
   interpreter's trace replaced the compiled chain, so an exception created in eval'd framework
   code listed 3 frames instead of 13. The builder now appends `runtime_backtrace_entry(…)`, the
   same suspended-activation reader `debug_backtrace()` uses. A `new` inside eval now reports the
   whole caller chain.
3. **Trait/interface/enum declaring files across eval contexts**
   (`context/classes_aliases.rs`): the class loop imported `class_source_files` from the global
   registry but the trait, interface and enum loops did not, so a trait method ran with no source
   file and `throw` reported the caller's file with the trait's line. Also
   `dynamic_class_method_site` now falls back to the generated reflection metadata for an AOT
   class-like, and an eval'd include's declarations keep the include's file instead of the outer
   eval marker (`executing_include` guard in `eval_declaration_source_file`).
4. **Include construct frames** (`interpreter/include_exec.rs`): an interpreted
   `include`/`include_once`/`require`/`require_once` now pushes a frame named after the construct,
   positioned at the caller and carrying the evaluated path, exactly like PHP. (Uncovered: a
   STATIC include the resolver inlined at build time, which is why the `autoload_runtime.php`
   frame is still missing — see below.)

Tooling/link hygiene learned the hard way:

- **A stale bridge archive can win symbol resolution over the fresh one.** `libelephc_web.a`
  contains its own copy of `elephc-builtin-contract`, so linking a fresh `libelephc_magician.a`
  against the 09:23 `libelephc_web.a` made `lookup_id` answer from the OLD catalog and the worker
  panicked with `eval builtin binding references unknown shared contract ID … from
  …/highlight_string.rs`. Rebuild/copy *all* `libelephc_*.a` from one `cargo build --release` into
  the candidate directory, not just Magician.
- `asm_without_embedded_script_path` (tests/codegen/support/compiler.rs) now drops every `.ascii`
  line holding the script path, not only the one after `_script_source_file:` — a frame reader's
  entry-file descriptor embeds the same bytes, and the temp directory is named after the test, so
  `!contains("shadowed")` matched the path instead of a surviving catch body.
- Disk: removed the stale worktree `target-idx`, `target-idx2`, `target-int` and `target-tests`
  trees (23.9 GiB) after the volume hit 100%; ~21 GiB free now.

Next on this axis: a frame for compiler-inlined static includes. `public/index.php`'s
`require_once dirname(__DIR__).'/vendor/autoload_runtime.php'` is folded to an absolute path and
inlined, so no runtime include exists and neither the AOT chain nor the interpreter frames can list
it. It needs a synthetic activation (or an include-chain record) emitted by the compiler for each
inlined include: `function =` the construct, `file`/`line` = the include site, `args = [path]`.

### Compile-warning census after the retype-policy change (2026-10-22)

A local-retype warning now fires only when the retype crosses the scalar/heap storage boundary
(`$a = 1; $a = "s";` — the documented `--strict-locals` case — still warns; `object` to `object`,
`string` to `string|false`, `array` to `string` are silent because both values are the same pointer
slot). This is a policy change, not a suppression: the retype DECISION and its abandoned slot are
unchanged, so lowering and runtime behavior are identical. Coverage:
`test_retype_warning_only_names_a_storage_class_change` plus the updated
`test_a_lent_reference_does_not_veto_the_widening` and
`test_every_lowering_fixture_takes_the_retype_path`.

Fresh `--web` build (`symfony-web-warn1`, EXIT 0, warm caches):

| class | before | now |
|---|---|---|
| total `warning:` lines | 604 | 345 |
| local retype | 199 | 23 |
| unused variable | 326 | ~322 |
| unreachable / tentative-return / non-iterable foreach | 77 | unchanged |

The 23 remaining retypes are genuine scalar↔heap or scalar↔union crossings (`int` to `string`,
`false` to `string`, `int` to `int|string`, `array` to `null`) — exactly the diagnostics the CLI
documents. Removing those to reach zero still needs a decision: either make the warning opt-in
(new `--warn-locals`, with `--strict-locals` still the error) or have the checker bind such locals
as the joined type from the first store. That is the next clean-build item, together with the
unused-variable family.

Pre-existing failures observed while validating this change (NOT caused by it — reproduced with the
guard temporarily disabled; they belong to strict-locals/marking work already in the worktree):

- `codegen_tests locals_retype`: `test_marked_local_across_switch_arms_warns_and_is_strict_rejected`
  and `test_marked_local_piped_into_a_known_by_value_target` — `--strict-locals` returns `Ok([])`
  instead of `cannot reassign`.
- `error_tests type_system`: 12 failures in the same family (`test_branch_divergent_assignment_
  errors_under_strict`, `test_marking_survives_a_type_guard_control_under_strict`,
  `test_unset_blocks_marking`, `test_mixed_storage_marking_at_dummy_spans_is_refused`,
  `test_an_is_array_guard_does_not_produce_false_advice`, three
  `test_error_typed_by_ref_parameter_rejects_*`, `test_error_short_circuit_assignment_effect_is_not_
  definite`, `test_declared_array_return_hint_preserves_method_and_property_contracts`,
  `test_null_coalesce_keeps_the_default_of_storage_and_undeclared_calls`,
  `test_unresolved_pipe_target_arg_not_killable`). Two failures in that set were fixed by this
  change's test updates; the other twelve predate it and are a strict-mode/marking regression to
  schedule.

### Console and web matrices with the current compiler (2026-10-22)

**Production web**: `symfony-web-final3/index` (213,080,040 B) against `php-routes-prod` — **6/6
responses byte-identical**, 404 included (status, Content-Type, SHA-256). Build: 345 warning
lines, warm asm cache.

**Production console**: rebuilt `console-isolated.zi471L/build-current/console`
(174,440,000 B, **31.7 s wall / 52.8 s user**, one assembler worker, 303 warning lines), then
`probe-console.py` on both copies. 33/33 exits identical (`secrets:generate-keys` = 1 on both,
everything else 0), and every stdout/stderr identical after normalising the app path, the clock,
memory/durations and the emulated PHP version. The three remaining `compare-console.py`
differences are all the same execution-path fact: Symfony prints `$_SERVER['PHP_SELF']` in command
help and in the `secrets:list` hint, so PHP (invoked as `php bin/console`) prints `bin/console`
where the binary (invoked by absolute path) prints its own absolute path. That is the campaign's
documented `PHP_SELF` gap, not a behaviour difference.

**Dev console** (new `probe-console-env.py`, a copy of the probe with `--env/--debug`): 33 commands
on both copies from a cold `var/cache/dev`. Three real gaps remain, all first-hit during the
**dev container build**, and three fixes landed while chasing them:

1. **Constructors are exempt from signature variance.** `autoload_pending_class_variance_types`
   autoloaded every type named by an overriding method, including `__construct`. PHP does not check
   constructor signatures, so it never resolves them; Symfony's
   `ProfilerExtension::__construct(?Stopwatch $stopwatch = null)` therefore declared fine without the
   Stopwatch package, while elephc autoloaded it and aborted the container build with "Class
   Symfony\Component\Stopwatch\Stopwatch not found while loading ProfilerExtension".
2. **`class_implements()`/`class_parents()` know the language-level `Closure` class.**
   The relation builtins asked the generated metadata directly, which has no row for `Closure`, and
   answered `false` instead of `[]`; `DebugClassLoader::getOwnInterfaces()` then ran
   `foreach (false)` ("foreach() argument must be of type array|object, false given"). They now use
   the same declared-class authority as `class_exists()` (which already special-cased `Closure`).
3. **A reflected property default evaluates `self::` in its DECLARING class.** Only the magic-constant
   scope was pushed, so `MetadataBag::$meta = [self::CREATED => 0, …]` reflected from
   `DebugClassLoader` resolved `self` to `DebugClassLoader` and failed with "Undefined constant
   DebugClassLoader::CREATED". The class and called-class scopes are now pushed around the default
   expression.

Regressions added: `test_eval_class_relations_know_the_closure_class`,
`test_reflected_property_default_self_uses_the_declaring_scope` (both AOT+eval, PHP-verified).

Still failing in the dev console after those fixes (recorded for the next pass):

- `Definition::getErrors()` iterates `$this->errors` and finds a **string**: during
  `PhpDumper->addService()` a `Definition` whose typed array property holds a string. `Definition`
  is AOT-compiled in the console binary (the asm carries
  `@fn name=Symfony\Component\DependencyInjection\Definition::addError`), while the object is built
  by the container builder; the likely class is an AOT-metadata/interpreter property-layout or
  typed-property append mismatch, not an interpreter-only bug (a pure-eval reproduction of
  `addError`/`getErrors` is byte-identical to PHP).
- `debug:config`/`lint:container`: `file_get_contents(): Failed to open stream` from
  `BuildDebugContainerTrait.php` / `ContainerLintCommand.php`.
- `error:dump` (all status codes): `stream_get_contents(): Argument #1 must be of type resource,
  false given` — `fopen('php://memory', 'r+')` returned false in that process. Single-code dumps
  succeed, so it is tied to the cold dev-container build path rather than the wrapper itself.

**Dev web**: unchanged and re-verified with this compiler — 5/6 routes byte-identical; the 404 page
is identical apart from the two `require_once` include frames (23/25 frames), the port, the random
Sfdump id and the clock.

### Final-build verification (2026-10-22, console `build-current`, web `symfony-web-final3`)

- Console: 31.19 s wall / 52.76 s user, 174,424,072 B, 303 warning lines.
- Web: 213,080,040 B, 345 warning lines; prod routes 6/6 byte-identical (sha256).
- Console prod matrix (fresh `matrix-prod5` on both copies): 4 differences of 33 —
  `list`, `about`, `secrets:list` (the documented absolute-vs-relative script path plus the
  emulated PHP profile/intl facts) and **`cache:clear`**.
- `cache:clear` exposed that the `Definition::$errors` corruption is **not dev-specific**: once the
  container cache is cold in prod, `PhpDumper->addService()` calls `Definition::getErrors()` and the
  compiled loop warns `foreach() argument must be of type array|object, string given` (the command
  still exits 0 in prod). With a warm prod container the command is clean, which is why earlier
  prod matrices reported 33/33.
- The compiled `Definition::addError` in `console.s` reads/writes the `errors` slot at object offset
  312 and its typed-property stamp at 320, and stores the default as a **boxed Mixed container**
  (`__rt_mixed_box_container_4`, `__rt_mixed_clone`, `__rt_mixed_array_append`). A pure-eval
  reproduction of the same class (`private array $errors = []`, `addError` with
  `string|\Closure|self`, `getErrors`) is byte-identical to PHP, so the corruption is specific to
  the AOT-compiled `Definition` being instantiated/populated during the container build. Next
  investigation: the typed-property default representation the interpreter writes when it creates
  an object of an AOT class (raw array vs boxed Mixed cell), and whether the compiled constructor
  or the interpreter initializes those slots in the failing path.

#### `Definition::$errors` investigation (continued)

Findings, in the order they were established:

- The failure is **not dev-only**: `APP_ENV=prod ./console cache:clear` with `var/cache/prod`
  removed exits 1 with the same `In Definition.php line 814` warning. Earlier prod 33/33 matrices
  were run against a warm prod container; the console's acceptance must clear the container cache
  to expose it.
- The failing call is `PhpDumper->addService()` → `dumpValue()` (an **eval** frame; PhpDumper is
  autoloaded) → an eval closure that calls the **compiled** `Definition::getErrors()`. The Definition
  is not a direct argument of `dumpValue`; the walker found only `ServiceLocator` definitions with
  `hasErrors() === false` in the frames' arguments, so the corrupted one is an inline definition
  reached from the argument graph.
- EIR (`--emit-ir`) types `Definition::$errors` as `Heap(Mixed) php=array` in **both** `addError`
  and `getErrors`, i.e. a boxed Mixed cell holding an array. The compiled `getErrors` therefore
  unboxes the cell and iterates it; the warning means the cell's tag at that moment is **string**,
  i.e. some path replaced the cell's value with a string.
- Isolated reproductions that MATCH PHP and therefore do not explain it: compiled `new` +
  `addError(string)` + `addError(Closure)` + `addError(Definition)` + `getErrors`; `clone`; reflection
  `newInstanceWithoutConstructor`; a compiled `ChildDefinition`; an eval-created `Definition` in a
  binary where `Definition` is AOT; and a compiled `ContainerBuilder` + `addError` + `compile()` +
  `PhpDumper->dump()` on a synthetic container (dumps 1213 bytes cleanly).
- Diagnostics used: a temporary `PROBE_DEF` block in the copy's `bin/console` (compiled) and a
  temporary `probe:def` command (eval), both removed afterwards; an error handler installed in the
  application closure to capture `debug_backtrace(DEBUG_BACKTRACE_PROVIDE_OBJECT)` at the warning.
- Disk: each console build with `--emit-asm`/`--keep-symbols` plus its binary is ~1.0-1.2 GiB; the
  probe builds were deleted after each use, leaving ~10 GiB free.

Next concrete step: build the real container inside a harness so the corrupted Definition can be
identified (e.g., a compiled entry that boots the dev kernel and, before dumping, walks
`ContainerBuilder::getDefinitions()` calling `hasErrors()`/`getErrors()` and prints class + id),
then inspect the one definition whose cell holds a string. The likely mechanism is the Mixed cell
being replaced by the appended value when an array property's cell is initialised but null in one
of the creation paths, or an `array_merge` result stored without boxing.

#### Container-walk harness (reusable, added 2026-10-22)

A compiled block in the copy's `bin/console`, gated on `PROBE_DEF`, builds the real container
WITHOUT booting/dumping and walks every definition (including ones nested in arguments and
`ArgumentInterface` values, with an `spl_object_id` visited set):

```php
$kernel = new \App\Kernel('prod', false);
(\Closure::bind(function () { $this->initializeBundles(); }, $kernel, $kernel::class))();
$c = (\Closure::bind(function () {
    $b = $this->getContainerBuilder(); $this->prepareContainer($b); return $b;
}, $kernel, $kernel::class))();
// for each $c->getDefinitions() as $id => $def: recurse args, call $def->hasErrors()/getErrors()
$c->compile();
// walk again
```

Result on a cold prod cache: `definitions=1` before compile and `123 definitions / 146 definitions
visited (inline included) / 0 bad` after `compile()`. The corrupted `Definition` therefore appears
only while `PhpDumper->dump()` runs, not during `ContainerBuilder::compile()`. Next step: append
`(new PhpDumper($c))->dump()` to that harness, with the error handler active, and walk again after
the dump throws; the dump itself must be creating or mutating the definition whose `errors` cell
holds a string (the dumper resolves `ChildDefinition`s and inline definitions from arguments).

The probe block was removed and `bin/console` restored byte-identically (`cmp` clean).

#### Harness result: synthetic compile+dump is clean

The compiled walker harness was extended to compile the real bundle configuration (123 definitions,
146 including inline ones) and then run `PhpDumper->dump()` twice: once with the default options
(134,539-byte string) and once with `as_files => true, debug => true` (105 files). **Both succeed and
no definition's `errors` is ever a non-array.** So the corrupted `Definition` is not produced by
`prepareContainer` + `compile()` + `PhpDumper` on the app's bundle configuration either — the real
path must differ in one of: the app's `Kernel` overrides (custom passes / `build()`),
`Kernel::initializeContainer`'s `setRemovingPasses([])`/`setAfterRemovingPasses([])` (which the
debug trait uses but the harness did not), the `preload_classes`/`inline_factories` options the
kernel computes, or a definition created by a pass that only runs in that exact configuration.

Next: harness step 2 — call `Kernel::initializeContainer()` in a bind-scope with the dumper
replaced by a subclass that walks the builder before dumping (or dump into a temp dir and catch the
warning with a handler), so the exact kernel configuration is used. The copy's `bin/console` was
restored byte-identically afterwards.

#### Dump-time walk (exact kernel path)

The harness was finally run at the exact point: a temporary `dumpContainer()` override in the copy's
`App\Kernel` walks the **real** `ContainerBuilder` (the one `buildContainer()` + `compile()` produce
for the app's bundles) immediately before `parent::dumpContainer()`.

Result: `WALK definitions=142 seen=176` with **no** definition (inline ones included) whose
`errors` is not an array — and the dump then fails in `Definition::getErrors()`. So the corrupted
definition is created or mutated **inside `PhpDumper::dump()`**, after that walk. `PhpDumper` never
calls `addError()`, and `dumpValue()`/`addNewInstance()`/`addService()` are all `private`, so the
next instrument must be the error handler active *at dump time* printing the full
`debug_backtrace(DEBUG_BACKTRACE_PROVIDE_OBJECT)` with every frame's `args` (including nested arrays
and `ArgumentInterface::getValues()`), to identify which value reaches the failing `getErrors()`.
The earlier handler showed the innermost dumper frame as `dumpValue(ServiceLocatorArgument)`; the
Definition must be nested one level deeper in that argument graph than the walker's argument
recursion or the backtrace display reached.

`src/Kernel.php` and `bin/console` were restored byte-identically, `build-p6`/`var/cache/prod`
removed.

### Zero compile warnings: the PHP-divergent lints are now opt-in (2026-10-22)

The warning census had three families, all of them diagnostics php never prints: unused variables
(264 console / 306 web), the permissive local-retype notice (23), and unreachable code (16).
`CheckOptions` gained `lint: bool`; `types::warnings::collect_warnings` and the retype notice only
run when it is set, so a default build is as silent as php. The switch is `--lint` (`CliConfig.lint`,
threaded through `pipeline::compile`), documented in `docs/compiling/cli-reference.md` with its own
"Lint mode" section, and `--strict-locals` still turns the retype into a hard error independently.

Test coverage: `cli::tests::lint_flag_sets_lint`, `cli::tests::no_lint_flag_defaults_off`, and
`warnings::test_php_divergent_lints_are_opt_in` (default options yield zero warnings; `--lint`
yields the unused/unreachable/retype notices). The diagnostic helpers
(`check_source_full`, `check_files_diagnostics`) opt in so every existing warning assertion keeps
its meaning; `warnings` 27/27 and `unreachable` 6/6 pass.

Measured with the final compiler (`candidate-compiler-diag1` rebuilt at 17:24):

| entry | warnings | binary |
|---|---|---|
| `bin/console` (`build-current`) | **0** (was 303) | 174,424,072 B, 31.2 s |
| `public/index.php --web` (`symfony-web-lint`) | **0** (was 345) | 213,080,040 B |

The web binary is byte-for-byte the same size as `symfony-web-final3`, and the runtime probes are
unchanged: prod routes **6/6 identical (sha256)**, dev **5/6** (the 404 include-frame gap).

`--check` on a retype fixture prints nothing by default and
`$a changes type from int to string…` with `--lint`, verified with the frozen binary.

#### Prod console matrix with the zero-warning build (warm container)

`probe-console.py` on both copies with the PHP-warmed prod container: 4 differences of 33 —
`list`, `about`, `secrets:list` (the documented absolute-vs-relative `PHP_SELF` spelling plus the
emulated PHP/intl facts) and `cache:clear` (the `Definition::$errors` dump-time corruption). The
other **29 commands are byte-identical after path normalisation**, and every exit matches. This
confirms the lints change did not alter generated code or runtime behavior.

#### Decisive dump-time evidence: array before, string during

A `dumpContainer()` override printed every `.errored..` definition **immediately before** `parent::dumpContainer()`:

```
PRE defs=142
PRE id=.errored..service_locator.1vYpZ1B.Symfony\Component\Config\Loader\LoaderInterface
    class='Symfony\Component\Config\Loader\LoaderInterface'
    errorsType=array
    value=["Cannot autowire service \".service_locator.1vYpZ1B\": it referen…"]
```

The same definition is the one reported as a **string** at the failure inside `PhpDumper::dump()`. So the
dump turns a valid `errors` array into a dangling cell whose memory is later reused by a
`ServiceLocator` class string — an ownership bug, not a bad default.

The dump calls `getErrors()` **several times per definition** (`addServices` service map at
PhpDumper:1986, `addService` at :924, `addServiceInstance`/`getServiceCall` at :2121), and each caller
releases the returned array. The compiled `getErrors` returns the property value (EIR
`Heap(Mixed) php=array`), so if the return transfer or the Mixed property append path loses the
property's ownership share, the first release frees the array and the next read sees reused memory.
A minimal compiled class with the same shape called twice (including from eval, with heap churn) stayed
correct, so the trigger involves the eval-created `.errored..` Definition whose `addError()` stored the
Mixed cell through the eval/compiled boundary — the audit to run next is the ownership of
`PropSet`/`MixedArrayAppend`/`Return` for `Heap(Mixed)` properties created by eval and read by compiled
methods.

Restored `src/Kernel.php` and `bin/console` byte-identically; probe build/cache removed.

### Eval-surface inventory (prod vs dev, one request, 2026-10-22)

Measured with `ELEPHC_EVAL_TRACE=1` on the final binaries (`symfony-web-lint` with the prod preload,
and the same binary in `APP_ENV=dev`). Counts are per request, worker startup excluded from the
interpretation figures:

| trace event | prod | dev |
|---|---|---|
| `include_ok` (interpreted include) | **30** | **135** |
| `include_fast_skip` (parse-cache hit, no execution) | 176 | 176 |
| `compiler_included_skip` (build-time include skipped at run time) | 1 | 1 |
| `native_constructor` resolved (AOT class built from eval) | 11 | 63 |
| `dynamic_method_start` (eval method dispatch) | 15 | 59 |

The AOT dev/prod container (prod preload) is what removes ~105 interpreted includes: prod executes
the dumped `App_KernelProdContainer.php` + `removed-ids.php` + `bundles.php` + `url_matching_routes.php`,
while dev has to generate/load that container from the eval'd kernel path.

The 30 prod includes, by group:

- **22 vendor files**: `symfony/deprecation-contracts/function.php` (a Composer eager `files`
  include), and on-demand class files: `console/CommandLoader/CommandLoaderInterface.php`,
  `twig-bundle/TwigBundle.php`, `routing/CompiledRoute.php`,
  `translation-contracts/TranslatableInterface.php`, `console/Messenger/RunCommandMessage.php`,
  `routing/Attribute/DeprecatedAlias.php`, `console/Helper/TableCellStyle.php`,
  `routing/Generator/ConfigurableRequirementsInterface.php`, `routing/Generator/UrlGenerator.php`,
  `routing/Generator/CompiledUrlGenerator.php`, `routing/Matcher/Dumper/CompiledUrlMatcherTrait.php`,
  `routing/Matcher/UrlMatcher.php`, `routing/Matcher/CompiledUrlMatcher.php`,
  `routing/Matcher/RedirectableUrlMatcherInterface.php`,
  `framework-bundle/Routing/RedirectableCompiledUrlMatcher.php` (several appear twice: once through
  the autoloader, once through an explicit `require` in the dumped container);
- **7 `var/cache/prod` files**: the dumped container, `removed-ids.php`, `bundles.php`,
  `url_matching_routes.php` and the compiled Twig templates under `var/cache/prod/twig/…`;
- **1 `public/index.php`**: `autoload_runtime.php`'s `$app = require $_SERVER['SCRIPT_FILENAME']`
  still executes the ENTRY file through the interpreter even though the entry is compiled.

Reduction targets, in order of value:

1. **Twig templates** (`var/cache/prod/twig/*.php`) — the known remaining runtime-compiled family;
   compiling the dumped template classes AOT removes the last large interpreted surface.
2. **The entry self-include** — `public/index.php` is compiled and also interpreted once per
   request; recognizing the compiled entry at that include would remove one include and its eval
   frames (and is a prerequisite for the `require_once` frame work).
3. **On-demand vendor classes above** — they are statically named by the framework (routing/console/
   twig-bundle) but outside the preload closure, so the autoloader interprets them; extending the
   AOT autoload closure or the preload list would fold them in.
4. **Composer eager files** (`deprecation-contracts/function.php`) — a build-time `files` include
   that still runs through `include_exec`; the resolver already knows it as an eager function file.

#### Root cause narrowed: the FIRST `getErrors()` call frees the property's array

A temporary diagnostic (copy's `Definition::getErrors`, reverted) logged the type at entry and after the
loop. During a cold `cache:clear` it shows exactly two calls for the same `.errored..` definition:

```
entry: array 1        (caller PhpDumper.php:924, addService)
AFTER loop: type=array value=["Cannot autowire service … LoaderInterface …"]
entry: string n/a     (caller PhpDumper.php:1986, addServices service map)
```

So `getErrors` itself is correct — it converts the Closure to its message and returns an array, and the
property is still an array at the end of the loop. The property cell is released **after the return**,
between the two calls, by the caller's release of the returned array. The array therefore no longer has
the property's ownership share: the compiled `getErrors` hands the property's cell to the caller without
retaining it (or the epilogue's cleanup slot table releases it), so the first caller's release frees the
storage and the second call reads reused memory — which the `ServiceLocator` class string had taken.

Isolated reproductions (compiled/eval-created object, compiled/eval caller, closure element, release,
2000-string churn) all keep the property alive, so the trigger is specific to this function's generated
ownership: the next step is to dump the epilogue slot table / register-allocation ownership for
`Definition::getErrors` and check whether the returned `Heap(Mixed)` property cell is registered as an
owned cleanup slot while it is also the return value. `--ir-opt=off` and `--regalloc=stack` both still
fail, so it is lowering/ownership, not a pass or the allocator.

The vendor patch and probe builds were reverted/removed; `Definition.php`, `bin/console` and
`src/Kernel.php` are byte-identical to their backups.

Addendum to the `getErrors` diagnosis: the compiled function's RETURN path is
`ldr x0, [x9, #312]` then `bl __rt_incref` — it **does** retain the property cell, and the eval wrapper
(`__elephc_eval_method_..._geterrors`) increfs again for its interpreter owner. So the return transfer
is correct. Since the first caller's release still frees the storage, the **object itself is not holding
a reference** to its `errors` cell: the cell was materialized without the property's ownership share
when the eval-created `.errored..` Definition was built (its default `[]`, or the clone `addError`
stored). Audit target: the eval-class object allocator and `PropSet`/`MixedArrayAppend` ownership for
`Heap(Mixed) php=array` properties — every such cell must start (and stay) owned by the object, so a
retained return plus its release nets to zero. This is consistent with everything measured: a second
`getErrors()` after the first release reads reused memory, while isolated fixtures (whose cell is
rebuilt/owned on the compiled path) survive even with heap churn.

#### Detached-copy experiment: dump succeeds, container has a second bug

Patching the copy's `Definition::getErrors` (diagnostic only) to return a **detached shallow copy**
(`$detached[$k] = $v` in a loop) instead of `$this->errors` makes `cache:clear` **exit 0**: the
`PhpDumper` completes and writes the container. This proves the dump-time crash is the **return sharing
the property's cell** — the caller's release frees storage the object still points at.

With the dump unblocked, the freshly generated prod container then fails at boot with
`Cannot use object of type Symfony\Component\DependencyInjection\Argument\ServiceLocator as array`
(`cache:clear` logs it as `[critical]` but exits 0, so `cache:clear --no-warmup`-style acceptance would
not see it; a request on the warm container would). That is a **second, downstream** defect in the
generated container's argument graph, to investigate after the ownership fix — it is not visible today
because the dump dies first.

The proper fix is not a copy in `getErrors`: it is to make the **object own its `Heap(Mixed)` property
cell**. Candidates, in order: (a) the eval object allocator's default cell for `Heap(Mixed) php=array`
properties is stored without the object's ownership share; (b) the compiled `PropSet`/append path's
clone is released by the function epilogue's slot table. The detached-copy test cannot distinguish
them, but it localises the whole symptom to the property-cell reference count.

Vendor patch reverted byte-identically; probe build and cache removed.

#### Clone-on-return experiment (reverted): the free is inside `getErrors`, not the caller

Two compiler variants were tried in `lower_return` for a `Mixed` property read
(`return $this->errors`), then reverted:

1. return a `MixedClone` of the read **and** release the read — `cache:clear` still fails;
2. return a `MixedClone` and let the frame epilogue release the read — still fails.

Since the clone detaches the returned cell from the property's cell, the caller's release can no
longer be the free. The property is still corrupted by the time the dump's second `getErrors()` runs,
so the storage is released **inside `getErrors` itself** — during the foreach teardown, the element
write, or the epilogue cleanup table that owns the property read. The diagnostic's `gettype(...)`
probe reads the property (an acquire+release) and can even mask one reference; the next instrument
must be the heap debug build with a `check_live` on the `errors` cell, or an lldb breakpoint on
`__rt_decref_mixed`/`__rt_heap_free_safe` filtered to the cell address read from the object at
`+312` (the `--keep-symbols` build makes `_method_..._Definition___geterrors` and the wrapper
breakpoint-able).

`src/ir_lower/stmt/control_exit.rs` was restored to its pre-experiment text (`grep` for the
experiment comment is empty), the candidate compiler and the console were rebuilt from that source
(console 0 warnings, 174,424,072 B), and `var/cache/prod` was cleared.

#### Property-return ownership experiments (all reverted) + a new regression test

- The declared `array` type is `PhpType::php_array()` — `Union([Array(Mixed), AssocArray{Mixed, Mixed}])`,
  whose codegen repr is `Mixed` (a boxed packed-or-hash cell). So `Definition::$errors`' property
  reads and the `mixed_array_append` in `addError` use the SAME representation; the earlier
  "Mixed vs raw array" reading of the assembly was wrong.
- `return $this->errors;` has NO extra retain in `acquire_borrowed_return_value` (the read is an
  owning temporary), and the eval wrapper then increfs for the interpreter. Forcing an extra
  `acquire_if_refcounted` on `PropGet` returns **fixed the dump** (`cache:clear` exit 0, container
  written) but the heap-debug regression showed a 6-block/320-byte leak (the returned cell's extra
  reference is not dropped by a raw-`array` release), so it was reverted.
- A `MixedClone` on the return (two variants) did not fix the dump; a current-class preference in
  `generic_object_array_property_type` did not change the generated append either; both reverted.
- New test `tests/codegen/runtime_gc/property_return_ownership.rs`
  (`test_returned_property_array_keeps_the_object_storage_alive`): a returned property array stays
  readable after the caller releases it and 2000 allocating iterations, and the heap summary is
  clean. It passes on the current source and would have caught the forced-retain leak.
- Remaining unknown: the console-only corruption of `errors` during `PhpDumper::dump()`. The next
  instrument is lldb on the `--keep-symbols` console: break on `_method_...Definition___geterrors`,
  record `*(void**)((char*)$x0+312)`, then break on `__rt_heap_free_safe`/`__rt_decref_mixed` with
  that address and print the backtrace (the diag's own property read can perturb the refcount, so
  no PHP-level probe).

Source restored to its pre-experiment text (grep for both experiment comments is empty), candidate
compiler and console rebuilt from it (console 0 warnings, known `Definition.php:814` failure still
reproduces), and `var/cache/prod` cleared.

#### lldb session: the `Definition::$errors` cell is freed by `getErrors`' own return cleanup

Instrumented `build-sym-hd` (console compiled with `--heap-debug --keep-symbols`), ran
`cache:clear` cold under lldb with a hardware watchpoint on the `errors` property slot
(`receiver + 0x138`) and on the new cell's refcount (`cell - 12`). Script kept at `/tmp/wp.py`
(pattern is the deliverable; it is not part of the tree).

Refcount trace of the cell the element write publishes into the property (slot `0x10b780b90`):

```
property slot write: cell=0x10b780b90 rc=1
  #1 +1  pc=getErrors+4036   (the `return $this->errors` prop_get retain)
  #2 -1  pc=getErrors+4100   (__rt_release_slot_table, called just before return)
  #3 -0  caller slice        (the bridge releases the returned value)
```

At #2 the table cursor decodes to **offset `0xb0`, kind 1 (`__rt_decref_mixed`)**. All other
table slots read 0 at that instant (the helper clears a slot before releasing it). The released
value `X = 0x10b84f870` is a **tag-10 cell (closure)**; its state `0x10b7d6610` owns a callable
descriptor whose **tag-7 (mixed) capture** is the property cell `0x10b780b90`. So a **closure
stored in the local at `fp-0xb0` captures the `errors` container cell**, and releasing that
closure at return drops the container to refcount 0 while the property still points at it.
The freed cell is then reused as a string, and the next `getErrors()` at `Definition.php:814`
reports `foreach() argument must be of type array|object, string given`.

Backtrace at the fatal decref:

```
#0 _rt_decref_mixed(0x10b780b90)
#1 _rt_cleanup_invoke            #2 _rt_callable_descriptor_release
#3 _rt_cleanup_invoke            #4 _rt_mixed_free_deep
#5 _rt_release_slot_table        #6 Definition::getErrors + 4100
```

`fp-0xb0` is a `Mixed` cleanup local; the eval-bridge call operand slots (`$error`, descriptor,
argument arrays) live there. The AOT closure path itself is sound: a small program with an
**AOT-created bound closure** stored in a `private array $errors`, `getErrors()` mutating
`$this->errors[$i]`, churn and a second call is **clean** under `--heap-debug`, called from AOT
**and** from `eval()` on an AOT object.

**Minimal reproducer of the adjacent defect (eval closure capturing an AOT object):**

```php
$d = new Def();                 // AOT class: private array $errors, addError(), getErrors()
$GLOBALS['the_def'] = $d;
eval('
    $def = $GLOBALS["the_def"];
    $mk = function (string $kind) use ($def) {
        return function () use ($def, $kind) { return "msg:" . $kind . ":" . $def->label(); };
    };
    $def->add($mk("a")); $def->add($mk("b"));
    $a = $def->getErrors(); echo count($a), "\n";
');
for ($i = 0; $i < 3000; $i++) { $junk = 'Symfony\\Component\\DependencyInjection\\ServiceLocator' . $i; }
eval('$def = $GLOBALS["the_def"]; $b = $def->getErrors(); echo count($b), "\n";');
```

Output is correct twice, but `--heap-debug` reports **14 live blocks / 960 bytes** instead of
`clean` (one live indexed-array block, refs=1). The eval-created closure's capture of the AOT
object is not being released (over-retain), which is the mirror image of the console's
over-release of a captured container cell. The next instrument is the eval bridge's capture
materialisation/release pair: for a closure created in eval that captures an AOT `Mixed`, the
descriptor capture must own exactly one lease (`__rt_mixed_from_value`-style retain) and the
interpreter's arena entry must retire it exactly once. Hypotheses still open for the console case: either the `.errored..` closure is eval-created (the
container build runs interpreted vendor code against AOT `Definition` objects) and its tag-7
capture of the container was materialized without a lease, or the AOT-bound `Closure::bindTo`
descriptor copied a Mixed capture without the `__rt_mixed_from_value` retain. The capture table
of that descriptor is what releases the property cell; the next session should instrument the
descriptor's capture metadata at creation (name, tag, source) rather than repeating the refcount
watch.

No source change was kept from this session: the forced-retain return experiment fixed the dump
but leaked in a native build, and the `generic_object_array_property_type` current-class
preference changed no generated append. Both were reverted; the console in `build-current` is
built from the reverted source (0 warnings) and still reproduces `Definition.php:814`.

#### The bound `$this` box is over-released on the exception path (verified, not reuse)

lldb, AutowirePass closure bind (`__rt_closure_bind`, reached via `Closure::bind`; the `-n` breakpoint
missed the triple-underscore Mach-O symbol, use `-r 'rt_closure_bind$'`), source with captures
`reference/label/currentId/this`. Immediately after the bind:

```
source currentId = 0x10b780900 tag=1 rc=3      copy currentId = same cell rc=3
source this      = 0x10b798d50 tag=6 rc=1      copy this      = 0x10b780b90 tag=6 rc=1  (new box, box_this ran)
```

Watching `copy.this - 12` from the bind to the crash (tag and kind printed at every stop, both
constant `tag=6 kind=5`, so **the address is never reused during this window**):

```
rc 1 -> 2   _rt_incref in fn__...AutowirePass...createTypeNotFoundMessageCallback_0 (the closure body takes $this)
rc 2 -> 1   _rt_decref_mixed
              _rt_cleanup_invoke / _rt_hash_free_deep
                _rt_mixed_free_deep / _rt_array_free_deep
                  ClassExistenceResource::throwOnRequiredClass__cdylib_exception_cleanup
rc 1 -> 0   _rt_decref_mixed / _rt_release_slot_table
              elephc_xslice_L_7fc123fe16da_13   (the closure body's normal return cleanup)
```

So the box has exactly two owners (descriptor + body) but **three releases**: one of them is the
exception cleanup freeing a container that holds the box **without a lease**. The chain is
`array -> Mixed -> hash -> Mixed(box)`: the generic call-arguments container (`__elephc_func_args`)
holds the bound `$this` as a Mixed element and its deep free decref's that element although the
element was never retained when the container was built. The descriptor's own capture lease is
1; the body's +1 and normal return are balanced; the exception path's decref is the bug.
This also explains why the property container looked freed "through a closure capture": the
bound `$this` box is a real object box, and the `Definition::$errors` container cell that later
occupied the same address was only reached after the box had already been freed.

Ruled out by measurement:
- `return_cleanup_skip_slot` for `store_local v; return v` (ClosureBind result in
  `createTypeNotFoundMessageCallback`: `v310 = closure_bind; store_local v310 slot[79]; return v310`).
  The descriptor refcount trace shows the caller's boxing +1 and the return-value release -1 are
  BALANCED (rc 1 -> 2 -> 1), so that slot is not the over-release. The experimental skip was
  reverted; it did not change the console failure.
- The bind's null-sentinel path (`receiver=0x7ffffffffffffffe`): that is the legitimate
  scope-only `Closure::bind(..., null, Scope)` shape (21 calls); the AutowirePass bind receives a
  real object (`0x10b705a20`).

Next instrument: find who builds the generic `__elephc_func_args` array for a closure invocation
and how each element is boxed; every Mixed element must own a lease (or the container's deep free
must not release borrowed elements). A minimal reproducer is a bound closure invoked with an
argument while an exception unwinds through the closure body's callee; the existing
`/tmp/propbind` programs do not exercise the exception path, which is why they stayed clean.

#### FIXED: the trace's frame hash stole the bound closure's `$this` capture

`src/codegen/lower_inst/core_builtins/backtrace.rs`, frame reader `$this` path: for a `Mixed` local
(a closure bound to a receiver stores `$this` as a borrowed Mixed capture),
`emit_box_current_value_as_mixed` is a deliberate no-op, so the reader handed the descriptor's own
capture cell to `__rt_backtrace_frame_hash`; the trace's frame hash takes ownership and released
it, and every exception whose trace materialized the frame decref'd the live capture. The fix
clones the borrowed cell (`__rt_mixed_clone`) so the trace owns a fresh cell; non-Mixed `$this`
values already get a fresh box.

Proof (same program, only the compiler differs):
- regression test `tests/codegen/runtime_gc/bound_closure_trace_object.rs`
  (`test_bound_closure_trace_object_keeps_this_capture_alive`: bound closure calls
  `debug_backtrace(DEBUG_BACKTRACE_PROVIDE_OBJECT)` then reads `$this` again after the trace dies):
  before, `Fatal error: heap debug detected free-list corruption`; after, `ok:ok:obj` and
  `HEAP DEBUG: leak summary: clean`.
- focused suites: `backtrace` 28/28, `throwable` 64/64, `trace_object` 1/1.

Console (vendor untouched, cold `var/cache/prod`): `cache:clear` now exits 0 with **no**
`Definition.php:814` warning; the container is dumped and `build-current/console list` boots it
(`Symfony v8.1.4 (env: prod, debug: false)`, exit 0). The remaining command failure is the separate
`Cannot use object of type Symfony\Component\DependencyInjection\Argument\ServiceLocator as array`
reported by the console's error listener; next step is its stack.

#### Next blocker after the trace fix: the shutdown TypeError from the callable invoker

With the trace fix, `cache:clear` completes the dump and the warmers:
- `APP_ENV=dev APP_DEBUG=1`: "[OK] Cache ... successfully cleared", warms RouterCacheWarmer and
  TemplateCacheWarmer, then dies at process shutdown with
  `Fatal error: Uncaught TypeError: Value must be of type array, mixed given` (exit 255).
- `APP_ENV=prod APP_DEBUG=1`: same success and same shutdown fatal.
- `APP_ENV=prod APP_DEBUG=0` (the earlier matrix): the console reports
  `Cannot use object of type ...ServiceLocator as array` instead; `list` on the freshly dumped
  container boots (exit 0), so the container itself is valid.

lldb on the symbolized console (`build-sym`, `--keep-symbols`), breakpoint on the shared dynamic
type-error helper `_eir_shared_type_error_given` (`-r 'type_error_given$'`): exactly one hit, at
shutdown, and its frame #1 and #2 are
`eir_Symfony_Component_Consol_callable_invoker_8999b4204acb_816`. Register/record decode at the
helper entry: the record lives in `x2`, the runtime tag in `x0` (the body's
`record_reg`/`int_result_reg` contract), and x0/x1 here hold the string payload
`"DOCTRINE…"` / `"_DEPRECA…"` with x3 = 1: a **string** whose text is a Doctrine deprecation
identifier reached a guard that expects an **array**. The message spells it "mixed given" only
because the helper's tag register did not match any arm. PHP's oracle runs `cache:clear` cleanly,
so the callable invoker is forwarding the wrong value/argument (a by-ref `array` parameter
receiving the deprecation message string is the leading shape). The invoker's own PHP activation
chain is not published at shutdown (`_exc_call_frame_top` reads 0), and its descriptor was not at
`fp - INVOKER_DESCRIPTOR_OFFSET` in this build (`shared_tails` frame setup), so the descriptor name
still has to be recovered — scan the invoker frame for the descriptor whose `+16` points to a
printable name, or disassemble `eir_..._816` and follow its by-ref argument materialization.

State note: the stale `symfony-web-lint/` web binary (built before the trace fix) was removed to
reclaim space; the `--web` entry must be rebuilt with the current compiler before the web matrix
is re-measured. `build-sym/` (symbolized console) and `build-current/` (the tested console, trace
fix included) are kept.

#### Shutdown TypeError: the argument is the superglobal KEY, read as a Mixed cell

The string in the failing guard decodes to `DOCTRINE_DEPRECATIONS`; the only source is
`vendor/symfony/framework-bundle/FrameworkBundle.php:117`
(`$_ENV['DOCTRINE_DEPRECATIONS'] = $_SERVER['DOCTRINE_DEPRECATIONS'] ??= 'trigger';`).

The plain statement is fine: `--emit-ir` shows the native shape
`runtime_call v102 v95 v96` = (_SERVER, key, value) after `mixed_clone` + `store_global`, and the
standalone program (with and without `eval`) runs and prints `trigger`. So this is not the array
set itself.

Disassembly of the failing invoker around the guard (`eir_..._callable_invoker_8999b4204acb_816`
+11052..+11100):

```
ldr  x0, [x0]                 ; load the argument value
bl   __rt_mixed_unbox         ; x0 = runtime tag
cmp  x0, #4 / #5              ; array tags accepted
bl   _eir_shared_type_error_given   ; otherwise TypeError, record in x2
```

At the helper, x0 is the bytes `DOCTRINE`/`_DEPRECA` (a raw string), not tag 4/5 or 1: the guard
is reading a **raw `Str`** as if it were a **Mixed cell**, so `__rt_mixed_unbox` returns the
string's first bytes as the tag. The invoker is entered from
`elephc_xslice_L_7d4dbef16188_28` (invoker saved LR `0x1035345fc`), whose caller is `main`; that
slice is the eval-bridge call path. `ELEPHC_EVAL_TRACE=1` ends with the successful
`CacheClearCommand::execute` / `Command::run` and an `object_is_a` on `CacheClearCommand` during
shutdown — the failing callable is a **by-ref `array` parameter** invoked from the interpreter
path with the **key string** in the array's argument slot.

Next instrument: name the invoker's target (its name table or the descriptor recovered from the
eval slice's arguments) and inspect how the interpreter packages the argument array for a
by-ref parameter; the fix belongs to that packaging or to the callable's argument order, not to
the trace code.

Nearest symbol for the invoker's caller PC `0x1035345fc`: `nm -n` places it inside
`_method___Symfony_N_Component_N_Runtime_N_Runner_N_Symfony_N_ConsoleApplicationRunner___run`
(the promoted slice label `_elephc_xslice_L_7d4dbef16188_28` sits inside the same function).
The only array-typed parameter reachable from that method is
`InputOption::__construct`'s `array|\Closure $suggestedValues` (omitted, default `[]`), which does
not explain the string. Treat the attribution as a lead, not a conclusion: the slice spans the
assembly chunk and the key string's only source is `FrameworkBundle.php:117`. Next instrument:
the interpreter's superglobal write/callback path for `$_SERVER[$key] ??=` in an eval-owned
superglobal, and the argument order it uses when it invokes the AOT helper/invoker; the mismatch
(array, key, value) vs (key, value, array) is the leading shape.

Caller-site disassembly (`0x1035345fc`, saved LR of the failing invoker frame):

```
+8480..+8540  push four Mixed arguments loaded from fp-0x128, fp-0x150, fp-0x138, fp-0x140
+8548  ldr  x10, [x0]              ; receiver class id
+8552  class_vtable_ptrs[class_id]
+8564  ldr  x9, [x9, #0x50]        ; vtable slot 0x50 = the callable invoke slot
+8568  blr  x9                    ; invoker(receiver=x0, a=x1, b=x2, c=x3)
```

So the call is `$callable(a, b, c)`, receiver + 3 arguments, through the callable vtable. The
PHP activation chain is empty at the failure (`&_exc_call_frame_top` reads 0), which places the
call in the shutdown/destructor phase rather than inside a live PHP frame; the nearest symbol
(`ConsoleApplicationRunner::run`) is then only a rough attribution. The guard checks the FIRST
parameter as a by-ref `array` and receives the key string in `x1`, i.e. the call passed
`(key, ?, ?)` where `(array, ?, ?)` belongs: the eval bridge's native-call binding/writeback
(`crates/elephc-magician/src/interpreter/dynamic_functions/{native_execution,function_binding,method_binding,native_staging}.rs`)
is the leading suspect — it resolves the by-ref target of `$_SERVER[$key] ??=` to the key
expression instead of the superglobal cell. Next: recover the invoker's descriptor/signature
(vtable class id + slot, or the name table read with the right frame offset) to name the callee,
then fix the by-ref target/argument order in the interpreter binding.

Receiver/argument read at the shutdown TypeError (caller frame `0x16fdfdf60`, slots used by the
disassembly):

```
receiver = 0x10acf2720                 (callable descriptor; no printable name at +16/+24)
arg1     = 0x10ace8ec0  [value]=0x6    (object: Mixed tag 6 or raw object metadata)
arg2     = 0x10ace91a0  [value]=0x6    (object)
arg3     = 0x10ace8b50  [value]=0x1    (Mixed cell tag 1 = string; the DOCTRINE_DEPRECATIONS key)
```

So the call is a three-argument callable invocation `(object, object, string)`. The invoker's guard
does `__rt_mixed_unbox` and then expects tag 4/5; the tag it saw was the raw string bytes, not 1,
so the argument it checked had already been **unboxed once** (raw `Str` pointer) and was then
unboxed again by the guard. That double unbox is the likely defect: the invoker's by-ref argument
materialization (inline named-argument builder used for signatures with `ref_params`, see
`assoc_normalizable` in `runtime_callable_invoker.rs`) unboxes the owner cell before the parameter
guard, and the guard unboxes the payload again. The leading callable shape remains a
`CacheItem`-callback with a `?array &$metadata` third parameter (object, object, string), i.e. the
cache `get()` callback contract (`ContractsTrait::doGet`), invoked at shutdown with the string in
the by-ref metadata slot.

Next: read the receiver descriptor correctly (dump its first 8 words and the object class ids of
arg1/arg2), then fix the by-ref argument materialization in the invoker (keep the cell for a by-ref
parameter, or skip the guard's second unbox).

#### Focused console acceptance (6 commands, prod, fixed binary) + cold-boot localization

Probe (`probe-console.py`) on the disposable PHP oracle and on the fixed native console
(`build-current/console`, `--only list,about,cache:warmup,lint:container,debug:container,cache:clear`):
- identical: `cache:clear` (exit 0, clean stderr), `debug:container`, `lint:container`;
- `list`: identical bytes except the executor path inside every command's `help` string
  (`bin/console` vs the absolute native binary) — a runtime-path normalization fact;
- `about`: identical except the emulated PHP version (8.5.0 vs oracle 8.5.10) — allowed;
- `cache:warmup`: differed only because the app started on a **stale cache**; on the cache left by
  the probe's `cache:clear` it succeeds (`[OK] Cache ... successfully warmed`). Matrices must start
  from a cleared `var/cache/prod` (already a campaign rule).

Cold first boot is the real blocker: `rm -rf var/cache/prod; ./build-current/console list` reports
`Cannot use object of type ...ServiceLocator as array` (3 `[critical]` lines, process exit 0), so a
cold `list`/web boot fails exactly like `cache:clear`.

lldb on the symbolized console, breakpoint `_rt_throw_object_not_array` during a cold `list`:
object `0x10ae03a90`, class word `0x17e`, caller PC `0x1064643f8` inside the promoted slice
`elephc_xslice_L_f02e6e3bea60_604`; the nearest function label is
`_method___Symfony_N_Component_N_DependencyInjection_N_EnvVarProcessor___getEnv`. The PHP
activation chain is empty (`&_exc_call_frame_top` = 0), so the frame does not publish an activation.
`EnvVarProcessor::getEnv()` is the function with the array reads: the `key:` branch does
`$array = $getEnv($next); if (!is_array($array)) throw ...; return $array[$key];` (and other branches
read `$this->loadedVars`). Two leads to check at the call: the `$getEnv` closure result being a
ServiceLocator (wrong closure return/capture) or the `getEnv` receiver/property being the locator
(wrong receiver), with the `is_array` guard either mis-evaluating or being bypassed.

#### Cold-boot ServiceLocator error: `Container::getEnv`'s own property access

lldb entry probe on `EnvVarProcessor::getEnv` (`-r 'EnvVarProcessor___getenv$'`, lowercase symbol)
during a cold `list`:
- receiver is always `0x10b433a80` (class word `0x1c9`, the container);
- the `$getEnv` closure is always `0x10b4f96f0`, named
  `Symfony\Component\DependencyInjection\Container::getenv`;
- the last calls are `query_string:default:container.runtime_mode:APP_RUNTIME_MODE`,
  `default:container.runtime_mode:APP_RUNTIME_MODE`, then `APP_RUNTIME_MODE`; the array-access Error
  follows immediately.

`vendor/symfony/dependency-injection/Container.php:354-384` `Container::getEnv()` contains the
offending array reads:
```php
if (isset($this->resolving[$envName = "env($name)"])) ...
if (isset($this->envCache[$name]) || \array_key_exists($name, $this->envCache)) ...
if (!$this->has($id = 'container.env_var_processors_locator')) { $this->set($id, new ServiceLocator([])); }
$processors = $this->get($id);                 // a ServiceLocator
...
$this->resolving[$envName] = true;           // ← array access
return $this->envCache[$name] = $processor->getEnv(...);
```
The object being indexed is a `ServiceLocator`, so one of `$this->resolving` / `$this->envCache`
resolves to the locator: the container's property slot is not the array the PHP source declares.
The container here is the **cold-built generated container** (the AOT binary's preloaded
`App_KernelProdContainer` is from the build-time cache; the cold build dumps and loads a NEW
generated class). So the leading cause is an **eval/AOT boundary mismatch**: `Container::getEnv`
is compiled AOT and reads fixed base-class property offsets on the freshly generated container,
while the generated/eval class or the native-slot deferral puts the service locator at that
offset. Next: inspect the receiver's class/object kind (AOT vs eval) and the
`eval_bridge_can_defer_to_native_declared_slot` decision for `Container::getEnv`'s `$resolving` /
`$envCache` reads, then align the cold-built container layout/dispatch with the AOT base layout.

Correction to the previous localization: the failing array access is at
`vendor/symfony/dependency-injection/EnvVarProcessor.php:195` in `EnvVarProcessor::getEnv`:
```php
$this->loadedVars[array_key_last($this->loadedVars)][$name] = $env = (string) $env;
```
The `ServiceLocator` hits in the IR are the **ArrayAccess class-id dispatch ladder** for the Mixed
element (`$this->loadedVars[...]`), not the `Container::getEnv` body; the function's IR is 5262
lines mostly because of that ladder (26 nominal_object arms, hundreds of method.override_arity
blocks). The element being indexed is a `ServiceLocator`, so the value stored by
`$this->loadedVars[] = $vars = $loader->loadEnvVars();` (line 199) — or an earlier loaded-vars map —
is a ServiceLocator instead of the loader's array. The env-loader path is only taken on a cold
build when a var is missing, which is why warm boots never hit it.

Next: at line 195/199 runtime, inspect `$this->loadedVars`, the loader object and the value
returned by `$loader->loadEnvVars()` (generated `container.env_var_loader` in the cold container),
then fix the return/element ownership or the loader dispatch.

`ELEPHC_EVAL_TRACE=1` on a cold `list` (fixed console) pins the runtime object:
```
... phase=method_exists class="...Argument\ServiceLocator" method="has" result=Ok(true)
... phase=method_exists class="...Argument\ServiceLocator" method="get" result=Ok(true)
... phase=object_is_a identity=4479562944 class="|runtime=...Argument\ServiceLocator"
      target="ArrayAccess" decided_by=aot-metadata result=Ok(false)
[critical] ... "Cannot use object of type ...ServiceLocator as array"
```
So the compiled code performs an array access on the same `ServiceLocator` the container just queried
with `has`/`get` (`Container::getEnv` line 363-368 / `EnvVarProcessor` path). No `loadEnvVars`
dynamic trace appears, and `var/cache/prod/App_KernelProdContainer.php` currently has no
`env_var_loader`/`env_var_processor` text, so the array access is either the inlined/AOT loader
path (line 195/199, whose element is the locator) or a `$processors->get(...)`-style call lowered
as an array access on the same locator. The `ArrayAccess` probe is the last thing before the raise.

Next: locate the exact instruction by dumping the cold container's `env_var_processor`/loader
service graph (or break on `__rt_throw_object_not_array` and read the AOT caller frame's locals),
then decide between: (a) the generated cold container returning a ServiceLocator where the loader
array belongs, (b) a dynamic method call on the locator lowered as an array access, or (c) an
inlined loader body using the locator as `$this`.

Cold container (`var/cache/prod/Container3yplunE/`) inspected:
- `container.env_var_processor` = `new EnvVarProcessor($container, new RewindableGenerator(fn () => yield 0 => secrets.env_var_loader, 1))`;
- `secrets.env_var_loader` = `new StaticEnvVarLoader(secrets.vault)`; neither `StaticEnvVarLoader` nor
  `SodiumVault` has an AOT symbol and no `loadEnvVars` trace appears, so the loader path is not the
  one that ran;
- `container.env_var_processors_locator` = `Argument\ServiceLocator` whose `serviceMap` lists only the
  built-in prefixes (base64/bool/key/url/query_string/default/string/…) — **there is no `web` entry**.
  The failing chain resolves `web:default:kernel.runtime_mode:`, so `Container::getEnv` should take
  the fallback `new EnvVarProcessor($this)` (empty loaders) for prefix `web`, not the locator branch.
- `Argument\ServiceLocator` extends the base and its interface chain is
  `ServiceCollectionInterface extends ServiceProviderInterface, \Countable, \IteratorAggregate` —
  **no `ArrayAccess`**, so the trace's `ArrayAccess -> false` is correct and the array access on the
  locator is illegal in PHP too: the compiled code reached the wrong branch/value.

Next: disassemble/inspect the compiled `Container::getEnv` branch for `$processors->has('web')` and
`Argument\ServiceLocator::get`'s `$this->serviceMap[$id]` read (property offset on the subclass);
the wrong branch is either `has` returning true for a missing prefix or the locator's `get`
reading the wrong slot and indexing the locator itself.

IR detail at the failure span (EnvVarProcessor::getEnv, span 195:71):
```
v620 = load_local slot[129]      ; php=Traversable
v621 = instance_of v620 data[361]
...  Countable load ...
v624 = load_local slot[135]
v625 = object_class_id v624
v626 = const_i64 35
cond_br (v625 == 35) -> arm / else
...
method.override_arity.hit:
  v676 = move v624 (php=...ServiceLocator)
  v679 = method_call v676 <args> data[5811]
```
So the failing operation is a **dynamic class-id dispatch ladder** (class id 35 and many arms,
`ServiceLocator` among them) calling the method named by `data[5811]`, not a direct array access in
the IR as first read. The current vendor's line 195 is
`$this->loadedVars[array_key_last($this->loadedVars)][$name] = ...`, while the IR's span 195 is the
`$loaders instanceof Countable`/count code — the compiled source's line numbering is shifted ~3
lines relative to the vendor copy on disk, so spans must not be trusted without the compiled text.

Next: identify the interned `data[5811]` string and class id 35 (asm comment or rodata), i.e. the
method actually dispatched on the ServiceLocator (likely `get`/`has`), then align the dispatch with
PHP: `ServiceLocator` has `get`/`has`, but the ladder treats it as an object of class 35 and calls
the method through the wrong arm.

The failure PC window (`0x1064643f8`) is eval-bridge code: `__elephc_eval_context_push_class_scope`
with a 53-char class name, `eir_shared_null_method_trace`, `eir_shared_static_throw`, plus the
shared type-error helpers — not a plain PHP function body. Combined with the eval trace's
`phase=method_exists class="…Argument\ServiceLocator" has_class=false` (and `decided_by=aot-metadata`
for the ArrayAccess probe), this places the cold-boot failure at the **eval/AOT boundary around
`Argument\ServiceLocator`**: the locator is created by the eval-loaded generated container
(`getContainer_EnvVarProcessorsLocatorService.php`), the interpreter does not know the class
(`has_class=false`), and AOT code reads it (`Container::getEnv`'s `has`/`get`, and the locator's
`Argument\ServiceLocator::get` with its `$this->serviceMap[$id]` read). One of those fixed-offset
reads yields the locator itself where the serviceMap array is expected, so the next array access
raises "Cannot use object of type …ServiceLocator as array".

Next: compare the AOT layout offsets of `Argument\ServiceLocator` (`factory`, `serviceMap`,
`serviceTypes`) with the eval object created by the generated container, or route those reads
through the eval-owned dispatch; the same fix likely covers the `has_class=false` method probes.

Correction after reading the failure window's strings: `str_2694` at the `push_class_scope` call
resolves to `Symfony\Component\Console\Helper\TreeStyle::box…`, unrelated to the env path. The
failure PC `0x1064643f8` therefore lies in **shared eval-bridge helper code**, and the earlier
nearest-symbol attribution to `EnvVarProcessor::getenv` (and the line 195/199 story) is unreliable.
What remains solid:
- the eval trace's last events before the raise are `method_exists` on
  `Argument\ServiceLocator` for `get`/`has` with `has_class=false`, then an ArrayAccess probe with
  `decided_by=aot-metadata` returning false, then the raise;
- the locator is an AOT class (class word 0x17e, aot-metadata), created by the eval-loaded generated
  container;
- the failing operation is therefore the **bridge's dynamic method dispatch** for `has`/`get` on
  that locator (or the next array access the bridge performs while resolving the call), not a
  miscompiled PHP array access at a known vendor line.

Next: instrument the eval-bridge dynamic-method path (`phase=method_exists`/`has_class=false`
handling in crates/elephc-magician and the `__elephc_eval_*` helpers) for an AOT
`Argument\ServiceLocator` receiver, and check the method-table lookup for an array access on the
receiver itself.

Eval-trace call sequence right before the cold-boot raise (cold `list`):
```
method_call_start get / Container::get          -> returns 0x10bc433a0
method_exists Argument\ServiceLocator::has (has_class=false) -> Ok(true)
method_exists Argument\ServiceLocator::get (has_class=false) -> Ok(true)
native_method_return_contract Container::getService -> 0x10bb92a20
method_exists EnvVarProcessor::getEnv (has_class=false) -> Ok(true)
object_is_a identity=4479562944 class=Argument\ServiceLocator target=ArrayAccess -> false
native_method_error class=Container method=getEnv status=UncaughtThrowable
```
So the array access happens while the `Container`/`Argument\ServiceLocator` code runs, after the locator's `has`/`get` were dispatched. The only array access in that code is `Argument\ServiceLocator::get`'s
`$this->serviceMap[$id]` (and `has`'s `serviceMap` read). The indexed object is the locator
itself, so the eval-created `Argument\ServiceLocator` instance's `serviceMap` slot reads back as
the locator (property-offset/value mismatch), not the array passed to the constructor.

Next: break at `Argument\ServiceLocator::get` during a cold `list` and print `$this`, the
`serviceMap` slot address/value and the constructor's stored array, then compare the layout of the
eval-created instance with the AOT class (the generated container creates it in eval at
`getContainer_EnvVarProcessorsLocatorService.php`).

Eliminations after the latest repro attempts:
- `var/cache/prod` is only 564K after the failed cold `list`, so the failure happens BEFORE the dump;
  the `Container3yplunE\App_KernelProdContainer` seen in the eval trace is the AOT preloaded
  container (build-time cache), not a freshly generated eval class. `has_class=false` +
  `decided_by=aot-metadata` confirm AOT objects. So there is no eval/AOT layout boundary here.
- The class-layout hypothesis is ruled out for eval subclasses of AOT classes: a base private
  array read from an eval-defined subclass holding a stdClass sibling passes.
- The inliner property shape (`$this->cache ??= $this->inner->get()` with a private array in the
  inner class) passes.
- `count()` on a Countable object through an array element (boxed Mixed) passes.
- Promoted constructor properties with a parent constructor and an array parameter pass
  (`new Loc(fn, ['a'=>'b'])`, `$this->serviceMap[...]` returns the array).
- The console was rebuilt from the current vendor (0 warnings); IR span 195 matches the vendor's
  `$count = $loaders instanceof \Countable ? $loaders->count() : 0;` line, so there is no line shift.

Remaining solid signal: the raise occurs after `Container::get`/`getService` and the locator's
`has`/`get` dispatch, with an ArrayAccess probe on the locator itself; the only array access in that
code is `Argument\ServiceLocator::get`'s `$this->serviceMap[$id]`. Next: rebuild `build-sym`
(35 s), break at `Argument\ServiceLocator::get` during a cold `list`, print `$this` and the
`serviceMap` slot address/value, and compare with the array passed by the AOT generated container's
`getContainer_EnvVarProcessorsLocatorService`.

Runtime probe at the AOT `Argument\ServiceLocator::get` (symbol
`method_..._DependencyInjection_N_Argument_N_ServiceLocator___get`, `--keep-symbols`) during a cold
`list`: it is entered for ids `default`, `string`, `string` on `self=0x10b7d68c0` and returns
without raising. The receiver's slots contain several heap blocks and the null sentinel
(`0x7ffffffffffffffd`) but no slot equals the locator itself, so `serviceMap`/`factory` are not
self-referential and the `serviceMap[$id]` read itself is fine. The `[critical]` error appears only
after the last `get` returns.

Therefore the failure is after the locator `get`, inside `EnvVarProcessor::getEnv` or the nested
`Container::getEnv` chain it drives: the `key:` branch does `$array = $getEnv($next); if
(!\is_array($array)) throw ...; ... return $array[$key];`, and the `ServiceLocator` is a plausible
value for `$array` (the nested `Container::getEnv` for a prefix not in the processors map returns
through the fallback path). If the `is_array` guard did not take its throw branch, the following
`$array[$key]` raises exactly "Cannot use object of type ...ServiceLocator as array".

Next: break at `EnvVarProcessor::getenv` (AOT symbol) during a cold `list`, inspect the `key:`
branch's `$array` value and the `is_array` branch, and check whether the guard's throw is
mis-branched or the nested `Container::getEnv` returns the locator.

IR check of the `key:` branch (source 75-95): the guard is correct —
```
v53 = load_local slot[18]            ; $array
v54 = type_predicate v53 array       ; is_array($array)
v56 = icmp v54, 0 Eq
cond_br v56, bb9, bb10                ; false -> the RuntimeException throw
```
so `is_array`/the guard is not the defect. The failing ladder is the one at span 195:71, which
the vendor line ties to `$count = $loaders instanceof \Countable ? $loaders->count() : 0;`, and the
runtime object probed for `ArrayAccess` there is `Argument\ServiceLocator`. Therefore
**`EnvVarProcessor::$loaders` is the `Argument\ServiceLocator`**, not the
`RewindableGenerator` the generated service passes:
`new EnvVarProcessor($container, new RewindableGenerator(fn () => yield 0 => secrets.env_var_loader, 1))`.
`ServiceLocator` is `Countable` (ServiceCollectionInterface extends Countable) but not ArrayAccess;
the count ladder's fallback then indexes it as an array and raises.

Next: break at `EnvVarProcessor::__construct` / `getenv` entry on a cold list and inspect the
`container`/`loaders` promoted-property slots, comparing the value passed by
`getContainer_EnvVarProcessorService` with what the constructor/property store ends up holding.

The exact `EnvVarProcessor` constructor shape (two promoted props `container`/`loaders`, body
`$this->originalLoaders = $this->loaders = $loaders ?? new \ArrayIterator();`, then
`$loaders = $this->loaders; $count = $loaders instanceof \Countable ? $loaders->count() : 0;` and
a following `$this->loaders = new \ArrayIterator();`) passes in isolation (count = 1, clean heap).
The generated `getContainer_EnvVarProcessorService` passes
`new EnvVarProcessor($container, new RewindableGenerator(fn () => yield 0 => secrets.env_var_loader, 1))`,
so the constructor inputs are the container and the generator — neither is a ServiceLocator.
Yet at runtime `$loaders` reaches the count ladder as `Argument\ServiceLocator`. The remaining
suspects are the service-factory execution context (the preloaded container's per-service file is a
dynamic `load`/eval include, while `EnvVarProcessor` and its methods are AOT) and the property
slot the AOT `getEnv` reads for `$this->loaders`.

Next: break at `EnvVarProcessor::getenv` on a cold list, dump the receiver's property slots and
identify which slot holds the ServiceLocator vs the RewindableGenerator, and inspect the AOT
`getContainer_EnvVarProcessorService`/`load` path that constructs the processor.

#### ROOT CAUSE: `$this->getEnv(...)` resolves to the wrong class's `getEnv`

Breakpoint at AOT `EnvVarProcessor::getenv` on a cold `list`: the receiver's class word is `0x1c9`
— the CONTAINER's class, not `EnvVarProcessor`. The method body therefore reads
`EnvVarProcessor`'s property slots on a Container object; the slot used for `$this->loaders`
lands on one of the container's properties that holds the `Argument\ServiceLocator`, and
`$loaders instanceof Countable ? $loaders->count()` reaches the count ladder's fallback on the
locator, raising "Cannot use object of type …ServiceLocator as array".

IR of `Container::getEnv` line 365:
```
v132 = load_local slot[0]                              ; $this (Container)
v133 = first_class_callable_new v132 data#14042 profile=elephc
...
prop_set v137 v139 data[14041]                          ; $this->getEnv = the callable
```
The target string is `object::getEnv` (`CallableTarget::Method` -> `format!("object::{}", method)`).
Codegen takes `emit_instance_method_first_class_callable`; because `Container::getEnv` is
overridden below (`ContainerBuilder`), it routes to `emit_object_receiver_first_class_callable`
with the `Container` filter, which builds a runtime receiver-bound descriptor from
`mixed_first_class_method_candidates` and `runtime_instance_method_descriptor_template`. The
runtime dispatch then binds the closure to `EnvVarProcessor::getenv` instead of
`Container::getEnv` even though `EnvVarProcessor` does not extend `Container`.

Effect: `Container::getEnv` stores a Closure whose body is the EnvVarProcessor's method; calling it
reads the container's properties with EnvVarProcessor offsets and corrupts the env resolution.

Next: inspect `runtime_instance_method_descriptor_template` and the candidate list/class-id
dispatch for `Container::getEnv` on `Container3yplunE\App_KernelProdContainer`, and make the
receiver-bound first-class callable bind the method of the receiver's own class (Container) rather
than a same-named method from another class.

## 2026-10-23 — cold-start ServiceLocator bug: corrected root cause, two fixes, matrix green

### Retraction of the earlier "wrong class's getEnv" claim

The entry above titled `ROOT CAUSE: $this->getEnv(...) resolves to the wrong class's getEnv` is
WRONG and must not be used. Verified by breaking at
`EnvVarProcessor::__construct` and `::getenv` in the SAME binary: the constructor receiver and the
`getenv` receiver are the same address with class word `0x1c9`, so `0x1c9` IS `EnvVarProcessor`.
The earlier trace that put a Container at `0x1c9` was a different binary/run; class ids are not
comparable across binaries. `$this->getEnv(...)` resolves correctly.

### Real cause 1: the eval global sync replayed a BORROWED snapshot over the live global

Break on `__rt_throw_object_not_array` during a cold `list`: the indexed object is the
`Argument\ServiceLocator`. Watchpoint on `_eir_global__u_ENV` (macOS aarch64, symbolized build)
shows the write comes from `with_native_global_sync` (`__elephc_eval_method_call` →
`Container::make`). The eval global scope was flushed with a borrowed cell for `$_ENV`
(`flags=0x0`); the native call replaced `$_ENV` and released that cell; the eval-to-native reload
then wrote the stale pointer back, which had been recycled as the ServiceLocator cell.

Fix (`src/codegen/lower_inst/builtins/eval/`):
- `context_registration.rs`: after the native-to-eval flush, call the new
  `emit_clear_eval_global_scope_dirty(ctx)` so the flush is not mistaken for an interpreted write.
- `scope_io.rs`: `reload_eval_global_scope` now skips a clean entry for EVERY synchronized global,
  not only the shared-ref-cell ones. A clean entry is an untouched borrowed snapshot, and the
  native call may already have replaced/released it.

Result: cold `list` and cold `cache:clear` in prod now exit cleanly (no `[critical]`).

### Real cause 2: relation reflectors became non-objects when the activation cell was 0

Cold `cache:warmup` (after `list`) died with `Argument must be of type ReflectionClass, null given`
in `Symfony\Component\Config\Resource\ReflectionClassResource::loadFiles`. Trace
(`ELEPHC_EVAL_TRACE=1`) shows `getInterfaces()` for `App\Kernel` returns the right five names, but
`reflection_aot_class_flags` reports `class=false interface=false` for each: the compiled interface
has an activation cell that no startup path raises (its source file is never dynamically included),
so `interface_exists()` (which consults the activation overlay and does not fall back to the
immutable registry) answers false. `eval_reflection_full_class_object_result` then returned a
non-object, and `loadFiles` recursed on it.

Fix (`crates/elephc-magician/src/interpreter/reflection/`):
- `owner_materialization.rs`: relation lookups (`getInterfaces`/`getTraits`) pass a relation kind.
  When the dynamic existence probes reject a name that the class's OWN relation metadata named,
  the reflector is built from the compiled reflection metadata (flags synthesized from
  `reflection_class_flags` + user-defined + interface/trait; members/interfaces/traits/parent from
  the AOT tables). A name unknown to the compiled metadata still yields a non-object.
- `class_api.rs`: pass `Some(relation_kind)`.

Result: cold `list` followed by `cache:warmup` succeeds, matching PHP.

### Acceptance matrix (33 console commands, prod, cold)

Fresh oracle and native app copies, `probe-console.py` on both, `compare-console.py`:
`3 differing command(s) of 33`, all three in the allowed normalization class:
- `list`: the script path inside Symfony help texts (`bin/console` vs the absolute binary path).
- `about`: emulated PHP `8.5.0` vs oracle `8.5.10`, and `Intl locale` `n/a` vs `en_US_POSIX`.
- `secrets:list`: the hint line wraps the absolute executable path.
`cache:warmup` and `cache:clear` now match exactly (exit, stdout, stderr).

### Tests added / run

- `tests/codegen/eval_global_sync.rs` (registered in `tests/codegen/mod.rs`):
  `test_eval_native_superglobal_replacement_survives_scope_reload` (heap-debug clean) and
  `test_eval_global_flush_marks_snapshot_clean`. The second one fails on the pre-fix compiler
  (`candidate-compiler-diag1`): its `eval_globals_from_native` body has no
  `__elephc_eval_scope_clear_dirty`, while the fixed compiler emits it. Both pass now.
- `crates/elephc-magician/src/interpreter/tests/builtins_class_metadata/class_identity.rs`:
  `execute_program_reflects_declared_relation_member_without_exists_probe` (would fail before the
  fallback: the element was not an object). `class_identity` 12/12, reflection suite 97/97.
- `cargo test --test codegen_tests eval_closures` 58/58, `eval_native_method_retention` 12/12.
- `cargo build --release` warning-free.

### Open divergences recorded (unfixed)

- `ReflectionClass::getInterfaces()` order differs from PHP for `App\Kernel` (PHP:
  HttpKernelInterface, TerminableInterface, RebootableInterface, KernelInterface, DI
  KernelInterface; elephc: TerminableInterface, RebootableInterface, HttpKernelInterface, ...).
  Not fatal, but a real ordering divergence.
- `interface_exists()` on a compiled interface whose activation cell is 0 still answers false
  (the reflection fallback only repairs the relation-object path). Fixing it at the activation
  layer needs an "early-bound interface is declared at startup" rule; the conditional-declaration
  tests must stay green (`test_interface_early_binding_distinguishes_parent_linking`).

### Next

- Rebuild `public/index.php --web` with the fixed compiler (the old `symfony-web-lint` was
  deleted) and replay prod + dev web matrices (routes, 404, dev error page), then the eval-surface
  and compile-time/binary-size measurements.

## 2026-10-23 — web entry replay with the cold-start fixes (`symfony-web-fix2`)

Rebuilt `examples/symfony-app/public/index.php --web` with `candidate-compiler-fix2` (eval global
sync + relation reflection), `--ini opcache.preload=<abs>/var/cache/prod/App_KernelProdContainer.preload.php`,
one assembler worker, prod cache regenerated by the PHP oracle first. Executable 213,096,920 B
(the 1.18 GB `.s` was deleted after the build). Wall time observed in this run: about 5 minutes,
which is ABOVE the ledger's earlier 40 s measurement for the same shape; the earlier run used a
warm runtime cache and an unloaded machine, so the 1-minute target needs a clean re-measurement
before it is claimed.

`probe-routes.py` (PHP built-in server vs `index --listen`, six routes):

| mode | result |
| --- | --- |
| prod (APP_ENV=prod APP_DEBUG=0) | 6/6 routes identical status + content type + SHA-256, including the 404 (993 B) |
| dev (APP_ENV=dev APP_DEBUG=1) | 5/6 identical; `/no-such-route` renders the dev error page (highlight works, worker survives) |

The dev 404 differs only by:
- the server port inside the exception message (per-run environment),
- the random `Sfdump` element id,
- the two missing `require_once` trace frames for the inlined `autoload_runtime.php` include (open
  item `include-frames`) and the resulting selected-line class shift.

No crash, no missing builtin, no `[critical]` line: the dev error page gap is now the trace-frame
shape only.

### Remaining campaign work

1. `include-frames`: synthesize `require_once`/`include` frames for statically inlined includes
   (both architectures), so the dev error page matches PHP modulo port/id.
2. `interface_exists` activation divergence recorded above.
3. Re-measure compile time under the ledger's conditions (warm runtime cache, one worker, idle
   host) and record before/after numbers.
4. Reduce the remaining eval surface (Twig compile-at-runtime, entry self-include) and document it.

## 2026-10-23 — include-frame attempt (complete in isolation, REVERTED: latent crash)

Goal: synthesize PHP's `require_once('<path>')` frame for a statically inlined include (the two
missing frames in the dev 404 trace).

Implemented and verified:
- `Op::IncludeFrameEnter`/`Exit` + `Immediate::IncludeFrame` (source, construct, frame slot);
  `StmtKind::IncludeOnceGuard` carries `required` so the construct is spelled correctly.
- AST→EIR: enter/exit around the guard body; depth-indexed frame slots allocated by the frame
  layout; a shared `__rt_include_frame_reader` emitted once per module; per-site descriptor
  `{function, len, 0,0,0,0, path, len}` doubling as the frame-hash info record.
- Small-program parity: `debug_backtrace()` and a caught throwable's `getTrace()` now match PHP
  byte for byte, including `require_once|main.php:7` with the resolved path argument, and a
  12-frame nested trace grows correctly.

Why it was reverted: with the reader linked, `bin/console error:dump` segfaults inside
`VarDumper\Caster\Caster::castObject` (foreach over an object's cast array) while casting a
`Symfony\Component\HttpKernel\Exception\HttpException`. Differential builds isolate it:
- record push + pop, reader slot left 0 (`--keep-symbols` console): `error:dump` OK;
- reader linked, returning the real frame: SIGSEGV;
- reader linked, returning an empty array: SIGSEGV (so it is not the frame contents);
- no record at all: OK.
Any extra frame in those throwable traces corrupts a boxed value (a Mixed cell with tag 11 and
payload `0x10` reaches the caster). That points at a latent trace-array/throwable-payload bug
surfaced by one more frame, not at the reader itself; it needs its own diagnosis before the feature
can ship. The feature is fully described above so it can be re-applied once that bug is fixed.

State after revert: `cargo build --release` clean; console binary 174,606,136 B; 33-command matrix
against the PHP oracle is back to `3 differing command(s) of 33` (`list`/`about`/`secrets:list`,
all execution-path/PHP-version/locale normalization); `error:dump` exit 0.

Open item renamed: `include-frames` is blocked on the trace corruption above (reproducer:
`candidate-compiler-fix3` + `bin/console error:dump` in a disposable app).

## 2026-10-23 — dev console matrix: two real gaps, root causes, measurements

Prod and dev matrices use the same 33 cases; the dev probe is `probe-console-dev.py` (a copy of
`probe-console.py` with `APP_ENV=dev APP_DEBUG=1`). Dev result on a fresh copy, caches cold:
`9 differing command(s) of 33` — `list`/`about`/`secrets:list` (the allowed normalization) plus
six real failures:

1. `config:dump-reference`, `debug:autowiring`, `debug:config`, `debug:container`,
   `lint:container` all die with
   `Typed property Symfony\Component\DependencyInjection\Container::$parameterBag must not be
   accessed before initialization`.
   Root cause (traced): `BuildDebugContainerTrait.php:66` calls
   `$dumpedContainer->getParameterBag()` after `$dumpedContainer = unserialize(<cache>.ser)`.
   The interpreter's `unserialize()` hydration takes the **native/AOT branch** for that
   `ContainerBuilder` and writes every payload key through `values.property_set` with the raw
   serialized name (`\0*\0parameterBag`): the protected key becomes a dynamic property and the
   declared typed slot keeps its uninitialized sentinel. The eval-declared branch already strips
   the mangling and stores the declared property; the native branch does not.
   Why the class takes that branch only in this flow: after AOT code has touched the class,
   `class_exists()` answers true, so no autoload declares it in the interpreter; before that
   touch the autoloader loads the vendor file through eval and the eval branch hydrates it
   correctly. Reproducer: `new ContainerBuilder()` in AOT, then eval
   `unserialize(file_get_contents('var/cache/dev/App_KernelDevDebugContainer.ser'))` →
   the typed property is uninitialized; without the prior `new` it restores fine.
   FIXED. A new runtime shim `__elephc_eval_value_hydrate_property` (emitted with the eval-bridge
   property helpers, both arches) forwards to the compiled decoder's `__rt_obj_store_prop`
   declared-slot matcher, and `RuntimeValueOps::hydrate_declared_property` lets the interpreter's
   native hydration branch use it. Two details were required: the Mixed object cell must be
   unboxed (`ldr x0,[x0,#8]` / `mov rdi,[rdi+8]`) because the matcher reads the class id from a RAW
   object header, and the matcher's array path must store a tag-4 indexed array directly instead
   of walking it as the decoder's tag-5 parsed hash. The first attempt omitted the unbox and
   passed the cell, which walked garbage and died at PC=0 after 41 hydrations; with the unbox the
   real `.ser` graph restores and `debug:container` passes.
   Regression test: `tests/codegen/serialize/hydration.rs`
   (`test_unserialize_restores_protected_typed_property_after_native_class_touch`).

2. `error:dump` (dev only, and only under the acceptance sandbox) dies with
   `stream_get_contents(): Argument #1 ($stream) must be of type resource, false given`, and the
   `Sfdump` page leaks to stdout. Root cause: `fopen('php://memory', 'r+')` is emulated with
   `__rt_tmpfile` (`mkstemp("/tmp/elephc-XXXXXX")`). The sandbox confines writes to the disposable
   app and sets `TMPDIR` inside it, so mkstemp fails and fopen returns false; PHP's
   `php://memory` uses no filesystem. Verified with the native `/tmp/memstream/main` under the same
   profile: `is_resource(fopen('php://memory','r+'))` is false under sandbox, true without it.
   FIXED by making `__rt_tmpfile` honor `TMPDIR` (both arches): it builds `<TMPDIR>/elephc-XXXXXX`
   in a 256-byte frame buffer, falls back to the static `/tmp` template when `TMPDIR` is unset or
   empty, and caps the prefix so the suffix always fits. `tmpfile()` now also matches PHP, which
   fails instead of falling back when `TMPDIR` is unusable (verified against php 8.5.10).
   Regression test: `tests/codegen/io/streams_ext.rs::test_tmpfile_honors_tmpdir_like_php`.
   Note: `php://memory` is still emulated with a temp file, so it still needs a writable TMPDIR;
   a real in-memory stream remains the only fully PHP-equivalent implementation.

### Final dev matrix (same acceptance harness)

`3 differing command(s) of 33` — `list`, `about`, `secrets:list`, the same three allowed
normalizations as prod. Prod re-verified with the same compiler: `3 differing command(s) of 33`.
Web re-verified: prod 6/6 routes byte-identical; dev 5/6 with only the port, the `Sfdump` id and
the two `require_once` frames.

### Measurements after the fixes (release compiler fix4, warm runtime cache, 1 assembler worker)

| build | wall | user | max RSS | executable |
| --- | --- | --- | --- | --- |
| `bin/console` (prod) | 36.62 s | 58.77 s | 4.47 GB | 174,606,136 B |
| `public/index.php --web` + prod preload | 46.67 s | 78.69 s | 5.45 GB | 213,096,920 B |

Both are inside the campaign targets (< 1 min wall, < 250 MiB binary). The earlier ~5-minute web
observation was host contention during parallel lldb/build runs, not a regression.

### Web replay with fix4 (prod and dev)

- prod: 6/6 routes identical (status, content type, SHA-256), 404 included.
- dev: 5/6 identical; the dev 404 renders (highlight works, worker survives) and differs only by
  the server port, the random `Sfdump` id, and the two missing `require_once` trace frames
  (`include-frames`, still open).

### Known pre-existing red (not a regression)

`tests/codegen/serialize.rs` →
`test_unserialize_refuses_classes_php_marks_not_serializable` still fails on
`blocked: __PHP_Incomplete_Class` with `Fatal error: eval() runtime failed`. Verified against both
`candidate-compiler-diag1` (pre-fix) and `candidate-compiler-fix4`, so it predates this
campaign's fixes and is unrelated to the hydration change (the rest of that module passes: 71/72).

## 2026-10-23 — include frames SHIPPED, plus the trace-property bug they exposed

The two `require_once` frames missing from the dev 404 trace are now synthesized for every
statically inlined `require_once`/`include_once` body.

Design (replaces the reverted attempt, with the two defects that made it crash fixed):
- The resolver now carries `required` on `IncludeOnceGuard`; `Op::IncludeFrameEnter`/`Exit` and
  `Immediate::IncludeFrame { source, required, slot }` bracket the inlined body.
- Each site gets its own 48-byte activation record in the function's frame (the entry body is
  synthetic, so the layout allocates these independently of the backtrace activation). The record
  is linked into `_exc_call_frame_top`: `[0]` previous, `[8]` no cleanup, `[16]` the site
  descriptor (the trace walkers only compare `[16]` with the record to recognise an ownership
  guard), `[24]` the shared reader, `[32]/[40]` unused.
- `__rt_include_frame_reader` (new, both arches) reads the descriptor from `[16]` and builds the
  frame with `__rt_backtrace_frame_hash`: `{file, line, function, args}` with
  `function = require_once|include_once`, the include statement's file/line, and the resolved
  path as the single argument. PHP keeps an include frame's `args` even under
  `DEBUG_BACKTRACE_IGNORE_ARGS`; the reader follows that.
- The previous attempt's crash had two independent causes, both fixed here:
  1. the reader passed the boxed Mixed cell to the declared-property matcher instead of unboxing
     to the raw object pointer (the `error:dump` fault in `Caster::castObject`), and
  2. **the Throwable `trace` property descriptor declared the slot as a boxed Mixed cell while
     `__rt_throwable_capture_trace` stores a RAW array pointer**. `(array) $exception` returned
     the trace as an integer and a non-empty trace faulted; VarDumper's `Caster::castObject`
     crashed on exactly that. The descriptor now tags the slot as an indexed array (offset
     `TRACE_OFFSET` + name `trace` + declared Mixed), mirroring `PropertySlot::release_type`.
     This bug was pre-existing and explainable on its own: `(array)(new Exception)` crashed
     without any include frame.

Verification:
- PHP-equivalence tests (`tests/codegen/callables/core_backtrace_regressions.rs`):
  `test_core_backtrace_regression_include_once_frame` pins
  `show|inc.php:2|0|require_once|main.php:10|1|` for `debug_backtrace()` and
  `boom|require_once|` for a Throwable trace;
  `test_core_backtrace_regression_exception_trace_property_casts_to_array` pins
  `array:1:array` for `(array)$e["\0Exception\0trace"]`.
- Dev web 404: **21 frames on both sides**, identical method/file/line for 20 of them (the two
  `require_once` frames now match byte for byte, `index.php:5` with the resolved path).
- `bin/console error:dump` (dev, the crash point) exits 0 under the acceptance harness; the dev
  console matrix is still **3/33** and the prod console matrix **3/33** (the allowed
  list/about/secrets:list normalizations). Web prod is **6/6** byte-identical.
- Focused suites: backtrace regressions 15/15, throwable 64/64, get_trace 2/2, magician
  unserialize 16/16, class_identity 12/12, eval-global sync 2/2, tmpfile 5/5; release build
  warning-free.
- Sizes: console 174,655,496 B; web 213,096,728 B (within the campaign targets).

### Remaining dev-404 delta (pre-existing, not include frames)

One frame in the previous-exception trace loses its `file`/`line`:
PHP `matchRequest|Router.php:188`, native `matchRequest|-`. It was already missing in the
fix11 build (before include frames) and reproduces independently of the preload. The flattening
probe in the app (`FlattenException::createFromThrowable($prev)`) preserves the file, so the loss
happens in the web error-rendering path (the page's trace starts at
`CompiledUrlMatcherTrait.php:70`, a path the CLI probe does not take). Consequence: the selected
source line is highlighted differently. Everything else on the page matches modulo the port and
the random `Sfdump` id.

### Eval-surface reduction experiment: AOT-preload the dumped Twig templates

The remaining runtime-compiled family is `var/cache/prod/twig/*.php` (two dumped template classes in
this app) plus the entry self-include. A generated wrapper preload
(`target/framework-audit-20261020.Wcomr7/symfony-preload-twig.php`) requires the prod container
preload and both template files, so `--ini opcache.preload=<wrapper>` takes their declarations into
the closed world:

- **Correctness: prod 6/6 routes byte-identical** with the Twig-preloaded binary.
- **Cost: binary 214,850,184 B (+1.75 MB)** and the build is clearly slower.

The build-time comparison is **confounded**: the host was running another worktree's full
`codegen_tests` suite (load average 40+), so a re-measurement of the adopted compiler gave 88 s
console / 103 s web on the same loaded host, versus 36.62 s / 46.67 s on the idle host recorded
above, and the Twig build took 188 s under that same load. The honest statement is: the Twig
preload adds ~1.75 MB and a measurable, apparently large compile cost for two interpreted includes;
it was **not adopted**, and the comparison must be re-run on an idle host before any conclusion
about the exact factor.

The documented boundary stands: prod 30 interpreted includes (22 on-demand vendor class files, 7
`var/cache` files including the templates, and the entry self-include), dev 135; the AOT container
is what removes the dev-generated container. Reduction candidates remain Twig AOT, the entry
self-include, extending the preload/autoload closure to the on-demand routing/console/twig-bundle
classes, and the Composer eager `files` include.

### Remaining dev-404 frame delta (pre-existing, tracked here)

`matchRequest` in the previous-exception trace: PHP `Router.php:188`, native no file/line. This was
already present before include frames (fix11) and is independent of the preload. A CLI probe that
boots the kernel and flattens the previous exception in the app preserves the file
(`FlattenException::createFromThrowable($prev)` frames: `match|UrlMatcher.php:93`,
`matchRequest|Router.php:188`, `matchRequest|RouterListener.php:101`), so the loss happens in the
web error-rendering path whose trace starts at `CompiledUrlMatcherTrait.php:70` (the compiled
matcher path the CLI probe does not take). Consequence: the selected source line is highlighted one
frame off. Next: reproduce the web trace with the compiled matcher (warm the dev router cache so the
request uses `CompiledUrlMatcher`), then compare the frame the renderer receives against
`$exception->getTrace()`.

## 2026-10-23 — dev 404 trace provenance: the last frame now matches PHP

Two fixes closed the `matchRequest` frame gap and a real PHP divergence found on the way:

1. **Compiled→eval call-site propagation.** The matcher call from compiled `Router::matchRequest`
   dispatches through the eval bridge (`__elephc_eval_method_call`). The interpreter built the
   callee's synthetic frame from `context.call_site()`, which the bridge had never seeded, so the
   frame read `UrlMatcher::matchRequest@:0` (the dev page's file-less `matchRequest`).
   `crates/elephc-magician/src/ffi/compiled_site.rs` now reads the compiled caller's published
   `_php_site` record (`{file descriptor, line}`) and `eval_method_call_inner` installs it before
   dispatch. The eval trace now shows `method_call_start ... file=/.../Router.php line=188`, and
   the trace frame is `UrlMatcher::matchRequest@Router.php:188`.
2. **Runtime include `__DIR__`/`__FILE__` canonicalization.** PHP resolves an include target:
   `__DIR__.'/../b/two.php'` answers `<root>/b`, not `<root>/a/../b`, and PHP's `__FILE__` is the
   resolved path (`/private/var/...` for a `/var/...` temp dir). `include_exec.rs` canonicalizes
   the path (with the literal path as the fallback for stream wrappers). Composer's autoloader and
   the runtime-generated Symfony container include through such paths.

Measured PHP behavior (php 8.5.10, macOS): `sys_get_temp_dir()` gives `/var/folders/.../T`, while
an included file's `__DIR__`/`__FILE__` carry `/private/var/folders/.../T`. The Magician include
test was comparing against the non-canonical spelling; it now compares against the resolved one.

Tests: `test_core_backtrace_regression_include_once_frame`,
`test_core_backtrace_regression_exception_trace_property_casts_to_array`,
`test_core_backtrace_regression_runtime_include_dir_is_canonical` (new), and the corrected
`execute_program_include_uses_call_site_and_returns_file_result`. Focused runs: backtrace 16/16,
throwable 64/64, magician include 34/34, eval-global 2/2, tmpfile 5/5; release build warning-free.

### Final acceptance matrices with fix15 (the adopted compiler)

| run | result |
| --- | --- |
| console prod, 33 commands | **3/33**: `list` (PHP_SELF spelling), `about` (emulated PHP version + intl locale), `secrets:list` (path wrap) |
| console dev, 33 commands | **3/33**: the same three |
| web prod, 6 routes | **6/6** byte-identical (status, content type, SHA-256), 404 included |
| web dev, 6 routes | **5/6** byte-identical; the 404 has **21/21 identical trace frames** (method/file/line/args), and differs only by the server port, the random `Sfdump` id and the renderer's displayed relative-path prefix (`vendor/...` vs `symfony-app/vendor/...`, i.e. its base-dir presentation of the same absolute paths; the `href`s are identical) |

The dev 404's remaining delta is therefore environment/path presentation, not trace content: the
`require_once` frames and the `matchRequest` provenance are now byte-identical.

The build numbers 36.62 s / 46.67 s above were measured on an idle host; during this session the
host ran another worktree's full `codegen_tests` suite (load average 40-108), so the same builds
measured 81-150 s. Those loaded numbers are not a regression in the compiler; re-measure on an
idle host before quoting them.

Known pre-existing red tests in this worktree (not from these changes): the six include-variant /
warning-policy codegen tests (`include_paths` x2, `declaration_activation` x3, `function_variants`
x1) and `test_an_include_and_an_eval_are_frames_of_their_own` (an interpreter frame duplicates the
include frame; reproduced on fix13, before this session's fixes).

## 2026-10-23 — eval frontier: the preload was being ignored in CLI probes, and the real AOT surface

The earlier CLI inventories undercounted the AOT effect because a CLI build only honors
`opcache.preload` when `opcache.enable_cli=1`; `opcache_cache_enabled_with_overrides` ANDs
`opcache.enable` with `enable_cli` for the CLI SAPI. Builds with `--ini opcache.enable_cli=1
--ini opcache.preload=<prod preload>` are the correct CLI analogue of the `--web` build.

Measured on a compiled CLI probe (`App\Kernel('prod', false)` + `GET /`, fix15):

| preload | interpreted includes | dynamic method dispatches | binary | output |
| --- | --- | --- | --- | --- |
| none (old probe) | 89 | 345 | 172,983,400 B | 200:695 |
| prod container preload | 11 | 15 | 212,549,576 B | 200:695 |
| + routing matcher family + TwigBundle + `url_matching_routes.php` | 5 | 11 | 213,889,352 B | 200:695 |
| + the two dumped Twig templates | **3** | **0** | 214,319,480 B | 200:695 |

The remaining three includes are **value-producing** includes: the prod container stub and its
`bundles.php` (the kernel takes the returned container) and `url_matching_routes.php` (the router
takes the returned routes). `compiler_included_skip` does not apply to them because
`value_used` is true.

Web validation with the full wrapper: prod **6/6 byte-identical** (binary 214,833,944 B, build
98.8 s on the loaded host). The marginal Twig-template cost here is only ~2 s / +0.4 MB, so the
earlier 188 s reading was confounded by host load and should not be quoted as the Twig price.

**The vendor/routing extension is NOT dev-safe.** A dual-env web binary built with the container
preload plus the routing matcher family (even without the Twig templates or the compiled routes)
serves prod 6/6 but fails dev with `InvalidArgumentException: ... Method "home" on class
"App\Controller\HelloController" should be public and non-abstract` after the route matched.
The container-preload-only binary (fix15) serves dev correctly. So the preloaded routing classes
perturb the dev controller's reflection metadata; the extension can only be offered as a
prod-only deployment recipe until that interaction is fixed. The prod-only full wrapper is
documented (3 includes / 0 dispatches) but is not the acceptance binary.

## 2026-10-23 — preloaded class lists redeclared AOT classes in dev (fixed): the full AOT wrapper is now dev-safe

Root cause of the dev break discovered with the preload audit: a wrapper preload that `require`s the
Symfony container preload makes the pipeline process that file's `$classes[] = '…';` list
(`named_class_requires`). Those class declarations are merged into the program from the preload
graph, but their source files were **not** in the module's source catalog, and they were not in
`preload_compiled_whole` either (that set only carries fully-declared files of the preload's own
require graph). At runtime Symfony's generated service factory does
`include_once '…/src/Controller/HelloController.php'`; because the file was not marked
compiler-included, the interpreter executed it, declared an **eval** `HelloController` that
shadowed the AOT class, and `ReflectionMethod::isPublic()` on its `home` method answered false
(`InvalidArgumentException: Method "home" … should be public and non-abstract`). Base builds (the
container preload passed directly) never processed the `$classes` list, so the autoload pass loaded
the controller normally and dev worked.

Fix in `src/pipeline.rs`:
- The preload graph's fully-declared sources and the preloaded class files are now added to the
  module's source catalog (canonical paths), so their declarations carry a real source row.
- Every preloaded class file whose class actually reached `ir_module.class_infos` is added to the
  compile-time inclusions, so its guard cell is `COMPILER_INCLUDED` and the interpreter skips the
  factory's `include_once` instead of re-declaring the class. Deferred classes stay out of the set.

Regression test: `tests/opcache_preload_tests.rs`
`a_class_file_named_by_the_preload_list_is_not_reincluded_at_runtime` — the class file prints a
top-level marker; with fix15 the runtime `include_once` ran it (`Xpublic:v`), with fix16 only
`public:v` (the include is skipped and the class stays AOT/public). OPcache suites green:
preload 20/20, manifest 15/15, ini 25/25, closed-world 10/10.

Adopted AOT wrapper (generated in the audit dir, no app/vendor change): the prod container
preload + the routing matcher family (`RedirectableCompiledUrlMatcher`, `CompiledUrlMatcher`,
`CompiledUrlMatcherTrait`, `RedirectableUrlMatcherInterface`, `UrlMatcher`), `TwigBundle.php`,
`var/cache/prod/url_matching_routes.php` and the two dumped Twig templates. Results with fix16:

| build | prod | dev | binary |
| --- | --- | --- | --- |
| container preload | 6/6 | 5/6 (21/21 frames) | 213,097,048 B |
| full wrapper (fix16) | **6/6** | **5/6 (21/21 frames)** | 214,833,944 B |

The CLI prod probe with `opcache.enable_cli=1` + the full wrapper reports **3 interpreted includes
and 0 dynamic dispatches** (down from 89/345 without the preload); the three remaining includes are
value-producing: the prod container stub, its `bundles.php`, and `url_matching_routes.php`. Console
matrices with fix16 remain **3/33** in prod and dev. Build wall time with the wrapper was 97.6 s
under the other worktree's load (100+); the idle-host container-preload baseline is 46.67 s, and
the wrapper's marginal cost is small — a clean idle measurement is still pending.

Remaining eval frontier: the three value-producing includes above and the web entry self-include
(`autoload_runtime.php` requires `$_SERVER['SCRIPT_FILENAME']` and takes its returned closure).

### Next frontier: the value-producing includes and the entry self-include

The full AOT wrapper leaves exactly one interpreted include on the dev web path for the entry:
`autoload_runtime.php:9` does `$app = require $_SERVER['SCRIPT_FILENAME'];`, which re-executes
`public/index.php`. The entry's shape is a compile-time-known no-op plus one value:

```php
require_once dirname(__DIR__).'/vendor/autoload_runtime.php';
return static function (array $context) { return new Kernel($context['APP_ENV'], (bool) $context['APP_DEBUG']); };
```

The nested include's only meaningful effect is returning that capture-less closure (the
`require_once autoload_runtime.php` is already included and is a no-op on the second pass). A safe
specialisation would evaluate the trailing `return <capture-less closure>` once at startup into a
runtime global and have the interpreter's include of the **entry source path** return that value
instead of executing the file (a new native hook + include check; the compiler must restrict it to
the entry's literal-closure shape so no top-level side effect is skipped). The three remaining
prod includes (container, `bundles.php`, `url_matching_routes.php`) are value-producing the same
way — the container/kernel and the router consume the returned values — and would need the same
return-value caching or a dedicated AOT constructor for the dumped container.

These are the next targets; the adopted wrapper already removes 86 of the 89 includes and all 345
dynamic dispatches measured in the CLI prod probe.
