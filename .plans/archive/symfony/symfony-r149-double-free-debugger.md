# r149 worker double-free: debugger evidence

2026-09-07. Acceptance remains open: stock public/index.php compiled with
--web must serve the default Welcome to Symfony page with HTTP 404.

Attached LLDB to the single worker of the existing target/symfony-web-r149/index,
launched from examples/symfony-app. No full rebuild and no source fixture changes.
The binary has stripped runtime symbols; obtained the heap-debug failure address
from the allocator's validation branch. In this process the breakpoint was at
0x10582c344. Addresses are process-specific and must be rediscovered on relaunch.

The breakpoint intercepted the exact double-free message. Backtrace:

```
0 0x10582c344 heap debug failure
1 0x1058340dc mixed-cell destruction, after freeing its payload
2 0x102c7fae4 compiled caller
```

Mapped the caller's instruction sequence to the retained index.s, inside
ResolveBindingsPass::processValue, at the return assignment on source line 152
(not the subsequent instanceof, despite that being the last eval trace).
Assembly around lines 12328510-12328530 contains:

```
; @src line=152 col=30 end=152:68 op=acquire
sub x9, x29, #3296
ldr x0, [x9]
str x0, [sp, #-16]!
bl __rt_incref
ldr x0, [sp], #16
; store_local follows
; @src line=152 col=30 end=152:68 op=release
sub x9, x29, #3296
ldr x0, [x9]
bl __rt_decref_mixed
```

The freed cell was 0x1095f6040; its header already had refcount zero and kind
zero, with payload size 32. Need identify the first free/actual return shape;
incref versus decref_mixed alone is not proof of a bug, because retaining a
boxed cell can legitimately use generic incref.

The initial web reducer and variants with eval helper returning a nullable
ReflectionFunctionAbstract, compiled nested arrays and getParameters all pass.
Next reducer moves the nullable-return helper into AOT and leaves reflection
creation in a dynamically included function. Keep the test honest: it has not
yet reproduced the full failure. A separate CLI probe currently asserts an
unproven clean heap and fails on live eval metadata; this remains to resolve.

Detached debugger and stopped this server cleanly after collecting evidence.
Kage context call expired after 300 seconds; no result was available.

## Continuation: reduced and traced the return boundary

The web reducer now reproduces the original double-free when the nullable
return helper is compiled and reflection creation is dynamically included.
Its return nominal guard validates the object but forwards the same boxed
Mixed cell. The epilogue did not trace through that guard and freed the
reassigned parameter slot while returning its dead cell.

An initial retain after the guard let request one pass but leaked on an
unmodified borrowed parameter. That attempt was removed completely. The
current fix extends return_cleanup_skip_slot_inner only for nominal guards
that forward Mixed/Union cells; concrete-object guards retain independent
unboxed payloads and remain excluded. Guard traversal preserves the checked
source representation when identifying the transferred local slot.

A second ownership defect surfaced in the recursive reducer: call-argument
cleanup transfers an owned argument to an identical result, but retaining-store
cleanup also skipped releasing that result. user_call_result_may_alias_argument
now reserves that suppression for borrowed arguments.

Focused verification on 2026-09-07:

- Three runtime_gc::nominal_return tests pass with a clean heap: borrowed
  recursive argument including null, owned mixed call result, reassigned mixed
  parameter. Recursion prevents trivial inlining from hiding the boundary.
- Two test_accumulator regressions pass (direct and interface dispatch).
- Four test_conditional_return_callee regressions: non-alias and container
  non-alias pass; alias and alternating branches still leak 20 and 10 cells.
  Baseline attribution remains to verify; do not call this filter green.
- The first web run with the initial retain passed request one, then failed
  request two with a dynamic class redeclaration. Dynamic class metadata was
  not reset alongside functions/includes/autoload contexts. Added request reset
  of GlobalEvalClassRegistry, preserving the separate immutable AOT snapshot.
  Final web replay of both fixes PASSES: two requests in the same worker,
  test web_eval_reflection_constructor_survives_array_roundtrip, 20.59 seconds.
- Frame unit test nominal_return_guard_transfers_only_forwarded_boxed_local
  passes. Its first compile used the nonexistent PhpType::Null; corrected to
  this IR's PhpType::Void nullable member before the passing run.
