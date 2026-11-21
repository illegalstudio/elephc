# PHP-array ABI correction required by scheduler review

Status: active implementation work; Candidate 61 is rejected. This is required to preserve the
accepted variadic Async/Parallel surface and existing positional PHP behavior before the next
absolute-consensus review. It does not add a public scheduler or I/O-provider API.

## Checklist

- [x] Reproduce the Candidate 61 Array/Hash transport failure and downstream regressions.
- [x] Audit existing signature specialization and rule out treating include/declaration variants
      as independently typed array-shape specializations.
- [ ] Define one internal PHP-array storage contract for shape-erased arguments and returns,
      independent of the declared PHP element type.
- [ ] Preserve known indexed-array fast paths and every previously supported positional consumer.
- [ ] Carry numeric and named keys across user functions, methods, closures, constructors,
      descriptor calls, forwarding, and declared `array` returns.
- [ ] Integrate shared array operations with runtime shape dispatch, key-preservation rules,
      mutation/COW, result typing, and balanced ownership on exceptions and normal exits.
- [ ] Preserve reference-cell semantics for positional and named by-reference variadics.
- [ ] Validate declared callback parameters before coercion using shared PHP binding policy,
      including partial scalar conversions and caller strictness.
- [ ] Add meaningful regressions for the complete affected consumer families, both optimization
      modes where relevant, and AArch64/x86_64 runtime paths.
- [ ] Pass current-source Linux runtime and iOS acceptance, prelude parity, builtin and assembly
      audits, and independent exact-artifact Codex/Kimi/GLM/DeepSeek reviews.

## Verified Candidate 61 counterexamples

The reviewed commit is `45b1dc6d145b6e3303d470cf9178355715cb70df`, tree
`29e53de7fa35aff375692de2599cec6ddbd7bbff`. A freshly built compiler from that source produced:

| Program | PHP oracle | Candidate 61 |
|---|---|---|
| `collect(mixed ...$xs): array { return $xs; }`, called with `foo: 42` | `{"foo":42}` | backend refuses Hash to Array return transport |
| `product(int ...$xs): int { return array_product($xs); }`, called with `2, 3` | `6` | backend refuses Hash input |
| `sum(float ...$xs) { return array_sum($xs); }`, called with `1.5, 2.5` | `4` | `2.5` (wrong helper and result ABI) |
| `array_sum(['a' => 2, 'b' => 3])` | `5` | `0` |
| callable relay passing `'abc'` to `f(int ...$xs)` | caught `TypeError` | `0`, call accepted |
| `f(&...$xs) { $xs['foo'] = 9; }`, called with `foo: $a` | caller `$a == 9` | caller `$a == 1`; baseline code already skips reference-shape specialization |

The runnable reproducers are local `scratchpad/c61_variadic_*.php` files. They are evidence inputs,
not publication source. The direct constant invalid callback call is rejected by the checker;
the runtime validation regression requires the callable/Mixed relay to reach descriptor binding.

## Storage and binding requirements

PHP `array` does not promise an indexed physical layout. Numeric-list and associative storage
must be represented through one semantic contract at an unknown-shape boundary. Declared element
binding, ownership, reference-cell payload, runtime shape, and PHP-visible key semantics are separate
properties. Casting selected Hash collectors to lists merely to call an indexed helper is insufficient
where operations preserve or inspect keys.

The existing checker specializes one stored `FunctionSig` and one source body in place.
`function_variant_groups` are include/declaration alternatives and require matching signatures;
they are not a monomorphization mechanism. True separate indexed/associative bodies would require
separate owners for checker metadata, return inference, loop information and callable descriptors.
That mechanism alone still would not solve dynamic declared-array return transport.

The implementation must retain existing indexed paths when shape is known, and introduce common
array primitives for shape-erased boundaries: ordered iteration, lookup/update, key retention or
renumbering, mutation/COW, numeric aggregation and result transport. Reuse the runtime's actual
Array/Hash discriminants and ownership helpers; do not smuggle a Hash pointer into an Array slot.
No seven-argument cap, discarded named keys, unconditional narrowing of return values, or eval
fallback can substitute for the accepted native contract.

## Audited consumer inventory

The migration must account for aggregates (`array_sum/product`), push/pop/shift/unshift and shuffle,
filter/reduce/walk, merge/diff/intersect and user-callback set operations, slice/splice/reverse,
chunk/pad, fill-keys/combine, random/count-values/column, sort/rsort/natural and user sorts,
find/any/all/multisort, and `fputcsv`. The Candidate 61 backend audit found indexed-only gates in
these families; the existing Hash paths for map/flip/values/keys/search/min/max also require
checker/result-shape and ownership verification. A single successful aggregate is not closure.

## Next review gate

Candidate 61 remains useful immutable failure evidence. No acceptance transfers to the live fixes.
Complete this correction together with the scheduler authority/failure/detection fixes, reconcile the
specification and docs, freeze a new exact artifact, and restart all independent reviewers. The
maintainer has already authorized bug corrections and necessary additional work toward absolute
consensus; publication occurs only once every current-artifact requirement is proved.

## Concrete internal representation direction

Use the existing raw-pointer dynamic Array/Hash representation, with an explicit backend-only
PHP-array storage marker retaining its element contract. A candidate internal type is
`PhpType::PhpArray { element: Box<PhpType>, references: bool }`, with the same pointer ABI as
`Iterable`, but an array-only semantic guarantee. No parser syntax, reflected parameter type or
public `iterable` declaration changes. The exact name/field placement is an implementation choice;
the separation between PHP declaration and physical storage is mandatory.

Current reusable infrastructure is source-confirmed:

- Array/Hash to raw Iterable is pointer forwarding (`array_access_runtime.rs:52-59`).
- Generic raw-pointer release uses `__rt_decref_any` (`abi/values.rs:115`).
- Raw pointer boxing selects Mixed Array/Hash tags by heap kind (`value_boxing.rs:269`).
- Foreach already dispatches kinds 2/3/4 and reads indexed element metadata, so kinds 2/3 supply
  the ordered Array/Hash views needed here (`iterators.rs:355,604,630,933`).
- JSON already has raw-shape dispatch (`builtins/json.rs:352`).

The existing general Iterable also accepts Traversable objects, so it cannot stand in for the
PHP-array semantic contract without the array-only marker/validation. Dynamic unboxing must reject
non-array payloads. Existing `print_r(Iterable)`, serialize, dimension writes and indexed-only
builtin gates must acquire explicit array-storage dispatch before the marker is enabled broadly.

At a known-shape call site, retain the existing indexed or Hash value and element layout. At an
unknown-shape user-call boundary, pass its pointer without converting every list into a Hash or
boxing every element. Callee locals and array returns carry the backend-only marker; shared
operations dispatch on actual heap kind and runtime element representation, then apply the PHP
operation's key policy. Original PHP signatures remain authoritative for parameter binding,
reflection, diagnostics and declared `array` validity. Reference captures retain reference cells,
not copied values; COW splits containers while aliases continue to address their original storage.

