---
title: "strncmp()"
description: "Compares the first n bytes of two strings."
---

## strncmp()

```php
function strncmp(string $string1, string $string2, int $length): int
```

Compares the first n bytes of two strings.

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

For how `strncmp` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strncmp.md).