- Source/crates/tests diff hygiene passes. Full diff hygiene reports existing
  whitespace in generated examples/symfony-app/config/reference.php; untouched.

Full rebuild r150 started after these focused gates, with:

```
ELEPHC_BACKEND_INVENTORY=1 CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 RUST_MIN_STACK=67108864 /usr/bin/time -l target/debug/elephc --web --heap-size=33554432 --with-regex --timings --output-dir target/symfony-web-r150 examples/symfony-app/public/index.php
```

Stdout/stderr are recorded as target/symfony-web-r150.compile.stdout/stderr.
Free disk at launch: 5.2 GiB. Existing parallel DateTime work was left alone.

The prior CLI eval heap-clean assertion has been replaced with a differential
test of the same binary at one versus sixteen iterations, comparing live heap
summaries while requiring identical valid output. This updated test has NOT
been run yet; do not claim that persistent metadata explains all retention.

### r150 performance sample (not yet an optimization verdict)

At about 78 seconds, a one-second sample of the compiler caught method type
checking with deeply nested fluent-call receivers. Only two stack samples were
captured, so this is a lead, not a statistical profile. Saved report:
target/symfony-web-r150.frontend.sample.

In src/types/checker/inference/expr/effects.rs around 757, the method-call arm
first infers object_type, then clones the receiver expression into a
CallableTarget and asks resolve_first_class_callable_sig, which re-infers that
same receiver at src/types/checker/callables/first_class.rs around 203. For long
chains this repeats traversal of successively larger receiver subtrees.
Potential next optimization: reuse the already inferred receiver type through
a shared signature-resolution helper, preserving visibility, magic-call,
callable __invoke and by-reference argument semantics. Do not introduce an
environment-insensitive inference cache or skip semantic checks. No code in
this subsystem has been changed in this continuation.

At about four minutes, a second short sample caught
emit_runtime_call_wrapper_inline in src/codegen/lower_inst/runtime_wrappers.rs
around 106 cloning ctx.module in order to build an extern-callable wrapper.
The sampled clone was copying the complete functions vector and each
function's blocks/values. Caller: shared_mixed_callable helper generation via
runtime_extern_descriptor_cases. Report:
target/symfony-web-r150.backend.sample. This is a concrete avoidable-copy lead,
but no speedup is measured yet. A replacement must preserve the metadata and
data-pool identifiers required by wrapper lowering, and builtin wrappers may
need more module information than extern wrappers. Do not blindly clear module
metadata or retain another complete module throughout code generation under
this machine's memory budget.

## r150 end-to-end result: still no HTTP response

Compilation passed: 476.85 s wall time, 115 MiB executable, 832 MiB assembly.
Phase times: checking types 93.68 s, declaration pruning 66.72 s, EIR lowering
33.15 s, EIR optimization 38.47 s, native generation 114.51 s, assembly 114.97 s,
link 857 ms. Runtime cache miss; eight assembler slices, none reused. Peak RSS
2,551,644,160 bytes; process footprint peak 4,510,339,296 bytes.

Run from examples/symfony-app with --listen 127.0.0.1:19080 --workers 1.
GET / returned no bytes (curl 52, HTTP 000). Worker no longer double-frees at
the old constructor assignment; it exhausts its 32 MiB heap after 23.75 s:
731779 allocations, 473822 frees, live 33457952 bytes. A traced replay also
exhausted the heap after 26.39 s, reaching RegisterServiceSubscribersPass.
Both servers and their workers were stopped afterwards. Logs stay under
target/symfony-web-r150.*; trace log is 7.7 MiB.

Two concrete memory leads, not yet closed:

1. Compiled spl_autoload_unregister in src/codegen/lower_inst/builtins/spl.rs
   still emits an unconditional true without removing a callback. Trace queue
   grows to 57 entries. The eval register/unregister paths only search the
   current owner context, while AOT registration creates an independent owner.
   This needs actual request-global callback identity/removal, not another
   placeholder. update-builtin-docs skill read and announced; no autoload
   implementation changed yet. PHP source baseline is php-8.5.6 peeled commit
   fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633, ext/spl/php_spl.c.
