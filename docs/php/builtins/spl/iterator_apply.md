---
title: "iterator_apply()"
description: "Lowers `iterator_apply()` over supported Traversable sources and callback forms."
sidebar:
  order: 301
---

## iterator_apply()

```php
function iterator_apply(traversable $iterator, callable $callback, array $args): int
```

Lowers `iterator_apply()` over supported Traversable sources and callback forms.

**Parameters**:
- `$iterator` (`traversable`)
- `$callback` (`callable`)
- `$args` (`array`), optional

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `iterator_apply` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/iterator_apply.md).

