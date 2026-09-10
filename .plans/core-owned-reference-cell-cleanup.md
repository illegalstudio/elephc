# Owned property reference-cell cleanup for PR #893

## Checklist

- [x] Confirm the boxed push and throwing usort leaks from CI on `0e17999932a5a509aaebd6cdc690f700b2f3dd9f`.
- [x] Give object-owned cells a typed heap identity without changing their two-word payload ABI.
- [x] Route object cleanup, both cycle collectors, marking, pinning and status through cell nodes.
- [x] Separate singleton cells during AOT and eval bridge cloning; share cells with real aliases.
- [x] Acquire a new property alias before releasing the target's previous object owner.
- [x] Add heap-debug regressions for surviving aliases, cloning, cycles and rebinding.
- [x] Implement ownership transfer for resolved reference-returning functions, methods and static callables.
- [x] Cover temporary receivers and locally constructed objects returned by reference.
- [x] Update runtime memory documentation for managed cell ownership and the checked lookup cost.
- [ ] Verify the complete reference-return matrix in CI, including exceptional epilogue cleanup.
- [ ] Audit unresolved descriptor reference assignment, which previously interpreted a boxed result as a raw cell address.
- [x] Complete build, static and generated-doc checks without running local tests.
- [x] Commit thematically, push `feat/core-align`, inspect CI on the exact new head.

## Storage and graph contract

Managed property cells use heap kind 7. Their payload is still two words: the value's
low word at offset 0 and its high word or string length at offset 8. Header bits
8 through 14 describe the payload using the existing property descriptor tags.
This distinguishes cells from boxed Mixed values and borrowed inline addresses.
Only object-owned reference properties receive class GC descriptor tag 11;
constructor-promoted borrowed references remain descriptor zero.

The collector follows `object -> cell -> value`. Counting an object-to-value shortcut
would miscount shared cells and fail to recognize external aliases as roots. Both
collectors pin cells during destructor callbacks and sweep them separately. Clone
decisions exclude the collector's temporary pin from the PHP reference count.

## Remaining ownership gate

Resolved reference returns now acquire a `ReturnRefCell` lease in the callee and
adopt it in the caller. A normal value use dereferences the cell before retiring
that lease. Ordinary descriptor invocations likewise box the actual value, not
the cell pointer. The regression matrix in
`tests/codegen/runtime_gc/reference_cell_owners.rs` has not been executed locally.
Its runtime results remain a CI gate, not a proven completion claim.

Acquiring only after `lower_expr(call)` is insufficient for temporary receivers or
owned argument temporaries, because call lowering releases them before returning.
Likewise, acquiring only in the caller is insufficient when a by-reference callee
returns a property of a local object that its epilogue destroys. A complete solution
must also distinguish ownership transfer from ordinary by-value consumption of a
reference-returning call. The new ownership lookup checks exact allocation
boundaries to reject borrowed frame/interior addresses. Unresolved reference
assignment through a descriptor remains an explicit audit item rather than a
claim of complete callable reference parity.

## CI evidence

On `0e17999932a5a509aaebd6cdc690f700b2f3dd9f`, Linux x86_64 codegen shard 8
(job `102606290254`) ran 499 tests: 497 passed and two failed. The remaining
failures were the boxed push property-owner leak (6 blocks, 264 bytes) and
throwing boxed usort property-owner leak (5 blocks, 208 bytes). Their leaked
raw 32-byte blocks are the unretired property cells. The positional usort
lowering rejection from the previous head no longer appears in that shard.

Other PR failures remain independently open, including eval operand ownership,
native/eval Throwable ownership and serialization. This plan does not declare
those problems resolved.

The managed-cell implementation was pushed as
`b9f4475dd41e4298947c2cc190bbd753dd037b1c`. CI run `34399014138` was started
on that exact head; its executable matrix was still pending at the first inspection.
Local verification was limited to builds, test compilation, assembly-comment
alignment and generated-doc audits. No local tests were executed.

### Follow-up evidence on 4e416c3c5 and pending verification

Linux x86_64 shard 8 on `4e416c3c5` ran 500 tests with 499 passing. The boxed
push property-owner leak no longer appeared. Throwing boxed usort instead
terminated after printing `stop|`, on both Linux architectures. Its ownership
gate remains open; the diagnostic now includes generated user assembly.
The eval array-read/strlen temporary leak also stopped failing in shard 7.

That head exposed three additional regressions: Apple assemblers rejected
conditional branches to global reference-cell helpers; rebinding an object
local to its own property did not retire the object; repeated reference
binding inside a loop leaked four cell retains. The corresponding fixes are
`768c9de48`, `745c5ed6e`, and `42f9d3292`. Test compilation and builds pass,
but executable verification is still pending.

`e136734ec` adds native/eval Throwable isolation controls. `b27528dfc` permits
runtime sort callback descriptors without weakening non-callable validation.
CI run `34402858366` is validating the latter exact head. Its predecessor was
cancelled by the next push and is not evidence of a passing executable matrix.

The additional finally-return fix retires a pending reference-return lease when
finally overrides it. At this checkpoint it is committed locally while the
current CI run finishes, to avoid cancelling the diagnostics needed for the
unresolved failures.

### Receiver cleanup isolated from b27528dfc assembly

On `b27528dfc`, both iOS compile/link jobs and macOS managed native packages
passed. Linux x86_64 shard 10 also passed, including the previously failing
self-rebinding case. Shard 7 no longer reports the repeated-reference loop
leak. The throwing comparator factory now passes the checker but fails during
execution, so it is not a completed ownership fix.

The failing usort assembly shows an extra object release immediately after
capturing the property reference from a `HiddenTemp` receiver. That slot still
owns its retained object and is released again by the sort finalizer. The
reference assignment and reference return paths now use the same owning-
temporary check as ordinary property and method reads. A destructor-order
regression and all-target EIR assertions cover the rooted receiver boundary.

Native/eval Throwable round trips still leak 12 blocks (597 bytes). The
reference-return cleanup-throw regression currently prints `holder|` followed
by an uncaught `Exception: cleanup`; its next failure log includes assembly.
Neither exception problem is claimed resolved by the receiver correction.

### Serialization return boundary and 89491f41d evidence

On `89491f41d`, Linux x86_64 codegen shards 8 and 10 passed. Shard 7 now
reports only the magic serializer layout mismatch and the old property COW
assembly expectation; the throwing comparator factory no longer fails there.
The native/eval exception and exceptional reference-return gates remain open.

