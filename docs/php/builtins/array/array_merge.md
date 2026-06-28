---
title: "array_merge()"
description: "Lowers `array_merge()` for two compatible indexed arrays with 8-byte payload slots."
sidebar:
  order: 15
---

## array_merge()

```php
function array_merge(...$arrays): array
```

Lowers `array_merge()` for two compatible indexed arrays with 8-byte payload slots.

**Parameters**:
- `...$arrays` — variadic: collects excess arguments into `$arrays`.

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_merge` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_merge.md).

