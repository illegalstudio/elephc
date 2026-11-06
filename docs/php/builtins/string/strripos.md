---
title: "strripos()"
description: "Finds the numeric position of the last case-insensitive occurrence of a substring."
---

## strripos()

```php
function strripos(string $haystack, string $needle, int $offset = 0): mixed
```

Finds the numeric position of the last case-insensitive occurrence of a substring.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$offset` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `strripos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strripos.md).