Implementation order: transport and declared-array guards first; dimensions/iteration/COW/reference
primitives second; reductions, transformations, multi-array operations and mutating/order consumers
next; JSON/serialization/debug/CSV consumers and complete ownership/target regression closure last.
Every currently supported positional consumer must pass before a new review candidate is frozen.

## Callable argument containers and ownership

Raw PHP source arguments and already-planned callee ABI arguments are distinct contracts. A direct
callee receives matched regular slots and one packed variadic capture. A descriptor invoker instead
receives the ordered source argument container and performs that binding/packing itself. Current
native validation exposed a double-pack path: a dynamic `callable` parameter with known variadic
signature was lowered as a direct ABI call, then the ClosureCall descriptor fallback packaged its
Hash collector as one raw argument. Valid scalar/Stringable values therefore arrived as an array.
The lowering must choose the proper boundary before argument packing, sharing named/spread planning
and source-order rules without packaging the same tail twice.

Descriptor binding validates and converts each physical parameter in invocation order; a global
all-parameter preflight must not skip earlier observable Stringable conversions. Source argument
evaluation order remains a separate frontend obligation. A valid Stringable conversion can throw
during marshalling; pending owned fixed
arguments and partial collectors require balanced cleanup across the borrowed callee ABI, through
normal return or unwind. EIR-prebuilt source boxes have explicit normal-path release instructions;
the backend must not also release the same owner on success. Typed Object/Packed
arguments on the existing descriptor routes are deliberately borrowed, so cleanup may not invent
an owner merely from the type. Each conversion path must state whether it retained/allocated or
borrowed its output. Caller strictness and PHP internal weak callback policy remain explicit and
separate from these transport/ownership facts.

## Native helper feasibility and primitive gates

Compiler-owned AST helpers may implement generic algorithms through the ordinary native AST/EIR
pipeline, while known indexed inputs retain current fast paths. This is an implementation technique,
not permission to replace the complete array contract with a subset of working helpers.

An older-binary probe with deliberately stable Mixed loop locals reproduced PHP results for numeric
sum/product and string-key filter/reversal. Its first version without stable locals instead exposed
loop-carried storage mismatches; helper-local storage must be explicit and checked. The same probe
still produced null gaps for sparse numeric filtering and mutated a caller's by-value boxed cell.
Both require fresh-source validation before an architecture is selected.

Required shared primitives include sparse-key-preserving insertion (promoting dense storage rather
than inventing null keys), generic deletion, independent by-value cells with payload COW, and
reference-aware access/mutation. An Iterable pointer is not a Mixed cell and must not be passed to
boxed dimension helpers without the proper typed view. Numeric reduction helpers must also model
PHP conversion/warning behavior; simply compiling a `$sum += $value` loop does not prove equivalence.
Native helper migration must cover the complete inventory above, with result shapes and heap cleanup
proved independently of ordinary scalar output.

## Shared Mixed value-copy prerequisite

The pinned checkpoint compiler reproduces incorrect source mutation across local assignment,
by-value parameters, identity returns, forwarding, methods, closures and exceptions. The latter
forwarding/method/closure probes additionally fail heap refcount checks. Runtime-string descriptor
dispatch returns null instead of the changed array. Isolated evidence and oracles are retained in
`scratchpad/c61_mixed_value_copy_evidence.md`; self-contained regressions are owned by
`tests/codegen/mixed_value_copies.rs` and must execute after the source correction.

Do not use existing owned-local flags as complete mutation/escape analysis: they miss dimension and
indirect mutation. Semantic copies must separate mutable value cells without breaking reference
bindings; object identity and shared resource state remain identities, not deep copies. Incoming
by-value parameters, ordinary semantic assignments and returned values must agree on the rule.
Compiler-internal spills need not pretend to be PHP assignments. Actual owned return shadows must
be reflected in direct-call cleanup and descriptor return ownership; retained Resource clones can
share an address while owning an independent lease.

Separate baseline probes expose a dynamic object-property discrepancy/leak and a resource that
remains writable after explicit close through another alias. These are not attributed to the new
cell-copy change without source evidence, but remain explicit unresolved findings. Passing guarded
declared-object and shared-stream-position controls does not prove dynamic properties or explicit
close semantics. Reconcile their affected-source scope and outcome before claiming the complete
Mixed copy/identity contract or final artifact acceptance.

## Main-integration backtrace and return-boundary probes (2026-10-10)

The named backtrace reader now has separate entry-time variadic capture and preserves names
without contaminating positional func_get_args. Its initial, typed-string and named reference
escape/aliasing controls pass with clean heap teardown on macOS; supported-target breadth and
other value kinds remain required gates. Private invoker marker tag 11 is not a PHP persistent reference;
resolve its source storage before copying a trace value. The managed temporary cell also needs
its own retained collector payload even when Hash storage is passed as raw Array(Mixed).
Evidence and retained debugger artifacts are in execution-scheduler-main-integration.md.

An additional valid PHP boundary remains rejected: a function declared to return array cannot
currently return the opaque Mixed result of $frame["args"]. The supplementary namedValues and
namedReferences controls use a mixed return to isolate their native reader/ownership behavior.
Changing those annotations back to array reproduces "return type expects array, got mixed".
Runtime declared-array return validation/unboxing must be covered by the full transport/return
campaign; the mixed-return controls do not close that acceptance requirement.

## Opaque declared-array return gate, 2026-10-11 (unpublished)

The published c933b7c714 compiler still rejects the PHP-valid mixed frame return;
PHP 8.5.10 produces {"label":42}. The bare declaration already resolves to the
exact boxed packed-or-keyed `PhpType::php_array()` union in checker AND EIR AST
signature reconstruction. Do not introduce another return-storage map or treat
`Array(Mixed)` generic-extension storage as the same contract. Initial attempted
metadata threading was removed after discovering this existing canonical path.

The checker now permits an opaque Mixed value only for that exact PHP-array
contract. A target-neutral AST/EIR return guard uses existing is_array and TypeError
construction before returning the original shape, borrowing existing string-return
source leases/debug-type helpers. It does not cast scalars/objects, discard keys,
convert hashes to vectors or weaken concrete element-storage checks.

Initial native replay: scalar rejection and heap cleanup pass; positive function/
method/closure forwarding exposes bad refcount. The standalone reducer instead
returns null with a clean heap. EIR reveals the accepted branch's LoadLocal is
followed by UnsetLocal before MixedBox consumes it; that load may be a deferred
slot view. Return materialization now precedes source-slot clearing. New strict
PHP/heap positive/list/hash/method/closure/descriptor and negative tests plus a
five-target emitter gate are under replay. No guard success is asserted yet.
Logs are `/tmp/elephc-array-return-*20261011.log`; current owned session follows
the owner-order native replay then emitter check. By-reference/nullable/union
return semantics and complete consumer/I/O/review gates remain required, not
closed by this slice.

