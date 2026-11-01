---
title: "array_key_last()"
description: "Gets the last key of an array."
sidebar:
  order: 28
---

## array_key_last()

```php
function array_key_last(array $array): mixed
```

Gets the last key of an array.

**Parameters**:
- `$array` (`array`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_key_last.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_key_last.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_key_last` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_key_last.md).
