---
title: "zval_pack()"
description: "Packs an elephc runtime value into a heap-allocated PHP zval pointer."
---

## zval_pack()

```php
function zval_pack(mixed $value): pointer
```

Packs an elephc runtime value into a heap-allocated PHP zval pointer.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `pointer`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `zval_pack` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/zval_pack.md).
