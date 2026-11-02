---
title: "array_find_key()"
description: "Returns the key of the first element satisfying a predicate callback, or null."
sidebar:
  order: 17
---

## array_find_key()

```php
function array_find_key(array $array, mixed $callback): mixed
```

Returns the key of the first element satisfying a predicate callback, or null.

**Parameters**:
- `$array` (`array`)
- `$callback` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected array_edge prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_find.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_find.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_find_key` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_find_key.md).