The object serializer previously read a declared `array` return directly as
an indexed/hash header. Such PHP returns now carry a Mixed cell, so the cell's
tag became the serialized count and its payload words became bogus elements.
The new magic-result boundary validates and borrows either physical array
representation. It keeps the original return owner through recursive encoding,
then retires it under a separate cleanup guard, including nested hook and
destructor exceptions. Completed concat output is restored after cleanup.
Regression fixtures cover both layouts, shared properties, invalid dynamic
returns, nested throws and destructor throws. Their executable results are
pending CI. `__sleep()` still needs the corresponding names-array adaptation.

### Rebase onto the XML main merge

The branch was rebased onto `b068c2b7d27627b9b6ce451dbbc2e71faba3f7d8`,
which includes PR #913. Shared support totals retain XML's 64 contracts,
including its 10 registry lowerings and 54 prelude routes, alongside Core.
The generated documentation was rebuilt from the combined catalog; its
audits report 999 public contracts and 443 non-registry backend routes.

The new XML eval dispatcher referenced the superseded unleased argument
evaluator. Both source-level and positional-hook calls now use the shared
owned-argument boundary, preserving reference writeback and exceptional
cleanup. A no-libxml2 regression covers dynamic string operands in direct
and named XML calls rejected with TypeError. Compilation and documentation
audits pass; executable verification remains delegated to CI.

### Reference-return exception boundary identified

The `89491f41d` failing reference-return assembly contains the callee's owner
lease and cleanup callback, but main has no try handler around the call.
AST `stmt_effect` classified every `RefAssign` as non-throwing and discarded
its source effects, allowing catch pruning before EIR lowering. Reference
assignment now includes the source effects and conservatively accounts for
throwing/global-mutating destruction of the replaced target. Structural
optimizer regressions retain catch/finally for both call and variable sources.
The existing heap-debug callee-cleanup regression remains the executable gate.

### Sleep return layout and lifetime

The sleep path now shares the guarded magic-result ownership boundary. It
validates raw versus boxed arrays, traverses their logical values instead of
assuming packed strings, and retains converted names across nested serializers.
Both the names result and a pending converted name are retired on exceptions.
Invalid return shapes warn and replace the provisional object prefix with null.
Regressions cover visibility-mangled keys, associative shared name arrays,
nested throws, throwing warning handlers and concat-prefix preservation.
Build and test compilation pass without executing local tests. Runtime results
remain a CI gate.

This change does not close the older property-selection gaps: missing,
duplicate and uninitialized names still require filtered counts and PHP
diagnostics, and recursive encoding should consume a selected-property
snapshot. Ancestor-private-name lookup and property references also remain
explicit audit items. No full serialization-parity claim is made.

### Omitted reference defaults after the array ABI change

On `b537d5e0e`, Linux x86_64 shard 4 rejects an omitted `array &$out = []`
before code generation: the Mixed writeback planner tries to narrow an
`Array(Never)` default back into a caller variable that does not exist.
Non-lvalue operands now bypass that writeback plan and use the existing
temporary-cell path. Same-representation defaults acquire an independent
cell owner so replacement and disposal cannot consume the EIR operand owner.
A nonempty-array regression covers repeated positional, named and method
calls. Builds, test compilation and assembly-comment checks pass; executable
results are still pending CI.

### Packed callable array metadata

The generic header-stamping helper omitted `PhpType::Callable`, leaving
packed descriptor arrays tagged as integer arrays. Extracting one through a
boxed PHP array therefore produced an integer and `array_map` rejected it.
The same missing tag prevented existing descriptor-aware COW and deep-free
paths from recognizing those children. Callable arrays now receive tag 10.
All-target emitter assertions and a heap-debug static-descriptor/COW fixture
cover the stamp and the retained late-bound called class. Existing returned
closure and static-callable map fixtures remain CI gates.

### Boxed literal spread boundary

On `9e0a0994a`, Linux ARM64 shard 4 still rejects a declared-array method
spread with `Heap(Array)` versus `Heap(Mixed)` in EIR validation. Literal
unpacking now converts boxed arrays through an explicitly typed, independently
owned hash result. Raw packed-to-hash promotion receives a dedicated owner
because that operation consumes its input. Checker, closure return inference,
and literal storage selection all account for possible associative keys.

Boxed validation happens before later element side effects. Existing scoped
operand-owner records retain prepared items across subsequent evaluation and
retire them on same-frame catches. New fixtures cover packed/hash/empty sources,
closure returns, key order, COW, repeated temporary cleanup and failure order.
The all-target structural fixture covers the typed runtime boundary and emitter
symbols. Local validation comprises `cargo build`, `cargo check --tests`,
assembly-comment checks and the complete generated-builtin-doc workflow only.
No local tests were executed; the executable and structural fixtures await CI.

The same ARM64 shard no longer reports the omitted-reference-default and sleep
serialization failures. Shards 3 and 9 still report callback-array validation,
boxed implode cleanup, unshift/sort validation and list-unpack leaks. These
remain open independently of the spread correction.

### Named default provenance and reflection fixture

The new nonempty reference-default fixture on `9e0a0994a` exposed a separate
checker failure in Linux x86_64 shard 2 (job `102667135178`): named argument
normalization inserted `[10]` and then treated it as an explicit by-reference
argument. Call validation now retains the shared planner's default-slot mask.
Resolved/forward functions and known callables skip caller-lvalue and writeback
requirements only for actual declaration defaults. Methods and constructors
pass original source arguments to that validator instead of losing provenance
through a second normalization. Explicit literals, even equal to a default,
still fail by-reference validation. The matrix includes omitted middle slots,
constructors, methods, static methods, first-class calls and explicit writeback.

The reflection fixture now narrows `getType()` with `instanceof
ReflectionUnionType` before calling `getTypes()`. The checker contract includes
named, union, intersection and null type objects, not just unions. Its expected
class and member counts remain unchanged; a non-union result takes an explicitly
failing output branch. This is a fixture correction, not a relaxation of the
method checker or a production reflection implementation change.

### Scalar metadata after empty-array writes

Linux ARM64 shard 15 on `9e0a0994a` prints float bit patterns as integers
after a foreach builder crosses a declared-array return. The shared word
write helpers specialize empty storage as integer storage, so float, bool
and callable writes now restore their semantic header tag on the returned
post-COW pointer. Indexed assignment receives the same correction.
Regressions cover all five target emitters, growth, empty-source aliases,
boxed returns and retained closure captures.

`cargo build`, `cargo check --tests` and `git diff --check` pass. No local
tests were executed. The assembly-comment checker reports the same 45
multiline-call false positives on HEAD and the edited file; this change
adds no direct assembly instructions. Executable results remain pending CI.

### Internal strlen coercion owner

The list-unpack fixture leaks six strings after three iterations containing
two `strlen` calls on boxed values. `strlen`'s EIR graph creates a string cast,
whose string-tagged runtime path calls `__rt_str_persist`, but did not release
that detached copy. The graph now retires its internal cast after reading the
length, leaving concrete borrowed strings and original boxed arguments alone.
An all-target EIR fixture checks the release dependency; a heap fixture covers
direct, call_user_func and first-class invocations with surviving aliases.

