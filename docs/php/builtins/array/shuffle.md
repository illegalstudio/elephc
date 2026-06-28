---
title: "shuffle()"
description: "Lowers `shuffle()` for indexed arrays with 8-byte slots by mutating the source array in place."
sidebar:
  order: 42
---

## shuffle()

```php
function shuffle(array $array): bool
```

Lowers `shuffle()` for indexed arrays with 8-byte slots by mutating the source array in place.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `shuffle` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/shuffle.md).

