---
title: "current()"
description: "Returns the element under the array's internal pointer."
sidebar:
  order: 53
---

## current()

```php
function current(array $array): mixed
```

Returns the element under the array's internal pointer.

**Parameters**:
- `$array` (`array`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/current.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/current.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `current` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/current.md).
