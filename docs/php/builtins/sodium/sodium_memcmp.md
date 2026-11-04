---
title: "sodium_memcmp()"
description: "Compares two strings in constant time."
---

## sodium_memcmp()

```php
function sodium_memcmp(string $string1, string $string2): int
```

Compares two strings in constant time.

**Parameters**:
- `$string1` (`string`)
- `$string2` (`string`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_memcmp` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_memcmp.md).
