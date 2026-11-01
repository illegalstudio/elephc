---
title: "array_is_list()"
description: "Checks whether an array is a list (sequential 0-based integer keys)."
sidebar:
  order: 25
---

## array_is_list()

```php
function array_is_list(mixed $array): bool
```

Checks whether an array is a list (sequential 0-based integer keys).

**Parameters**:
- `$array` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_is_list.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_is_list.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_is_list` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_is_list.md).
