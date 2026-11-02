---
title: "array_diff_ukey()"
description: "Computes the difference of arrays using a user callback to compare keys."
sidebar:
  order: 12
---

## array_diff_ukey()

```php
function array_diff_ukey(array $array, ...$rest): array
```

Computes the difference of arrays using a user callback to compare keys.

**Parameters**:
- `$array` (`array`)
- `...$rest` - variadic: collects excess arguments into `$rest`.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected array_ucompare prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_ucompare.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_ucompare.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_diff_ukey` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_diff_ukey.md).
