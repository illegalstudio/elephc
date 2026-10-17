# Complete BCMath with BcMath\Number

- [x] Implement and cross-check the immutable decimal value semantics and scale rules.
- [x] Expose the internal class, constructor, properties, methods, and serialization.
- [x] Add a typed internal-class operator contract and EIR lowering, including replacement operations and casts.
- [x] Align dynamic eval and native runtime dispatch, preserving identity and ownership.
- [x] Audit Number independence from the procedural scale and document existing INI limitations.
- [x] Add focused regression, ownership, optimizer, error, and supported-target checks plus an example.
- [x] Regenerate and audit builtin/module compatibility documentation.

## Scope and references

The procedural decimal engine already implements all 14 BCMath functions. This work
adds PHP 8.4's final readonly `BcMath\Number` class and the typed internal object
operator dispatch tracked by https://github.com/illegalstudio/elephc/issues/701.
The shared symbol report should then cover every BCMath function and class.

The class must preserve input scale, calculate omitted scales independently of
`bcscale()`, return fresh immutable results, support serialization and Stringable,
and keep strict equality as object identity. Arithmetic, value comparisons,
unary operations, compound assignments, and increment/decrement must preserve PHP
evaluation order and release temporary values exactly once. Userland classes do
not acquire operator magic methods.

## Implementation boundaries

Decimal semantics stay in `crates/elephc-bcmath`; shared class metadata describes
the supported operations. Synthetic declarations enter the normal AST to EIR
pipeline. Existing target-aware BCMath ABI shapes are reused for every supported
target. Magician consumes the same class/operator contract and decimal engine.
The procedural process scale remains shared across native and eval calls.
The existing `bcmath.scale` INI integration and procedural `bcround()` integer-mode
interface are outside issue #701 and remain documented limitations. The 100%
module report measures supported PHP symbols, not every configuration behavior.

## Validation

Use the installed PHP 8.5 BCMath implementation as a focused behavioral oracle.
Exercise normal and failing operands, exact and repeating division, scale
preservation, all rounding modes, serialization, identity, readonly enforcement,
dynamic calls, source-order effects, and owned temporaries. Run focused host or
resource-limited Linux tests, target emitter checks, builtin parity/documentation
gates, the warning-free build, and diff hygiene. Leave complete matrix execution
to CI.

## Verified results

- 30 BCMath end-to-end tests pass on Linux x86_64, including five heap-debug cases,
  opaque eval, optimizer on/off, and emitter checks for all five supported targets.
- 1,176 decimal engine cases match PHP 8.5.11, with zero differences.
- The decimal crate passes 32 unit tests and six procedural parity fixtures.
- The shared contract passes 53 tests; Magician passes its five BCMath tests.
- Builtin prelude audits, synthetic declaration round trips, class/reflection catalog
  checks, synthetic operator resolution, parser precedence, and error tests pass.
- Generated docs and EIR boundary audits pass. The report now counts BCMath
  functions 14/14 and classes 1/1. Complete cross-target executable coverage remains CI work.
