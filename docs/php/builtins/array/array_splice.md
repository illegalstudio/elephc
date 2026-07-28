---
title: "array_splice()"
description: "Removes a portion of the array and replaces it with something else."
sidebar:
  order: 38
---

## array_splice()

```php
function array_splice(array &$array, int $offset, int $length = null): array
```

Removes a portion of the array and replaces it with something else.

**Parameters**:
- `$array` (`array`), passed by reference
- `$offset` (`int`)
- `$length` (`int`), default `null`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_splice.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_splice.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_splice` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_splice.md).
