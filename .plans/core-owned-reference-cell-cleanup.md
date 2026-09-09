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
