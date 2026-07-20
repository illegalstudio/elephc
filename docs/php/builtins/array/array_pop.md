---
title: "array_pop()"
description: "Pops the element off the end of array."
sidebar:
  order: 27
---

## array_pop()

```php
function array_pop(array $array): mixed
```

Pops the element off the end of array.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_pop.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_pop.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_pop` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_pop.md).

