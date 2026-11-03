---
title: "zval_unpack()"
description: "Unpacks a PHP zval pointer into an owned elephc Mixed value."
---

## zval_unpack()

```php
function zval_unpack(pointer $zval): mixed
```

Unpacks a PHP zval pointer into an owned elephc Mixed value.

**Parameters**:
- `$zval` (`pointer`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `zval_unpack` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/zval_unpack.md).
