---
title: "iterator_apply()"
description: "Call a function for every element in an iterator."
sidebar:
  order: 338
---

## iterator_apply()

```php
function iterator_apply(traversable $iterator, callable $callback, array $args = null): int
```

Call a function for every element in an iterator.

**Parameters**:
- `$iterator` (`traversable`)
- `$callback` (`callable`)
- `$args` (`array`), default `null`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/iterator_apply.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/iterator_apply.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `iterator_apply` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/iterator_apply.md).

