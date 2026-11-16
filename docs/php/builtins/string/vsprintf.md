---
title: "vsprintf()"
description: "Returns a formatted string using an array of values."
---

## vsprintf()

```php
function vsprintf(string $format, array $values): string
```

Returns a formatted string using an array of values.

**Parameters**:
- `$format` (`string`)
- `$values` (`array`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `vsprintf` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/vsprintf.md).
