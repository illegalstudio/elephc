---
title: "sodium_add()"
description: "Adds two strings as little-endian integers."
---

## sodium_add()

```php
function sodium_add(string $string1, string $string2): void
```

Adds two strings as little-endian integers.

**Parameters**:
- `$string1` (`string`), passed by reference
- `$string2` (`string`)

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_add` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_add.md).
