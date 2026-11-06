---
title: "strrpos()"
description: "Finds the numeric position of the last occurrence of a substring."
---

## strrpos()

```php
function strrpos(string $haystack, string $needle, int $offset = 0): mixed
```

Finds the numeric position of the last occurrence of a substring.

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

For how `strrpos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strrpos.md).