Build and test compilation pass. The builtin documentation regeneration and
all three audits pass without generated changes. No local tests were run;
the list-unpack regression and new ownership fixture remain executable CI gates.

### Mixed spread validation reaches the runtime boundary

On `0071912c1`, Linux x86_64 shard 4 is green. Shard 10 instead rejects the
new dynamic invalid-spread fixtures during checking: `Spread(Mixed)` was
still rejected even though literal storage inference and EIR lowering support
the checked boxed boundary. The checker now admits Mixed operands, retaining
the diagnostic for statically non-array operands. All-target emitter coverage
also includes a Mixed parameter; a new heap fixture checks packed/hash keys
and caller-owner survival. The existing scalar rejection and throw-order
fixtures must now pass through the runtime validation rather than fail early.

`cargo check --tests` and `git diff --check` pass without running tests locally.
CI on `0071912c1` still reports independent failures, including the newly
reachable middle-default constructor leak (five blocks, 224 bytes), generic
array builtin validation, native/eval exceptions and hydration ownership.

### Splice reference regression follows the boxed array contract

Linux x86_64 shard 5 on `0071912c1` fails an old test because a declared-array
reference splice now compiles instead of producing the historical raw-array
promotion diagnostic. The test keeps that int-to-string replacement and now
asserts its runtime output, inserted value type, unchanged COW alias and clean
heap. This does not claim runtime success from compilation alone; the stronger
behavioral assertion remains a CI gate. Test compilation and diff hygiene pass.

A fresh `git fetch origin` confirms that `origin/main` remains
`b068c2b7d27627b9b6ce451dbbc2e71faba3f7d8`, already an ancestor of this branch.
No further rebase or history rewrite is needed at this checkpoint.

### Boxed usort callable body

The runtime descriptor wrapper used the generic one-instruction builtin body,
passing a boxed array directly into the raw usort backend even when the source
call had already selected the private-array graph. Boxed usort wrappers now
reuse full semantic body lowering, including parameter references, private
working storage, normal/exceptional publication and ownership finalization.
The wrapper factory validates the resulting EIR before emitting any assembly.
No separate backend sorter or reduced callable semantics were added.

Shared callback inference also treats union-backed arrays as dynamic values
and keys instead of applying the historical integer fallback. New fixtures
exercise an opaque descriptor parameter, COW aliases, comparator mutation and
throwing cleanup, plus declared arrays with typed string comparators. The
all-target structural test validates the wrapper's handler and concrete work
array in addition to compiling its emitted body.

`cargo build`, `cargo check --tests`, assembly-comment alignment and the full
builtin documentation workflow pass. The generators produce no documentation
diff. No local tests were executed. Runtime and all-target structural results
remain CI gates; CI `34415026947` on the preceding pushed head is still live.

### Implicit call coercion unwind owners

CI on `5970ca6a7` still leaks the boxed argument and its hash/object children
when a declared-array callee throws without joining anything. The backend's
implicit argument boxes had normal-return cleanup only, outside EIR local
ownership. Each tracked coercion slot now includes a preallocated unwind
record, published immediately after conversion and detached in reverse order
before normal release. Direct, static and instance argument staging share the
same layout, including offsets past reference cells and overflow arguments.

The existing isolated throw test now also requires a clean heap. Added fixtures
cover two coerced arrays, surviving aliases, overflow and float arguments,
ordinary returns followed by a throw, and all five target record layouts.
`cargo build`, `cargo check --tests`, assembly-comment alignment and diff hygiene
pass. No local tests were executed; these behavioral assertions await CI.

The preceding CI confirms that the strengthened splice fixture no longer fails.
It does not confirm full recovery: the list-unpack string leak persists, the
new scalar-header fixture exposes separate box leaks, and discarded invalid
spread calls lose their expected exception. These remain open gates alongside
the previously recorded callback, native/eval and hydration failures.

### Discarded dynamic spread effects

Both AST effect inference and exception-flow summaries previously treated a
spread as only its operand expression. A function returning `[...$items]` could
therefore be discarded as pure, and its caller's catch removed, even though
dynamic unpack validation raises Error. Dynamic sources now retain observable
effects and conservative throwable flow; literal arrays keep their recursively
computed child effects. Unit coverage pins that distinction. A runtime-unknown
fixture covers unused literals, discarded function results, catch/finally and
valid arrays with EIR optimization both on and off.

Build, test compilation and diff hygiene pass. No local tests were executed;
the new optimizer fixture and existing invalid-spread regression await CI.

### Deferred string-local consumption

The remaining list-unpack, join-length and join-comparison leaks share another
cleanup gap: a consumer can see a string load before a later assignment widens
its frame slot to Mixed. Codegen then allocates a detached string on that load,
but ordinary call, comparison and retaining-store cleanup treated it as only
borrowed. A cleanup-only predicate now emits provisional releases for these
loads. Existing builder finalization removes them when storage stays concrete.
The stronger ownership-transfer predicate is deliberately unchanged, so this
does not make a borrowed string eligible for moves or consume its local owner.
Callback rooting uses the same predicate instead of a private duplicate.

All-target EIR assertions cover widened loads and borrowed controls. A heap
fixture covers calls, comparisons and assignment aliases with optimization
enabled and disabled. Build, test compilation, generated-document audits and
diff hygiene pass; no local tests were run and generated docs are unchanged.
Executable confirmation remains pending. A fresh fetch still finds main at
`b068c2b7d27627b9b6ce451dbbc2e71faba3f7d8`, already included in this branch.

### Regex callback argument adaptation

The literal-string callback fast path passed the regex runtime's raw match
array straight into a PHP function expecting a boxed declared-array argument.
All string callbacks now use the existing descriptor adapter, as first-class
callables already do. Runtime feature detection retains the invoker even for
literal callback names. Regression fixtures cover literal, propagated-string
and first-class callbacks, plus descriptor emission on all five targets.

Build, test compilation, assembly-comment alignment and diff hygiene pass.
No local tests were executed. The crash regression remains subject to CI.
CI on `7b9199d65` no longer reports the isolated parameter-shadow throw leak
or the discarded invalid-spread failure in their Linux x86_64 shards, but
other array, callback and native/eval ownership failures remain open.

### Merge callable argument packs

The generic descriptor wrapper passed its single variadic pack directly to
the two-operand merge backend. Merge descriptors now lower a small semantic
body that checks the existing AOT two-argument limit before extracting either
operand. Ordinary function-body lowering owns the pack on return and throw.
The canonical variadic signature is unchanged; unsupported argument counts
raise ArgumentCountError instead of reaching a compiler backend assertion.

