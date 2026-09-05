---
title: "array_key_exists()"
description: "Checks if the given key or index exists in the array."
sidebar:
  order: 19
---

## array_key_exists()

```php
function array_key_exists(string $key, array $array): bool
```

Checks if the given key or index exists in the array.

**Parameters**:
- `$key` (`string`)
- `$array` (`array`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_key_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_key_exists.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_key_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_key_exists.md).