The exact-head CI on c933b7c714 also exposes callback/runtime failures beyond the
fixed catalog count: Linux x86_64 curl shard1 is **36/53**, with callback/eval
SIGSEGVs, empty callback output and one heap exhaustion. PDO x86 shard2 is
**60/61** (20 ignored), failing the invokable-object FETCH_FUNC callback with
SIGSEGV. Raw completed-job logs are retained under
`/tmp/elephc-pr958-{curl-x86-shard1,pdo-x86-shard2}-20261011.log`. These are separate
pending integration gates; do not call published CI green or attribute them to
this unpublished array-return guard without evidence.

The source-holder ordering adjustment alone did not fix the positives. LLDB on
the exact positive source stops at __rt_heap_debug_fail from __rt_decref_mixed in
the caller's second assignment; the cell already has RC0. Emitted assembly shows
MixedBox of an already-Mixed source simply forwarding the pointer, then the
generic boxing convenience helper releasing that source. The checked return now
uses a typed, owned Acquire to materialize the result before explicitly releasing
the source lease. It does not pretend that re-boxing an existing cell allocated
another owner. The three initial strict-PHP/heap controls now pass **3/3**, 1.51s,
and the five-target guard emitter control passes **1/1**, 0.17s. Logs:
`elephc-array-return-explicit-acquire-20261011.log` and
`elephc-array-return-emitter-20261011.log`.

Additional allocation/COW isolation and null/bool/float/string/stdClass refusals
are added. Full copy/oracle group plus the new boundary subset with EIR optimization
off are running in owned session **8616**. Those expanded results are pending;
source remains uncommitted/unpushed. The named/list/method/closure/descriptor
successes above do not close nullable/union/reference-return gates or the broad
consumer campaign. CI callback failures on published c933 remain independent.

The expanded strict copy/oracle replay first reached **46/47**, failing only
PHP's return diagnostic for true: PHP reports literal `true returned`, not
`bool returned`. The array guard now selects true/false debug names explicitly,
without changing the shared string-return diagnostic. Both boolean literals
are covered. Final source passes **47/47** with **47** executed strict PHP
oracles/heap assertions, **5/5** new boundary controls with IR optimization off
and **1/1** all-five-target emitter gate. Logs:
`elephc-array-return-boolean-types-copies-20261011.log`,
`elephc-array-return-noir-20261011.log`, and
`elephc-array-return-emitter-final-20261011.log`.

The by-reference gate is now actively reproduced, not assumed covered:
`declared-array-reference-rejection.php` returns a mixed by-reference parameter
from a function declared `: array`. Elephc prints `accepted:integer` with a clean
heap, while PHP raises
`TypeError:arrayReference(): Return value must be of type array, int returned`.
`control_exit::lower_return` dispatches reference returns before ordinary return
coercion, so it bypasses the new guard. A variable result reaches AcquireRefCell
through its LoadRefCell provenance; a property result uses a checked raw cell
pointer. The next implementation must validate the referenced PHP VALUE on both
paths without copying/replacing the caller's cell or losing its lease. Do not
publish this broader checker permission until this new reference gate is reconciled.

All owned local validation sessions for this slice are terminal. Current PR head
is still c933b7c714; the new guard/checker/tests are unpublished local changes.
Published-head curl/PDO callback failures remain separate real CI regressions,
with saved completed-job logs. Whole transport/reference/consumer, nullable/union,
I/O/target/review requirements remain active; no acceptance or completion claim.

## Reference-value guard implementation replay, 2026-10-11 (unpublished)

The ordinary return guard now also validates variable reference-return payload
views, keeping the original LoadRefCell provenance for AcquireRefCell. A raw
property cell is captured into the existing ReturnRefCell lease first, then an
owning temporary ref alias exposes its PHP payload across the throwing guard.
The alias is retired/zeroed on success; receiver retirement remains after capture.
No by-reference result is replaced by the checked by-value box.

The initial expanded boundary replay is **7/8**, 2.66s. Variable positive and
scalar refusal pass; property mutation correctly reaches the caller. The property
scalar rejection produces the correct TypeError but its original 42 disappears,
even though heap counts balance. Owning the temporary CELL did not own the payload
view, yet the ordinary boundary's source-retirement rule treated it as transferable.
The reference-specific validation now passes an explicit Ownership::Borrowed
Borrow to the shared boundary, then retires only the boundary's checked value
lease. This correction is under replay, not accepted yet.

Owned session **9882** runs the eight strict PHP/heap cases then the new
all-five-target variable/property reference emitter check. Logs:
`/tmp/elephc-array-reference-borrowed-payload-20261011.log` and
`/tmp/elephc-array-reference-emitter-20261011.log`.
The previous failed log is `elephc-array-reference-guard-native-20261011.log`.
Kioku recall served the exact prior bypass evidence; graft's scoped callers lookup
had no current symbol, so checked-out source and backend materialization were used.

Published-head CI now has failures beyond curl/PDO, including non-codegen runtime
snapshots and a stale sysv alignment allowlist entry __elephc_eval_fatal which
the analyzer now proves aligned. Completed job log:
`/tmp/elephc-pr958-noncodegen-x86-shard5-20261011.log`. These cannot be dismissed as
one parity count issue. No new push, current-source full CI or independent model
consensus is claimed; reconcile the actual failed artifacts and ABI consumers.

The explicit payload Borrow and removal of unnecessary replacement staging did
not resolve the property source loss: both retries remain **7/8**. Keep those
failed logs. The fresh guard-only view now binds directly into its own cell owner,
with an asserted fresh target and a published ReturnRefCell source lease; no
existing general reference-binding helper was weakened.

LLDB at __rt_local_ref_cell_release (condition $x1 != 0) on
array-property-reference-negative.php exposes the decisive cleanup order:
the same managed cell 0x10004c708 has counts **3**, **2**, **1** at three
successive calls from method_ArrayReferenceOwner_get__cdylib_exception_cleanup.
The final decrement reaches zero even though the caller property still holds
that cell. Payload borrowing is therefore not the whole cause.

emit_function_local_epilogue_cleanup retired hidden ref-cell owners BEFORE
ordinary local alias cleanup; the latter checks whether its hidden owner is
nonzero to avoid a duplicate retirement. After owner cleanup clears it, that
check cannot prevent the second release. Function return and exception cleanup
now inspect ordinary aliases while their owners remain published, then retire
the owners, matching the existing main epilogue order. This is a common frame
ownership fix, so broader ref/exception/callback controls are required afterwards.

Owned session **31154** replays the eight strict PHP/heap cases, then the reference
guard emitter on all five targets. Log:
`elephc-array-reference-epilogue-order-20261011.log`; result still pending.
Assembly comments and diff hygiene pass. No new commit/push/acceptance is asserted.

Epilogue-order replay remained **7/8**, so its source change is necessary but not
accepted as sufficient. The second exact omission is now source-confirmed:
FunctionContext::ref_cell_owner_for_slot recognized only PromoteLocalRefCell.
The fresh guard view uses BindRefCellPtr with an explicit (alias, owner) pair,
so alias cleanup could not see its still-live owner even after cleanup reordering.
The lookup now covers Promote, Bind and Adopt owner pairs, and retained Alias
bindings. Alias lookup uses the immediately following RetainLocalRefCell for
that exact binding, not another alias that shares the same source owner; cycles
are guarded. This common provenance change needs focused multiple-alias/normal/
exception tests, not just the new property fixture.

