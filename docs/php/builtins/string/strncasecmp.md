---
title: "strncasecmp()"
description: "Compares the first n bytes of two strings, ignoring ASCII case."
---

## strncasecmp()

```php
function strncasecmp(string $string1, string $string2, int $length): int
```

Compares the first n bytes of two strings, ignoring ASCII case.

**Parameters**:
- `$string1` (`string`)
- `$string2` (`string`)
- `$length` (`int`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `strncasecmp` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strncasecmp.md).
