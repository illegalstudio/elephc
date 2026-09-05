---
title: "array_fill_keys()"
description: "Fill an array with values, specifying keys."
sidebar:
  order: 11
---

## array_fill_keys()

```php
function array_fill_keys(array $keys, mixed $value): array
```

Fill an array with values, specifying keys.

**Parameters**:
- `$keys` (`array`)
- `$value` (`mixed`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_fill_keys.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_fill_keys.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_fill_keys` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_fill_keys.md).
