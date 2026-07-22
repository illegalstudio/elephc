---
title: "array_unique()"
description: "Removes duplicate values from an array."
sidebar:
  order: 42
---

## array_unique()

```php
function array_unique(array $array): array
```

Removes duplicate values from an array.

**Parameters**:
- `$array` (`array`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_unique.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_unique.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_unique` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_unique.md).
