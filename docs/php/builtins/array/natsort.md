---
title: "natsort()"
description: "Sorts an array using a natural order algorithm."
sidebar:
  order: 56
---

## natsort()

```php
function natsort(array &$array): bool
```

Sorts an array using a natural order algorithm.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/natsort.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/natsort.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `natsort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/natsort.md).
