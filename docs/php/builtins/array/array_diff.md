---
title: "array_diff()"
description: "Computes the difference of arrays."
sidebar:
  order: 6
---

## array_diff()

```php
function array_diff(array $array, ...$arrays): array
```

Computes the difference of arrays.

**Parameters**:
- `$array` (`array`)
- `...$arrays` — variadic: collects excess arguments into `$arrays`.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_diff.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_diff.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_diff` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_diff.md).

