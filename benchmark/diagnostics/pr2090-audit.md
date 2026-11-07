# PR2090 follow-up verification

Controlled Linux x86_64 comparison for [PR2247](https://github.com/illegalstudio/elephc/pull/2247).
Measured UTC: 2026-11-04T23:56:36Z. Raw samples and compiler/archive fingerprints: [pr2090-results.json](pr2090-results.json).

## Revisions and method

| Label | Exact source revision |
|---|---|
| audit-base | `55758e892fc9fbbc84e7c1c2581c916e8dd79c90` |
| premerge-main | `5bf918a7140bedb517c9eb5951e97e821190c4dd` |
| fixed | `5d5516c8c30fb19997866bde1f670456e5ed45c9` |

`audit-base` is the audit merge base. `premerge-main` is the immediate main parent of the PR2090 merge, including PR2237. The latter isolates the combined PR2090 and follow-up change from intervening main work. The runtime table uses the semantic-fixes revision `5d5516c8c3`. A subsequent compiler-only follow-up uses `bdf23da21fdc14a4049cfb6871d23d2b03ec54f1` and is measured separately below. Both source revisions predate this report commit.

Machine: AMD Ryzen 5 7530U with Radeon Graphics; Linux-7.2.8-9-omarchy-x86_64-with-glibc2.44; rustc 1.98.0 (88d9e12ae 2026-08-18); PHP 8.5.12 (cli) (built: Oct 21 2026 19:50:41) (NTS); cc (GCC) 16.2.1 20260810. Every process and its children used CPU 0.

The runtime comparison used release-profile compiler builds and matching bridge archives. The separate debug compiler comparison is labeled below. Fixture sources are unchanged from the audit base. The three suites contain 16 fixtures and 30 native variants. Each native variant has two warmups and 15 measured process executions per compiler, with rotating compiler order. Every execution must match the versioned expected output. PHP retains the host CLI configuration with OPcache and JIT explicitly disabled; C uses `-O2`. Each revision uses an isolated managed-native cache. The workstation build queue prevents concurrent heavy jobs.

These are local process wall times, including startup, rather than universal performance claims. Sub-millisecond results are especially sensitive to startup and scheduling. Minima, maxima and all samples remain in the JSON. PHP/C context measurements run as their separate fixture variants.

## Runtime medians

Times are milliseconds. Speedup is immediate pre-merge main divided by the fixed compiler; values below 1 indicate a slower median.

| Fixture / native variant | Audit base | Pre-merge main | Fixed | Speedup |
|---|---:|---:|---:|---:|
| native/array_sum | 7.586 | 7.583 | 2.245 | 3.38x |
| native/string_concat | 2.096 | 2.118 | 2.081 | 1.02x |
| native/sum_loop | 8.822 | 8.860 | 2.458 | 3.60x |
| magician/algebra_heavy / native | 2.220 | 2.254 | 2.051 | 1.10x |
| magician/algebra_heavy / eval | 3.397 | 3.418 | 2.714 | 1.26x |
| magician/arithmetic_loop / native | 2.191 | 2.231 | 2.033 | 1.10x |
| magician/arithmetic_loop / eval | 13.148 | 13.394 | 13.175 | 1.02x |
| magician/array_reads_writes / native | 2.077 | 2.058 | 2.033 | 1.01x |
| magician/array_reads_writes / eval | 10.243 | 10.290 | 10.101 | 1.02x |
| magician/builtin_calls / native | 2.290 | 2.342 | 2.131 | 1.10x |
| magician/builtin_calls / eval | 22.248 | 22.331 | 22.174 | 1.01x |
| magician/callable_dispatch / native | 2.427 | 2.447 | 2.285 | 1.07x |
| magician/callable_dispatch / eval | 24.065 | 24.071 | 24.226 | 0.99x |
| magician/eval_declared_function / native | 2.637 | 2.726 | 2.349 | 1.16x |
| magician/eval_declared_function / eval | 23.229 | 22.877 | 22.855 | 1.00x |
| magician/include_repeated / native | 2.097 | 2.093 | 2.074 | 1.01x |
| magician/include_repeated / eval | 18.231 | 18.130 | 17.864 | 1.01x |
| magician/large_eval_loop / native | 2.278 | 2.293 | 2.119 | 1.08x |
| magician/large_eval_loop / eval | 2.980 | 3.047 | 2.833 | 1.08x |
| magician/literal_scalar_aot / native | 2.386 | 2.392 | 2.157 | 1.11x |
| magician/literal_scalar_aot / eval | 2.717 | 2.774 | 2.654 | 1.05x |
| magician/literal_scope_read_write_aot / native | 2.238 | 2.231 | 2.085 | 1.07x |
| magician/literal_scope_read_write_aot / eval | 3.207 | 2.986 | 2.973 | 1.00x |
| magician/repeated_small_eval / native | 2.234 | 2.240 | 2.072 | 1.08x |
| magician/repeated_small_eval / eval | 14.396 | 14.194 | 14.304 | 0.99x |
| magician/string_output / native | 2.328 | 2.385 | 2.427 | 0.98x |
| magician/string_output / eval | 11.732 | 11.651 | 11.904 | 0.98x |
| hot-path/buffer-vs-arrays / buffer | 3.052 | 3.062 | 2.853 | 1.07x |
| hot-path/buffer-vs-arrays / indexed | 38.100 | 38.239 | 30.570 | 1.25x |
| hot-path/buffer-vs-arrays / assoc | 27.733 | 27.816 | 28.344 | 0.98x |

PHP and C context for the native suite, measured on this same machine:

| Fixture | Fixed elephc | PHP | C |
|---|---:|---:|---:|
| native/array_sum | 2.245 | 24.296 | 1.871 |
| native/string_concat | 2.081 | 23.728 | 1.921 |
| native/sum_loop | 2.458 | 24.397 | 1.758 |

## Compiler cost before the validator follow-up

Warm-cache CLI compilation includes frontend, EIR, code generation, assembly and linking. Bridge archives are supplied explicitly, so these samples exclude Cargo and native-package builds. Each probe and IR mode has one warmup and five rotating measured compilations per revision. Compiled outputs are checked against PHP outside the compilation timer. Phase timing output is retained in the JSON. These release samples do not reproduce the debug-suite timeout budgets recorded in the original audit.

| Probe / IR optimization | Audit base (s) | Pre-merge main (s) | Fixed (s) | Change vs pre-merge |
|---|---:|---:|---:|---:|
| small-program / on | 0.164 | 0.176 | 0.152 | -13.3% |
| small-program / off | 0.148 | 0.151 | 0.151 | +0.4% |
| opaque-eval / on | 8.435 | 7.460 | 8.580 | +15.0% |
| opaque-eval / off | 7.127 | 7.150 | 7.895 | +10.4% |

## Dense dominator validation follow-up

The validator already computes dense dominator bitsets. This change keeps them through membership checks instead of converting them into a hash set for every block. It preserves the dominance algorithm and validation gates, including unreachable blocks and same-block instruction order. A 72-block positive/negative regression exercises membership across a bitset word boundary.

MAD is the median absolute deviation divided by the median. This separate release comparison repeats the same probes with one warmup and five rotating samples per compiler and mode. Bridge archive fingerprints are identical to the semantic-fixes build. Selected array_sum, string_concat, sum_loop and indexed-buffer fixtures emit byte-identical assembly and retain their expected outputs. Their runtime samples above were collected at `5d5516c8c3`; the full runtime suite was not repeated after the compiler-only change.

| Probe / IR optimization | Pre-merge main (s) | Semantic fixes (s) | Dense validation (s) | Dense vs pre-merge | Dense MAD |
|---|---:|---:|---:|---:|---:|
| small-program / on | 0.167 | 0.168 | 0.170 | +2.1% | 3.06% |
| small-program / off | 0.150 | 0.147 | 0.150 | +0.1% | 1.66% |
| opaque-eval / on | 8.299 | 8.335 | 8.569 | +3.3% | 2.57% |
| opaque-eval / off | 7.260 | 8.227 | 8.117 | +11.8% | 1.55% |

Release opaque-eval phase medians expose the remaining lowering cost separately from assembly and linking:

| IR optimization / phase | Pre-merge main (s) | Semantic fixes (s) | Dense validation (s) |
|---|---:|---:|---:|
| on / Lowering program to EIR | 0.437 | 1.040 | 0.987 |
| on / Optimizing EIR | 0.465 | 0.586 | 0.577 |
| on / Generating native code | 3.320 | 2.970 | 3.000 |
| off / Lowering program to EIR | 0.383 | 1.030 | 0.948 |
| off / Optimizing EIR | 0.000 | 0.000 | 0.000 |
| off / Generating native code | 2.910 | 2.930 | 2.910 |

Debug opaque-eval compilation, IR optimization on, three samples per compiler with the same fixture and matching debug bridge archives:

| Phase | Semantic fixes (s) | Dense validation (s) | Change |
|---|---:|---:|---:|
| Lowering program to EIR | 6.900 | 5.950 | -13.8% |
| Optimizing EIR | 14.360 | 10.190 | -29.0% |
| Generating native code | 9.120 | 9.730 | +6.7% |
| Total | 36.990 | 32.450 | -12.3% |

Phase times are compiler-reported rounded values; raw process wall times and phase logs remain in the JSON. The initial debug pair preceded the release rebuild, then two more pairs alternated compiler order. This is a diagnostic sample, not a full-suite timeout guarantee.

Opaque-eval release compilation remains +3.3% with IR optimization on and +11.8% with it off, versus immediate pre-merge main. The validator change reduces validation overhead, especially in debug builds. Extra whole-prelude lowering work remains visible and is not claimed to be eliminated.

The small probe is `examples/fizzbuzz/main.php`. The opaque probe uses runtime-unknown eval source, forcing whole-catalog/prelude lowering:

```php
<?php
$code = $argc > 0 ? 'echo 42, "\n";' : '';
eval($code);
```

## Correctness coverage

PR2247 covers every actionable semantic/diagnostic issue from audit sections 1 and 2.1-2.2, plus the additional regressions found during follow-up:

- Padding/case mode errors and evaluation order, padding error names, literal multibyte trim masks, regex anchoring, and removal of the debug print.
- Live MIME/regex encoding, options and limits; Shift-JIS delimiter bytes; failed split-count TypeError and owned-result cleanup.
- Interior MIME Base64 padding, caller-visible progressive session mutations and iconv settings, progressive resource-limit warnings, and variable-conversion illegal-character counter updates.
- Original explode diagnostics and single operand evaluation when fusion declines unsupported coercion.

The five standalone semantic probes match PHP on both pre2090 compilers and on the fixed release with IR optimization on/off. The raw data also retain the incorrect outputs from initial follow-up head `8fa88e8c`, the PHP sources and their hashes.

The semantic-fix validation passed thirteen focused codegen tests and three focused unit gates. The validator follow-up additionally passed fifteen focused unit tests and eighteen codegen tests, including the semantic regressions and five existing bounded mixed-counter cases. New semantic fixtures run with IR optimization on/off and heap diagnostics. The emitter guard test covers all five supported targets. Builtin documentation regeneration has no generated drift, and the docs, target-boundary, assembly-comment and diff audits pass. Full target execution coverage remains with CI.

The audit also records pre-existing limitations: Stringable explode coercion remains a compile error, and the strict-types/null-deprecation and string-increment deprecation observations are outside the PR2090 regression fixes.

## Reproduction

Build each exact revision with `cargo build --release` in an isolated checkout and keep its matching `libelephc_*.a` archives beside the compiler. Fixture-level reruns use the existing runner:

```sh
taskset -c CPU python3 benchmark/run.py --suite all --no-build \
  --compiler /path/to/revision/elephc --iterations 15 --warmup 2 \
  --json results.json
```

For compilation probes, invoke that compiler with the same PHP file, `--ir-opt=on` or `--ir-opt=off`, `--timings --quiet`, one warmup and five measured runs. The recorded experiment rotates revision order each round; independent runner reruns are useful checks but do not reproduce that paired ordering. These diagnostic results are separate from the CI dashboard history.
