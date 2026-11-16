---
title: "buffer_free()"
description: "Frees a buffer<T> and nulls the local variable that held it."
---

## buffer_free()

```php
function buffer_free(buffer $buffer): void
```

Frees a buffer<T> and nulls the local variable that held it.

**Parameters**:
- `$buffer` (`buffer`)

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `buffer_free` is implemented in the compiler, see [the internals page](../../../internals/builtins/buffer/buffer_free.md).
