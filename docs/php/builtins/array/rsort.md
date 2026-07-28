---
title: "rsort()"
description: "Sorts an array in descending order."
sidebar:
  order: 58
---

## rsort()

```php
function rsort(array &$array): bool
```

Sorts an array in descending order.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/rsort.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/rsort.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `rsort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/rsort.md).
