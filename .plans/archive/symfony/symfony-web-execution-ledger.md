# Symfony web execution ledger

## Goal

Replace blind runtime iteration with a frozen, evidence-backed map of the stock
application's default HTTP request. The map records every code edge observed by
native PHP and every extra compiler/runtime edge introduced while lowering it.
Each unsupported or divergent edge gets a PHP-oracle regression before an
Elephc implementation is written.

The acceptance outcome remains the stock `--web` executable answering `GET /`
with the application's default `404` response containing `Welcome to Symfony!`.
Neither the application, its dependencies, nor generated framework sources may
be changed. Production code must remain independent of framework, package
manager, or project-layout names.

## Checklist

- [x] Freeze the exact fixture and toolchain baseline.
- [x] Capture one native PHP Xdebug trace for `GET /`.
- [ ] Capture the compiler resolver/EIR/runtime inventory for the same entry point.
- [x] Build and validate the normalized execution ledger.
- [ ] Derive PHP-oracle TDD fixtures for every missing or divergent surface.
- [ ] Implement closed batches of generic compiler/runtime behavior.
- [ ] Re-run the ledger and web acceptance after every batch.
- [ ] Profile compilation size/time only after the functional ledger is closed.

## Frozen baseline

Record before any new implementation:

| Evidence | Required value |
| --- | --- |
| Compiler revision and dirty-source digest | `git rev-parse HEAD` plus hashes of touched compiler files |
| Stock fixture identity | tracked-file hash and a check that application/dependency files remain unchanged |
| Native PHP identity | `php -v`, loaded Xdebug version, relevant INI values |
| Request contract | HTTP method, path, headers, status, body marker |
| Elephc command | complete command, target, heap size, output directory |

All generated maps, traces, logs, and comparison outputs belong under `target/`.
They are diagnostic artifacts, not application inputs.

### Captured native baseline

The first capture was made on 2026-08-29 with PHP 8.5.6 and Xdebug 3.5.0.
`GET /` returned `404 Not Found`, a 39,542-byte response, and the marker
`Welcome to Symfony!`. The compact Xdebug trace has 72,544 events, 570 distinct
callee symbols, and 215 distinct include targets. The before/after fixture status
contained the same paths and statuses; only the generated status file's final
newline differed.

### Normalized execution inventory

`scripts/normalize_xdebug_trace.py` is a generic Xdebug format-4 normalizer;
it contains no application, framework, or package-layout rule. Its regression
test is `scripts/tests/test_normalize_xdebug_trace.py`. The captured trace is
materialized under `target/symfony-native-xdebug/ledger/` as `events.tsv`,
`edges.tsv`, `symbols.tsv`, `includes.tsv`, and `summary.json`.

The frozen request currently contains 36,269 call-entry events, 1,421 distinct
PHP-level edges, 570 distinct targets, and 215 include targets:

| Kind | Events | Interpretation |
| --- | ---: | --- |
| Builtin | 26,582 | Compare against the shared AOT/eval contract before adding any surface. |
| Reflection | 5,715 | Treat as metadata families, not independent methods. |
| Instance/static/constructor | 3,737 | User code and dynamic bridge dispatch; classify by their actual execution context. |
| Include | 215 | Preserve observed paths and autoload origin in the ledger. |
| Function | 20 | User-level global call surface. |

The contract cross-check found 94 observed builtins supported in both AOT and
eval (25,729 calls), six supported only in AOT (`get_debug_type`, `filter_var`,
`substr_count`, `unserialize`, `headers_sent`, `error_log`; 65 calls), and 15
observed but not registered in the shared contract (788 calls). The latter are
an explicit backlog, not evidence that every one must be implemented before
the current earlier execution blocker: `error_reporting`, `substr_compare`,
`opcache_is_script_cached`, `ini_set`, error/exception-handler management,
`parse_str`, `ini_get`, `strnatcmp`, shutdown registration, `flush`, and
`error_get_last`.

### Static retention cross-check

`scripts/classify_execution_ledger.py` joins the native `symbols.tsv` and
`edges.tsv` with a prior executable's assembly labels, the generated shared
builtin registry, and the compiler-mode prelude sources selected by the target
mode. It is application-agnostic: labels establish static retention, not runtime
parity, and every non-retained result remains a provenance question until it has
an oracle.

The first run against the r54 executable classified the 570 native symbols as
follows:

