---
title: "sodium_increment()"
description: "Increments a little-endian integer string."
---

## sodium_increment()

```php
function sodium_increment(string &$string): void
```

Increments a little-endian integer string.

**Parameters**:
- `$string` (`string`), passed by reference

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_increment` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_increment.md).