Owned session **14447** currently runs the eight native boundaries then the
five-target emitter; `/tmp/elephc-array-reference-owner-provenance-20261011.log`.
Its source was further refined to the exact alias-retain association while the
first compile was running, so revalidate the final source afterwards even if the
first result is green. All source remains unpublished. Failed published CI
artifacts and broad array/I/O/target/model gates remain active.

The owner-provenance native replay now passes **8/8**, 18.68s. The retained
property value is again 42 after TypeError, and both variable/property returned
aliases mutate the original storage with clean heaps and strict executed PHP
oracles. Log: `elephc-array-reference-owner-provenance-20261011.log`.
The five-target emitter compilation is still live in session14447; no result is
inferred yet. A final-source full copy/no-IR replay is the next guard against
unintended changes from the common owner lookup/epilogue correction. No commit
or push of this code and no complete reference/array/CI acceptance are asserted.

The reference emitter gate also passes **1/1**, 0.18s, exercising variable and
raw-property-cell guards on all five supported targets. Session14447 is terminal.
Final-source session **3637** is now live on full strict copy/oracle controls,
then the eight boundary controls without EIR optimization. Logs:
`elephc-array-reference-final-copies-20261011.log` and
`elephc-array-reference-final-noir-20261011.log`. Do not infer their outcome from
the earlier eight-case source or the verified emitter pass.
Kioku recorded the confirmed eight-case improvement as bug_fix0c9ae8c2 and the
two-layer owner diagnosis/provenance findings separately; no personal memory or
published-source status was changed. New guard source remains untracked and all
implementation edits remain unstaged/unpublished. Seven inherited vendor deletes
are preserved separately. Next work must re-poll3637, validate broader reference/
exception cleanup and reconcile published-head CI callback failures before push.

## Final reference replay and PDO callback reducer, 2026-10-11

Final-source copy/oracle group passes **50/50**, 21.55s, and the eight declared-array
boundary cases pass without IR optimization **8/8**, 3.37s. Fifty actual strict
PHP oracle markers were counted. Both owned sessions are terminal; this closes
those focused replays, not the broader union/nullable/consumer/I/O/model gates.

The published-head PDO FETCH_FUNC invokable-object failure reproduces locally
as SIGBUS (Linux CI was SIGSEGV). The test runner's retained LLDB replay stops in
__rt_is_callable_method_name reading a heap pointer as class ID, through
__rt_is_callable_object / __rt_is_callable_mixed / PDOStatement::fetchAll.
Log: `/tmp/elephc-pdo-invokable-local-20261011.log` (1 failed, 113.68s, including
managed bridge build). A direct mixed-object callback relay without variadics
prints invoked=object and reports clean heap, matching PHP.

An independent no-PDO variadic reducer fails heap debug before is_callable:
invokeVariadic(int $mode, mixed ...$args) assigns args[0] to a callback, validates
it and invokes it. LLDB stops in __rt_incref / __rt_mixed_from_value / __rt_mixed_clone
on the already-freed invokable object. Its generated EIR shows two Release v3
instructions after one MixedBox v3, before the collector ArrayPush. Artifacts:
`/tmp/elephc-clone-reconcile.b6mG9w/invokable-{mixed-relay,variadic-validation}.php`
and `/tmp/elephc-invokable-variadic-validation-20261011.eir`.

The scoped caller audit confirms coerce_variadic_tail_value boxes a concrete
source via box_value_as_mixed, which already retires an owning source. Only
lower_variadic_tail_source_value redundantly releases that original again when
the result changes. That duplicate retirement is removed; the helper's consuming
contract is documented, not changed for other callers. The added strict PHP/heap
variadic-invokable regression then the original PDO test are running serially in
owned session **85832**. Logs:
`elephc-variadic-invokable-single-release-20261011.log` and
`elephc-pdo-invokable-single-release-20261011.log`. Results remain pending; other
curl/eval/PDO failures are not attributed or claimed fixed yet. No new publication.

The temporary invokable regression now passes **1/1**, 3.12s, with the strict PHP
oracle marker and clean heap. The original PDO FETCH_FUNC invokable-object
fixture also passes **1/1**, 39.88s, without changing the fixture or is_callable
helper. Session85832 is terminal. This confirms the double-retirement primitive
as the cause of that reproduced callback failure; it does not prove every CI
callback failure has the same cause or close Linux coverage.

A representative failed CI curl callback (throw clearing a previous transfer's
error state) and the full copy/oracle group are now running serially. Logs:
`/tmp/elephc-curl-callback-single-release-20261011.log` and
`/tmp/elephc-variadic-single-release-full-copies-20261011.log`.
All code remains local/unpublished. Existing public head is c933b7c714, whose
CI failures are retained until an exact-head corrected replay is actually green.

Full strict copy/oracle group after the variadic retirement correction passes
**51/51**, 11.95s. Its regression includes the temporary invokable object and
is_callable validation; it does not hide the failure by removing that check.

The curl callback has a second, independent foreign-boundary defect. On macOS,
the retained LLDB replay stops in __rt_heap_alloc reading address0x4030 through
x28, entered from __rt_curl_invoke_callback / Rust run_bytes_callback / libcurl.
Rust/C preserves callee-saved registers on return but may use them while calling
back; x28 at that entry is not an Elephc runtime context. This adapter had no
context publication, unlike the generic descriptor FFI trampoline.

The curl adapter now saves foreign x28/r14 in an added aligned frame spill,
publishes the existing runtime context before allocating arguments, and restores
the foreign register on both normal and no-invoker exits. Handler record offsets
remain unchanged; ARM frame is320 bytes, x86 frame304 bytes. No public API or
binding-policy argument changed. The existing scope restrictions on curl remain;
pooled/worker context provenance is still a separate acceptance audit, not claimed
by standalone root-context publication.

The exact previously failing curl callback now passes **1/1**, 10.78s, in
`elephc-curl-callback-foreign-ctx-20261011.log`. A new all-five-target emitter
ordering/save/restore test and full callbacks/streams filters are live serially.
Logs: `elephc-curl-foreign-ctx-emitter`, `elephc-curl-callbacks-foreign-ctx-full`,
`elephc-curl-streams-foreign-ctx-full` with 20261011 suffix. No broader outcome,
new push, complete CI or exact-artifact consensus is claimed yet.

The new foreign-context emitter invariant passes **1/1**, 0.00s, covering all
five targets: spill the foreign callee-saved ctx register, publish before the
first argument allocation, restore on both exits. Log:
`/tmp/elephc-curl-foreign-ctx-emitter-20261011.log`.
The broader callbacks/streams run remains live in owned session **93607**,
currently compiling the curl-feature codegen binary. Re-poll that session and
read both logs rather than restart based on elapsed time. No source changed
after the last nonsemantic comment alignment/test addition; no push is authorized
by this passing narrow invariant alone. Source hygiene passes and index is empty.

