---
title: "array_fill_keys()"
description: "Lowers `array_fill_keys()` through the legacy hash-building runtime helpers."
sidebar:
  order: 7
---

## array_fill_keys()

```php
function array_fill_keys(array $keys, mixed $value): array
```

Lowers `array_fill_keys()` through the legacy hash-building runtime helpers.

**Parameters**:
- `$keys` (`array`)
- `$value` (`mixed`)

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_fill_keys` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_fill_keys.md).

