---
title: "array_combine()"
description: "Lowers `array_combine()` through the legacy hash-building runtime helpers."
sidebar:
  order: 3
---

## array_combine()

```php
function array_combine(array $keys, array $values): array
```

Lowers `array_combine()` through the legacy hash-building runtime helpers.

**Parameters**:
- `$keys` (`array`)
- `$values` (`array`)

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_combine` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_combine.md).

