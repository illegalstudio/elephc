---
title: "fflush()"
description: "Flushes the output to a file."
---

## fflush()

```php
function fflush(resource $stream): bool
```

Flushes the output to a file.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `fflush` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fflush.md).
