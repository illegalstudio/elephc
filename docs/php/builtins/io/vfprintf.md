---
title: "vfprintf()"
description: "Write a formatted string to a stream."
---

## vfprintf()

```php
function vfprintf(resource $stream, string $format, array $values): int
```

Write a formatted string to a stream.

**Parameters**:
- `$stream` (`resource`)
- `$format` (`string`)
- `$values` (`array`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `vfprintf` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/vfprintf.md).
