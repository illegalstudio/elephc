# array_fill(0, $n, <int>) as a direct stored-map source

## Gap

The stored-map fusion proved array literals, `range()`, key-renumbering
`array_slice()`, `array_values()`, zero-offset key-preserving slices and default
`array_reverse()`. A direct `array_fill(0, $n, <int>)` source stayed on the generic
`array_map()` path even though the checker types it as a packed `array<int>` and the
lowering builds a dense indexed array.

## Change

- `lowering_packed_int_source` and `is_packed_int_source_candidate` accept
  `array_fill(0, $count, <int literal>)`. The count may be a runtime value; the
  start must be literal `0` so the result is a list, and the fill value must be an
  integer literal so the element type is statically `int`.
- The proof is recursive through slices, values, reversals and fills.
- A non-zero start keeps the associative result, and a non-int fill keeps its own
  element type; neither is a packed-int source.

## Validation

- `test_stored_numeric_map_array_fill_source`: literal count and runtime count
  fills feed integer and float-scale kernels, and a reversed fill feeds the
  numeric kernel.
- The direct-source heap-debug, all-target and `--ir-opt` parity tests include an
  `array_fill()` source; the all-target assembly checks the int-division kernel.
- `optimizer::`: 923 passed / 0 failed / 1 ignored. The arrays suite is
  unchanged from the previous block (1306 passed / 1 ignored); this change only
  touches the stored-map source proof.
- x86_64 release qemu matches PHP for both the literal-count benchmark and the
  runtime-count A/B fixture; the assembly emits `__rt_array_map_numeric` with no
  generic `__rt_array_map`.

## Benchmark

`benchmark/cases/native/array_map_array_fill_source`:
`array_fill(0, 100, 2)`, `$v * 3`, 5000 maps; PHP and C both print `3000000`.

Formal harness (5 runs + 2 warmups):

| case | elephc | PHP | PHP+JIT | C `-O2` |
|---|---:|---:|---:|---:|
| `array_fill()` source | **1.581 ms** | 16.650 | 17.284 | 0.375 |
| `array_reverse()` baseline | 1.382 ms | 17.616 | 17.755 | 0.525 |

Interleaved A/B on one core (in-process `hrtime`, runtime count):

- parent `8438a9c8d3`: 21.781 / 21.786 / 22.710 / 22.743 / 22.996 / 23.924 /
  25.230 ms, median about 22.7 ms.
- new `array_fill()` source: 1.106 / 1.113 / 1.121 / 1.231 / 1.278 / 1.106 ms,
  median about 1.12 ms.
- Both printed `3000000`; the new EIR uses `runtime.array_map_numeric`, the
  parent the generic `runtime.array_map`.
- PHP 11.164 ms / JIT 9.828 ms in the same batch.
