# Local mb_chr length sink

- Added `src/ir_passes/mb_chr_len_local_sink.rs`. It traces
  `$x = mb_chr($codepoint, $encoding); strlen($x)` through the Mixed clone/acquire/store
  scaffolding to the single local slot, proves every load feeds a `Cast Str` and `StrLen`, then
  retargets the call to `RuntimeFnId::MbChrLen` and retires the local plus clone/cast/length
  scaffolding.
- Multiple `strlen()` observations share one retargeted call. UTF-16, malformed codepoints, and
  the bridge fallback remain exact because `MbChrLen` owns the same dispatch as `mb_chr`.
- Any value observation (`echo $x`, `=== false`, another call) keeps the boxed result.
- New tests: `tests/codegen/optimizer/mb_chr_len_local_sink.rs` (4 tests), covering UTF-8 byte
  widths, multiple lengths, UTF-16, false, value echo, and identity checks.
- Interleaved A/B on the same machine, `mb_chr` + `strlen` loop, 1M calls: old HEAD
  233-382 ms, new 40-64 ms (~6-8x). Native benchmark `benchmark/cases/native/mb_chr_len`
  (5 runs, 2 warmups, process timing): elephc 37.59 ms vs PHP 40.70, PHP+JIT 28.54,
  C -O2 1.43. The existing value-observing `mb_chr` fixture stays at 443 ms vs PHP 46.70 /
  PHP+JIT 29.87 / C 1.33 because `ord()` still requires the string.
