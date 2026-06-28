---
title: "array_shift()"
description: "Lowers `array_shift()` for indexed arrays by compacting slots and boxing `T|null` as Mixed."
sidebar:
  order: 24
---

## array_shift()

```php
function array_shift(array $array): mixed
```

Lowers `array_shift()` for indexed arrays by compacting slots and boxing `T|null` as Mixed.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_shift` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_shift.md).

