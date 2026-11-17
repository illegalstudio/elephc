---
title: "zval_free()"
description: "Frees a PHP zval pointer allocated by `zval_pack`."
---

## zval_free()

```php
function zval_free(pointer $zval): void
```

Frees a PHP zval pointer allocated by `zval_pack`.

**Parameters**:
- `$zval` (`pointer`)

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `zval_free` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/zval_free.md).
