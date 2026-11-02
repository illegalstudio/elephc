---
title: "array_intersect_assoc()"
description: "Computes the intersection of arrays with additional index check."
sidebar:
  order: 21
---

## array_intersect_assoc()

```php
function array_intersect_assoc(array $array, ...$arrays): mixed
```

Computes the intersection of arrays with additional index check.

**Parameters**:
- `$array` (`array`)
- `...$arrays` - variadic: collects excess arguments into `$arrays`.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_ucompare.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_ucompare.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_intersect_assoc` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_intersect_assoc.md).
