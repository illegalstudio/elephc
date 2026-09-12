---
title: "next()"
description: "Advances the array's internal pointer and returns the new element."
sidebar:
  order: 61
---

## next()

```php
function next(array $array): mixed
```

Advances the array's internal pointer and returns the new element.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/next.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/next.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `next` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/next.md).
