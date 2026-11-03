---
title: "fseek()"
description: "Seeks on a file pointer."
---

## fseek()

```php
function fseek(resource $stream, int $offset, int $whence = 0): int
```

Seeks on a file pointer.

**Parameters**:
- `$stream` (`resource`)
- `$offset` (`int`)
- `$whence` (`int`), default `0`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `fseek` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fseek.md).
