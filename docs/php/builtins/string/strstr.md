---
title: "strstr()"
description: "Returns the portion of a string starting at the first occurrence of a substring, or false."
---

## strstr()

```php
function strstr(string $haystack, string $needle, bool $before_needle = false): mixed
```

Returns the portion of a string starting at the first occurrence of a substring, or false.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$before_needle` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `strstr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strstr.md).
