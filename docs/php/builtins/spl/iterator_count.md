---
title: "iterator_count()"
description: "Count the elements in an iterator."
sidebar:
  order: 341
---

## iterator_count()

```php
function iterator_count(traversable $iterator): int
```

Count the elements in an iterator.

**Parameters**:
- `$iterator` (`traversable`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/iterator_count.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/iterator_count.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `iterator_count` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/iterator_count.md).
