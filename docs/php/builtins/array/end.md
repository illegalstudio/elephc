---
title: "end()"
description: "Moves the array's internal pointer to the last element and returns it."
sidebar:
  order: 54
---

## end()

```php
function end(array $array): mixed
```

Moves the array's internal pointer to the last element and returns it.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/end.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/end.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `end` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/end.md).
