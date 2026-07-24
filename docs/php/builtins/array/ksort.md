---
title: "ksort()"
description: "Sorts an array by key in ascending order."
sidebar:
  order: 54
---

## ksort()

```php
function ksort(array &$array): bool
```

Sorts an array by key in ascending order.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/ksort.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/ksort.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ksort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/ksort.md).
