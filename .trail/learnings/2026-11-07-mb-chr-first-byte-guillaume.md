# mb_chr first-byte and packed observation sinks

## Design

- `src/ir_passes/mb_chr_len_local_sink.rs` now proves two storage-neutral observation chains
  from the single local slot bound to `mb_chr(...)`:
  - length: `Cast` to `Str` then `StrLen`;
  - first byte: `$local[0]` (constant zero index, warning flag `true`), `Cast` to `Str`, then
    `ord`.
  Anything else keeps the boxed `mb_chr()`.
- Length-only locals keep the existing `MbChrLen` retarget. First-byte-only locals use the new
  `MbChrFirst`. A mixed local uses the new `MbChrPacked`, which returns
  `(byte_length << 8) | first_byte`; the length terminal becomes `IShrA 8` and the first-byte
  terminal becomes `IBitAnd 0xFF`. One encoding pass serves both observations, and the original
  call operands (which the call already consumed and cleanup releases) are never re-evaluated.
- `lower_chr_observation` reuses the proven UTF-8 validation branches from `lower_chr_len` and
  encodes the first byte as `cp`, `0xC0 | cp >> 6`, `0xE0 | cp >> 12`, or `0xF0 | cp >> 18`.
  Invalid codepoints observe zero, matching existing `ord(false[0])` output.
- The non-UTF-8 bridge fallback unboxes `mb_chr()`'s result and derives both observations from
  the boxed string. U+0000 stays `1:0`, invalid stays `0:0`, and UTF-16 `"A"` stays `2:0`.

## Regression found and fixed

The first fallback implementation loaded the first payload byte *after* calling
`__rt_decref_any` on the boxed cell. The cell owns the bridge-produced string, so the byte read
touched freed storage. It worked for a lone `mb_chr(65, "UTF-16")` because the freed block still
held the old bytes, but `UTF-8` + invalid + `UTF-16` reported `2:165`. The final code loads the
byte before the release and saves it on the stack.

## Validation

- `tests/codegen/optimizer/mb_chr_len_local_sink.rs`: 11 passed, including first-byte-only,
  packed mixed, U+0000/invalid/UTF-16, value observation, nonzero offset, heap-debug cleanliness,
  and `--ir-opt` parity.
- `optimizer::`: 846 passed / 0 failed / 1 ignored (includes the all-target emitter test).
- `arrays::`: 1298 passed / 0 failed / 1 ignored.
- `runtime_gc::mbstring`: 12 non-eval ownership tests passed / 0 failed before the eval-heavy
  remainder was stopped; `runtime_gc::borrowed_string_reads`: 6 passed / 0 failed.
- x86_64 cross build (`ELEPHC_MBSTRING_LIB_DIR=/tmp/x86mb`, `PATH=/tmp/x86shim`) plus
  `qemu-x86_64 -L /usr/x86_64-linux-gnu` matched PHP for `226|240|65`, `735`, and
  `1:0|0:0|2:0` on both the debug and release compilers.

## Measurements

- Debug compiler benchmark (`benchmark/cases/native/mb_chr/main.php`, 1M iterations):
  about 11 ms per process run vs PHP 51-206 ms, PHP+JIT 35-172 ms, C `-O2` 1-8 ms. The previous
  boxed path was observed around 443 ms.
- Release formal benchmark, 5 runs + 2 warmups: elephc **11.845 ms** vs PHP 41.134,
  PHP+JIT 26.403, C `-O2` 1.811. A later run under heavy host load measured 25.258 vs
  71.069 / 68.023 / 4.202; the C baseline moved with the same load.
- Interleaved typed A/B against the parent compiler (`ffc6f17fe1`), in-process `hrtime`,
  1M iterations: parent 331.1 / 340.6 / 439.4 / 519.0 / 537.0 ms; new
  10.4 / 11.0 / 11.1 / 11.3 / 22.6 ms. Both printed `183750000` (median ~39x).
