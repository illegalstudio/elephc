---
title: "explode()"
description: "Splits a string by a separator into an array of substrings."
---

## explode()

```php
function explode(string $separator, string $string, int $limit = PHP_INT_MAX): array
```

Splits a string by a separator into an array of substrings.

**Parameters**:
- `$separator` (`string`)
- `$string` (`string`)
- `$limit` (`int`), default `PHP_INT_MAX`, optional

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `explode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/explode.md).
