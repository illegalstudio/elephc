---
title: "gmp_import()"
description: "Imports a binary string as a GMP number."
---

## gmp_import()

```php
function gmp_import(string $data, int $word_size = 1, int $flags = 17): object
```

Imports a binary string as a GMP number.

**Parameters**:
- `$data` (`string`)
- `$word_size` (`int`), default `1`, optional
- `$flags` (`int`), default `17`, optional

**Returns**: `object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_import` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_import.md).
