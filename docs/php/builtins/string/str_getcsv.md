---
title: "str_getcsv()"
description: "Parse a CSV string into an array."
sidebar:
  order: 457
---

## str_getcsv()

```php
function str_getcsv(string $string, string $separator = ',', string $enclosure = '"', string $escape = '\\'): array
```

Parse a CSV string into an array.

**Parameters**:
- `$string` (`string`)
- `$separator` (`string`), default `','`, optional
- `$enclosure` (`string`), default `'"'`, optional
- `$escape` (`string`), default `'\\'`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/str_getcsv.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/str_getcsv.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `str_getcsv` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_getcsv.md).