| Classification | Symbols | Interpretation |
| --- | ---: | --- |
| AOT and dynamic contract | 94 | Shared builtin surface is present in both paths. |
| AOT-only contract | 5 | A dynamic boundary may still expose a missing implementation. |
| Web-mode prelude only | 10 | Generic `--web` source provides the name; dynamic compatibility remains open. |
| Missing in this target mode | 6 | 374 calls need a standard generic surface or a documented capability boundary. |
| Static label retained | 234 | The compiled binary contains a matching source-declared symbol. |
| Reflection runtime provenance required | 30 | Reflection uses metadata/runtime paths rather than ordinary user-method labels. |
| Resolver provenance required | 4 | `include`/`require` forms require path and loaded-file proof. |
| Static label not retained | 187 | Do not call this unsupported without further provenance. |

The registry-only view still has 15 absent names, but the target-mode pass proves
that the web prelude covers the error-mask, handler-stack, shutdown-registration,
INI, and `error_log` rows (415 calls). The remaining rows are `substr_compare`,
`opcache_is_script_cached`, `parse_str`, `error_get_last`, `flush`, and
`strnatcmp`; this is the actual contract backlog for `--web`.

The reflection category accounts for 5,715 native calls. The remaining
non-retained rows include generated closure/container names and ordinary source
methods. Consequently the next oracle batch is ordered by boundary rather than
raw symbol count: reflection metadata first, then generated/dynamic declaration
identity, then only genuinely unretained source methods. This avoids treating a
metadata family as hundreds of independent missing APIs.

## Evidence acquisition

### 1. Native request trace

Use the stock public directory with PHP's built-in server and Xdebug tracing.
Keep the application source unchanged. Capture one clean `GET /`, then stop
only the server started for this trace.

The trace must include function/method entry and exit, file inclusion, and
timestamp/memory columns where Xdebug provides them. A request timeout, warning,
or exception is evidence and belongs in the ledger; do not silently filter it.

### 2. Compiler inventory

Compile the identical entry point into an isolated output directory with:

- the backend inventory enabled;
- per-phase timings;
- EIR/source-map output when needed to map an emitted symbol back to a source
  declaration;
- `ELEPHC_EVAL_TRACE=1` only for dynamic-evaluation boundaries.

The inventory must distinguish source declarations kept conservatively from
declarations and runtime helpers actually reached during the native request.
This prevents a static compilation closure from being mistaken for the HTTP
execution path.

### 3. Normalization

Normalize both traces into PHP-level edges:

```text
caller symbol -> callee symbol | file:line | kind | context
```

`kind` is one of: include/autoload, function, instance method, static method,
constructor, reflection, callable, dynamic evaluation, builtin, property/array,
exception, or web/runtime bridge. Preserve source order and recursion depth.

Dynamic strings, callable targets, reflection targets, and runtime-selected
classes are not collapsed into a wildcard. Record the observed value and its
origin, then separately mark whether the compiler has a sound conservative
fallback for unobserved values.

## Ledger

Maintain one row per normalized edge with these columns:

| Column | Meaning |
| --- | --- |
| Order/depth | Native request order and call depth |
| Edge | Canonical caller and callee PHP symbols |
| Source | Physical file and line from the native trace/source map |
| Context | AOT, eval, bridge, reflection, callable, or web worker |
| Native proof | Trace event(s) and observed operands |
| Elephc path | Resolver/type/EIR/codegen/runtime path or diagnostic |
| Classification | Supported, divergent, missing, conservatively retained, or unobserved |
| Oracle | Exact `php -n` fixture/assertion required for the edge |
| Elephc test | Focused test name/path after implementation |
| Batch | Closed implementation batch identifier |
| Status | Open, implemented, verified, or rejected with evidence |

The ledger is complete only when every native edge is `verified` or has a
documented, PHP-observable reason why it cannot occur in the compiled request.
“Compiled successfully”, an emitted declaration, or a static source match is
not verification.

## TDD and batching rules

1. Convert every `missing` or `divergent` edge into the smallest standalone PHP
   program that retains its real language shape. Capture `php -n` stdout,
   stderr, exit status, and observable state before coding.
2. Add the failing Elephc regression in the narrowest existing test module.
   Keep dynamic-eval, reflection, ABI, ownership, and web-worker behavior in
   their respective test surfaces when that improves diagnosis.
3. Batch only edges with one shared semantic root cause. A batch may contain
   multiple functions/classes when they share the same parser, resolver,
   lowering, ABI, runtime, or ownership rule; it must not join unrelated
   failures merely because they were adjacent in the trace.
4. Implement the generic PHP behavior, including all supported targets. Never
   add an application/framework/tool-specific conditional, stub, or fixture
   mutation.
5. Run the oracle regression, focused Elephc tests, `git diff --check`, and the
   smallest relevant cross-target assembly/runtime checks before marking a row
   `verified`.
6. Re-run the native/Elephc trace comparison after each batch. New edges extend
   the ledger; they do not invalidate previously verified edges unless their
   semantics changed.

## Current evidence and first closed batch

