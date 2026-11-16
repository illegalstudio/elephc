---
title: "ftruncate()"
description: "Truncates a file to a given length."
---

## ftruncate()

```php
function ftruncate(resource $stream, int $size): bool
```

Truncates a file to a given length.

**Parameters**:
- `$stream` (`resource`)
- `$size` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ftruncate` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ftruncate.md).
