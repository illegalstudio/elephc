---
title: "gmp_init()"
description: "Creates a GMP number from an integer or string."
---

## gmp_init()

```php
function gmp_init(string|int $num, int $base = 0): object
```

Creates a GMP number from an integer or string.

**Parameters**:
- `$num` (`string|int`)
- `$base` (`int`), default `0`, optional

**Returns**: `object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_init` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_init.md).
