---
id: gotcha-rt-itoa-reserves-at-concat-off-so-a-helper-build-2b5eeee5
type: gotcha
title: "__rt_itoa reserves AT _concat_off, so a helper building past an unpublished offset gets overwritten"
description: "implode(',', [1,22,333]) printed 12233333 after itoa moved to exact __rt_concat_reserve; implode_int now publishes its cursor before each conversion"
created: 2026-09-22
sources:
  - path: src/codegen_support/runtime/strings/implode_int.rs
    blob: c4f70dfbac1ac72a868d3051bca53956e641d9b0
    lines: 60-110
    snip: 768676e4cb6c
    anchor: "emitter.instruction(\"ldp x1, x2, [sp]\"); // reload glue ptr and length"
  - path: src/codegen_support/runtime/strings/itoa.rs
    blob: ee6d58e0eacc93cf7cfbffda44b89bc2f845f1b0
---

# __rt_itoa reserves AT _concat_off, so a helper building past an unpublished offset gets overwritten

## Fact

`__rt_itoa` was rewritten (to fix the serialize overrun, see that packet) to render into a frame
buffer and copy into an EXACT `__rt_concat_reserve` reservation, which lands at `_concat_off`.
The old itoa wrote 20 bytes PAST `_concat_off` and bumped it by 21, which tolerated a caller that
had a few unpublished bytes in progress. `__rt_implode_int` builds its whole result from
`_concat_off` onward and published only at the end, so every conversion landed on the glue it had
just written.

MEASURED against php 8.5.10: `implode(',', [1, 22, 333])` php `1,22,333`, elephc `12233333`;
`implode('--', [10, 20])` elephc `102020`.

FIX: publish the write cursor into `_concat_off` before each `bl __rt_itoa` (the reservation then
lands exactly on the cursor, so the copy is a no-op), and set the final offset to the cursor rather
than adding the length. Audited the other itoa callers that touch `_concat_off` with one probe per
helper -- implode_bool, json_encode (int/assoc/object/dynamic), serialize, comparisons, long2ip,
string interpolation -- all byte-identical to php; only implode_int had the shape.

NOT verified on x86_64 by execution: the change is symmetric and was checked by reading only.

## Why

Any runtime helper that writes its result at _concat_buf+_concat_off and publishes only at the end breaks the moment it calls a helper that reserves
