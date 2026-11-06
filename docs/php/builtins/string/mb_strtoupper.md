---
title: "mb_strtoupper()"
description: "Converts every character to its Unicode uppercase mapping."
---

## mb_strtoupper()

```php
function mb_strtoupper(string $string, ?string $encoding = null): string
```

Converts every character to its Unicode uppercase mapping.

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

For how `mb_strtoupper` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strtoupper.md).