The current worker reaches container compilation. The previous dynamic hash-capacity
failure is closed: the default 8 MiB worker now exhausts its heap after ordinary
small allocations instead of requesting a pointer-sized capacity. With a diagnostic
32 MiB heap, the next reachable fault is a SIGBUS in `_rt_serialize_object()` while
serializing the generated definition graph. The bad payload has an object header of
zero, which is ownership corruption rather than an unsupported serializer surface.

The reduced oracle showed that `return $parameter` did not retain a refcounted
parameter. The frontend provisionally classified every concrete local load as an
owner; this incorrectly included incoming parameters, so return lowering skipped
its normal borrowed-value acquire. The caller then released a value that the callee
had never acquired. This is independent of application, framework, or serializer.

The first closed batch adds a parameter-slot query to EIR construction and prevents
incoming parameters from being classified as movable local owners. Return lowering
therefore emits `acquire` before returning a refcounted parameter. The permanent
oracles are:

- `test_serialize_returned_object_function_parameter_survives_caller_unset`;
- `test_serialize_returned_object_static_method_parameter_survives_caller_unset`;
- `test_serialize_returned_object_method_parameter_survives_caller_unset`;
- `test_serialize_object_from_array_property_method_result`.

All four pass locally. The last one is the direct reduction of the serializer crash:
an object stored in a method-owned array property survives both an ignored method
result and an `unset()` of its original local binding.

### Dynamic-include declaration retention

The r55 executable validates the first batch end-to-end: it compiles normally in
188.47 seconds to 218 MiB and the prior serializer SIGBUS no longer occurs. The
next HTTP failure is an ordinary uncaught service-resolution exception during
container compilation, not a memory fault.

The opt-in dynamic trace confirms that runtime-included configuration creates
and mutates the objects used to build the container. Separately, a reduced
generic reproducer exposed a bridge rule that must be fixed before attributing
the service exception: a runtime include can create a dynamic object whose
destructor invokes an AOT method. The reachability scanner had treated remaining
`StmtKind::Include` nodes as ordinary path expressions, so it pruned an AOT
method that existed only in the opaque included source. The eval method bridge
then had no dispatch slot and reported a runtime fatal. Literal `eval()` already
widened the same three declaration domains; resolver-preserved include/require
now does so too. A fresh web binary is required to prove whether this closes the
container-service failure.

The closed regression is
`test_dynamic_required_temporary_destructor_mutates_aot_receiver`: a runtime
required class retains an AOT receiver, invokes its `void` method from
`__destruct()`, and publishes its value. It fails before the reachability
correction and passes afterwards. The scanner also has direct unit coverage for
the internal rewritten function and for an unresolved include statement.

The next batch is therefore:

- compile the same frozen entry point through the resolver/EIR inventory only;
- join each native edge to an AOT, dynamic-evaluation, reflection, or runtime path;
- reduce every unclassified or divergent capability signature to a PHP oracle;
- close shared roots in bounded batches, then re-run the ledger before a web build.

### Execution-map specification

The trace is the input to a bounded compatibility campaign, not merely a
post-mortem source. Its current frozen request has 570 distinct symbols and
1,421 distinct caller/callee edges. They are grouped by execution surface
before any implementation work:

| Surface | Distinct symbols | Request calls | Closure rule |
| --- | ---: | ---: | --- |
| Builtins | 115 | 26,582 | Compare each observed call shape with the shared AOT and dynamic-evaluation contracts. |
| Reflection | 30 | 5,715 | Group by metadata family and verify returned values, visibility, ownership, and declaration-file attribution. |
| Instance methods | 289 | 3,516 | Group by lowering boundary: direct AOT, dynamic invocation, callback, generated declaration, or bridge dispatch. |
| Static methods | 68 | 126 | Verify canonical resolution, late binding, closures, and generated declarations independently of source provenance. |
| Constructors | 60 | 95 | Verify allocation, defaults, typed properties, and inherited initializers. |
| Includes/autoload | 4 forms | 215 | Verify generic path resolution and declaration registration; do not infer behavior from any package or layout name. |
| Global functions | 4 | 20 | Verify direct, callable, and dynamically resolved forms where observed. |

For each row, derive a **capability signature** from the native event plus the
surrounding source shape: argument passing mode, concrete/boxed value
representation, return/exception behavior, mutation/aliasing, dispatch
context, and observable output. Different source edges sharing a signature
become one implementation batch; different signatures never share a
"convenience" fix.

The map has three closure passes:

1. **Source-to-entry pass:** construct the exact native request DAG, including
   includes, runtime-selected symbols, and reflection targets. Record values
   responsible for each dynamic decision rather than treating them as a
   wildcard.
2. **Compiler-path pass:** map every observed edge to resolver, type checker,
   EIR, codegen, dynamic evaluator, and runtime ownership paths. A compiled
   declaration is not support: the path must have a focused execution test.
