---
title: "usort()"
description: "Sorts an array by values using a user-defined comparison function."
sidebar:
  order: 63
---

## usort()

```php
function usort(array $array, callable $callback): bool
```

Sorts an array by values using a user-defined comparison function.

**Parameters**:
- `$array` (`array`), passed by reference
- `$callback` (`callable`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/usort.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/usort.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `usort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/usort.md).

