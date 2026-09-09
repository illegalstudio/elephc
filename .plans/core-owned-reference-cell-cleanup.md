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
