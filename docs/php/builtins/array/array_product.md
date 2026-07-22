---
title: "array_product()"
description: "Calculate the product of values in an array."
sidebar:
  order: 28
---

## array_product()

```php
function array_product(array $array): int
```

Calculate the product of values in an array.

**Parameters**:
- `$array` (`array`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_product.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_product.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_product` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_product.md).
