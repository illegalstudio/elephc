# Caching scalar-only JSON row lengths

## Gap

`json_encode_rows` calls `strlen(json_encode($rows))` on a loop-invariant array of
scalar rows 30,000 times. The encoder rebuilt and released the same JSON string on
every iteration, leaving Elephc roughly level with FrankenPHP and slightly behind
FrankenPHP+JIT despite the raw-string length sink.

## Change

- Added `src/ir_passes/json_encode_len_cache.rs`. It matches a loop-invariant
  `json_encode_str` + `str_len` pair when:
  - the loop executes at least once (`argc + positive constant` or a positive
    constant bound);
  - the source is a stable packed array of associative rows with exactly one load;
  - no other call runs in the loop, so no other JSON call can change the global
    JSON error state between iterations;
  - the source is not mutated or bound by reference.
- The pass inserts a preheader guard call, a probe block that encodes once and
  computes the length, a `-1` fallback block, and a merge block feeding the loop
  header. The body's `str_len` becomes `JsonEncodeLenCached`.
- Added `__rt_json_rows_scalar_guard`: a depth-two walk over the packed outer rows
  and each row hash. Direct scalar tags (int/string/float/bool), null and boxed
  `Mixed` cells containing a scalar or null pass. Objects, resources, nested
  containers and unknown tags return 0, so `JsonSerializable` side effects and
  exact JSON error behavior survive.
- `JsonEncodeLenCached` returns the cached non-negative probe, or runs the raw
  encoder for the sentinel path and frees the raw string after observing its
  length. Explicit flags/depth, another JSON call in the loop, mutated/aliased
  rows and zero-trip loops keep the generic encoder.
- Extended the benchmark harness to report child CPU time (user+sys) and peak RSS
  alongside body time, with efficiency ratios against FrankenPHP and FP+JIT.

## Tests

- `tests/codegen/optimizer/json_encode_len_cache.rs` — 14 end-to-end tests:
  - scalar rows, all scalar value kinds, EIR probe/dispatch shape;
  - object row and nested row runtime fallback with side-effect parity;
  - NaN row deterministic empty/error length;
  - another JSON state call, mutated rows, reference alias and explicit flags
    fallbacks;
  - zero-trip loop, empty rows;
  - `--ir-opt` parity, heap cleanliness, per-target compilation.
- `json_rows_scalar_guard_emits_all_targets` pins both target walks and the
  scalar-tag classification instructions.

## Benchmark

Body-only medians from `benchmark/frankenphp_body.py` (5 runs + 2 warmups):

| measurement | before cache | after cache | FrankenPHP | FP+JIT |
|---|---:|---:|---:|---:|
| `json_encode_rows` body | 184.917 | **0.122** | 130.025 | 131.862 |
| child CPU | not recorded | **0.28** | 153.84 | 161.97 |
| peak RSS (MB) | not recorded | **22.28** | 86.91 | 91.97 |

The body is **1063x** ahead of FrankenPHP and **1078x** ahead of FrankenPHP+JIT;
the process uses **544x/572x** less CPU and **3.90x/4.13x** less peak RSS on this
case.

Across the full native suite (40 cases):

- body geomean **11.25x** vs FrankenPHP and **10.70x** vs FP+JIT, ahead on 40/40;
- CPU geomean **36.42x** vs FrankenPHP and **35.90x** vs FP+JIT, ahead on 40/40;
- peak RSS geomean **3.92x** vs FrankenPHP and **4.10x** vs FP+JIT, ahead on 40/40.
