---
title: "array_reverse()"
description: "Lowers `array_reverse()` for indexed arrays with 8-byte payload slots."
sidebar:
  order: 22
---

## array_reverse()

```php
function array_reverse(array $array, bool $preserve_keys): array
```

Lowers `array_reverse()` for indexed arrays with 8-byte payload slots.

**Parameters**:
- `$array` (`array`)
- `$preserve_keys` (`bool`)

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_reverse` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_reverse.md).

