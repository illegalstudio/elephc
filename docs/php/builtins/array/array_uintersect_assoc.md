---
title: "array_uintersect_assoc()"
description: "Computes the intersection of arrays with an additional index check, comparing data by a user callback."
sidebar:
  order: 53
---

## array_uintersect_assoc()

```php
function array_uintersect_assoc(array $array, ...$rest): array
```

Computes the intersection of arrays with an additional index check, comparing data by a user callback.

**Parameters**:
- `$array` (`array`)
- `...$rest` - variadic: collects excess arguments into `$rest`.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected array_ucompare prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_ucompare.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_ucompare.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_uintersect_assoc` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_uintersect_assoc.md).
