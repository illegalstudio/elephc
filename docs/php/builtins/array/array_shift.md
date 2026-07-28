---
title: "array_shift()"
description: "Shifts an element off the beginning of array."
sidebar:
  order: 36
---

## array_shift()

```php
function array_shift(array &$array): mixed
```

Shifts an element off the beginning of array.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_shift.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_shift.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_shift` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_shift.md).
