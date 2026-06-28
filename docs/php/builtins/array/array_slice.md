---
title: "array_slice()"
description: "Lowers `array_slice()` for indexed arrays with pointer-sized payload slots."
sidebar:
  order: 25
---

## array_slice()

```php
function array_slice(array $array, int $offset, int $length, bool $preserve_keys): array
```

Lowers `array_slice()` for indexed arrays with pointer-sized payload slots.

**Parameters**:
- `$array` (`array`)
- `$offset` (`int`)
- `$length` (`int`), optional
- `$preserve_keys` (`bool`)

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_slice` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_slice.md).