2. A corrected CLI differential test (one versus sixteen calls, same argument
   count and same-length mode arguments) leaks 1440 blocks / 139440 bytes for
   the fifteen extra constructor inspections: 96 blocks / 9296 bytes per call.
   Baseline: 130 blocks / 13488 bytes; repeated: 1570 / 152928. The initial test
   wrongly changed argc itself, retaining extra argv entries; it now uses
   argv[1] once/many and has been rerun red before the next fix.

The small generated test permits changing only its dynamically included runner
without recompiling. In that diagnostic copy (not the source test), no-op
inspection proved argv accounted for two blocks / 128 bytes per extra CLI
argument. Direct getConstructor + instanceof still leaked substantially;
explicit unset only removed one cell, leaving the object graph alive.

Current fix being tested: __rt_mixed_from_value retains object payloads, but
emit_reflection_owner_new_aarch64 / x86_64 never released their fresh allocation
reference after boxing. Added emit_release_boxed_reflection_owner on BOTH
architectures, preserving the boxed return in a dead input spill and releasing
the raw object. Focused CLI replay is in progress. Added a two-ABI emitter test,
not yet run. The sibling ReflectionAttribute materializers have the same
pattern and still need correction plus coverage. No successful final heap
verdict or full Symfony replay exists for this latest fix yet.

### Further ownership reductions and debugger proof

Releasing the raw allocation after boxing alone reduced the constant baseline
but not the per-call leak: 116 blocks/11824 bytes versus 1556/151264. It is a
real independent missing release, not the whole constructor retention cause.

Added eval_reflection_array_set_owned in owner_materialization.rs: array_set
borrows keys and retains values, so all six metadata-array builders release
their fresh keys/elements after insertion. Also release temporary type/bool
cells after property_set. Built Magician's top-level staticlib explicitly before
testing; cargo test alone need not refresh that archive. This reduced the
differential to 92 blocks/9088 bytes per inspection (112/11648 versus
1492/147968), still red.

Debug binary: target/reflection-retention-debug/main, built with --keep-symbols
from the retained CLI fixture. Diagnostic runner copy only was reduced to a
direct getConstructor, instanceof, explicit unset, and a string return.
Do not treat this reduced runner as the actual regression test. Runtime helper
symbols in LLDB drop one Mach-O underscore: _rt_decref_object / _rt_incref;
C bridge symbols retain __elephc_eval_*.

LLDB showed the outer ReflectionMethod payload (class id 81 in this binary)
reach refcount one and be freed on unset. Its __parameters array instead had
refcount two before owner destruction and one afterwards. The boxed array had
refcount one at __elephc_eval_reflection_owner_new entry but TWO by the Rust
values.release(method_objects) at owner_materialization.rs:341.

