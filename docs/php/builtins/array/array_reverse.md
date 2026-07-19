---
title: "array_reverse()"
description: "Returns an array with the elements in reverse order."
sidebar:
  order: 34
---

## array_reverse()

```php
function array_reverse(array $array): array
```

Returns an array with the elements in reverse order.

**Parameters**:
- `$array` (`array`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_reverse.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_reverse.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_reverse` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_reverse.md).

