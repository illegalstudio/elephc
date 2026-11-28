# Float-seeded and non-literal-int-seeded `array_reduce()` sums

## Gap

The previous block fused `array_reduce($ints, fn($c, $v) => $c + $v, C)` only when
`C` was an integer literal. A raw-int local/parameter seed and any float seed still
took the generic boxed reduce (one Mixed cell plus one closure call per element).

## Change

- The initial value is classified *before* lowering, so an unsupported shape falls
  back without ever lowering (and evaluating) it twice:
  - integer literal → `IntLiteral`;
  - a local/parameter whose flow type is a raw `Int` → `IntValue`;
  - float literal → `FloatLiteral`;
  - a local/parameter whose flow type is a raw `Float` → `FloatValue`;
  - reference-bound locals and borrowed element refs are rejected.
- `IntLiteral(0)` keeps `ArraySum`. Other raw ints (literal or value) lower to
  `ArrayReduceSumSeed` → `__rt_array_sum_int_i64`.
- Raw floats (literal or value) lower to the new `ArrayReduceSumFloatSeed` →
  `__rt_array_sum_float_i64`. The helper takes raw IEEE-754 seed bits in `x3`/`rcx`,
  starts the accumulator in double mode, converts each int element with
  `scvtf`/`cvtsi2sd`, adds in source order, and boxes only the final float. An empty
  range returns the seed as a float cell (`-0.0` stays `float(-0)`).
- The float entry is a self-contained sibling of the int reducer entries, so ELF/
  Mach-O dead stripping can still remove unused entries. Both seeded reducers are
  `BuiltinResultOwnership::Fresh`.

## Validation

- `tests/codegen/optimizer/reduce_sum_seed.rs`: 12 passed. New cases cover int
  locals, typed int parameters, float literals, raw-float locals and parameters,
  empty ranges, `-0.0`, `PHP_INT_MAX`, unproven initials (string/bool/expression/
  reference) evaluated exactly once, heap-debug cleanliness, `--ir-opt` parity, and
  all-target assembly for both helpers.
- PHP parity fixtures: int-variable seeds `16|11|-1|integer:9223372036854775800`;
  float seeds `7.5|double|1.5|double|8.5|double|float(-0)` plus
  `double:9.2233720368548E+18`; fallbacks `16|7|16|1|11|12`.
- x86_64 debug cross + qemu matched all three fixtures; the runtime object
  disassembly shows `jmp __rt_array_sum_float_i64_float_init` and the empty branch
  boxing tag 2.
- `optimizer::`: 886 passed / 0 failed / 1 ignored.
- `arrays::`: 1301 passed / 0 failed / 1 ignored.
- Release x86_64 cross + qemu matched PHP for the three fixtures, both benchmark
  mains (`25257500`, `25300000`), and the runtime object's float entry.
- EIR keeps the seed raw: `const_f64 1.5` (LICM-hoisted) feeds
  `runtime.array_reduce_sum_float_seed` directly, and the int-local seed load feeds
  `runtime.array_reduce_sum_seed`; no Mixed clone/box wraps either seed. Assembly
  passes the bits with `fmov x3, d8` / `mov x3, x22` before the helper call.

## Benchmarks

- `benchmark/cases/native/array_reduce_float_seed`: float literal 1.5, 100 ints,
  5000 reductions; PHP and C both print `25257500`.
- `benchmark/cases/native/array_reduce_variable_seed`: int local seed, same shape;
  PHP and C both print `25300000`.

Formal harness (5 runs + 2 warmups):

| case | elephc | PHP | PHP+JIT | C `-O2` |
|---|---:|---:|---:|---:|
| float seed 1.5 | **1.068 ms** | 12.492 | 12.331 | 0.788 |
| int local seed 10 | **0.839 ms** | 12.407 | 13.097 | 0.623 |
| int literal seed 10 | 2.071 | 12.482 | 13.269 | 1.786 |

Same-compiler A/B (`$values` fused vs `$holder[0]` generic boxed, in-process
`hrtime`, 5000 reductions):

- Float: raw 0.355917 / 0.357709 / 0.361875 / 0.364833 / 0.376000 / 0.376541 /
  0.378917 ms; generic 45.443292 / 46.934750 / 47.836542 / 47.917083 / 47.954375 /
  48.261417 / 48.690834 ms. Both printed `25257500` (median about 47.917 -> 0.365
  ms, about 131x).
- Int local: raw 0.193958 / 0.195667 / 0.204208 / 0.214084 / 0.218833 / 0.228125 /
  0.229667 ms; generic 47.398417 / 47.753417 / 47.926625 / 48.101625 / 48.313667 /
  48.724375 / 48.885500 ms. Both printed `25300000` (median about 48.102 -> 0.214
  ms, about 225x).
- PHP on the same fixtures: float 7.047 ms / JIT 6.712 ms; int 6.753 ms / JIT
  6.593 ms.
