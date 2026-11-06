---
title: "sodium_memzero()"
description: "Overwrites a string with zero bytes."
---

## sodium_memzero()

```php
function sodium_memzero(string &$string): void
```

Overwrites a string with zero bytes.

**Parameters**:
- `$string` (`string`), passed by reference

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_memzero` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_memzero.md).
