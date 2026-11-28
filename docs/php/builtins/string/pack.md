---
title: "pack()"
description: "Packs data into a binary string."
---

## pack()

```php
function pack(string $format, ...$values): string
```

Packs data into a binary string.

**Parameters**:
- `$format` (`string`)
- `...$values` - variadic: collects excess arguments into `$values`.

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pack` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/pack.md).