New fixtures exercise opaque descriptor calls, result/input independence,
zero/one/three arguments, invalid operand types and heap cleanup. All-target
structural coverage pins exactly two boxed backend operands. Build, test
compilation, assembly-comment alignment, the complete generated-document
workflow and diff hygiene pass. No generated files changed and no tests were
executed locally. Runtime correctness and cleanup remain CI gates.

### Type-name result ownership

All gettype paths return a type-name literal from static data, but its default
may-alias result contract suppressed release of owned Mixed read arguments.
The typed runtime descriptor now marks this result independent. The existing
scalar-builder regression has exactly three gettype reads and reports three
leaked Mixed cells; executable confirmation of the corrected contract is
still pending. New tests pin argument releases on all targets and exercise
boxed scalar, string, nested-array and object reads with a clean-heap gate.

Build, test compilation, generated-document audits and diff hygiene pass.
The documentation skill regenerated the internal ownership description and
registry entry. No tests were executed locally.

### Boxed array membership

Declared PHP array parameters and Mixed haystacks now reach a checked in_array
scanner. It borrows stack-shaped cells, traverses the existing logical array
iterator, and delegates value comparisons to the shared strict/loose helpers.
It neither copies the array nor changes its PHP cursor. Invalid dynamic
haystacks raise TypeError before metadata access. The generic descriptor policy
now admits this operation without narrowing either input to integer arrays.

The strict Mixed comparator also now compares floats numerically rather than
by bit identity, preserving signed-zero equality and rejecting NaN equality.
Added heap regressions for scalar and compound values, dynamic strictness,
cursor preservation, opaque/FCC/CUF calls, invalid inputs and float edges.
Structural coverage includes every supported target. The array example now
shows membership through a reusable declared-array parameter.

CI on ab6302f2f confirms clean shard 10 on Linux ARM64 and x86_64. The ARM64
list-unpack leak no longer appears in its completed shard. Array merge callable
and hydration cleanup-throw failures no longer appear in ARM64 shard 15, but
that shard still reports a string leak in returned-array usort coverage.
Callback, by-reference mutation, boxed callable reads, nullable keys and
native/eval hydration ownership remain open gates, not review-ready claims.

Build, test compilation, assembly-comment alignment, all generated-document
audits and diff hygiene pass. The documentation skill updated the membership
callable policy in the registry and internal reference. No local tests were
executed; executable membership behavior and heap cleanup still await CI.

### Nullable integer hash keys

The completed Linux x86_64 shard 14 reports an unsupported TaggedScalar hash
key. Both target materializers now decode the inline tag/payload pair: a real
integer stays an integer key, while null selects the empty string key. This
does not reinterpret the pair as a boxed pointer. Expanded the existing isset
regression to distinguish null from zero and null-valued entries. Additional
coverage exercises read/write/unset, caller alias preservation, heap cleanup
and code emission on all five targets.

Build, test compilation, assembly-comment alignment and diff hygiene pass.
No tests were executed locally. A fresh origin fetch still finds main at
b068c2b7d, already included in the branch. Linux x86_64 shard 15 additionally
confirms that the hydration cleanup-throw failure remains on that platform;
the earlier ARM64 shard result does not establish cross-target recovery.

### Concrete pop and shift result ownership

The remaining returned-array usort fixture empties a concrete string array
with a discarded pop inside its factory. The concrete pop/shift emitters
removed the slot from the live array but then used borrowed boxing, acquiring
another payload reference without retiring the removed owner. Both target
paths now use the existing owned-value boxer. String payloads transfer directly;
refcounted containers, objects and descriptors balance the old owner after
boxing. Empty/null and already-boxed element paths remain unchanged.

New regressions cover discarded and retained string results, COW aliases,
object destructor order and clean heaps. All-target structural tests require
the removed string path to allocate its box without borrowed payload boxing.
Build, test compilation, assembly-comment alignment, generated-document audits
and diff hygiene pass. Generated docs are unchanged. No local tests were run;
the existing usort regression remains an executable CI gate.

### Direct calls through boxed array reads

The checker rejected Mixed variable and expression callees even though the
descriptor backend already dispatches their runtime string, closure, method
array and invokable-object tags. Both checker paths now defer these values to
that existing dispatcher, retaining unknown-signature alias tracking and the
syntactic prohibition on unpacking after named arguments. Statically known
non-callable scalar types are still rejected.

New fixtures cover every callable shape after releasing its source array,
named and spread binding, reference arguments, clean heaps and descriptor
emission on every supported target. The closures example demonstrates an
extracted callback. Build, test compilation and diff hygiene pass; no tests
were executed locally. Invalid dynamic-call diagnostics and ownership during
argument-evaluation throws remain separate audit concerns, not newly claimed
parity. Current CI still reports boxed unshift/sort checker failures.

### Boxed array prepend

Declared PHP array references now reach a boxed array_unshift path. It validates
the receiver before allocating, retains a prefix before separating the outer
cell, and reuses the two-layout merge helper to renumber numeric keys while
preserving string keys. The fresh hash is published before retiring the old
payload. Retaining the prefix first is required when an argument contains the
receiver itself. The concrete scalar fast path is unchanged.

Regressions cover zero values, empty receivers, growth, mixed keys, COW aliases,
self-prepending, nested/object/scalar payloads and clean heaps. All-target
structural coverage pins retain-before-COW-before-merge ordering. Build, test
compilation, assembly-comment alignment, generated-document audits and diff
hygiene pass; generated docs are unchanged and no local tests were run.
CI on 4f4be08d9 now reports only the known hydration cleanup-throw failure in
Linux x86_64 shard 15, with the earlier returned-array usort leak absent.
All six web shards are green, including the previously timed-out macOS job.

### Boxed scalar sorting

Sort and rsort now accept declared PHP array storage. Their boxed path extracts
an independently owned dense Mixed array, separates any retained packed alias,
publishes that owner and uses the existing scalar comparator in the selected
direction. Payload replacement is shared with boxed unshift. The existing
explicit non-scalar restriction remains enforced and documented.

Added list/map key reindexing, caller-alias preservation, by-value array copies,
empty arrays, heap strings, floats, mixed scalar values and unsupported-element
regressions. All-target structural tests pin normalization and guarded comparator
emission. Build, test compilation, assembly-comment alignment, generated-document
audits and diff hygiene pass, without executing local tests. The docs skill
updated both builtin pages and the registry from the shared catalog.

A fresh fetch still finds origin/main at b068c2b7d, already an ancestor. The
completed Linux x86_64 non-codegen shard now exposes additional open gates:
implode monitoring metadata, reference-cell clone alignment analysis, stale
sleep-result owner assertions, malformed standalone-spread optimizer coverage,
array-flip declared-array handling and several ownership/type fixtures. These
need individual inspection; this progress does not establish a green PR.

