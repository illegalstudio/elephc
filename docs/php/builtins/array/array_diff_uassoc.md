---
title: "array_diff_uassoc()"
description: "Computes the difference of arrays with an additional index check performed by a user callback."
sidebar:
  order: 11
---

## array_diff_uassoc()

```php
function array_diff_uassoc(array $array, ...$rest): array
```

Computes the difference of arrays with an additional index check performed by a user callback.

**Parameters**:
- `$array` (`array`)
- `...$rest` - variadic: collects excess arguments into `$rest`.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected array_ucompare prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_ucompare.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_ucompare.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_diff_uassoc` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_diff_uassoc.md).
