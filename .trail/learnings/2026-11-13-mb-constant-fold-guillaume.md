# Compile-time folding of constant mbstring producers

## Gap

`mb_convert_kana()`, `mb_encode_numericentity()` and `mb_decode_numericentity()`
were invoked on every loop iteration even when their subject, mode, map and
encoding were constants. The body comparison still put Elephc behind
FrankenPHP+JIT on those native cases.

## Change

- Added `src/ir_passes/mb_constant_fold.rs`, an EIR fixed-point pass that folds
  the three producers when:
  - the subject is a literal or an entry-local literal;
  - `mb_convert_kana`'s mode is a literal valid mode and the encoding is a literal
    valid encoding;
  - the numeric-entity map is the canonical full-decimal packed array proved by
    `full_decimal_map`, and the encoding is a literal valid encoding;
  - the result bytes are valid UTF-8.
- The call becomes a persistent `ConstStr`; the surrounding acquire/store/release
  and loop scaffolding stay intact, so no per-iteration bridge call or heap box
  is needed.
- Any dynamic subject/mode/encoding, non-canonical map, invalid map/encoding or
  non-UTF-8 result keeps the runtime bridge call and PHP's exact diagnostics.
- The compiler now depends on the `elephc-mbstring` rlib for the pure text
  routines; emitted programs still link only the staticlib they require.

## Tests

- `tests/codegen/optimizer/mb_constant_fold.rs`:
  - literal `mb_convert_kana` folds; optimized EIR has no runtime call, no-IR-opt
    keeps it;
  - canonical-map encode/decode over inlined literal sources folds;
  - dynamic subject, dynamic mode/encoding, non-canonical maps and non-UTF-8
    outputs fall back to the runtime and match PHP;
  - empty literal subject;
  - `--ir-opt=on/off` parity;
  - heap-debug cleanliness;
  - all five supported targets compile the folded source with no runtime mb call.
- Full optimizer suite and body benchmark are recorded after the build.

## Benchmark

Body-only medians from `benchmark/frankenphp_body.py` (5 runs + 2 warmups):

| case | before | after | FrankenPHP | FP+JIT |
|---|---:|---:|---:|---:|
| `mb_convert_kana` | 106.110 | **7.597** | 105.770 | 146.144 |
| `mb_decode_numericentity` | 64.748 | **9.800** | 75.213 | 49.916 |
| `mb_encode_numericentity` | 73.079 | **9.697** | 61.272 | 50.739 |

The runtime call disappears entirely, so the remaining loop is only the constant
string observations and the accumulator. The machine was heavily loaded in this
run (absolute PHP/FP times moved substantially); the structural gain is that
Elephc is now 5x-19x ahead of both FrankenPHP variants.

Remaining behind cases after this block: the mixed int-division map rebuild,
`intval_string` and the heavy JSON loop.
