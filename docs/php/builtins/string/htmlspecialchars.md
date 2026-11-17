---
title: "htmlspecialchars()"
description: "Converts the HTML special characters in a string into their entities."
---

## htmlspecialchars()

```php
function htmlspecialchars(string $string, int $flags = 11, string $encoding = 'UTF-8'): string
```

Converts the HTML special characters in a string into their entities.

**Parameters**:
- `$string` (`string`)
- `$flags` (`int`), default `11`, optional
- `$encoding` (`string`), default `'UTF-8'`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `htmlspecialchars` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/htmlspecialchars.md).