### Join monitoring boundary

Implode keeps every conservative callback/destructor effect except the two
flags that identify a directly monitored blocking/network operation. Nested
PHP I/O is monitored by its own runtime target; the join is not itself a
network driver. The effect regression still requires global writes, throws,
reference counting and observable execution. This addresses the registry
monitoring gate without suppressing string conversions or inventing I/O events.
Build, test compilation and all generated-document audits pass. The docs skill
updated implode/join effect counts and the registry; no tests ran locally.

### Reference clone return paths

The reference-cell clone now returns immediately after restoring its frame,
leaving its frameless null-return label separate. This preserves runtime behavior
and lets the SysV instruction walker follow both paths without merging their
different recorded stack depths. No alignment allowlist was expanded. Added
all-target structural coverage for both return sites. Build, test compilation,
assembly-comment alignment and diff hygiene pass; executable CI remains required.

### Structural fixture repairs

The sleep-result owner assertion now counts address materializations rather
than symbol mentions: ARM64 adrp/add references each cleanup target twice,
while x86_64 lea references it once. It still requires two distinct release
targets and three guarded invocations. The discarded-spread effect fixture
now parenthesizes its array expression so the statement parser does not select
the destructuring path before the optimizer runs. Its catch/finally and
observable-spread assertions are unchanged. Test compilation and static
hygiene pass; no local test execution was performed.

### Boxed array flip

Declared PHP array inputs now reach a logical runtime flip instead of failing
the checker. The helper retains the source payload before warning callbacks,
normalizes string keys, replaces duplicate values in order and guards its
partial result and source snapshot against throws. Invalid element tags warn
and skip, following the PHP manual. The concrete integer/string paths retain
their existing support boundary. Callback effects prevent discarded calls from
losing their warnings or global mutations.

Added all-target emitter coverage and executable fixtures for mixed keys,
growth, warnings, reentrant handlers, source replacement and throw cleanup.
Former declared-array compile-refusal fixtures now assert successful values,
exact warning counts and clean heaps. Build, test compilation, assembly-comment
checks and the complete builtin-document workflow pass; no tests ran locally.
CI on 558d98274 still reports unrelated checker and ownership failures and a
standalone spread parsing failure. The PR remains draft and unmerged.

### Standalone array expression parsing

Statement dispatch now selects bracket destructuring only when the matching
outer bracket is immediately followed by assignment. Standalone array literals,
spreads and indexed literal expressions reach the expression parser. Parser
regressions retain nested/keyed destructuring and malformed-target rejection;
the optimizer regression again uses the original unparenthesized source, as
does the existing executable CI fixture. Build and test compilation pass,
without local test execution.

### Sleep warning ABI

The serializer's class-qualified warning builder now passes its pointer and
length in rdi/rsi on x86_64, matching the diagnostic dispatcher. ARM64 retains
x1/x2. The previous x86_64 sequence used the wrong argument registers for all
three fragments. Added an exact handler-message regression and all-target
emitter assertions; existing throwing-handler tests remain in place. Build,
test compilation, assembly-comment alignment and diff hygiene pass. Runtime
confirmation is delegated to CI; no local tests ran.

### Splice COW and removed-slot ownership

Boxed splice now separates and publishes its outer cell before converting or
mutating the packed payload. The refcounted runtime transfers removed pointer
slots directly into a preallocated result, instead of retaining owners that
the shortened source no longer contains. Copy loops make no nested calls, so
ARM64 cursor registers remain live, and the result inherits only the source
element tag, not its persistent COW flag.

Added regressions for alias preservation, replacement growth, multiple object
destructors, nested arrays, empty/full removals and all-target lowering/emission.
Build, test compilation, assembly-comment checks, generated-document audits and
diff hygiene pass. No tests were executed locally. CI on the previous head
still reports other boxed-array checker/callback issues, native/eval Throwable
leaks, nullable property reads and unserialize ownership failures; this is not
a review-ready or green-CI claim.

### Nullsafe literal storage

Indexed and associative literals now ask the nullsafe-chain lowerer for its
actual boxed result representation. A nullable property receiver previously
lost its class during a separate syntactic lookup, falling back to integer
array slots and coercing object/null results. The shared chain predicate also
covers ordinary property/index suffixes and non-nullable receivers, whose
chain lowering still returns Mixed. Added executable object/null/scalar and
heap regressions plus all-target EIR slot assertions. Build, test compilation
and diff hygiene pass without local test execution.

The latest fetch adds only main's c91beb343 traffic-statistics commit. Rebase
the clean thematic history onto that main tip before the next protected push.

### Boxed indexed slice snapshots

Read-only boxed indexed slices now acquire an independent payload owner before
the consuming Mixed conversion. They release that private snapshot after the
result retains its selected elements, without rewriting the borrowed source
cell. Both architectures use the same snapshot cleanup sequence. This change
does not extend the existing boxed hash or invalid-input behavior.

Added source-alias, discarded/empty slice, string and object lifetime regressions
plus structural coverage for all five targets. Build, test compilation,
assembly-comment checks and generated-document audits pass. No tests ran
locally. CI on fa8a7ee5c remains red, including eval by-reference array writeback
and previously recorded checker/ownership failures. The PR stays draft.

### Eval native array reference ABI

Magician now stages a declared PHP array reference as an owned boxed cell, not
as a raw array payload. Native array declarations use the packed-or-hash boxed
ABI, so the previous raw slot was interpreted and written back with the wrong
representation. Scalar, string, iterable and concrete object staging is
unchanged. Added a staging-owner unit regression and an executable heap fixture
covering direct/named/dynamic replacements, layout changes, unchanged references
and surviving aliases. The existing CI regression checks the exact missing A/B
values. Compiler build and compiler/Magician test compilation pass; execution
remains delegated to CI.

### Descriptor temporary reference owners

Descriptor invokers now allocate temporary/default reference arguments as managed
typed cells and register their cell lease in the existing normal/exception cleanup
ledger. Coercions retain the resulting representation, not the original source
type, and string staging owns a persisted buffer. Newly boxed hash sources are
rooted through conversion and retired after cell publication. Borrowed reference
markers remain caller-owned.

Closures capturing a managed reference cell acquire their own lease, and descriptor
retirement releases only validated managed cell addresses through guarded cleanup.
Legacy raw or borrowed capture addresses keep their previous behavior. This is
necessary so retiring a temporary invocation cell cannot invalidate an escaping
capture. Added executable regressions for layout replacement, named/default calls,
native throws and escaping captures, plus all-target structural coverage.

Build, test compilation, assembly-comment checks and builtin-document regeneration
and audits pass. No local tests ran. CI on 83cb548a1 is still running; this does
not close the other known callable, eval Throwable, checker and unserialize issues.

