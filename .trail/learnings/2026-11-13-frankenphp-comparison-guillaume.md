# Native benchmark comparison: Elephc vs FrankenPHP

## Version and security check

- FrankenPHP **v1.13.5**, the latest stable release, published 2026-11-05.
- Downloaded the official `frankenphp-linux-aarch64-gnu` asset from
  `php/frankenphp` (the same release as `dunglas/frankenphp`).
- SHA-256: `44ff0950ab0b27bd43770d1885e44c0cde4917617003fefb9a83651fcef68aaf`,
  exactly matching the GitHub release asset digest.
- v1.13.5 is a security release: it fixes 17 vulnerabilities. The queried
  advisories (including the php_server source-disclosure, HTTP/2 Host, worker
  state, crash and negative-exit issues) all list **`patched_versions: 1.13.5`**.
- Embedded runtime: PHP 8.5.12, Caddy 2.11.9.

## Method

- Same `benchmark/cases/native` fixtures as the normal native suite; the
  uncommitted `array_map_array_merge_source` WIP fixture was excluded.
- `frankenphp php-cli main.php` for the embedded CLI runtime, with a pinned
  `PHPRC`: plain variant `opcache.enable_cli=0`/`opcache.jit=0`, JIT variant
  `opcache.enable_cli=1`, `opcache.jit_buffer_size=64M`, `opcache.jit=tracing`.
- 5 measured runs + 2 warmups per variant, median reported.
- `benchmark/frankenphp_compare.py`: process wall time, startup included.
- `benchmark/frankenphp_body.py`: injects `hrtime(true)` at the top of each
  fixture and a body marker at the end, preserving the standalone `$argc` and
  Elephc's stored-map lowering. PHP/FrankenPHP parse the file before the timed
  region, so parsing is excluded for every runtime.

## Results (39 native cases)

| measure | vs FrankenPHP | vs FrankenPHP+JIT |
|---|---:|---:|
| process wall, geometric mean | **23.16x** | **21.82x** |
| body only, geometric mean | **6.80x** | **5.83x** |
| cases where Elephc is faster | 34/39 | 30/39 |

The wall ratio is dominated by FrankenPHP's ~50-75 ms CLI startup on
microbenchmarks; the body comparison is the meaningful runtime figure.

Representative CPU-heavy body medians (ms):

| case | Elephc | FrankenPHP | FP+JIT | Elephc vs FP |
|---|---:|---:|---:|---:|
| `json_encode_rows` | 615.969 | 582.243 | 493.533 | 0.95x |
| `intval_string` | 253.053 | 103.308 | 88.082 | 0.41x |
| `mb_encode_numericentity` | 256.836 | 103.766 | 88.095 | 0.40x |
| `mb_decode_numericentity` | 186.276 | 187.254 | 79.106 | 1.01x |
| `mb_convert_kana` | 260.367 | 287.490 | 205.043 | 1.10x |
| `mixed_int_division` | 61.379 | 52.774 | 35.646 | 0.86x |
| `int_accumulator` | 36.748 | 196.698 | 15.002 | 5.35x |
| `sprintf_fixed` | 110.061 | 118.589 | 93.380 | 1.08x |
| `array_map_int_div_mixed` | 23.792 | 12.149 | 12.339 | 0.51x |

The fused stored-map, reduce, string and integer-loop kernels are generally
8x-40x faster than FrankenPHP without JIT (for example `array_map_square`
0.784 vs 11.202 ms, `array_reduce_seed` 0.294 vs 11.834 ms, `sum_loop` 0.317 vs
1.130 ms). The remaining Elephc losses are concentrated in
`intval_string`, the mbstring numeric-entity encoders/decoders, the mixed
int-division map rebuild (`array_map_int_div_mixed`), and the heavy JSON case;
FrankenPHP+JIT also beats Elephc on `int_accumulator` and `sprintf_fixed`.

## Caveats

- The machine load varied between runs, so absolute body medians are indicative;
  ratios are more stable.
- The body wrapper adds two `hrtime` calls and excludes process startup; it is
  intended for runtime comparisons, not for the published native CI history.
- The comparison used the pushed compiler `d7f68a51eb`; the uncommitted
  `array_merge` source-proof WIP was not part of the measurement.
