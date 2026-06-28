---
title: "boolval()"
description: "Lowers `boolval()` using the same concrete scalar PHP truthiness rules as `IsTruthy`."
sidebar:
  order: 388
---

## boolval()

```php
function boolval(mixed $value): bool
```

Lowers `boolval()` using the same concrete scalar PHP truthiness rules as `IsTruthy`.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `boolval` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/boolval.md).

