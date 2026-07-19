---
title: "array_sum()"
description: "Calculate the sum of values in an array."
sidebar:
  order: 39
---

## array_sum()

```php
function array_sum(array $array): int
```

Calculate the sum of values in an array.

**Parameters**:
- `$array` (`array`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_sum.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_sum.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_sum` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_sum.md).

