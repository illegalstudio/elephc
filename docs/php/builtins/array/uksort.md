---
title: "uksort()"
description: "Sorts an array by keys using a user-defined comparison function."
sidebar:
  order: 62
---

## uksort()

```php
function uksort(array $array, callable $callback): bool
```

Sorts an array by keys using a user-defined comparison function.

**Parameters**:
- `$array` (`array`), passed by reference
- `$callback` (`callable`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/uksort.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/uksort.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `uksort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/uksort.md).

