---
title: "zval_type()"
description: "Returns the PHP zval type byte for a zval pointer."
---

## zval_type()

```php
function zval_type(pointer $zval): int
```

Returns the PHP zval type byte for a zval pointer.

**Parameters**:
- `$zval` (`pointer`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `zval_type` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/zval_type.md).
