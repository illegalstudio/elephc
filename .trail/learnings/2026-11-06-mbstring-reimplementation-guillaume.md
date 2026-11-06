# Restored mbstring kernels and boxed-string indexing

- Reimplemented `__rt_mb_chr_utf8`, `__rt_mb_convert_kana_a_utf8` (AArch64 +
  x86_64), `__rt_mb_encode_numericentity_utf8`, and
  `__rt_mb_decode_numericentity_utf8`, with their `mbstring_fast` lowerings and
  module registration. `mb_chr` and kana matrices match PHP 8.5 byte-for-byte;
  encode/decode matrices (2/3/4-byte scalars, invalid UTF-8, narrow/full maps,
  hex/11-digit fallbacks, plain `&`) match PHP.
- Restored the `const_str` acquire elision in `lower_acquire`: persistent
  data-section literals skip `__rt_str_persist`. Release `mb_chr` is 251 ms for
  1M calls (PHP 30.5, PHP+JIT 17.1, C 0.79); kana 140.6 (126.3/116.6/3.34),
  encode 139.6 (90.4/64.5/15.9), decode 96.4 (65.7/59.8/14.4). The remaining
  mbstring gap is call scaffolding and Mixed boxing, not the kernels.
- Fixed a base bug surfaced by the benchmark: `__rt_mixed_array_get` lacked the
  string tag branch, so `$boxed[0]` returned null and `ord($boxed[0])` was 0 for
  `json_encode` and `mb_chr` results. Added the string branch on both targets
  (integer and negative offsets, owned one-byte result, missing-offset warning).
- Restored the Mixed non-object property-write `Error` and the missing-array-read
  literal widening (`Void`/`Never` literal elements box as Mixed).
- Commits: 405fd1198d (LICM substr_count), 38561b2c46 (mbstring kernels),
  b3a0b782e8 (boxed string indexing), b104132b5e (property/literal fixes).
- Tests: eir_licm 10, substr_count 9, by_ref 445, optimizer 796 passed/0 failed/1
  ignored, plus the new mb_chr/kana/numericentity/boxed-offset tests.
