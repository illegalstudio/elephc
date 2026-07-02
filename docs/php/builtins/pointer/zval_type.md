---
title: "zval_type()"
description: "Lowers `zval_type(zval_ptr)` by invoking `__rt_zval_type`, which returns the"
sidebar:
  order: 302
---

## zval_type()

```php
function zval_type(mixed $zval): int
```

Lowers `zval_type(zval_ptr)` by invoking `__rt_zval_type`, which returns the

**Parameters**:
- `$zval` (`mixed`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `zval_type` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/zval_type.md).

