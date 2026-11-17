---
title: "rsort()"
description: "Sorts an array in descending order. In compiled (AOT) code, arrays with runtime-typed (`mixed`) elements, including declared PHP array parameters, are accepted when every element is `null`, `bool`, `int`, `float`, or `string`. Keys are reindexed and value copies keep their original order. A non-scalar element (nested array, object, resource, or boxed callable) terminates execution before sorting with `Fatal error: sorting Mixed arrays containing non-scalar values is not supported`. This deliberate restriction does not implement full PHP container ordering."
---

## rsort()

```php
function rsort(array $array): bool
```

Sorts an array in descending order. In compiled (AOT) code, arrays with runtime-typed (`mixed`) elements, including declared PHP array parameters, are accepted when every element is `null`, `bool`, `int`, `float`, or `string`. Keys are reindexed and value copies keep their original order. A non-scalar element (nested array, object, resource, or boxed callable) terminates execution before sorting with `Fatal error: sorting Mixed arrays containing non-scalar values is not supported`. This deliberate restriction does not implement full PHP container ordering.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `rsort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/rsort.md).
