# Direct `ord($boxed[0])` fusion measured no gain (reverted)

- Tried lowering `ord($value[0])` on a `string|false` local to
  `strlen($value) > 0 ? ord($value) : <generic $value[0] read>`, avoiding the one-character
  Mixed string allocation from the boxed-offset path.
- Interleaved A/B on the same machine (HEAD compiler vs fused compiler), 3M iterations:
  old 167-186 ms, new 168-188 ms. No measurable gain. The `mb_chr` consumer fixture likewise
  stayed inside the noise (old 430-499 ms, new 453-546 ms).
- The cast/length branch and the remaining per-iteration Mixed accumulator dominate; avoiding
  the one-character allocation is not the lever. Reverted the lowering, its module wiring, and
  its regression test.
- The retained `ord` coverage remains in `tests/codegen/strings/encoding.rs`; the existing
  boxed-string index branch in `__rt_mixed_array_get` still preserves PHP semantics.
