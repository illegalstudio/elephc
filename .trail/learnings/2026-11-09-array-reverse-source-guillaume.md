# Default array_reverse() as a direct stored-map source

## Gap

The stored-map fusion proved array literals, `range()`, key-renumbering
`array_slice()`, `array_values()` and zero-offset key-preserving slices. A default
`array_reverse($packed)` stayed on the generic `array_map()` path even though PHP
renumbers its keys and returns the same packed element type.

## Change

- `lowering_packed_int_source` and `is_packed_int_source_candidate` accept
  `array_reverse($source)` when the second argument is absent or literal `false`
  and `$source` is itself a proven packed `array<int>` source.
- The proof is recursive through slices, values and reversals.
- `array_reverse(..., true)` still keeps the hash result and stays generic; its
  keys are the source's integer keys in reverse order.

## Validation

- `test_stored_numeric_map_array_reverse_source`: integer, float-scale and
  float-division maps over array literals, variables, slices and nested reversals
  emit the fused kernels and match PHP; the key-preserving reverse keeps one
  generic map.
- The direct-source heap-debug and `--ir-opt` parity tests include an
  `array_reverse()` source.
- The all-target direct-source test emits the square kernel for an
  `array_reverse(range(...))` source on all five supported targets.
- `optimizer::`: 922 passed / 0 failed / 1 ignored.
- The arrays suite is unchanged from the previous block (1306 passed / 1
  ignored); this change only touches the stored-map source proof.
- x86_64 release qemu matches PHP for the benchmark, and the assembly emits
  `__rt_array_map_numeric` with no generic `__rt_array_map`.

## Benchmark

`benchmark/cases/native/array_map_array_reverse_source`:
`array_reverse(range(1, 100))`, `$v * 2`, 5000 maps; PHP and C both print
`50500000`.

Formal harness (5 runs + 2 warmups):

| case | elephc | PHP | PHP+JIT | C `-O2` |
|---|---:|---:|---:|---:|
| default `array_reverse()` source | **1.119 ms** | 12.565 | 13.042 | 0.368 |
| `array_values()` baseline | 0.932 ms | 12.228 | 12.340 | 0.381 |

Interleaved A/B on one core (in-process `hrtime`):

- parent `5a5e7c95da`: 17.300 / 17.378 / 17.498 / 17.677 / 18.040 / 18.158 ms,
  median about 17.7 ms.
- new `array_reverse()` source: 0.782 / 0.815 / 0.824 / 0.830 / 0.845 / 0.855 /
  0.903 ms, median about 0.83 ms.
- Both printed `50500000`; the new EIR uses `runtime.array_map_numeric`, the
  parent the generic `runtime.array_map`.
- PHP 8.146 ms / JIT 8.016 ms in the same batch.
