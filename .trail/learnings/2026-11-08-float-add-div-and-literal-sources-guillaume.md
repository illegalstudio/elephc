# Stored float add/sub/div maps and array-literal sources

## Gap

The stored-map sink covered `$v * F` and `$v * F + G`, but `$v + F`, `$v - F`, and
`$v / F` still used the generic boxed map. The marker also required a source local,
so `array_map(..., [1, 2, 3])` stayed generic even though the literal is a packed
`array<int>` at run time.

## Change

- `StoredNumericMapShape` gained `FloatDiv { divisor }`. Float add/sub parse into
  the existing `FloatAffine` kernel with `scale = 1.0`, which is exactly
  `double($v) + offset` (multiplication by 1.0 is exact).
- `$v / F` accepts only a float literal divisor and rejects `0.0` / `-0.0`.
  Integer divisors are rejected because PHP returns an int for evenly divisible int
  operands; the generic map keeps that per-element distinction and the zero-divisor
  diagnostic.
- `__rt_array_map_float_div` shares the float map emitter with `..._float_scale`
  (one parameterized kernel per target, `fdiv`/`divsd` instead of `fmul`/`mulsd`).
- The marker accepts non-empty all-int array literals as packed `array<int>`
  sources. The lowering releases the literal temporary after the map call, so
  `--heap-debug` stays clean. Other non-variable sources remain generic.
- All stored map kernels are now `BuiltinResultOwnership::Fresh`: each allocates a
  new destination and never returns source storage.

## Validation

- `tests/codegen/optimizer/stored_numeric_map.rs`: 43 passed (7 new). New cases
  cover float add/sub in both orders, negative zero, empty ranges, float division,
  `INF` divisors, zero-divisor errors staying generic, int-divisor fallback,
  array-literal sources for every kernel, element-write fallback, heap-debug
  cleanliness, `--ir-opt` parity, and all-target assembly.
- PHP parity fixtures match byte-for-byte.
- A pre-existing generic-path issue is noted but untouched: an untyped
  `fn($v) => $v / 2` callback boxes an exact int division as a double. The old
  compiler reproduces it; the float divisor path is separate.
- x86_64 debug cross + qemu matched the add/div/literal fixtures; heap debug is
  clean after the literal-source release.
- `optimizer::`: 899 passed / 0 failed / 1 ignored.
- `arrays::`: 1301 passed / 0 failed / 1 ignored.
- Release x86_64 cross + qemu matched the add, div, literal and heap fixtures and
  both benchmark mains (`26000000`, `12625000`); the runtime object shows the
  `divsd` kernel.

## Benchmarks

- `benchmark/cases/native/array_map_float_add`: `$v + 1.5`, 5000 maps of 100 ints;
  PHP and C both print `26000000`.
- `benchmark/cases/native/array_map_float_div`: `$v / 2.0`; PHP and C both print
  `12625000`.

Formal harness (5 runs + 2 warmups):

| case | elephc | PHP | PHP+JIT | C `-O2` |
|---|---:|---:|---:|---:|
| `$v + 1.5` | **1.213 ms** | 10.403 | 11.309 | 0.851 |
| `$v / 2.0` | **1.166 ms** | 11.396 | 12.182 | 0.699 |
| `$v * 1.5` (neighbour) | 0.916 | 10.287 | 10.682 | 0.981 |
| `$v * 1.5 + 2.5` (neighbour) | 1.221 | 10.644 | 10.625 | 0.831 |

Same-compiler A/B (`$values` fused vs `$holder[0]` generic boxed, in-process
`hrtime`, 5000 maps of 100 ints):

- Add: raw 0.644541 / 0.649417 / 0.658750 / 0.661458 / 0.662208 / 0.674709 /
  0.684208 ms; generic 13.789458 / 14.097417 / 14.311125 / 14.359917 / 14.392959 /
  14.734375 / 16.482917 ms. Both printed `26000000` (median about 14.360 -> 0.661
  ms, about 21.7x).
- Divide: raw 0.600708 / 0.603375 / 0.609000 / 0.609250 / 0.609583 / 0.626958 /
  0.643125 ms; generic 14.970666 / 14.984084 / 15.046875 / 15.112625 / 15.235333 /
  15.364375 / 15.747750 ms. Both printed `12625000` (median about 15.113 -> 0.609
  ms, about 24.8x).
- Array-literal source (10 ints, map + count): raw 0.215625 / 0.221458 / 0.223750 /
  0.224167 / 0.225375 / 0.228750 / 0.237375 ms; generic 0.914708 / 0.928250 /
  0.955583 / 0.970917 / 0.983291 / 0.984333 / 1.022042 ms. Both printed `50000`
  (median about 0.971 -> 0.224 ms, about 4.3x).
- PHP on the same fixtures: add 6.085 ms / JIT 5.930; divide 6.809 ms / JIT 7.027;
  literal-10 0.831 ms / JIT 0.832.
