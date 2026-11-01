---
id: gotcha-a-by-value-foreach-element-write-mutates-the-sou-b5e61961
type: gotcha
title: "A by-value foreach element write mutates the source array"
description: "foreach ($x as $row) { $row[0] = ... } writes through into the source when the array came from a parameter or a property; only a local literal is safe"
created: 2026-09-17
sources:
  - path: src/codegen/lower_inst/mixed_array_runtime.rs
    blob: ec1736df93bd3d094cbe8c544c84ecf1fe8612a4
  - path: src/ir_lower/stmt/array_write_core.rs
    blob: 3fe3ec43a30a43ead96bcd530689f3fb4da164bb
---

# A by-value foreach element write mutates the source array

## Fact

`foreach ($x as $row)` is BY VALUE, so `$row` is a copy and writing `$row[0]` must separate it
first. elephc writes straight through into the source's nested array.

    function fromParam(array $rows): string {
        foreach ($rows as $row) { $row[0] = 'REPLACED'; }
        return $rows[0][0];              // php: keep    elephc: REPLACED
    }

Measured against `php -n` (2026-09-17):

    local array literal                  keep      keep     OK
    declared `array` parameter           REPLACED  keep     WRONG
    object property                      REPLACED  keep     WRONG
    property copied to a local first     REPLACED  keep     WRONG

Assigning the property to a local does NOT help, so the outer copy shares its inner arrays without
copy-on-write separation.

Path: `$row` is a boxed Mixed, so `array_set_op` picks `Op::RuntimeCall`, which lowers to
`__rt_mixed_array_set` -- whose own doc says "Indexed arrays mutate slots directly". Nothing
separates the container, and nothing can: publishing a separated container back into the SHARED
Mixed cell would make the parent observe it too, so the fix has to separate the cell as well and
store it back into the local, which only the lowering knows how to do.

`separate_get_for_write_receiver` in `src/codegen/lower_inst/arrays.rs` already does exactly this
for the by-REFERENCE foreach path (issue #580) and documents the double-free trap: `ensure_unique`
CONSUMES a reference from the source when it splits.

Symptoms this explains, which look like separate bugs:
- A closure held in a nested array is destroyed by `$entry[0] = $entry[0]()`. The second call gets
  runtime tag 0 (Int) and fatals `Unsupported EIR callable_descriptor_invoke mixed value is not
  callable`. Calling the SAME method twice is enough; the shared `_eir_shared_mixed_callable_invoke`
  helper is a red herring -- the inline path fails identically.
- Symfony's `kernel.request` listener table is a nested array on a container property, and only the
  lowest-priority listener ever runs; `RouterListener::onKernelRequest` never does.

## Why

It destroys data structures on first use and looks like several unrelated bugs; it is the prime suspect for Symfony's listener table being empty after the first dispatch
