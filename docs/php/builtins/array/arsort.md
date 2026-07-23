---
title: "arsort()"
description: "Sorts an array in descending order and maintains index association."
sidebar:
  order: 47
---

## arsort()

```php
function arsort(array &$array): bool
```

Sorts an array in descending order and maintains index association.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/arsort.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/arsort.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `arsort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/arsort.md).
