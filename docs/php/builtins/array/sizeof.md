---
title: "sizeof()"
description: "Alias of count()."
---

## sizeof()

```php
function sizeof(mixed $value, int $mode = 0): int
```

Alias of count().

**Parameters**:
- `$value` (`mixed`)
- `$mode` (`int`), default `0`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sizeof` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/sizeof.md).
