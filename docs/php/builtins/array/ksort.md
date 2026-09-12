---
title: "ksort()"
description: "Sorts an array by key in ascending SORT_REGULAR order; PHP sort flags are not yet supported."
sidebar:
  order: 58
---

## ksort()

```php
function ksort(array $array): bool
```

Sorts an array by key in ascending SORT_REGULAR order; PHP sort flags are not yet supported.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/ksort.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/ksort.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ksort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/ksort.md).
