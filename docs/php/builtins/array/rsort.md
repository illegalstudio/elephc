---
title: "rsort()"
description: "Sorts an array in descending order. In compiled (AOT) code, indexed arrays with runtime-typed (`mixed`) elements are accepted when every element is `null`, `bool`, `int`, `float`, or `string`. A non-scalar element (nested array, object, resource, or boxed callable) terminates execution before sorting with `Fatal error: sorting Mixed arrays containing non-scalar values is not supported`. This deliberate restriction does not implement full PHP container ordering."
sidebar:
  order: 65
---

## rsort()

```php
function rsort(array $array): bool
```

Sorts an array in descending order. In compiled (AOT) code, indexed arrays with runtime-typed (`mixed`) elements are accepted when every element is `null`, `bool`, `int`, `float`, or `string`. A non-scalar element (nested array, object, resource, or boxed callable) terminates execution before sorting with `Fatal error: sorting Mixed arrays containing non-scalar values is not supported`. This deliberate restriction does not implement full PHP container ordering.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/rsort.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/rsort.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `rsort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/rsort.md).
