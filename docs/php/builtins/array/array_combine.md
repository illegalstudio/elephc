---
title: "array_combine()"
description: "Creates an array by using one array for keys and another for values."
sidebar:
  order: 5
---

## array_combine()

```php
function array_combine(array $keys, array $values): array
```

Creates an array by using one array for keys and another for values.

**Parameters**:
- `$keys` (`array`)
- `$values` (`array`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_combine.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_combine.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_combine` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_combine.md).
