---
title: "fprintf()"
description: "Write a formatted string to a stream."
---

## fprintf()

```php
function fprintf(resource $stream, string $format, ...$values): int
```

Write a formatted string to a stream.

**Parameters**:
- `$stream` (`resource`)
- `$format` (`string`)
- `...$values` - variadic: collects excess arguments into `$values`.

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `fprintf` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fprintf.md).
