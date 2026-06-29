---
title: "is_object()"
description: "Lowers `is_object()`: true for statically-known objects, or a boxed Mixed/Union value whose"
sidebar:
  order: 408
---

## is_object()

```php
function is_object(mixed $value): bool
```

Lowers `is_object()`: true for statically-known objects, or a boxed Mixed/Union value whose

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_object` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_object.md).

