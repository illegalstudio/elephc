# Function-local stored maps via deferred source proof

## Gap

The stored-map marker proved a variable source's `array<int>` type from the
checker's `TypeEnv`. In user functions that environment only contains parameter
types, so `$values = range(1, 10); array_map(..., $values)` stayed generic even
though lowering and the EIR already knew the local was a packed `array<int>`.

## Change

- The marker no longer requires the source type. It still proves the callback
  shape, destination neutrality, array-literal sources, and rejects reference
  aliases.
- `lower_stored_numeric_map` proves the variable's flow-sensitive
  `ctx.local_type(...) == array<int>` at the actual call site, before emitting
  anything. This uses the lowering context's live type facts, so reassignments,
  merged branches, `range(..., step)`, and array literals all work.
- If the proof fails (for example an `array<mixed>` parameter or an `array<float>`
  local), `lower_function_call` now falls back to the ordinary `array_map()`
  lowering instead of asserting on the marker. The marker is only an
  optimization; it no longer has to be right about the source type.
- Array literal sources keep their static proof, and their lowered temporary is
  still released after the map call.

## Validation

- `tests/codegen/optimizer/stored_numeric_map.rs`: 49 passed (6 new). New cases
  cover function-local `range`/literal sources, reassignment, merged int branches,
  stepped ranges, mixed-source fallback, reference-alias fallback, heap-debug
  cleanliness, `--ir-opt` parity, and all-target assembly.
- PHP parity fixtures match byte-for-byte.
- Heap debug reports `allocs == frees`, `live_blocks=0`.
- `optimizer::`: 905 passed / 0 failed / 1 ignored.
- `arrays::`: 1301 passed / 0 failed / 1 ignored.
- Release x86_64 cross + qemu matched the function-local, reassignment, branch,
  stepped-range, mixed-source fallback, alias fallback, and benchmark fixtures.

## Benchmark

`benchmark/cases/native/array_map_function_local`: a function-local `range(1,100)`
source mapped with `$v + 1.5` and summed 5000 times; PHP and C both print
`26000000`.

- Formal harness (5 runs + 2 warmups): elephc **4.476 ms** vs PHP 11.162,
  PHP+JIT 11.724, C `-O2` 1.005. The neighbouring main-level `$v + 1.5` case
  measured 2.337 ms; the formal number includes process startup.
- Interleaved A/B (in-process `hrtime`, 5000 maps of 100 ints):
  - parent compiler `634fe8444c` (function-local source stays generic):
    15.137500 / 15.168125 / 15.297667 / 15.323875 / 15.515625 / 15.520583 /
    15.542500 ms, median about 15.324.
  - new compiler, same source fused: 0.639792 / 0.651584 / 0.656708 / 0.657084 /
    0.666000 / 0.669792 / 0.673667 ms, median about 0.657.
  - new compiler, same-source generic baseline (`$holder[0]`): 14.431208 /
    14.505084 / 14.652959 / 14.784958 / 14.839916 / 14.985959 / 16.195375 ms,
    median about 14.785.
  - All variants printed `26000000`; fused is about 23.3x faster than the parent
    compiler and about 22.5x faster than the same-compiler generic fallback.
  - EIR confirms the parent and same-compiler generic baseline use `runtime.array_map`; the new
    function-local path uses one `runtime.array_map_float_affine`.
  - PHP 6.171 ms, PHP+JIT 6.032 ms on the same fixture.
