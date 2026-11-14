# Restored single-pass JSON ASCII fast paths

- Replaced the two-pass short-string fast paths (one scan plus one copy, with a per-byte
  compare chain) in `__rt_json_encode_str` and the associative-array key/value encoders
  with a 256-byte `_json_plain_ascii` classification table. The key/value paths reserve
  `len + 2`, classify each source byte through the table, and copy accepted bytes in the
  same pass. The direct-copy bound is 16 bytes for keys, values, and the standalone string
  helper.
- A byte that needs escaping abandons the partial write and resumes through the full
  encoder. Because `__rt_json_buf_ensure` may grow and move the buffer, the cursor returned
  after growth is published to the saved slot before scanning, so the fallback never uses a
  pre-growth pointer.
- Correctness: a 5-flag × 24-string matrix (plain, quote, backslash, slash, control bytes,
  DEL, 2/3/4-byte UTF-8, empty, 16/17-byte boundaries) matches PHP 8.5 byte-for-byte on
  aarch64 and linux-x86_64 under qemu. A >64 KiB growth fixture with a later escaped key
  and value also matches PHP on both targets.
- Tests: codegen `json` 561 passed; new `json_short_ascii_fast_path_length_boundaries_match_php`
  and `json_ascii_fast_path_falls_back_after_buffer_growth`.
- Native benchmark `benchmark/cases/native/json_encode_rows` (30k encodes of 100 associative
  rows, process timing, 3 runs): elephc 139.45 ms vs PHP 157.95, PHP+JIT 160.43, C -O2
  14.21. The change moved the same fixture from roughly 235 ms (pre-change release) to
  139 ms and now beats PHP+JIT by 1.15x; C remains ahead because it writes into a fixed
  stack buffer with no allocation or hash walk.
