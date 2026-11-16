---
title: "rand()"
description: "Generate a random integer."
---

## rand()

```php
function rand(int $min, int $max): int
```

Generate a random integer.

**Parameters**:
- `$min` (`int`)
- `$max` (`int`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `rand` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/rand.md).
