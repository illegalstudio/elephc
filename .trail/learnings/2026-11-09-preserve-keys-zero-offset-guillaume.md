# array_slice(preserve_keys: true) at a zero offset

## Gap

The checker always typed a key-preserving slice of an indexed array as
`AssocArray`, because PHP keeps the source integer keys and a dense indexed array
cannot represent a window that starts at a non-zero key. That forced every
`array_slice(..., true)` through `__rt_array_slice_to_hash`, and the stored-map
fusion rejected it.

A literal offset of `0` is the exception: the preserved keys are exactly
`0..n-1`, which is the packed-list layout. The dense representation is exact, so
the slice can stay an indexed array and feed the stored-map kernels.

## Change

- `array_slice`'s checker recognizes a literal integer offset of `0`. A
  `PhpType::Array(elem)` source with `preserve_keys: true` then returns
  `Array(elem)` instead of `AssocArray`; later or dynamic offsets keep the hash
  shape.
- `lower_array_slice` uses the normal packed slice path whenever the result is an
  indexed array, even with `preserve_keys: true`. The packed helper renumbers the
  window, which produces the same keys because the window starts at `0`.
- The stored-map source proof accepts `array_slice(..., 0, ..., true)` and keeps
  rejecting `array_slice(..., offset != 0, ..., true)`, whose result is a hash.
- The checker keeps the hash result type for a `Never`-element generic array and for
  parameters, globals, `static` locals and reference aliases. Those can load through
  boxed storage even when their checked type is a concrete list, and the boxed slice
  lowering builds a hash. The stored-map proof rejects the same bindings before any
  emission, so it cannot assume packed slots there.

## Validation

- `test_array_slice_preserve_keys_zero_offset_stays_list`: values, explicit
  `null` length, negative length and `array_is_list()`.
- `test_array_slice_preserve_keys_zero_offset_payloads`: string, mixed and nested
  refcounted payloads stay on the packed path.
- The existing non-zero, negative and `null`-length key-preserving tests still
  require the hash path.
- `test_stored_numeric_map_preserve_keys_at_zero_offset`: integer, float-scale and
  float-division maps over zero-offset key-preserving slices emit the fused
  kernels and match PHP.
- The direct-source heap-debug, all-target and `--ir-opt` parity tests include the
  zero-offset preserve-keys source.
- The generic fallback test still keeps `array_slice($values, 2, null, true)`
  generic.
- `test_array_slice_preserve_keys_boxed_parameter_keeps_hash_type`: an untyped
  parameter called with a list or a hash does not crash and returns PHP's values.
- `test_stored_numeric_map_preserve_keys_boxed_binding_falls_back`: a parameter or
  global source keeps the generic map, and the module IR contains neither the
  numeric kernel nor a packed-source assumption.
- `arrays::`: 1306 passed / 0 failed / 1 ignored.
- `optimizer::`: 921 passed / 0 failed / 1 ignored.
- The final formal benchmark and x86_64 release checks are recorded below.
- x86_64 release qemu matches PHP for the zero-offset slice keys and the fused
  benchmark, and the assembly emits `__rt_array_map_float_affine` with no generic
  `__rt_array_map`.

## Benchmark

`benchmark/cases/native/array_map_preserve_keys_source`: `array_slice(range(1,
100), 0, 80, true)`, `$v + 1.5`, 5000 maps; PHP and C both print `16800000`.

Formal harness (5 runs + 2 warmups):

| case | elephc | PHP | PHP+JIT | C `-O2` |
|---|---:|---:|---:|---:|
| preserve_keys at offset 0 | **1.532 ms** | 13.911 | 14.982 | 0.768 |
| default key-renumbering slice | 3.205 ms | 17.875 | 19.737 | 2.548 |

The first formal run was on a heavier machine (1.780 ms); the final release run
after the boxed-binding guard measured 1.532 ms. The generated benchmark path is
unchanged: it uses a plain local source.

Interleaved A/B on one core (in-process `hrtime`):

- parent `53f1c79b42`: 43.052 / 43.772 / 53.857 / 84.812 / 85.460 / 93.032 /
  120.563 ms, median about 84.8 ms.
- new zero-offset preserve-keys source: 1.167 / 1.178 / 1.195 / 1.318 / 1.413 /
  1.486 / 7.277 (one loaded outlier) ms, median about 1.318 ms.
- Both printed `16800000`; the new EIR uses `runtime.array_map_float_affine`,
  the parent the generic `runtime.array_map`.
- PHP 40.804 ms / JIT 9.634 ms in the same batch.
