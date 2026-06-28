---
title: "array_unique()"
description: "Lowers `array_unique()` for indexed arrays with 8-byte payload slots."
sidebar:
  order: 28
---

## array_unique()

```php
function array_unique(array $array, int $flags): array
```

Lowers `array_unique()` for indexed arrays with 8-byte payload slots.

**Parameters**:
- `$array` (`array`)
- `$flags` (`int`)

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_unique` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_unique.md).

