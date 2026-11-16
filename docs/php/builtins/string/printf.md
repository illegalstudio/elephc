---
title: "printf()"
description: "Outputs a formatted string."
---

## printf()

```php
function printf(string $format, ...$values): int
```

Outputs a formatted string.

**Parameters**:
- `$format` (`string`)
- `...$values` - variadic: collects excess arguments into `$values`.

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `printf` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/printf.md).
