---
title: "array_fill()"
description: "Lowers `array_fill()` for pointer-sized scalar and refcounted payloads."
sidebar:
  order: 6
---

## array_fill()

```php
function array_fill(int $start_index, int $count, mixed $value): array
```

Lowers `array_fill()` for pointer-sized scalar and refcounted payloads.

**Parameters**:
- `$start_index` (`int`)
- `$count` (`int`)
- `$value` (`mixed`)

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_fill` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_fill.md).

