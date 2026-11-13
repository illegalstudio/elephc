# Two-operand array_merge() as a direct stored-map source

## Gap

The stored-map fusion proved array literals, `range()`, key-renumbering
`array_slice()`, `array_values()`, zero-offset key-preserving slices, default
`array_reverse()` and `array_fill()`. A direct `array_merge($left, $right)` of two
packed-int arrays stayed on the generic `array_map()` path even though the checker
keeps `array<int>` and the lowering concatenates packed slots.

## Change

- `lowering_packed_int_source` and `is_packed_int_source_candidate` accept a
  two-operand `array_merge()` when both operands are themselves proven packed-int
  sources.
- The proof is recursive through slices, values, reversals, fills and merges.
- A dynamic/boxed operand or an associative result is not a packed-int source and
  falls back to the generic map.

## Validation

- `test_stored_numeric_map_array_merge_source`: variable, array-literal and nested
  slice/reverse operands feed integer, float-scale and float-division kernels and
  match PHP.
- The direct-source heap-debug, all-target and `--ir-opt` parity tests include an
  `array_merge()` source; the all-target assembly checks the square-affine kernel.
- `optimizer::`: 924 passed / 0 failed / 1 ignored. The arrays suite is unchanged
  from the previous block (1306 passed / 1 ignored).
- x86_64 release qemu matches PHP and the assembly emits the numeric kernel.

## Benchmark

`benchmark/cases/native/array_map_array_merge_source`:
`array_merge(range(1, 50), range(51, 100))`, `$v * 2`, 5000 maps; PHP and C both
print `50500000`.

Formal harness (5 runs + 2 warmups):

| case | elephc | PHP | PHP+JIT | C `-O2` |
|---|---:|---:|---:|---:|
| `array_merge()` source | **1.063 ms** | 11.808 | 12.249 | 0.207 |

Interleaved A/B on one core (in-process `hrtime`):

- parent `d7f68a51eb`: 15.034 / 15.318 / 15.696 / 15.925 / 16.008 / 16.091 /
  16.278 ms, median about 15.9 ms.
- new `array_merge()` source: 0.855 / 0.890 / 0.898 / 0.930 / 0.977 / 1.008 ms,
  median about 0.93 ms.
- Both printed `50500000`; the new EIR uses `runtime.array_map_numeric`, the
  parent the generic `runtime.array_map`.
- PHP 7.420 ms in the same batch.