## Broader curl gates: default-output foreign entry, 2026-10-11

The full callback filter is **30/32**, 68.54s. Only stdout-default paths fail:
curl_setopt_null_restores_the_default_write_path and copy-handle stdout mode over
a rooted write callback. The independent streams filter is **7/9**, 31.57s,
again failing default stdout after option/reset/copy transitions. Preserve both
logs; neither outcome is complete curl coverage.

Their LLDB traces stop in __rt_stdout_write reading 0x4100 through the foreign
ctx register, entered directly from Rust php_layer::write_all_stdout. These calls
never pass through the newly fixed PHP-callable adapter. The published sink had
been the INTERNAL stdout helper itself. Its pointer/length convention already
matches the C ABI on both architectures; the missing part is safe ctx entry, not
an invented argument shuffle.

A new __rt_curl_output_sink C entry saves foreign ctx, uses the existing context
publish, delegates to __rt_stdout_write, restores ctx and returns. Curl transfer
lowerings now publish this adapter address. The Rust bridge retains the same
opaque (ptr,len)->void ABI and updated safety description. No runtime bridge
symbol or direct-fd fallback was introduced; output buffering semantics remain
on the ordinary funnel. All five targets get the same ABI helper path plus an
emitter order/save/restore regression. Root publication does not close pooled/
worker creator-context or output-handler exception-firewall audits.

Owned session **9699** replays the two exact stdout regressions then the
all-target emitter test. Logs:
`elephc-curl-default-stdout-foreign-ctx-20261011.log`,
`elephc-curl-copy-stdout-foreign-ctx-20261011.log`,
`elephc-curl-stdout-foreign-ctx-emitter-20261011.log`.
Still compiling; no pass is asserted. All code is unpublished, public headc933
still has unresolved complete CI, and full array/I/O/review requirements remain.

The foreign-safe default-output adapter now passes both unchanged exact native
stdout regressions: option-null reset **1/1**, 37.58s, and copy-handle mode
preservation **1/1**, 40.80s. Their actual outputs match the existing assertions.
The five-target output-sink emitter check remains live in session9699; the full
callback/streams filters are queued serially as a further replay, not claimed
green yet. Logs:
`elephc-curl-callbacks-output-sink-final-20261011.log` and
`elephc-curl-streams-output-sink-final-20261011.log`.
All code is still local/unpublished; no complete callback, pooled/worker-context,
handler-throw, full target matrix or model acceptance is inferred from two tests.

Current owned handles are confirmed live: **9699** on the output-sink emitter
compile, **88937** on callbacks/streams replay waiting for Cargo's artifact lock.
Do not restart either because of elapsed observation time; serialize via the
existing lock, then inspect terminal results. Metadata/hygiene checks pass, all
new output-sink code and address publication changes remain local, and the index
is empty. Kioku recorded the separate default-sink foreign-entry finding as
gotchae817343b. Pooled/worker context provenance and native output-handler throw
firewall remain explicit whole-goal gates rather than hidden root-only acceptance.

The output-sink emitter passes **1/1** on all five targets; session9699 is
terminal. Full corrected callback filter now passes **32/32**, 62.03s, and
streams pass **9/9**, 30.13s; session88937 is terminal. No skips are reported in
these logs. The previous 30/32 and 7/9 outcomes remain retained failure evidence.
This confirms default stdout context entry as the remaining cause in those two
filters, not a complete curl/eval or native target acceptance claim.

The curl-eval filter is now running in owned session **93897** on the same source
with real native packages in `elephc-curl-eval-foreign-ctx-full-20261011.log`.
Keep the PR headc933 and its old failures separate from local corrected-source
outcomes. No new push or full-goal acceptance is asserted.

The exact eval session93897 is confirmed live after its per-test >60s notices.
Its own compile output shows managed Magician/crypto/phar/pcntl/iconv bridge
auto-build activity, followed by more active test notices. Those notices are not
test failures or a terminal process. Keep the same handle and recover actual
results before rerun; all prior stdout/callback/stream handles are terminal.
The new output-sink module remains untracked, implementation edits unstaged,
index empty and public head c933b7c714 unchanged. No complete target/provenance/
output-handler exception/review acceptance has been inferred from this progress.

Session93897 is now terminal: curl-eval is **22/34**, 484.66s. Twelve retained
failures concern callback throw/nesting/pause/read/header-progress-debug, multipart
POSTFIELDS/default MIME, CURLFile construction and write-mode transitions.
Most callback cases are SIGSEGV; two multipart cases report eval runtime failed.
Exact log: `elephc-curl-eval-foreign-ctx-full-20261011.log`. The successful AOT
callbacks32/32 and streams9/9 must NOT be reported as a green eval boundary.
No process was restarted or killed based solely on >60s notices.

All owned local validation handles in this tranche are terminal. Next work traces
the first retained eval crash through actual LLDB frames and callback/native
runtime registration, then reconciles the other error group. Implementation,
new output-sink module and plans remain local/unpublished; index stays empty.
Public PR head is unchanged atc933b7c714. Complete PHP-array nullable/union/
consumer/reference breadth, context-pool provenance, output-handler exceptions,
I/O queue/provider, all targets and dedicated review consensus remain open.

## Resumed eval foreign-context investigation, 2026-10-11

The retained LLDB trace enters `hash_context` through Magician's curl callback,
then the scalar C wrapper and `__rt_mixed_from_value`, with foreign x28 reading
0x4028 in the allocator. Callee-saved is a return obligation, not a guarantee
that Rust leaves the register untouched throughout a callback. Publishing root
`_rt_ctx` would not establish worker/pooled-context provenance.

Local WIP introduces an additive six-argument `execute_in_context_v1` entry:
the generated caller passes its actual reserved-register arena explicitly. A
borrowed thread-local activation restores the previous pointer on return and
Rust panic. The existing five-argument ABI and descriptor ABI4 stay unchanged.
The full Magician value-wrapper emitter adds argument-preserving C gateways
that consult this activation, enter that arena, call the original body, and
restore the foreign register. Protected wrappers emitted in the same scope use
the gateway too; native-only eval fragments do not acquire a Magician dependency.

TLS nesting/unwind/thread-isolation unit is **1/1**; five-target emitter check
is still compiling in owned session13257. No native replay is green yet. This
is not whole-boundary acceptance: calls outside a scoped eval activation keep
their existing behavior and still require provenance/firewall inventory.
Generated stack-argument wrappers, exceptional paths and callback native
descriptor entries must be tested rather than assumed covered. No publication.

The first five-target gateway unit passes **1/1** (session13257 terminal).
The first exact native replay is **0/1**, 116.76s, retained at
`/tmp/elephc-curl-eval-explicit-ctx-20261011.log`. Its LLDB stack no longer dies
in `hash_context` allocation: it now reaches callback execution and fails inside
the per-program `RuntimeException` constructor while freeing native metadata.
That constructor was emitted in user assembly, outside the runtime facade's
C-entry label helper, and inherited foreign x28 too. This is not eval success.

