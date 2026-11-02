---
id: bug_fix-the-serialize-family-wrote-past-the-64-kib-conca-dd95f6b5
type: bug_fix
title: "The serialize family wrote past the 64 KiB concat scratch, silently"
description: "serialize() of a result over 64 KiB overran _concat_buf into the neighbouring .comm globals; right length, wrong bytes, then SIGBUS"
created: 2026-09-22
verified_by: "serialize(range(1,$n)) byte-identical to php 8.5.10 at n=100..20000; cargo test --release --lib -p elephc keeps its 29-failure baseline"
sources:
  - path: src/codegen_support/runtime/strings/concat_scratch.rs
    blob: 3eeb7e3e08fdda7326da0ace11f5b114b7f3f5cb
  - path: src/codegen_support/runtime/system/serialize.rs
    blob: 4e67a233ee71be6daa218a622ba88b8a37c24587
  - path: src/codegen_support/runtime/strings/itoa.rs
    blob: ee6d58e0eacc93cf7cfbffda44b89bc2f845f1b0
---

# The serialize family wrote past the 64 KiB concat scratch, silently

## Fact

`_concat_buf` is a fixed 64 KiB `.comm`, and `__rt_concat_append` / `__rt_serialize_uint` /
`__rt_itoa` appended at `_concat_buf + _concat_off` with NO capacity check. Past the end they
wrote into the neighbouring common symbols.

MEASURED against php 8.5.10, `serialize(range(1, $n))`:
    n=100/1000/3000  identical bytes and md5
    n=5000           right LENGTH (67792) and WRONG CONTENT  <- 67792 > 65536
    n=20000          SIGBUS

The overrun is what killed Symfony's `bin/console`. `sample` on the hung process showed
`__rt_serialize_object + 40` -- the return address of its `blr x9` -- branching to
0x645f746c7561, which is ASCII ("ault_d"): the slot `_elephc_eval_serialize_object_fn`, 8344
bytes past `_concat_buf`'s end, had been overwritten with string bytes by its own caller. A
conditional lldb watchpoint on that address named `__rt_itoa + 112` as the writer.

METHOD WORTH REUSING: `nm -n` the binary and read the `.comm` layout. `_concat_off` sits
immediately after `_concat_buf`, and the eval callback slots sit 7 KiB before `_heap_buf`, so
"which global did this clobber" is answered by arithmetic, not by guessing.

FIX: `__rt_itoa` now renders into a frame-local buffer and copies into an exact-size
`__rt_concat_reserve` reservation (the shape `__rt_dec_to_base` already used, and for its stated
reason: writing right-to-left straight into the reservation hands back an interior pointer of a
heap-backed block that the release path cannot free). `__rt_concat_append` and
`__rt_serialize_uint` bound themselves and fatal through `__rt_alloc_overflow`.
`CONCAT_BUF_CAPACITY` is now 8 MiB plus a 64 KiB guard, and `runtime::data::fixed` derives the
`.comm` size from the constant instead of spelling 65536 again.

STILL OWED: serialize keeps ABSOLUTE write pointers in each recursive frame, so its storage
cannot relocate mid-value; the principled fix is to carry OFFSETS and grow through
`__rt_concat_grow` like `__rt_sprintf` does -- about forty sites across both architectures. Until
then 8 MiB is a ceiling, not a guarantee.

## Why

A result larger than the scratch corrupted the .comm globals that follow it, including the interpreter callback slots