Conditional breakpoint on _rt_incref for that box identified the exact extra
retain: emit_set_owner_parameter_type_property_aarch64 took incoming stack
slot 120 (the callable's parameter array) and stored it into __type at offset
344. reflection_owner_layout had enabled this PARAMETER type-slot mapping for
every class with a __type property, including ReflectionMethod. Rust later
overwrites __type with the actual return type, stranding the boxed array owner.
Caller PC was 0x1003862fc in this diagnostic process; raw array 0x100d5bc00,
box 0x100d5bc50. Addresses and class ids are binary/process-specific.

Current pending test adds reflection_owner_layout_with_deferred_type for
ReflectionFunction, ReflectionMethod and ReflectionClassConstant. Their types
are set after the common materializer; do not prefill them with the unrelated
member-array argument. The shared layout fix covers both ABIs. Debugger and
diagnostic inferior are closed; the focused codegen test is running.

### September 7 continuation: condition and array insertion owners

Deferred-type fix alone left the full Reflection differential red at 110 blocks /
11552 bytes versus 1460 / 146432. Plain assignment actually returns an independent
owned result (write_location preserves it); classifying Assign as borrowed leaked
that result when used as a condition. eval_truthy_expr now consumes evaluated
temporary conditions and preserves borrowed storage across if/loops/logical
operators/ternary. Assignment copies borrowed RHS values before preserving both
storage and result owners. The destructor/alias regression
test_eval_assignment_conditions_release_owned_values_and_preserve_aliases passes.
Magician control_flow filter: 36 passed, zero failed. Full Reflection remained red
at 105 / 11248 versus 1380 / 141584.

Array append statement retained a fresh RHS after array_set had retained its own
element owner, and leaked the generated integer key. Added
test_eval_array_append_releases_owned_values_and_preserves_borrowed_values:
before fix stdout abdc instead of dabddc; after fix passed (20.57 seconds).
It covers direct objects, a borrowed object alias and a nested array of objects.
The non-object append now settles the key and owned RHS, including setter failure.
Full Reflection differential after this fix: 32 blocks / 3136 bytes versus
212 / 12064, still red (12 extra blocks per extra inspection, down from 85).
No new full Symfony replay has been performed.

Indexed array literal generated keys also lacked releases after insertion and
reference binding. A fix is built; a dedicated heap-debug regression is running.
Do not claim that regression passed until its result is recorded.

Kage refresh completed: three evidence packets, 177 approved packets, partial
graph index (4782 files, 42932 symbols, capped 50000 calls), no validation errors,
three warnings on old references. Do not overwrite stale historical claims
without verification. The Reflection packet predates the latest condition and
append fixes and needs a factual follow-up when this ownership family is closed.

Indexed literal follow-up: release generated keys and owned scalar results after
array insertion. The original empty-heap assertion included four remaining
process/eval allocations (416 bytes). Replaced it with a stricter isolated
comparison of the SAME binary / ONE eval activation / same-size CLI arguments,
varying only the number of literal elements. That differential passes (19.12 s).
Full Reflection now reports 30 blocks / 3040 bytes versus 180 / 10528, still red.

A retained diagnostic runner in the 86846 fixture (not the actual test source)
proved pure return and scalar assignment are stable at 20 blocks / 2560 bytes;
getConstructor OR getName adds one 48-byte block per invocation. Enabling the
eval trace adds ANOTHER block per method call. Both method re-binding and tracing
read runtime_object_class_name, which copies via RuntimeValueOps::string_bytes.
Its generated wrapper invoked __rt_mixed_cast_string, whose string branch calls
__rt_str_persist. Rust copies those bytes, but the detached allocation has no
owner or release path. This affects every consumer, not only Reflection.

Fix being built on both ABIs: string_bytes calls shared __rt_mixed_unbox (already
peels nested wrappers), borrows an existing string payload while Rust immediately
copies it, otherwise preserves shared __rt_value_cast_string conversion. No ABI
signature change, no framework code. Added two-ABI emitter test and a dedicated
same-binary repeated metadata-read heap regression; results pending. A new Kage
context recall timed out at 300 seconds; the earlier successful refresh stands.

### Reflection differential gate now passes; broader repeated assignment stays separate

The two-ABI string-byte view test and reflection_owner_box_releases_allocation_reference
both passed. All four initial eval_assignment regressions passed together (63.14 s).
String byte-view fix reduced the full Reflection differential to 21 blocks / 2160
bytes versus 81 / 5040, four extra cells per inspection.

Two cells were null placeholders allocated when plain assignment evaluated an
absent variable as a READ location. EvaluatedLocation now carries an optional
observation: plain writes to variables/properties/static properties carry None,
and only read locations carry Some(current). No fabricated null is needed for a
plain write. test_eval_assignment_to_absent_locals_releases_fetch_placeholders
failed at 9 / 688 versus 12 / 832, then passed after this change. The full
Reflection differential then retained just two cells per inspection.

Initial attribution of the last two cells to destructuring keys was incomplete:
changing explicit key ownership classification did not change the implicit-key
reproducer. eval_array_reference_key calls eval_int_value. That helper allocated
a boxed integer, converted it to text and reparsed it, but never released the
integer. It now preserves values.cast_int semantics, reads raw_value_word and
releases the converted cell on success/error, avoiding text conversion entirely.
This affects many builtins, so the update-builtin-docs skill was read and its
generation/audit sequence is still to be completed.

test_eval_reflection_get_constructor_result_has_balanced_ownership PASSED on
September 7 (16.37 s), with its original helper/conditions/nested arrays/foreach/
destructuring and one-versus-sixteen inspection comparison intact.
web_eval_reflection_constructor_survives_array_roundtrip PASSED on two requests
to one worker (21.30 s). These are focused gates, not Symfony HTTP acceptance.

New test_eval_destructuring_releases_literal_key_cells additionally repeats the
same destructuring assignment in one activation. It exposed an independent
same-cell replacement leak: after the integer fix its baseline was 8 / 640 versus
10 / 736. Array reads own a reference but can return the same cell already in the
destination; scope replacement intentionally treats identical cells as in-place
reuse and does not return an old owner to release. Current pending fix detaches
only colliding destructuring results using existing copy_value, releases the read
owner and transfers the detached cell through normal scope/reference replacement.
Added a signed-extremes/input-owner unit test for eval_int_value; tests pending.

Disk housekeeping: own r149/index.s and r150/index.s were losslessly gzip-compressed
from 832 MiB to 63 MiB each; binaries and logs preserved. They can be restored with
gunzip. Free space remained roughly 2 GiB, so check before a new full build.

### Validation and full r151 replay launched

Colliding destructuring assignment regression now passes (16.14 s). Latest
Magician filters: builtins_scalars 11 passed (including signed-extremes/input
owner), control_flow 36 passed, null_coalesce_assign 7 passed, destructur 11
passed (filters overlap). No local Linux executable validation yet.

The builtin-doc skill exposed pre-existing drift: tools/gen_builtins.rs omitted
UnsupportedReason::AotImplementationPending and did not compile. Added the
explicit spelling and a passing exporter unit test. Regeneration includes the
already-declared register_tick_function/unregister_tick_function eval-only
contracts and the existing readfile optional parameters. Most of the 433 tracked
doc changes are generated sidebar orders shifted by those two entries. The audit
had a stale census of thirteen exceptional routes; updated to fifteen, with
explicit checks for the two pending-AOT reasons. Docs audit zero errors, site
compatibility 1118 pages passed, EIR boundary inventory zero structural errors.

Disk fell below one GiB despite compressing our old asm. The user had authorized
inactive other-worktree Cargo cleanup. Verified no active compiler/process for
agent-a893f129bbb0788f9 and that target was a real child build directory, then ran
cargo clean with its exact manifest and target paths. Removed 18587 regenerable
build files, Cargo-reported 8.8 GiB. Sources and the DateTime worktree/cache were
untouched. Free space recovered to 9.5 GiB (later ten).

Full r151 command now running, session 86510, compiler PID 9774:

```
ELEPHC_BACKEND_INVENTORY=1 ELEPHC_ASM_JOBS=2 CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 RUST_MIN_STACK=67108864 /usr/bin/time -l target/debug/elephc --web --heap-size=33554432 --with-regex --timings --output-dir target/symfony-web-r151 examples/symfony-app/public/index.php > target/symfony-web-r151.compile.stdout 2> target/symfony-web-r151.compile.stderr
```

Assembler concurrency explicitly limited to two for resource safety. No full
r151 build or HTTP result yet. Its compiler was around 527 MiB RSS at 55 seconds.

Read-only r151 profiling reconfirmed the frontend lead in
target/symfony-web-r151.frontend.sample: 23 captured compile-thread samples were
under method type checking, recursively traversing effects.rs:757 receiver
chains. That branch already computes object_type, then deep-clones object into
CallableTarget::Method and calls first_class.rs:203, which infers the same
receiver again. A prospective optimization should extract the existing method
signature resolver into a shared helper accepting an already inferred receiver
type and borrowed Expr; keep its visibility, interface/static fallback,
declared-parameter flags and __invoke special cases intact. Do NOT merely reverse
the fallback order to instance_call_effect_signature: that simpler helper lacks
those policies. No performance implementation has been made during r151.

The r151 backend sample is different from r150's whole-module wrapper clone:
175 captured compile-thread samples were in emit_module, with 36 under
runtime_instance_method_descriptor's Vec scan (shared_state.rs:319), reached
through repeated callable-normalization lookup-table emission. Prospective safe
optimization: index receiver/method cache buckets without changing full
impl_class/signature equality, insertion/label assignment order or emitted code.
A two-level map permits borrowed class/method lookup without allocating a tuple
of Strings on every hit. Other sibling Vec descriptor/invoker caches should be
audited too; this is a measured lead, not an implemented optimization.

### r151 builds, HTTP now fails at a null newInstance receiver

Build completed with exit zero: 495.11 s wall time, 115 MiB binary, 832 MiB asm.
Type checking 93.43 s; declaration pruning 65.70 s; EIR lowering 32.21 s;
EIR optimization 38.03 s; native code generation 108.81 s; assembly 145.10 s;
link 870.24 ms. Two assembler slices, zero cached. Runtime cache miss.
Peak RSS 2481274880 bytes; peak footprint 4098658192 bytes. The binary-size
threshold is met; the one-to-two-minute compilation target is NOT met.

Started the binary from examples/symfony-app with one worker on 127.0.0.1:19080.
GET / returned no HTTP bytes, curl status 52 / HTTP 000 after 21.641877 s.
Worker 12235 exited 255. Server stdout:
`Fatal error: Uncaught Error: Call to a member function newInstance() on null`.
Stderr still contains eight Undefined variable $GLOBALS warnings, but no heap
exhaustion report in this replay. Do not call this HTTP acceptance.

Parent 12185 was stopped with SIGTERM and its tool session completed cleanly.
A trace-enabled replay of the SAME binary is now running: server session 46559,
curl session 58252; outputs under target/symfony-web-r151.trace.*. No source fix
for the new null receiver has been implemented yet.

Trace pinpointed the AOT null receiver in Attribute/Target.php:50, parseName:
the prior `if (!$target = $parameter->getAttributes(self::class)[0] ?? null)`
failed to take the null branch. The receiver was the raw null sentinel
0x7ffffffffffffffe. Read framework code only; no vendor edit.
Both trace server parent 12603 and its idle worker were stopped via parent
SIGTERM after the failed replay; server session completed. No active r151 server.

Generic reducer test_nullable_object_array_coalesce_assignment_condition uses
only an ordinary object array, a missing element/coalesce/assignment condition
and a normal method call. It reproduced `label() on null` in 1.20 s. Cause:
EIR lower_is_truthy returned constant true for Object, although raw object slots
can hold the miss sentinel or zero. It now uses the existing shared container
null predicate and inverts its boolean on BOTH ABIs. Initial reducer passed.
EIR constant folding only folds actual constant operands; no object-type fold
was found in that pass.

Sibling empty() had the same constant-false assumption. A standalone generic
CLI probe printed false:full while PHP printed false:empty. lower_empty now uses
the shared null predicate for Object. Extended the reducer to check explicit
bool casts and empty() for missing/present values. Latest native rerun pending.
object_truthiness_checks_null_on_every_supported_target PASSED for IsTruthy and
explicit Cast emission on macOS ARM64, iOS device, iOS simulator, Linux ARM64
and Linux x86_64. No Linux executable run is claimed.

Extended native object-array regression (bool cast + empty + negated assignment)
passed, expected false/empty/none:true/full/set. Built an optimized compiler in
the separate release directory without changing Cargo.toml: opt-level one,
256 codegen units, no LTO, one Cargo job, incremental disabled. Build cost
699.77 s, peak RSS 2921463808 bytes, compiler binary 41 MiB. This is tool
preparation time, not Symfony compilation time. Debug compiler is preserved.
The optimized compiler's pure object-miss CLI probe returned false:empty for a
miss and true:full for a value, with EIR optimization both enabled and disabled.

Build-time observation: src/main.rs and src/lib.rs each declare the compiler's
module tree, so Cargo compiles the core as library and again as binary. Moving
the CLI orchestration behind one library entry point is a prospective developer
build optimization; not implemented or reviewed yet.

Full r152 is running in session 9779 (a launch note, not an acceptance result), using the same
web entry, heap size, regex capability, timings and two assembler workers as
r151, but target/release/elephc with the optimized profile. Logs use
target/symfony-web-r152.compile.stdout/stderr. All code edits are paused during
the full build so its inputs stay stable. No r152 HTTP result yet.

### r152 optimized result: faster build, another heap ceiling before HTTP

Build passed in 139.44 s wall time (139.18 phase sum), still 115 MiB binary and
832 MiB assembly. Same two cold assembler slices, no cache reuse. Type checking
10.23 s; declaration pruning 13.02 s; EIR lowering 5.88 s; EIR optimization 1.38 s;
native generation 29.16 s; assembly 74.03 s. Peak RSS 4173086720 bytes and peak
footprint 4455665864 bytes: faster but higher measured RSS than r151. The size
goal is met; cold compilation remains nineteen seconds above two minutes.
Optimized compiler reused the existing tested debug bridge archives found by
the standard linker discovery order. This isolates compiler profiling, not
optimized-runtime performance.

Initial curl raced server startup and got connection refused; that is not a PHP
execution failure. After the listening log appeared, request1 ran for 20.324765 s
and returned curl 52 / HTTP 000, zero bytes. Worker 25005 exhausted the same
32 MiB heap: 759727 allocations, 552038 frees, live 33427680, peak 33427728,
bump 33554384, requested 128 bytes. No newInstance-on-null fatal in this run.
Eight Undefined variable $GLOBALS warnings remain. Acceptance is still OPEN.

Parent 24962 was stopped cleanly. Same-binary trace replay is running under
server session 30487; trace curl was launched separately with connection-refused
startup retries only. Outputs use target/symfony-web-r152.trace.*. Need locate
the final trace stage before selecting the next root-cause family.

Trace replay instead ended with worker SIGSEGV during ResolveBindingsPass on
Routing/Router metadata, after getMethod(setConfigCacheFactory), around 24.45 s.
This differs from the untraced heap fatal; do not conflate the outcomes or claim
determinism. The user then explicitly asked to prioritize Undefined variable
$GLOBALS. Trace parent 25860 was stopped via SIGTERM; no server remains active.

GLOBALS trace evidence: all eight warnings occur in a bound loader closure,
checking a per-file once map; each lookup sees array identity zero. The loader
source uses empty($GLOBALS[...][$id]), writes true and requires a file. Only read
the vendor source. Graft found no GLOBALS runtime support in Magician: its hits
were the arrow-superglobal classifier and a parser-only generated-loader test.
AOT src/globals_array.rs already rewrites supported literal keys to collision-
proof global aliases and explicitly refuses other shapes. Magician does not
apply that path; do not inject a fake empty GLOBALS array.

Pinned php-src zend_compile.c was opened at commit
fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633. Its direct GLOBALS dimension access lowers
to global-variable fetch/store, evaluating/converting the key, not to a local
array. Whole-array assignment/reference restrictions are separately checked.

Added tests/codegen/runtime_gc/eval_globals.rs with a generic bound-closure
once guard and included counter update, requiring compiled/eval global sharing.
Initial direct-AOT-closure version returned 0:seen:0 versus 1:seen:2. Updated
the fixture to define the bound closure in a dynamically included factory so
the guard itself uses eval, as in the trace; its run is pending. No GLOBALS
implementation change yet. Need distinguish absent GLOBALS handling from any
native/global-scope synchronization issue by comparing a global-keyword control.
Kage recall for this subtask is pending in functions cell 1129.

### GLOBALS implementation checkpoint after the hydration request

Authoritative current detail is .plans/globals-native-eval-parity.md. Added real
element access helpers in interpreter/globals.rs, a global writable location,
and quiet direct isset/empty/unset handling. Three interpreter globals tests now
pass, plus existing control_flow (36) and null_coalesce_assign (7) filters.
Native explicit eval scalar set/unset passes with 7:absent. Global-keyword native
callback control rerun still fails at 0:seen:0 versus 1:seen:2; do not close the
callback import/export gap. No callback ABI/protocol changes have landed.

The user additionally requested CLI/request hydration parity. Pinned php-src
main/php_variables.c and sapi/cli/php_cli.c plus the installed PHP 8.5.6 oracle
were checked. Direct ENV sentinel hydration differs (PHP fixture, Elephc missing).
GLOBALS argv/argc also has a raw-vs-Mixed startup representation mismatch; the
new native argv test is red. Correct lazy activation matters: GLOBALS-only ENV
access did not activate it in PHP, while direct ENV mentions did. Record HTTP
input parsing, REQUEST precedence, SESSION presence and request reset in the
hydration matrix rather than assuming all arrays must always be filled.

No new full Symfony build since r152; no server or Cargo job was left running
after the focused work, apart from any explicitly noted docs audit command.
Kage recaptures succeeded; cold context recalls 1129 and 1151 timed out after
300 seconds. Latest packet records the three distinct gates (syntax, native
callback synchronization, SAPI hydration). No final model consensus is claimed.