The second local revision routes per-program constructor, method, property,
static-property, class-constant, reflection and handler C labels through the
same gateway when `module.required_runtime_features.eval_bridge` is true.
Known seven/eight/seventeen-integer hook prototypes get real C stack forwarding
on both architectures; new tests check the inventory against Rust declarations
and all five emitted layouts. The extra native-only no-Rust-dependency assertion
and newer stack tests are pending. The replay is owned session51624 in
`/tmp/elephc-curl-eval-explicit-ctx-constructor-20261011.log`, still compiling.
Source is local/unpublished; callback descriptor entries, out-of-activation
cleanup, all supported native targets and performance/firewall gates stay open.

Kioku lexical recall and scoped file scans work. Semantic recall explicitly
reports its engine unavailable and falls back to lexical search; do not present
that run as semantic evidence. The graph's old line spans were marked drifted,
and focused scans refreshed the touched leaf rather than scanning scratch trees.

Session51624 is terminal: the unchanged exact native callback-throw reproducer
passes **1/1**, 102.39s, without a native-package skip. The expected complete
output proves callback exception construction/catch, errno zero and a successful
second transfer after resetting the write option. This is host evidence, not a
complete eval/target/provenance acceptance. The first failed native log stays.

The current-source full curl-eval filter (two test threads) and then the focused
eval-bridge unit module are queued serially. Logs:
`/tmp/elephc-curl-eval-explicit-ctx-full-20261011.log` and
`/tmp/elephc-eval-explicit-ctx-bridge-units-20261011.log`.
No new feature work or source edits are planned while that artifact is validated.

The full filter runs in session62367; an early copy of the same focused unit
command runs in session49907 while native replay is assembling/running, with
only one heavyweight Cargo compilation active. Its separate log is
`/tmp/elephc-eval-explicit-ctx-bridge-units-early-20261011.log`. The original
serial unit replay remains queued and will reuse the build. Do not infer a
terminal result from either handle's idle output.

Semantic recall correction: Ollama answers `/api/tags`, `embeddinggemma:latest`
exists, and the identical semantic query succeeds when its one-call timeout is
raised to 30s (`KIOKU_SEMANTIC_TIMEOUT=30`), without changing configuration.
The earlier default-timeout failure is retained. Inspecting Kioku's primary CLI
shows a 3s default timeout also covering the initial entire-corpus embedding
request; corpus-hash mismatch rebuilds that batch. This is a concrete UX/perf
feedback candidate: distinguish model availability from embedding timeout, and
separate cold corpus preparation from warm query latency. No Kioku code changed.

### 2026-10-11 resumed: explicit arena replay and native collector mismatch

The full current-source curl-eval replay has finished: **31 passed / 3 failed / 0
ignored** (2020.45s runtime; two test threads). This supersedes the earlier
22/34 baseline only for this filter. The three remaining failures are
`eval_curlfile_and_curl_file_create_construct_working_objects`,
`eval_curl_postfields_array_posts_real_multipart`, and
`eval_curl_postfields_empty_array_and_default_mime_match_php`.
Evidence: `/tmp/elephc-curl-eval-explicit-ctx-full-20261011.log`.

The GC test's global-symbol assertions were obsolete in context-register mode.
They now check target-aware generated state loads/stores/clears, preserving the
original counts and collection/exception/boxing/ownership ordering. Focused
GC replay passes 1/1, and the complete eval-bridge unit filter passes **28/28
with two cross-assembler tests still ignored**. Logs:
`/tmp/elephc-eval-gc-context-state-20261011.log` and
`/tmp/elephc-eval-context-bridge-confirmed-20261011.log`.

LLDB against the actual failed CURLFile executable establishes that native
allocation returns tag6 Object, but construction returns false. The failure
occurs on the fourth physical argument (hidden surplus collector): tag4 packed
array is rejected by the generated constructor's tag5 Hash ABI guard. This is
not a missing CURLFile class or constructor. By-value EIR method/constructor
collectors are currently Hash(Mixed), whereas Magician allocated packed
collectors and promoted only after a string key. Empty/positional collectors
therefore disagreed. By-reference EIR collectors retain their distinct path.

New focused fake-runtime regression
`native_collector_storage_matches_generated_eir_transport` reproduces the
packed-vs-hash mismatch before the fix (0/1). The binder now allocates hashes
for by-value collectors from the start and retains indexed storage for reference
collectors; the regression covers source/hidden collectors, count metadata and
the binder's release. Post-fix regression passes 1/1; focused Magician scopes
pass 28/28 (native scope), 26/26 (method arguments), 13/13 (reference cleanup)
and 30/30 (adjacent argument introspection/object construction). Logs:
`/tmp/elephc-native-collector-before-20261011.log` (red),
`/tmp/elephc-native-collector-after-20261011.log` (green),
`/tmp/elephc-native-collector-scope-20261011.log`,
`/tmp/elephc-native-collector-methods-20261011.log`,
`/tmp/elephc-native-collector-ref-cleanup-20261011.log`, and
`/tmp/elephc-native-collector-adjacent-20261011.log`.
The actual native three-test replay is now terminal **3/3 passed, zero ignored**
(403.01s execution after 3m51s compiler build), using real curl native packages
and HTTP multipart transfers. Log:
`/tmp/elephc-curl-eval-collector-native-20261011.log`.
The complete CURLFile/CURLStringFile object's expected output was independently
confirmed with the host PHP **8.5.10** curl extension. This closes the three
previously failing cases in a focused replay; it does not claim a fresh 34/34
full curl-eval replay after the collector change.
This aligns an existing internal transport, not a decision to expose uniform
Hash as PHP array semantics. Canonical consumer/return boundaries remain open.

Kioku `callers bind_native_signature_args --context 1` was terminated after
three minutes without output; scoped source and native assembly evidence were
used instead. No Kioku configuration or implementation was changed.

An additional focused gateway test cross-assembles the actual register-preserving
wrappers, including floating input and 7/8/17-integer C signatures, with clang on
all five supported target triples. Running the complete eval-bridge unit filter
with `--include-ignored` now passes **31/31, zero ignored**, including both
previously ignored protected-output/mbstring assembly checks. This is assembler
evidence, not Linux executable or iOS SDK/runtime acceptance.
Log: `/tmp/elephc-eval-context-bridge-assembled-20261011.log`.

All owned build/test handles in this resumed tranche are terminal. Hygiene
`git diff --check` passes; the index is empty. The local fixes remain unpublished,
and inherited vendor deletions/unrelated local metadata have not been staged.
No new public API, I/O-provider or heartbeat behavior was added. Remaining gates
are canonical PHP-array consumer/reference breadth, direct callable foreign
entry/context provenance, pool/worker ownership and exception firewalls, scoped
I/O completion, supported-target native/SDK/CI evidence and the frozen exact
Candidate62 dedicated Codex/Kimi/GLM/DeepSeek consensus round.

