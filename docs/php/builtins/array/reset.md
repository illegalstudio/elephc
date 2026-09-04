---
title: "reset()"
description: "Rewinds the array's internal pointer to the first element and returns it."
sidebar:
  order: 64
---

## reset()

```php
function reset(array $array): mixed
```

Rewinds the array's internal pointer to the first element and returns it.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/reset.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/reset.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `reset` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/reset.md).
