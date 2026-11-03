---
title: "count()"
description: "Counts all elements in an array or Countable object."
---

## count()

```php
function count(array $value, int $mode = 0): int
```

Counts all elements in an array or Countable object.

**Parameters**:
- `$value` (`array`)
- `$mode` (`int`), default `0`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `count` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/count.md).
