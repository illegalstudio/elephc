---
title: "mb_convert_case()"
description: "Converts case using one of the eight Unicode case modes."
---

## mb_convert_case()

```php
function mb_convert_case(string $string, int $mode, ?string $encoding = null): string
```

Converts case using one of the eight Unicode case modes.

**Parameters**:
- `$string` (`string`)
- `$mode` (`int`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_convert_case` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_convert_case.md).
