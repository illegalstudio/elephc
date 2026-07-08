---
title: "iterator_apply()"
description: "Call a function for every element in an iterator."
sidebar:
  order: 320
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

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `iterator_apply` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/iterator_apply.md).

