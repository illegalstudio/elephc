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
