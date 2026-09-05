---
title: "prev()"
description: "Rewinds the array's internal pointer and returns the new element."
sidebar:
  order: 62
---

## prev()

```php
function prev(array $array): mixed
```

Rewinds the array's internal pointer and returns the new element.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/prev.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/prev.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `prev` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/prev.md).
