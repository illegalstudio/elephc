# Fusing stored int-division maps into their sum

## Gap

`array_map_int_div_mixed` stores `$mapped = array_map(fn($v) => $v / 2, $values)`
inside the loop and immediately reduces it with `array_sum($mapped)`. Lowering
already avoided the generic callback and built the raw-int division kernel, but it
still materialized a 100-element Mixed map on every one of the 5,000 iterations
and then walked it again for the sum. The body comparison stayed at roughly half of
FrankenPHP's throughput.

## Change

- Added `src/ir_passes/array_map_sum_div_fusion.rs`, an EIR fixed-point pass that
  recognizes the frozen pair:
  - one store into a `PhpLocal` mapped slot from `ArrayMapIntDiv`;
  - exactly one load of that slot, consumed by `array_sum()`;
  - only `ReleaseLocalSlot` cleanup mentions besides the store and load;
  - a non-escaping packed `array<int>` source local whose loaded value reaches no
    other address-taking consumer;
  - a nonzero `ConstI64` divisor;
  - every map/sum scaffold instruction in the call block in source order, with no
    source-slot write between the map store and the sum load.
- The pass retargets the `array_sum()` call to `ArrayMapSumIntDiv` with the
  original source SSA value and divisor, then neutralizes the map call, map
  acquire/store/load, its releases and the hidden temporary-owner scaffold.
- Added `RuntimeFnId::ArrayMapSumIntDiv` with a two-operand descriptor, a fresh
  boxed Mixed result and `reads_heap | alloc_heap` intrinsic effects. It is listed
  as a value-only consumer for by-reference escape analysis.
- Added `__rt_array_map_sum_int_div` for AArch64 and x86_64 in
  `src/codegen_support/runtime/arrays/array_map_sum_int_div.rs`. The kernel walks
  the packed payload once without allocating an intermediate array:
  - exact integer division keeps an integer quotient;
  - the first non-exact quotient, or `PHP_INT_MIN / -1`, converts the accumulated
    integer prefix to double and switches the accumulator to double;
  - integer-sum overflow converts the prefix and the overflowing term to double,
    then stays in double mode;
  - empty input returns integer zero.
- A zero divisor, dynamic divisor, parameter source, observed map, reassigned map
  local, source write between the map and the sum, or any unexpected scaffold keeps
  the generic materialized map and PHP's exact result/diagnostic.

## Tests

- `tests/codegen/optimizer/array_map_sum_div_fusion.rs` — 15 end-to-end tests:
  - stored-map fusion and EIR target selection;
  - exact quotients staying `int`;
  - non-exact and negative quotients;
  - `PHP_INT_MIN / -1` promotion;
  - integer-sum overflow promotion;
  - dynamic divisor fallback;
  - observed map fallback;
  - source write between map and sum fallback;
  - reassigned mapped-local fallback;
  - zero divisor keeping `DivisionByZeroError`;
  - parameter-source fallback;
  - two independent stored maps fusing independently;
  - `--ir-opt=on/off` output parity and EIR presence;
  - heap-debug cleanliness;
  - per-target staticlib compilation on all five supported targets.
- `array_map_sum_int_div_emits_all_targets` in the runtime emitter tests pins the
  AArch64 and x86_64 division/promotion instruction sequences on every target.
- `array_map_sum_int_div_is_a_two_operand_fresh_boxed_kernel` pins the runtime
  descriptor, EIR name, arity, intrinsic effects and fresh ownership.
- Focused runs are recorded after the implementation; the full optimizer suite is
  run before committing.

## Benchmark

Body-only medians from `benchmark/frankenphp_body.py` (7 runs + 3 warmups):

| case | before | after | FrankenPHP | FP+JIT |
|---|---:|---:|---:|---:|
| `array_map_int_div_mixed` | 23.452 | **0.672** | 11.093 | 11.409 |
| `array_map_int_div` | — | **0.598** | 11.791 | 11.427 |

The fused case is **16.5x** ahead of FrankenPHP and **17.0x** ahead of FrankenPHP+JIT
in this run. The remaining loop performs one packed load, one bit mask, one
division with remainder, and the accumulator update; the 100-element Mixed map and
its 500,000 boxed cells per run are gone.
