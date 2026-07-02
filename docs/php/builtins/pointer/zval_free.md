---
title: "zval_free()"
description: "Lowers `zval_free(zval_ptr)` by invoking `__rt_zval_free` to release the zval"
sidebar:
  order: 300
---

## zval_free()

```php
function zval_free(mixed $zval): void
```

Lowers `zval_free(zval_ptr)` by invoking `__rt_zval_free` to release the zval

**Parameters**:
- `$zval` (`mixed`)

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `zval_free` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/zval_free.md).

