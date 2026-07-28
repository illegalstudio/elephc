---
title: "array_unshift()"
description: "Prepends one or more elements to the beginning of an array."
sidebar:
  order: 43
---

## array_unshift()

```php
function array_unshift(array &$array, ...$values): int
```

Prepends one or more elements to the beginning of an array.

**Parameters**:
- `$array` (`array`), passed by reference
- `...$values` — variadic: collects excess arguments into `$values`.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_unshift.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_unshift.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_unshift` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_unshift.md).
