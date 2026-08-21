---
title: "array_intersect()"
description: "Computes the intersection of arrays."
sidebar:
  order: 15
---

## array_intersect()

```php
function array_intersect(array $array, ...$arrays): mixed
```

Computes the intersection of arrays.

**Parameters**:
- `$array` (`array`)
- `...$arrays` — variadic: collects excess arguments into `$arrays`.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_intersect.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_intersect.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_intersect` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_intersect.md).