### Handler preparation owners

Core handler registration now releases the temporary normalized descriptor and any
new display box after the active registration retains them. Already-boxed callbacks
remain borrowed instead of creating a fake MixedBox owner. Normalization precedes
boxing so rejected callbacks cannot strand a new display box.

Added heap regressions for temporary captured handlers, restoration and caller-owned
boxed callbacks, plus structural checks across all supported targets. Build, test
compilation, generated builtin docs and audits pass without local test execution.
The latest CI remains in progress; captured local cells require a separate ownership
fix and the other recorded failures are still open.

### Captured local reference cells

Closure lowering now promotes a captured local through an explicit EIR owner slot
before taking its address. Promoted local cells use the managed typed allocator,
so descriptor capture leases and local/alias retirement share one cell lifetime.
Existing borrowed incoming references remain borrowed. Regression coverage includes
escaping closures, multiple descriptors sharing mutable state, captured objects,
repeated handler registrations, and structural owner checks on every target.

Build, test compilation and assembly-comment checks pass, with no local execution.
CI on 9b85ed40a confirms the handler/legacy-cell leak still present on that older
head, as well as boxed callback checker failures, array_sum rejection and usort
exception cleanup leaks. These results are not a green-CI or review-ready claim.

### Handler restoration after destructor throws

Handler release now guards each detached callback, descriptor and eval-context owner.
A nested guard in pop lets the preceding registration and linked node be restored
and retired before a pending destructor exception propagates. Added executable
coverage for both error and exception handlers and all-target emission assertions.
Build, test compilation, assembly-comment checks and diff hygiene pass; no local
tests were executed.

CI on 9b85ed40a passed the Linux x86_64 eval shard containing the native heap-reference
regression. The named middle-default leak remains on both Linux architectures:
its raw cell comes from constructor MayOutliveCall staging, not the descriptor
invoker. Constructor-promoted reference properties still borrow those fallback
cells, so deleting the heap fallback or using caller-stack storage is not a safe fix.

### Descriptor callable argument leases

Invoker argument staging now records raw Callable owners, which are intentionally
not classified as ordinary refcounted PHP types. A per-slot release discipline
uses descriptor cleanup for those values on normal and exceptional exits. Managed
reference cells still use the heap-kind dispatcher, even when their payload is
callable, and the boxed return slot keeps its existing Mixed ownership contract.

Added positional/named native call regressions with captured objects, a throwing
consumer and caller reuse, plus all-target release-entry assertions. Build and
test compilation pass; executable checks remain delegated to CI. This addresses
one confirmed source of the opaque usort leak without claiming its entire failure
or the remaining array/checker issues are resolved.

### Loop-carried declared array contracts

The loop-storage lattice now preserves the logical PHP array contract when a
concrete entry array is reassigned from a declared array result. Its physical
representation remains boxed, without degrading the checker environment to an
unrestricted Mixed value. Scalar, nullable and unknown evidence still removes
the array proof. Added symmetric positive/negative join tests and an all-target
lowering regression matching the existing mixed-key-write executable fixture.

Build, test compilation and diff hygiene pass; no tests ran locally. CI on
22ef9aecb confirms codegen shard 8 passes on both Linux architectures after the
handler and local-capture fixes. Other shards still fail, including typed array
callbacks, array_sum, splice return typing and ownership cleanup regressions.

### Declared array splice results

The array_splice checker now preserves the declared PHP array result contract
instead of degrading it to unrestricted Mixed. The backend already boxes the
removed-elements array for this representation, so no runtime acceptance or
storage changes are needed. Added all-target lowering coverage and negative
checks for scalar receivers and arbitrary Mixed returns. Existing heap fixtures
cover empty/full removal, nested values, replacement growth and aliases.

Build, test compilation, generated builtin documentation and contract audits
pass. No local tests ran. This typing correction does not expand the runtime's
existing indexed-only splice implementation to boxed associative arrays.

### Named reference places

Named source arguments now use the shared plan's parameter mapping to apply the
same reference-place lowering as positional arguments. Element addresses and
required container widening survive source-order evaluation, parameter reordering
and named arguments after a spread prefix. Ordinary value arguments keep their
previous conversion path. Added heap coverage for direct, instance, static and
spread calls with COW snapshots and observable source-order markers, plus
all-target EIR address assertions.

Build, test compilation and diff hygiene pass without local execution. CI on
22ef9aecb now confirms codegen shard 8 and eval shard 15 on all three executable
targets. The current branch already contains origin/main; no rebase is required.
The remaining failed shards are still merge blockers.

### Iterator callback argument containers

iterator_apply now accepts the proven PHP array contract for its callback
argument container. The existing descriptor backend already accepts its boxed
representation. Since either indexed or named keys may arrive, checker callback
specialization conservatively permits associative arguments. Extended the
Traversable/iterable regression with named containers and added scalar/Mixed
rejection controls. Build, test compilation and diff hygiene pass without local
execution; CI validation remains pending.

### Unserialize temporary-list depth comparison

The x86_64 detachment helper loaded the current parser depth through the ABI
tertiary scratch register (rcx), then compared node depths against an unrelated
hardcoded r11. It could leave the completed parser's hydration nodes published
and skip their releases. The comparison now uses the selected registers on both
architectures. The emitter regression checks the exact load/compare pair on all
targets, alongside existing executable nested-context and destructor-throw tests.

Build, test compilation, assembly-comment alignment and diff hygiene pass, with
no local execution. This does not fix the separate missing array back-reference
registry entries in the decoder.

### Checker assertions for declared array storage

Updated four stale diagnostic/signature tests that still equated a PHP array
declaration with indexed Array(Mixed), or expected its ABI to specialize to one
string/object element layout. The assertions now require the exact packed-or-hash
union and add an associative call site to the parameter test. Mixed returns and
false-bearing array unions remain rejected; these are expectation updates, not
relaxed negative tests. Test compilation and diff hygiene pass without execution.
Incorrect executable outputs and ownership failures remain separate open issues.

### Sparse property-array expectations

Reclassified two previously listed CLI output failures after inspecting their
fixtures: PHP counts present keys, not the highest index plus one. The manual's
count example uses keys 0, 5 and 10 and returns 3:
https://www.php.net/manual/en/function.count.php
Updated instance/static/late-bound property expectations to sparse counts and
added explicit missing-key and present-null checks. The property initializer
structural test now requires the declared boxed array contract, while retaining
the check that each stored default has the property's actual representation.

Test compilation and diff hygiene pass; no test or PHP fixture ran locally.
These expectation updates do not resolve unrelated runtime ownership failures.

### Ownership assertions after local rebinding

