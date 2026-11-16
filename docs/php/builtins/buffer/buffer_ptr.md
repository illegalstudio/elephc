---
title: "buffer_ptr()"
description: "Returns the payload address of a buffer<T>, valid until the buffer is freed."
---

## buffer_ptr()

```php
function buffer_ptr(buffer $buffer): pointer
```

Returns the payload address of a buffer<T>, valid until the buffer is freed.

**Parameters**:
- `$buffer` (`buffer`)

**Returns**: `pointer`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `buffer_ptr` is implemented in the compiler, see [the internals page](../../../internals/builtins/buffer/buffer_ptr.md).
