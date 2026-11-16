---
title: "sprintf()"
description: "Returns a formatted string."
---

## sprintf()

```php
function sprintf(string $format, ...$values): string
```

Returns a formatted string.

**Parameters**:
- `$format` (`string`)
- `...$values` - variadic: collects excess arguments into `$values`.

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sprintf` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/sprintf.md).
