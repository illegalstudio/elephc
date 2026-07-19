---
title: "sort()"
description: "Sorts an array in ascending order."
sidebar:
  order: 60
---

## sort()

```php
function sort(array $array): bool
```

Sorts an array in ascending order.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/sort.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/sort.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `sort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/sort.md).

