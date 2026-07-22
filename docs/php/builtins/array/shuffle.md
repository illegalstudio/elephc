---
title: "shuffle()"
description: "Shuffles an array into random order."
sidebar:
  order: 59
---

## shuffle()

```php
function shuffle(array $array): bool
```

Shuffles an array into random order.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/shuffle.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/shuffle.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `shuffle` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/shuffle.md).
