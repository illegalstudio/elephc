---
title: "mb_convert_case()"
description: "Converts a string between Unicode case mappings using MB_CASE_* modes."
sidebar:
  order: 468
---

## mb_convert_case()

```php
function mb_convert_case(string $string, int $mode, string $encoding = null): string
```

Converts a string between Unicode case mappings using MB_CASE_* modes.

**Parameters**:
- `$string` (`string`)
- `$mode` (`int`)
- `$encoding` (`string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_convert_case.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_convert_case.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_convert_case` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_convert_case.md).
