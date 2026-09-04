---
title: "array_count_values()"
description: "Counts the occurrences of each distinct value in an array."
sidebar:
  order: 6
---

## array_count_values()

```php
function array_count_values(array $array): array
```

Counts the occurrences of each distinct value in an array.

**Parameters**:
- `$array` (`array`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_count_values.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_count_values.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_count_values` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_count_values.md).