### 2026-10-11 callback-entry provenance audit in progress

- [x] Protect Rust -> registered AOT free-function descriptor ABI4 entry before
      ownership initialization, binding or exception-frame access.
- [x] Capture the actual native arena when a delayed eval callback re-enters Rust
      after the original execute activation returned.
- [x] Check Rust -> callback-adapter allocation, which is emitted outside the
      runtime C-wrapper facade.
- [ ] Validate nested/throw/normal returns and foreign register restoration on
      all supported targets; keep native-only invokers free of Magician dependencies.
- [ ] Audit remaining post-barrier probes/binding/handler and pooled-context entries
      independently; do not turn a captured eval metadata handle into arena ownership.

The registered free-function path is source-confirmed:
`NativeFunction::call` invokes the generated ABI4 function pointer directly
from Rust; `emit_eval_native_function_invoker_inline` selects an exception-bounded
invoker, whose body starts using the reserved context register without a gateway.
Ordinary native descriptors use a different entry and must remain unchanged.

The delayed callback path uses the legacy four-argument
`__elephc_eval_callable_call_array`, with no explicit native arena. The previous
execute TLS activation cannot cover calls that happen after `eval()` returns.
Callback-adapter allocation also has its own user-assembly entry
`__elephc_eval_wrap_callback`, outside the shared `label_c_global` facade.

New regression sources are `runtime_callable_invoker/foreign_context_tests.rs`
and `tests/codegen/eval/foreign_context.rs`. The former checks context installation
before the actual descriptor body on all five targets and native-only dependency
isolation; the latter covers an allocating native function plus a catchable native
throw, and an eval closure invoked after its creating activation returns. Initial
test-helper compile error was corrected without changing production behavior;
the actual before-fix runs are pending in logs
`/tmp/elephc-descriptor-context-before-20261011.log` and
`/tmp/elephc-eval-descriptor-context-before-native-20261011.log`.
Disk remains constrained (~24-27GiB free), so focused Cargo builds are serialized;
foreign worktrees/builds and inherited vendor deletions remain untouched.

The actual descriptor entry regression is red before production changes: 0/1,
missing the captured-arena gateway on macos-aarch64. Both functional native
scenarios pass BEFORE the fix (2/2, 450.21s); they are positive coverage, not a
deterministic reproduction of Rust's use of its callee-saved context register.
Do not claim they were crashing regressions.

The first production correction installs the shared gateway only for eval's
Rust-entered free-function invokers (`catch_native_throws=true`). Their native
ownership/binding/unwind body is unchanged; ordinary native invokers still have
no TLS dependency. The full focused invoker unit filter passes **29/29**, including
the previously red gateway ordering/native-only isolation test on all targets.
Log: `/tmp/elephc-descriptor-context-after-units-20261011.log`.

An additive `__elephc_eval_callable_call_array_in_context_v1` entry then scopes
the actual caller arena around delayed Rust callback execution and restores outer
activation on return/panic. Descriptor ABI4 and the legacy four-argument callback
export are unchanged. Both the dynamic descriptor invoker and the EIR callable
array lowering pass the fifth C argument from x28/r14 through ABI helpers.
The separate callback-adapter allocation entry now uses the same shared gateway.
This does not transfer ownership of an eval metadata handle or admit callbacks
from a different worker arena.

Focused callback/gateway/bridge replay passes **35/35, zero ignored**, including
clang cross-assembly of the real dynamic callback entries and descriptor data on
all five supported targets. Log:
`/tmp/elephc-callback-context-final-units-20261011.log`.
The post-change native two-test replay is queued/running in
`/tmp/elephc-eval-descriptor-context-after-native-20261011.log`.
A new explicit native C probe poisons x28/r14 with 0x4038, makes the getter clobber
integer and floating argument registers, and checks installed arena, argument
preservation and foreign-register restoration. It tests the actual generated
gateway with a controlled fake pointer (no heap or full runtime claim), and is
pending in `/tmp/elephc-callback-context-poisoned-final-units-20261011.log`.
Assembly-comment alignment and `git diff --check` pass on the touched sources.

The final focused gateway/callback/bridge filter is now **36/36 passed, zero
ignored**, including the actual C native poisoned-register probe and the target
assembly checks. The poison probe uses the existing gateway with a controlled
arena pointer, checks all four integer operands plus d0/xmm0, and verifies x28/r14
returns to 0x4038 rather than leaking the installed pointer into the foreign
caller. Log: `/tmp/elephc-callback-context-poisoned-final-units-20261011.log`.
The exact two PHP sources independently produce the asserted outputs on host
PHP8.5.10; `/tmp/elephc-eval-descriptor-context-php-oracle-20261011.log`.
The post-change native two-test replay and the focused existing curl callback
throw/nested transfer pair are still running; the latter's log is
`/tmp/elephc-callback-context-curl-nested-20261011.log`.

Final native results are terminal: **2/2** allocating AOT descriptor/exception
and delayed callback tests pass (92.07s), and **2/2** existing curl callback-throw
and nested-transfer tests pass (78.84s), without ignored tests or native-package
skips. The former matches the independently executed PHP8.5.10 sources.
Logs: `/tmp/elephc-eval-descriptor-context-after-native-20261011.log` and
`/tmp/elephc-callback-context-curl-nested-20261011.log`.
The internals documentation now distinguishes interpreter metadata from the
borrowed native arena, explains the additive execute/callback context entries,
and preserves the legacy exports/descriptorABI4 compatibility.

All owned validation sessions for this tranche are terminal. Code, regression
tests, docs and Kioku packets remain local/unpublished; index is empty and
inherited vendor deletions remain untouched. No model consensus or full scheduler
acceptance is claimed. Next audit targets the activation-free post-barrier
`is_callable`/closure-binding/handler hooks, pool/worker provenance and metadata
lifetime; target-native Linux/iOS SDK, canonical PHP-array/I/O and exact-review
gates remain open. Assembly evidence is not native multi-target acceptance.

### 2026-10-11 post-barrier callable probes and closure rebinding

- [x] Route generated `is_callable` and dynamic callable-descriptor probes through
      an additive three-argument context entry, preserving the legacy export.
- [x] Add five-argument closure rebinding entry and a distinct v2 hook installer/slot;
      keep v1 four-argument invocation separate and retain native-only closure paths.
- [ ] Validate hook replacement/null disabling, five-target assembly, host heap-debug
      rebinding after `eval` returns, and adjacent callback/native closure regressions.
- [ ] Continue other post-barrier/handler/object-lifetime hooks and pooled worker audit.

Source confirms `is_callable` had three generated call sites without an arena:
the EIR probe plus ARM64/x86_64 dynamic descriptor materialization. They now pass
the current arena as C argument3 into a nested borrowed activation. The generated
closure binder previously called a process-installed four-argument Rust callback
after its creating `eval` activation had returned. A new versioned hook is required,
not reinterpretation of the existing pointer's signature.

