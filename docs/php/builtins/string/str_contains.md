---
title: "str_contains()"
description: "Determines if a string contains a given substring."
---

## str_contains()

```php
function str_contains(string $haystack, string $needle): bool
```

Determines if a string contains a given substring.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `str_contains` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_contains.md).
