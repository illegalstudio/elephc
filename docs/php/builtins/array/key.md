---
title: "key()"
description: "Returns the key of the element under the array's internal pointer."
sidebar:
  order: 56
---

## key()

```php
function key(array $array): mixed
```

Returns the key of the element under the array's internal pointer.

**Parameters**:
- `$array` (`array`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/key.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/key.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `key` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/key.md).
