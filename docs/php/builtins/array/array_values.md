---
title: "array_values()"
description: "Returns all the values of an array, re-indexed numerically."
sidebar:
  order: 44
---

## array_values()

```php
function array_values(array $array): array
```

Returns all the values of an array, re-indexed numerically.

**Parameters**:
- `$array` (`array`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_values.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_values.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_values` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_values.md).

