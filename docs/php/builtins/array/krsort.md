---
title: "krsort()"
description: "Sorts an array by key in descending order, comparing keys under $flags."
sidebar:
  order: 57
---

## krsort()

```php
function krsort(array $array, int $flags = 0): bool
```

Sorts an array by key in descending order, comparing keys under $flags.

**Parameters**:
- `$array` (`array`), passed by reference
- `$flags` (`int`), default `0`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/krsort.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/krsort.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `krsort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/krsort.md).
