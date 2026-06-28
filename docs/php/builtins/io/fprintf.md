---
title: "fprintf()"
description: "Lowers `fprintf(stream, format, values...)` as `sprintf()` plus stream write."
sidebar:
  order: 150
---

## fprintf()

```php
function fprintf(resource $stream, string $format, ...$values): int
```

Lowers `fprintf(stream, format, values...)` as `sprintf()` plus stream write.

**Parameters**:
- `$stream` (`resource`)
- `$format` (`string`)
- `...$values` — variadic: collects excess arguments into `$values`.

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fprintf` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fprintf.md).

