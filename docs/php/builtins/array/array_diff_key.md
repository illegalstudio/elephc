---
title: "array_diff_key()"
description: "Computes the difference of arrays using keys for comparison."
sidebar:
  order: 8
---

## array_diff_key()

```php
function array_diff_key(array $array, ...$arrays): array
```

Computes the difference of arrays using keys for comparison.

**Parameters**:
- `$array` (`array`)
- `...$arrays` — variadic: collects excess arguments into `$arrays`.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_diff_key.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_diff_key.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_diff_key` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_diff_key.md).
