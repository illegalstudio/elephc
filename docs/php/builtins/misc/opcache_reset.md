---
title: "opcache_reset()"
description: "Clears the whole opcode cache."
---

## opcache_reset()

```php
function opcache_reset(): bool
```

Clears the whole opcode cache.

**Parameters**: none.

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `opcache_reset` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/opcache_reset.md).
