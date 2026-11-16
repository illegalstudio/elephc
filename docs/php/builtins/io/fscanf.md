---
title: "fscanf()"
description: "Parses input from a file according to a format."
---

## fscanf()

```php
function fscanf(resource $stream, string $format, ...$vars): array
```

Parses input from a file according to a format.

**Parameters**:
- `$stream` (`resource`)
- `$format` (`string`)
- `...$vars` - variadic: collects excess arguments into `$vars`.

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `fscanf` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fscanf.md).
