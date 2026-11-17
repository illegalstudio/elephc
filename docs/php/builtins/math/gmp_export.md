---
title: "gmp_export()"
description: "Exports a GMP number as a binary string."
---

## gmp_export()

```php
function gmp_export(object|string|int $num, int $word_size = 1, int $flags = 17): string
```

Exports a GMP number as a binary string.

**Parameters**:
- `$num` (`object|string|int`)
- `$word_size` (`int`), default `1`, optional
- `$flags` (`int`), default `17`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_export` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_export.md).