Updated three structural expectations superseded by the current lowering design.
Straight-line incompatible reassignments retire and clear the old binding, then
allocate a new typed slot; they no longer force earlier object/string reads to
use widened Mixed storage. The tests now pin concrete old slots, publication
retains and explicit retirement. Declared array property append instead borrows
the cell already separated and published by PropGetForWrite, so its test checks
that address is passed to MixedArrayAppend and is never released as an owned
temporary. Test compilation and diff hygiene pass without local execution.

### Declared array column extraction

array_column now accepts proven PHP array arguments and selects a target-aware
boxed path instead of assuming raw indexed rows. The helper uses the logical
iterator, normalizes numeric-string keys, distinguishes missing keys from present
nulls, and adopts owned row reads into an independent Mixed-valued indexed result.
No user callback or warning runs while source rows are borrowed. The existing
concrete-row helpers and the two-argument string-column scope are unchanged.

Added all-target lowering/emitter coverage and heap regressions for packed/keyed
rows, missing and null columns, string/nested/object lifetimes, COW and growth.
Extended the nested-array example and kept negative scalar/Mixed receiver checks.
Build, test compilation, assembly comments, generated builtin docs and contract
audits pass. No tests ran locally. CI on 541bd9814 still reports typed callback
array rejection and the six-block opaque usort cleanup leak, which remain open.

### User-call argument unwind roots

Direct user calls with independent results now register owned by-value arguments
in the existing scoped cleanup chain before entering the callee. A same-frame
catch cannot bypass their retirement. Reverse publication preserves the existing
first-to-last normal argument cleanup order while leaving later owners visible
if an earlier destructor throws. Reference arguments, by-reference returns and
possible argument-result transfers retain their previous paths.

Added all-target EIR coverage and a heap regression for temporary versus borrowed
callbacks, including a passthrough-return control. The existing opaque usort
fixture exercises a throwing comparator passed through a user-function boundary.
This change protects callee execution and ABI materialization; it does not claim
to fix temporaries abandoned while evaluating a later source argument. Build,
test compilation and diff hygiene pass, without local test execution.

### Callable argument ABI normalization

Descriptor invokers now normalize boxed callback arguments before invoking an
entry whose parameter uses the raw Callable ABI. The previous coercion whitelist
left Mixed cells unchanged, so positional calls could dereference a box as a
descriptor and named calls could record the wrong cleanup representation.

One demand-driven helper reuses the existing closed-world callable selectors for
descriptors, names, packed method pairs and invokable native objects. It returns
one descriptor owner, stores its borrowed input in a stable frame slot and raises
TypeError for invalid shapes. Both AOT and eval-generated invokers request the
same helper without making AOT-only programs depend on Magician. Owned coercion
inputs and partially populated indexed variadic arrays are now unwind-visible.

Added all-target emitter/lowering coverage, heap tests for named and positional
callback shapes and invalid argument cleanup, and a callback example. Build,
test compilation and assembly-comment checks pass without local test execution.
The existing selector's keyed-hash callback-pair limitation and associative
typed-variadic normalization remain follow-up work, not claims of full parity.
CI on bbaffc6eb remains red in multiple shards. Its x86_64 shard 3 now reports
only the declared-array callback checker failure, versus the additional opaque
usort leak on the previous head; the full matrix and this new fix still need CI.

### Boxed column result heap identity

CI on bbaffc6eb caught a separate x86_64 emitter defect in the new column path:
the Mixed-element stamp kept only the low kind/COW bits and erased the high heap
identity marker. This made later declared-array boxing classify the result as
null and made guarded decrefs ignore otherwise valid result containers. Preserve
the high marker exactly as the existing indexed-array emitters do. The three
new column heap fixtures already catch null results, nested COW and delayed
object release; the all-target emitter test now also pins the preservation mask.
No local tests were executed.

### Class-name argument independence

The remaining ARM64 column object-owner failure is an introspection lifetime
issue, not column storage: get_class on an owned Mixed read conservatively kept
that read and its object alive after returning a name from static metadata.
GetClass and GetParentClass now declare Independent result aliasing. The direct
get_class_methods object-name projection uses that same no-argument-alias proof.
Ordinary, Closure and incomplete-class name emitters all select independent
metadata; they do not need fresh string allocations just to retire the input.

Added all-target EIR release assertions and heap fixtures for boxed reads,
retained names, throwing temporary destructors and special class names. The eval
class-name bridge's separately allocated result-cell cleanup remains a distinct
open issue. This change does not claim to resolve that bridge allocation.
Build, test compilation, generated builtin documentation and all contract/site
audits pass. No tests were executed locally.

### Nullable slice and splice lengths

The CI slice fixture exposed an unsupported TaggedScalar operand when a helper
passes its nullable integer length to array_slice. Both slice and splice now
convert that two-word value through the shared nullable integer conversion and
derive the separate length-presence flag from its tag. Null means open-ended;
zero, negative lengths and integers matching the null payload sentinel remain
real lengths. No boxed cell or source mutation is needed for this conversion.

Added all-target lowering coverage and a heap regression for null, zero,
negative, positive and sentinel-colliding lengths through slice and splice.
Extended the array example with a nullable page limit. Test compilation and
assembly-comment checks pass. Runtime verification remains assigned to CI;
no tests were executed locally.

### Runtime callback fixture reachability

CI on c6136456f rejects the invalid callback cleanup fixture during checking:
its declared Callable target acquired a known signature from the caller, so
the Mixed callback never reached descriptor invocation. Keep the target Mixed
in both callback-shape fixtures and the matching all-target emitter gate. This
exercises runtime validation without changing static argument compatibility or
accepting an invalid callback into the native callee. The emitter gate also
asserts the target retains its Mixed ABI. Local validation compiles the tests
only; runtime results still require CI.

### Reference property assembly expectation

The issue-642 reference-slot assertion still expected a raw indexed array, while
the declared PHP array now owns a Mixed box through its managed reference cell.
The emitted assembly attached to the failing CI assertion confirms the correct
sequence: dereference the cell, clone the box, release its old owner and publish
back through the cell. Update the ordered assertions for both architectures and
explicitly forbid passing the box to the raw indexed-array split helper. Keep
the existing prohibition on overwriting the property reference-cell pointer.
This is a test-expectation update, not a runtime ownership fix. The tests were
compiled but not executed locally.

### Unset exception leak isolation

CI on c6136456f still reports one live string after declared-array unset throws
from an element destructor. The hash helper already frees the key and commits
removal before releasing the value, so changing that ordering is not justified.
Add independent literal-key, computed-key and getMessage-result controls, and
attach the original fixture's user assembly to its heap failure. These controls
distinguish the temporary key from the getter's fresh string without suppressing
the heap assertion. The leak remains open pending CI evidence; no local tests
were executed.

