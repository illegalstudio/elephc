---
title: "array_flip()"
description: "Exchanges all keys with their associated values in an array."
sidebar:
  order: 13
---

## array_flip()

```php
function array_flip(array $array): array
```

Exchanges all keys with their associated values in an array.

**Parameters**:
- `$array` (`array`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_flip.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_flip.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_flip` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_flip.md).
