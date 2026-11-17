# Folding intval over masked literal string arrays

## Gap

`intval_string` reads one of eight literal strings with `$texts[$i & 7]` and
casts it to an integer on every one of 3,000,000 iterations. The EIR kept the
generic checked `array_get` (null/shape/bounds/key work) plus the runtime
`__rt_str_to_int` conversion, even though every element and its integer result
are compile-time constants. The body comparison stayed behind FrankenPHP.

## Change

- Added `src/ir_passes/const_string_array_intval_fold.rs`, an EIR fixed-point
  pass that recognizes `(int) $texts[$i & M]` when:
  - the receiver is a `PhpLocal` built from `array_new` plus `array_push` calls
    over single-use `ConstStr` literals;
  - the array is stored exactly once and loaded exactly once, by the read;
  - every construction instruction dominates the read (or precedes it in the
    same block);
  - no `ReleaseLocalSlot` of the array sits inside the read's natural loop or in
    a block dominating the read;
  - the index is a literal in `0..len` or `value & (len - 1)` with a
    power-of-two literal length.
- The pass rewrites each pushed `ConstStr` in place to the compile-time PHP
  integer cast of that literal, retypes the literal array and its ownership
  wrappers to `array<int>`, changes the checked read to `array_get_unchecked`,
  and retires the acquire/cast/release scaffolding.
- PHP conversion semantics reuse
  `crate::optimize::fold::casts::parse_string_cast_int`, the shared AST cast
  fold: leading whitespace/sign/digit run, `42abc`, `0x1A`, `1_000`, `007`,
  float/exponent forms, empty/blank strings, and out-of-range saturation.
- Any observed or mutated array, dynamic index, mask/length mismatch,
  non-power-of-two mask, runtime-built array, explicit-base `intval()`, or
  non-dominating construction keeps the generic checked read.

## Tests

- `tests/codegen/optimizer/const_string_array_intval_fold.rs` — 18 end-to-end
  tests:
  - benchmark shape with `array_get_unchecked` and `array<int>`;
  - explicit `(int)` cast;
  - signed, blank-padded, float, exponent, hex-like and underscore strings;
  - empty/blank/sign-only strings;
  - single-element mask zero;
  - folded cast used directly as a branch condition;
  - literal array built inside a user function;
  - dynamic `% 8` index fallback;
  - mask/length mismatch fallback;
  - non-power-of-two mask fallback;
  - observed array fallback;
  - mutation fallback;
  - non-dominating construction fallback;
  - runtime-built array fallback;
  - explicit-base `intval()` fallback;
  - `--ir-opt=on/off` parity;
  - heap-debug cleanliness;
  - per-target staticlib compilation on all five supported targets.
- Focused and full optimizer suite results are recorded before committing.

## Benchmark

Body-only medians from `benchmark/frankenphp_body.py` (7 runs + 3 warmups):

| case | before | after | FrankenPHP | FP+JIT |
|---|---:|---:|---:|---:|
| `intval_string` | 105.196 | **5.261** | 69.104 | 51.798 |

The folded body is **13.1x** ahead of FrankenPHP and **9.85x** ahead of
FrankenPHP+JIT in this run. The loop now loads one packed integer with no bounds,
shape, key, warning, string-cast or runtime-parse work.
