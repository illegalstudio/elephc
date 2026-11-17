---
title: "mb_ereg_search_pos()"
description: "Returns the next multibyte regex match byte offset and length, or false."
---

## mb_ereg_search_pos()

```php
function mb_ereg_search_pos(?string $pattern = null, ?string $options = null): array|false
```

Returns the next multibyte regex match byte offset and length, or false.

**Parameters**:
- `$pattern` (`?string`), default `null`, optional
- `$options` (`?string`), default `null`, optional

**Returns**: `array|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ereg_search_pos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ereg_search_pos.md).
