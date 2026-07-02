---
title: "zval_unpack()"
description: "Lowers `zval_unpack(zval_ptr)` by invoking `__rt_zval_unpack`, which returns a"
sidebar:
  order: 303
---

## zval_unpack()

```php
function zval_unpack(mixed $zval): mixed
```

Lowers `zval_unpack(zval_ptr)` by invoking `__rt_zval_unpack`, which returns a

**Parameters**:
- `$zval` (`mixed`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `zval_unpack` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/zval_unpack.md).

