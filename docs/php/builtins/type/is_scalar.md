---
title: "is_scalar()"
description: "Lowers `is_scalar()`: true for int/float/string/bool, a non-null tagged scalar, or a boxed"
sidebar:
  order: 410
---

## is_scalar()

```php
function is_scalar(mixed $value): bool
```

Lowers `is_scalar()`: true for int/float/string/bool, a non-null tagged scalar, or a boxed

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_scalar` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_scalar.md).

