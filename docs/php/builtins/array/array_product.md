---
title: "array_product()"
description: "Calculate an integer or float product of array values; an empty array returns integer 1."
sidebar:
  order: 29
---

## array_product()

```php
function array_product(array $array): int|float
```

Calculate an integer or float product of array values; an empty array returns integer 1.

**Parameters**:
- `$array` (`array`)

**Returns**: `int|float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_product.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_product.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_product` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_product.md).
