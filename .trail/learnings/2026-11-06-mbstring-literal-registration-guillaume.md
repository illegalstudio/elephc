# One-shot mbstring literal registration

- `lower_const_str` previously called `__rt_mbstring_ini_literal` on every execution of the
  same literal in programs with mbstring/eval features. That helper saves every general and
  vector register and enters the Rust identity bridge even though registration is idempotent.
- Each deduplicated string literal now gets an eight-byte `.comm` generation stamp in the
  program data. `__rt_mbstring_ini_reset` advances `_mbstring_request_generation` on every
  request (including the dormant path, before it skips the Rust reset), and codegen calls the
  registration helper only when the stamp differs from the current generation. First uses
  register, later uses pay two loads and a compare, and web/eval request resets cannot inherit
  a previous request's stamp.
- Kept the existing ABI: the helper preserves `x1`/`x2` and `rax`/`rdx`, so the string result
  remains live across the conditional call on both targets.
- Tests: mb_chr/kana/numeric-entity fast paths, `mbstring_startup` 11, `mbstring_scalar` 8,
  `mb_detect_order` literal fast path, `mbstring_parse_str` 5 all pass.
- Interleaved A/B on the same machine (old HEAD compiler vs generation-stamped compiler,
  `mb_chr` loop with the result discarded, 1M calls): old 91-130 ms, new 70-105 ms; medians
  roughly 109 -> 76 ms (~30%). The full consumer fixture (strlen + ord on a boxed result) is
  dominated by Mixed allocation, so its A/B is noise-limited.
- Suite: 189 non-regex mbstring tests passed before the regex capture tests blocked on the
  missing managed Oniguruma package. Targeted `mbstring_startup` 11, `mbstring_scalar` 8,
  `mbstring_parse_str` 5, and the chr/kana/numeric-entity fast paths all pass.
