---
title: "uasort()"
description: "Sorts an array with a user-defined comparison function and maintains index association."
sidebar:
  order: 61
---

## uasort()

```php
function uasort(array $array, callable $callback): bool
```

Sorts an array with a user-defined comparison function and maintains index association.

**Parameters**:
- `$array` (`array`), passed by reference
- `$callback` (`callable`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/uasort.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/uasort.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `uasort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/uasort.md).

