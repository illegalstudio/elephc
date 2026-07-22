---
title: "array_rand()"
description: "Pick one or more random keys out of an array."
sidebar:
  order: 30
---

## array_rand()

```php
function array_rand(array $array): int
```

Pick one or more random keys out of an array.

**Parameters**:
- `$array` (`array`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_rand.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_rand.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_rand` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_rand.md).
