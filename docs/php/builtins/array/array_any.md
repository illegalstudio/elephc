---
title: "array_any()"
description: "Returns true when at least one array element satisfies the predicate callback."
sidebar:
  order: 2
---

## array_any()

```php
function array_any(mixed $array, mixed $callback): bool
```

Returns true when at least one array element satisfies the predicate callback.

**Parameters**:
- `$array` (`mixed`)
- `$callback` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_find.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_find.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_any` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_any.md).
