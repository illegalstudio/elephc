---
id: bug_fix-file-put-contents-ignored-file-append-lock-ex-in-d0f5152f
type: bug_fix
title: "file_put_contents ignored FILE_APPEND/LOCK_EX in AOT, returned -1 instead of false, and wrote to errno as a descriptor on macOS"
description: "AOT honours flags via __rt_file_put_contents_flags, types the result int|false, detects raw open failure by carry flag; eval takes flags/context/array data"
created: 2026-09-26
sources:
  - path: src/codegen_support/runtime/io/file_put_contents.rs
    blob: e5565db801a8032e10dd74253b52fdb7808ed922
    lines: 34-125
    snip: 9b3fda5d8bb6
    anchor: "pub fn emit_file_put_contents(emitter: &mut Emitter) {"
  - path: src/codegen/lower_inst/builtins/io/phar_write.rs
    blob: d4a525e6562774ad6816ade28ddec24dc04dde84
    lines: 43-90
    snip: 160ab27de167
    anchor: "fn store_byte_count_or_false(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {"
  - path: crates/elephc-magician/src/interpreter/builtins/filesystem/file_put_contents.rs
    blob: d64054dc43d201bb46c5da6a7e9b8949e01475e9
    lines: 112-130
    snip: 2b156900f30c
    anchor: "fn write_local_file(path: &std::path::Path, data: &[u8], flags: i64) -> std::io::Result<()> {"
---

# file_put_contents ignored FILE_APPEND/LOCK_EX in AOT, returned -1 instead of false, and wrote to errno as a descriptor on macOS

## Fact

`file_put_contents()` had three silent defects, found because Symfony's `SodiumVault` writes its
keys with `file_put_contents($path, $data, LOCK_EX)` and checks `false === ...`:

- AOT accepted `$flags` and ignored it: `FILE_APPEND` truncated (php `abcdxy3`, elephc `xy3`).
  `__rt_file_put_contents_flags` now takes the flags in x5 / r8, opens WITHOUT `O_TRUNC`,
  `flock(LOCK_EX)`s when asked and only then `ftruncate`s unless appending (php's own order).
  `__rt_file_put_contents` stays as the flags=0 entry for the copy and PHAR writers, and the
  maybe-phar wrapper tail-calls the flags entry so the register survives.
- AOT typed the result `Int`, so a failed write returned int(-1) and `false === ...` folded to
  false. It is now `int|false`, boxed in `store_byte_count_or_false`, and FilePutContents is
  `Independent` in `result_ownership`.
- On macOS arm64 the raw `open` syscall reports failure with the CARRY flag and a POSITIVE errno
  in x0; the old helper used that errno as a descriptor, so writing to a missing directory with
  ENOENT (2) sent the data to stderr and returned its length. Check `b.cs` on macOS and the sign
  on Linux after any raw open.
- The interpreter accepted exactly two arguments ("unsupported NamespacedCall
  file_put_contents()"); it now takes `$flags`, `$context`, and array data.

Verified with a php-vs-elephc probe on both backends, and the tests
`test_file_put_contents_flags_array_data_and_failure` / `test_eval_file_put_contents_flags_and_context`.

## Why

Symfony's SodiumVault writes keys with LOCK_EX and checks false ===; the macOS raw-syscall carry-flag convention is a trap for any runtime helper that calls open
