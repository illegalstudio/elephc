---
title: "mb_strtolower()"
description: "Converts every character to its Unicode lowercase mapping."
---

## mb_strtolower()

```php
function mb_strtolower(string $string, ?string $encoding = null): string
```

Converts every character to its Unicode lowercase mapping.

**Parameters**:
- `$string` (`string`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_strtolower` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strtolower.md).
