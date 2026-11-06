---
title: "array_search()"
description: "Searches the array for a given value and returns the first corresponding key if successful."
---

## array_search()

```php
function array_search(mixed $needle, array $haystack, bool $strict = false): mixed
```

Searches the array for a given value and returns the first corresponding key if successful.

**Parameters**:
- `$needle` (`mixed`)
- `$haystack` (`array`)
- `$strict` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_search` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_search.md).