3. **Oracle pass:** reduce every missing/divergent capability signature to a
   small program, freeze its native observable result, then add the failing
   Elephc test before a generic correction. Re-run the trace after each closed
   batch and add newly reached edges to the same map.

The current first closed signature is **a concrete associative array crossing
a dynamic boundary, then being reinserted into a Mixed-valued associative
container during a nested by-reference write**. Its regression is
`test_eval_assoc_array_nested_aot_by_ref_write_uses_raw_hash_payload`. The
native result is `changed`; a raw hash pointer stored with the tag reserved for
a boxed Mixed cell is invalid on all applications, so its fix belongs in
generic hash value materialization rather than in a source-specific path.

### Observed standard-function backlog

The 15 observed functions not yet registered by the shared contract are
grouped by semantics, so the campaign can make broad progress without merging
unrelated behavior into one change:

| Batch | Observed functions | Required generic oracle shapes |
| --- | --- | --- |
| Process diagnostic state | `error_reporting`, `set_error_handler`, `restore_error_handler`, `set_exception_handler`, `restore_exception_handler`, `register_shutdown_function`, `error_get_last` | get/set return values; stacked replacement and restoration; callback order; fatal/error state; shutdown order. |
| Runtime configuration | `ini_get`, `ini_set` | missing/current/changed values; previous-value return; request-local isolation. |
| Text comparison | `substr_compare`, `strnatcmp` | offsets and lengths, case-insensitive comparison, out-of-range errors, binary input, and comparator sign only. |
| Query parsing | `parse_str` | by-reference result replacement, PHP key normalization, repeated/nested keys, empty input, and source-order effects. |
| Runtime-cache inquiry | `opcache_is_script_cached` | path coercion and false/no-cache behavior through the standard cache capability, not a named-project rule. |
| Output synchronization | `flush` | output-buffer interaction, return value, and safe no-op behavior where the underlying transport has nothing to flush. |

Every row starts with a frozen native probe of the complete observed call
shape. The implementation must then join the same shared contract in both
backends; merely making the AOT checker accept a name is insufficient.

The trace source locations narrow those probes further without becoming
application-specific production behavior:

| Capability family | Observed language shape to preserve in its oracle |
| --- | --- |
| Error mask | Nested `error_reporting(error_reporting() \| mask)` with a `finally` restore. |
| Error handler stack | Capture previous callback, install an object/array or closure callback, invoke it through a warning path, then restore in `finally`. |
| Exception handler stack | Replace, inspect, temporarily restore, and reinstate a callable handler. |
| Shutdown callbacks | Register a callable and a nested registration; preserve registration and execution order. |
| INI state | Read current key, set a string/int/bool value, use the previous result for restoration, and verify request-local reset. |
| Query parser | Parse an empty-or-query-derived string into a by-reference result hash, including normalized/nested keys. |
| Comparator callback | Use `strnatcmp` through `uksort` and assert deterministic key order. |
| Cache predicate | Guard a file inclusion with a path-based cache inquiry that safely returns false when no cache capability exists. |
| Flush | Close output buffers then flush an already-sent response stream; return normally without injecting bytes. |

### Current root-cause tranche: hash capacity boundary

The static `NodeBuilder` construction probe has identical native and Elephc
output and compiles in 10.32 seconds. It rules out that construction and append
sequence in ordinary AOT code. The web worker instead reaches the dynamic
bridge and then requests a 284 GiB hash allocation while only about 13 MiB is
live. The prior r44 worker's allocator return address resolves inside
`__rt_hash_new`; the request has the shape `64 * pointer_like_value + 48`,
which means a pointer reached the hash-capacity ABI position.

The next isolated binary records the caller entering `__rt_hash_new` before its
nested allocator call, then includes that origin in a terminal heap diagnostic.
This is generic ABI provenance, implemented for AArch64 and x86_64 and covered
by:

- `hash_new_captures_call_origin_for_heap_exhaustion_diagnostics`;
- `heap_exhaustion_reports_allocation_counters`;
- `test_heap_hash_origin_slot_is_declared_for_each_object_format`.

Only after that caller is resolved may a PHP-oracle regression and its generic
ABI/ownership fix be written. The static probe is retained as negative evidence
that the root is specific to the dynamic boundary.

## Functional and performance gates

Functional work comes first. Do not use a larger heap as a substitute for a
semantic fix. Do not claim completion until an isolated compiled executable
returns the required HTTP response and its worker logs contain no fatal/error.

After the ledger's functional closure, profile the same frozen command with
phase timings, peak RSS, assembly bytes/lines, link duration, and executable
size. Optimization work must preserve the ledger's oracle tests and web
acceptance, and should target the product goal of approximately 1–2 minutes and
an executable below 250 MiB.
