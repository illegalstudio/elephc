# array_values() as a direct stored-map source

## Gap

The stored-map fusion proved array literals, `range()`, and key-renumbering
`array_slice()` sources, but `array_values($packed)` stayed on the generic
`array_map()` path even though it aliases an indexed array unchanged.

## Change

- `lowering_packed_int_source` and `is_packed_int_source_candidate` accept
  `array_values($source)` when `$source` is itself a proven packed `array<int>`
  source. The proof is recursive through slices and values.
- The normal `array_values()` lowering already aliases an indexed array with an
  incref, so the lowered result stays `array<int>` and the stored-map kernel can
  read its packed slots.
- No PHP-visible behavior changes: the checker still types the original call,
  and a failed proof falls back to the generic map.

## Validation

- `test_stored_numeric_map_array_values_source`: integer, float-scale and
  float-division maps over `array_values()` sources emit the fused kernels and
  match PHP.
- The direct-source heap-debug test now includes an `array_values()` source and
  stays leak-free.
- The all-target direct-source test emits the numeric kernel for an
  `array_values(range(...))` source on all five supported targets.
- `optimizer::`: 919 passed / 0 failed / 1 ignored. The arrays suite is
  unchanged from the previous block (1303 passed / 1 ignored); this change only
  touches the stored-map source proof.
- x86_64 release qemu output matches PHP, and the assembly emits
  `__rt_array_map_numeric` with no generic `__rt_array_map`.

## Benchmark

`benchmark/cases/native/array_map_array_values_source`: `array_values(range(1,
100))`, `$v * 2`, 5000 maps; PHP and C both print `50500000`.

Formal harness (5 runs + 2 warmups):

| case | elephc | PHP | PHP+JIT | C `-O2` |
|---|---:|---:|---:|---:|
| `array_values()` source | **1.537 ms** | 26.313 | 35.950 | 0.779 |
| direct `range()` baseline | 1.807 ms | 26.426 | 22.999 | 0.889 |

Interleaved A/B on one core (in-process `hrtime`):

- parent `fefd44a948`: 27.784 / 28.528 / 39.393 / 76.604 / 96.858 / 115.610 /
  221.232 ms, median about 76.6 ms.
- new `array_values()` source: 1.002 / 1.038 / 1.075 / 1.100 / 1.263 / 2.858 ms,
  median about 1.075 ms.
- Both printed `50500000`; the new assembly uses `runtime.array_map_numeric`
  while the parent uses the generic `runtime.array_map`.
- PHP 39.897 ms / JIT 11.514 ms in the same batch.
