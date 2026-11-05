---
title: "flock()"
description: "Portable advisory file locking."
---

## flock()

```php
function flock(resource $stream, int $operation, bool &$would_block = null): bool
```

Portable advisory file locking.

**Parameters**:
- `$stream` (`resource`)
- `$operation` (`int`)
- `$would_block` (`bool`), passed by reference, default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `flock` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/flock.md).