The new all-target closure-entry regression is **red before the fix (0/1)**,
missing the context hook slot. Evidence:
`/tmp/elephc-post-barrier-bind-before-20261011.log`.
New v2 registration points to `__elephc_eval_closure_bind_this_in_context_v1`;
the runtime selects `_elephc_eval_closure_bind_ctx_fn` first and only materializes
the fifth arena argument for that branch. V1 remains a four-argument fallback.
Installing either version clears the other slot, also for a null replacement,
so a stale signature cannot remain selected. Hook pointers are code, not arena
ownership; worker/context metadata lifetime is not thereby closed.

The native regression probes an eval closure after its execute activation ends,
rebinds it to an AOT object, and calls it with heap debugging enabled. Its source
independently prints `callable|bound-arena|alive` on PHP8.5.10;
`/tmp/elephc-post-barrier-bind-php-oracle-20261011.log`.
Focused initial unit/native replays are pending in
`/tmp/elephc-post-barrier-context-units-20261011.log` and
`/tmp/elephc-post-barrier-context-native-20261011.log`.
The added ABI replacement and five-target assembler checks require a final
current-source replay; do not treat an earlier build as covering them.

Initial focused units finished **38/38 passed, zero ignored**. The final added
installer-replacement and five-target closure-bind assembly tests have started,
but the independent C poisoned-register probe is still running after a >60s
notice; no terminal final-unit count is available yet.

The first new native replay is NOT green: the unchanged delayed-callback case
timed out at the harness's 60s binary limit with empty stdout/stderr. The log
retains that failure and continues with the allocating native descriptor case.
The adjacent native-closure binding test has passed; the eval rebinding/ref and
receiver destructor-cleanup cases are still pending. Commands querying host
state (`vm_stat`) and reading logs also respond unusually slowly. This is
correlated environmental evidence, not proof the timeout is unrelated to code.
No other-worktree process was killed or paused.

Retained delayed-callback executable:
`/private/var/folders/9d/f4jbjvxs0t5d94xl7cp671t00000gn/T/elephc_test_96735_ThreadId(2)_0/test`.
The first manual replay command omitted shell quoting around `ThreadId(2)` and
failed in zsh glob expansion; it did not execute the binary and is not runtime
evidence. Re-run with an explicitly quoted path before drawing conclusions.
Validation and consensus remain open until a real terminal native result exists.

Final focused units are terminal **40/40 passed, zero ignored**, including both
version-replacement/null-disabling semantics and the actual five-target rebinding
branch assembler checks. The independent C native gateway probe eventually
passed; total filter duration was 1102.74s, so retain that timing rather than
presenting it as an ordinary fast run.

Both first native filters failed their 60s executable deadlines: **0/3** on the
eval-context filter, **1/3** on the adjacent closure/receiver filter. All five
failures have empty stdout/stderr and remain retained. Host `vm_stat` eventually
reported roughly6.6GB free and no swapins/swapouts; it does not prove memory
exhaustion. Sampling the manually rerun delayed callback executable establishes
it was sleeping at `_dyld_start+0` throughout894 samples with a112KB peak footprint,
before executing the generated program. Evidence:
`/tmp/elephc-post-barrier-delayed-sample-20261011.log`.
The same unchanged retained executable subsequently exited0 with
`ARENA/later|alive`. This diagnoses a sampled loader-startup stall, not a compiler
loop, and does not automatically exonerate every other timeout.

Explicitly quoted direct replays of the four other retained binaries are pending
in `/tmp/elephc-post-barrier-{bind,descriptor,byref,receiver}-direct-replay-20261011.log`.
No security setting, watchdog limit, other-worktree process or test expectation
was changed to manufacture green evidence. Keep direct replays distinct from
the failed harness runs and investigate any genuine output/heap difference.

All three other live direct replays were independently sampled: bind887,
descriptor888, receiver888 samples stayed at `_dyld_start+0`, each with112KB
peak footprint and no binary-image description available. Logs:
`/tmp/elephc-post-barrier-{bind,descriptor,receiver}-startup-sample-20261011.log`.
They have not reached generated program execution. Current-source closure/byref
retained replay exits0; a warm second run captures exact stdout separately:
`O:15:integer:15|19:integer:19|23:integer:23|N`.
Its stderr is the expected `Cannot bind an instance to a static closure` warning.
Logs: `/tmp/elephc-post-barrier-byref-{stdout,stderr}-20261011.log`.

The bind binary has an embedded linker-generated ad-hoc arm64 signature, as
expected for these native outputs, and `com.apple.provenance` rather than an
observed quarantine attribute. Signature verification is read-only and pending;
no attribute was removed and no signing/security policy was changed. The three
direct replays remain live; validation is held at the host loader boundary, not
declared complete from successful unit or assembly results.

`codesign --verify --verbose=2` on the retained bind executable exits0:
valid on disk and satisfies its Designated Requirement. This rules out a detected
invalid signature for that artifact without altering its ad-hoc signature.
The read-only Docker image/volume inventory is checked as a possible Linux
validation alternative; do not assume a matching cached Rust archive exists.

The remaining three retained direct replays are now terminal exit0 with the
expected PHP outputs, but TWO heap ownership defects are exposed. The new
post-barrier probe/bind case leaves6live blocks/560bytes (80kind4,272kind0,
80kind3,40kind5,48kind1,40kind5); the existing throwing receiver cleanup case
leaves1live block/1040bytes kind1. Neither is heap-clean acceptance. The new
regression now explicitly requires `HEAP DEBUG: leak summary: clean`, as the
existing receiver test already did. Logs:
`/tmp/elephc-post-barrier-bind-direct-replay-20261011.log` and
`/tmp/elephc-post-barrier-receiver-direct-replay-20261011.log`.
The descriptor direct replay prints `arena/ARENA|native-arena|alive` and exits0.

Docker ARM image is pinned locally at
`sha256:3f506a232838c660a0258049609b99a32be19beeaff50f93bd691da0c599a96e`.
The first source build failed SIGBUS while compiling dependencylibm with its
target directory on the macOS bind mount; it is not Linux acceptance. A second
build uses task-owned `elephc-post-barrier-arm-target-20261011` on native Docker
storage (~62.6GB free), read-only workspace,1CPU/3GB, and finished exit0. It
emits pre-existing musl libc::time_t deprecation warnings in time/date.rs,
unrelated to these edits. No unrelated volume is mounted or removed.
Next priority is the two real ownership failures; the loader timeout evidence
is preserved independently, and all consensus/publication gates remain open.

The native Docker ARM build produces a92.4MB current-source Magician staticlib
and passes the nested/unwind/thread-local arena unit **1/1**. The target volume
uses~670.3MB. This proves the ARMmusl Rust bridge builds and its scoped activation
works; it is not an executed PHP program or Linux x86_64 acceptance. All owned
build, native replay and sampling sessions are terminal. The new clean-heap
assertion is intentionally expected to fail until the six-block leak is fixed;
no fresh green codegen claim is made after strengthening it. Current changes
remain unstaged/unpublished, with inherited vendor deletions preserved.
