---
title: "array_splice()"
description: "Lowers `array_splice()` by mutating an indexed source array and returning removed elements."
sidebar:
  order: 26
---

## array_splice()

```php
function array_splice(array $array, int $offset, int $length, array $replacement): array
```

Lowers `array_splice()` by mutating an indexed source array and returning removed elements.

**Parameters**:
- `$array` (`array`), passed by reference
- `$offset` (`int`)
- `$length` (`int`), optional
- `$replacement` (`array`)

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_splice` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_splice.md).

