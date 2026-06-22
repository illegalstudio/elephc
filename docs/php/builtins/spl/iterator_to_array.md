---
title: "iterator_to_array()"
description: "Lowers `iterator_to_array()` over arrays, `iterable`, and Traversable objects."
sidebar:
  order: 303
---

## iterator_to_array()

```php
function iterator_to_array(traversable $iterator, bool $preserve_keys): array
```

Lowers `iterator_to_array()` over arrays, `iterable`, and Traversable objects.

**Parameters**:
- `$iterator` (`traversable`)
- `$preserve_keys` (`bool`), optional

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `iterator_to_array` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/iterator_to_array.md).

