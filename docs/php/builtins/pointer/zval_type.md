---
title: "zval_type()"
description: "Returns the PHP zval type byte for a zval pointer."
sidebar:
  order: 302
---

## zval_type()

```php
function zval_type(pointer $zval): int
```

Returns the PHP zval type byte for a zval pointer.

**Parameters**:
- `$zval` (`pointer`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `zval_type` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/zval_type.md).

