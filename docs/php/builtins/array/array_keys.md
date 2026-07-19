---
title: "array_keys()"
description: "Returns all the keys of an array."
sidebar:
  order: 21
---

## array_keys()

```php
function array_keys(array $array): array
```

Returns all the keys of an array.

**Parameters**:
- `$array` (`array`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_keys.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_keys.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_keys` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_keys.md).

