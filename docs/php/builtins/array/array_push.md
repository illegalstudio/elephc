---
title: "array_push()"
description: "Lowers `array_push()` by appending one value and publishing the mutated array."
sidebar:
  order: 19
---

## array_push()

```php
function array_push(array $array, ...$values): void
```

Lowers `array_push()` by appending one value and publishing the mutated array.

**Parameters**:
- `$array` (`array`), passed by reference
- `...$values` — variadic: collects excess arguments into `$values`.

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_push` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_push.md).

