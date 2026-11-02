---
title: "array_find()"
description: "Returns the first element satisfying a predicate callback, or null."
sidebar:
  order: 16
---

## array_find()

```php
function array_find(mixed $array, mixed $callback): mixed
```

Returns the first element satisfying a predicate callback, or null.

**Parameters**:
- `$array` (`mixed`)
- `$callback` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_find.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_find.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_find` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_find.md).