### Callee-owned arguments with aliasing results

The merge descriptor fixture reaches its arity and type-error branches through
a user function returning Mixed. Its unknown return-alias summary suppressed
caller-side unwind roots even for the declared array parameter, whose callee
shadow has an independent owner. Use that existing signature ownership proof
per argument, not only the whole call's result-alias summary. Raw passthrough
callables and by-reference returns retain their previous transfer rules.

Normal cleanup now visits rooted and unrooted arguments in their original
parameter order. It does not filter and renumber the unrooted operands, which
would apply the wrong alias summary when a later callable is returned. Added
all-target EIR assertions, heap regressions for array/Mixed shadow returns and
throws, and a later-callable passthrough control. The existing boxed merge
descriptor fixture remains the end-to-end arity/error gate. No tests were
executed locally; executable verification remains assigned to CI.

### Eval class-name result owners

The eval class-name ABI returns an owned Mixed cell. Native string lowering
previously borrowed its payload indefinitely without retiring the cell. Detach
the string before releasing that cell, and allow validated string release for
class-name lookups: the native path returns static metadata, while the eval
path can return an owned copy. Neither path returns concat scratch. Boxed EIR
results now keep the bridge cell instead of incorrectly boxing its pointer as
a boolean; the non-object fallback boxes its existing empty-string result.

Added an all-target lowering gate and repeated heap checks for immediate reads
and names retained after the inspected object is unset. Extended the Core
introspection example with a post-eval class-name read. Test compilation, the
builtin documentation workflow and assembly-comment checks pass; no executable
tests were run locally. CI must confirm the runtime ownership behavior.

### Opaque callback example boundary

Keep the callback example's descriptor target Mixed, matching the already
corrected regression fixtures. A Callable declaration lets interprocedural
checking specialize the target and reject the Mixed callback before descriptor
normalization. This example is intended to demonstrate the runtime descriptor
boundary, not relax static callback argument checking. No tests were executed.

### Boxed flip warning-replacement fixture

CI rejects the reentrant flip fixture before reaching the runtime: its caller
source was inferred as a concrete associative array, then rebound to a packed
array inside the handler. Obtain the source from a declared PHP-array return so
the captured cell genuinely supports both layouts. Keep the callback replacement,
recursive flip, original snapshot assertions and clean-heap assertion intact.
This repairs the boxed-runtime fixture without changing concrete-array assignment
rules or claiming that the runtime heap checks have passed locally.

### Unset computed-key widening fix

The user assembly from CI job 102742016886 locates the one-string leak. The
computed key was stored in an OwnedTemp later assigned null, which widened the
slot to Mixed. Loading that slot as Str called mixed_cast_string and created a
second owner that was not rooted across hash_unset. In the same assembly,
getMessage's persisted string is explicitly freed after echo, ruling out the
earlier getter hypothesis for this fixture.

Use the shared scoped operand root for the key. It preserves the concrete
operand type, is visible to same-frame catches, and retires through a slot
release rather than a null assignment. Added an all-target root/type/order gate
and a repeated same-frame-catch heap fixture alongside the original external
catch and isolation regressions. Tests were compiled, not executed locally.

Remaining CI evidence on 25e023f67 includes declared-array aggregate/callback
rejections, boxed-map static-descriptor owners, eval exception transfers and
eval parameter-shadow owners. Job 102742016924 also reports the escaping
descriptor default reference capture returning 1:1 instead of 2:3, with six
live blocks. None of these distinct failures is claimed fixed by the key root.

### Escaping reference-capture isolation

Attach user assembly to the failing descriptor-default capture fixture and add
an explicit direct-reference control. Both require the closure's writes to use
one mutable array cell, with unchanged value and heap assertions. These separate
default-cell staging from reference capture and mutation after return. This is
diagnostic coverage for the still-open 1:1 versus 2:3 regression, not its fix.

### Static callable argument unwind roots

The static callable resolver bypassed the scoped argument roots used by ordinary
user calls. Its CUF, FCC and direct closure paths only released temporary
arguments after the call returned, leaving those owners behind on exceptions.
Apply the shared user-call rooting and retirement helpers to visible arguments,
keeping hidden captures outside the argument ownership ledger. User-function
targets consume the same return-alias summary as direct calls; closure targets
remain conservative and preserve reference-return exclusions.

Added all-target EIR ordering coverage and repeated CUF/FCC/closure heap checks
requiring temporary object arrays to retire before a same-frame catch. Local
verification is build and test compilation only, not executable test runs.

### Managed native reference defaults

CI job 102746649421 exposes the actual failing path: the FCC factory specializes
to a native call, whose omitted reference default lives in a temporary stack
cell. The returned closure captures that address, but the caller discards it.
This explains the observed 1:1 result instead of 2:3 without changing closure
mutation or count semantics.

Native default arguments now allocate typed managed reference cells. A scoped
owner record retires the caller lease on return or throw; escaping captures
retain independent leases. Fresh EIR array/hash defaults also receive unwind
roots so their original owners do not leak when the native callee throws.
Existing caller locals and element addresses remain reference places. The
constructor-promoted borrowed-property fallback remains separate and is not
claimed fixed here.

Added all-target assembly/EIR coverage, interleaved escaping captures with two
defaults, and repeated same-frame exception cleanup with payload destructors.
Updated the old stack-only documentation. Build, test compilation and assembly
comment checks are the local validation; no tests were executed locally.

### Remaining eval-owner diagnostics

The previous CI head still reports six string owners in the eval class-name
fixture, and one Mixed cell plus one string in the array-parameter shadow
fixture. Include only user assembly in their failure diagnostics so the next
remote run identifies the remaining allocation and retirement paths. Preserve
all existing output and clean-heap assertions; this is instrumentation, not a
claim that either remaining leak is fixed.

### Completed-array unserialize references

The recursive decoder reserved reference indices for arrays but left their
registry slots empty. A later R reference therefore returned a failed value,
including the existing hydration-data regression that printed kept| instead
of kept|kept. Publish completed array boxes with a parser-context lease, using
the existing depth-aware deferred-owner list so nested decoders and discarded
or replaced hydration data cannot invalidate the registry pointer.

Replace the hand-copied reference box with the shared Mixed clone helper, which
retains array payloads as well as objects. Keep lowercase object-identity
references restricted to objects. This change covers later reads of completed
arrays, not reconstruction of recursive arrays or full writable-reference parity.

Added all-target publication/ownership gates and heap fixtures for a replaced
duplicate key and an invalid object-identity target. The original hydration
regression remains unchanged. Build and test compilation pass; executable
validation remains delegated to CI, with no local test execution.
The builtin-doc generation and audits pass without generated-file changes;
assembly-comment alignment and git diff hygiene also pass.
