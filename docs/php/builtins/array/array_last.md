---
title: "array_last()"
description: "Returns the last value of an array in insertion order, or null when it is empty."
sidebar:
  order: 30
---

## array_last()

```php
function array_last(array $array): mixed
```

Returns the last value of an array in insertion order, or null when it is empty.

**Parameters**:
- `$array` (`array`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_first.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_first.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_last` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_last.md).
