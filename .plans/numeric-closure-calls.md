# Specialize proven numeric closure arguments

- [x] Implement exact scalar call-site gates and a bounded private expression family.
- [x] Add ownership and source-identity preservation for implementation variants.
- [x] Validate actual PHP, IR on/off, heap cleanup, scalar/stack ABI and every target emitter.
- [x] Verify float fixtures on the exact discovery baseline before interpreting candidate failures.
- [ ] Measure final release binaries with matched native/PHP kernels and noise gates.
- [ ] Complete independent review, publish the focused PR, and verify review/CI results.

Discovery baseline: main `51d65ce7cb98809459d0e1894f1e82d1f58cb341`.
Integrated base: main `bf6bd73334a2dd53c3faefc727c555a953845e03`.
Code Journal plan: `114c1388`.

Already-direct calls to small numeric closures can still box scalar arguments and
clone them into callee shadows. A module phase recognizes closed one-block numeric
expression graphs and emits private entry variants for exact integer/float argument
tuples. All seven function-like host tables participate, including methods and generated wrappers.
Lowering owns closure identity and per-call-site admission. Other calls and
original callable descriptors continue to target the original implementation.

The initial family accepts scalar by-value parameters/captures, constants and checked
add/subtract/multiply. Bool/null, references, user variadics, calls, diagnostics,
generators, typed-return conversion boundaries, globals and unknown effects fail
closed. Integer overflow keeps PHP float promotion. Expression nodes are shared and
every owned intermediate is retired after the independent final result is ready.

Private variants retain the original PHP source name, lexical scope and arithmetic
source spans. Instrumentation ids and call counters aggregate entries by that source
identity; probe tables and backtrace/source markers identify the physical entry with
the original PHP function. Private variants remain absent from public function tables
and cannot change the original descriptor ABI. Literal and dynamic `function_exists()`
lookups exclude these private entries, without reserving their name prefix from users.

This family deliberately excludes the generated Array argument collector used by
func-args/backtrace support, including the eval-enabled mixed diagnostic. Its measured
standalone closure gain must not be credited to that mixed program. Supporting an
unused collector while retaining its physical ABI is a separate, unmeasured extension.


Acceptance includes per-site null/bool negatives, capture snapshots, integer overflow,
float edges, typed-return fallback, repeated expression nodes, mixed register banks,
stack arguments, constant allocation accounting, monitoring identity and all supported
target emitters. Local execution covers Linux x86_64; the full executable target matrix
remains a CI requirement.
