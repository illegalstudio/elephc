---
title: "range()"
description: "Lowers `range()` for integer endpoints through the shared runtime constructor."
sidebar:
  order: 40
---

## range()

```php
function range(mixed $start, mixed $end, int $step): array
```

Lowers `range()` for integer endpoints through the shared runtime constructor.

**Parameters**:
- `$start` (`mixed`)
- `$end` (`mixed`)
- `$step` (`int`)

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `range` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/range.md).

