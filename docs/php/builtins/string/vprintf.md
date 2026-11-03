---
title: "vprintf()"
description: "Outputs a formatted string using an array of values."
---

## vprintf()

```php
function vprintf(string $format, array $values): int
```

Outputs a formatted string using an array of values.

**Parameters**:
- `$format` (`string`)
- `$values` (`array`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `vprintf` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/vprintf.md).
